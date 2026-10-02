import test from "node:test";
import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";
import { AgentEnvironmentBridge } from "../src/bridge.mjs";
import { Catalog, digest, fits } from "../src/catalog.mjs";

const tool = (name, exposure = "direct", declared = exposure === "direct") => ({ name, exposure, declared, callable: exposure !== "model-only" && exposure !== "hidden", description: `Tool ${name}`, parameters: { type: "object", properties: { value: { type: "string" } }, required: ["value"] } });
function fixture(tools = [tool("echo"), tool("other")]) {
  const state = { identity: { native_source: "fake", session: "native-session-one", native_session_id: "session-one", cwd: "/fixture", model: "local/tools" }, tools, invocations: 0, records: [], idle: true, handler: undefined };
  const environment = {
    identity: () => ({ ...state.identity }), tools: () => state.tools, isIdle: () => state.idle,
    async invoke(calls, options) {
      options.beforeDispatch(); state.invocations++; state.idle = false;
      try {
        if (state.handler) return await state.handler(calls, options);
        const results = calls.map((call) => ({ toolCallId: call.id, toolName: call.name, content: [{ type: "text", text: call.arguments.value ?? "" }], isError: call.arguments.fail === true }));
        state.records.push(...results); return results.reverse();
      } finally { state.idle = true; }
    },
    async refresh() {}, async abort() {}, async close() {}, history: (limit) => state.records.slice(-limit),
  };
  const bridge = new AgentEnvironmentBridge(async () => environment);
  const start = () => bridge.execute({ action: "sessions" });
  const describe = async (name = "echo") => (await bridge.execute({ action: "describe", session: state.identity.session, tool: name })).tool;
  const call = async (calls, extra = {}) => bridge.execute({ action: "call", session: state.identity.session, request: bridge.request, calls, ...extra });
  return { state, bridge, start, describe, call };
}

for (const [name, count] of [["empty", 0], ["large", 2048]]) test(`bounded ${name} catalog without flattening capability`, () => {
  const catalog = new Catalog(Array.from({ length: count }, (_, i) => tool(`tool_${i}`)), "native-session");
  const page = catalog.page({ limit: 9999 });
  assert.equal(page.tools.length, Math.min(32, count)); assert.equal(page.total, count);
  assert(fits(page, 32 * 1024));
  if (count) {
    const next = catalog.page({ cursor: page.next_cursor, limit: 32 });
    assert.equal(next.tools.length, 32); assert.notEqual(next.tools[0].name, page.tools[0].name);
    assert.throws(() => new Catalog([tool("new")], "native-session").page({ cursor: page.next_cursor }), /stale_cursor/);
  }
});

test("Pi-style exposure separates discovery, native callability and model declaration", () => {
  const catalog = new Catalog([tool("direct"), tool("model", "model-only", true), tool("code", "codemode", false), tool("lazy", "deferred", false), tool("secret", "hidden", false), { ...tool("off"), declared: false, callable: false }], "session");
  assert.deepEqual(catalog.page({}).tools.map((t) => t.name), ["code", "direct", "model"]);
  assert.equal(catalog.page({ query: "lazy" }).tools[0].access, "native_orchestrator_only");
  for (const name of ["secret", "off"]) assert.throws(() => catalog.describe(name), /native_tool_unavailable/);
  assert.throws(() => catalog.admit([{ id: "a", tool: "code", binding: catalog.describe("code").binding, arguments: {} }]), /native_tool_not_declared/);
});

test("duplicate names, malformed schemas and oversized schemas fail closed", () => {
  assert.throws(() => new Catalog([tool("a"), tool("a")], "s"), /duplicate/);
  assert.throws(() => new Catalog([{ ...tool("a"), parameters: null }], "s"), /invalid/);
  assert.throws(() => new Catalog([{ ...tool("a"), parameters: { type: "object", description: "x".repeat(100_000) } }], "s").describe("a"), /bounds/);
});

test("exact session and Project cwd are fenced, with no implicit default", async () => {
  const f = fixture();
  assert.equal((await f.bridge.execute({ action: "tools", session: "unknown" })).native_dispatch_state, "not_started");
  const started = await f.start(); assert.equal(started.session, "native-session-one");
  assert.equal((await f.bridge.execute({ action: "tools" })).error, "native_session_unavailable");
  f.state.identity.cwd = "/different-project";
  assert.equal((await f.bridge.execute({ action: "tools", session: started.session })).error, "native_session_changed");
  assert.equal(f.state.invocations, 0);
});

