import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { NodeStreamableHTTPServerTransport, localhostHostValidation } from "@modelcontextprotocol/node";
import { createFixtureStore, createPdfServer } from "./mcp-server.mjs";

export async function startPreview(port = 0) {
  const store = await createFixtureStore();
  const validateHost = localhostHostValidation();
  const server = createServer(async (req, res) => {
    if (!validateHost(req, res)) return;
    const origin = `http://${req.headers.host}`;
    if (req.headers.origin && req.headers.origin !== origin) { res.writeHead(403).end(); return; }
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("X-Content-Type-Options", "nosniff");
    const url = new URL(req.url, origin);
    try {
      if (url.pathname === "/mcp" && req.method === "POST") {
        const mcp = await createPdfServer();
        const transport = new NodeStreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true });
        await mcp.connect(transport);
        res.on("close", () => { mcp.close().catch(() => {}); });
        await transport.handleRequest(req, res); return;
      }
      if (url.pathname === "/tool" && req.method === "POST") {
        let body = "";
        for await (const chunk of req) { body += chunk; if (body.length > 4096) { res.writeHead(413).end(); return; } }
        const request = JSON.parse(body);
        const result = request.name === "display_pdf_sample" ? store.open(request.arguments)
          : request.name === "read_pdf_sample_bytes" ? store.read(request.arguments) : null;
        if (!result) { res.writeHead(404).end(); return; }
        res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(result)); return;
      }
      if (req.method !== "GET") { res.writeHead(405).end(); return; }
      const files = { "/": ["host.html", "text/html; charset=utf-8"],
        "/host.js": ["dist/host.js", "text/javascript; charset=utf-8"], "/viewer": ["dist/viewer.html", "text/html; charset=utf-8"] };
      const file = files[url.pathname];
      if (!file) { res.writeHead(404).end(); return; }
      if (url.pathname === "/viewer") {
        // A deliberately strict test policy; this is NOT ChatGPT's actual CSP.
        res.setHeader("Content-Security-Policy", ["default-src 'none'", "script-src 'unsafe-inline'",
          "style-src 'unsafe-inline'", "font-src data: blob:", "img-src data: blob:", "connect-src 'none'",
          `worker-src ${url.searchParams.get("policy") === "deny" ? "'none'" : "blob:"}`, "frame-src 'none'", "object-src 'none'", "base-uri 'none'"].join("; "));
      } else res.setHeader("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'unsafe-inline'; connect-src 'self'; frame-src 'self'; object-src 'none'; base-uri 'none'");
      res.writeHead(200, { "Content-Type": file[1] }).end(await readFile(new URL(file[0], import.meta.url)));
    } catch (error) {
      if (!res.headersSent) res.writeHead(400, { "Content-Type": "application/json" }).end(JSON.stringify({ error: error.message }));
      else res.end();
    }
  });
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  return { server, url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((resolve, reject) => {
    server.close(error => error ? reject(error) : resolve()); server.closeAllConnections();
  }) };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const preview = await startPreview(Number(process.env.PDF_POC_PORT || 0));
  console.log(`PDF preview: ${preview.url}\nMCP endpoint: ${preview.url}/mcp`);
  for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, async () => { await preview.close(); process.exit(0); });
}
