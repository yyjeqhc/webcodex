import assert from "node:assert/strict";
import test from "node:test";
import { homedir } from "node:os";
import { createFillBatch, reconcileFill } from "../dist/fill-batch.js";
import { resolveUploadLocation } from "../dist/upload-source.js";
import { resolveFormMappings } from "../dist/form-cache.js";
import { planChoiceGroup } from "../dist/choice-controls.js";

const scope = { client_id: "fixture", browser_id: "browser", page_id: "page", url: "https://fixture.invalid/form", snapshot_generation: 1 };
const complete = count => ({ execution_state: "completed", requested_count: count, completed_count: count,
  remaining_count: 0, stability: { stable: true } });
function choice(name, role, index, checked = "false") {
  return { role, name, checked, actionable: true, actions: ["click"], element_id: "choice-" + index,
    form_context: { field_signature: "choice-sig-" + index, dom_tag: "input", input_type: role } };
}
test("text, radio, checkbox and resume upload share one batch with state/filename readback", () => {
  const nodes = [
    { role: "textbox", name: "姓名", value: "", actionable: true, actions: ["set_value"], element_id: "text",
      form_context: { field_signature: "text", dom_tag: "input" } },
    { ...choice("男", "radio", 1), value: "男" },
    choice("是否接受岗位调剂", "checkbox", 2, "true"),
    { role: "button", name: "简历", value: "", actionable: true, actions: ["upload_file"], element_id: "file",
      form_context: { field_signature: "file", dom_tag: "input", input_type: "file" } },
  ];
  const actions = [
    { kind: "set_value", value: "Fictional candidate" },
    { kind: "click", value: "男", desired_state: true },
    { kind: "click", value: "否", desired_state: false },
    { kind: "upload_file", value: "fixtures/resume.pdf", upload: { project: "fixture-project", path: "fixtures/resume.pdf" } },
  ].map((action, i) => ({ ...action, label: nodes[i].name, element_id: nodes[i].element_id, confidence: 1 }));
  const plan = createFillBatch(scope, nodes, actions);
  assert.deepEqual(plan.batch.operations.map(op => op.action), ["set_value", "click", "click", "upload_file"]);
  assert.deepEqual(plan.batch.operations[3], { action: "upload_file", element_id: "file", project: "fixture-project", path: "fixtures/resume.pdf" });
  const fresh = nodes.map((node, index) => ({ ...node, element_id: "fresh-" + index }));
  fresh[0].value = actions[0].value;
  fresh[1].checked = "true";
  fresh[2].checked = "false";
  fresh[3].value = "C:\\fakepath\\resume.pdf";
  const result = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 }, fresh, complete(4));
  assert.deepEqual(result, { confirmed: 4, needs_attention: [] });
  assert.ok(JSON.stringify(result).length < 50);
  assert.equal(createFillBatch({ ...scope, snapshot_generation: 2 }, fresh,
    actions.map((action, i) => ({ ...action, element_id: fresh[i].element_id }))).batch, undefined);
});
test("checked/selected readback is authoritative and absent/mixed state never toggles", () => {
  for (const state of [undefined, "mixed"]) {
    const node = { ...choice("男", "radio", 1), checked: state, value: "男" };
    const action = { kind: "click", label: "性别", element_id: node.element_id, value: "男", desired_state: true, confidence: 1 };
    assert.equal(createFillBatch(scope, [node], [action]).batch, undefined);
  }
  const node = { ...choice("硕士", "option", 1), checked: undefined, selected: false };
  const action = { kind: "click", label: "学历", element_id: node.element_id, value: "硕士", desired_state: true, confidence: 1 };
  const plan = createFillBatch(scope, [node], [action]);
  const result = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
    [{ ...node, element_id: "new-option", selected: true }], complete(1));
  assert.deepEqual(result, { confirmed: 1, needs_attention: [] });
  assert.equal(createFillBatch(scope, [{ ...node, role: "group", form_context: { dom_tag: "div", field_signature: "wrapper" } }], [action]).batch, undefined);
});
test("radio mapping selects only one data choice and rejects duplicate labels without group provenance", () => {
  const male = { ...choice("男", "radio", 0), group_label: "性别" };
  const female = { ...choice("女", "radio", 1), group_label: "性别" };
  const resolved = resolveFormMappings([male, female], [], "native-choice-fixture");
  const planned = planChoiceGroup(resolved.nodes[0], resolved.nodes, "male");
  assert.deepEqual(planned.targets.map(item => item.node.element_id), ["choice-0"]);
  const duplicate = resolveFormMappings([male, { ...male, element_id: "duplicate",
    form_context: { ...male.form_context, field_signature: "different" } }], [], "native-choice-duplicate");
  assert.match(planChoiceGroup(duplicate.nodes[0], duplicate.nodes, "男").reason, /Multiple choices/);
  assert.match(planChoiceGroup(resolved.nodes[0], resolved.nodes, "未指定").reason, /No unique choice/);
});
test("scalar checkbox/switch desired false can uncheck; negative choice label inverts the toggle", () => {
  for (const role of ["checkbox", "switch"]) {
    const resolved = resolveFormMappings([choice("是否接受岗位调剂", role, 0, "true")], [], "toggle-" + role);
    assert.equal(planChoiceGroup(resolved.nodes[0], resolved.nodes, "否").targets[0].desired_state, false);
    const negative = resolveFormMappings([{ ...choice("否", role, 0), group_label: "是否接受岗位调剂" }], [], "negative-" + role);
    assert.equal(planChoiceGroup(negative.nodes[0], negative.nodes, "否").targets[0].desired_state, true);
    const explicitOther = { ...resolved.nodes[0], mapping: { ...resolved.nodes[0].mapping, choiceValue: "其他" } };
    assert.match(planChoiceGroup(explicitOther, [explicitOther], "是").reason, /explicit boolean/);
  }
});
test("upload path derivation requires explicit project and confines absolute/home paths to its root", () => {
  assert.deepEqual(resolveUploadLocation("fixtures/resume.pdf", { project: "fixture" }), { project: "fixture", path: "fixtures/resume.pdf" });
  assert.deepEqual(resolveUploadLocation("/fixture/project/files/resume.pdf", { project: "fixture", project_root: "/fixture/project" }),
    { project: "fixture", path: "files/resume.pdf" });
  assert.deepEqual(resolveUploadLocation("~/resume-fixture/resume.pdf", { project: "fixture", project_root: homedir() }),
    { project: "fixture", path: "resume-fixture/resume.pdf" });
  assert.deepEqual(resolveUploadLocation("C:\\fixture\\project\\resume.pdf", { project: "fixture", project_root: "C:\\fixture\\project" }),
    { project: "fixture", path: "resume.pdf" });
  for (const [value, source] of [
    ["resume.pdf", undefined], ["/fixture/project/resume.pdf", { project: "fixture" }],
    ["/fixture/outside/resume.pdf", { project: "fixture", project_root: "/fixture/project" }],
    ["/fixture/project-other/resume.pdf", { project: "fixture", project_root: "/fixture/project" }],
    ["../resume.pdf", { project: "fixture" }], ["C:\\other\\resume.pdf", { project: "fixture", project_root: "C:\\fixture\\project" }],
  ]) assert.equal(resolveUploadLocation(value, source), undefined);
});
