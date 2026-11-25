import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { privateToolMetadata } from "../src/mcp-apps/work-result/pdf-document-reader.mjs";

// The Host runs the generated bundle. This executes the same reader source,
// with PDF.js replaced so a projection can be observed without a browser.
const source = readFileSync(new URL("../src/mcp-apps/work-result/pdf-document.mjs", import.meta.url), "utf8")
  .replace(/^import .*;\r?\n/gm, "");
const flush = () => new Promise(resolve => setImmediate(resolve));
const sha = "ab".repeat(32);
const pdfDocument = {
  project: "agent:pdf:demo",
  path: "reports/brief.pdf",
  name: "brief.pdf",
  sha256: sha,
  bytes: 128,
};
const full = { success: true, output: { pdf_document: pdfDocument } };
const page = {
  project: pdfDocument.project, path: pdfDocument.path, sha256: pdfDocument.sha256,
  bytes_total: pdfDocument.bytes, byte_offset: 0, next_byte_offset: null, complete: true,
};

function reader() {
  const nodes = {}, sent = [], listeners = new Map();
  const node = () => ({
    textContent: "", hidden: false, title: "", dataset: {}, children: [], childNodes: [],
    style: { setProperty() {} },
    replaceChildren(...children) { this.children = children; },
  });
  const document = {
    documentElement: node(),
    getElementById: id => nodes[id] ||= node(),
  };
  const parent = { postMessage: message => { sent.push(message); } };
  const createPreview = (host, options) => ({
    load: async () => {
      try {
        const result = await options.request(0, 20_000);
        host.dataset.page = result.page;
        host.dataset.encoded = result.encoded;
      } catch (error) {
        host.dataset.error = error.message;
        options.failed?.();
      }
    },
    destroy() {},
  });
  runInNewContext(source, {
    document, parent, createPreview, privateToolMetadata,
    readPdfDocument: async () => { throw new Error("unused"); },
    AbortController,
    addEventListener: (name, callback) => listeners.set(name, callback),
    setTimeout: () => 0,
    clearTimeout() {},
  });
  const deliver = message => listeners.get("message")({ source: parent, data: { jsonrpc: "2.0", ...message } });
  return {
    nodes, sent,
    chunks: () => sent.filter(message => message.method === "tools/call"),
    present(value) {
      let error = null;
      try { deliver({ method: "ui/notifications/tool-result", params: value }); }
      catch (caught) { error = caught; }
      return error;
    },
    reply(request, value) { deliver({ id: request.id, result: value }); },
  };
}

test("a canonical structured PDF envelope still opens the document", () => {
  const view = reader();
  const error = view.present({ structuredContent: full });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 1);
  assert.equal(view.chunks()[0].params.name, "read_app_artifact_chunk");
  assert.equal(view.chunks()[0].params.arguments.path, pdfDocument.path);
  assert.equal(view.chunks()[0].params.arguments.sha256, sha);
  assert.equal(view.chunks()[0].params.arguments.bytes, pdfDocument.bytes);
  assert.equal(view.nodes.filename.textContent, pdfDocument.name);
});

test("standard text opens a PDF when the host omits structured content and private metadata", () => {
  const view = reader();
  const error = view.present({ content: [{ type: "text", text: JSON.stringify(full) }] });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 1);
  assert.equal(view.chunks()[0].params.arguments.path, pdfDocument.path);
});

test("a canonical present_pdf failure does not open a document", () => {
  const view = reader();
  const error = view.present({ structuredContent: { success: false, error: "present failed" } });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 0);
  assert.match(view.nodes.empty.textContent, /present_pdf 未成功/);
});

test("a non-canonical host structuredContent does not mask the private PDF envelope", () => {
  const view = reader();
  const error = view.present({
    structuredContent: { projected: true },
    _meta: { "webcodex/pdfDocument": full },
  });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 1);
  assert.equal(view.chunks()[0].params.arguments.path, pdfDocument.path);
  assert.equal(view.chunks()[0].params.arguments.sha256, sha);
  assert.equal(view.chunks()[0].params.arguments.bytes, pdfDocument.bytes);
  assert.equal(view.nodes.filename.textContent, pdfDocument.name);
  assert.doesNotMatch(view.nodes.empty.textContent, /不可用|无效/);
});

test("a canonical host projection does not mask the private PDF envelope", () => {
  const view = reader();
  const error = view.present({
    structuredContent: { success: true, output: { name: "short.pdf" } },
    content: { type: "text" },
    _meta: { "webcodex/pdfDocument": full },
  });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 1);
  assert.equal(view.chunks()[0].params.arguments.path, pdfDocument.path);
  assert.equal(view.chunks()[0].params.arguments.sha256, sha);
  assert.doesNotMatch(view.nodes.empty.textContent, /不可用|无效/);
});

for (const key of ["result", "toolResult", "tool_result"]) {
  test(`nested ${key} server text outranks outer canonical PDF projection`, () => {
    const view = reader();
    const error = view.present({
      structuredContent: { success: true, output: { name: "short.pdf" } },
      [key]: { content: [{ type: "text", text: JSON.stringify(full) }] },
    });
    assert.equal(error, null);
    assert.equal(view.chunks().length, 1);
    assert.equal(view.chunks()[0].params.arguments.path, pdfDocument.path);
  });
}

test("nested PDF server failure outranks outer Host success", () => {
  const view = reader();
  const error = view.present({
    structuredContent: full,
    toolResult: { content: [{ type: "text", text: JSON.stringify({ success: false, error: "read denied" }) }] },
  });
  assert.equal(error, null);
  assert.equal(view.chunks().length, 0);
  assert.match(view.nodes.empty.textContent, /未成功/);
});

test("server text wins when a host projection drops the PDF chunk offset", async () => {
  const view = reader();
  const error = view.present({ _meta: { "webcodex/pdfDocument": full } });
  assert.equal(error, null);
  const request = view.chunks()[0];
  const canonical = { success: true, output: { artifact_chunk: page } };
  view.reply(request, {
    structuredContent: { success: true, output: { artifact_chunk: { ...page, byte_offset: 1 } } },
    content: [{ type: "text", text: JSON.stringify(canonical) }],
    _meta: { "webcodex/artifactChunk": { content_base64: "JVBERg==" } },
  });
  await flush();
  assert.equal(view.nodes.reader.dataset.error, undefined);
  assert.equal(view.nodes.reader.dataset.page.byte_offset, 0);
  assert.equal(view.nodes.reader.dataset.encoded, "JVBERg==");
});
