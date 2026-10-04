import { getDocument, PDFWorker, TextLayer } from "pdfjs-dist";
import { readPdf } from "./pdf-reader.mjs";

const decode = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
async function unzip(value) {
  return new Uint8Array(await new Response(new Blob([decode(value)]).stream()
    .pipeThrough(new DecompressionStream("gzip"))).arrayBuffer());
}
class InlineBinaryDataFactory {
  async fetch({ kind, filename }) {
    const value = WEBCODEX_PDF_ASSETS[`${kind}/${filename}`];
    if (!value) throw new Error("PDF font resource unavailable");
    return unzip(value); // Fresh transferable bytes for each PDF.js request.
  }
}

// A single active row owns this renderer. The HTML controller owns RPC and
// snapshot authority; this module owns only the document, Worker and DOM.
export function createPreview(host, options) {
  let disposed = false, loading = null, pdf = null, worker = null, port = null, workerUrl = null;
  let renderTask = null, textLayer = null, pageNumber = 1, scale = 1, fit = true, resizeTimer;
  let queue = Promise.resolve(), busy = false, readyCancel = null, workerFailure = null;
  const current = () => !disposed && options.current();
  const element = (tag, className = "") => {
    const node = host.ownerDocument.createElement(tag); node.className = className; return node;
  };
  const toolbar = element("div", "pdf-toolbar"), status = element("p", "pdf-status");
  const navigation = element("div", "pdf-control-group"), zoom = element("div", "pdf-control-group"), searchGroup = element("div", "pdf-search-group");
  toolbar.append(navigation, zoom, searchGroup);
  status.setAttribute("role", "status");
  const stage = element("div", "pdf-stage"), page = element("div", "pdf-page");
  const canvas = element("canvas"), layer = element("div", "textLayer");
  canvas.setAttribute("aria-label", "PDF page");
  page.append(canvas, layer); stage.append(page); page.hidden = true;
  host.replaceChildren(toolbar, status, stage);
  const button = (label, action, group = toolbar) => {
    const node = element("button", "refresh"); node.type = "button"; node.textContent = label;
    node.onclick = () => enqueue(action); group.append(node); return node;
  };
  const prev = button("Previous", async () => { if (pdf && pageNumber > 1) { pageNumber--; await render(); } }, navigation);
  const info = element("span"); navigation.append(info);
  const next = button("Next", async () => { if (pdf && pageNumber < pdf.numPages) { pageNumber++; await render(); } }, navigation);
  const minus = button("−", async () => { fit = false; scale = Math.max(0.15, scale / 1.2); await render(); }, zoom);
  minus.setAttribute("aria-label", "Zoom out");
  const plus = button("+", async () => { fit = false; scale = Math.min(3, scale * 1.2); await render(); }, zoom);
  plus.setAttribute("aria-label", "Zoom in");
  const fitButton = button("Fit width", async () => { fit = true; await render(); }, zoom);
  fitButton.setAttribute("aria-label", "Fit page width");
  const search = element("input"); search.type = "search"; search.placeholder = "Find on page";
  search.setAttribute("aria-label", "Find text on current PDF page"); searchGroup.append(search);
  const find = button("Find", async () => {
    const query = search.value.trim().toLocaleLowerCase(); if (!query) return;
    const match = [...layer.querySelectorAll("span")].find(node => node.textContent.toLocaleLowerCase().includes(query));
    if (!match) { status.textContent = "No match on this page · scanned images may have no text"; return; }
    const range = host.ownerDocument.createRange(); range.selectNodeContents(match);
    const selection = host.ownerDocument.getSelection(); selection.removeAllRanges(); selection.addRange(range);
    match.scrollIntoView({ block: "nearest", inline: "nearest" }); status.textContent = "Matching text selected";
  }, searchGroup);
  search.onkeydown = event => { if (event.key === "Enter") { event.preventDefault(); find.onclick(); } };
  function controls() {
    for (const node of [prev, next, minus, plus, fitButton, find, search]) node.disabled = busy || !pdf || !current();
    prev.disabled ||= pageNumber <= 1; next.disabled ||= pageNumber >= (pdf?.numPages ?? 0);
    info.textContent = pdf ? `${pageNumber} / ${pdf.numPages}` : "0 / 0";
  }
  function enqueue(action) {
    queue = queue.then(async () => {
      if (!current()) return;
      busy = true; controls();
      try { await action(); } catch (error) {
        release(); if (current()) {
          options.failed?.();
          const reason = error.message.includes("Image exceeded maximum allowed size")
            ? "An embedded image exceeds the 16 million pixel limit; a complete preview is unavailable."
            : error.message;
          status.textContent = `PDF unavailable: ${reason}`;
        }
      }
      finally { busy = false; controls(); }
    });
    return queue;
  }
  function workerMessage(event) {
    // PDF.js can complete a streamed render after its operator-list stream errored.
    // Keep the actual Worker failure instead of accepting that partial page.
    const reason = event.data?.reason;
    if (current() && event.data?.stream && typeof reason?.message === "string")
      workerFailure = new Error(reason.message);
  }
  async function createWorker() {
    const source = await unzip(WEBCODEX_PDF_WORKER);
    if (!current()) throw new Error("PDF preview closed");
    workerUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
    port = new Worker(workerUrl, { type: "module", name: "webcodex-pdf" });
    port.addEventListener("message", workerMessage);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => finish(new Error("PDF Worker unavailable")), 3000);
      const message = event => { if (event.data?.webcodexPdfReady) finish(); };
      const error = () => finish(new Error("PDF Worker blocked by the Host"));
      function finish(reason) {
        clearTimeout(timer); port?.removeEventListener("message", message); port?.removeEventListener("error", error);
        readyCancel = null; reason ? reject(reason) : resolve();
      }
      readyCancel = () => finish(new Error("PDF preview closed"));
      port.addEventListener("message", message); port.addEventListener("error", error);
    });
    if (!current()) throw new Error("PDF preview closed");
    worker = new PDFWorker({ port }); await worker.promise;
  }
  async function render() {
    if (!current() || !pdf) return;
    const pageProxy = await pdf.getPage(pageNumber); if (!current()) return;
    if (fit) scale = Math.min(2, Math.max(0.15, (stage.clientWidth - 24) / pageProxy.getViewport({ scale: 1 }).width));
    const viewport = pageProxy.getViewport({ scale });
    if (!Number.isFinite(viewport.width) || !Number.isFinite(viewport.height) || viewport.width <= 0 || viewport.height <= 0)
      throw new Error("Invalid PDF page size");
    const dpr = Math.min(devicePixelRatio || 1, 2, Math.sqrt(8_000_000 / (viewport.width * viewport.height)));
    canvas.width = Math.max(1, Math.floor(viewport.width * dpr)); canvas.height = Math.max(1, Math.floor(viewport.height * dpr));
    canvas.style.width = page.style.width = `${viewport.width}px`; canvas.style.height = page.style.height = `${viewport.height}px`;
    textLayer?.cancel(); layer.replaceChildren();
    layer.style.setProperty("--scale-factor", scale); layer.style.setProperty("--total-scale-factor", scale);
    page.hidden = false;
    renderTask = pageProxy.render({ canvas, canvasContext: canvas.getContext("2d"), viewport, transform: [dpr, 0, 0, dpr, 0, 0] });
    await renderTask.promise; renderTask = null; if (!current()) return;
    if (workerFailure) throw workerFailure;
    const text = await pageProxy.getTextContent(); if (!current()) return;
    textLayer = new TextLayer({ textContentSource: text, container: layer, viewport }); await textLayer.render();
    if (!current()) return;
    if (workerFailure) throw workerFailure;
    fitButton.textContent = fit ? "Fit width" : `${Math.round(scale * 100)}%`;
    status.textContent = `${options.sourceLabel || "PDF"} · ${pdf.numPages} ${pdf.numPages === 1 ? "page" : "pages"} · ${Math.round(scale * 100)}%`;
    controls();
  }
  let observedWidth = null;
  const observer = new ResizeObserver(entries => {
    const width = entries[0]?.contentRect.width;
    if (width === observedWidth) return;
    observedWidth = width;
    if (!current() || !pdf || !fit) return;
    clearTimeout(resizeTimer); resizeTimer = setTimeout(() => enqueue(render), 120);
  });
  observer.observe(stage);
  // Terminate immediately even while an async read/render is outstanding. Late
  // promises are fenced before they can recreate a Worker or mutate the DOM.
  function release() {
    readyCancel?.(); renderTask?.cancel(); textLayer?.cancel();
    renderTask = null; textLayer = null;
    const task = loading; loading = null; pdf = null;
    const oldWorker = worker, oldPort = port, oldUrl = workerUrl;
    worker = port = workerUrl = null;
    oldPort?.removeEventListener("message", workerMessage); workerFailure = null;
    let finished = false, timer;
    const finish = () => {
      if (finished) return; finished = true; clearTimeout(timer);
      oldWorker?.destroy(); oldPort?.terminate(); if (oldUrl) URL.revokeObjectURL(oldUrl);
    };
    // Give PDF.js its Terminate acknowledgment before terminating the port;
    // bound cleanup if the Worker is blocked or unresponsive.
    if (task) { timer = setTimeout(finish, 1000); void task.destroy().catch(() => {}).finally(finish); }
    else finish();
    canvas.width = canvas.height = 0; page.hidden = true; layer.replaceChildren();
  }
  controls();
  return {
    load: () => enqueue(async () => {
      try {
        const bytes = await (options.readDocument || readPdf)({ ...options, current, progress: (loaded, total) => {
          if (current()) status.textContent = `Loading PDF · ${loaded} / ${total} bytes`;
        } });
        await createWorker(); if (!current()) return;
        // Fail instead of presenting a successful page with oversized images removed.
        loading = getDocument({ data: bytes, worker, stopAtErrors: true, isEvalSupported: false, enableXfa: false,
          useWasm: false, useWorkerFetch: false, BinaryDataFactory: InlineBinaryDataFactory,
          cMapPacked: true, cMapUrl: "inline/cmaps/", standardFontDataUrl: "inline/fonts/",
          useSystemFonts: false, maxImageSize: 16_000_000, canvasMaxAreaInBytes: 32_000_000 });
        pdf = await loading.promise; if (!current()) return;
        await render();
      } catch (error) {
        release(); if (current()) { options.failed?.(); throw error; }
      }
    }),
    destroy() {
      if (disposed) return;
      disposed = true; clearTimeout(resizeTimer); observer.disconnect(); release(); controls();
    },
  };
}
