import { createInterface } from "node:readline";
const lines = createInterface({ input: process.stdin, terminal: false });
for await (const line of lines) {
  const request = JSON.parse(line);
  if (request.id === undefined) continue;
  let result;
  if (request.method === "initialize") result = { protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "webcodex-native-dogfood", version: "1" } };
  else if (request.method === "tools/list") result = { tools: [{ name: "echo", description: "Real local MCP echo fixture", inputSchema: { type: "object", properties: { value: { type: "string" } }, required: ["value"], additionalProperties: false } }] };
  else if (request.method === "tools/call") result = { content: [{ type: "text", text: `mcp:${request.params.arguments.value}` }], structuredContent: { value: request.params.arguments.value }, isError: false };
  else if (request.method === "ping") result = {};
  else { process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "Method not found" } }) + "\n"); continue; }
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: request.id, result }) + "\n");
}
