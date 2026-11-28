import assert from "node:assert/strict";
import http from "node:http";
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import { WebSocket, WebSocketServer } from "ws";
import { createPreviewGateway } from "./gateway.mjs";
import { connectPreviewAgent } from "./agent.mjs";

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return server.address().port;
}

const closeHttp = server => new Promise(resolve => server.close(resolve));
async function previewRequest(base, host, path = "/", method = "GET", body = "") {
  const endpoint = new URL(base);
  return new Promise((resolve, reject) => {
    const request = http.request({
      hostname: endpoint.hostname, port: Number(endpoint.port), path,
      method, headers: { Host: host, ...(body ? { "Content-Type": "text/plain" } : {}) },
    }, response => {
      const chunks = [];
      response.on("data", chunk => chunks.push(chunk));
      response.on("end", () => {
        const text = Buffer.concat(chunks).toString();
        resolve({
          status: response.statusCode,
          headers: { get: name => response.headers[name.toLowerCase()] },
          json: async () => JSON.parse(text),
        });
      });
    });
    request.on("error", reject);
    request.end(body);
  });
}

async function fixture(run) {
  const local = http.createServer(async (req, res) => {
    let payload = "";
    for await (const part of req) payload += part;
    res.writeHead(200, { "content-type": "application/json", "x-test": "proxied" });
    res.end(JSON.stringify({ url: req.url, method: req.method, body: payload, host: req.headers.host }));
  });
  const upstreamWss = new WebSocketServer({ server: local });
  upstreamWss.on("connection", ws => ws.on("message", (data, binary) => ws.send(data, { binary })));
  const port = await listen(local);
  const gateway = createPreviewGateway({
    adminToken: "q".repeat(32), previewDomain: "example.test",
    publicScheme: "http", sessionTtlMs: 20_000,
  });
  const gatewayPort = await listen(gateway.server);
  const base = "http://127.0.0.1:" + gatewayPort;
  let agent;
  try {
    await run({ base, port, gateway, gatewayPort, setAgent(value) { agent = value; } });
  } finally {
    agent?.close();
    await gateway.close();
    await new Promise(resolve => upstreamWss.close(resolve));
    await closeHttp(local);
  }
}

async function createSession(base, port, idempotencyKey = "preview-test-key-0001") {
  const response = await fetch(base + "/admin/sessions", {
    method: "POST",
    headers: {
      Authorization: "Bearer " + "q".repeat(32),
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ port, idempotency_key: idempotencyKey }),
  });
  assert.ok([200, 201].includes(response.status), "session creation failed");
  return response.json();
}

test("admin auth, idempotent session creation, status, HTTP relay and close", async () => {
  await fixture(async ({ base, port, gatewayPort, setAgent }) => {
    assert.equal((await fetch(base + "/admin/sessions", {
      method: "POST", headers: { Authorization: "Bearer wrong" },
    })).status, 401);
    assert.equal((await fetch(base + "/admin/sessions", {
      method: "POST",
      headers: { Authorization: "Bearer " + "q".repeat(32), "Content-Type": "application/json" },
      body: JSON.stringify({ port: 99999 }),
    })).status, 400);

    const session = await createSession(base, port);
    assert.match(session.id, /^[0-9a-f]{32}$/);
    assert.match(session.agent_token, /^[\w-]{40,}$/);
    assert.equal(session.preview_url, "http://wcprev-" + session.id + ".example.test/");
    const replay = await createSession(base, port);
    assert.equal(replay.id, session.id);
    assert.equal(replay.agent_token, session.agent_token);
    assert.equal(replay.reused, true);

    const conflict = await fetch(base + "/admin/sessions", {
      method: "POST",
      headers: { Authorization: "Bearer " + "q".repeat(32), "Content-Type": "application/json" },
      body: JSON.stringify({ port: port + 1, idempotency_key: "preview-test-key-0001" }),
    });
    assert.equal(conflict.status, 409);

    const statusUrl = base + "/admin/sessions/" + session.id;
    assert.equal((await fetch(statusUrl)).status, 401);
    const disconnected = await fetch(statusUrl, { headers: { Authorization: "Bearer " + "q".repeat(32) } });
    assert.equal((await disconnected.json()).connected, false);
    assert.equal((await previewRequest(base, "random.example.test")).status, 404);
    assert.equal((await previewRequest(base, "wcprev-" + session.id + ".example.test", "/test")).status, 503);

    const agent = await connectPreviewAgent({
      gatewayUrl: "ws://127.0.0.1:" + gatewayPort + session.agent_path,
      agentToken: session.agent_token, port,
    });
    setAgent(agent);
    const connected = await fetch(statusUrl, { headers: { Authorization: "Bearer " + "q".repeat(32) } });
    assert.equal((await connected.json()).connected, true);

    const response = await previewRequest(
      base, "wcprev-" + session.id + ".example.test", "/my/page?x=1", "POST", "hello gateway",
    );
    assert.equal(response.status, 200);
    assert.equal(response.headers.get("x-test"), "proxied");
    assert.deepEqual(await response.json(), {
      url: "/my/page?x=1", method: "POST", body: "hello gateway", host: "127.0.0.1:" + port,
    });

    const closed = await fetch(statusUrl, {
      method: "DELETE", headers: { Authorization: "Bearer " + "q".repeat(32) },
    });
    assert.equal((await closed.json()).closed, true);
    assert.equal((await previewRequest(base, "wcprev-" + session.id + ".example.test")).status, 404);
  });
});

