import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { JSDOM } from 'jsdom';

function source(name, lineEnding) {
  let value = readFileSync(new URL(`../src/mcp-apps/work-result/${name}.mjs`, import.meta.url), 'utf8');
  if (lineEnding) value = value.replace(/\r?\n/g, lineEnding);
  return value.replace(/^import .*;\r?\n/gm, '').replace('export function createPreview', 'function createPreview');
}
const flush = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

// Exercise the actual preview and Host lifecycle. Only PDF.js, Worker and layout
// boundaries are substituted so failures/races are deterministic without Chrome.
function reader(options = {}) {
  const dom = new JSDOM('<div id="filename"></div><button id="retry" hidden></button><p id="empty"></p><div id="reader"></div>');
  const { document, HTMLElement, HTMLCanvasElement } = dom.window;
  const host = document.getElementById('reader');
  const timers = new Map(), listeners = new Map(), workers = [], documents = [], observers = [];
  let nextStream = 0;
  let nextTimer = 0, width = 800;
  const timer = (callback, delay) => { timers.set(++nextTimer, { callback, delay }); return nextTimer; };
  const viewport = () => host.querySelector('.pdf-viewer-container');
  Object.defineProperty(HTMLElement.prototype, 'clientWidth', { get: () => width });
  HTMLElement.prototype.getBoundingClientRect = function () {
    if (!this.classList.contains('page')) return { top: 0, left: 0, right: width, bottom: 700, width, height: 700 };
    let top = -viewport().scrollTop;
    for (const sibling of this.parentElement.children) { if (sibling === this) break; top += parseFloat(sibling.style.height) + 16; }
    const height = parseFloat(this.style.height), pageWidth = parseFloat(this.style.width);
    return { top, left: 0, right: pageWidth, bottom: top + height, width: pageWidth, height };
  };
  HTMLElement.prototype.scrollIntoView = function () { viewport().scrollTop += this.getBoundingClientRect().top; };
  HTMLCanvasElement.prototype.getContext = () => ({});
  class Observer {
    constructor(callback, settings) { this.callback = callback; this.settings = settings; this.nodes = new Set(); observers.push(this); }
    observe(node) { this.nodes.add(node); }
    unobserve(node) { this.nodes.delete(node); }
    disconnect() { this.nodes.clear(); }
  }
  class Worker {
    constructor() { this.listeners = new Map(); workers.push(this); queueMicrotask(() => this.emit({ webcodexPdfReady: true })); }
    addEventListener(name, callback) { if (!this.listeners.has(name)) this.listeners.set(name, new Set()); this.listeners.get(name).add(callback); }
    removeEventListener(name, callback) { this.listeners.get(name)?.delete(callback); }
    postMessage(message) { this.lastStream = message; }
    fail(message, stream = this.lastStream) { this.emit({ stream: 5, streamId: stream.streamId, targetName: stream.sourceName, reason: { message } }); }
    emit(data) { for (const callback of this.listeners.get('message') || []) callback({ data }); }
    terminate() { this.terminated = true; }
  }
  class TextLayer {
    constructor({ textContentSource, container }) { this.text = textContentSource; this.container = container; }
    render() {
      const span = document.createElement('span'); span.textContent = `page ${this.text.number}`; this.container.append(span);
      return options.textLayer?.(this.text, this) || Promise.resolve();
    }
    cancel() { this.cancelled = true; }
    static cleanup() {}
  }
  const getDocument = () => {
    const doc = { index: documents.length, pages: [], numPages: options.numPages || 3,
      async getPage(number) {
        const page = { number, doc, renders: 0, cleanups: 0,
          getViewport: ({ scale }) => ({ width: 400 * scale, height: 600 * scale }),
          render({ canvas }) {
            page.renders++;
            if (!options.reuseOperatorStream || doc.pages.filter(item => item.number === number).length === 1) workers.at(-1).postMessage({ action: 'GetOperatorList', sourceName: `doc${doc.index}`, streamId: ++nextStream, data: { pageIndex: number - 1 } });
            const pending = options.paint?.(page, canvas);
            const task = { promise: pending?.promise || Promise.resolve(), cancel() { task.cancelled = true; } };
            page.task = task; return task;
          },
          async getTextContent() { return options.textContent?.(page) || { number, doc }; },
          cleanup() { page.cleanups++; },
        };
        doc.pages.push(page); return page;
      },
    };
    documents.push(doc);
    return { promise: Promise.resolve(doc), async destroy() { doc.destroyed = true; } };
  };
  const context = {
    document, Worker, TextLayer, getDocument,
    PDFWorker: class { promise = Promise.resolve(); destroy() {} },
    IntersectionObserver: Observer, ResizeObserver: Observer, devicePixelRatio: 1,
    setTimeout: timer, clearTimeout: id => timers.delete(id),
    requestAnimationFrame: callback => timer(callback, 16), cancelAnimationFrame: id => timers.delete(id),
    Blob: class { stream() { return { pipeThrough() {} }; } },
    Response: class { async arrayBuffer() { return new ArrayBuffer(0); } },
    DecompressionStream: class {}, Uint8Array, atob, AbortController,
    URL: { createObjectURL: () => 'blob:fixture', revokeObjectURL() {} },
    WEBCODEX_PDF_WORKER: '', WEBCODEX_PDF_ASSETS: {},
    readPdfDocument: async () => new Uint8Array(),
    addEventListener: (name, callback) => listeners.set(name, callback),
  };
  const parent = { postMessage(message) {
    if (message.method === 'ui/initialize') queueMicrotask(() => deliver({ id: message.id, result: {} }));
  } };
  context.parent = parent;
  const deliver = data => listeners.get('message')({ source: parent, data: { jsonrpc: '2.0', ...data } });
  const previewSource = source('pdf-continuous-preview', options.sourceLineEnding);
  const documentSource = source('pdf-document', options.sourceLineEnding);
  runInNewContext(`(() => { ${previewSource}\nglobalThis.createPreview = createPreview; })();\n${documentSource}`, context);
  return {
    dom, document, host, timers, workers, documents, observers,
    status: () => host.querySelector('.pdf-status')?.textContent,
    page: number => host.querySelector(`[data-page-number="${number}"]`),
    async present(name = 'first') {
      deliver({ method: 'ui/notifications/tool-result', params: { success: true, output: { pdf_document: {
        project: 'agent:pdf:test', path: `${name}.pdf`, name: `${name}.pdf`, bytes: 5, sha256: 'a'.repeat(64),
      } } } }); await flush();
    },
    async click(label) {
      const button = [...document.querySelectorAll('button')].find(node => node.textContent === label || node.getAttribute('aria-label') === label);
      assert.ok(button, `Missing button: ${label}`); button.click(); await flush();
    },
    async retry() { document.getElementById('retry').click(); await flush(); },
    async resize(value) {
      width = value; observers.find(observer => !observer.settings).callback();
      for (const [id, entry] of [...timers]) if (entry.delay === 120) { timers.delete(id); entry.callback(); }
      await flush();
    },
    async near(...numbers) {
      observers.find(observer => observer.settings?.rootMargin).callback(numbers.map(number => ({ target: this.page(number), isIntersecting: true })));
      await flush();
    },
    async close() { deliver({ method: 'ui/resource-teardown', id: 'teardown' }); await flush(); },
  };
}
const failure = 'PDF unavailable: page paint failed';
const rejectedPaint = () => ({ promise: Promise.reject(new Error('page paint failed')) });

