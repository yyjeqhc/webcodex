import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readPdfDocument } from "../src/mcp-apps/work-result/pdf-document-reader.mjs";
import { PDF_PAGE_BYTES } from "../src/mcp-apps/work-result/pdf-reader.mjs";

const data = Buffer.concat([Buffer.from("%PDF-1.7\n"), Buffer.alloc(PDF_PAGE_BYTES * 2, 42)]);
const identity = { project: "agent:pdf:demo", path: "unchanged.pdf", bytes: data.length, sha256: createHash("sha256").update(data).digest("hex") };
const response = (offset, patch = {}) => {
  const next = Math.min(offset + PDF_PAGE_BYTES, data.length);
  return { page: { project: identity.project, path: identity.path, sha256: identity.sha256,
    bytes_total: data.length, byte_offset: offset, next_byte_offset: next === data.length ? null : next,
    complete: next === data.length, ...patch }, encoded: data.subarray(offset, next).toString("base64") };
};
const options = request => ({ identity, request, current: () => true });

test("dedicated PDF reads verify the whole version and use one absolute deadline", async () => {
  const calls = []; let time = 0;
  const result = await readPdfDocument({ ...options(async (offset, remaining) => { calls.push([offset, remaining]); time += 10_000; return response(offset); }), now: () => time });
  assert.deepEqual(Buffer.from(result), data);
  assert.deepEqual(calls, [[0, 120_000], [PDF_PAGE_BYTES, 110_000], [PDF_PAGE_BYTES * 2, 100_000]]);
});
test("dedicated PDF rejects substituted identity, malformed continuation and public-only binary", async () => {
  for (const patch of [{ project: "other" }, { path: "other.pdf" }, { sha256: "0".repeat(64) },
    { bytes_total: data.length + 1 }, { byte_offset: 1 }, { next_byte_offset: 1 }, { complete: true }])
    await assert.rejects(readPdfDocument(options(async offset => response(offset, patch))), /chunk|continuation/);
  await assert.rejects(readPdfDocument(options(async offset => ({ page: response(offset).page }))), /chunk/);
  await assert.rejects(readPdfDocument(options(async offset => ({ ...response(offset), encoded: "AAAA" }))), /continuation/);
});
test("dedicated PDF rejects corrupted bytes despite a matching claimed SHA", async () => {
  await assert.rejects(readPdfDocument(options(async offset => {
    const result = response(offset), bytes = Buffer.from(result.encoded, "base64"); bytes[bytes.length - 1] ^= 1;
    return { ...result, encoded: bytes.toString("base64") };
  })), /version mismatch/);
});
test("closing and transfer timeout never accept a late document", async () => {
  let active = true, calls = 0;
  await assert.rejects(readPdfDocument({ ...options(async offset => { calls++; active = false; return response(offset); }), current: () => active }), /closed/);
  assert.equal(calls, 1);
  let time = 0;
  await assert.rejects(readPdfDocument({ ...options(async offset => { time = 120_001; return response(offset); }), now: () => time }), /timed out/);
});
