// Browser harness for the shipped PDF App. Fixed public synthetic files only.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { localhostHostValidation } from "@modelcontextprotocol/node";

export async function startPdfDocumentPreview(port = 0) {
  const paths = { text: "fixtures/text.pdf", cjk: "fixtures/cjk.pdf", scan: "fixtures/scan.pdf", ratio: "output/pdf/sidebar-match-630x496.pdf" };
  const files = new Map();
  for (const [sample, path] of Object.entries(paths)) {
    const data = await readFile(new URL(path, import.meta.url));
    files.set(sample, { data, identity: { project: "agent:pdf:synthetic", path: `${sample}.pdf`, name: `${sample}.pdf`, bytes: data.length, sha256: createHash("sha256").update(data).digest("hex") } });
  }
  const host = await build({ entryPoints: [new URL("pdf-document-host.mjs", import.meta.url).pathname.replace(/^\/(\w:)/, "$1")], bundle: true, platform: "browser", target: "es2022", format: "esm", write: false });
  const validateHost = localhostHostValidation();
  const server = createServer(async (req, res) => {
    if (!validateHost(req, res)) return;
    const origin = `http://${req.headers.host}`;
    if (req.headers.origin && req.headers.origin !== origin) { res.writeHead(403).end(); return; }
    const url = new URL(req.url, origin); res.setHeader("Cache-Control", "no-store");
    const json = value => res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(value));
    try {
      if (req.method === "POST" && url.pathname === "/tool") {
        let body = ""; for await (const chunk of req) { body += chunk; if (body.length > 4096) throw new Error("Request too large"); }
        const call = JSON.parse(body), args = call.arguments, file = files.get(args?.path?.replace(/\.pdf$/, ""));
        if (call.name !== "read_pdf_chunk" || !file || ["project", "path", "sha256", "bytes"].some(key => args[key] !== file.identity[key])) throw new Error("Unknown synthetic document");
        if (!Number.isSafeInteger(args.byte_offset) || args.byte_offset < 0 || args.byte_offset >= file.data.length) throw new Error("Invalid offset");
        if (url.searchParams.get("stale") === "1") return json({ structuredContent: { success: false, output: { error_kind: "snapshot_changed" } } });
        const next = Math.min(args.byte_offset + 128 * 1024, file.data.length);
        const structuredContent = { success: true, output: { pdf_chunk: { project: args.project, path: args.path, sha256: args.sha256,
          bytes_total: args.bytes, byte_offset: args.byte_offset, complete: next === args.bytes, next_byte_offset: next === args.bytes ? null : next } } };
        return json({ ...(url.searchParams.get("fallback") === "1" ? {} : { structuredContent }), content: [{ type: "text", text: JSON.stringify(structuredContent) }], _meta: { "webcodex/pdfChunk": { content_base64: file.data.subarray(args.byte_offset, next).toString("base64") } } });
      }
      if (req.method !== "GET") { res.writeHead(405).end(); return; }
      if (url.pathname === "/present") {
        const file = files.get(url.searchParams.get("sample") || "cjk"); if (!file) throw new Error("Unknown sample");
        const structuredContent = { success: true, output: { pdf_document: file.identity } };
        return json({ ...(url.searchParams.get("fallback") === "1" ? {} : { structuredContent }), content: [{ type: "text", text: "PDF reader opened" }], _meta: { "webcodex/pdfDocument": structuredContent } });
      }
      if (url.pathname === "/host.js") return res.writeHead(200, { "Content-Type": "text/javascript" }).end(host.outputFiles[0].text);
      if (url.pathname === "/viewer") {
        res.setHeader("Content-Security-Policy", `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data: blob:; img-src data: blob:; connect-src 'none'; worker-src ${url.searchParams.get("policy") === "deny" ? "'none'" : "blob:"}; object-src 'none'; base-uri 'none'`);
        return res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end(await readFile(new URL("../../src/mcp_pdf_app.html", import.meta.url)));
      }
      if (url.pathname === "/") return res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end('<!doctype html><style>html,body{margin:0;height:100%;background:#f3f4f5}iframe{border:0;width:100%;height:100%;display:block}</style><iframe title="PDF document reader" sandbox="allow-scripts allow-same-origin"></iframe><script type="module" src="/host.js"></script>');
      res.writeHead(404).end();
    } catch (error) { res.writeHead(400, { "Content-Type": "application/json" }).end(JSON.stringify({ isError: true, content: [{ type: "text", text: error.message }] })); }
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  return { url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }) };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const preview = await startPdfDocumentPreview(Number(process.env.PDF_DOCUMENT_PORT || 0)); console.log(preview.url);
  for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, async () => { await preview.close(); process.exit(0); });
}