test("WebSocket HMR-like bidirectional relay", async () => {
  await fixture(async ({ base, port, gatewayPort, setAgent }) => {
    const session = await createSession(base, port, "preview-test-key-0002");
    setAgent(await connectPreviewAgent({
      gatewayUrl: "ws://127.0.0.1:" + gatewayPort + session.agent_path,
      agentToken: session.agent_token, port,
    }));
    const ws = new WebSocket("ws://127.0.0.1:" + gatewayPort + "/hmr",
      { headers: { Host: "wcprev-" + session.id + ".example.test" } });
    try {
      await new Promise((resolve, reject) => { ws.once("open", resolve); ws.once("error", reject); });
      const result = Promise.race([
        new Promise((resolve, reject) => {
          ws.once("message", data => resolve(data.toString()));
          ws.once("error", reject);
        }),
        new Promise((_, reject) => setTimeout(() => reject(new Error("WebSocket relay timed out")), 5000)),
      ]);
      ws.send("vite-update");
      assert.equal(await result, "vite-update");
    } finally { ws.close(); }
  });
});

test("agent token is authenticated in a non-echoed WebSocket subprotocol", async () => {
  await fixture(async ({ base, port, gatewayPort }) => {
    const session = await createSession(base, port, "preview-test-key-0003");
    const ws = new WebSocket(
      "ws://127.0.0.1:" + gatewayPort + session.agent_path,
      ["webcodex-preview-agent", "webcodex-preview-token.invalid"],
    );
    const outcome = await new Promise(resolve => {
      ws.once("open", () => resolve("opened"));
      ws.once("error", error => resolve(error.message));
      ws.once("close", () => resolve("closed"));
    });
    assert.notEqual(outcome, "opened");
  });
});
test("detached Runner-style Node eval agent reads stdin and serves preview", async () => {
  await fixture(async ({ base, port, gatewayPort }) => {
    const session = await createSession(base, port, "preview-test-key-0004");
    const script = readFileSync(new URL("./agent.mjs", import.meta.url), "utf8")
      + "\nconst config = await readPreviewConfig();\nawait connectPreviewAgent(config);\n";
    const child = spawn(process.execPath, ["--input-type=module", "-e", script], {
      stdio: ["pipe", "ignore", "pipe"],
    });
    let stderr = "";
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", chunk => { stderr += chunk; });
    try {
      child.stdin.end(JSON.stringify({
        gatewayUrl: "ws://127.0.0.1:" + gatewayPort + session.agent_path,
        agentToken: session.agent_token,
        port,
      }));
      let connected = false;
      for (let i = 0; i < 40; i++) {
        if (child.exitCode !== null) break;
        const status = await fetch(base + "/admin/sessions/" + session.id, {
          headers: { Authorization: "Bearer " + "q".repeat(32) },
        });
        if ((await status.json()).connected) { connected = true; break; }
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      assert.equal(connected, true, "detached agent failed to connect: " + stderr);
      const proxied = await previewRequest(
        base, "wcprev-" + session.id + ".example.test", "/from-detached",
      );
      assert.equal(proxied.status, 200);
      assert.equal((await proxied.json()).url, "/from-detached");
    } finally {
      child.kill();
      await new Promise(resolve => {
        if (child.exitCode !== null || child.signalCode !== null) return resolve();
        child.once("exit", resolve);
      });
    }
  });
});
