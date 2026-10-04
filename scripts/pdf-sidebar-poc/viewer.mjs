import { App, applyDocumentTheme, applyHostStyleVariables } from "@modelcontextprotocol/ext-apps";
import { getDocument, PDFWorker, TextLayer, version } from "pdfjs-dist";
import * as mainThreadParser from "pdfjs-dist/build/pdf.worker.mjs";

const $ = id => document.getElementById(id);
const MAX_BYTES = 20 * 1024 * 1024;
const app = new App({ name: "WebCodex PDF feasibility", version: "0.1.0" }, {}, { autoResize: false });
const diag = { pdfjs: version, host: "connecting", engine: "none", workerFailure: null,
  document: null, pages: 0, page: 0, scale: 0, renderMs: 0, activeWorkers: 0, activeBlobUrls: 0,
  assetReads: [], cspViolations: [], text: "", renders: 0 };
let loading = null, pdf = null, worker = null, port = null, workerUrl = null;
let pageNumber = 1, scale = 1, fit = true, renderTask = null, textLayer = null;
let queue = Promise.resolve(), resizeTimer, disposed = false, blockControls = false;
function status(text) { $("status").textContent = text; }
function diagnostic() { $("diagnostics").textContent = JSON.stringify(diag, null, 2); }
function controls() {
  for (const id of ["prev", "next", "minus", "plus", "fit", "find", "close"]) $(id).disabled = blockControls || !pdf;
  $("prev").disabled ||= pageNumber <= 1;
  $("next").disabled ||= pageNumber >= (pdf?.numPages ?? 0);
  $("fullscreen").disabled = diag.host !== "connected";
}
function enqueue(action, block = true) {
  queue = queue.then(async () => {
    if (disposed) return;
    blockControls = block; controls();
    try { await action(); } catch (error) { status(`无法预览：${error.message}`); diag.error = error.message; }
    finally { blockControls = false; controls(); diagnostic(); }
  });
  return queue;
}
const decode = value => Uint8Array.from(atob(value), c => c.charCodeAt(0));
async function unzip(value) {
  return new Uint8Array(await new Response(new Blob([decode(value)]).stream().pipeThrough(new DecompressionStream("gzip"))).arrayBuffer());
}
class InlineBinaryDataFactory {
  async fetch({ kind, filename }) {
    const key = `${kind}/${filename}`;
    const value = PDF_POC_ASSETS[key];
    if (!value) throw new Error(`未打包的 PDF.js 资源：${key}`);
    if (!diag.assetReads.includes(key)) diag.assetReads.push(key);
    return unzip(value); // Fresh buffer: PDF.js can transfer it to the worker.
  }
}
document.addEventListener("securitypolicyviolation", event => {
  diag.cspViolations.push({ directive: event.effectiveDirective, blocked: event.blockedURI });
  diag.cspViolations = diag.cspViolations.slice(-12); diagnostic();
});
async function createWorker() {
  diag.workerFailure = null;
  delete globalThis.pdfjsWorker;
  workerUrl = URL.createObjectURL(new Blob([await unzip(PDF_POC_WORKER)], { type: "text/javascript" }));
  diag.activeBlobUrls = 1;
  try {
    port = new Worker(workerUrl, { type: "module", name: "webcodex-pdf-poc" });
    diag.activeWorkers = 1;
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => finish(new Error("Worker 启动超过 3 秒")), 3000);
      const message = event => { if (event.data?.pdfPocReady) finish(); };
      const error = () => finish(new Error("Worker 被 CSP 拒绝或无法启动"));
      function finish(reason) {
        clearTimeout(timer); port.removeEventListener("message", message); port.removeEventListener("error", error);
        reason ? reject(reason) : resolve();
      }
      port.addEventListener("message", message); port.addEventListener("error", error);
    });
    worker = new PDFWorker({ port });
    diag.engine = "module-worker";
  } catch (error) {
    port?.terminate(); port = null; diag.activeWorkers = 0;
    URL.revokeObjectURL(workerUrl); workerUrl = null; diag.activeBlobUrls = 0;
    // Pinned PDF.js 5.6.205 hook. This experiment deliberately includes the parser
    // in the main bundle too, so fallback never dynamically imports a blocked URL.
    globalThis.pdfjsWorker = mainThreadParser;
    worker = new PDFWorker();
    diag.engine = "main-thread-fallback"; diag.workerFailure = error.message;
  }
  await worker.promise;
}
async function closeDocument() {
  renderTask?.cancel(); textLayer?.cancel(); renderTask = null; textLayer = null;
  await loading?.destroy(); loading = null; pdf = null;
  worker?.destroy(); worker = null; port?.terminate(); port = null;
  if (workerUrl) URL.revokeObjectURL(workerUrl);
  workerUrl = null; delete globalThis.pdfjsWorker;
  diag.activeWorkers = 0; diag.activeBlobUrls = 0; diag.pages = 0; diag.page = 0; diag.text = "";
  diag.engine = "none"; diag.document = null;
  $("page").hidden = true; $("empty").hidden = false; $("page-info").textContent = "0 / 0";
  $("page").querySelector("canvas").width = 0;
  $("page").querySelector("canvas").height = 0;
  $("page").querySelector(".textLayer").replaceChildren();
}
async function renderPage() {
  if (!pdf) return;
  const start = performance.now();
  const page = await pdf.getPage(pageNumber);
  if (fit) scale = Math.min(2, Math.max(0.15, ($("stage").clientWidth - 32) / page.getViewport({ scale: 1 }).width));
  const viewport = page.getViewport({ scale });
  // One visible page only. Cap backing pixels, including oversized PDF pages.
  const dpr = Math.min(devicePixelRatio || 1, 2, Math.sqrt(8_000_000 / (viewport.width * viewport.height)));
  const canvas = $("page").querySelector("canvas");
  canvas.width = Math.max(1, Math.floor(viewport.width * dpr)); canvas.height = Math.max(1, Math.floor(viewport.height * dpr));
  canvas.style.width = `${viewport.width}px`; canvas.style.height = `${viewport.height}px`;
  $("page").style.width = `${viewport.width}px`; $("page").style.height = `${viewport.height}px`;
  const layer = $("page").querySelector(".textLayer");
  layer.replaceChildren(); layer.style.setProperty("--scale-factor", scale);
  layer.style.setProperty("--total-scale-factor", scale);
  $("empty").hidden = true; $("page").hidden = false;
  renderTask = page.render({ canvas, canvasContext: canvas.getContext("2d"), viewport,
    transform: [dpr, 0, 0, dpr, 0, 0] });
  await renderTask.promise; renderTask = null;
  const text = await page.getTextContent();
  textLayer = new TextLayer({ textContentSource: text, container: layer, viewport });
  await textLayer.render();
  diag.text = text.items.map(item => item.str ?? "").join(" ");
  diag.page = pageNumber; diag.pages = pdf.numPages; diag.scale = scale;
  diag.renderMs = Math.round(performance.now() - start); diag.renders++;
  $("page-info").textContent = `${pageNumber} / ${pdf.numPages}`;
  $("fit").textContent = fit ? "适合宽度" : `${Math.round(scale * 100)}%`;
  status(`${diag.document} · ${pdf.numPages} 页 · ${diag.engine === "module-worker" ? "Worker 渲染" : "主线程回退"}`);
  diagnostic();
}
async function openBytes(bytes, name) {
  if (bytes.length === 0 || bytes.length > MAX_BYTES) throw new Error("文件大小须在 1 字节至 20 MiB 之间");
  await closeDocument(); diag.assetReads = []; delete diag.error;
  const start = performance.now();
  diag.document = name; status("正在加载 PDF…");
  try {
    await createWorker();
    loading = getDocument({ data: bytes, worker, isEvalSupported: false, enableXfa: false,
      useWasm: false, useWorkerFetch: false, BinaryDataFactory: InlineBinaryDataFactory,
      cMapPacked: true, cMapUrl: "inline/cmaps/", standardFontDataUrl: "inline/fonts/",
      useSystemFonts: false, maxImageSize: 16_000_000, canvasMaxAreaInBytes: 32_000_000 });
    pdf = await loading.promise; pageNumber = 1; fit = true; await renderPage();
    diag.firstLoadMs = Math.round(performance.now() - start);
  } catch (error) { await closeDocument(); throw error; }
}
async function openSample(result) {
  const file = result.structuredContent;
  if (result.isError || !file || !Number.isSafeInteger(file.totalBytes) || file.totalBytes <= 0 || file.totalBytes > MAX_BYTES)
    throw new Error("Host 未返回有效 PDF 元数据");
  const bytes = new Uint8Array(file.totalBytes);
  for (let offset = 0; offset < bytes.length;) {
    const response = await app.callServerTool({ name: "read_pdf_sample_bytes",
      arguments: { sample: file.sample, snapshot: file.snapshot, offset } });
    const chunk = response._meta?.pdfChunk;
    if (response.isError || !chunk || chunk.snapshot !== file.snapshot || chunk.offset !== offset || chunk.totalBytes !== bytes.length)
      throw new Error("PDF 分块身份或偏移不匹配");
    const data = decode(chunk.base64);
    if (!data.length || data.length > 256 * 1024 || offset + data.length > bytes.length) throw new Error("无效 PDF 分块大小");
    bytes.set(data, offset); offset += data.length;
  }
  await openBytes(bytes, file.name);
}
$("prev").onclick = () => enqueue(async () => { pageNumber--; await renderPage(); });
$("next").onclick = () => enqueue(async () => { pageNumber++; await renderPage(); });
$("minus").onclick = () => enqueue(async () => { fit = false; scale = Math.max(0.15, scale / 1.2); await renderPage(); });
$("plus").onclick = () => enqueue(async () => { fit = false; scale = Math.min(3, scale * 1.2); await renderPage(); });
$("fit").onclick = () => enqueue(async () => { fit = true; await renderPage(); });
$("close").onclick = () => enqueue(async () => { await closeDocument(); status("文档已关闭，Worker 与画布已释放。"); });
$("find").onclick = () => {
  const query = $("search").value.trim(); if (!query) return;
  const nodes = [...$("page").querySelectorAll(".textLayer span")];
  const match = nodes.find(node => node.textContent.toLocaleLowerCase().includes(query.toLocaleLowerCase()));
  if (!match) { status("本页没有匹配文字（扫描图片不含可搜索文本）。"); return; }
  const range = document.createRange(); range.selectNodeContents(match);
  const selection = getSelection(); selection.removeAllRanges(); selection.addRange(range); match.scrollIntoView({ block: "nearest" });
  status(`已选中本页匹配文字：${query}`);
};
$("file").onchange = () => {
  const file = $("file").files[0]; $("file").value = "";
  if (file) enqueue(async () => {
    if (file.size > MAX_BYTES) throw new Error("文件超过 20 MiB");
    await openBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  });
};
$("fullscreen").onclick = () => enqueue(async () => {
  await app.requestDisplayMode({ mode: app.getHostContext()?.displayMode === "fullscreen" ? "inline" : "fullscreen" });
});
function hostChanged(context) {
  if (context.theme) applyDocumentTheme(context.theme);
  if (context.styles?.variables) applyHostStyleVariables(context.styles.variables);
  if (context.displayMode !== undefined) {
    $("fullscreen").textContent = context.displayMode === "fullscreen" ? "收起" : "展开";
    document.documentElement.dataset.displayMode = context.displayMode;
  }
  clearTimeout(resizeTimer); resizeTimer = setTimeout(() => enqueue(renderPage, false), 120);
}
app.onhostcontextchanged = hostChanged;
app.ontoolresult = result => enqueue(() => openSample(result));
app.onteardown = async () => {
  disposed = true; clearTimeout(resizeTimer); observer.disconnect();
  await queue; await closeDocument(); diagnostic(); return {};
};
const observer = new ResizeObserver(() => {
  if (!pdf || !fit || disposed) return;
  clearTimeout(resizeTimer); resizeTimer = setTimeout(() => enqueue(renderPage, false), 120);
});
observer.observe($("stage"));
window.addEventListener("pagehide", () => {
  disposed = true; clearTimeout(resizeTimer); observer.disconnect();
  port?.terminate(); if (workerUrl) URL.revokeObjectURL(workerUrl);
});
// Read-only inspection for the local compatibility harness.
globalThis.pdfPocDiagnostics = () => structuredClone(diag);
app.connect().then(() => {
  diag.host = "connected"; hostChanged(app.getHostContext() ?? {}); controls(); diagnostic();
}).catch(error => { diag.host = "unavailable"; status(`Host 连接失败：${error.message}`); diagnostic(); });
diagnostic();
