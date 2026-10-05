import { createPreview } from "./pdf-continuous-preview.mjs";
import { privateToolMetadata, readPdfDocument } from "./pdf-document-reader.mjs";

const el = id => document.getElementById(id);
let disposed = false, epoch = 0, identity = null, preview = null, nextId = 1;
const pending = new Map();
function send(method, params, timeoutMs = 20_000) {
  if (disposed) return Promise.reject(new Error("PDF reader closed"));
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error("Host request timed out")); }, timeoutMs);
    pending.set(id, { resolve, reject, timer });
    parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
  });
}
function stop() { epoch++; preview?.destroy(); preview = null; }
function close() {
  stop(); disposed = true;
  for (const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(new Error("PDF reader closed")); }
  pending.clear();
}
function envelope(result) {
  const candidates = [result, result?.result, result?.toolResult, result?.tool_result];
  for (const candidate of candidates) {
    if (!candidate || typeof candidate !== "object") continue;
    if (typeof candidate.success === "boolean") return candidate;
    const structured = candidate.structuredContent ?? candidate.structured_content;
    if (structured && typeof structured === "object") return structured;
    const privateResult = candidate._meta?.["webcodex/pdfDocument"] ?? candidate.meta?.["webcodex/pdfDocument"];
    if (privateResult && typeof privateResult === "object") return privateResult;
    for (const block of candidate.content || []) {
      if (block.type === "text") {
        try {
          const value = JSON.parse(block.text);
          if (typeof value.success === "boolean") return value;
        } catch {}
      }
    }
  }
  return null;
}
function invalidDocumentField(value) {
  if (!value || typeof value !== "object") return "document";
  if (typeof value.project !== "string" || value.project.length === 0 || value.project.length > 512) return "project";
  if (typeof value.path !== "string" || value.path.length === 0 || value.path.length > 512
    || !/\.pdf$/i.test(value.path) || value.path.startsWith("/") || value.path.includes("\\")
    || value.path.split("/").includes("..")) return "path";
  if (typeof value.name !== "string" || value.name.length === 0 || value.name.length > 255) return "name";
  if (!/^[0-9a-f]{64}$/.test(value.sha256 || "")) return "sha256";
  if (!Number.isSafeInteger(value.bytes) || value.bytes < 5 || value.bytes > 20 * 1024 * 1024) return "bytes";
  return null;
}
async function open() {
  stop(); const selected = identity, generation = epoch;
  const current = () => !disposed && generation === epoch;
  el("retry").hidden = true; el("empty").hidden = true;
  preview = createPreview(el("reader"), {
    identity: selected, current, readDocument: readPdfDocument, sourceLabel: "PDF",
    failed: () => { if (current()) el("retry").hidden = false; },
    request: async (offset, remaining) => {
      const result = await send("tools/call", { name: "read_app_artifact_chunk", arguments: {
        project: selected.project, path: selected.path, sha256: selected.sha256, bytes: selected.bytes, byte_offset: offset,
      } }, Math.min(remaining, 65_000));
      const value = envelope(result);
      if (!value?.success) {
        if (value?.output?.error_kind === "snapshot_changed") throw new Error("PDF version changed · reopen the document");
        throw new Error("PDF read unavailable · retry this version");
      }
      const encoded = privateToolMetadata(result, "webcodex/artifactChunk")?.content_base64;
      if (typeof encoded !== "string") throw new Error("PDF chunk bytes missing from Host result");
      return { page: value.output?.artifact_chunk, encoded };
    },
  });
  await preview.load();
}
function theme(context) {
  if (context?.theme === "light" || context?.theme === "dark") document.documentElement.dataset.theme = context.theme;
  if (context?.styles?.variables) {
    for (const [key, value] of Object.entries(context.styles.variables)) {
      if (key.startsWith("--") && typeof value === "string") document.documentElement.style.setProperty(key, value);
    }
  }
}
addEventListener("message", event => {
  if (event.source !== parent || event.data?.jsonrpc !== "2.0") return;
  const message = event.data;
  if (pending.has(message.id)) {
    const entry = pending.get(message.id); pending.delete(message.id); clearTimeout(entry.timer);
    message.error ? entry.reject(new Error("Host request failed")) : entry.resolve(message.result); return;
  }
  if (message.method === "ui/resource-teardown") {
    close(); parent.postMessage({ jsonrpc: "2.0", id: message.id, result: {} }, "*"); return;
  }
  if (disposed) return;
  if (message.method === "ui/notifications/host-context-changed") theme(message.params);
  if (message.method === "ui/notifications/tool-result") {
    const value = envelope(message.params), selected = value?.output?.pdf_document;
    const invalidField = invalidDocumentField(selected);
    if (!value?.success || invalidField) {
      stop(); identity = null; el("retry").hidden = true; el("reader").replaceChildren(); el("empty").hidden = false;
      el("empty").textContent = !value
        ? "PDF unavailable: Host tool result did not include a readable WebCodex envelope."
        : value.success !== true
          ? "PDF unavailable: present_pdf did not succeed."
          : `PDF unavailable: invalid PDF document identity (${invalidField}).`;
      return;
    }
    // Repeated Host delivery must not restart a live transfer/Worker.
    if (identity && ["project", "path", "sha256", "bytes"].every(key => identity[key] === selected[key])) return;
    identity = Object.freeze({ ...selected });
    el("filename").textContent = selected.name; el("filename").title = selected.path;
    void open();
  }
});
el("retry").onclick = () => { if (identity && !disposed) void open(); };
addEventListener("pagehide", close, { once: true });
addEventListener("beforeunload", close, { once: true });
send("ui/initialize", { protocolVersion: "2026-01-26", appInfo: { name: "webcodex-pdf", title: "PDF", version: "2.0.0" }, appCapabilities: {} })
  .then(result => {
    if (disposed) return; theme(result?.hostContext);
    parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/initialized", params: {} }, "*");
    if (result?.hostContext?.displayMode !== "fullscreen") parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 720 } }, "*");
  }).catch(() => { if (!disposed) el("empty").textContent = "PDF reader could not connect to the Host."; });
