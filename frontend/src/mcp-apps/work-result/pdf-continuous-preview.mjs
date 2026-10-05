import { getDocument, PDFWorker, TextLayer } from "pdfjs-dist";
import { readPdf } from "./pdf-reader.mjs";

const MAX_CACHED_PAGES = 6;
const MAX_CACHED_PIXELS = 24_000_000;
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
  let resizeTimer = null, renderGeneration = 0, rendering = false, scrollFrame = null;
  let defaultGeometry = null;
  let layoutWidth = null, readingPosition = null;
  const pageStates = new Map();
  const cachedPages = new Set(), nearPages = new Set(), visiblePages = new Set(), renderQueue = new Set();
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
    if (fit) scale = pageStates.get(currentPage)?.scale ?? scale;
    fit = false; scale = Math.max(0.15, scale / 1.2); await rescale();
  }, zoom);
  minus.setAttribute("aria-label", "Zoom out");
  const plus = button("+", async () => {
    if (!pdf) return;
    if (fit) scale = pageStates.get(currentPage)?.scale ?? scale;
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
    const pageScale = pageStates.get(currentPage)?.scale ?? scale;
    status.textContent = `${options.sourceLabel || "PDF"} · ${pdf.numPages} ${pdf.numPages === 1 ? "page" : "pages"} · ${Math.round(pageScale * 100)}%`;
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

  function applyPageGeometry(state) {
    const base = state.geometry ?? defaultGeometry;
    state.scale = fit ? Math.min(2, Math.max(0.15, (viewport.clientWidth - 24) / base.width)) : scale;
    const view = state.pageProxy?.getViewport({ scale: state.scale })
      ?? { width: base.width * state.scale, height: base.height * state.scale };
    if (!Number.isFinite(view.width) || !Number.isFinite(view.height) || view.width <= 0 || view.height <= 0)
      throw new Error("Invalid PDF page size");
    state.node.style.width = `${view.width}px`;
    state.node.style.height = `${view.height}px`;
    state.viewport = view;
  }

  function clearPage(state) {
    state.revision++;
    const proxy = state.pageProxy, paint = state.renderTask, text = state.textPromise;
    state.renderTask?.cancel(); state.textLayer?.cancel();
    state.renderTask = state.textLayer = state.textPromise = state.pageProxy = null;
    state.canvas.width = state.canvas.height = 0; state.layer.replaceChildren();
    state.rendered = false; state.renderedScale = null;
    cachedPages.delete(state);
    if (paint || text) {
      void Promise.allSettled([paint?.promise, text]).then(() => proxy?.cleanup());
    } else proxy?.cleanup();
  }
  function reserveCanvas(state, pixels) {
    cachedPages.add(state);
    const totalPixels = () => [...cachedPages].reduce((total, page) => total
      + (page === state ? pixels : page.canvas.width * page.canvas.height), 0);
    while (cachedPages.size > MAX_CACHED_PAGES || totalPixels() > MAX_CACHED_PIXELS) {
      const candidates = [...cachedPages].filter(page => page !== state);
      candidates.sort((a, b) => Number(a.number === currentPage) - Number(b.number === currentPage)
        || Math.abs(b.number - currentPage) - Math.abs(a.number - currentPage));
      clearPage(candidates[0]);
    }
  }
  async function renderPage(state, generation = renderGeneration) {
    if (!current() || !pdf || (state.rendered && state.renderedScale === state.scale)) return;
    const revision = state.revision;
    const active = () => current() && generation === renderGeneration && revision === state.revision;
    try {
      if (!state.pageProxy) {
        const proxy = await pdf.getPage(state.number);
        if (!active()) { proxy.cleanup(); return; }
        state.pageProxy = proxy;
        const base = proxy.getViewport({ scale: 1 });
        state.geometry = { width: base.width, height: base.height };
      }
      applyPageGeometry(state);
      if (!active()) return;
      const view = state.viewport;
      const dpr = Math.min(devicePixelRatio || 1, 2, Math.sqrt(8_000_000 / (view.width * view.height)));
      const width = Math.max(1, Math.floor(view.width * dpr)), height = Math.max(1, Math.floor(view.height * dpr));
      reserveCanvas(state, width * height);
      state.canvas.width = width; state.canvas.height = height;
      state.canvas.style.width = `${view.width}px`;
      state.canvas.style.height = `${view.height}px`;
      state.layer.replaceChildren();
      state.layer.style.setProperty("--scale-factor", state.scale);
      state.layer.style.setProperty("--total-scale-factor", state.scale);
      state.renderTask = state.pageProxy.render({
        canvas: state.canvas,
        canvasContext: state.canvas.getContext("2d"),
        viewport: view,
        transform: [dpr, 0, 0, dpr, 0, 0],
      });
      await state.renderTask.promise;
      if (!active()) return;
      state.renderTask = null;
      if (workerFailure) throw workerFailure;
      const text = await state.pageProxy.getTextContent();
      if (!active()) return;
      state.textLayer = new TextLayer({ textContentSource: text, container: state.layer, viewport: view });
      state.textPromise = state.textLayer.render();
      await state.textPromise;
      if (!active()) return;
      state.textPromise = null;
      if (workerFailure) throw workerFailure;
      state.rendered = true; state.renderedScale = state.scale;
      summary(); controls();
    } catch (error) {
      if (active() && error?.name !== "RenderingCancelledException") failure(error);
    }
  }

  async function drainRenderQueue() {
    if (rendering || !current()) return;
    rendering = true;
    try {
      while (current() && pdf && renderQueue.size) {
        const state = [...renderQueue].sort((a, b) => Math.abs(a.number - currentPage)
          - Math.abs(b.number - currentPage))[0];
        renderQueue.delete(state);
        await renderPage(state);
      }
    } finally { rendering = false; }
  }
  function schedulePages() {
    if (!current() || !pdf) return;
    renderQueue.clear();
    const selected = pageStates.get(currentPage);
    const bounds = viewport.getBoundingClientRect();
    const candidates = [...new Set([selected, ...nearPages])].filter(Boolean)
      .filter(state => {
        const rect = state.node.getBoundingClientRect();
        return state === selected || (rect.bottom > bounds.top - 900 && rect.top < bounds.bottom + 900);
      })
      .sort((a, b) => Math.abs(a.number - currentPage) - Math.abs(b.number - currentPage));
    const wanted = new Set(candidates.slice(0, MAX_CACHED_PAGES));
    // Fast jumps can skip Observer transitions. Retention uses current geometry too.
    for (const state of cachedPages) if (!wanted.has(state)) clearPage(state);
    for (const state of wanted) {
      if (!state.rendered) renderQueue.add(state);
    }
    void drainRenderQueue();
  }
  function updateCurrentPage() {
    if (!current() || !pdf || viewport.clientWidth !== layoutWidth) return;
    const bounds = viewport.getBoundingClientRect();
    let best = null, fullyVisibleCurrent = false;
    for (const state of visiblePages) {
      const rect = state.node.getBoundingClientRect();
      const area = Math.max(0, Math.min(rect.bottom, bounds.bottom) - Math.max(rect.top, bounds.top))
        * Math.max(0, Math.min(rect.right, bounds.right) - Math.max(rect.left, bounds.left));
      if (state.number === currentPage && area > 0 && rect.top >= bounds.top - 1 && rect.bottom <= bounds.bottom + 1)
        fullyVisibleCurrent = true;
      if (area > 0 && (!best || area > best.area || (area === best.area && state.number < best.state.number)))
        best = { state, area };
    }
    // Preserve Next/Previous on a short page when another page also fits below it.
    if (!fullyVisibleCurrent && best && currentPage !== best.state.number) {
      currentPage = best.state.number;
      controls(); summary(); schedulePages();
    }
    rememberReadingPosition();
  }
  const visibleObserver = new IntersectionObserver(entries => {
    if (!current() || !pdf) return;
    for (const entry of entries) {
      const state = pageStates.get(Number(entry.target.dataset.pageNumber));
      if (!state) continue;
      entry.isIntersecting ? visiblePages.add(state) : visiblePages.delete(state);
    }
    updateCurrentPage(); schedulePages();
  }, { root: viewport, threshold: [0, 0.1, 0.5, 0.9] });
  const pageObserver = new IntersectionObserver(entries => {
    if (!current() || !pdf) return;
    for (const entry of entries) {
      const number = Number(entry.target.dataset.pageNumber);
      const state = pageStates.get(number);
      if (!state) continue;
      entry.isIntersecting ? nearPages.add(state) : nearPages.delete(state);
      if (!entry.isIntersecting && number !== currentPage) clearPage(state);
    }
    schedulePages();
  }, { root: viewport, rootMargin: "900px 0px", threshold: [0, 0.1, 0.5, 0.9] });

  function onScroll() {
    if (scrollFrame === null) scrollFrame = requestAnimationFrame(() => {
      scrollFrame = null; updateCurrentPage();
    });
  }
  viewport.addEventListener("scroll", onScroll, { passive: true });

  async function buildPages() {
    const firstProxy = await pdf.getPage(1);
    if (!current()) { firstProxy.cleanup(); return; }
    const firstView = firstProxy.getViewport({ scale: 1 });
    defaultGeometry = { width: firstView.width, height: firstView.height };
    layoutWidth = viewport.clientWidth;
    pages.replaceChildren(); pageStates.clear();
    for (let number = 1; number <= pdf.numPages; number++) {
      if (!current()) return;
      const node = element("div", "page"), wrapper = element("div", "canvasWrapper"),
        canvas = element("canvas"), layer = element("div", "textLayer");
      canvas.width = canvas.height = 0;
      node.dataset.pageNumber = String(number);
      canvas.setAttribute("aria-label", `PDF page ${number}`);
      wrapper.append(canvas); node.append(wrapper, layer); pages.append(node);
      const state = { number, node, canvas, layer, pageProxy: number === 1 ? firstProxy : null, viewport: null,
        geometry: number === 1 ? defaultGeometry : null, scale: 1, revision: 0,
        renderTask: null, textLayer: null, textPromise: null, rendered: false, renderedScale: null };
      pageStates.set(number, state);
      applyPageGeometry(state);
      pageObserver.observe(node); visibleObserver.observe(node);
    }
    currentPage = 1; rememberReadingPosition(); controls(); summary();
    schedulePages();
  }

  function rememberReadingPosition() {
    // Width changes can already have changed CSS page margins before the observer runs.
    if (viewport.clientWidth !== layoutWidth) return readingPosition;
    const selected = pageStates.get(currentPage);
    if (selected) {
      const rect = selected.node.getBoundingClientRect();
      readingPosition = { selected, offset: (viewport.getBoundingClientRect().top - rect.top) / rect.height };
    }
    return readingPosition;
  }
  async function rescale() {
    if (!pdf || !current()) return;
    const position = rememberReadingPosition();
    renderGeneration++;
    for (const state of pageStates.values()) {
      clearPage(state); applyPageGeometry(state);
    }
    // Keep the current page and its relative reading position as page heights change.
    layoutWidth = viewport.clientWidth;
    if (position) {
      const rect = position.selected.node.getBoundingClientRect();
      viewport.scrollTop += rect.top - viewport.getBoundingClientRect().top + position.offset * rect.height;
      currentPage = position.selected.number;
    }
    rememberReadingPosition();
    schedulePages(); summary(); controls();
  }
  function jump(number) {
    if (!pdf) return;
    const clamped = Math.max(1, Math.min(pdf.numPages, number));
    pageStates.get(clamped)?.node.scrollIntoView({ block: "start", behavior: "smooth" });
    currentPage = clamped; rememberReadingPosition(); controls(); summary(); schedulePages();
  }

  const resizeObserver = new ResizeObserver(() => {
    if (viewport.clientWidth === layoutWidth || !current() || !pdf) return;
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => { void rescale(); }, 120);
  });
  resizeObserver.observe(viewport);

  function release() {
    readyCancel?.(); readyCancel = null;
    renderGeneration++;
    for (const state of pageStates.values()) {
      pageObserver.unobserve(state.node); visibleObserver.unobserve(state.node);
      clearPage(state);
    }
    nearPages.clear(); visiblePages.clear(); renderQueue.clear(); cachedPages.clear();
    pageStates.clear(); pages.replaceChildren();
    layoutWidth = readingPosition = null;
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
      TextLayer.cleanup();
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
      if (scrollFrame !== null) cancelAnimationFrame(scrollFrame);
      viewport.removeEventListener("scroll", onScroll);
      resizeObserver.disconnect(); pageObserver.disconnect(); visibleObserver.disconnect();
      release(); controls();
    },
  };
}
