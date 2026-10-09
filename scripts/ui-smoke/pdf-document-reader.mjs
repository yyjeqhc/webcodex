import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { pdfSamples } from './fixtures/pdf-samples.mjs';

// Real shipped HTML/Worker/PDF.js, with an isolated private MCP Host carrier.
// No Server, Runner, credentials or external document requests are involved.
const args = process.argv.slice(2);
assert.ok(!args.length || (args.length === 2 && args[0] === '--html'), 'Only --html PATH is supported');
const html = await readFile(args[1] || new URL('../../src/mcp_pdf_app.html', import.meta.url), 'utf8');
const output = new URL(args[1] ? '../../artifacts/pdf-reader/baseline/' : '../../artifacts/pdf-reader/', import.meta.url);
await mkdir(output, { recursive: true });
const report = { fixture: true, nativeBackend: false, browser: null, cases: [] };
const server = createServer((request, response) => {
  response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  response.end('<!doctype html><style>body{margin:0}iframe{display:block;width:100%;height:100vh;border:0}</style>');
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
let browser;
try {
  browser = await chromium.launch({ headless: true,
    ...(process.env.WEBCODEX_SMOKE_BROWSER ? { executablePath: process.env.WEBCODEX_SMOKE_BROWSER } : {}) });
  report.browser = browser.version();

  async function mount(name, { width = 800, theme = 'light', hold = false, changed = false, dpr = 1 } = {}) {
    const page = await browser.newPage({ viewport: { width, height: 720 }, deviceScaleFactor: dpr, reducedMotion: 'reduce' });
    page.setDefaultTimeout(15_000);
    const errors = [], externalRequests = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.route('**/*', route => {
      if (new URL(route.request().url()).hostname !== '127.0.0.1') {
        externalRequests.push(route.request().url()); return route.abort();
      }
      return route.continue();
    });
    await page.addInitScript(() => {
      // Track artifact-request timeouts while keeping real browser timing.
      const nativeSetTimeout = window.setTimeout.bind(window), nativeClearTimeout = window.clearTimeout.bind(window);
      window.pdfHostTimers = new Set();
      window.setTimeout = (callback, delay, ...args) => {
        const id = nativeSetTimeout(() => { pdfHostTimers.delete(id); callback.apply(window, args); }, delay);
        if (delay > 60_000) pdfHostTimers.add(id);
        return id;
      };
      window.clearTimeout = id => { pdfHostTimers.delete(id); nativeClearTimeout(id); };
      const NativeWorker = Worker;
      window.pdfWorkerStats = { live: 0, created: 0, getPages: [] };
      window.Worker = class extends NativeWorker {
        constructor(...args) { super(...args); this.stopped = false; pdfWorkerStats.live++; pdfWorkerStats.created++; }
        postMessage(message, ...args) {
          if (message?.action === 'GetPage') pdfWorkerStats.getPages.push(message.data.pageIndex + 1);
          return super.postMessage(message, ...args);
        }
        terminate() { if (!this.stopped) { this.stopped = true; pdfWorkerStats.live--; } return super.terminate(); }
      };
    });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const selected = sample => ({ project: 'agent:pdf-smoke:demo', path: `${sample}.pdf`, name: `${sample}.pdf`,
      bytes: pdfSamples[sample].length, sha256: createHash('sha256').update(pdfSamples[sample]).digest('hex') });
    const samples = Object.fromEntries(Object.entries(pdfSamples).map(([sample, bytes]) => [sample, {
      identity: selected(sample), encoded: bytes.toString('base64'),
    }]));
    await page.evaluate(({ html, samples, name, theme, hold, changed }) => {
      window.fixture = { reads: [], errors: [], hold, changed, held: [], teardownAck: false };
      const frame = document.createElement('iframe');
      frame.setAttribute('sandbox', 'allow-scripts allow-same-origin');
      let sample = samples[name];
      const reply = (id, result) => frame.contentWindow.postMessage({ jsonrpc: '2.0', id, result }, '*');
      const deliver = () => frame.contentWindow.postMessage({ jsonrpc: '2.0', method: 'ui/notifications/tool-result',
        params: { tool_result: { structuredContent: { success: true, output: { pdf_document: sample.identity } } } } }, '*');
      window.deliverPdf = (next = name) => { sample = samples[next]; deliver(); };
      window.closePdf = () => frame.contentWindow.postMessage({ jsonrpc: '2.0', id: 9000, method: 'ui/resource-teardown', params: {} }, '*');
      addEventListener('message', event => {
        if (event.source !== frame.contentWindow || event.data?.jsonrpc !== '2.0') return;
        const message = event.data;
        if (message.id === 9000 && message.result) { fixture.teardownAck = true; return; }
        if (message.method === 'ui/initialize') {
          reply(message.id, { protocolVersion: '2026-01-26', hostContext: { theme, displayMode: 'fullscreen' } });
        } else if (message.method === 'ui/notifications/initialized') deliver();
        else if (message.method === 'tools/call') {
          const args = message.params.arguments;
          fixture.reads.push(args);
          if (message.params.name !== 'read_app_artifact_chunk'
            || ['project', 'path', 'sha256', 'bytes'].some(key => args[key] !== sample.identity[key])) {
            fixture.errors.push('Unpinned or unexpected artifact request');
            return reply(message.id, { structuredContent: { success: false } });
          }
          const requestedSample = sample;
          const finish = () => {
            if (fixture.changed) return reply(message.id, { structuredContent: { success: false, output: { error_kind: 'snapshot_changed' } } });
            const bytes = Uint8Array.from(atob(requestedSample.encoded), char => char.charCodeAt(0));
            const next = Math.min(args.byte_offset + 512 * 1024, bytes.length);
            const encoded = btoa(String.fromCharCode(...bytes.subarray(args.byte_offset, next)));
            reply(message.id, { tool_result: { structuredContent: { success: true, output: { artifact_chunk: {
              project: args.project, path: args.path, sha256: args.sha256, bytes_total: bytes.length,
              byte_offset: args.byte_offset, next_byte_offset: next === bytes.length ? null : next, complete: next === bytes.length,
            } } }, _meta: { 'webcodex/artifactChunk': { content_base64: encoded } } } });
          };
          fixture.hold ? fixture.held.push(finish) : finish();
        }
      });
      frame.srcdoc = html; document.body.append(frame);
    }, { html, samples, name, theme, hold, changed });
    await page.locator('iframe').waitFor();
    const frame = page.frames().find(frame => frame.parentFrame());
    assert.ok(frame);
    return { page, frame, errors, externalRequests };
  }
  async function painted(frame, number, text = true) {
    await frame.waitForFunction(({ number, text }) => {
      const node = document.querySelector(`[data-page-number="${number}"]`), canvas = node?.querySelector('canvas');
      if (!canvas?.width || (text && !node.querySelector('.textLayer span'))) return false;
      const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
      for (let i = 0; i < pixels.length; i += 4) if (pixels[i + 3] && pixels[i] < 240 && pixels[i + 1] < 240) return true;
      return false;
    }, { number, text });
  }
  async function jump(frame, number, text = true) {
    await frame.locator(`[data-page-number="${number}"]`).evaluate(node => node.scrollIntoView({ block: 'start', behavior: 'instant' }));
    await painted(frame, number, text);
    await frame.waitForFunction(number => document.querySelector('.pdf-control-group span').textContent.startsWith(`${number} /`), number);
  }
  async function cache(frame) {
    return frame.evaluate(() => {
      const canvases = [...document.querySelectorAll('.canvasWrapper canvas')].filter(canvas => canvas.width && canvas.height);
      return { pages: canvases.length, pixels: canvases.reduce((sum, canvas) => sum + canvas.width * canvas.height, 0) };
    });
  }
  function bounded(value) { assert.ok(value.pages <= 6, `Cached ${value.pages} pages`); assert.ok(value.pixels <= 24_000_000, `Cached ${value.pixels} pixels`); }
  async function close({ page, frame }) {
    await page.evaluate(() => closePdf());
    await page.waitForFunction(() => fixture.teardownAck);
    await frame.waitForFunction(() => pdfWorkerStats.live === 0 && !document.querySelector('canvas'));
    assert.equal(await frame.evaluate(() => pdfHostTimers.size), 0, 'Teardown retained a Host request timer');
  }
  async function run(label, operation) {
    const result = { name: label, passed: false };
    report.cases.push(result);
    try { Object.assign(result, await operation()); result.passed = true; console.log(`PASS ${label}`); }
    catch (error) { result.error = error.message; throw error; }
  }
  async function verified(mounted, screenshot) {
    assert.deepEqual(mounted.errors, [], 'Browser exceptions');
    assert.deepEqual(mounted.externalRequests, [], 'PDF renderer attempted an external fetch');
    assert.deepEqual(await mounted.page.evaluate(() => fixture.errors), [], 'Host carrier identity mismatch');
    if (screenshot) await mounted.page.screenshot({ path: fileURLToPath(new URL(`${screenshot}.png`, output)) });
  }

  for (const width of [800, 390]) await run(`mixed sizes, rotation, text alignment, zoom and resize (${width}px)`, async () => {
    const mounted = await mount('mixed', { width });
    const { page, frame } = mounted;
    try {
      await painted(frame, 1);
      await frame.waitForFunction(() => document.querySelector('.pdf-control-group span').textContent === '1 / 3');
      for (const number of [1, 2, 3]) {
        if (number > 1) await frame.getByRole('button', { name: 'Next', exact: true }).click();
        await painted(frame, number);
        await frame.waitForFunction(number => document.querySelector('.pdf-control-group span').textContent === `${number} / 3`, number);
        const geometry = await frame.locator(`[data-page-number="${number}"]`).evaluate(node => {
          const rect = node.getBoundingClientRect(), viewport = document.querySelector('.pdf-viewer-container');
          const text = node.querySelector('.textLayer span').getBoundingClientRect();
          return { width: rect.width, height: rect.height, available: viewport.clientWidth, textLeft: text.left - rect.left,
            textInside: text.left >= rect.left - 1 && text.top >= rect.top - 1 && text.right <= rect.right + 1 && text.bottom <= rect.bottom + 1 };
        });
        assert.ok(geometry.width <= geometry.available, `Page ${number} overflows fit width: ${JSON.stringify(geometry)}`);
        assert.ok(geometry.textInside, `Page ${number} text layer is outside its canvas`);
        if (number !== 3) assert.ok(Math.abs(geometry.textLeft - 40 * geometry.width / (number === 1 ? 400 : 900)) < 1,
          `Page ${number} text layer does not match the PDF text position`);
        if (number !== 1) assert.ok(geometry.width > geometry.height, 'Landscape/rotated page lost its orientation');
        bounded(await cache(frame));
      }
      // Rapid requests must not let a cancelled render overwrite the new canvas.
      await frame.evaluate(() => {
        for (let i = 0; i < 4; i++) document.querySelector('[aria-label="Zoom in"]').click();
        for (let i = 0; i < 3; i++) document.querySelector('[aria-label="Zoom out"]').click();
        document.querySelector('[aria-label="Fit page width"]').click();
      });
      await painted(frame, 3);
      await page.setViewportSize({ width: width === 800 ? 500 : 460, height: 720 });
      await frame.waitForFunction(() => {
        const node = document.querySelector('[data-page-number="3"]');
        const available = document.querySelector('.pdf-viewer-container').clientWidth - 24;
        return Math.abs(parseFloat(node.style.width) - available) < 2;
      });
      await painted(frame, 3); bounded(await cache(frame));
      await verified(mounted, `mixed-${width}`); await close(mounted);
      return { pages: 3 };
    } finally { await page.close(); }
  });

  await run('embedded Chinese glyphs, text selection and search', async () => {
    const mounted = await mount('chinese'); const { page, frame } = mounted;
    try {
      await painted(frame, 1);
      assert.ok((await frame.locator('.textLayer').textContent()).includes('中文'));
      const blackPixels = await frame.locator('.canvasWrapper canvas').evaluate(canvas => {
        const scale = canvas.width / 400, ctx = canvas.getContext('2d');
        const data = ctx.getImageData(Math.floor(40 * scale), Math.floor(95 * scale), Math.floor(65 * scale), Math.floor(50 * scale)).data;
        let count = 0;
        for (let i = 0; i < data.length; i += 4) if (data[i + 3] && data[i] < 80 && data[i + 1] < 80 && data[i + 2] < 80) count++;
        return count;
      });
      assert.ok(blackPixels > 200, 'Chinese text extracted but glyphs were not painted');
      await frame.getByRole('searchbox').fill('中文'); await frame.getByRole('button', { name: 'Find', exact: true }).click();
      assert.ok((await frame.evaluate(() => getSelection().toString())).includes('中文'));
      await verified(mounted, 'chinese'); await close(mounted);
      return { glyphPixels: blackPixels };
    } finally { await page.close(); }
  });

  await run('scan image pixels and empty text layer (dark theme)', async () => {
    const mounted = await mount('scan', { theme: 'dark' }); const { page, frame } = mounted;
    try {
      await painted(frame, 1, false);
      assert.equal(await frame.locator('.textLayer span').count(), 0);
      const pixels = await frame.locator('.canvasWrapper canvas').evaluate(canvas => {
        const s = canvas.width / 400, ctx = canvas.getContext('2d');
        return [[150, 250], [250, 250], [150, 350], [250, 350]].map(([x, y]) => [...ctx.getImageData(Math.floor(x * s), Math.floor(y * s), 1, 1).data].slice(0, 3));
      });
      assert.deepEqual(pixels, [[220, 40, 40], [40, 60, 220], [40, 180, 60], [220, 180, 40]]);
      await frame.getByRole('searchbox').fill('anything'); await frame.getByRole('button', { name: 'Find', exact: true }).click();
      await frame.getByRole('status').filter({ hasText: 'No match on this page' }).waitFor();
      await verified(mounted, 'scan-dark'); await close(mounted);
      return { sampledPixels: pixels };
    } finally { await page.close(); }
  });

  await run('long document lazy loading, bounded cache and scroll back', async () => {
    const mounted = await mount('long'); const { page, frame } = mounted;
    try {
      await painted(frame, 1);
      const initial = await frame.evaluate(() => pdfWorkerStats.getPages);
      assert.ok(initial.length < 10, `Fetched ${initial.length} page proxies before first display`);
      const measurements = [];
      for (const number of [1, 8, 16, 24, 32, 1]) {
        await jump(frame, number); const measurement = await cache(frame); bounded(measurement); measurements.push({ number, ...measurement });
      }
      assert.equal(await frame.locator('[data-page-number="16"] canvas').evaluate(canvas => canvas.width), 0);
      const before = await page.evaluate(() => fixture.reads.length);
      await page.evaluate(() => deliverPdf());
      await frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.evaluate(() => fixture.reads.length), before, 'Repeated delivery restarted the transfer');
      await page.evaluate(() => deliverPdf('chinese')); await painted(frame, 1);
      await frame.waitForFunction(() => document.querySelector('.textLayer').textContent.includes('中文') && pdfWorkerStats.live === 1);
      await verified(mounted, 'long-reopened'); await close(mounted);
      return { initiallyFetchedPages: initial, cache: measurements };
    } finally { await page.close(); }
  });

  await run('reading position survives zoom, fit width and resize', async () => {
    const mounted = await mount('long'); const { page, frame } = mounted;
    try {
      await painted(frame, 1); await jump(frame, 16);
      await frame.locator('[data-page-number="16"]').evaluate(node => {
        document.querySelector('.pdf-viewer-container').scrollTop += node.getBoundingClientRect().height / 4;
      });
      const position = () => frame.evaluate(() => {
        const viewport = document.querySelector('.pdf-viewer-container').getBoundingClientRect();
        const rect = document.querySelector('[data-page-number="16"]').getBoundingClientRect();
        return { current: document.querySelector('.pdf-control-group span').textContent,
          offset: (viewport.top - rect.top) / rect.height, height: rect.height };
      });
      const settle = () => frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      await settle(); const initial = await position(); assert.equal(initial.current, '16 / 32');
      const measurements = [];
      for (const action of ['Zoom in', 500, 'Zoom out', 'Fit page width', 900, 390]) {
        const before = await position();
        const fit = await frame.getByRole('button', { name: 'Fit page width', exact: true }).textContent() === 'Fit width';
        if (typeof action === 'number') await page.setViewportSize({ width: action, height: 720 });
        else await frame.getByRole('button', { name: action, exact: true }).click();
        await frame.waitForFunction(({ height, action, fit, offset }) => {
          const node = document.querySelector('[data-page-number="16"]');
          if (typeof action === 'number') {
            const viewport = document.querySelector('.pdf-viewer-container'), rect = node.getBoundingClientRect();
            if (fit) return Math.abs(rect.width - Math.min(800, viewport.clientWidth - 24)) < 2;
            return Math.abs(rect.height - height) < 2
              && Math.abs(viewport.getBoundingClientRect().top - rect.top - offset * rect.height) < 2;
          }
          return Math.abs(node.getBoundingClientRect().height - height) > 2 || action === 'Fit page width';
        }, { height: before.height, action, fit, offset: initial.offset });
        await settle(); const after = await position();
        assert.equal(after.current, '16 / 32', `${action} changed the current page`);
        assert.ok(Math.abs(after.offset - initial.offset) * after.height < 2,
          `${action} moved the reading position: ${JSON.stringify({ initial, after })}`);
        await painted(frame, 16); bounded(await cache(frame));
        measurements.push({ action, ...after });
      }
      await verified(mounted); await close(mounted); return { measurements };
    } finally { await page.close(); }
  });

  await run('total canvas pixel budget at DPR 2', async () => {
    const mounted = await mount('wide', { dpr: 2 }); const { page, frame } = mounted;
    try {
      await painted(frame, 1);
      // Six prefetched pages would need 24.3M pixels. Wait for their requests so
      // this exercises the total budget, rather than only the first canvas.
      await frame.waitForFunction(() => pdfWorkerStats.getPages.length >= 6);
      await painted(frame, 6);
      const measurement = await cache(frame); bounded(measurement);
      assert.ok(measurement.pixels >= 16_000_000, 'Stress fixture did not exercise cache eviction');
      await verified(mounted); await close(mounted);
      return { dpr: 2, ...measurement };
    } finally { await page.close(); }
  });

  await run('page render failure remains visible through navigation, zoom, resize and recovery', async () => {
    const mounted = await mount('renderFailure'); const { page, frame } = mounted;
    try {
      await painted(frame, 1);
      const status = frame.locator('.pdf-status');
      await status.filter({ hasText: '16 million pixel limit' }).waitFor();
      const failure = await status.textContent();
      const partial = frame.locator('[data-page-number="2"]');
      await frame.waitForFunction(() => document.querySelector('[data-page-number="2"] canvas')?.width === 0);
      assert.equal(await partial.locator('.textLayer span').count(), 0);
      await frame.getByRole('button', { name: 'Next', exact: true }).click();
      assert.equal(await status.textContent(), failure);
      await frame.getByRole('button', { name: 'Next', exact: true }).click();
      await painted(frame, 3);
      assert.equal(await status.textContent(), failure);
      await frame.getByRole('button', { name: 'Zoom in', exact: true }).click();
      await painted(frame, 3); assert.equal(await status.textContent(), failure);
      await frame.evaluate(() => { window.beforeFailureResizeText = document.querySelector('[data-page-number="3"] .textLayer span'); });
      await page.setViewportSize({ width: 600, height: 720 });
      await frame.waitForFunction(() => !window.beforeFailureResizeText.isConnected);
      await painted(frame, 3); assert.equal(await status.textContent(), failure);
      await frame.getByRole('button', { name: 'Retry preview' }).click();
      await frame.waitForFunction(() => pdfWorkerStats.created === 2);
      await painted(frame, 1);
      await status.filter({ hasText: '16 million pixel limit' }).waitFor();
      await page.evaluate(() => deliverPdf('chinese'));
      await painted(frame, 1);
      await frame.waitForFunction(() => document.querySelector('.textLayer').textContent.includes('中文'));
      assert.ok(!(await status.textContent()).includes('unavailable'));
      assert.equal(await frame.getByRole('button', { name: 'Retry preview' }).isVisible(), false);
      bounded(await cache(frame)); await verified(mounted, 'render-failure-recovered');
      await close(mounted); return { pages: 3 };
    } finally { await page.close(); }
  });

  for (const [name, options, reason] of [
    ['malformed', {}, 'PDF unavailable'], ['mixed', { changed: true }, 'version changed'],
  ]) await run(`actionable ${name === 'mixed' ? 'changed version' : 'invalid PDF'} failure`, async () => {
    const mounted = await mount(name, options); const { page, frame } = mounted;
    try {
      await frame.getByRole('status').filter({ hasText: reason }).waitFor();
      await frame.getByRole('button', { name: 'Retry preview' }).waitFor();
      if (options.changed) assert.equal(await frame.evaluate(() => pdfWorkerStats.created), 0);
      await verified(mounted); await close(mounted); return {};
    } finally { await page.close(); }
  });

  for (const replacement of [true, false]) await run(`${replacement ? 'replacement' : 'invalid selection'} cancels a pending transfer`, async () => {
    const mounted = await mount('mixed', { hold: true }); const { page, frame } = mounted;
    try {
      await page.waitForFunction(() => fixture.held.length === 1);
      assert.equal(await frame.evaluate(() => pdfHostTimers.size), 1);
      await page.evaluate(() => deliverPdf());
      await frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.evaluate(() => fixture.reads.length), 1, 'Repeated delivery restarted a pending transfer');
      if (replacement) {
        await page.evaluate(() => { fixture.hold = false; deliverPdf('chinese'); });
        await frame.waitForFunction(() => document.querySelector('.textLayer')?.textContent.includes('中文'));
        await painted(frame, 1);
      } else {
        await page.evaluate(() => document.querySelector('iframe').contentWindow.postMessage({ jsonrpc: '2.0',
          method: 'ui/notifications/tool-result', params: { structuredContent: { success: false } } }, '*'));
        await frame.getByRole('status').filter({ hasText: 'present_pdf did not succeed' }).waitFor();
      }
      assert.equal(await frame.evaluate(() => pdfHostTimers.size), 0, 'Old transfer retained its Host timeout');
      await page.evaluate(() => fixture.held.forEach(finish => finish()));
      await frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await frame.evaluate(() => pdfWorkerStats.created), replacement ? 1 : 0, 'Late bytes created an old Worker');
      if (replacement) assert.equal(await frame.locator('#filename').textContent(), 'chinese.pdf');
      await verified(mounted); await close(mounted); return {};
    } finally { await page.close(); }
  });

  await run('teardown during a pending transfer rejects late bytes', async () => {
    const mounted = await mount('mixed', { hold: true }); const { page, frame } = mounted;
    try {
      await page.waitForFunction(() => fixture.held.length === 1);
      await close(mounted);
      await page.evaluate(() => fixture.held.forEach(finish => finish()));
      await frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await frame.evaluate(() => pdfWorkerStats.created), 0);
      await verified(mounted); return {};
    } finally { await page.close(); }
  });
} finally {
  await writeFile(new URL('report.json', output), JSON.stringify(report, null, 2) + '\n');
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
