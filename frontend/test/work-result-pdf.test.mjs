import test from "node:test";
import assert from "node:assert/strict";
import { readPdf, PDF_PAGE_BYTES, MAX_PDF_BYTES } from "../src/mcp-apps/work-result/pdf-reader.mjs";

const identity = { project: "agent:pdf:demo", session_id: null, snapshot_id: "pinned", path: "report.pdf" };
const response = (data, offset, overrides = {}) => {
  const bytes = data.subarray(offset, offset + PDF_PAGE_BYTES), end = offset + bytes.length;
  return { page: { ...identity, view: "pdf", byte_offset: offset, bytes_total: data.length,
    next_byte_offset: end === data.length ? null : end, complete: end === data.length, ...overrides },
    encoded: Buffer.from(bytes).toString("base64") };
};
const options = request => ({ identity, request, current: () => true });

test("PDF chunks reconstruct exactly across the former text limit and carry one fixed deadline", async () => {
  const data = Uint8Array.from({ length: PDF_PAGE_BYTES * 2 + 7 }, (_, index) => index % 256);
  let time = 10, calls = [], progress = [];
  const result = await readPdf({ ...options(async (offset, remaining) => {
    calls.push([offset, remaining]); time += 10_000; return response(data, offset);
  }), now: () => time, progress: (...args) => progress.push(args) });
  assert.deepEqual(result, data);
  assert.deepEqual(calls, [[0, 120_000], [PDF_PAGE_BYTES, 110_000], [PDF_PAGE_BYTES * 2, 100_000]]);
  assert.deepEqual(progress.at(-1), [data.length, data.length]);
});

test("PDF reader rejects scope substitution, malformed boundaries and missing private bytes", async () => {
  const data = new Uint8Array(10);
  for (const patch of [{ project: "other" }, { session_id: "other" }, { snapshot_id: "other" },
    { path: "other.pdf" }, { view: "content" }, { byte_offset: 1 }, { bytes_total: MAX_PDF_BYTES + 1 },
    { bytes_total: -1 }, { bytes_total: 0 }, { bytes_total: 10.5 }, { complete: false },
    { next_byte_offset: 10 }, { unavailable_reason: "invented" }]) {
    await assert.rejects(readPdf(options(async () => response(data, 0, patch))), /Invalid/);
  }
  for (const encoded of [undefined, "not base64", "", "YQ==", "A".repeat(PDF_PAGE_BYTES * 2)]) {
    await assert.rejects(readPdf(options(async () => ({ ...response(data, 0), encoded }))), /Invalid/);
  }
});

test("PDF reader stops on close, a changed size, and expiration rather than retargeting", async () => {
  const data = new Uint8Array(PDF_PAGE_BYTES + 10);
  let active = true, calls = 0;
  await assert.rejects(readPdf({ ...options(async offset => {
    calls++; active = false; return response(data, offset);
  }), current: () => active }), /closed/);
  assert.equal(calls, 1);
  calls = 0;
  await assert.rejects(readPdf(options(async offset => {
    calls++; return response(data, offset, calls === 1 ? {} : { bytes_total: data.length + 1 });
  })), /bounds/);
  let time = 0;
  await assert.rejects(readPdf({ ...options(async offset => {
    time = 120_001; return response(data, offset);
  }), now: () => time }), /timed out/);
});

test("unsupported PDFs return a bounded actionable reason without requesting another segment", async () => {
  let calls = 0;
  await assert.rejects(readPdf(options(async () => {
    calls++; return { page: { ...identity, view: "pdf", unavailable_reason: "too_large" } };
  })), /20 MiB/);
  assert.equal(calls, 1);
});
