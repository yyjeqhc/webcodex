import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { McpServer } from "@modelcontextprotocol/server";
import { registerAppResource, registerAppTool, RESOURCE_MIME_TYPE } from "@modelcontextprotocol/ext-apps/server";
import { z } from "zod";

export const RESOURCE_URI = "ui://webcodex/pdf-feasibility-v2.html";
export const CHUNK_BYTES = 256 * 1024;
const fixtures = Object.freeze({ text: "fixtures/text.pdf", cjk: "fixtures/cjk.pdf", scan: "fixtures/scan.pdf",
  ratio: "output/pdf/sidebar-match-630x496.pdf" });
const sampleSchema = z.enum(Object.keys(fixtures));
const openSchema = z.object({ sample: sampleSchema.default("text") }).strict();
const readSchema = z.object({ sample: sampleSchema, snapshot: z.string().regex(/^[a-f0-9]{64}$/),
  offset: z.number().int().min(0).max(20 * 1024 * 1024) }).strict();
export async function createFixtureStore() {
  const samples = new Map();
  for (const [sample, path] of Object.entries(fixtures)) {
    const bytes = await readFile(new URL(path, import.meta.url));
    if (!bytes.length || bytes.length > 20 * 1024 * 1024) throw new Error("Invalid fixture size");
    samples.set(sample, { bytes, name: path.split("/").at(-1), snapshot: createHash("sha256").update(bytes).digest("hex") });
  }
  return {
    open(args) {
      const { sample } = openSchema.parse(args);
      const { bytes, name, snapshot } = samples.get(sample);
      return { content: [{ type: "text", text: `Opened synthetic ${sample} PDF in the experiment viewer.` }],
        structuredContent: { sample, snapshot, totalBytes: bytes.length, name } };
    },
    read(args) {
      const { sample, snapshot, offset } = readSchema.parse(args);
      const file = samples.get(sample);
      if (file.snapshot !== snapshot) throw new Error("PDF snapshot mismatch");
      if (offset >= file.bytes.length) throw new Error("PDF offset out of range");
      return { content: [{ type: "text", text: "PDF chunk delivered to viewer." }],
        _meta: { pdfChunk: { snapshot, offset, totalBytes: file.bytes.length,
          base64: file.bytes.subarray(offset, offset + CHUNK_BYTES).toString("base64") } } };
    },
  };
}
export async function createPdfServer() {
  const store = await createFixtureStore();
  const server = new McpServer({ name: "webcodex-pdf-feasibility", version: "0.1.0" });
  registerAppTool(server, "display_pdf_sample", {
    title: "PDF feasibility sample", description: "Open one synthetic PDF sample: text, cjk, scan, or ratio (630 x 496 pt geometric calibration). Read-only; no project files or URLs accepted.",
    inputSchema: openSchema, annotations: { readOnlyHint: true, openWorldHint: false },
    _meta: { ui: { resourceUri: RESOURCE_URI },
      "openai/ui": { entrypoints: [{ type: "global" }, { type: "thread" }] } },
  }, async args => store.open(args));
  registerAppTool(server, "read_pdf_sample_bytes", {
    description: "Read a bounded chunk of the exact synthetic PDF snapshot for the viewer.",
    inputSchema: readSchema, annotations: { readOnlyHint: true, openWorldHint: false },
    _meta: { ui: { visibility: ["app"] } },
  }, async args => store.read(args));
  registerAppResource(server, "PDF feasibility viewer", RESOURCE_URI, { mimeType: RESOURCE_MIME_TYPE }, async () => ({
    contents: [{ uri: RESOURCE_URI, mimeType: RESOURCE_MIME_TYPE,
      text: await readFile(new URL("dist/viewer.html", import.meta.url), "utf8"),
      _meta: { ui: { prefersBorder: true, csp: { connectDomains: [], resourceDomains: [], frameDomains: [] } },
        "openai/ui": { availableDisplayModes: ["inline", "fullscreen"], preferredDisplayMode: "fullscreen" } } }],
  }));
  return server;
}
