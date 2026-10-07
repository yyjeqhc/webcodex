import assert from "node:assert/strict";
import test from "node:test";
import { createFillBatch, reconcileFill } from "../dist/fill-batch.js";
import { choiceReadback, validChoicePath } from "../dist/widget-values.js";

const scope = { client_id: "fixture", browser_id: "browser", page_id: "page", url: "https://fixture.invalid/widgets", snapshot_generation: 1 };
const complete = count => ({ execution_state: "completed", requested_count: count, completed_count: count,
  remaining_count: 0, stability: { stable: true } });
function fixture(count) {
  const nodes = Array.from({ length: count }, (_, index) => ({
    role: "combobox", name: "Field " + index, value: "", actionable: true, read_only: true,
    element_id: "widget-" + index, actions: ["select_choice"],
    form_context: { field_signature: "widget-sig-" + index, dom_tag: "input", component_hint: "select" },
  }));
  const actions = nodes.map((node, index) => ({ kind: "select_choice", element_id: node.element_id,
    label: node.name, value: "Option " + index, choice_path: ["Option " + index], confidence: 1 }));
  return { nodes, actions };
}
test("native fields run first; verified widget limit batches siblings after one fresh observation", () => {
  const { nodes, actions } = fixture(5);
  nodes.unshift({ role: "textbox", name: "Native", value: "", actionable: true, actions: ["set_value"], element_id: "native",
    form_context: { field_signature: "native", dom_tag: "input" } });
  actions.unshift({ kind: "set_value", element_id: "native", label: "Native", value: "Fictional", confidence: 1 });
  const native = createFillBatch(scope, nodes, actions, [], 5);
  assert.deepEqual(native.batch.operations.map(op => op.action), ["set_value"]);
  assert.equal(native.deferred_count, 5);
  const fresh = nodes.map(n => ({ ...n, element_id: "fresh-" + n.element_id }));
  fresh[0].value = "Fictional";
  const widgets = reconcileFill(native.plan_id, { ...scope, snapshot_generation: 2 }, fresh, complete(1));
  assert.equal(widgets.confirmed, 1);
  assert.equal(widgets.batch.operations.length, 5);
  assert.ok(widgets.batch.operations.every(op => op.action === "select_choice" && op.element_id.startsWith("fresh-")));
  assert.deepEqual(widgets.batch.operations[0].choice_path, ["Option 0"]);
  const done = fresh.map((n, index) => ({ ...n, element_id: "last-" + index, value: actions[index].value }));
  assert.deepEqual(reconcileFill(widgets.plan_id, { ...scope, snapshot_generation: 3 }, done, complete(5)),
    { confirmed: 5, needs_attention: [] });
});
test("default one-widget boundary preserves continuation and follows rerendered sibling authority", () => {
  const { nodes, actions } = fixture(3);
  nodes[1].value = undefined;
  let current = nodes;
  let plan = createFillBatch(scope, nodes, actions);
  let confirmed = 0;
  for (let generation = 2; plan.batch; generation++) {
    assert.equal(plan.batch.operations.length, 1);
    const issued = plan.batch.operations[0];
    current = current.map((node, index) => ({ ...node, element_id: "generation-" + generation + "-" + index,
      ...(node.element_id === issued.element_id ? { value: issued.choice_path[0] } : {}) }));
    plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: generation }, current, complete(1));
    confirmed += plan.confirmed;
    assert.deepEqual(plan.needs_attention, []);
  }
  assert.equal(confirmed, 3);
});
test("custom action admission, 1..4 step bounds and 1..8 widget limit are enforced", () => {
  const { nodes, actions } = fixture(1);
  assert.equal(createFillBatch(scope, [{ ...nodes[0], actions: ["click"] }], actions).batch, undefined);
  assert.equal(createFillBatch(scope, [{ ...nodes[0], disabled: true }], actions).batch, undefined);
  for (const path of [[], ["x", "y", "z", "a", "b"], [" "], ["x\0y"], ["x".repeat(4097)]]) {
    assert.equal(validChoicePath(path), false);
    assert.equal(createFillBatch(scope, nodes, [{ ...actions[0], choice_path: path }]).batch, undefined);
  }
  assert.equal(validChoicePath(["x".repeat(4096)]), true);
  for (const limit of [0, 9, 1.5]) assert.throws(() => createFillBatch(scope, nodes, actions, [], limit), /widget batch limit/);
});
test("editable search text is not an already-selected choice before the typed operation completes", () => {
  const { nodes, actions } = fixture(1);
  for (const read_only of [false, undefined]) {
    const searching = { ...nodes[0], read_only, value: actions[0].value };
    const plan = createFillBatch(scope, [searching], actions);
    assert.deepEqual(plan.batch.operations, [{ action: "select_choice", element_id: searching.element_id,
      choice_path: actions[0].choice_path }]);
    assert.deepEqual(reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
      [{ ...searching, element_id: "after-completion" }], complete(1)), { confirmed: 1, needs_attention: [] });
    for (const committed of [
      { ...searching, selected: true },
      { ...searching, read_only: true },
      { ...searching, form_context: { ...searching.form_context, dom_tag: "div" } },
    ]) assert.deepEqual(createFillBatch(scope, [committed], actions), { needs_attention: [] });
  }
});
test("equal search text cannot confirm a deferred sibling or a choice without completed effect evidence", () => {
  const { nodes, actions } = fixture(2);
  const searching = nodes.map((node, i) => ({ ...node, read_only: false, value: actions[i].value }));
  const first = createFillBatch(scope, searching, actions);
  const second = reconcileFill(first.plan_id, { ...scope, snapshot_generation: 2 }, searching, complete(1));
  assert.equal(second.confirmed, 1);
  assert.equal(second.batch.operations[0].element_id, searching[1].element_id);
  assert.deepEqual(second.needs_attention, []);
  assert.deepEqual(reconcileFill(second.plan_id, { ...scope, snapshot_generation: 3 }, searching, complete(1)),
    { confirmed: 1, needs_attention: [] });
  const uncertain = createFillBatch(scope, [searching[0]], [actions[0]]);
  const unresolved = reconcileFill(uncertain.plan_id, { ...scope, snapshot_generation: 2 }, [searching[0]], {});
  assert.equal(unresolved.confirmed, 0);
  assert.equal(unresolved.batch, undefined);
  assert.equal(unresolved.needs_attention.length, 1);
  assert.equal(unresolved.needs_attention[0].status, "unresolved");
});
test("stable form signature survives changed display names; selected label can reconcile without reopening", () => {
  const { nodes, actions } = fixture(1);
  const plan = createFillBatch(scope, nodes, actions);
  const selected = { ...nodes[0], name: "Option 0", element_id: "rerendered", value: undefined, selected: true };
  assert.deepEqual(reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 }, [selected], complete(1)),
    { confirmed: 1, needs_attention: [] });
});
test("completed composite widget with leaf-only or unavailable semantic readback is not repeated", () => {
  const { nodes, actions } = fixture(1);
  const action = { ...actions[0], value: "Province / City", choice_path: ["Province", "City"] };
  assert.equal(choiceReadback({ ...nodes[0], value: " Province  /  City " }, action.choice_path), "confirmed");
  for (const value of ["City", "", "encoded-id-42", undefined]) {
    const plan = createFillBatch(scope, nodes, [action]);
    const result = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
      [{ ...nodes[0], value, element_id: "fresh" }], complete(1));
    assert.equal(result.confirmed, 0);
    assert.equal(result.batch, undefined);
    assert.equal(result.needs_attention[0].status, "unresolved");
    assert.match(result.needs_attention[0].reason, /without repeating/);
  }
});
test("composite readback preserves boundaries and never confirms an undelimited path collision", () => {
  const { nodes, actions } = fixture(1);
  for (const choice_path of [["AB", "C"], ["A", "BC"]]) {
    const action = { ...actions[0], value: choice_path.join(" / "), choice_path };
    assert.equal(choiceReadback({ ...nodes[0], value: "ABC" }, choice_path), "unresolved");
    assert.equal(choiceReadback({ ...nodes[0], value: choice_path.join(" / ") }, choice_path), "confirmed");
    const plan = createFillBatch(scope, nodes, [action]);
    const result = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
      [{ ...nodes[0], value: "ABC", element_id: "fresh" }], complete(1));
    assert.equal(result.confirmed, 0);
    assert.equal(result.batch, undefined);
    assert.equal(result.needs_attention[0].status, "unresolved");
  }
});
test("native select code/label differences need attention and stay non-replayable through deferred widgets", () => {
  const { nodes, actions } = fixture(2);
  nodes.unshift({ role: "combobox", name: "Gender", value: "", actionable: true,
    actions: ["select_option"], element_id: "native-select",
    form_context: { field_signature: "native-select", dom_tag: "select" } });
  actions.unshift({ kind: "select_option", element_id: "native-select", label: "Gender", value: "male", confidence: 1 });
  let plan = createFillBatch(scope, nodes, actions);
  assert.deepEqual(plan.batch.operations, [{ action: "select_option", element_id: "native-select", option: "male" }]);
  let current = nodes.map((node, i) => ({ ...node, element_id: "fresh-" + i, value: i === 0 ? "男" : "" }));
  plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 }, current, complete(1));
  assert.equal(plan.confirmed, 0);
  assert.deepEqual(plan.needs_attention.map(item => item.status), ["unresolved"]);
  assert.equal(plan.batch.operations[0].action, "select_choice");
  for (let i = 1; i <= 2; i++) {
    current = current.map((node, index) => ({ ...node, element_id: "step-" + i + "-" + index,
      value: index === i ? actions[index].value : node.value }));
    plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 + i }, current, complete(1));
    assert.equal(plan.confirmed, 1);
    assert.deepEqual(plan.needs_attention.map(item => item.label), ["Gender"]);
    assert.ok(plan.batch === undefined || plan.batch.operations.every(op => op.action === "select_choice"));
  }
  assert.equal(plan.batch, undefined);
  const satisfied = createFillBatch(scope, [{ ...nodes[0], value: "male" }], [actions[0]]);
  assert.deepEqual(satisfied, { needs_attention: [] });
});
test("partial widget batch confirms visible values but never retries or advances the uncertain boundary", () => {
  const { nodes, actions } = fixture(3);
  const plan = createFillBatch(scope, nodes, actions, [], 2);
  const result = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 },
    nodes.map((node, i) => ({ ...node, value: i === 0 ? actions[0].value : "", element_id: "fresh-" + i })),
    { execution_state: "outcome_unknown", requested_count: 2, completed_count: 1, remaining_count: 0,
      stopped_at_index: 1, stopped_execution_state: "outcome_unknown" });
  assert.equal(result.confirmed, 1);
  assert.equal(result.batch, undefined);
  assert.equal(result.needs_attention.length, 2);
});
test("completed widget exceptions stay non-replayable while other deferred widgets finish", () => {
  const { nodes, actions } = fixture(3);
  let plan = createFillBatch(scope, nodes, actions);
  let current = nodes.map((node, i) => ({ ...node, element_id: "second-" + i }));
  plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 }, current, complete(1));
  assert.equal(plan.batch.operations[0].element_id, "second-1");
  current = current.map((node, i) => ({ ...node, element_id: "third-" + i, value: i === 1 ? actions[i].value : "" }));
  plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 3 }, current, complete(1));
  assert.equal(plan.batch.operations[0].element_id, "third-2");
  current = current.map((node, i) => ({ ...node, element_id: "fourth-" + i, value: i > 0 ? actions[i].value : "" }));
  const done = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 4 }, current, complete(1));
  assert.equal(done.batch, undefined);
  assert.deepEqual(done.needs_attention.map(item => item.label), ["Field 0"]);
});

