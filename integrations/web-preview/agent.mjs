import http from "node:http";

const MAX_HTTP_RESPONSE_BYTES = 4 * 1024 * 1024;
const MAX_MESSAGE_BYTES = 8 * 1024 * 1024;
const AGENT_PROTOCOL = "webcodex-preview-agent";
const TOKEN_PROTOCOL_PREFIX = "webcodex-preview-token.";
const HOP_HEADERS = new Set([
  "connection", "host", "keep-alive", "proxy-authenticate", "proxy-authorization",
  "te", "trailer", "transfer-encoding", "upgrade", "content-length",
]);

function send(ws, value) {
  if (ws.readyState !== WebSocket.OPEN || ws.bufferedAmount > MAX_MESSAGE_BYTES) return false;
  ws.send(JSON.stringify(value));
  return true;
}
function safeHeaders(headers) {
  return Object.fromEntries(Object.entries(headers || {}).filter(([key, value]) =>
    !HOP_HEADERS.has(key.toLowerCase()) && value != null));
}
function bufferFromMessage(data) {
  if (typeof data === "string") return Buffer.from(data);
  if (data instanceof ArrayBuffer) return Buffer.from(data);
  if (ArrayBuffer.isView(data)) return Buffer.from(data.buffer, data.byteOffset, data.byteLength);
  if (typeof Blob !== "undefined" && data instanceof Blob)
    return data.arrayBuffer().then(value => Buffer.from(value));
  return Buffer.from(String(data));
}
async function checkLocalPort(port) {
  await new Promise((resolve, reject) => {
    const request = http.get({ hostname: "127.0.0.1", port, path: "/", timeout: 2500 }, response => {
      response.resume();
      resolve();
    });
    request.once("error", reject);
    request.once("timeout", () => request.destroy(new Error("local_preview_timeout")));
  });
}

