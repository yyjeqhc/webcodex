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
    return unzip(value);
  }
}

// Continuous dedicated reader built on the host-proven PDF.js core renderer.
// Only pages near the viewport are painted; the full pdf_viewer.mjs application
// bundle is deliberately avoided because live ChatGPT MCP App dogfood failed
// before that variant reached artifact transport.
export function createPreview(host, options) {
  let disposed = false, loading = null, pdf = null, worker = null, port = null, workerUrl = null;
  let readyCancel = null, workerFailure = null, scale = 1, fit = true, currentPage = 1;
  let resizeTimer = null, renderGeneration = 0;
  const pageStates = new Map();
  const current = () => !disposed && options.current();
  const element = (tag, className = "") => {
    const node = host.ownerDocument.createElement(tag);
    node.className = className;
    return node;
  };

  const toolbar = element("div", "pdf-toolbar"), status = element("p", "pdf-status");
  const navigation = element("div", "pdf-control-group"), zoom = element("div", "pdf-control-group"),
    searchGroup = element("div", "pdf-search-group");
  toolbar.append(navigation, zoom, searchGroup);
  status.setAttribute("role", "status");
  const stage = element("div", "pdf-stage"), viewport = element("div", "pdf-viewer-container"),
    pages = element("div", "pdfViewer");
  viewport.append(pages); stage.append(viewport); host.replaceChildren(toolbar, status, stage);

  const button = (label, action, group = toolbar) => {
    const node = element("button", "refresh");
    node.type = "button"; node.textContent = label; node.onclick = () => void action();
    group.append(node); return node;
  };
  const prev = button("Previous", () => jump(currentPage - 1), navigation);
  const info = element("span"); navigation.append(info);
  const next = button("Next", () => jump(currentPage + 1), navigation);
  const minus = button("−", async () => {
    if (!pdf) return;
    fit = false; scale = Math.max(0.15, scale / 1.2); await rescale();
  }, zoom);
  minus.setAttribute("aria-label", "Zoom out");
  const plus = button("+", async () => {
    if (!pdf) return;
    fit = false; scale = Math.min(3, scale * 1.2); await rescale();
  }, zoom);
  plus.setAttribute("aria-label", "Zoom in");
  const fitButton = button("Fit width", async () => {
    if (!pdf) return;
    fit = true; await rescale();
  }, zoom);
  fitButton.setAttribute("aria-label", "Fit page width");

  const search = element("input");
  search.type = "search"; search.placeholder = "Find on page";
  search.setAttribute("aria-label", "Find text on current PDF page");
  searchGroup.append(search);
  const find = button("Find", () => {
    const query = search.value.trim().toLocaleLowerCase();
    if (!query) return;
    const state = pageStates.get(currentPage);
    const match = [...(state?.layer?.querySelectorAll("span") || [])]
      .find(node => node.textContent.toLocaleLowerCase().includes(query));
    if (!match) {
      status.textContent = state?.rendered
        ? "No match on this page · scanned images may have no text"
        : "Current page text is still loading";
      return;
    }
    const range = host.ownerDocument.createRange();
    range.selectNodeContents(match);
    const selection = host.ownerDocument.getSelection();
    selection.removeAllRanges(); selection.addRange(range);
    match.scrollIntoView({ block: "nearest", inline: "nearest" });
    status.textContent = "Matching text selected";
  }, searchGroup);
  search.onkeydown = event => {
    if (event.key === "Enter") { event.preventDefault(); find.onclick(); }
  };

  function controls() {
    const ready = !!pdf && current();
    for (const node of [prev, next, minus, plus, fitButton, find, search]) node.disabled = !ready;
    prev.disabled ||= currentPage <= 1;
    next.disabled ||= currentPage >= (pdf?.numPages ?? 0);
    info.textContent = ready ? `${currentPage} / ${pdf.numPages}` : "0 / 0";
    fitButton.textContent = ready && !fit ? `${Math.round(scale * 100)}%` : "Fit width";
  }
  function summary() {
    if (!pdf) return;
    status.textContent = `${options.sourceLabel || "PDF"} · ${pdf.numPages} ${pdf.numPages === 1 ? "page" : "pages"} · ${Math.round(scale * 100)}%`;
  }
  function failure(error) {
    if (!current()) return;
    options.failed?.();
    const reason = String(error?.message || error).includes("Image exceeded maximum allowed size")
      ? "An embedded image exceeds the 16 million pixel limit; a complete preview is unavailable."
      : String(error?.message || error);
    status.textContent = `PDF unavailable: ${reason}`;
  }
  function workerMessage(event) {
    const reason = event.data?.reason;
    if (current() && event.data?.stream && typeof reason?.message === "string") {
      workerFailure = new Error(reason.message);
      failure(workerFailure);
    }
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
        clearTimeout(timer);
        port?.removeEventListener("message", message);
        port?.removeEventListener("error", error);
        readyCancel = null;
        reason ? reject(reason) : resolve();
      }
      readyCancel = () => finish(new Error("PDF preview closed"));
      port.addEventListener("message", message); port.addEventListener("error", error);
    });
    if (!current()) throw new Error("PDF preview closed");
    worker = new PDFWorker({ port });
    await worker.promise;
  }

  function fitScale(pageProxy) {
    const base = pageProxy.getViewport({ scale: 1 });
    return Math.min(2, Math.max(0.15, (viewport.clientWidth - 24) / base.width));
  }
  async function applyPageGeometry(state) {
    if (!state.pageProxy) state.pageProxy = await pdf.getPage(state.number);
    if (!current()) return;
    const targetScale = fit ? fitScale(state.pageProxy) : scale;
    if (fit && state.number === 1) scale = targetScale;
    const view = state.pageProxy.getViewport({ scale: fit ? scale : targetScale });
    if (!Number.isFinite(view.width) || !Number.isFinite(view.height) || view.width <= 0 || view.height <= 0)
      throw new Error("Invalid PDF page size");
    state.node.style.width = `${view.width}px`;
    state.node.style.height = `${view.height}px`;
    state.viewport = view;
  }
  async function renderPage(state, generation = renderGeneration) {
    if (!current() || !pdf || state.rendering || (state.rendered && state.renderedScale === scale)) return;
    state.rendering = true;
    try {
      await applyPageGeometry(state);
      if (!current() || generation !== renderGeneration) return;
      const view = state.viewport;
      const dpr = Math.min(devicePixelRatio || 1, 2, Math.sqrt(8_000_000 / (view.width * view.height)));
      state.canvas.width = Math.max(1, Math.floor(view.width * dpr));
      state.canvas.height = Math.max(1, Math.floor(view.height * dpr));
      state.canvas.style.width = `${view.width}px`;
      state.canvas.style.height = `${view.height}px`;
      state.layer.replaceChildren();
      state.layer.style.setProperty("--scale-factor", scale);
      state.layer.style.setProperty("--total-scale-factor", scale);
      state.renderTask = state.pageProxy.render({
        canvas: state.canvas,
        canvasContext: state.canvas.getContext("2d"),
        viewport: view,
        transform: [dpr, 0, 0, dpr, 0, 0],
      });
      await state.renderTask.promise;
      state.renderTask = null;
      if (!current() || generation !== renderGeneration) return;
      if (workerFailure) throw workerFailure;
      const text = await state.pageProxy.getTextContent();
      if (!current() || generation !== renderGeneration) return;
      state.textLayer = new TextLayer({ textContentSource: text, container: state.layer, viewport: view });
      await state.textLayer.render();
      if (!current() || generation !== renderGeneration) return;
      if (workerFailure) throw workerFailure;
      state.rendered = true; state.renderedScale = scale;
      summary(); controls();
    } catch (error) {
      if (error?.name !== "RenderingCancelledException") failure(error);
    } finally {
      state.rendering = false;
    }
  }

  const pageObserver = new IntersectionObserver(entries => {
    if (!current() || !pdf) return;
    let best = null;
    for (const entry of entries) {
      const number = Number(entry.target.dataset.pageNumber);
      const state = pageStates.get(number);
      if (!state) continue;
      state.visible = entry.isIntersecting;
      if (entry.isIntersecting) {
        void renderPage(state);
        for (const adjacent of [number - 1, number + 1]) {
          const neighbor = pageStates.get(adjacent);
          if (neighbor) void renderPage(neighbor);
        }
      }
      if (!best || entry.intersectionRatio > best.ratio) best = { number, ratio: entry.intersectionRatio };
    }
    if (best?.ratio > 0) {
      currentPage = best.number;
      controls(); summary();
    }
  }, { root: viewport, rootMargin: "900px 0px", threshold: [0, 0.1, 0.5, 0.9] });

  async function buildPages() {
    pages.replaceChildren(); pageStates.clear();
    for (let number = 1; number <= pdf.numPages; number++) {
      if (!current()) return;
      const node = element("div", "page"), wrapper = element("div", "canvasWrapper"),
        canvas = element("canvas"), layer = element("div", "textLayer");
      node.dataset.pageNumber = String(number);
      canvas.setAttribute("aria-label", `PDF page ${number}`);
      wrapper.append(canvas); node.append(wrapper, layer); pages.append(node);
      const state = { number, node, canvas, layer, pageProxy: null, viewport: null,
        renderTask: null, textLayer: null, rendering: false, rendered: false, renderedScale: null, visible: false };
      pageStates.set(number, state);
      await applyPageGeometry(state);
      pageObserver.observe(node);
    }
    currentPage = 1; controls(); summary();
    const first = pageStates.get(1);
    if (first) await renderPage(first);
  }

  async function rescale() {
    if (!pdf || !current()) return;
    renderGeneration++;
    const generation = renderGeneration;
    for (const state of pageStates.values()) {
      state.renderTask?.cancel(); state.textLayer?.cancel();
      state.renderTask = null; state.textLayer = null; state.rendering = false;
      state.rendered = false; state.renderedScale = null;
      state.canvas.width = state.canvas.height = 0; state.layer.replaceChildren();
    }
    for (const state of pageStates.values()) await applyPageGeometry(state);
    for (const state of pageStates.values()) {
      if (state.visible || Math.abs(state.number - currentPage) <= 1) void renderPage(state, generation);
    }
    summary(); controls();
  }
  function jump(number) {
    if (!pdf) return;
    const clamped = Math.max(1, Math.min(pdf.numPages, number));
    pageStates.get(clamped)?.node.scrollIntoView({ block: "start", behavior: "smooth" });
    currentPage = clamped; controls(); summary();
  }

  let observedWidth = null;
  const resizeObserver = new ResizeObserver(entries => {
    const width = entries[0]?.contentRect.width;
    if (width === observedWidth) return;
    observedWidth = width;
    if (!current() || !pdf || !fit) return;
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => { void rescale(); }, 120);
  });
  resizeObserver.observe(viewport);

  function release() {
    readyCancel?.(); readyCancel = null;
    renderGeneration++;
    for (const state of pageStates.values()) {
      pageObserver.unobserve(state.node);
      state.renderTask?.cancel(); state.textLayer?.cancel();
      state.canvas.width = state.canvas.height = 0;
    }
    pageStates.clear(); pages.replaceChildren();
    const task = loading; loading = null; pdf = null;
    const oldWorker = worker, oldPort = port, oldUrl = workerUrl;
    worker = port = workerUrl = null;
    oldPort?.removeEventListener("message", workerMessage); workerFailure = null;
    let finished = false, timer;
    const finish = () => {
      if (finished) return;
      finished = true; clearTimeout(timer);
      oldWorker?.destroy(); oldPort?.terminate();
      if (oldUrl) URL.revokeObjectURL(oldUrl);
    };
    if (task) {
      timer = setTimeout(finish, 1000);
      void task.destroy().catch(() => {}).finally(finish);
    } else finish();
  }

  controls();
  return {
    load: async () => {
      try {
        const bytes = await (options.readDocument || readPdf)({
          ...options,
          current,
          progress: (loaded, total) => {
            if (current()) status.textContent = `Loading PDF · ${loaded} / ${total} bytes`;
          },
        });
        await createWorker();
        if (!current()) return;
        loading = getDocument({
          data: bytes, worker, stopAtErrors: true, isEvalSupported: false, enableXfa: false,
          useWasm: false, useWorkerFetch: false, BinaryDataFactory: InlineBinaryDataFactory,
          cMapPacked: true, cMapUrl: "inline/cmaps/", standardFontDataUrl: "inline/fonts/",
          useSystemFonts: false, maxImageSize: 16_000_000, canvasMaxAreaInBytes: 32_000_000,
        });
        pdf = await loading.promise;
        if (!current()) return;
        await buildPages();
      } catch (error) {
        release();
        if (current()) failure(error);
      }
    },
    destroy() {
      if (disposed) return;
      disposed = true; clearTimeout(resizeTimer);
      resizeObserver.disconnect(); pageObserver.disconnect();
      release(); controls();
    },
  };
}