test("schema changes, disappearance and native model/session replacement are fenced", async () => {
  for (const mutation of [
    (f) => { f.state.tools[0].parameters.properties.value.type = "integer"; },
    (f) => { f.state.tools.shift(); },
    (f) => { f.state.identity.model = "remote/model"; },
    (f) => { f.state.identity.native_session_id = "restarted"; },
  ]) {
    const f = fixture(); await f.start(); const d = await f.describe(); mutation(f);
    const result = await f.call([{ id: "a", tool: "echo", binding: d.binding, arguments: { value: "x" } }]);
    assert.equal(result.native_dispatch_state, "not_started"); assert.equal(f.state.invocations, 0);
  }
});

test("unrelated lazy catalog additions retain exact schema bindings; refresh invalidates them", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  f.state.tools.push(tool("mcp_new", "deferred", false));
  assert.equal((await f.call([{ id: "a", tool: "echo", binding: d.binding, arguments: { value: "ok" } }])).native_dispatch_state, "completed");
  await f.bridge.execute({ action: "refresh", session: f.state.identity.session });
  assert.equal((await f.call([{ id: "b", tool: "echo", binding: d.binding, arguments: { value: "no" } }])).error, "native_schema_changed");
  assert.equal(f.state.invocations, 1);
});

test("batch maps exact ids independent of native result order, including partial failure", async () => {
  const f = fixture(); const s = await f.start(); const a = await f.describe(); const b = await f.describe("other");
  const result = await f.call([{ id: "a", tool: "echo", binding: a.binding, arguments: { value: "first" } }, { id: "b", tool: "other", binding: b.binding, arguments: { value: "second", fail: true } }]);
  assert.equal(result.native_dispatch_state, "completed"); assert.deepEqual(result.results.map((r) => [r.id, r.tool, r.is_error]), [["a", "echo", false], ["b", "other", true]]);
  const replay = await f.call([{ id: "a", tool: "echo", binding: a.binding, arguments: { value: "repeat" } }], { request: s.next_request });
  assert.equal(replay.error, "stale_request_no_replay"); assert.equal(f.state.invocations, 1);
});

test("malformed batches and unknown metadata never reach native execution or leak credentials", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  const valid = { id: "a", tool: "echo", binding: d.binding, arguments: {} };
  for (const calls of [[], [valid, valid], [{ ...valid, arguments: [] }], [{ ...valid, _meta: { secret: "CREDENTIAL_SENTINEL" } }]]) {
    assert.equal((await f.call(calls)).native_dispatch_state, "not_started");
  }
  const meta = await f.bridge.execute({ action: "sessions", _meta: { "openai/session": "CREDENTIAL_SENTINEL" } });
  assert.equal(meta.error, "invalid_arguments"); assert(!JSON.stringify(meta).includes("CREDENTIAL_SENTINEL")); assert.equal(f.state.invocations, 0);
});

test("cancel and deadline before native dispatch do not consume effects", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  const calls = [{ id: "a", tool: "echo", binding: d.binding, arguments: {} }];
  const expired = await f.call(calls, { deadline: Date.now() - 1 });
  assert.equal(expired.error, "deadline_before_dispatch");
  const cancel = await f.bridge.execute({ action: "call", session: f.state.identity.session, request: f.bridge.request, calls }, AbortSignal.abort());
  assert.equal(cancel.error, "cancelled_before_dispatch"); assert.equal(f.state.invocations, 0);
});

test("post-dispatch timeout is quarantined outcome_unknown, not a retry hint", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  f.state.handler = async () => { await delay(40); return []; };
  const calls = [{ id: "a", tool: "echo", binding: d.binding, arguments: {} }];
  const result = await f.call(calls, { timeout_ms: 5 });
  assert.equal(result.native_dispatch_state, "outcome_unknown"); assert.equal(result.no_replay, true);
  assert.equal((await f.call(calls)).native_dispatch_state, "not_started"); assert.equal(f.state.invocations, 1);
  assert.equal((await f.bridge.execute({ action: "refresh", session: f.state.identity.session })).error, "native_session_quarantined_or_busy");
  await delay(45);
});

