import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { ExtensionApprovalStore, fingerprintExtension } from "./dist/approval-store.js";

async function fixture(run) {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "webpi-approval-security-"));
  try { await run(root); } finally { await fs.rm(root, { recursive: true, force: true }); }
}

test("approval rejects linked directories, linked descendants and hardlinked files", async () => fixture(async (root) => {
  const candidate = path.join(root, "extension");
  const target = path.join(root, "outside");
  await fs.mkdir(candidate); await fs.mkdir(target);
  await fs.writeFile(path.join(candidate, "index.ts"), "export default () => {};\n");
  await fs.writeFile(path.join(target, "helper.ts"), "export const x = 1;\n");
  await fs.symlink(target, path.join(candidate, "helper"), process.platform === "win32" ? "junction" : "dir");
  await assert.rejects(fingerprintExtension(candidate), /symlink|junction/u);
  await assert.rejects(fingerprintExtension(path.join(candidate, "helper")), /symlink|junction/u);
  await assert.rejects(fingerprintExtension(path.join(candidate, "helper", "helper.ts")), /symlink|junction/u);
  await fs.link(path.join(target, "helper.ts"), path.join(root, "linked.ts"));
  await assert.rejects(fingerprintExtension(path.join(root, "linked.ts")), /single-link/u);
}));

test("approval size limits run before allocating oversized file buffers", async () => fixture(async (root) => {
  const file = path.join(root, "large.ts");
  const handle = await fs.open(file, "w");
  await handle.truncate(64 * 1024 * 1024 + 1); await handle.close();
  await assert.rejects(fingerprintExtension(file), /size limit/u);
}));

test("approval depth limits include empty directories", async () => fixture(async (root) => {
  const candidate = path.join(root, "extension");
  await fs.mkdir(path.join(candidate, ...Array(66).fill("d")), { recursive: true });
  await assert.rejects(fingerprintExtension(candidate), /depth limit/u);
}));

test("malformed approval documents cannot become trusted entries", async () => fixture(async (root) => {
  const agent = path.join(root, "state");
  await fs.mkdir(agent);
  const store = new ExtensionApprovalStore(root, agent);
  for (const document of [{ version: 1, approvals: [] }, { version: 1, approvals: { relative: "a".repeat(64) } }, { version: 1, approvals: { [path.join(root, "x.ts")]: "not-a-sha" } }]) {
    await fs.writeFile(store.file, JSON.stringify(document));
    await assert.rejects(store.list(), /invalid/u);
  }
}));

test("exact approval rejects changed candidates and revocation clears authority", async () => fixture(async (root) => {
  const file = path.join(root, "extension.ts");
  await fs.writeFile(file, "export default () => {};\n");
  const store = new ExtensionApprovalStore(root, path.join(root, "state"));
  const before = await fingerprintExtension(file);
  await store.approveExpected(file, before);
  assert.equal((await store.status(file)).approved, true);
  await fs.writeFile(store.file + ".lock", "synthetic-other-operation");
  await assert.rejects(store.revoke(file), /busy/u);
  assert.equal((await store.status(file)).approved, true);
  await fs.unlink(store.file + ".lock");
  await fs.writeFile(file, "export default () => { const changed = true; };\n");
  assert.equal((await store.status(file)).approved, false);
  await assert.rejects(store.approveExpected(file, before), /changed/u);
  assert.equal(await store.revoke(file), true);
  assert.equal((await store.status(file)).approved, false);
}));