for (const [name, sourceLineEnding] of [['LF', '\n'], ['CRLF', '\r\n']]) {
  test(`reader lifecycle loads ${name} source files`, async () => {
    const view = reader({ sourceLineEnding });
    try {
      await view.present();
      assert.match(view.status(), /^PDF · 3 pages/);
      assert.ok(view.page(1).querySelector('canvas').width > 0);
    } finally { await view.close(); view.dom.window.close(); }
  });
}

for (const action of ['Next', 'Zoom in', 'resize', 'completion']) {
  test(`page render failure survives ${action} without hiding Retry`, async () => {
    const view = reader({ paint: page => page.number === 1 && page.doc.pages.filter(item => item.number === 1).length === 1 ? rejectedPaint() : undefined });
    await view.present();
    assert.equal(view.status(), failure);
    if (action === 'resize') await view.resize(600);
    else if (action === 'completion') await view.near(1, 2);
    else await view.click(action);
    assert.equal(view.status(), failure);
    assert.equal(view.document.getElementById('retry').hidden, false);
    await view.close(); view.dom.window.close();
  });
}

test('failed pages release partial canvas/proxy and do not automatically retry on navigation or rescale', async () => {
  const view = reader({ paint: page => page.number === 1 ? rejectedPaint() : undefined });
  await view.present();
  const canvas = view.page(1).querySelector('canvas'), failedPage = view.documents[0].pages[0];
  assert.equal(canvas.width, 0); assert.equal(canvas.height, 0);
  assert.equal(failedPage.cleanups, 1);
  await view.near(1, 2); await view.click('Next'); await view.click('Previous');
  await view.click('Zoom in'); await view.resize(650);
  assert.equal(view.documents[0].pages.filter(page => page.number === 1).length, 1);
  assert.ok(view.documents[0].pages.some(page => page.number === 2 && page.renders === 1), 'Valid queued pages still render');
  await view.close(); view.dom.window.close();
});

