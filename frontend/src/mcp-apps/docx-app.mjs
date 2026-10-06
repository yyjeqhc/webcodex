import { validDocxIdentity, readDocxDocument } from "./docx-reader.mjs";
import { renderDocx } from "./docx-preview.mjs";

export function mountDocxApp(win = window, doc = document) {
  const el = id => doc.getElementById(id), pending = new Map();
  let nextId = 1, generation = 0, disposed = false, identity = null, downloadBytes = null, zoom = 1, fitting = true, downloadAvailable = false, displayMode = "inline", displayModes = [];
  function send(method, params, timeoutMs = 20_000) {
    if (disposed) return Promise.reject(new Error("DOCX reader closed"));
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const timer = win.setTimeout(() => { pending.delete(id); reject(new Error("Host request timed out")); }, timeoutMs);
      pending.set(id, { resolve, reject, timer, method });
      win.parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
    });
  }
  function stop() {
    generation++; downloadBytes = null;
    el("download").hidden = true; el("stage").replaceChildren();
    for (const [id, item] of pending) { if (item.method === "tools/call") { win.clearTimeout(item.timer); item.reject(new Error("DOCX reader closed")); pending.delete(id); } }
  }
  function close() { stop(); disposed = true; resizeObserver.disconnect(); for (const item of pending.values()) { win.clearTimeout(item.timer); item.reject(new Error("DOCX reader closed")); } pending.clear(); win.removeEventListener("message", onMessage); }
  function privateMetadata(result, key) {
    for (const candidate of [result, result?.result, result?.toolResult, result?.tool_result]) {
      const metadata = candidate?._meta?.[key] ?? candidate?.meta?.[key];
      if (metadata && typeof metadata === "object") return metadata;
    }
    return null;
  }
  function envelope(result, privateKey) {
    for (const candidate of [result, result?.result, result?.toolResult, result?.tool_result]) {
      if (!candidate || typeof candidate !== "object") continue;
      if (typeof candidate.success === "boolean") return candidate;
      const structured = candidate.structuredContent ?? candidate.structured_content;
      if (structured && typeof structured === "object") return structured;
      const privateResult = privateKey && privateMetadata(candidate, privateKey);
      if (privateResult) return privateResult;
      for (const block of candidate.content || []) {
        if (block.type !== "text" || typeof block.text !== "string" || block.text.length > 400_000) continue;
        try { const value = JSON.parse(block.text); if (typeof value?.success === "boolean") return value; } catch {}
      }
    }
    return null;
  }
  function scale(value) { zoom = Math.max(0.25, Math.min(2, value)); el("stage").style.zoom = String(zoom); el("zoom").textContent = `${Math.round(zoom * 100)}%`; }
  function fitWidth() {
    const pages = [...el("stage").querySelectorAll("section.docx")], width = el("viewport").clientWidth;
    const pageWidth = Math.max(0, ...pages.map(page => page.offsetWidth));
    if (!disposed && fitting && pageWidth && width > 32) scale(width / (pageWidth + 32));
  }
  const resizeObserver = new win.ResizeObserver(fitWidth);
  resizeObserver.observe(el("viewport"));
  function updateDisplayMode() {
    const target = displayMode === "fullscreen" ? "inline" : "fullscreen";
    el("fullscreen").hidden = !displayModes.includes(target);
    el("fullscreen").textContent = displayMode === "fullscreen" ? "Collapse" : "Expand";
  }
  async function download() {
    if (!downloadAvailable || !downloadBytes || disposed) return;
    const bytes = downloadBytes, selected = identity, epoch = generation;
    el("download").disabled = true;
    try {
      const parts = [];
      for (let offset = 0; offset < bytes.length; offset += 32768) parts.push(String.fromCharCode(...bytes.subarray(offset, offset + 32768)));
      const result = await send("ui/download-file", { contents: [{ type: "resource", resource: {
        uri: `file:///${encodeURIComponent(selected.name)}`, mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", blob: win.btoa(parts.join("")),
      } }] });
      if (result?.isError) throw new Error("Download declined");
    } catch {
      if (!disposed && epoch === generation) {
        el("status").hidden = false; el("status").textContent = "Download unavailable. You can request a file export in the chat.";
      }
    } finally { if (!disposed && epoch === generation) el("download").disabled = false; }
  }
  async function open() {
    stop(); fitting = true; const selected = identity, epoch = generation, current = () => !disposed && epoch === generation;
    el("retry").hidden = true; el("status").hidden = false; el("status").textContent = "Loading document…";
    try {
      const bytes = await readDocxDocument({ identity: selected, current,
        progress: (loaded, total) => { if (current()) el("status").textContent = `Loading document… ${Math.round(loaded / total * 100)}%`; },
        request: async (offset, remaining) => {
          const result = await send("tools/call", { name: "read_app_artifact_chunk", arguments: {
            project: selected.project, path: selected.path, sha256: selected.sha256, bytes: selected.bytes, byte_offset: offset,
          } }, remaining);
          const value = envelope(result);
          if (!value?.success) throw new Error(value?.output?.error_kind === "snapshot_changed"
            ? "Document changed. Reopen it to view the new version." : "Document read unavailable. Retry this version.");
          return { page: value.output?.artifact_chunk,
            encoded: privateMetadata(result, "webcodex/artifactChunk")?.content_base64 };
        },
      });
      if (!current()) return;
      el("status").textContent = "Rendering document…";
      const detached = doc.createElement("div"); detached.className = "docx-stage";
      await renderDocx(bytes, detached, current);
      if (!current()) return;
      el("stage").replaceChildren(...detached.childNodes);
      fitWidth(); downloadBytes = bytes;
      el("download").disabled = false; el("download").hidden = !downloadAvailable;
      el("status").hidden = true;
    } catch (error) {
      if (!current()) return;
      el("stage").replaceChildren(); el("status").textContent = error.message; el("retry").hidden = false;
    }
  }
  function applyHostContext(context) {
    if (Array.isArray(context?.availableDisplayModes)) displayModes = context.availableDisplayModes;
    if (typeof context?.displayMode === "string") displayMode = context.displayMode;
    updateDisplayMode();
    if (["light", "dark"].includes(context?.theme)) doc.documentElement.dataset.theme = context.theme;
    for (const [key, value] of Object.entries(context?.styles?.variables || {})) {
      if (/^--[A-Za-z0-9_-]+$/.test(key) && typeof value === "string") doc.documentElement.style.setProperty(key, value);
    }
  }
  function onMessage(event) {
    if (event.source !== win.parent || event.data?.jsonrpc !== "2.0") return;
    const message = event.data;
    if (pending.has(message.id)) {
      const item = pending.get(message.id); pending.delete(message.id); win.clearTimeout(item.timer);
      message.error ? item.reject(new Error("Host request failed")) : item.resolve(message.result); return;
    }
    if (message.method === "ui/resource-teardown") { close(); win.parent.postMessage({ jsonrpc: "2.0", id: message.id, result: {} }, "*"); return; }
    if (disposed) return;
    if (message.method === "ui/notifications/host-context-changed") applyHostContext(message.params);
    if (message.method === "ui/notifications/tool-result") {
      const value = envelope(message.params, "webcodex/docxDocument"), selected = value?.output?.docx_document;
      if (!value?.success || !validDocxIdentity(selected)) {
        stop(); identity = null; el("retry").hidden = true; el("status").hidden = false;
        el("status").textContent = "DOCX unavailable. Open an authorized project document."; return;
      }
      if (identity && ["project", "path", "sha256", "bytes"].every(key => identity[key] === selected[key])) return;
      identity = Object.freeze({ ...selected }); el("filename").textContent = selected.name; el("filename").title = selected.path; void open();
    }
  }
  win.addEventListener("message", onMessage); win.addEventListener("pagehide", close, { once: true });
  el("retry").onclick = () => { if (identity && !disposed) void open(); };
  el("zoom-out").onclick = () => { fitting = false; scale(zoom - 0.1); };
  el("zoom-in").onclick = () => { fitting = false; scale(zoom + 0.1); };
  el("fit").onclick = () => { fitting = true; fitWidth(); };
  el("download").onclick = () => { void download(); };
  el("fullscreen").onclick = () => {
    const mode = displayMode === "fullscreen" ? "inline" : "fullscreen";
    if (!displayModes.includes(mode)) return;
    void send("ui/request-display-mode", { mode }).then(result => {
      if (!disposed && typeof result?.mode === "string") { displayMode = result.mode; updateDisplayMode(); }
    }).catch(() => {});
  };
  send("ui/initialize", { protocolVersion: "2026-01-26", appInfo: { name: "webcodex-docx", title: "DOCX", version: "1.0.0" }, appCapabilities: { availableDisplayModes: ["inline", "fullscreen"] } })
    .then(result => {
      if (disposed) return; downloadAvailable = !!result?.hostCapabilities?.downloadFile; applyHostContext(result?.hostContext);
      el("download").hidden = !downloadAvailable || !downloadBytes;
      win.parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/initialized", params: {} }, "*");
      if (result?.hostContext?.displayMode !== "fullscreen") win.parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 720 } }, "*");
    }).catch(() => { if (!disposed && !identity) el("status").textContent = "DOCX reader could not connect to the Host."; });
  return { close };
}
mountDocxApp();
