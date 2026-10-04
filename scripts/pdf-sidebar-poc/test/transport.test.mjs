import test from "node:test";
import assert from "node:assert/strict";
import { Client, InMemoryTransport, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { createFixtureStore, createPdfServer, RESOURCE_URI, CHUNK_BYTES } from "../mcp-server.mjs";
import { startPreview } from "../preview-server.mjs";
import { startChatgptServer } from "../chatgpt-server.mjs";

test("fixed snapshots, bounded chunks and component-only binary output", async () => {
  const store = await createFixtureStore();
  for (const sample of ["text", "cjk", "scan", "ratio"]) {
    const info = store.open({ sample }).structuredContent;
    const result = store.read({ sample, snapshot: info.snapshot, offset: 0 });
    const bytes = Buffer.from(result._meta.pdfChunk.base64, "base64");
    assert.equal(bytes.subarray(0, 5).toString(), "%PDF-");
    assert.ok(bytes.length <= CHUNK_BYTES);
    assert.equal(result._meta.pdfChunk.totalBytes, info.totalBytes);
    assert.equal(result.structuredContent, undefined);
    assert.ok(!JSON.stringify(result.content).includes(result._meta.pdfChunk.base64));
    assert.throws(() => store.read({ sample, snapshot: "0".repeat(64), offset: 0 }), /snapshot mismatch/);
    assert.throws(() => store.read({ sample, snapshot: info.snapshot, offset: info.totalBytes }), /out of range/);
    assert.throws(() => store.read({ sample, snapshot: info.snapshot, offset: -1 }));
    const chunks = [];
    for (let offset = 0; offset < info.totalBytes;) {
      const chunk = store.read({ sample, snapshot: info.snapshot, offset })._meta.pdfChunk;
      const data = Buffer.from(chunk.base64, "base64"); chunks.push(data); offset += data.length;
    }
    assert.equal(Buffer.concat(chunks).length, info.totalBytes);
    if (sample === "scan") assert.ok(chunks.length > 1, "scan fixture must exercise multiple chunks");
  }
  assert.throws(() => store.open({ sample: "../../private" }));
  assert.throws(() => store.open({ sample: "text", path: "secret.pdf" }));
});

test("MCP SDK handshake, resource metadata and tool bindings", async () => {
  const server = await createPdfServer();
  const client = new Client({ name: "pdf-poc-test", version: "0.1.0" }, { capabilities: { extensions: { "io.modelcontextprotocol/ui": {} } } });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  try {
    await server.connect(serverTransport); await client.connect(clientTransport);
    const tools = (await client.listTools()).tools;
    assert.equal(tools.find(tool => tool.name === "display_pdf_sample")._meta.ui.resourceUri, RESOURCE_URI);
    assert.deepEqual(tools.find(tool => tool.name === "display_pdf_sample")._meta["openai/ui"].entrypoints,
      [{ type: "global" }, { type: "thread" }]);
    assert.deepEqual(tools.find(tool => tool.name === "read_pdf_sample_bytes")._meta.ui.visibility, ["app"]);
    const resource = (await client.readResource({ uri: RESOURCE_URI })).contents[0];
    assert.deepEqual(resource._meta.ui.csp, { connectDomains: [], resourceDomains: [], frameDomains: [] });
    assert.deepEqual(resource._meta["openai/ui"], {
      availableDisplayModes: ["inline", "fullscreen"], preferredDisplayMode: "fullscreen",
    });
    assert.equal(resource.mimeType, "text/html;profile=mcp-app");
    assert.ok(resource.text.includes("PDF 预览实验"));
    const info = (await client.callTool({ name: "display_pdf_sample", arguments: { sample: "text" } })).structuredContent;
    const chunk = await client.callTool({ name: "read_pdf_sample_bytes", arguments: { sample: "text", snapshot: info.snapshot, offset: 0 } });
    assert.ok(chunk._meta.pdfChunk.base64.length);
  } finally { await client.close(); await server.close(); }
});

test("ChatGPT ingress admits only bounded MCP fixture requests", async () => {
  const ingress = await startChatgptServer();
  const client = new Client({ name: "chatgpt-ingress-test", version: "0.1.0" });
  try {
    assert.equal((await fetch(ingress.url.replace("/mcp", "/viewer"))).status, 404);
    assert.equal((await fetch(ingress.url, { method: "POST", headers: { Origin: "https://external.invalid" }, body: "{}" })).status, 403);
    assert.equal((await fetch(ingress.url, { method: "POST", body: "x".repeat(16385) })).status, 413);
    await client.connect(new StreamableHTTPClientTransport(new URL(ingress.url)));
    const info = (await client.callTool({ name: "display_pdf_sample", arguments: {} })).structuredContent;
    assert.equal(info.sample, "text");
    assert.ok((await client.readResource({ uri: RESOURCE_URI })).contents[0].text.includes("PDF 预览实验"));
  } finally { await client.close(); await ingress.close(); }
});

test("real stateless HTTP MCP and loopback request guards", async () => {
  const preview = await startPreview();
  const client = new Client({ name: "pdf-http-test", version: "0.1.0" });
  try {
    const denied = await fetch(`${preview.url}/tool`, { method: "POST", headers: { Origin: "https://external.invalid" }, body: "{}" });
    assert.equal(denied.status, 403);
    const unknown = await fetch(`${preview.url}/../../secret`);
    assert.equal(unknown.status, 404);
    await client.connect(new StreamableHTTPClientTransport(new URL(`${preview.url}/mcp`)));
    const result = await client.callTool({ name: "display_pdf_sample", arguments: { sample: "scan" } });
    assert.equal(result.structuredContent.sample, "scan");
  } finally { await client.close(); await preview.close(); }
});
