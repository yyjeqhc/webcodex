import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { createHash } from 'node:crypto';

const html = readFileSync(new URL('../../src/mcp_spreadsheet_app.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const flush = () => new Promise(resolve => setImmediate(resolve));
const bytes = Buffer.from('value\n0007');
const sha256 = createHash('sha256').update(bytes).digest('hex');
const uri = name => 'webcodex-artifact://export/wc_export_' + name;
const result = (name, sha = sha256) => {
  const value = { success: true, output: { project: 'agent:exporter:demo', path: name + '.csv', name: 'data.csv', bytes: bytes.length, sha256: sha } };
  return { structuredContent: value, content: [{ type: 'resource_link', uri: uri(name) }], _meta: { 'webcodex/spreadsheetSource': value } };
};
function segment(request, payload, patch = {}) {
  const identity = request.params.arguments, offset = identity.byte_offset;
  const next = Math.min(offset + 512 * 1024, identity.bytes);
  return { page: { project: identity.project, path: identity.path, sha256: identity.sha256,
    bytes_total: identity.bytes, byte_offset: offset, next_byte_offset: next === identity.bytes ? null : next,
    complete: next === identity.bytes }, encoded: payload.subarray(offset, next).toString('base64'), ...patch };
}
function segmentEnvelope(chunk, textOnly = false) {
  const value = { success: true, output: { artifact_chunk: chunk.page } };
  return { ...(textOnly ? { content: [{ type: 'text', text: JSON.stringify(value) }] } : { structuredContent: value }),
    _meta: { 'webcodex/artifactChunk': { content_base64: chunk.encoded } } };
}
const workbook = { sheets: [{ name: '数据', firstRow: 0, firstColumn: 0, rows: 1, columns: 1, cells: [[0, 0, '0007', '0007', false]] }] };

// Deterministic Host/Worker lifecycle fixture. Real layout is checked separately
// in Chrome; these tests cover races without timing or browser dependencies.
function reader(options = {}) {
  const nodes = {}, sent = [], workers = [], listeners = new Map(), timers = new Map();
  let nextTimer = 0, now = 0, payload = bytes;
  const delays = [];
  const node = (tagName = 'div') => ({
    tagName, textContent: '', hidden: true, children: [], style: {}, dataset: {}, attributes: {}, events: {},
    scrollTop: 0, scrollLeft: 0, clientHeight: 352, clientWidth: 600, tabIndex: -1,
    append(...children) { this.children.push(...children.flatMap(child => child.tagName === '#fragment' ? child.children : [child])); },
    replaceChildren(...children) {
      if (document.activeElement !== this && this.contains(document.activeElement)) document.activeElement = document.body;
      this.children = []; this.append(...children);
    },
    setAttribute(name, value) { this.attributes[name] = value; },
    addEventListener(name, callback) { this.events[name] = callback; },
    querySelectorAll(selector) {
      return this.children.flatMap(child => [child, ...child.querySelectorAll('*')])
        .filter(child => selector === '*' || selector === 'button' && child.tagName === 'button'
          || selector === '[aria-selected="true"]' && child.attributes['aria-selected'] === 'true');
    },
    querySelector(selector) { return this.querySelectorAll(selector)[0] || null; },
    contains(target) { return this === target || this.children.some(child => child.contains(target)); },
    focus() { document.activeElement = this; },
  });
  const document = { body: node(), documentElement: node(), getElementById: id => nodes[id] ||= node(), createElement: node, createDocumentFragment: () => node('#fragment') };
  document.activeElement = document.body;
  document.getElementById('viewport').append(document.getElementById('cells'));
  const parent = { postMessage: message => {
    sent.push(message);
    if (message.method === 'tools/call' && !options.manual) {
      assert.equal(message.params.name, 'read_app_artifact_chunk');
      let chunk = segment(message, payload);
      if (options.corrupt) chunk = options.corrupt(chunk);
      now += options.stepMs || 0;
      const value = segmentEnvelope(chunk, options.textOnly);
      deliver({ id: message.id, result: options.wrap ? options.wrap(value) : value });
    }
  } };
  const setTimer = (callback, delay) => { const id = ++nextTimer; timers.set(id, { callback, delay }); delays.push(delay); return id; };
  class Worker {
    constructor() { workers.push(this); }
    postMessage(message) { this.input = message; }
    terminate() { this.terminated = true; }
    reply(value = workbook) { this.onmessage({ data: { workbook: value } }); }
  }
  runInNewContext(script, {
    document, parent, Worker, Blob, Uint8Array, atob, performance: { now: () => now },
    URL: { createObjectURL: () => 'blob:fixture', revokeObjectURL() {} },
    crypto: { subtle: { async digest(_, input) { const hash = createHash('sha256').update(input).digest(); return hash.buffer.slice(hash.byteOffset, hash.byteOffset + hash.byteLength); } } },
    ResizeObserver: class { observe() {} disconnect() {} },
    requestAnimationFrame: callback => setTimer(callback, 16), cancelAnimationFrame: id => timers.delete(id),
    setTimeout: setTimer, clearTimeout: id => timers.delete(id),
    addEventListener: (name, callback) => listeners.set(name, callback),
  });
  const deliver = (message, source = parent) => listeners.get('message')({ source, data: { jsonrpc: '2.0', ...message } });
  return {
    nodes, sent, workers, timers, document, delays,
    payload: value => { payload = value; },
    chunks: () => sent.filter(message => message.method === 'tools/call'),
    reads: () => sent.filter(message => message.method === 'resources/read'),
    present: (value, source) => deliver({ method: 'ui/notifications/tool-result', params: value }, source),
    async initialize() { deliver({ id: sent[0].id, result: { hostContext: { theme: 'light' } } }); await flush(); },
    async reply(request, value) { deliver({ id: request.id, result: value }); await flush(); },
    async teardown() { deliver({ method: 'ui/resource-teardown', id: 'teardown' }); await flush(); },
    async expire(delay) { for (const [id, timer] of [...timers]) if (timer.delay === delay) { timers.delete(id); timer.callback(); } await flush(); },
  };
}

test('private source waits for initialization, reads exact segments once, and ignores foreign messages', async () => {
  const view = reader();
  view.present(result('foreign'), {});
  view.present(result('first')); view.present(result('first'));
  assert.equal(view.reads().length, 0);
  await view.initialize();
  assert.equal(view.reads().length, 0);
  await flush();
  view.workers[0].reply(); await flush();
  assert.equal(view.nodes.value.textContent, '0007');
  view.present(result('first')); assert.equal(view.reads().length, 0);
  assert.equal(view.chunks().length, 1);
  await view.teardown();
});

test('source replacement fences late digest completions and malformed results clear old cells', async () => {
  const view = reader(); await view.initialize();
  view.present(result('old')); view.present(result('new'));
  await flush(); assert.equal(view.workers.length, 1);
  view.workers[0].reply(); await flush();
  assert.equal(view.nodes.grid.hidden, false);
  view.present(result('invalid', 'bad'));
  assert.equal(view.nodes.grid.hidden, true); assert.match(view.nodes.status.textContent, /Invalid/);
  await view.teardown();
});

test('changed hashes fail before parsing and a parser deadline terminates its Worker', async () => {
  const view = reader(); await view.initialize();
  view.present(result('changed', '0'.repeat(64))); await flush();
  assert.equal(view.workers.length, 0); assert.match(view.nodes.status.textContent, /content changed/);
  view.present(result('slow')); await flush();
  await view.expire(10000);
  assert.equal(view.workers[0].terminated, true); assert.match(view.nodes.status.textContent, /exceeded 10 seconds/);
  assert.equal(view.nodes.grid.hidden, true);
  await view.teardown();
});

test('teardown cancels pending work and ignores late Host or Worker replies', async () => {
  const view = reader(); await view.initialize();
  view.present(result('pending')); await flush();
  const worker = view.workers[0];
  await view.teardown(); worker.reply(); await flush();
  assert.equal(worker.terminated, true); assert.equal(view.nodes.grid.hidden, true);
  assert.equal(view.timers.size, 0);
  view.present(result('later')); assert.equal(view.reads().length, 0);
});


test('a failed notification fences old parsing and allows the same source to open again', async () => {
  const view = reader(); await view.initialize();
  view.present(result('same'));
  view.present({ structuredContent: { success: false, error: 'Unavailable' } });
  view.present(result('same'));
  await flush();
  assert.equal(view.reads().length, 0);
  assert.equal(view.workers.length, 1);
  view.workers[0].reply(); await flush();
  assert.equal(view.nodes.grid.hidden, false);
  await view.teardown();
});

test('virtual scrolling keeps one keyboard entry and retains grid focus when the selected cell unmounts', async () => {
  for (const scroll of [{ top: 320000, left: 0 }, { top: 0, left: 1440 }, { top: 320000, left: 1440 }]) {
    const view = reader(); await view.initialize();
    view.present(result('scroll')); await flush();
    view.workers[0].reply({ sheets: [{ name: '数据', firstRow: 0, firstColumn: 0, rows: 20001, columns: 50, cells: [] }] });
    await flush(); await view.expire(16);
    const viewport = view.nodes.viewport, grid = view.nodes.cells;
    const selected = grid.querySelector('[aria-selected="true"]');
    assert.ok(selected); selected.focus();
    assert.equal(viewport.tabIndex, -1);
    viewport.scrollTop = scroll.top; viewport.scrollLeft = scroll.left;
    viewport.events.scroll(); await view.expire(16);
    assert.equal(grid.querySelector('[aria-selected="true"]'), null);
    assert.equal(grid.querySelectorAll('button').filter(button => button.tabIndex === 0).length, 0);
    assert.equal(viewport.tabIndex, 0);
    assert.equal(view.document.activeElement, viewport);
    assert.equal(view.nodes.address.textContent, 'A1');
    let prevented = false;
    viewport.events.keydown({ key: 'ArrowRight', preventDefault() { prevented = true; } });
    assert.equal(prevented, true);
    assert.equal(view.nodes.address.textContent, 'B1');
    assert.equal(viewport.tabIndex, -1);
    assert.equal(view.document.activeElement, grid.querySelector('[aria-selected="true"]'));
    assert.equal(grid.querySelectorAll('button').filter(button => button.tabIndex === 0).length, 1);
    viewport.scrollTop = scroll.top; viewport.scrollLeft = scroll.left;
    viewport.events.scroll(); await view.expire(16);
    const clicked = grid.querySelectorAll('button')[0]; clicked.focus();
    grid.events.click({ target: { closest: () => clicked } });
    assert.equal(viewport.tabIndex, -1);
    assert.equal(grid.querySelectorAll('button').filter(button => button.tabIndex === 0).length, 1);
    assert.equal(view.reads().length, 0);
    assert.equal(view.chunks().length, 1);
    await view.teardown();
  }
});

test('private identity and segment metadata survive Host wrappers and normalized meta fields', async () => {
  const wrappers = [value => value, value => ({ result: value }), value => ({ toolResult: value }),
    value => ({ tool_result: value }), value => ({ ...value, meta: value._meta, _meta: undefined })];
  for (const wrap of wrappers) {
    const view = reader({ wrap }); await view.initialize();
    const full = result('private'); delete full.content; delete full.structuredContent;
    view.present(wrap(full)); await flush();
    view.workers[0].reply(); await flush();
    assert.equal(view.nodes.value.textContent, '0007');
    view.present({ structuredContent: { success: true, output: { name: 'data.csv' } } });
    assert.equal(view.nodes.grid.hidden, true); assert.match(view.nodes.status.textContent, /Invalid/);
    assert.equal(view.reads().length, 0);
    await view.teardown();
  }
});

test('paged reads preserve 5 MiB across slow Host round trips with a text-only envelope', async () => {
  const view = reader({ textOnly: true, stepMs: 15000 }); await view.initialize();
  const payload = Buffer.alloc(5 * 1024 * 1024);
  for (let index = 0; index < payload.length; index++) payload[index] = index % 251;
  view.payload(payload);
  const full = result('large');
  full.structuredContent.output.bytes = payload.length;
  full.structuredContent.output.sha256 = createHash('sha256').update(payload).digest('hex');
  view.present(full); await flush();
  assert.equal(view.workers.length, 1);
  assert.deepEqual(Buffer.from(view.workers[0].input.bytes), payload);
  assert.deepEqual(view.chunks().map(call => call.params.arguments.byte_offset), Array.from({ length: 10 }, (_, index) => index * 512 * 1024));
  assert.ok(view.chunks().every(call => call.params.arguments.path === 'large.csv'));
  assert.deepEqual(view.delays.filter(delay => delay > 20000), Array.from({ length: 10 }, (_, index) => 260000 - index * 15000));
  assert.equal(view.reads().length, 0);
  await view.teardown();
});

test('clipped, reordered and changed segments fail before parsing', async () => {
  for (const corrupt of [chunk => ({ ...chunk, encoded: 'AAAA…' }), chunk => ({ ...chunk, page: { ...chunk.page, byte_offset: 1 } }), chunk => ({ ...chunk, encoded: chunk.encoded.slice(0, -4) }), chunk => ({ ...chunk, page: { ...chunk.page, project: 'other' } }), chunk => ({ ...chunk, page: { ...chunk.page, path: 'other.csv' } }), chunk => ({ ...chunk, page: { ...chunk.page, sha256: '0'.repeat(64) } })]) {
    const view = reader({ corrupt }); await view.initialize();
    view.present(result('clipped')); await flush();
    assert.equal(view.workers.length, 0); assert.equal(view.nodes.grid.hidden, true);
    assert.match(view.nodes.status.textContent, /complete spreadsheet file data/);
    await view.teardown();
  }
  const view = reader(); await view.initialize(); view.payload(Buffer.alloc(bytes.length, 1));
  view.present(result('changed')); await flush();
  assert.equal(view.workers.length, 0); assert.match(view.nodes.status.textContent, /content changed/);
  await view.teardown();
});

test('selection replacement and teardown fence pending segment reads without requesting another segment', async () => {
  const view = reader({ manual: true }); await view.initialize();
  const full = result('old'); full.structuredContent.output.bytes = 512 * 1024;
  view.present(full); const first = view.chunks()[0];
  view.present(result('new'));
  await view.reply(first, segmentEnvelope(segment(first, Buffer.alloc(512 * 1024))));
  assert.equal(view.chunks().length, 2); assert.equal(view.workers.length, 0);
  assert.ok(view.chunks().every(call => call.params.arguments.byte_offset === 0));
  await view.teardown();
  await view.reply(view.chunks()[1], segmentEnvelope(segment(view.chunks()[1], bytes)));
  assert.equal(view.chunks().length, 2); assert.equal(view.workers.length, 0); assert.equal(view.timers.size, 0);
});


test('transfer deadline is bounded and a late final segment never starts parsing', async () => {
  const view = reader({ manual: true }); await view.initialize();
  view.present(result('timeout'));
  await view.expire(120000);
  assert.equal(view.workers.length, 0); assert.match(view.nodes.status.textContent, /timed out/);
  await view.reply(view.chunks()[0], segmentEnvelope(segment(view.chunks()[0], bytes)));
  assert.equal(view.workers.length, 0);
  await view.teardown();
  const late = reader({ stepMs: 120001 }); await late.initialize();
  late.present(result('late')); await flush();
  assert.equal(late.workers.length, 0); assert.match(late.nodes.status.textContent, /timed out/);
  await late.teardown();
});

test('transient transfer failure exposes an explicit retry for the same pinned source', async () => {
  const view = reader({ manual: true }); await view.initialize();
  view.present(result('retry')); const first = view.chunks()[0];
  await view.expire(120000);
  assert.equal(view.nodes.retry.hidden, false); assert.match(view.nodes.status.textContent, /timed out/);
  view.nodes.retry.events.click(); await flush();
  assert.equal(view.nodes.retry.hidden, true); assert.equal(view.chunks().length, 2);
  const second = view.chunks()[1];
  assert.equal(second.params.arguments.byte_offset, 0);
  await view.reply(second, segmentEnvelope(segment(second, bytes)));
  view.workers[0].reply(); await flush();
  assert.equal(view.nodes.grid.hidden, false); assert.equal(view.nodes.retry.hidden, true);
  await view.reply(first, segmentEnvelope(segment(first, bytes)));
  assert.equal(view.workers.length, 1);
  await view.teardown();
});

test('public-only bytes and malformed continuations never enter the parser', async () => {
  for (const wrap of [value => ({ structuredContent: { ...value.structuredContent, output: { artifact_chunk: {
    ...value.structuredContent.output.artifact_chunk, content_base64: value._meta['webcodex/artifactChunk'].content_base64,
  } } } }), value => ({ ...value, structuredContent: { success: true, output: { artifact_chunk: {
    ...value.structuredContent.output.artifact_chunk, next_byte_offset: 1,
  } } } })]) {
    const view = reader({ wrap }); await view.initialize();
    view.present(result('invalid-transport')); await flush();
    assert.equal(view.workers.length, 0); assert.equal(view.nodes.grid.hidden, true);
    await view.teardown();
  }
});
