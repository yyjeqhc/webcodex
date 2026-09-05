// Minimal WebCodex Native Tool Plugin. No MCP SDK and no third-party package.
// Protocol: one JSON-RPC 2.0 request/response per line on stdin/stdout.

import readline from "node:readline";

const PROTOCOL_VERSION = "webcodex-plugin-v1";

const tools = [
  {
    name: "echo",
    description: "Echo one string",
    inputSchema: {
      type: "object",
      properties: { text: { type: "string" } },
      required: ["text"],
      additionalProperties: false,
    },
  },
  {
    name: "add",
    description: "Add two numbers",
    inputSchema: {
      type: "object",
      properties: {
        a: { type: "number" },
        b: { type: "number" },
      },
      required: ["a", "b"],
      additionalProperties: false,
    },
  },
];

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function result(id, value) {
  send({ jsonrpc: "2.0", id, result: value });
}

function rpcError(id, code, message) {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

function textResult(text, structuredContent) {
  return {
    content: [{ type: "text", text }],
    structuredContent,
    isError: false,
  };
}

const input = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
  terminal: false,
});

input.on("line", (line) => {
  let request;
  try {
    request = JSON.parse(line);
  } catch {
    // stdout is protocol-only. Local diagnostics belong on stderr.
    console.error("received malformed JSON");
    return;
  }

  const { id, method, params = {} } = request;
  if (request.jsonrpc !== "2.0" || id === undefined) {
    rpcError(id ?? null, -32600, "invalid request");
    return;
  }

  if (method === "initialize") {
    if (params.protocolVersion !== PROTOCOL_VERSION) {
      rpcError(id, -32602, "unsupported protocol version");
      return;
    }
    result(id, { protocolVersion: PROTOCOL_VERSION });
    return;
  }

  if (method === "tools/list") {
    result(id, { tools });
    return;
  }

  if (method === "tools/call") {
    const args = params.arguments ?? {};
    if (params.name === "echo" && typeof args.text === "string") {
      result(id, textResult(args.text, { text: args.text }));
      return;
    }
    if (
      params.name === "add" &&
      typeof args.a === "number" &&
      typeof args.b === "number"
    ) {
      const sum = args.a + args.b;
      result(id, textResult(String(sum), { sum }));
      return;
    }
    result(id, {
      content: [{ type: "text", text: "unknown tool or invalid arguments" }],
      structuredContent: {},
      isError: true,
    });
    return;
  }

  rpcError(id, -32601, "method not found");
});
