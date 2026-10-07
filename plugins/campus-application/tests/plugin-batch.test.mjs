import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, copyFile, unlink, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import { once } from "node:events";
import test from "node:test";

async function provider(t, dir, editProfile) {
  const profile = join(dir, "profile.json");
  await copyFile(new URL("../profile.example.json", import.meta.url), profile);
  if (editProfile) {
    const fictional = JSON.parse(await readFile(profile, "utf8"));
    editProfile(fictional);
    await writeFile(profile, JSON.stringify(fictional));
  }
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
test("date-only profile never emits a guaranteed-invalid datetime-local set_value", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-datetime-local-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const datetime = {
    ...node("出生日期", 0),
    form_context: { field_signature: "datetime-local", dom_tag: "input", input_type: "datetime-local" },
  };
  const plan = await call("plan_fill", { ...scope, title: "Fictional datetime form", nodes: [datetime] });
  assert.equal(plan.batch, undefined);
  assert.equal(plan.needs_attention.length, 1);
  assert.match(plan.needs_attention[0].reason, /required precision/);
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
test("provider plans admitted custom choices and compactly continues a verified sibling batch", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-custom-choice-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const combo = (name, i) => ({ ...node(name, i), role: "combobox", read_only: true, actions: ["select_choice"],
    form_context: { field_signature: "combo-sig-" + i, dom_tag: "input", component_hint: "select" } });
  const nodes = [node("姓名", 0), combo("学历", 1), combo("政治面貌", 2), combo("性别", 3)];
  const plan = await call("plan_fill", { ...scope, title: "Fictional custom form", nodes, widget_batch_limit: 3 });
  assert.equal(plan.batch.operations.length, 1);
  assert.equal(plan.deferred_count, 3);
  const fresh = nodes.map(n => ({ ...n, element_id: "fresh-" + n.element_id }));
  fresh[0].value = "示例候选人";
  const choices = await call("reconcile_fill", { ...scope, snapshot_generation: 2, plan_id: plan.plan_id, nodes: fresh,
    receipt: { execution_state: "completed", requested_count: 1, completed_count: 1, remaining_count: 0, stability: { stable: true } } });
  assert.equal(choices.batch.operations.length, 3);
  assert.deepEqual(choices.batch.operations.map(op => op.choice_path), [["硕士"], ["群众"], ["男"]]);
  assert.ok(choices.batch.operations.every(op => op.action === "select_choice" && op.path === undefined));
  ["硕士", "群众", "男"].forEach((value, i) => { fresh[i + 1].value = value; });
  const result = await call("reconcile_fill", { ...scope, snapshot_generation: 3, plan_id: choices.plan_id, nodes: fresh,
    receipt: { execution_state: "completed", requested_count: 3, completed_count: 3, remaining_count: 0, stability: { stable: true } } });
  assert.deepEqual(result, { confirmed: 3, needs_attention: [] });
  const unadmitted = await call("plan_fill", { ...scope, title: "Fictional old Runner",
    nodes: [{ ...combo("学历", 7), actions: ["click", "select_option"] }] });
  assert.equal(unadmitted.batch, undefined);
  assert.equal(unadmitted.needs_attention.length, 1);
});
test("provider does not skip editable combobox search text that equals the profile choice", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-editable-choice-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const searching = { ...node("学历", 0), role: "combobox", read_only: false, value: "硕士",
    actions: ["select_choice"] };
  const plan = await call("plan_fill", { ...scope, title: "Fictional search combobox", nodes: [searching] });
  assert.deepEqual(plan.batch.operations, [{ action: "select_choice", element_id: "element-0", choice_path: ["硕士"] }]);
  assert.deepEqual(plan.needs_attention, []);
  const result = await call("reconcile_fill", { ...scope, snapshot_generation: 2, plan_id: plan.plan_id,
    nodes: [{ ...searching, element_id: "completed-selection" }],
    receipt: { execution_state: "completed", requested_count: 1, completed_count: 1, remaining_count: 0,
      stability: { stable: true } } });
  assert.deepEqual(result, { confirmed: 1, needs_attention: [] });
});
test("learned custom choice_value survives provider restart without repeated model interpretation", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-choice-memory-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir);
  const nodes = [{ ...node("", 0), role: "combobox", actions: ["select_choice"] }];
  const analysis = await call("analyze_form", { title: "Fictional choice", url: scope.url, nodes });
  const mapping_id = analysis.unmapped_candidates[0].mapping_id;
  const taught = await call("plan_fill", { ...scope, title: "Fictional choice", nodes,
    mapping_hints: [{ mapping_id, canonical_field: "degree", choice_value: "研究生（硕士）" }] });
  assert.deepEqual(taught.batch.operations[0].choice_path, ["研究生（硕士）"]);
  const restarted = await provider(t, dir);
  const reused = await restarted("plan_fill", { ...scope, title: "Fictional choice", nodes: [{ ...nodes[0], element_id: "fresh" }] });
  assert.deepEqual(reused.batch.operations, [{ action: "select_choice", element_id: "fresh", choice_path: ["研究生（硕士）"] }]);
});
test("provider emits distinct location paths from split facts and canonical custom/native dates", { timeout: 10000 }, async t => {
  const dir = await mkdtemp(join(tmpdir(), "campus-location-date-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const call = await provider(t, dir, profile => {
    for (const [field, label] of [["native_place", "籍贯"], ["household_registration", "户籍"], ["student_origin", "生源"]]) {
      profile.personal[field] = "";
      for (const [axis, suffix] of [["province", "省"], ["city", "市"], ["district", "区"]]) profile.personal[field + "_" + axis] = label + suffix;
    }
    profile.education[0].graduation_date = "预计2027年6月";
  });
  const custom = (name, i, action, component_hint) => ({ ...node(name, i), role: action === "set_date" ? "textbox" : "combobox",
    actions: [action], read_only: true, form_context: { field_signature: "complex-" + i, dom_tag: "input", input_type: "text", component_hint } });
  const nodes = [
    custom("籍贯", 0, "select_choice", "cascader"), custom("户籍", 1, "select_choice", "cascader"),
    custom("生源地", 2, "select_choice", "cascader"), custom("毕业日期", 3, "set_date", "month-picker"),
    { ...node("出生年月", 4), form_context: { field_signature: "native-month", dom_tag: "input", input_type: "month" } },
  ];
  const native = await call("plan_fill", { ...scope, title: "Fictional locations and dates", nodes, widget_batch_limit: 4 });
  assert.deepEqual(native.needs_attention, []);
  assert.deepEqual(native.batch.operations, [{ action: "set_value", element_id: "element-4", value: "2000-01" }]);
  const fresh = nodes.map(n => ({ ...n, element_id: "fresh-" + n.element_id }));
  fresh[4].value = "2000-01";
  const widgets = await call("reconcile_fill", { ...scope, snapshot_generation: 2, plan_id: native.plan_id, nodes: fresh,
    receipt: { execution_state: "completed", requested_count: 1, completed_count: 1, remaining_count: 0, stability: { stable: true } } });
  assert.deepEqual(widgets.batch.operations.map(op => op.choice_path ?? op.value),
    [["籍贯省", "籍贯市", "籍贯区"], ["户籍省", "户籍市", "户籍区"], ["生源省", "生源市", "生源区"], "2027-06"]);
  assert.equal(widgets.batch.operations.at(-1).action, "set_date");
  ["籍贯省 / 籍贯市 / 籍贯区", "户籍省 / 户籍市 / 户籍区", "生源省 / 生源市 / 生源区", "2027年6月"].forEach((value, i) => { fresh[i].value = value; });
  const done = await call("reconcile_fill", { ...scope, snapshot_generation: 3, plan_id: widgets.plan_id, nodes: fresh,
    receipt: { execution_state: "completed", requested_count: 4, completed_count: 4, remaining_count: 0, stability: { stable: true } } });
  assert.deepEqual(done, { confirmed: 4, needs_attention: [] });
  const preference = custom("意向地点", 10, "select_choice", "cascader");
  const alternatives = await call("plan_fill", { ...scope, title: "Fictional preferences", nodes: [preference] });
  assert.equal(alternatives.batch, undefined);
  assert.match(alternatives.needs_attention[0].reason, /alternatives/);
  const selectedPreference = await call("plan_fill", { ...scope, title: "Fictional preferences", nodes: [preference],
    mapping_hints: [{ mapping_id: alternatives.needs_attention[0].mapping_id, canonical_field: "preferred_locations",
      resume_path: "job_preferences.preferred_locations[1]" }] });
  assert.deepEqual(selectedPreference.batch.operations[0].choice_path, ["Hangzhou"]);
  const profile = await call("profile_get", {});
  assert.equal(profile.profile.native_place_district, "籍贯区");
  assert.equal(profile.profile.student_origin_district, "生源区");
});



