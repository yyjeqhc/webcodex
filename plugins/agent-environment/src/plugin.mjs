import { Console } from "node:console";
import { createHash } from "node:crypto";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { definePlugin, defineTool, errorResult, schema, servePlugin, textResult } from "@yyjeqhc/webcodex-plugin-sdk";
import { AgentEnvironmentBridge } from "./bridge.mjs";

// stdout belongs exclusively to native Plugin JSON-RPC. Ordinary extension
// console output goes to Runner's existing bounded stderr drain, never the model.
globalThis.console = new Console({ stdout: process.stderr, stderr: process.stderr });
const { values } = parseArgs({ options: { "agent-dir": { type: "string" }, "state-dir": { type: "string" } }, strict: true });
if (values["agent-dir"]) process.env.PI_CODING_AGENT_DIR = resolve(values["agent-dir"]);
const { getAgentDir } = await import("@earendil-works/pi-coding-agent");
const { createPiEnvironment } = await import("./pi.mjs");
const cwd = process.cwd();
const stateDir = values["state-dir"] ?? join(homedir(), ".webcodex", "agent-environments", createHash("sha256").update(cwd).digest("hex").slice(0, 24));
const bridge = new AgentEnvironmentBridge(() => createPiEnvironment({ cwd, agentDir: getAgentDir(), stateDir: resolve(stateDir) }));
const id = () => schema.string({ minLength: 1, maxLength: 128 });
const nativeName = () => schema.string({ minLength: 1, maxLength: 256 });
const tool = defineTool({
  name: "agent_environment",
  title: "Native Agent Tool Environment",
  description: "Pi-first native environment gateway. Start with sessions, then tools/describe, then call using exact session, next_request, and per-tool binding. Dynamic native tools are NOT outer Plugin/MCP tools. Native-orchestrator-only tools require Pi's configured codemode/tool-search. Calls run inside Pi's native tool pipeline, not a second LLM. Project/cwd binding is NOT a sandbox. Never replay an outcome_unknown request; inspect history and reconcile effects. refresh uses native session reload without restarting WebCodex.",
  projectBound: true,
  inputSchema: schema.object({
    action: schema.string({ enum: ["sessions", "tools", "describe", "call", "refresh", "history"] }),
    session: schema.optional(id()), tool: schema.optional(nativeName()), request: schema.optional(id()),
    query: schema.optional(schema.string({ maxLength: 128 })), cursor: schema.optional(id()),
    limit: schema.optional(schema.integer()), timeout_ms: schema.optional(schema.integer()), deadline: schema.optional(schema.integer()),
    calls: schema.optional(schema.array(schema.object({ id: id(), tool: nativeName(), binding: id(), arguments: schema.object({}, { additionalProperties: true }) }), { minItems: 1, maxItems: 16 })),
  }),
  annotations: { readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true },
  async execute(input) {
    const result = await bridge.execute(input);
    const failed = Boolean(result.error) || result.results?.some((call) => call.is_error);
    const caption = `Native ${result.native_source ?? "agent"} environment: ${result.native_dispatch_state}${result.error ? ` (${result.error}; do not replay)` : ""}.`;
    return failed ? errorResult(caption, result) : textResult(caption, result);
  },
});

let terminating = false;
const terminate = () => {
  if (terminating) return;
  terminating = true;
  const timer = setTimeout(() => process.exit(1), 1500);
  timer.unref();
  void bridge.close().finally(() => process.exit(0));
};
process.once("SIGTERM", terminate);
process.once("SIGINT", terminate);
try { await servePlugin(definePlugin({ tools: [tool] }), { input: process.stdin, output: process.stdout, error: process.stderr }); }
catch { process.stderr.write("Agent environment Plugin transport failed.\n"); process.exitCode = 1; }
finally { await bridge.close(); }
