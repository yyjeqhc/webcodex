// Separate fixture-only ingress. Never exposes the local harness or arbitrary files.
import { createServer } from "node:http";
import { pathToFileURL } from "node:url";
import { NodeStreamableHTTPServerTransport, localhostHostValidation } from "@modelcontextprotocol/node";
import { createPdfServer } from "./mcp-server.mjs";
import { createWorkResultServer } from "./work-result-fixtures.mjs";

export async function startChatgptServer(port = 0) {
  const validateHost = localhostHostValidation();
  const server = createServer(async (req, res) => {
    if (!validateHost(req, res)) return;
    if (req.headers.origin) { res.writeHead(403).end(); return; }
    if (req.url !== "/mcp") { res.writeHead(404).end(); return; }
    if (req.method !== "POST") { res.writeHead(405, { Allow: "POST" }).end(); return; }
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("X-Content-Type-Options", "nosniff");
    let mcp;
    try {
      const chunks = []; let size = 0;
      for await (const chunk of req) {
        size += chunk.length;
        if (size > 16384) { res.writeHead(413).end(); return; }
        chunks.push(chunk);
      }
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      mcp = await (process.env.PDF_POC_WORK_RESULT === "1" ? createWorkResultServer() : createPdfServer());
      const transport = new NodeStreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true });
      await mcp.connect(transport);
      res.on("close", () => { mcp.close().catch(() => {}); });
      await transport.handleRequest(req, res, body);
    } catch {
      if (!res.headersSent) res.writeHead(400).end(); else res.end();
      await mcp?.close().catch(() => {});
    }
  });
  server.requestTimeout = 15000;
  server.headersTimeout = 10000;
  await new Promise((resolve, reject) => { server.once("error", reject); server.listen(port, "127.0.0.1", resolve); });
  return { server, url: `http://127.0.0.1:${server.address().port}/mcp`, close: () => new Promise((resolve, reject) => {
    server.close(error => error ? reject(error) : resolve()); server.closeAllConnections();
  }) };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const fixture = await startChatgptServer(Number(process.env.PDF_POC_CHATGPT_PORT || 0));
  console.log(`Fixture-only MCP: ${fixture.url}`);
  for (const signal of ["SIGINT", "SIGTERM"]) process.once(signal, async () => { await fixture.close(); process.exit(0); });
}