for (const stage of ['textContent', 'textLayer']) {
  test(`${stage} failures clear all partial output`, async () => {
    const view = reader({ [stage]: () => Promise.reject(new Error('text failed')) });
    await view.present();
    assert.equal(view.status(), 'PDF unavailable: text failed');
    assert.equal(view.page(1).querySelector('canvas').width, 0);
    assert.equal(view.page(1).querySelector('.textLayer').children.length, 0);
    assert.equal(view.documents[0].pages[0].cleanups, 1);
    await view.close(); view.dom.window.close();
  });
}

test('stale render failure after zoom cannot clear the new canvas or poison the current generation', async () => {
  const old = deferred();
  const view = reader({ paint: page => page.doc.pages.length === 1 ? old : undefined });
  await view.present(); const original = view.documents[0].pages[0];
  await view.click('Zoom in');
  old.reject(new Error('stale render error')); await flush();
  assert.equal(original.task.cancelled, true); assert.equal(original.cleanups, 1);
  assert.match(view.status(), /^PDF · 3 pages/);
  assert.ok(view.page(1).querySelector('canvas').width > 0);
  assert.equal(view.document.getElementById('retry').hidden, true);
  await view.close(); view.dom.window.close();
});

for (const action of ['retry', 'replacement', 'teardown']) {
  test(`${action} fences late page errors and releases old resources`, async () => {
    const old = deferred();
    const view = reader({ paint: page => page.doc.index === 0 ? old : undefined });
    await view.present(); const original = view.documents[0].pages[0], oldWorker = view.workers[0];
    const oldCanvas = view.page(1).querySelector('canvas');
    if (action === 'retry') await view.retry();
    else if (action === 'replacement') await view.present('second');
    else await view.close();
    const before = view.status();
    old.reject(new Error('late old document failure')); oldWorker.emit({ stream: 'error', reason: { message: 'late worker error' } });
    await flush();
    assert.equal(view.status(), before);
    assert.equal(oldCanvas.width, 0); assert.equal(original.cleanups, 1);
    assert.equal(oldWorker.terminated, true); assert.equal(view.documents[0].destroyed, true);
    assert.equal(view.document.getElementById('retry').hidden, true);
    if (action !== 'teardown') {
      assert.match(view.status(), /^PDF · 3 pages/);
      assert.ok(view.page(1).querySelector('canvas').width > 0);
      await view.close();
    }
    assert.equal(view.host.querySelectorAll('canvas').length, 0); assert.equal(view.timers.size, 0);
    view.dom.window.close();
  });
}

test('explicit Retry and new source clear a recorded render failure and paint again', async () => {
  for (const action of ['retry', 'replacement']) {
    const view = reader({ paint: page => page.doc.index === 0 ? rejectedPaint() : undefined });
    await view.present(); assert.equal(view.status(), failure);
    if (action === 'retry') await view.retry(); else await view.present('new');
    assert.match(view.status(), /^PDF · 3 pages/);
    assert.equal(view.document.getElementById('retry').hidden, true);
    assert.equal(view.documents[0].destroyed, true);
    assert.ok(view.page(1).querySelector('canvas').width > 0);
    await view.close(); view.dom.window.close();
  }
});

test('Worker stream failure clears only its active page while valid queued pages still render', async () => {
  const paint = deferred();
  const view = reader({ paint: page => page.number === 1 ? paint : undefined });
  await view.present(); await view.near(1, 2);
  view.workers[0].fail('Image exceeded maximum allowed size');
  paint.resolve(); await flush();
  assert.match(view.status(), /16 million pixel limit/);
  assert.equal(view.page(1).querySelector('canvas').width, 0);
  assert.ok(view.page(2).querySelector('canvas').width > 0, 'Unrelated page must remain renderable');
  assert.equal(view.page(2).querySelector('.textLayer').textContent, 'page 2');
  await view.close(); view.dom.window.close();
});

test('Worker errors from a cancelled generation cannot poison a newer zoom render', async () => {
  const old = deferred();
  const view = reader({ paint: page => page.doc.pages.length === 1 ? old : undefined });
  await view.present(); await view.click('Zoom in');
  view.workers[0].fail('stale Worker failure');
  old.reject(new Error('stale render failure')); await flush();
  assert.match(view.status(), /^PDF · 3 pages/);
  assert.equal(view.document.getElementById('retry').hidden, true);
  assert.ok(view.page(1).querySelector('canvas').width > 0);
  await view.close(); view.dom.window.close();
});


