import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { build } from "esbuild";
import { localhostHostValidation } from "@modelcontextprotocol/node";
import { createWorkResultFixtures } from "./work-result-fixtures.mjs";
export async function startWorkResultPreview(port = 0) {
  const store = await createWorkResultFixtures(), validateHost = localhostHostValidation();
  const host = await build({ entryPoints: [new URL("work-result-host.mjs", import.meta.url).pathname.replace(/^\/(\w:)/, "$1")], bundle: true,
    platform: "browser", target: "es2022", format: "esm", write: false });
  const server = createServer(async (req, res) => {
    if (!validateHost(req, res)) return;
    const origin = `http://${req.headers.host}`;
    if (req.headers.origin && req.headers.origin !== origin) { res.writeHead(403).end(); return; }
    const url = new URL(req.url, origin); res.setHeader("Cache-Control", "no-store");
    try {
      if (req.method === "POST" && url.pathname === "/tool") {
        let body = ""; for await (const chunk of req) { body += chunk; if (body.length > 4096) throw new Error("Request too large"); }
        const call = JSON.parse(body); if (call.name !== "get_work_result_state") throw new Error("Tool not admitted");
        res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(store.read(call.arguments))); return;
      }
      if (req.method !== "GET") { res.writeHead(405).end(); return; }
      if (url.pathname === "/present") { res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(store.present(url.searchParams.get("sample") || "text"))); return; }
      if (url.pathname === "/host.js") { res.writeHead(200, { "Content-Type": "text/javascript" }).end(host.outputFiles[0].text); return; }
      if (url.pathname === "/viewer") {
        res.setHeader("Content-Security-Policy", `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; font-src data: blob:; img-src data: blob:; connect-src 'none'; worker-src ${url.searchParams.get("policy") === "deny" ? "'none'" : "blob:"}; object-src 'none'; base-uri 'none'`);
        res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end(await readFile(new URL("../../src/mcp_work_result_app.html", import.meta.url))); return;
      }
      if (url.pathname === "/") {
        res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" }).end('<!doctype html><style>html,body{margin:0;height:100%;background:#f4f4f4}iframe{border:0;width:100%;height:100%;display:block}</style><iframe title="WebCodex PDF file preview" sandbox="allow-scripts allow-same-origin"></iframe><script type="module" src="/host.js"></script>'); return;
      }
      res.writeHead(404).end();
    } catch (error) { res.writeHead(400, { "Content-Type": "application/json" }).end(JSON.stringify({ isError: true, content: [{ type: "text", text: error.message }] })); }
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  return { url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }) };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const preview = await startWorkResultPreview(Number(process.env.PDF_POC_PORT || 0)); console.log(preview.url);
  for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, async () => { await preview.close(); process.exit(0); });
}
