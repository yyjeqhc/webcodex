import assert from "node:assert/strict";
import http from "node:http";
import { once } from "node:events";
import { WebSocket, WebSocketServer } from "ws";
import { connectPreviewAgent } from "./agent.mjs";

const token = process.env.PREVIEW_ADMIN_TOKEN;
if (typeof token !== "string" || token.length < 24)
  throw new Error("PREVIEW_ADMIN_TOKEN is required");
const host = process.env.PREVIEW_PUBLIC_HOST || "https://codex.zetx.tech";
const content = "<!doctype html><title>WebCodex Public Preview Smoke</title><h1>Preview OK</h1>";
const local = http.createServer((req, res) => {
  res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  res.end(content);
});
const echo = new WebSocketServer({ server: local });
echo.on("connection", ws => ws.on("message", data => ws.send(data)));
local.listen(0, "127.0.0.1");
await once(local, "listening");
const port = local.address().port;
let agent;
let visitor;
try {
  const created = await fetch(host + "/admin/sessions", {
    method: "POST",
    headers: { authorization: "Bearer " + token, "content-type": "application/json" },
    body: JSON.stringify({ port }),
  });
  assert.equal(created.status, 201, "preview session creation failed");
  const session = await created.json();
  assert.match(session.preview_url, /^https:\/\/wcprev-[0-9a-f]{32}\.zetx\.tech\/$/);
  agent = await connectPreviewAgent({
    gatewayUrl: host.replace(/^https:/, "wss:") + session.agent_path,
    agentToken: session.agent_token, port,
  });
  const html = await fetch(session.preview_url);
  assert.equal(html.status, 200, "public HTTP preview did not respond");
  assert.ok((await html.text()).startsWith(content), "HTTP preview content was altered");
  visitor = new WebSocket(session.preview_url.replace(/^https:/, "wss:") + "hmr");
  await once(visitor, "open");
  const reply = once(visitor, "message");
  visitor.send("vite-hmr-smoke");
  const [payload] = await Promise.race([
    reply,
    new Promise((_, reject) => setTimeout(() => reject(new Error("HMR WebSocket timeout")), 8000)),
  ]);
  assert.equal(payload.toString(), "vite-hmr-smoke");
  console.log("PUBLIC_PREVIEW_HTTP=PASS");
  console.log("PUBLIC_PREVIEW_WEBSOCKET=PASS");
  console.log("PUBLIC_PREVIEW_URL=" + session.preview_url);
  const holdSeconds = Number(process.env.PREVIEW_KEEP_OPEN_SECONDS || 0);
  if (Number.isInteger(holdSeconds) && holdSeconds > 0 && holdSeconds <= 600) {
    console.log("TEMPORARY_DEMO_ACTIVE=1");
    await new Promise(resolve => setTimeout(resolve, holdSeconds * 1000));
  }
} finally {
  visitor?.close();
  agent?.close();
  await new Promise(resolve => echo.close(resolve));
  await new Promise(resolve => local.close(resolve));
}