test('a late stream error cannot be attributed to a different active page', async () => {
  const old = deferred(), next = deferred();
  const view = reader({ paint: page => page.number === 1 ? old : next });
  await view.present(); const oldStream = view.workers[0].lastStream;
  await view.click('Next');
  old.resolve(); await flush();
  view.workers[0].fail('late previous page stream', oldStream);
  next.resolve(); await flush();
  assert.match(view.status(), /^PDF · 3 pages/);
  assert.equal(view.document.getElementById('retry').hidden, true);
  assert.ok(view.page(2).querySelector('canvas').width > 0);
  await view.close(); view.dom.window.close();
});

for (const reuseOperatorStream of [false, true]) {
  test(`zoom ${reuseOperatorStream ? 'retains a reused operator stream identity' : 'rejects the replaced operator stream identity'}`, async () => {
    const old = deferred(), latest = deferred();
    const view = reader({ reuseOperatorStream, paint: page => page.doc.pages.length === 1 ? old : latest });
    await view.present(); const oldStream = view.workers[0].lastStream;
    await view.click('Zoom in'); old.resolve(); await flush();
    view.workers[0].fail('operator stream failed', oldStream);
    latest.resolve(); await flush();
    if (reuseOperatorStream) {
      assert.equal(view.status(), 'PDF unavailable: operator stream failed');
      assert.equal(view.page(1).querySelector('canvas').width, 0);
    } else {
      assert.match(view.status(), /^PDF · 3 pages/);
      assert.equal(view.document.getElementById('retry').hidden, true);
      assert.ok(view.page(1).querySelector('canvas').width > 0);
    }
    await view.close(); view.dom.window.close();
  });
}

test('long documents still load lazily and preserve selected page/relative position across zoom and resize', async () => {
  const view = reader({ numPages: 32 }); await view.present();
  assert.equal(view.documents[0].pages.length, 1);
  await view.near(1, 2, 3, 4, 5, 6, 7);
  assert.ok(view.documents[0].pages.length < 32);
  for (let number = 2; number <= 8; number++) await view.click('Next');
  const viewport = view.host.querySelector('.pdf-viewer-container');
  const selected = view.page(8);
  viewport.scrollTop += selected.getBoundingClientRect().height / 4;
  for (const action of [() => view.click('Zoom in'), () => view.resize(600)]) {
    await action();
    assert.equal(view.host.querySelector('.pdf-control-group span').textContent, '8 / 32');
    const bounds = selected.getBoundingClientRect();
    assert.ok(Math.abs(-bounds.top / bounds.height - 0.25) < 0.001);
    const canvases = [...view.host.querySelectorAll('canvas')].filter(canvas => canvas.width);
    assert.ok(canvases.length <= 6);
    assert.ok(canvases.reduce((sum, canvas) => sum + canvas.width * canvas.height, 0) <= 24_000_000);
  }
  await view.close(); view.dom.window.close();
});

test('real PDF.js accepts the render-failure fixture and emits an error for only its oversized-image page', async () => {
  const { getDocument, PDFWorker } = await import('pdfjs-dist/legacy/build/pdf.mjs');
  const { pdfSamples } = await import('../../scripts/ui-smoke/fixtures/pdf-samples.mjs');
  const worker = new PDFWorker(); await worker.promise;
  const failures = [];
  worker.port.addEventListener('message', ({ data }) => {
    if (data.stream && data.reason?.message) failures.push(data.reason.message);
  });
  const loading = getDocument({ worker, data: new Uint8Array(pdfSamples.renderFailure),
    stopAtErrors: true, maxImageSize: 16_000_000, useSystemFonts: true });
  try {
    const pdf = await loading.promise;
    assert.equal(pdf.numPages, 3, 'Failure is after successful document parsing');
    assert.ok((await (await pdf.getPage(1)).getOperatorList()).fnArray.length > 0);
    assert.deepEqual(failures, []);
    await (await pdf.getPage(2)).getOperatorList();
    assert.deepEqual(failures, ['Image exceeded maximum allowed size and was removed.']);
    assert.ok((await (await pdf.getPage(3)).getOperatorList()).fnArray.length > 0);
    assert.equal(failures.length, 1, 'Other pages remain valid');
  } finally { await loading.destroy(); worker.destroy(); }
});

test('an active Worker failure clears partial pixels immediately and stays failed across a rapid zoom', async () => {
  const paint = deferred();
  const view = reader({ paint: () => paint }); await view.present();
  view.workers[0].fail('operator failed');
  assert.equal(view.page(1).querySelector('canvas').width, 0);
  await view.click('Zoom in'); paint.resolve(); await flush();
  assert.equal(view.documents[0].pages.length, 1, 'Zoom must not retry the failed page');
  assert.equal(view.status(), 'PDF unavailable: operator failed');
  await view.close(); view.dom.window.close();
});
