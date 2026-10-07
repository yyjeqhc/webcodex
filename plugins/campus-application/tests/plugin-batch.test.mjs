import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, copyFile, unlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { once } from "node:events";
import test from "node:test";

async function provider(t, dir) {
  const profile = join(dir, "profile.json");
  await copyFile(new URL("../profile.example.json", import.meta.url), profile);
  const child = spawn(process.execPath, [fileURLToPath(new URL("../dist/plugin.js", import.meta.url))], {
    stdio: ["pipe", "pipe", "pipe"],
    env: { ...process.env,
      WEBCODEX_CAMPUS_APPLICATION_PROFILE: profile,
      WEBCODEX_CAMPUS_APPLICATION_MAPPING_MEMORY: join(dir, "mapping-memory.json") },
  });
  let stderr = "";
  child.stderr.setEncoding("utf8").on("data", chunk => { stderr += chunk; });
  const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  t.after(async () => {
    child.stdin.end();
    if (child.exitCode === null) await once(child, "exit");
    assert.equal(child.exitCode, 0, stderr);
  });
  let id = 0;
  return async (name, args) => {
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: ++id, method: "tools/call", params: { name, arguments: args } }) + "\n");
    const line = await lines.next();
    assert.equal(line.done, false, stderr);
    const response = JSON.parse(line.value);
    assert.equal(response.result.isError, false, JSON.stringify(response));
    return response.result.structuredContent;
  };
}
const scope = { client_id: "fictional", browser_id: "browser", page_id: "page", snapshot_generation: 1, url: "https://fixture.invalid/form" };
const labels = ["姓名", "姓", "名", "性别", "出生日期", "身份证号", "民族", "政治面貌", "健康状况", "籍贯",
  "户籍", "生源地", "婚姻状况", "邮箱", "手机", "城市", "地址", "学校", "学历", "专业"];
function node(name, i) {
  return { role: "textbox", name, value: "", actionable: true, actions: ["input_text", "set_value"], element_id: `element-${i}`,
    form_context: { field_signature: `sig-${i}`, dom_tag: "input", input_type: "text" } };
}
test("real provider protocol emits compact executable plan and reconciles without loading profile again", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-batch-protocol-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const nodes = labels.map(node);
  const plan = await call("plan_fill", { ...scope, title: "Fictional form", nodes });
  assert.equal(plan.batch.action, "batch");
  assert.equal(plan.batch.operations.length, 20, JSON.stringify(plan));
  assert.equal(plan.actions, undefined);
  assert.equal(plan.recognized, undefined);
  assert.deepEqual(plan.needs_attention, []);
  const values = new Map(plan.batch.operations.map(op => [op.element_id, op.value]));
  const readback = nodes.map(n => ({ ...n, value: values.get(n.element_id), element_id: `new-${n.element_id}` }));
  await unlink(join(dir, "profile.json"));
  const result = await call("reconcile_fill", { ...scope, snapshot_generation: 2, plan_id: plan.plan_id, nodes: readback,
    receipt: { execution_state: "completed", state_changed: true, requested_count: 20, completed_count: 20,
      remaining_count: 0, needs_snapshot: false, stability: { stable: true, waited_ms: 250, reason: "quiet" } } });
  assert.deepEqual(result, { confirmed: 20, needs_attention: [] });
});
test("mapping memory survives provider restart and emits batch without re-teaching unnamed field", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-batch-memory-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const first = await provider(t, dir);
  const nodes = [node("", 0)];
  delete nodes[0].name;
  const analysis = await first("analyze_form", { title: "Form", url: scope.url, nodes });
  const mapping_id = analysis.unmapped_candidates[0].mapping_id;
  const taught = await first("plan_fill", { ...scope, title: "Form", nodes,
    mapping_hints: [{ mapping_id, canonical_field: "full_name" }] });
  assert.equal(taught.batch.operations.length, 1);
  const second = await provider(t, dir);
  const reused = await second("plan_fill", { ...scope, title: "Form", nodes: [{ ...nodes[0], element_id: "replacement" }] });
  assert.deepEqual(reused.batch.operations, [{ action: "set_value", element_id: "replacement", value: "示例候选人" }]);
});