test("native disconnect and corrupt result identities after dispatch are uncertain and redacted", async () => {
  for (const handler of [async () => { throw new Error("CREDENTIAL_SENTINEL"); }, async () => [], async (calls) => [{ toolCallId: calls[0].id, toolName: "wrong", content: [] }]]) {
    const f = fixture(); await f.start(); const d = await f.describe(); f.state.handler = handler;
    const result = await f.call([{ id: "a", tool: "echo", binding: d.binding, arguments: {} }]);
    assert.equal(result.native_dispatch_state, "outcome_unknown"); assert(!JSON.stringify(result).includes("CREDENTIAL_SENTINEL"));
  }
});

test("busy rejection cannot unlock an in-flight call", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  f.state.handler = async (calls) => { await delay(20); return calls.map((c) => ({ toolCallId: c.id, toolName: c.name, content: [] })); };
  const call = f.call([{ id: "a", tool: "echo", binding: d.binding, arguments: {} }]);
  assert.equal((await f.bridge.execute({ action: "sessions" })).error, "native_session_busy");
  assert.equal((await f.bridge.execute({ action: "sessions" })).error, "native_session_busy");
  assert.equal((await call).native_dispatch_state, "completed"); assert.equal(f.state.invocations, 1);
});

test("oversized native content is explicitly bounded without losing completed/error identity", async () => {
  const f = fixture(); await f.start(); const d = await f.describe();
  f.state.handler = async (calls) => calls.map((c) => ({ toolCallId: c.id, toolName: c.name, isError: true, content: [{ type: "text", text: "x".repeat(600_000) }], details: { huge: true } }));
  const result = await f.call([{ id: "a", tool: "echo", binding: d.binding, arguments: {} }]);
  assert.equal(result.native_dispatch_state, "completed"); assert.equal(result.results[0].is_error, true); assert.equal(result.results[0].result.omitted, true); assert(fits(result, 16_000));
});

test("failed native initialization is uncertain and never automatically retried", async () => {
  let attempts = 0;
  const bridge = new AgentEnvironmentBridge(async () => { attempts++; throw new Error("NATIVE_CONFIG_SECRET"); });
  const first = await bridge.execute({ action: "sessions" });
  assert.equal(first.native_dispatch_state, "outcome_unknown");
  assert(!JSON.stringify(first).includes("NATIVE_CONFIG_SECRET"));
  const second = await bridge.execute({ action: "sessions" });
  assert.equal(second.error, "native_environment_quarantined"); assert.equal(attempts, 1);
});

test("a failed native reload quarantines the partially changed environment", async () => {
  const f = fixture(); await f.start();
  f.bridge.environment.refresh = async () => { throw new Error("partial native reload"); };
  const result = await f.bridge.execute({ action: "refresh", session: f.state.identity.session });
  assert.equal(result.native_dispatch_state, "outcome_unknown"); assert.equal(result.quarantined, true);
  assert.equal((await f.bridge.execute({ action: "refresh", session: f.state.identity.session })).native_dispatch_state, "not_started");
});

test("bounded discovery preserves native names and trims prose, never the callable schema", () => {
  const definition = { ...tool("native/查询"), description: "large native help ".repeat(10_000), namespace: { name: "n".repeat(100_000) } };
  const catalog = new Catalog([definition], "native-session");
  const described = catalog.describe(definition.name);
  assert.equal(described.name, definition.name); assert.equal(described.description_truncated, true);
  assert.deepEqual(described.inputSchema, definition.parameters); assert(fits(catalog.page({}), 32 * 1024));
  assert.equal(catalog.admit([{ id: "one", tool: definition.name, binding: described.binding, arguments: { value: "ok" } }])[0].name, definition.name);
});

test("history and an explicit native reload remain usable when the catalog needs repair", async () => {
  const f = fixture(); await f.start();
  f.state.tools = [tool("duplicate"), tool("duplicate")];
  assert.equal((await f.bridge.execute({ action: "tools", session: f.state.identity.session })).error, "native_catalog_duplicate_or_invalid_name");
  assert.deepEqual((await f.bridge.execute({ action: "history", session: f.state.identity.session })).history, []);
  f.bridge.environment.refresh = async () => { f.state.tools = [tool("repaired")]; };
  assert.equal((await f.bridge.execute({ action: "refresh", session: f.state.identity.session })).native_dispatch_state, "completed");
  assert.equal((await f.bridge.execute({ action: "tools", session: f.state.identity.session })).tools[0].name, "repaired");
});

test("canonical schemas have stable key-order hashes", () => { assert.equal(digest({ a: 1, b: 2 }), digest({ b: 2, a: 1 })); });
