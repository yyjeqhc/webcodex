import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import vm from "node:vm";

test("operation navigation shows one destination and moves focus without replacing its form", async () => {
  const source = await readFile(new URL("../dist/runtime.js", import.meta.url), "utf8");
  const start = source.indexOf("function revealOperationsSection(");
  const end = source.indexOf("\n}", start) + 2;
  const names = ["overview", "runners", "agents"].map((name) => "runtime-operations-" + name);
  let focused;
  const panels = new Map(names.map((id) => [id, {
    hidden: id !== names[0],
    draft: "unsent message",
    focus() { focused = id; },
  }]));
  const buttons = names.map((id) => ({
    dataset: { operationsTarget: id },
    attrs: {},
    classList: { toggle() {} },
    setAttribute(key, value) { this.attrs[key] = value; },
    removeAttribute(key) { delete this.attrs[key]; },
  }));
  const scroll = { scrollTop: 800 };
  const context = vm.createContext({
    document: { querySelectorAll: () => buttons, querySelector: () => scroll },
    applyWorkspaceView() {},
    show(id, visible) { panels.get(id).hidden = !visible; },
    el: (id) => panels.get(id),
  });
  vm.runInContext(source.slice(start, end), context);
  for (const id of [names[2], names[1], names[0], names[2]]) {
    context.revealOperationsSection(id);
    assert.deepEqual([...panels].filter(([, panel]) => !panel.hidden).map(([key]) => key), [id]);
    assert.deepEqual(buttons.filter((button) => button.attrs["aria-current"] === "page").map((button) => button.dataset.operationsTarget), [id]);
    assert.equal(focused, id);
    assert.equal(scroll.scrollTop, 0);
    assert.equal(panels.get(id).draft, "unsent message");
  }
  context.revealOperationsSection("invalid");
  assert.equal(focused, names[2]);
  assert.equal(panels.get(names[2]).hidden, false);
});

test("context overview exposes validation while activity and identity have direct entries", async () => {
  const html = await readFile(new URL("../src/runtime.html", import.meta.url), "utf8");
  const overview = html.slice(html.indexOf('<section id="runtime-context-overview"'), html.indexOf('<section id="runtime-context-activity"'));
  assert.match(overview, /id="runtime-overview-validation"/);
  assert.match(overview, /id="runtime-overview-attention"/);
  assert.doesNotMatch(overview, /<details/);
  for (const name of ["overview", "activity", "details"]) {
    assert.match(html, new RegExp('data-context-target="runtime-context-' + name + '" aria-controls="runtime-context-' + name + '"'));
  }
});

test("closing mobile navigation fences its delayed focus callback", async () => {
  const source = await readFile(new URL("../dist/runtime.js", import.meta.url), "utf8");
  const start = source.indexOf("function setMobileNavigationOpen(");
  const end = source.indexOf("\n}", start) + 2;
  const classes = new Set();
  const callbacks = [];
  let focused = false;
  const shell = { classList: {
    toggle(name, enabled) { if (enabled) classes.add(name); else classes.delete(name); },
    contains(name) { return classes.has(name); },
  } };
  const context = vm.createContext({
    el(id) {
      if (id === "runtime-console") return shell;
      if (id === "runtime-mobile-nav-close") return { focus() { focused = true; } };
      return { setAttribute() {}, removeAttribute() {} };
    },
    mobileNavigationViewport: () => true,
    closeAppearanceMenus() {},
    closeTopbarMore() {},
    closeRuntimeInspector() {},
    window: { setTimeout(callback) { callbacks.push(callback); } },
  });
  vm.runInContext(source.slice(start, end), context);
  context.setMobileNavigationOpen(true);
  context.setMobileNavigationOpen(false);
  callbacks.shift()();
  assert.equal(focused, false);
  context.setMobileNavigationOpen(true);
  callbacks.shift()();
  assert.equal(focused, true);
});
