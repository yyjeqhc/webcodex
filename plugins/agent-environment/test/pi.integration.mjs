import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, copyFile, writeFile, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";
import { PluginClient } from "./plugin-client.mjs";

const fixture = (name) => fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));
const readable = (result) => JSON.stringify(result);

test("real Pi native Plugin lifecycle: builtins/extensions/MCP/hooks/batch/reload/deadline", { timeout: 120000 }, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "webcodex-pi-native-"));
  const cwd = join(root, "project"), agentDir = join(root, "pi-config"), stateDir = join(root, "bridge-state");
  await Promise.all([mkdir(cwd), mkdir(join(agentDir, "extensions"), { recursive: true }), mkdir(stateDir)]);
  await copyFile(fixture("extension.mjs"), join(agentDir, "extensions", "fixture.js"));
  const settings = JSON.stringify({ retry: { enabled: true }, compaction: { enabled: true }, defaultTools: ["read", "bash", "edit", "write", "codemode", "tool_search"] });
  const authSentinel = JSON.stringify({ unused: { type: "api_key", key: "DO_NOT_DELEGATE_LLM_CREDENTIAL" } });
  await writeFile(join(agentDir, "settings.json"), settings);
  await writeFile(join(agentDir, "auth.json"), authSentinel);
  await writeFile(join(agentDir, "mcp.json"), JSON.stringify({ mcpServers: { fixture: { command: process.execPath, args: [fixture("mcp-server.mjs")], exposure: "direct" } } }));
  const client = new PluginClient({ cwd, agentDir, stateDir });
  let status;
  const invoke = async (action, extra = {}) => {
    const result = await client.action({ action, ...(action === "sessions" ? {} : { session: status.session }), ...extra });
    if (result.next_request) status = { ...status, next_request: result.next_request };
    return result;
  };
  const describe = async (name) => {
    const result = await invoke("describe", { tool: name });
    assert(!result.error, readable(result));
    return result.tool;
  };
  const batch = async (entries, extra = {}) => {
    const calls = [];
    for (const [index, entry] of entries.entries()) {
      const definition = await describe(entry[0]);
      calls.push({ id: `call-${index}`, tool: entry[0], binding: definition.binding, arguments: entry[1] });
    }
    return invoke("call", { request: status.next_request, calls, ...extra });
  };
  try {
    await t.test("one fixed project-bound gateway; native session identity", async () => {
      const outer = await client.initialize();
      assert.equal(outer.tools.length, 1); assert.equal(outer.tools[0].name, "agent_environment"); assert.equal(outer.tools[0].projectBound, true);
      status = await client.action({ action: "sessions" });
      assert(!status.error, readable(status)); assert.equal(status.native_source, "pi"); assert(status.native_session_id); assert.equal(status.cwd, cwd);
    });
    await t.test("native write/edit/read/bash and persistent transcript", async () => {
      const write = await batch([["write", { path: "native.txt", content: "before\n" }]]);
      assert.equal(write.native_dispatch_state, "completed", readable(write)); assert(!write.results[0].is_error, readable(write));
      const edit = await batch([["edit", { path: "native.txt", oldText: "before", newText: "after" }]]);
      assert(!edit.results?.[0]?.is_error && edit.native_dispatch_state === "completed", readable(edit));
      const result = await batch([["read", { path: "native.txt" }], ["bash", { command: "printf 'native-bash'" }]]);
      assert.equal(result.native_dispatch_state, "completed", readable(result)); assert(readable(result.results[0]).includes("after")); assert(readable(result.results[1]).includes("native-bash"));
      assert.equal(await readFile(join(cwd, "native.txt"), "utf8"), "after\n");
      assert((await readdir(join(stateDir, "sessions"))).some((name) => name.endsWith(".jsonl")));
    });
    await t.test("native extension, permission hook and native schema validation", async () => {
      const allowed = await batch([["native_echo", { value: "ok" }]]);
      assert.equal(allowed.native_dispatch_state, "completed", readable(allowed)); assert(readable(allowed).includes("native_echo:ok"));
      const denied = await batch([["native_echo", { value: "denied" }]]);
      assert.equal(denied.native_dispatch_state, "completed", readable(denied)); assert.equal(denied.results[0].is_error, true); assert(readable(denied).includes("native permission fixture"));
      const malformed = await batch([["native_echo", {}]]);
      assert.equal(malformed.native_dispatch_state, "completed", readable(malformed)); assert.equal(malformed.results[0].is_error, true);
      const hooks = await readFile(join(agentDir, "native-hooks.jsonl"), "utf8");
      assert(hooks.includes('"event":"tool_call"')); assert(hooks.includes('"event":"tool_result"')); assert(hooks.includes('"tool":"native_echo"'));
    });
    await t.test("real MCP server -> Pi native tools -> Plugin gateway", async () => {
      const result = await batch([["mcp__fixture__echo", { value: "through-native-mcp" }]]);
      assert.equal(result.native_dispatch_state, "completed", readable(result)); assert(!result.results[0].is_error, readable(result)); assert(readable(result).includes("mcp:through-native-mcp"));
      assert((await readFile(join(agentDir, "native-hooks.jsonl"), "utf8")).includes('"tool":"mcp__fixture__echo"'));
    });
    await t.test("native code-mode/deferred discovery without directly declaring hidden tools", async () => {
      const tools = await invoke("tools", { query: "native_", limit: 32 });
      assert(!tools.tools.some((tool) => ["native_hidden", "native_off"].includes(tool.name)));
      assert.equal((await describe("native_code")).access, "native_orchestrator_only");
      assert.equal((await describe("native_deferred")).access, "native_orchestrator_only");
      const hidden = await invoke("describe", { tool: "native_hidden" }); assert.equal(hidden.error, "native_tool_unavailable");
      const modelOnly = await batch([["native_model", { value: "native-model-only" }]]);
      assert.equal(modelOnly.native_dispatch_state, "completed", readable(modelOnly));
      assert(readable(modelOnly).includes("native_model:native-model-only"));
      const result = await batch([["codemode", { code: "console.log(await tools.native_code({value: 'via-codemode'})); let blocked = false; try { await tools.native_model({value: 'must-not-execute'}); } catch { blocked = true; } console.log('model-only-blocked=' + blocked);" }]]);
      assert.equal(result.native_dispatch_state, "completed", readable(result)); assert(!result.results[0].is_error, readable(result)); assert(readable(result).includes("via-codemode"));
      assert(readable(result).includes("model-only-blocked=true"));
    });
    await t.test("mixed native success/failure retains per-call ids and input order", async () => {
      const result = await batch([["native_echo", { value: "partial-ok" }], ["native_fail", {}]]);
      assert.equal(result.native_dispatch_state, "completed", readable(result));
      assert.deepEqual(result.results.map((r) => [r.id, r.tool, r.is_error]), [["call-0", "native_echo", false], ["call-1", "native_fail", true]]);
    });
    await t.test("dynamic extension load on native reload, same process/session and stale schema fence", async () => {
      const old = await describe("native_echo");
      const nativeId = status.native_session_id;
      await writeFile(join(agentDir, "extensions", "dynamic.js"), 'export default function(pi) { pi.registerTool({name:"native_dynamic",label:"Dynamic",description:"Dynamically loaded extension",parameters:{type:"object",properties:{},additionalProperties:false},async execute(){return {content:[{type:"text",text:"dynamic-loaded"}]};}}); }\n');
      const refresh = await invoke("refresh"); assert(!refresh.error, readable(refresh)); assert.equal(refresh.native_session_id, nativeId);
      const stale = await invoke("call", { request: status.next_request, calls: [{ id: "stale", tool: "native_echo", binding: old.binding, arguments: { value: "must-not-run" } }] });
      assert.equal(stale.error, "native_schema_changed"); assert.equal(stale.native_dispatch_state, "not_started");
      const dynamic = await batch([["native_dynamic", {}]]); assert.equal(dynamic.native_dispatch_state, "completed", readable(dynamic)); assert(readable(dynamic).includes("dynamic-loaded"));
    });
    await t.test("history and credentials/config isolation; no outer metadata forwarding", async () => {
      const history = await invoke("history", { limit: 8 }); assert(history.history.length > 0, readable(history));
      assert(!readable(history).includes("DO_NOT_DELEGATE_LLM_CREDENTIAL"));
      const outerMeta = await invoke("tools", { _meta: { "openai/session": "UNTRUSTED_METADATA_SENTINEL" } });
      assert.equal(outerMeta.error, "invalid_arguments"); assert(!readable(outerMeta).includes("UNTRUSTED_METADATA_SENTINEL"));
      assert.equal(await readFile(join(agentDir, "settings.json"), "utf8"), settings);
      assert.equal(await readFile(join(agentDir, "auth.json"), "utf8"), authSentinel);
    });
    await t.test("expired deadline does not start, post-dispatch deadline is unknown and never replayed", async () => {
      const definition = await describe("native_slow");
      const calls = [{ id: "slow", tool: "native_slow", binding: definition.binding, arguments: {} }];
      const expired = await invoke("call", { request: status.next_request, calls, deadline: Date.now() - 1 });
      assert.equal(expired.error, "deadline_before_dispatch");
      const request = status.next_request;
      const result = await invoke("call", { request, calls, timeout_ms: 120 });
      assert.equal(result.native_dispatch_state, "outcome_unknown", readable(result)); assert.equal(result.quarantined, true);
      const retry = await invoke("call", { request, calls }); assert.equal(retry.native_dispatch_state, "not_started");
      await delay(700);
      const hooks = (await readFile(join(agentDir, "native-hooks.jsonl"), "utf8")).trim().split("\n").map(JSON.parse);
      assert.equal(hooks.filter((event) => event.event === "slow_started").length, 1);
      assert.equal(hooks.filter((event) => event.event === "slow_effect").length, 1);
    });
  } finally {
    await client.close();
    if (process.env.WEBCODEX_KEEP_DOGFOOD === "1") console.log("PI_DOGFOOD_DIRECTORY", root);
    else await rm(root, { recursive: true, force: true });
  }
});