export async function connectPreviewAgent({ gatewayUrl, agentToken, port }) {
  if (!/^wss?:\/\//.test(gatewayUrl || "")) throw new Error("gatewayUrl must be a WebSocket URL");
  if (!agentToken || typeof agentToken !== "string") throw new Error("agentToken is required");
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error("invalid port");
  if (typeof WebSocket !== "function") throw new Error("Node.js 22 or later is required for the preview agent");
  await checkLocalPort(port);

  const control = new WebSocket(gatewayUrl, [
    AGENT_PROTOCOL,
    TOKEN_PROTOCOL_PREFIX + agentToken,
  ]);
  control.binaryType = "arraybuffer";
  const streams = new Map();

  function runHttp(message) {
    if (typeof message.requestId !== "string" || typeof message.path !== "string" ||
        !message.path.startsWith("/") || message.path.length > 8192 ||
        typeof message.body !== "string" || message.body.length > 3 * 1024 * 1024) return;
    const headers = safeHeaders(message.headers);
    if (headers.origin) headers.origin = "http://127.0.0.1:" + port;
    if (headers.referer) headers.referer = "http://127.0.0.1:" + port + "/";
    const request = http.request({
      hostname: "127.0.0.1", port, method: message.method || "GET",
      path: message.path, headers: { ...headers, host: "127.0.0.1:" + port },
      timeout: 15_000,
    }, response => {
      const parts = [];
      let length = 0;
      response.on("data", chunk => {
        length += chunk.length;
        if (length > MAX_HTTP_RESPONSE_BYTES) {
          request.destroy(new Error("response_too_large"));
          return;
        }
        parts.push(chunk);
      });
      response.on("end", () => {
        send(control, {
          type: "http_response", requestId: message.requestId,
          status: response.statusCode || 502, headers: safeHeaders(response.headers),
          body: Buffer.concat(parts).toString("base64"),
        });
      });
    });
    request.on("error", error => {
      const status = error.message === "response_too_large" ? 502 : 503;
      send(control, {
        type: "http_response", requestId: message.requestId,
        status, headers: { "content-type": "text/plain" },
        body: Buffer.from("Local preview unavailable").toString("base64"),
      });
    });
    if (message.body) request.end(Buffer.from(message.body, "base64"));
    else request.end();
  }

  function openSocket(message) {
    if (typeof message.streamId !== "string" || typeof message.path !== "string" ||
        !message.path.startsWith("/") || message.path.length > 8192 || streams.size >= 64) {
      send(control, { type: "ws_ready", streamId: message.streamId, accepted: false });
      return;
    }
    const protocols = Array.isArray(message.protocols)
      ? message.protocols.filter(value => typeof value === "string" && value.length <= 128).slice(0, 4)
      : [];
    const upstream = new WebSocket("ws://127.0.0.1:" + port + message.path, protocols);
    upstream.binaryType = "arraybuffer";
    streams.set(message.streamId, upstream);
    upstream.addEventListener("open", () =>
      send(control, { type: "ws_ready", streamId: message.streamId, accepted: true }));
    upstream.addEventListener("message", async event => {
      const bytes = await bufferFromMessage(event.data);
      send(control, {
        type: "ws_message", streamId: message.streamId,
        data: bytes.toString("base64"), binary: typeof event.data !== "string",
      });
    });
    upstream.addEventListener("error", () =>
      send(control, { type: "ws_ready", streamId: message.streamId, accepted: false }));
    upstream.addEventListener("close", () => {
      if (streams.get(message.streamId) !== upstream) return;
      streams.delete(message.streamId);
      send(control, { type: "ws_close", streamId: message.streamId });
    });
  }

  control.addEventListener("message", event => {
    let message;
    try { message = JSON.parse(String(event.data)); } catch { return; }
    if (!message || typeof message !== "object") return;
    if (message.type === "http_request") runHttp(message);
    else if (message.type === "ws_open") openSocket(message);
    else if (message.type === "ws_message") {
      const upstream = streams.get(message.streamId);
      if (upstream?.readyState === WebSocket.OPEN && typeof message.data === "string" &&
          message.data.length <= MAX_MESSAGE_BYTES) {
        const bytes = Buffer.from(message.data, "base64");
        upstream.send(message.binary === true ? bytes : bytes.toString("utf8"));
      }
    } else if (message.type === "ws_close") streams.get(message.streamId)?.close();
  });
  control.addEventListener("close", () => {
    for (const ws of streams.values()) ws.close();
    streams.clear();
  });
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("preview_gateway_connect_timeout")), 10_000);
    control.addEventListener("open", () => { clearTimeout(timer); resolve(); }, { once: true });
    control.addEventListener("error", () => { clearTimeout(timer); reject(new Error("preview_gateway_connect_failed")); }, { once: true });
    control.addEventListener("close", () => {
      clearTimeout(timer);
      reject(new Error("preview_gateway_closed_before_connect"));
    }, { once: true });
  });
  return {
    close: () => {
      control.close();
      for (const ws of streams.values()) ws.close();
      streams.clear();
    },
  };
}

export async function readPreviewConfig() {
  const chunks = [];
  let length = 0;
  for await (const chunk of process.stdin) {
    length += chunk.length;
    if (length > 4096) throw new Error("preview agent configuration is too large");
    chunks.push(chunk);
  }
  return JSON.parse(Buffer.concat(chunks).toString("utf8"));
}

if (process.argv[1] && import.meta.url === new URL("file://" + process.argv[1]).href) {
  const [gatewayUrl, portArgument] = process.argv.slice(2);
  const agentToken = process.env.PREVIEW_AGENT_TOKEN;
  if (!agentToken) throw new Error("Set PREVIEW_AGENT_TOKEN for the assigned session");
  connectPreviewAgent({ gatewayUrl, agentToken, port: Number(portArgument) })
    .then(() => console.log("Preview agent connected"))
    .catch(error => { console.error(error.message); process.exitCode = 1; });
}
