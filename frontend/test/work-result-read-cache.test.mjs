import test from "node:test";
import assert from "node:assert/strict";
import { createReadCache } from "../src/mcp-apps/work-result/read-cache.mjs";
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

test("read owners deduplicate the exact key, retain success, and never retry failures", async () => {
  const cache = createReadCache(), pending = deferred(); let calls = 0;
  const load = () => { calls++; return pending.promise; };
  const first = cache.read("one", load), second = cache.read("one", load);
  assert.equal(first, second); assert.equal(calls, 1);
  pending.resolve({ value: 1 }); assert.equal(await first, await second);
  assert.deepEqual(await cache.read("one", load), { value: 1 }); assert.equal(calls, 1);
  await assert.rejects(cache.read("failure", () => { calls++; throw new Error("failed"); }));
  assert.equal(calls, 2); assert.equal(cache.peek("failure"), undefined);
  assert.equal(await cache.read("failure", () => { calls++; return "retried explicitly"; }), "retried explicitly");
  assert.equal(calls, 3);
});

for (const late of ["success", "failure"]) test(`reset fences late ${late} and does not clear a replacement request`, async () => {
  const cache = createReadCache(), old = deferred(), fresh = deferred();
  const ignored = assert.rejects(cache.read("same", () => old.promise));
  cache.reset();
  const current = cache.read("same", () => fresh.promise);
  if (late === "success") old.resolve("obsolete"); else old.reject(new Error("obsolete"));
  await ignored;
  assert.equal(cache.peek("same"), undefined);
  assert.equal(cache.read("same", () => { throw new Error("duplicate"); }), current);
  fresh.resolve("current"); assert.equal(await current, "current");
  assert.equal(cache.peek("same"), "current");
});

test("dispose fences pending reads, releases cached values and prevents new transport work", async () => {
  const cache = createReadCache(), pending = deferred();
  await cache.read("cached", () => "cached");
  let valid; const ignored = assert.rejects(cache.read("pending", current => { valid = current; return pending.promise; }));
  cache.dispose(); cache.dispose(); assert.equal(valid(), false);
  pending.resolve("obsolete"); await ignored;
  assert.equal(cache.peek("cached"), undefined);
  await assert.rejects(cache.read("new", () => { assert.fail("must not start"); }));
});
