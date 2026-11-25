import { createPreview } from "./pdf-continuous-preview.mjs";
import { privateToolMetadata, readPdfDocument } from "./pdf-document-reader.mjs";

const el = id => document.getElementById(id);
let disposed = false, epoch = 0, identity = null, preview = null, nextId = 1;
let transferController = null;
const pending = new Map();
function send(method, params, timeoutMs = 20_000, signal = null) {
  if (disposed || signal?.aborted) return Promise.reject(new Error("PDF reader closed"));
  const id = nextId++;
  return new Promise((resolve, reject) => {
    let timer;
    const finish = (callback, value) => {
      if (!pending.delete(id)) return;
      clearTimeout(timer); signal?.removeEventListener("abort", abort);
      callback(value);
    };
    const abort = () => finish(reject, new Error("PDF preview closed"));
    pending.set(id, { resolve: value => finish(resolve, value), reject: error => finish(reject, error) });
    timer = setTimeout(() => finish(reject, new Error("Host request timed out")), timeoutMs);
    signal?.addEventListener("abort", abort, { once: true });
    parent.postMessage({ jsonrpc: "2.0", id, method, params }, "*");
  });
}
function stop() {
  epoch++; transferController?.abort(); transferController = null;
  preview?.destroy(); preview = null;
}
function close() {
  stop(); disposed = true;
  for (const entry of pending.values()) entry.reject(new Error("PDF reader closed"));
  pending.clear();
}
function canonicalEnvelope(value) {
  return !!value && typeof value === "object" && !Array.isArray(value)
    && typeof value.success === "boolean"
    && (Object.prototype.hasOwnProperty.call(value, "output") || Object.prototype.hasOwnProperty.call(value, "error"));
}
function textEnvelope(candidate) {
  if (!Array.isArray(candidate.content)) return null;
  for (const block of candidate.content) {
    if (!block || block.type !== "text" || typeof block.text !== "string" || block.text.length > 400_000) continue;
    try {
      const parsed = JSON.parse(block.text);
      if (canonicalEnvelope(parsed)) return parsed;
    } catch {}
  }
  return null;
}
// Hosts can replace structuredContent. Prefer the Server text copy, then the
// private Server copy, and accept a Host projection only when it is still canonical.
function envelope(result) {
  const candidates = [result, result?.result, result?.toolResult, result?.tool_result]
    .filter(candidate => candidate && typeof candidate === "object" && !Array.isArray(candidate));
  // Check all Server envelopes before any Host structuredContent projection,
  // including when the Host nests the original result in another field.
  for (const candidate of candidates) {
    if (canonicalEnvelope(candidate)) return candidate;
    const text = textEnvelope(candidate);
    if (text) return text;
  }
  for (const candidate of candidates) {
    const privateResult = candidate._meta?.["webcodex/pdfDocument"] ?? candidate.meta?.["webcodex/pdfDocument"];
    if (canonicalEnvelope(privateResult)) return privateResult;
  }
  for (const candidate of candidates) {
    const structured = candidate.structuredContent ?? candidate.structured_content;
    if (canonicalEnvelope(structured)) return structured;
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
  const controller = new AbortController(); transferController = controller;
  const current = () => !disposed && generation === epoch;
  el("retry").hidden = true; el("empty").hidden = true;
  preview = createPreview(el("reader"), {
    identity: selected, current, readDocument: readPdfDocument, sourceLabel: "PDF",
    failed: () => { if (current()) el("retry").hidden = false; },
    request: async (offset, remaining) => {
      const result = await send("tools/call", { name: "read_app_artifact_chunk", arguments: {
        project: selected.project, path: selected.path, sha256: selected.sha256, bytes: selected.bytes, byte_offset: offset,
      } }, remaining, controller.signal);
      const value = envelope(result);
      if (!value?.success) {
        if (value?.output?.error_kind === "snapshot_changed") throw new Error("PDF 版本已变化 · 请重新打开文档");
        throw new Error("PDF 读取不可用 · 请重试此版本");
      }
      const encoded = privateToolMetadata(result, "webcodex/artifactChunk")?.content_base64;
      if (typeof encoded !== "string") throw new Error("宿主结果中缺少 PDF 分块字节");
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
    const entry = pending.get(message.id);
    message.error ? entry.reject(new Error("宿主请求失败")) : entry.resolve(message.result); return;
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
        ? "PDF 不可用：宿主工具结果中没有可读取的 WebCodex 信封。"
        : value.success !== true
          ? "PDF 不可用：present_pdf 未成功。"
          : `PDF 不可用：PDF 文档标识无效（${invalidField}）。`;
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
send("ui/initialize", { protocolVersion: "2026-01-26", appInfo: { name: "webcodex-pdf", title: "PDF", version: "2.1.2" }, appCapabilities: {} })
  .then(result => {
    if (disposed) return; theme(result?.hostContext);
    parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/initialized", params: {} }, "*");
    if (result?.hostContext?.displayMode !== "fullscreen") parent.postMessage({ jsonrpc: "2.0", method: "ui/notifications/size-changed", params: { height: 720 } }, "*");
  }).catch(() => { if (!disposed) el("empty").textContent = "PDF 阅读器无法连接到宿主。"; });
