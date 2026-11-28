import http from "node:http";
import { randomBytes, timingSafeEqual } from "node:crypto";
import { WebSocketServer } from "ws";

const MAX_REQUEST_BYTES = 2 * 1024 * 1024;
const MAX_MESSAGE_BYTES = 8 * 1024 * 1024;
const MAX_INFLIGHT = 32;
const DEFAULT_TTL_MS = 30 * 60 * 1000;
const RESPONSE_TIMEOUT_MS = 20_000;
const AGENT_PROTOCOL = "webcodex-preview-agent";
const TOKEN_PROTOCOL_PREFIX = "webcodex-preview-token.";
const IGNORED_HEADERS = new Set([
  "connection", "host", "keep-alive", "proxy-authenticate", "proxy-authorization",
  "te", "trailer", "transfer-encoding", "upgrade", "content-length",
]);
const IGNORED_RESPONSE_HEADERS = new Set([
  ...IGNORED_HEADERS, "content-security-policy", "content-security-policy-report-only",
  "x-frame-options", "cross-origin-opener-policy", "cross-origin-embedder-policy",
]);

function json(res, status, value) {
  const body = Buffer.from(JSON.stringify(value));
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
    "content-length": body.length,
  });
  res.end(body);
}
function error(res, status, message) { json(res, status, { error: message }); }
function safeEqual(left, right) {
  if (typeof left !== "string" || typeof right !== "string") return false;
  const a = Buffer.from(left);
  const b = Buffer.from(right);
  return a.length === b.length && timingSafeEqual(a, b);
}
function cleanHeaders(headers, excluded) {
  return Object.fromEntries(Object.entries(headers).filter(([key, val]) =>
    !excluded.has(key.toLowerCase()) && val !== undefined));
}
function hostName(value) {
  return (value || "").split(":")[0].toLowerCase().replace(/\.$/, "");
}
function sessionHost(id, suffix) { return "wcprev-" + id + "." + suffix; }
function sessionFromHost(value, suffix) {
  const hostname = hostName(value);
  const subdomain = hostname.endsWith("." + suffix)
    ? hostname.slice(0, -(suffix.length + 1))
    : "";
  return /^wcprev-([0-9a-f]{32})$/.exec(subdomain)?.[1] || "";
}
function validIdempotencyKey(value) {
  return typeof value === "string" && value.length >= 16 && value.length <= 100 &&
    /^[A-Za-z0-9_-]+$/.test(value);
}
async function bodyWithLimit(stream) {
  const chunks = [];
  let length = 0;
  for await (const chunk of stream) {
    length += chunk.length;
    if (length > MAX_REQUEST_BYTES) throw new Error("body_too_large");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}
function send(ws, value) {
  if (ws?.readyState !== 1) return false;
  if (ws.bufferedAmount > MAX_MESSAGE_BYTES) return false;
  ws.send(JSON.stringify(value));
  return true;
}
function sessionAlive(session) {
  return session && Date.now() < session.expiresAt;
}
function sessionView(session, previewDomain, publicScheme) {
  return {
    id: session.id,
    agent_token: session.token,
    agent_path: "/agent/" + session.id,
    preview_url: publicScheme + "://" + sessionHost(session.id, previewDomain) + "/",
    expires_at: new Date(session.expiresAt).toISOString(),
  };
}
function agentTokenFromRequest(req) {
  const authorization = req.headers.authorization;
  if (typeof authorization === "string" && authorization.startsWith("Bearer "))
    return authorization.slice("Bearer ".length);
  const protocols = String(req.headers["sec-websocket-protocol"] || "")
    .split(",").map(value => value.trim()).filter(Boolean);
  return protocols.find(value => value.startsWith(TOKEN_PROTOCOL_PREFIX))
    ?.slice(TOKEN_PROTOCOL_PREFIX.length) || "";
}

export function createPreviewGateway({
  adminToken, previewDomain = "zetx.tech", publicScheme = "https",
  sessionLimit = 24, sessionTtlMs = DEFAULT_TTL_MS,
} = {}) {
  if (typeof adminToken !== "string" || adminToken.length < 24)
    throw new Error("PREVIEW_ADMIN_TOKEN must have at least 24 characters");
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(previewDomain))
    throw new Error("invalid preview domain");
  if (!["http", "https"].includes(publicScheme))
    throw new Error("invalid public scheme");

  const sessions = new Map();
  const idempotencyKeys = new Map();
  const agentWss = new WebSocketServer({
    noServer: true,
    maxPayload: MAX_MESSAGE_BYTES,
    handleProtocols(protocols) {
      return protocols.has(AGENT_PROTOCOL) ? AGENT_PROTOCOL : false;
    },
  });
  const visitorWss = new WebSocketServer({ noServer: true, maxPayload: MAX_MESSAGE_BYTES });
  let serial = 0;

  function closeSession(session) {
    sessions.delete(session.id);
    if (session.idempotencyKey && idempotencyKeys.get(session.idempotencyKey) === session.id)
      idempotencyKeys.delete(session.idempotencyKey);
    for (const pending of session.pending.values()) {
      clearTimeout(pending.timer);
      pending.respond(503, { "content-type": "text/plain" }, Buffer.from("Preview disconnected"));
    }
    session.pending.clear();
    for (const visitor of session.visitors.values()) visitor.close(1001, "preview ended");
    session.visitors.clear();
    session.visitorQueues.clear();
    session.visitorReady.clear();
    session.agent?.close(1001, "preview ended");
    session.agent = null;
  }
  function cleanup() {
    for (const session of sessions.values()) {
      if (!sessionAlive(session)) closeSession(session);
    }
  }
  function authorizedAdmin(req) {
    return safeEqual(req.headers.authorization, "Bearer " + adminToken);
  }

  async function onHttp(req, res) {
    const path = new URL(req.url || "/", "http://local").pathname;
    if (path === "/healthz" && req.method === "GET") {
      json(res, 200, { status: "ok" });
      return;
    }
    if (path === "/admin/sessions" && req.method === "POST") {
      if (!authorizedAdmin(req)) { error(res, 401, "unauthorized"); return; }
      let input;
      try { input = JSON.parse((await bodyWithLimit(req)).toString("utf8")); }
      catch { error(res, 400, "invalid_json"); return; }
      if (!Number.isInteger(input?.port) || input.port < 1 || input.port > 65535) {
        error(res, 400, "invalid_port");
        return;
      }
      if (input.idempotency_key != null && !validIdempotencyKey(input.idempotency_key)) {
        error(res, 400, "invalid_idempotency_key");
        return;
      }
      cleanup();
      if (input.idempotency_key) {
        const existingId = idempotencyKeys.get(input.idempotency_key);
        const existing = sessions.get(existingId);
        if (sessionAlive(existing)) {
          if (existing.port !== input.port) { error(res, 409, "idempotency_conflict"); return; }
          json(res, 200, { ...sessionView(existing, previewDomain, publicScheme), reused: true });
          return;
        }
        idempotencyKeys.delete(input.idempotency_key);
      }
      if (sessions.size >= sessionLimit) { error(res, 429, "session_capacity"); return; }
      const id = randomBytes(16).toString("hex");
      const token = randomBytes(32).toString("base64url");
      const session = {
        id, token, port: input.port, idempotencyKey: input.idempotency_key || null,
        expiresAt: Date.now() + sessionTtlMs, agent: null, pending: new Map(),
        visitors: new Map(), visitorQueues: new Map(), visitorReady: new Set(),
      };
      sessions.set(id, session);
      if (session.idempotencyKey) idempotencyKeys.set(session.idempotencyKey, id);
      json(res, 201, { ...sessionView(session, previewDomain, publicScheme), reused: false });
      return;
    }
    const adminSession = /^\/admin\/sessions\/([0-9a-f]{32})$/.exec(path);
    if (adminSession && ["GET", "DELETE"].includes(req.method)) {
      if (!authorizedAdmin(req)) { error(res, 401, "unauthorized"); return; }
      cleanup();
      const session = sessions.get(adminSession[1]);
      if (!sessionAlive(session)) { error(res, 404, "preview_not_found"); return; }
      if (req.method === "GET") {
        json(res, 200, {
          id: session.id,
          port: session.port,
          connected: session.agent?.readyState === 1,
          expires_at: new Date(session.expiresAt).toISOString(),
        });
      } else {
        closeSession(session);
        json(res, 200, { id: adminSession[1], closed: true });
      }
      return;
    }

    const id = sessionFromHost(req.headers.host, previewDomain);
    const session = sessions.get(id);
    if (!/^[0-9a-f]{32}$/.test(id) || !sessionAlive(session)) {
      error(res, 404, "preview_not_found");
      return;
    }
    if (!session.agent || session.agent.readyState !== 1) {
      error(res, 503, "preview_agent_disconnected");
      return;
    }
    if (session.pending.size >= MAX_INFLIGHT) { error(res, 429, "preview_busy"); return; }
    let body;
    try { body = await bodyWithLimit(req); }
    catch { error(res, 413, "request_too_large"); return; }
    const requestId = (++serial).toString(36);
    const headers = cleanHeaders(req.headers, IGNORED_HEADERS);
    const timer = setTimeout(() => {
      session.pending.delete(requestId);
      if (!res.headersSent) error(res, 504, "preview_timeout");
    }, RESPONSE_TIMEOUT_MS);
    session.pending.set(requestId, {
      timer,
      respond(status, headers, body) {
        if (res.headersSent || res.destroyed) return;
        const safeHeaders = cleanHeaders(headers, IGNORED_RESPONSE_HEADERS);
        if (safeHeaders["set-cookie"]) {
          safeHeaders["set-cookie"] = [safeHeaders["set-cookie"]].flat().map(cookie =>
            cookie.split(";").filter(part => !/^\s*domain=/i.test(part)).join(";") + "; Secure");
        }
        safeHeaders["referrer-policy"] = "no-referrer";
        safeHeaders["x-content-type-options"] = "nosniff";
        res.writeHead(status >= 100 && status <= 599 ? status : 502, safeHeaders);
        if (req.method === "HEAD" || status === 204 || status === 304) res.end();
        else res.end(body);
      },
    });
    req.on("close", () => { /* Request completion is not a response cancellation. */ });
    if (!send(session.agent, {
      type: "http_request", requestId, method: req.method, path: req.url,
      headers, body: body.toString("base64"),
    })) {
      clearTimeout(timer);
      session.pending.delete(requestId);
      error(res, 503, "agent_send_failed");
    }
  }

  const server = http.createServer((req, res) => {
    void onHttp(req, res).catch(() => {
      if (!res.headersSent) error(res, 500, "internal_error");
    });
  });

  server.on("upgrade", (req, socket, head) => {
    const pathname = new URL(req.url || "/", "http://local").pathname;
    if (pathname.startsWith("/agent/")) {
      const id = pathname.slice("/agent/".length);
      const session = sessions.get(id);
      const token = agentTokenFromRequest(req);
      if (!/^[0-9a-f]{32}$/.test(id) || !sessionAlive(session) ||
          !safeEqual(token, session.token) ||
          (session.agent && session.agent.readyState === 1)) {
        socket.destroy();
        return;
      }
      agentWss.handleUpgrade(req, socket, head, ws => {
        session.agent = ws;
        ws.isAlive = true;
        ws.on("pong", () => { ws.isAlive = true; });
        ws.on("message", data => {
          let message;
          try { message = JSON.parse(data.toString()); } catch { return; }
          if (!message || typeof message !== "object") return;
          if (message.type === "http_response") {
            const pending = session.pending.get(message.requestId);
            if (!pending) return;
            session.pending.delete(message.requestId);
            clearTimeout(pending.timer);
            if (typeof message.body !== "string" || message.body.length > MAX_MESSAGE_BYTES) {
              pending.respond(502, {}, Buffer.from("Oversized agent response")); return;
            }
            pending.respond(Number(message.status) || 502, message.headers || {}, Buffer.from(message.body, "base64"));
          } else if (message.type === "ws_ready") {
            const visitor = session.visitors.get(message.streamId);
            if (visitor && message.accepted !== true) visitor.close(1011, "upstream rejected");
            else if (visitor && message.accepted === true) {
              session.visitorReady.add(message.streamId);
              for (const queued of session.visitorQueues.get(message.streamId) || []) {
                if (!send(session.agent, { type: "ws_message", streamId: message.streamId, ...queued })) {
                  visitor.close(1011, "relay congested"); break;
                }
              }
              session.visitorQueues.delete(message.streamId);
            }
          } else if (message.type === "ws_message") {
            const visitor = session.visitors.get(message.streamId);
            if (visitor?.readyState === 1 && typeof message.data === "string" &&
                message.data.length <= MAX_MESSAGE_BYTES) {
              visitor.send(Buffer.from(message.data, "base64"), { binary: message.binary === true });
            }
          } else if (message.type === "ws_close") {
            session.visitors.get(message.streamId)?.close(1000, "upstream closed");
          }
        });
        ws.on("close", () => {
          if (session.agent !== ws) return;
          session.agent = null;
          for (const pending of session.pending.values()) {
            clearTimeout(pending.timer);
            pending.respond(503, {}, Buffer.from("Agent disconnected"));
          }
          session.pending.clear();
          for (const visitor of session.visitors.values()) visitor.close(1011, "agent disconnected");
          session.visitors.clear();
          session.visitorQueues.clear();
          session.visitorReady.clear();
        });
      });
      return;
    }

    const id = sessionFromHost(req.headers.host, previewDomain);
    const session = sessions.get(id);
    if (!/^[0-9a-f]{32}$/.test(id) || !sessionAlive(session) || session.agent?.readyState !== 1) {
      socket.destroy();
      return;
    }
    visitorWss.handleUpgrade(req, socket, head, visitor => {
      const streamId = randomBytes(12).toString("hex");
      session.visitors.set(streamId, visitor);
      session.visitorQueues.set(streamId, []);
      const protocols = String(req.headers["sec-websocket-protocol"] || "")
        .split(",").map(value => value.trim()).filter(Boolean).slice(0, 4);
      if (!send(session.agent, {
        type: "ws_open", streamId, path: req.url, protocols,
        headers: cleanHeaders(req.headers, IGNORED_HEADERS),
      })) visitor.close(1011, "agent unavailable");
      visitor.on("message", (data, binary) => {
        if (data.length > MAX_MESSAGE_BYTES / 2) { visitor.close(1009, "message too large"); return; }
        const frame = { data: Buffer.from(data).toString("base64"), binary };
        if (session.visitorReady.has(streamId)) {
          if (!send(session.agent, { type: "ws_message", streamId, ...frame }))
            visitor.close(1011, "relay congested");
        } else {
          const queue = session.visitorQueues.get(streamId);
          if (!queue || queue.length >= 8 || data.length > 65536) {
            visitor.close(1011, "upstream not ready"); return;
          }
          queue.push(frame);
        }
      });
      visitor.on("close", () => {
        session.visitors.delete(streamId);
        session.visitorQueues.delete(streamId);
        session.visitorReady.delete(streamId);
        send(session.agent, { type: "ws_close", streamId });
      });
    });
  });

  const cleaner = setInterval(cleanup, 30_000);
  const heartbeat = setInterval(() => {
    for (const session of sessions.values()) {
      const agent = session.agent;
      if (!agent || agent.readyState !== 1) continue;
      if (agent.isAlive === false) {
        agent.terminate();
        continue;
      }
      agent.isAlive = false;
      agent.ping();
    }
  }, 30_000);
  cleaner.unref();
  heartbeat.unref();
  return {
    server, sessions,
    close: async () => {
      clearInterval(cleaner);
      clearInterval(heartbeat);
      for (const session of [...sessions.values()]) closeSession(session);
      await new Promise(resolve => server.close(resolve));
      agentWss.close();
      visitorWss.close();
    },
  };
}

if (process.argv[1] && import.meta.url === new URL("file://" + process.argv[1]).href) {
  const gateway = createPreviewGateway({
    adminToken: process.env.PREVIEW_ADMIN_TOKEN,
    previewDomain: process.env.PREVIEW_DOMAIN || "zetx.tech",
    publicScheme: process.env.PREVIEW_PUBLIC_SCHEME || "https",
  });
  gateway.server.listen(Number(process.env.PREVIEW_PORT || 8093), "0.0.0.0");
}
