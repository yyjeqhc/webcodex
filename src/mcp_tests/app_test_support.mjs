import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { webcrypto } from "node:crypto";

export const flush = () => new Promise(resolve => setImmediate(resolve));

// Execute the shipped App script with deterministic Host messages and timers.
export function app(filename, { deliverToolMeta = true, deliverToolStructuredContent = true, crypto = webcrypto, navigator = {} } = {}) {
  const html = readFileSync(new URL(`../${filename}`, import.meta.url), "utf8");
  const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
  const nodes = {};
  const listeners = new Map();
  const timers = new Map();
  const sent = [];
  const viewport = { scrollY: 0, innerHeight: 800, moves: [] };
  const selection = { value: null };
  let nextTimer = 1;
  let nowMs = 2_000_000_000_000;
  const HostDate = class extends Date {
    static now() { return nowMs; }
  };
  const parent = { postMessage(message) { sent.push(message); } };
  function element(tagName = "div") {
    const attributes = new Map();
    return {
      tagName: String(tagName).toUpperCase(),
      textContent: "", hidden: false, open: false, children: [], className: "", type: "", onclick: null, ontoggle: null,
      parentNode: null,
      get ownerDocument() { return document; },
      style: { setProperty() {} },
      getBoundingClientRect() { return { height: 400, width: 800, top: 0, bottom: 400 }; },
      get isConnected() { return !!this.documentNode || !!this.parentNode?.isConnected; },
      contains(node) { return node === this || this.children.some(child => child.contains(node)); },
      focus() { document.activeElement = this; },
      append(...children) { for (const child of children) this.appendChild(child); },
      appendChild(child) { return this.insertBefore(child, null); },
      insertBefore(child, before) {
        if (child === before) return child;
        child.remove();
        const index = before === null ? this.children.length : this.children.indexOf(before);
        if (index < 0) throw new Error("Reference node is not a child");
        this.children.splice(index, 0, child); child.parentNode = this; return child;
      },
      remove() {
        if (this.parentNode) this.parentNode.children.splice(this.parentNode.children.indexOf(this), 1);
        this.parentNode = null;
      },
      replaceChildren(...children) {
        for (const child of [...this.children]) child.remove();
        this.append(...children); this.textContent = "";
      },
      setAttribute(name, value) { attributes.set(name, String(value)); },
      getAttribute(name) { return attributes.get(name); },
    };
  }
  const document = {
    hidden: false,
    body: element("body"),
    documentElement: element("html"),
    getElementById: id => nodes[id] ||= Object.assign(element(), { documentNode: true }),
    createElement: tagName => element(tagName),
  };
  document.activeElement = document.body;
  function addEventListener(name, listener) {
    if (!listeners.has(name)) listeners.set(name, []);
    listeners.get(name).push(listener);
  }
  function emit(name, event) {
    for (const listener of listeners.get(name) || []) listener(event);
  }
  function setTimer(callback, delay, interval = false) {
    const id = nextTimer++;
    timers.set(id, { callback, delay, interval });
    return id;
  }
  runInNewContext(script, {
    document, parent, addEventListener, TextEncoder, crypto, navigator, btoa, atob, Date: HostDate,
    // These controller tests do not render canvases. The shipped PDF bundle
    // constructs an identity DOMMatrix on initialization; real graphics/Worker
    // behavior is covered by the browser harness.
    DOMMatrix: class DOMMatrix {},
    ResizeObserver: class ResizeObserver { observe() {} disconnect() {} },
    performance: { now: () => nowMs },
    getSelection: () => selection.value,
    get scrollY() { return viewport.scrollY; },
    get innerHeight() { return viewport.innerHeight; },
    scrollBy({ top }) { viewport.moves.push(top); viewport.scrollY += top; },
    requestAnimationFrame: callback => setTimer(callback, 16),
    setTimeout: setTimer,
    clearTimeout: id => timers.delete(id),
    setInterval: (callback, delay) => setTimer(callback, delay, true),
    clearInterval: id => timers.delete(id),
  });
  function deliver(message, source = parent) {
    emit("message", { source, data: { jsonrpc: "2.0", ...message } });
  }
  return {
    nodes, timers, sent, viewport, selection, document,
    calls(name) { return sent.filter(message => message.method === "tools/call" && message.params.name === name); },
    notification(method, params, source) {
      deliver({ method, params }, source);
    },
    toolInput(args, source) {
      deliver({ method: "ui/notifications/tool-input", params: { arguments: args } }, source);
    },
    toolResult(output, source) {
      deliver({ method: "ui/notifications/tool-result", params: toolResult(output) }, source);
    },
    async reply(request, result) {
      // Only App-originated tools/call crosses this policy. Initial model-tool
      // result notifications remain a separate lifecycle with their own shape.
      if (request.method === "tools/call") {
        if (!deliverToolMeta) result = stripToolResultMeta(result);
        if (!deliverToolStructuredContent) result = stripToolResultStructuredContent(result);
      }
      deliver({ id: request.id, result });
      await flush();
    },
    async reject(request, error = { code: -32000, message: "Host request failed" }) {
      deliver({ id: request.id, error });
      await flush();
    },
    async initialize(outcome = "success") {
      if (outcome === "timeout") await this.fireTimers(10000);
      else {
        deliver({ id: sent[0].id, ...(outcome === "success"
          ? { result: { protocolVersion: "2026-01-26" } }
          : { error: { message: "Host initialization rejected" } }) });
        await flush();
      }
    },
    async fireTimers(delay) {
      nowMs += delay;
      for (const [id, timer] of [...timers]) {
        if (timer.delay !== delay) continue;
        if (!timer.interval) timers.delete(id);
        timer.callback();
      }
      await flush();
    },
    advanceTime(ms) {
      nowMs += ms;
    },
    async visibility(hidden) {
      document.hidden = hidden;
      emit("visibilitychange", {});
      await flush();
    },
    async teardown(method = "ui/resource-teardown") {
      if (method === "ui/resource-teardown") deliver({ method, id: "host-teardown" });
      else emit(method, {});
      await flush();
    },
  };
}

export function stripToolResultMeta(result) {
  const { _meta, ...standard } = result;
  return standard;
}

export function stripToolResultStructuredContent(result) {
  const { structuredContent, structured_content, ...standard } = result;
  return standard;
}

export function toolResult(output, privateMeta) {
  return {
    structuredContent: { success: true, output },
    ...(privateMeta ? { _meta: { "webcodex/agentContinuation": privateMeta } } : {}),
  };
}
