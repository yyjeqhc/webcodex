import test from "node:test";
import assert from "node:assert/strict";
import { createHostBridge } from "../src/mcp-apps/host-bridge.mjs";

function fixture(t, options = {}) {
  const sent = [], timers = new Map();
  let nextTimer = 1;
  t.mock.method(globalThis, "setTimeout", (callback, delay) => {
    const id = nextTimer++;
    timers.set(id, { callback, delay });
    return id;
  });
  t.mock.method(globalThis, "clearTimeout", id => timers.delete(id));
  const target = { postMessage(message, origin) { assert.equal(origin, "*"); sent.push(message); } };
  const bridge = createHostBridge({ target, timeoutMs: 42, ...options });
  const receive = (data, source = target) => bridge.receive({ source, data: { jsonrpc: "2.0", ...data } });
  return { bridge, target, sent, timers, receive };
}

test("only the bound host settles the exact pending request once", async t => {
  const { bridge, sent, timers, receive } = fixture(t);
  const request = bridge.request("tools/call", { name: "observe" });
  const id = sent[0].id;
  assert.equal(receive({ id, result: "foreign" }, {}), null);
  assert.equal(receive({ jsonrpc: "1.0", id, result: "wrong protocol" }), null);
  assert.equal(timers.size, 1);
  assert.equal(receive({ id, result: "observed" }), null);
  assert.equal(await request, "observed");
  assert.equal(timers.size, 0);
  receive({ id, result: "duplicate" });
  assert.equal(sent.length, 1);
  assert.equal(receive({ method: "ui/notifications/tool-result" }).method, "ui/notifications/tool-result");
});

test("timeout removes the waiter without retrying an uncertain request", async t => {
  const { bridge, sent, timers, receive } = fixture(t);
  const request = bridge.request("tools/call", { name: "change" }, 17);
  const [timer] = timers.values();
  assert.equal(timer.delay, 17);
  timer.callback();
  await assert.rejects(request, /Host request timed out/);
  receive({ id: sent[0].id, result: "late" });
  assert.equal(sent.length, 1);
});

test("teardown clears waiters, preserves unique unbind ids and allows its acknowledgement", async t => {
  const { bridge, sent, timers } = fixture(t);
  const request = bridge.request("tools/call", { name: "read" });
  bridge.sendUntracked("tools/call", { name: "unbind" });
  assert.notEqual(sent[0].id, sent[1].id);
  bridge.close();
  bridge.close();
  await assert.rejects(request, /App torn down/);
  await assert.rejects(bridge.request("tools/call", {}), /App torn down/);
  bridge.notify("ui/notifications/initialized", {});
  bridge.sendUntracked("tools/call", { name: "unbind" });
  assert.equal(sent.length, 2);
  assert.equal(timers.size, 0);
  bridge.reply("teardown-id", {});
  assert.deepEqual(sent[2], { jsonrpc: "2.0", id: "teardown-id", result: {} });
});

test("host errors retain the caller's classification and clear the deadline", async t => {
  const expected = new Error("classified failure");
  const { bridge, sent, timers, receive } = fixture(t, {
    error(kind, response) { assert.equal(kind, "response"); assert.equal(response.code, -32602); return expected; },
  });
  const request = bridge.request("tools/call", {});
  receive({ id: sent[0].id, error: { code: -32602 } });
  await assert.rejects(request, error => error === expected);
  assert.equal(timers.size, 0);
});

test("synchronous host transport failure releases the pending entry and timer", async t => {
  const { bridge, target, timers } = fixture(t);
  target.postMessage = () => { throw new Error("transport unavailable"); };
  await assert.rejects(bridge.request("ui/initialize", {}), /transport unavailable/);
  assert.equal(timers.size, 0);
  bridge.close();
});
