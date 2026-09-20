import test from "node:test";
import assert from "node:assert/strict";
import { app, toolResult } from "./app_test_support.mjs";

const goal_id = "wc_goal_G4G4G4G4G4G4G4G4";
const agent_id = "wc_dagent_AgAgAgAgAgAgAgAg";
const step = (id, status) => ({ id, title: id, status, updated_at_unix_ms: 1000 });
const plan = {
  version: 3, goal_id, title: "Single-window workflow", controller_agent_id: agent_id,
  total_step_count: 5, completed_step_count: 2, current_step_id: "validate",
  steps: [step("inspect", "completed"), step("implement", "completed"), step("validate", "in_progress"), step("review", "pending"), step("closeout", "pending")],
  progress_summary: "Implementation complete; run focused validation", checkpoint_at_unix_ms: 1000,
  lifecycle: "active", revision: 3, updated_at_unix_ms: 1000, terminal_at_unix_ms: null,
  agent_task_count: 0, workflow_session_count: 1,
  activity: {
    available: true, state: "attention_needed", idle_threshold_ms: 300000,
    last_seen_at_ms: 301000, last_meaningful_activity_at_ms: 1000, quiet_for_ms: 300000,
    linked_window_count: 1, active_meaningful_request_count: 0, coverage_partial: false,
  },
  continuity: {
    available: true, state: "stalled", production_auto_resume_available: false,
    wake_state: null, host_delivery: "not_started", fresh_turn: "not_confirmed",
    last_resume_at_unix_ms: null,
  },
};
const attention = {
  state_changed: true,
  attention: { event_id: "wc_attention_event_EvEvEvEvEvEvEvEv", wake_id: "wc_wake_WaWaWaWaWaWaWaWa", created: true },
};

async function ready(value = plan) {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput({ goal_id });
  await view.reply(view.calls("goal_plan_state")[0], toolResult({ goal_plan: value }));
  return view;
}

test("Goal workflow renders sparse mechanical progress and sends only an exact detector selector", async () => {
  const view = await ready();
  assert.equal(view.nodes.progress.textContent, "Step 3 / 5");
  assert.equal(view.nodes.steps.textContent, "✓ inspect\n✓ implement\n→ validate\n   review\n   closeout");
  assert.equal(view.nodes.checkpoint.textContent, `Last checkpoint: ${plan.progress_summary}`);
  assert.equal(view.calls("goal_plan_recheck_attention").length, 1);
  assert.deepEqual({ ...view.calls("goal_plan_recheck_attention")[0].params.arguments }, { goal_id });
  assert(!view.sent.some(message => message.method === "ui/message"));
  await view.reply(view.calls("goal_plan_recheck_attention")[0], toolResult(attention));
  assert(!view.nodes.status.textContent.includes("resumed"));
});

test("10,000 same-epoch polls cannot create additional detector calls or Host turns after durable attention", async () => {
  const view = await ready();
  await view.reply(view.calls("goal_plan_recheck_attention")[0], toolResult(attention));
  for (let index = 0; index < 10000; index++) {
    await view.fireTimers(3000);
    await view.reply(view.calls("goal_plan_state").at(-1), toolResult({ goal_plan: plan }));
  }
  assert.equal(view.calls("goal_plan_state").length, 10001);
  assert.equal(view.calls("goal_plan_recheck_attention").length, 1);
  assert(!view.sent.some(message => message.method === "ui/message"));
});

test("a new meaningful-work epoch can be rechecked, but Goal revision changes alone cannot", async () => {
  const view = await ready();
  await view.reply(view.calls("goal_plan_recheck_attention")[0], toolResult(attention));
  await view.fireTimers(3000);
  await view.reply(view.calls("goal_plan_state").at(-1), toolResult({ goal_plan: { ...plan, revision: 4 } }));
  assert.equal(view.calls("goal_plan_recheck_attention").length, 1);
  await view.fireTimers(3000);
  await view.reply(view.calls("goal_plan_state").at(-1), toolResult({ goal_plan: {
    ...plan, revision: 4, activity: { ...plan.activity, state: "active", last_meaningful_activity_at_ms: 400000, quiet_for_ms: 0 },
  } }));
  assert.equal(view.calls("goal_plan_recheck_attention").length, 1);
  await view.fireTimers(3000);
  await view.reply(view.calls("goal_plan_state").at(-1), toolResult({ goal_plan: {
    ...plan, revision: 4, activity: { ...plan.activity, last_meaningful_activity_at_ms: 400000, last_seen_at_ms: 700000 },
  } }));
  assert.equal(view.calls("goal_plan_recheck_attention").length, 2);
});

