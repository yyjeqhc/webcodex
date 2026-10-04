import { validDocxIdentity, readDocxDocument } from "./docx-reader.mjs";
import { renderDocx } from "./docx-preview.mjs";

export function mountDocxApp(win = window, doc = document) {
  const el = id => doc.getElementById(id), pending = new Map();
  let nextId = 1, generation = 0, disposed = false, identity = null, downloadUrl = null, zoom = 1;
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
    generation++; if (downloadUrl) win.URL.revokeObjectURL(downloadUrl); downloadUrl = null;
    el("download").hidden = true; el("download").removeAttribute("href"); el("stage").replaceChildren();
    for (const [id, item] of pending) { if (item.method === "tools/call") { win.clearTimeout(item.timer); item.reject(new Error("DOCX reader closed")); pending.delete(id); } }
  }
  function close() { stop(); disposed = true; for (const item of pending.values()) { win.clearTimeout(item.timer); item.reject(new Error("DOCX reader closed")); } pending.clear(); win.removeEventListener("message", onMessage); }
  function envelope(result) {
    if (result?.structuredContent) return result.structuredContent;
    if (result?._meta?.["webcodex/docxDocument"]) return result._meta["webcodex/docxDocument"];
    for (const block of result?.content || []) { if (block.type === "text") { try { const value = JSON.parse(block.text); if (typeof value.success === "boolean") return value; } catch {} } }
    return null;
  }
  function scale(value) { zoom = Math.max(0.25, Math.min(2, value)); el("stage").style.zoom = String(zoom); el("zoom").textContent = `${Math.round(zoom * 100)}%`; }
  async function open() {
    stop(); const selected = identity, epoch = generation, current = () => !disposed && epoch === generation;
    el("retry").hidden = true; el("status").hidden = false; el("status").textContent = "Loading document…";
    try {
      const bytes = await readDocxDocument({ identity: selected, current,
        progress: (loaded, total) => { if (current()) el("status").textContent = `Loading document… ${Math.round(loaded / total * 100)}%`; },
        request: async (offset, remaining) => {
          const result = await send("tools/call", { name: "read_docx_chunk", arguments: {
            project: selected.project, path: selected.path, sha256: selected.sha256, bytes: selected.bytes, byte_offset: offset,
          } }, Math.min(remaining, 65_000));
          const value = envelope(result);
          if (!value?.success) throw new Error(value?.output?.error_kind === "snapshot_changed"
            ? "Document changed. Reopen it to view the new version." : "Document read unavailable. Retry this version.");
          return { page: value.output?.docx_chunk, encoded: result._meta?.["webcodex/docxChunk"]?.content_base64 };
        },
      });
      if (!current()) return;
      el("status").textContent = "Rendering document…";
      const detached = doc.createElement("div"); detached.className = "docx-stage";
      await renderDocx(bytes, detached, current);
      if (!current()) return;
      el("stage").replaceChildren(...detached.childNodes);
      const page = el("stage").querySelector("section.docx");
      scale(page?.offsetWidth && el("viewport").clientWidth > 32 ? Math.min(1, (el("viewport").clientWidth - 32) / page.offsetWidth) : 1);
      downloadUrl = win.URL.createObjectURL(new win.Blob([bytes], { type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" }));
      el("download").href = downloadUrl; el("download").download = selected.name; el("download").hidden = false;
      el("status").hidden = true;
    } catch (error) {
      if (!current()) return;
      el("stage").replaceChildren(); el("status").textContent = error.message; el("retry").hidden = false;
    }
  }
  function theme(context) {
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
    if (message.method === "ui/notifications/host-context-changed") theme(message.params);
    if (message.method === "ui/notifications/tool-result") {
      const value = envelope(message.params), selected = value?.output?.docx_document;
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
  el("zoom-out").onclick = () => scale(zoom - 0.1); el("zoom-in").onclick = () => scale(zoom + 0.1);
  el("fit").onclick = () => { const page = el("stage").querySelector("section.docx"); if (page) scale((el("viewport").clientWidth - 32) / page.offsetWidth); };
  el("fullscreen").onclick = () => { void send("ui/request-display-mode", { mode: "fullscreen" }).catch(() => {}); };
  send("ui/initialize", { protocolVersion: "2026-01-26", appInfo: { name: "webcodex-docx", title: "DOCX", version: "1.0.0" }, appCapabilities: {} })
    .then(result => {
      if (disposed) return; theme(result?.hostContext);
      win.parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/initialized", params: {} }, "*");
      if (result?.hostContext?.displayMode !== "fullscreen") win.parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 720 } }, "*");
    }).catch(() => { if (!disposed && !identity) el("status").textContent = "DOCX reader could not connect to the Host."; });
  return { close };
}
mountDocxApp();
