import { appendFileSync } from "node:fs";
import { join } from "node:path";
const parameters = { type: "object", properties: { value: { type: "string" } }, required: ["value"], additionalProperties: false };
export default function (pi) {
  const trace = join(process.env.PI_CODING_AGENT_DIR, "native-hooks.jsonl");
  pi.on("session_start", () => { appendFileSync(trace, JSON.stringify({ event: "session_start" }) + "\n"); });
  pi.on("tool_call", (event) => {
    appendFileSync(trace, JSON.stringify({ event: "tool_call", tool: event.toolName, id: event.toolCallId }) + "\n");
    if (event.toolName === "native_echo" && event.input.value === "denied") return { block: true, reason: "native permission fixture denied this call" };
  });
  pi.on("tool_result", (event) => { appendFileSync(trace, JSON.stringify({ event: "tool_result", tool: event.toolName, id: event.toolCallId }) + "\n"); });
  for (const [name, exposure, defaultActive] of [["native_echo", "direct", true], ["native_model", "model-only", true], ["native_code", "codemode", false], ["native_deferred", "deferred", false], ["native_hidden", "hidden", false], ["native_off", "direct", false]]) {
    pi.registerTool({ name, label: name, description: `Native ${exposure} dogfood tool`, parameters, exposure, defaultActive,
      async execute(id, args) { return { content: [{ type: "text", text: `${name}:${args.value}` }], details: { toolCallId: id } }; } });
  }
  pi.registerTool({ name: "native_fail", label: "Native failure", description: "Intentional native failure", parameters: { type: "object", properties: {}, additionalProperties: false },
    async execute() { throw new Error("intentional_native_failure"); } });
  pi.registerTool({ name: "native_slow", label: "Native delayed effect", description: "Local dogfood deliberately ignores cancellation", parameters: { type: "object", properties: {}, additionalProperties: false },
    async execute() {
      appendFileSync(trace, JSON.stringify({ event: "slow_started" }) + "\n");
      await new Promise((resolve) => setTimeout(resolve, 600));
      appendFileSync(trace, JSON.stringify({ event: "slow_effect" }) + "\n");
      return { content: [{ type: "text", text: "delayed_effect" }] };
    } });
}
