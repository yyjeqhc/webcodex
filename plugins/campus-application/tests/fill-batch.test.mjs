import assert from "node:assert/strict";
import test from "node:test";
import { createFillBatch, reconcileFill } from "../dist/fill-batch.js";
const scope = { client_id: "fixture", browser_id: "browser", page_id: "page", url: "https://fixture.invalid/form", snapshot_generation: 1 };
const fresh = { ...scope, snapshot_generation: 2 };
function fixture(count) {
  const nodes = Array.from({ length: count }, (_, i) => ({ role: "textbox", name: `Field ${i}`, value: "",
    actionable: true, actions: ["input_text"], element_id: `old-${i}`,
    form_context: { field_signature: `sig-${i}`, dom_tag: "input" } }));
  const actions = nodes.map((node, i) => ({ kind: "input_text", element_id: node.element_id,
    value: `fictional-${i}`, label: node.name, confidence: 0.99 }));
  return { nodes, actions };
}
const receipt = count => ({ execution_state: "completed", requested_count: count, completed_count: count,
  remaining_count: 0, stability: { stable: true } });
function readback(nodes, actions, confirmed = nodes.length) {
  return nodes.map((node, i) => ({ ...node, element_id: `fresh-${i}`, value: i < confirmed ? actions[i].value : "" }));
}
test("20 fields: one executable batch, one readback, count-only success", () => {
  const { nodes, actions } = fixture(20);
  const plan = createFillBatch(scope, nodes, actions);
  assert.equal(plan.batch.operations.length, 20);
  assert.equal(plan.batch.action, "batch");
  assert.deepEqual(plan.batch.operations[0], { action: "input_text", element_id: "old-0", text: "fictional-0" });
  assert.equal(plan.actions, undefined);
  const result = reconcileFill(plan.plan_id, fresh, readback(nodes, actions), receipt(20));
  assert.deepEqual(result, { confirmed: 20, needs_attention: [] });
  assert.ok(JSON.stringify(result).length < 50);
  assert.throws(() => reconcileFill(plan.plan_id, fresh, nodes, receipt(20)), /expired/);
});
test("1..32 bound and 65 fields continue as 32/32/1 from fresh authority", () => {
  for (const count of [1, 32, 65]) {
    const { nodes, actions } = fixture(count);
    let plan = createFillBatch(scope, nodes, actions);
    let done = 0, generation = 1;
    const sizes = [];
    while (plan.batch) {
      sizes.push(plan.batch.operations.length);
      done += plan.batch.operations.length;
      plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: ++generation }, readback(nodes, actions, done), receipt(plan.batch.operations.length));
    }
    assert.deepEqual(sizes, count === 65 ? [32, 32, 1] : [count]);
    assert.deepEqual(plan.needs_attention, []);
  }
  assert.equal(createFillBatch(scope, [], []).batch, undefined);
});
test("initial blockers are delta-only and are not repeated across deferred batches", () => {
  const { nodes, actions } = fixture(65);
  const blocker = { label: "Custom picker", status: "unresolved", reason: "manual widget" };
  let plan = createFillBatch(scope, nodes, actions, [blocker]);
  assert.deepEqual(plan.needs_attention, [blocker]);
  let done = plan.batch.operations.length;
  plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 2 }, readback(nodes, actions, done), receipt(32));
  assert.deepEqual(plan.needs_attention, []);
  done += plan.batch.operations.length;
  plan = reconcileFill(plan.plan_id, { ...scope, snapshot_generation: 3 }, readback(nodes, actions, done), receipt(32));
  assert.deepEqual(plan.needs_attention, []);
});
test("18 confirmed, one mismatch, one unresolved: only fresh mismatch enters retry", () => {
  const { nodes, actions } = fixture(20);
  const plan = createFillBatch(scope, nodes, actions);
  const observed = readback(nodes, actions, 18).slice(0, 19);
  const result = reconcileFill(plan.plan_id, fresh, observed, receipt(20));
  assert.equal(result.confirmed, 18);
  assert.deepEqual(result.needs_attention.map(x => x.status), ["mismatch", "unresolved"]);
  assert.deepEqual(result.batch.operations.map(x => x.element_id), ["fresh-18"]);
  assert.ok(!JSON.stringify(result).includes("fictional-0"));
});
test("partial and outcome unknown preserve counts but never return an automatic replay", () => {
  for (const execution_state of ["completed", "outcome_unknown"]) {
    const { nodes, actions } = fixture(20);
    const plan = createFillBatch(scope, nodes, actions);
    const result = reconcileFill(plan.plan_id, fresh, readback(nodes, actions, 18), {
      execution_state, requested_count: 20, completed_count: 18,
      remaining_count: execution_state === "completed" ? 2 : 1,
      stopped_at_index: 18, stopped_execution_state: execution_state === "completed" ? "not_started" : "outcome_unknown",
    });
    assert.equal(result.confirmed, 18);
    assert.equal(result.needs_attention.length, 2);
    assert.equal(result.batch, undefined);
    assert.match(result.needs_attention[0].reason, execution_state === "completed" ? /Definitely unstarted/ : /Effect uncertain/);
  }
});
test("missing counts, unstable completion and outer unknown cannot trigger replay", () => {
  for (const r of [{}, { ...receipt(1), stability: { stable: false } }, { ...receipt(1), execution_state: "outcome_unknown" }]) {
    const { nodes, actions } = fixture(1);
    const plan = createFillBatch(scope, nodes, actions);
    assert.equal(reconcileFill(plan.plan_id, fresh, readback(nodes, actions, 0), r).batch, undefined);
  }
});
test("stale snapshots, changed URL/page/browser/runner reject; no old element reuse", () => {
  const { nodes, actions } = fixture(1);
  const plan = createFillBatch(scope, nodes, actions);
  for (const changed of [scope, { ...fresh, url: "https://fixture.invalid/next" },
    { ...fresh, page_id: "other" }, { ...fresh, browser_id: "other" }, { ...fresh, client_id: "other" }]) {
    assert.throws(() => reconcileFill(plan.plan_id, changed, nodes, receipt(1)), /Fresh same-target/);
  }
  const result = reconcileFill(plan.plan_id, fresh, [{ ...nodes[0], element_id: "new-authority" }], receipt(1));
  assert.equal(result.batch.operations[0].element_id, "new-authority");
});
test("document replacement receipt stops even with same-URL matching fields", () => {
  const { nodes, actions } = fixture(2);
  const plan = createFillBatch(scope, nodes, actions);
  const result = reconcileFill(plan.plan_id, fresh, readback(nodes, actions, 1), {
    execution_state: "completed", requested_count: 2, completed_count: 1, remaining_count: 1,
    stopped_at_index: 1, stopped_execution_state: "not_started",
  });
  assert.equal(result.batch, undefined);
  assert.equal(result.confirmed, 1);
});
test("admission, ambiguity, disabled controls, custom selects and confidence are not guessed", () => {
  for (const change of [{ actions: [] }, { disabled: true }, { read_only: true }, { actionable: false }]) {
    const { nodes, actions } = fixture(1);
    assert.equal(createFillBatch(scope, [{ ...nodes[0], ...change }], actions).batch, undefined);
  }
  const { nodes, actions } = fixture(1);
  assert.equal(createFillBatch(scope, [...nodes, { ...nodes[0], element_id: "duplicate" }], actions).batch, undefined);
  assert.equal(createFillBatch(scope, nodes, [{ ...actions[0], confidence: 0.88 }]).batch, undefined);
  const select = { ...nodes[0], role: "combobox", actions: ["select_option"], form_context: { field_signature: "native", dom_tag: "select" } };
  assert.equal(createFillBatch(scope, [select], [{ ...actions[0], kind: "select_option" }]).batch.operations[0].option, "fictional-0");
  assert.equal(createFillBatch(scope, [{ ...select, form_context: { ...select.form_context, dom_tag: "div" } }], [{ ...actions[0], kind: "select_option" }]).batch, undefined);
});
test("bounded plan retention evicts oldest without returning sensitive expectations", () => {
  const { nodes, actions } = fixture(1);
  const old = createFillBatch(scope, nodes, actions);
  for (let i = 0; i < 32; i++) createFillBatch(scope, nodes, actions);
  assert.throws(() => reconcileFill(old.plan_id, fresh, nodes, receipt(1)), /expired/);
});
test("unresolved fields persist across the next batch, confirmed fields do not", () => {
  const { nodes, actions } = fixture(3);
  const initial = createFillBatch(scope, nodes, actions);
  const first = reconcileFill(initial.plan_id, fresh, readback(nodes, actions, 1).slice(0, 2), receipt(3));
  assert.equal(first.confirmed, 1);
  assert.equal(first.batch.operations.length, 1);
  const second = reconcileFill(first.plan_id, { ...scope, snapshot_generation: 3 }, readback(nodes, actions, 2).slice(0, 2), receipt(1));
  assert.equal(second.confirmed, 1);
  assert.deepEqual(second.needs_attention.map(x => x.label), ["Field 2"]);
  assert.equal(second.batch, undefined);
});
test("insert-only old Runner cannot append to a nonempty field", () => {
  const { nodes, actions } = fixture(1);
  assert.equal(createFillBatch(scope, [{ ...nodes[0], value: "existing" }], actions).batch, undefined);
  const native = { ...nodes[0], value: "existing", actions: ["set_value"] };
  assert.equal(createFillBatch(scope, [native], [{ ...actions[0], kind: "set_value" }]).batch.operations[0].value, "fictional-0");
});
test("clipped readback is unresolved, never an automatic repeat of long text", () => {
  const { nodes, actions } = fixture(2);
  nodes.forEach(n => { n.actions = ["set_value"]; });
  actions.forEach(a => { a.kind = "set_value"; });
  actions[0].value = "x".repeat(600);
  const plan = createFillBatch(scope, nodes, actions);
  const result = reconcileFill(plan.plan_id, fresh, [{ ...nodes[0], value: "x".repeat(512) }, { ...nodes[1], value: "wrong" }], receipt(2));
  assert.equal(result.needs_attention[0].status, "unresolved");
  assert.equal(result.batch.operations.length, 1);
  assert.equal(result.batch.operations[0].element_id, nodes[1].element_id);
});
test("reported validation failure retains the exception without a repeat effect", () => {
  const { nodes, actions } = fixture(1);
  nodes[0].actions = ["set_value"];
  actions[0].kind = "set_value";
  const plan = createFillBatch(scope, nodes, actions);
  const observed = readback(nodes, actions);
  observed[0].form_context = { ...observed[0].form_context, aria_invalid: true };
  const result = reconcileFill(plan.plan_id, fresh, observed, receipt(1));
  assert.equal(result.confirmed, 0);
  assert.equal(result.needs_attention[0].status, "mismatch");
  assert.equal(result.batch, undefined);
});
