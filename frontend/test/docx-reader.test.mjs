import test from "node:test";
import assert from "node:assert/strict";
import { webcrypto } from "node:crypto";
import { readDocxDocument, DOCX_CHUNK_BYTES, validDocxIdentity } from "../src/mcp-apps/docx-reader.mjs";
import { docxFixture } from "./fixtures/docx.mjs";
if (!globalThis.crypto) globalThis.crypto = webcrypto;
async function identityFor(data) {
  return { project: "agent:docx:demo", path: "reports/report.docx", name: "report.docx", bytes: data.length,
    sha256: Buffer.from(await crypto.subtle.digest("SHA-256", data)).toString("hex") };
}
function segment(identity, data, offset) {
  const next = Math.min(offset + DOCX_CHUNK_BYTES, data.length);
  return { page: { project: identity.project, path: identity.path, sha256: identity.sha256, bytes_total: data.length,
    byte_offset: offset, next_byte_offset: next === data.length ? null : next, complete: next === data.length },
    encoded: Buffer.from(data.subarray(offset, next)).toString("base64") };
}
test("DOCX transfer validates every segment and the complete digest", async () => {
  const data = new Uint8Array(DOCX_CHUNK_BYTES + 7); data.set([80, 75, 3, 4]);
  const identity = await identityFor(data), calls = [];
  const result = await readDocxDocument({ identity, current: () => true, request: async offset => { calls.push(offset); return segment(identity, data, offset); } });
  assert.deepEqual(result, data); assert.deepEqual(calls, [0, DOCX_CHUNK_BYTES]);
});
test("DOCX transfer rejects stale identities, incomplete segments and wrong digests", async () => {
  const data = await docxFixture(), identity = await identityFor(data);
  for (const mutate of [value => { value.page.sha256 = "a".repeat(64); }, value => { value.page.next_byte_offset = 1; }, value => { value.encoded = "UEs="; }, value => { value.encoded = Buffer.from(new Uint8Array(data.length)).toString("base64"); }]) {
    await assert.rejects(readDocxDocument({ identity, current: () => true, request: async offset => { const value = segment(identity, data, offset); mutate(value); return value; } }));
  }
});
test("DOCX teardown and the original deadline stop transfer without another request", async () => {
  const data = await docxFixture(), identity = await identityFor(data); let calls = 0;
  await assert.rejects(readDocxDocument({ identity, current: () => false, request: async () => { calls++; } }), /closed/); assert.equal(calls, 0);
  let now = 0;
  await assert.rejects(readDocxDocument({ identity, current: () => true, now: () => now, request: async offset => { now = 120001; calls++; return segment(identity, data, offset); } }), /timed out/); assert.equal(calls, 1);
});
test("DOCX identity rejects unsupported size, paths and file formats", () => {
  const base = { project: "~p1", path: "a.docx", name: "a.docx", bytes: 100, sha256: "a".repeat(64) };
  assert.ok(validDocxIdentity(base));
  for (const patch of [{ bytes: 10485761 }, { bytes: 0 }, { path: "../a.docx" }, { path: "C:/a.docx" }, { path: "a.doc" }, { sha256: "bad" }]) assert.ok(!validDocxIdentity({ ...base, ...patch }));
});
