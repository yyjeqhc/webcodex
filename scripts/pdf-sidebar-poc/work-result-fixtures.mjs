// Synthetic-only adapter for testing the shipped Work Result HTML in a browser
// or ChatGPT. Never accepts real Project paths or reads arbitrary files.
import { readFile } from "node:fs/promises";
import { McpServer } from "@modelcontextprotocol/server";
import { registerAppResource, registerAppTool, RESOURCE_MIME_TYPE } from "@modelcontextprotocol/ext-apps/server";
import { z } from "zod";
import { baseState, project } from "../../src/mcp_tests/work_result_app_fixture.mjs";

export const RESOURCE_URI = "ui://webcodex/work-result/v27";
const files = { text: "fixtures/text.pdf", cjk: "fixtures/cjk.pdf", scan: "fixtures/scan.pdf", ratio: "output/pdf/sidebar-match-630x496.pdf" };
export async function createWorkResultFixtures() {
  const documents = new Map();
  for (const [name, path] of Object.entries(files)) {
    const bytes = await readFile(new URL(path, import.meta.url));
    documents.set(`${name}.pdf`, { bytes });
  }
  function present(sample = "text") {
    if (!Object.hasOwn(files, sample)) throw new Error("Unknown synthetic sample");
    const state = structuredClone(baseState);
    for (const key of ["session_id", "session", "validation", "review"]) delete state[key];
    state.workspace.branch = "experiment/pdf-sidebar-poc";
    state.activity = { available: true, scope: "window", active: false, current: null, last: null, last_meaningful_activity_at_ms: null, coverage_partial: false };
    state.window_activity.events = []; state.window_activity.events_returned = state.window_activity.events_observed = 0;
    state.workspace.files = [...documents.keys()].map(path => ({ path, status: "added", kind: "tracked", staged: false, unstaged: true, binary: true }));
    state.workspace.files_total = documents.size;
    state.workspace.counts = { ...state.workspace.counts, modified: 0, added: documents.size };
    state.workspace.additions = state.workspace.deletions = 0;
    // Row navigation chooses the sample explicitly; all four files are synthetic.
    state.workspace.files.sort((a, b) => a.path === `${sample}.pdf` ? -1 : b.path === `${sample}.pdf` ? 1 : a.path.localeCompare(b.path));
    const structuredContent = { success: true, output: { work_result: state }, error: null };
    return { structuredContent, content: [{ type: "text", text: "Synthetic PDF Work Result preview." }],
      _meta: { "webcodex/workResult": structuredContent, "webcodex/workResultThread": { session_id: null } } };
  }
  function read(args) {
    if (args.project !== project || args.session_id != null) throw new Error("Synthetic Project mismatch");
    const request = args.files;
    if (!request) return present();
    if (!request.path) {
      if (request.offset || request.snapshot_id && request.snapshot_id !== "wc_changes_snapshot_" + "a".repeat(32)) throw new Error("Invalid inventory fence");
      return envelope({ project, session_id: null, snapshot_id: "wc_changes_snapshot_" + "a".repeat(32), offset: 0,
        next_offset: null, files_total: documents.size, source_truncated: false,
        files: [...documents.keys()].map(path => ({ path, kind: "added", additions: null, deletions: null, binary: true })) });
    }
    const file = documents.get(request.path);
    if (!file || request.snapshot_id !== "wc_changes_snapshot_" + "a".repeat(32)) throw new Error("Unadvertised synthetic path or snapshot");
    if (request.view !== "pdf") {
      return request.view === "content" ? envelope({ project, session_id: null, snapshot_id: request.snapshot_id,
        path: request.path, view: "content", unavailable_reason: "binary" })
        : envelope({ project, session_id: null, snapshot_id: request.snapshot_id, path: request.path, text: "Binary PDF file", bytes_total: 15, lines_total: 1, truncated: false });
    }
    const offset = request.byte_offset ?? 0;
    if (!Number.isSafeInteger(offset) || offset < 0 || offset >= file.bytes.length) throw new Error("Invalid PDF offset");
    const bytes = file.bytes.subarray(offset, offset + 128 * 1024), next = offset + bytes.length;
    const result = envelope({ project, session_id: null, snapshot_id: request.snapshot_id, path: request.path, view: "pdf",
      byte_offset: offset, bytes_total: file.bytes.length, next_byte_offset: next === file.bytes.length ? null : next,
      complete: next === file.bytes.length });
    result._meta = { "webcodex/pdfChunk": { content_base64: bytes.toString("base64") } }; return result;
  }
  return { present, read };
}
function envelope(page) {
  const structuredContent = { success: true, output: { work_result_files: page }, error: null };
  return { structuredContent, content: [{ type: "text", text: JSON.stringify(structuredContent) }] };
}
export async function createWorkResultServer() {
  const store = await createWorkResultFixtures();
  const server = new McpServer({ name: "webcodex-pdf-work-result-test", version: "0.2.0" });
  registerAppTool(server, "display_pdf_sample", {
    title: "WebCodex PDF file preview", description: "Open the shipped WebCodex Work Result viewer using synthetic PDF fixtures only: text, cjk, scan, ratio. No real Project files or URLs accepted.",
    inputSchema: z.object({ sample: z.enum(Object.keys(files)).default("text") }).strict(),
    annotations: { readOnlyHint: true, openWorldHint: false },
    _meta: { ui: { resourceUri: RESOURCE_URI }, "openai/ui": { entrypoints: [{ type: "global" }, { type: "thread" }] } },
  }, async ({ sample }) => store.present(sample));
  registerAppTool(server, "get_work_result_state", {
    description: "App-only synthetic Work Result snapshot/PDF chunk read.",
    inputSchema: z.object({ project: z.literal(project), files: z.object({ snapshot_id: z.string().optional(), path: z.string().optional(),
      view: z.enum(["content", "pdf"]).optional(), byte_offset: z.number().int().nonnegative().optional(), offset: z.number().int().nonnegative().optional() }).strict().optional() }).strict(),
    annotations: { readOnlyHint: true, openWorldHint: false }, _meta: { ui: { visibility: ["app"] } },
  }, async args => store.read(args));
  registerAppResource(server, "WebCodex PDF file preview", RESOURCE_URI, { mimeType: RESOURCE_MIME_TYPE }, async () => ({
    contents: [{ uri: RESOURCE_URI, mimeType: RESOURCE_MIME_TYPE, text: await readFile(new URL("../../src/mcp_work_result_app.html", import.meta.url), "utf8"),
      _meta: { ui: { csp: { connectDomains: [], resourceDomains: [] } }, "openai/widgetPrefersBorder": true } }],
  }));
  return server;
}