test("ineligible or uncertain detector results may recheck authoritative state, without dispatch or local event invention", async () => {
  const view = await ready();
  await view.reply(view.calls("goal_plan_recheck_attention")[0], toolResult({ state_changed: false, attention: null }));
  await view.fireTimers(3000);
  await view.reply(view.calls("goal_plan_state").at(-1), toolResult({ goal_plan: plan }));
  assert.equal(view.calls("goal_plan_recheck_attention").length, 2);
  await view.reply(view.calls("goal_plan_recheck_attention")[1], toolResult({ state_changed: false, attention: { ...attention.attention, created: false } }));
  assert(!view.sent.some(message => message.method === "ui/message"));
  assert(!view.nodes.status.textContent.includes("resumed"));
});

for (const [name, change] of [
  ["no controller", { controller_agent_id: null, continuity: {
    available: true, state: "not_configured", production_auto_resume_available: false,
    wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
    last_resume_at_unix_ms: null,
  } }],
  ["active work", { activity: { ...plan.activity, state: "active" }, continuity: { ...plan.continuity, state: "ready" } }],
  ["inflight work", { activity: { ...plan.activity, active_meaningful_request_count: 1 }, continuity: { ...plan.continuity, state: "ready" } }],
  ["partial evidence", { activity: { ...plan.activity, coverage_partial: true }, continuity: {
    available: false, state: "unavailable", production_auto_resume_available: false,
    wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
    last_resume_at_unix_ms: null,
  } }],
  ["unobserved", { activity: { ...plan.activity, state: "unobserved" }, continuity: { ...plan.continuity, state: "ready" } }],
]) {
  test(`Goal detector does not request continuation with ${name}`, async () => {
    const view = await ready({ ...plan, ...change });
    assert.equal(view.calls("goal_plan_recheck_attention").length, 0);
    assert(!view.sent.some(message => message.method === "ui/message"));
  });
}

test("terminal Goal stops both polling and in-flight detector follow-up", async () => {
  const view = await ready();
  view.toolResult({ goal_plan: {
    ...plan, lifecycle: "completed", revision: 4, terminal_at_unix_ms: 302000,
    completed_step_count: 5, current_step_id: null, steps: plan.steps.map(step => ({ ...step, status: "completed" })),
    activity: { available: false, state: "not_applicable", idle_threshold_ms: 300000,
      last_seen_at_ms: null, last_meaningful_activity_at_ms: null, quiet_for_ms: null,
      linked_window_count: null, active_meaningful_request_count: null, coverage_partial: false },
    continuity: { available: true, state: "not_applicable", production_auto_resume_available: false,
      wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
      last_resume_at_unix_ms: null },
  } });
  await view.reply(view.calls("goal_plan_recheck_attention")[0], toolResult(attention));
  await view.fireTimers(3000);
  assert.equal(view.nodes.lifecycle.textContent, "Completed");
  assert.equal(view.nodes.progress.textContent, "5 / 5 steps complete");
  assert.equal(view.calls("goal_plan_state").length, 1);
  assert.equal(view.calls("goal_plan_recheck_attention").length, 1);
  assert.equal(view.timers.size, 0);
});

for (const [name, change] of [
  ["old wire", { version: 2 }],
  ["wrong step count", { total_step_count: 6 }],
  ["wrong completed count", { completed_step_count: 3 }],
  ["unknown current", { current_step_id: "missing" }],
  ["duplicate ids", { steps: [...plan.steps.slice(0, 4), step("inspect", "pending")] }],
  ["two current steps", { steps: [...plan.steps.slice(0, 4), step("closeout", "in_progress")] }],
  ["incomplete completion", { lifecycle: "completed" }],
  ["oversize summary", { progress_summary: "é".repeat(1025) }],
  ["oversize title", { steps: [{ ...plan.steps[0], title: "x".repeat(121) }, ...plan.steps.slice(1)] }],
  ["missing checkpoint time", { checkpoint_at_unix_ms: null }],
]) {
  test(`Goal Plan fails closed on ${name}`, async () => {
    const view = await ready({ ...plan, ...change });
    assert.equal(view.nodes.status.textContent, "State refresh pending");
    assert.equal(view.calls("goal_plan_recheck_attention").length, 0);
  });
}
