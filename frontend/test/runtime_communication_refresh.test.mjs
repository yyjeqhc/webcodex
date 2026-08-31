import test from "node:test";
import assert from "node:assert/strict";
import { RuntimeCommunicationRefreshCoordinator } from "../dist/runtime_console_state.js";

function deferred() {
  let resolve;
  const promise = new Promise((done) => { resolve = done; });
  return { promise, resolve };
}

async function settleMicrotasks() {
  await Promise.resolve();
  await Promise.resolve();
}

function communicationRefreshHarness() {
  const lease = deferred();
  const full = deferred();
  const refreshes = [];
  const dataEndpoints = [];
  const coordinator = new RuntimeCommunicationRefreshCoordinator(async (includeData) => {
    refreshes.push(includeData ? "full" : "lease-only");
    if (!includeData) {
      await lease.promise;
      return true;
    }
    dataEndpoints.push("agents", "conversations", "conversation", "inbox");
    await full.promise;
    return true;
  });
  return { coordinator, lease, full, refreshes, dataEndpoints };
}

test("lease-only in flight + manual full refresh waits for full communication data", async () => {
  const { coordinator, lease, full, refreshes, dataEndpoints } = communicationRefreshHarness();
  const autoRefresh = coordinator.refresh(false);
  await settleMicrotasks();

  const manualRefresh = coordinator.refresh(true);
  let manualSettled = false;
  void manualRefresh.then(() => { manualSettled = true; });
  await settleMicrotasks();
  assert.deepEqual(refreshes, ["lease-only"]);
  assert.equal(manualSettled, false);

  lease.resolve();
  assert.equal(await autoRefresh, true);
  await settleMicrotasks();
  assert.deepEqual(refreshes, ["lease-only", "full"]);
  assert.deepEqual(dataEndpoints, ["agents", "conversations", "conversation", "inbox"]);
  assert.equal(manualSettled, false);

  full.resolve();
  assert.equal(await manualRefresh, true);
  assert.equal(manualSettled, true);
});

test("lease-only in flight + switch to Operations queues exactly one full refresh", async () => {
  const { coordinator, lease, full, refreshes, dataEndpoints } = communicationRefreshHarness();
  const autoRefresh = coordinator.refresh(false);
  await settleMicrotasks();

  const operationsRefresh = coordinator.refresh(true);
  const secondFullCaller = coordinator.refresh(true);
  let operationsSettled = false;
  void operationsRefresh.then(() => { operationsSettled = true; });
  await settleMicrotasks();
  assert.deepEqual(refreshes, ["lease-only"]);
  assert.equal(operationsSettled, false);

  lease.resolve();
  assert.equal(await autoRefresh, true);
  await settleMicrotasks();
  assert.deepEqual(refreshes, ["lease-only", "full"]);
  assert.deepEqual(dataEndpoints, ["agents", "conversations", "conversation", "inbox"]);
  assert.equal(operationsSettled, false);

  full.resolve();
  assert.equal(await operationsRefresh, true);
  assert.equal(await secondFullCaller, true);
  assert.deepEqual(refreshes, ["lease-only", "full"]);
});
