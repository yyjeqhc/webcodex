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
test("real provider batches native choice/upload, skips satisfied states, and never guesses a radio group", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-native-choice-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const radio = (name, index) => ({ role: "radio", name, group_id: "gender-group", group_label: "性别",
    value: name, checked: "false", actionable: true, actions: ["click"], element_id: "radio-" + index,
    form_context: { field_signature: "radio-sig-" + index, dom_tag: "input", input_type: "radio", group_index: index, group_size: 2 } });
  const nodes = [
    node("姓名", 0), radio("男", 0), radio("女", 1),
    { role: "checkbox", name: "是否接受岗位调剂", checked: "true", actionable: true, actions: ["click"], element_id: "checkbox",
      form_context: { field_signature: "checkbox", dom_tag: "input", input_type: "checkbox" } },
    { role: "switch", name: "是否应届毕业生", checked: "false", actionable: true, actions: ["click"], element_id: "switch",
      form_context: { field_signature: "switch", dom_tag: "button" } },
    { role: "button", name: "简历", value: "", actionable: true, actions: ["upload_file"], element_id: "upload",
      form_context: { field_signature: "upload", dom_tag: "input", input_type: "file" } },
  ];
  const plan = await call("plan_fill", { ...scope, title: "Fictional mixed form", nodes, upload_source: { project: "fixture-project" } });
  assert.deepEqual(plan.needs_attention, []);
  assert.deepEqual(plan.batch.operations.map(op => op.action), ["set_value", "click", "click", "click", "upload_file"]);
  assert.ok(!plan.batch.operations.some(op => op.element_id === "radio-1"));
  assert.deepEqual(plan.batch.operations.at(-1), { action: "upload_file", element_id: "upload",
    project: "fixture-project", path: "plugins/campus-application/fixtures/sample-resume.pdf" });
  const fresh = nodes.map(n => ({ ...n, element_id: "new-" + n.element_id }));
  fresh[0].value = "示例候选人";
  fresh[1].checked = "true"; fresh[3].checked = "false"; fresh[4].checked = "true";
  fresh[5].value = "sample-resume.pdf";
  const result = await call("reconcile_fill", { ...scope, snapshot_generation: 2, plan_id: plan.plan_id, nodes: fresh,
    receipt: { execution_state: "completed", requested_count: 5, completed_count: 5, remaining_count: 0, stability: { stable: true } } });
  assert.deepEqual(result, { confirmed: 5, needs_attention: [] });
  const satisfied = await call("plan_fill", { ...scope, snapshot_generation: 2, title: "Fictional mixed form", nodes: fresh });
  assert.equal(satisfied.batch, undefined);
  assert.deepEqual(satisfied.needs_attention, []);
  const ambiguous = await call("plan_fill", { ...scope, title: "Fictional radio group",
    nodes: [radio("男", 0), radio("男", 1)] });
  assert.equal(ambiguous.batch, undefined);
  assert.match(ambiguous.needs_attention[0].reason, /Multiple choices/);
  const noProject = await call("plan_fill", { ...scope, title: "Fictional upload", nodes: [nodes[5]] });
  assert.equal(noProject.batch, undefined);
  assert.match(noProject.needs_attention[0].reason, /upload_source/);
});

