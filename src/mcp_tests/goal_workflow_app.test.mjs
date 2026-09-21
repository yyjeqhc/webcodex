import test from "node:test";
import assert from "node:assert/strict";
import { app, toolResult } from "./app_test_support.mjs";

const goal_id = "wc_goal_G4G4G4G4G4G4G4G4";
const agent_id = "wc_dagent_AgAgAgAgAgAgAgAg";
const latency = {
  attention_candidate_at_unix_ms: null,
  attention_created_at_unix_ms: null,
  wake_created_at_unix_ms: null,
  host_dispatch_prepared_at_unix_ms: null,
  host_dispatch_accepted_at_unix_ms: null,
  host_dispatch_unknown_at_unix_ms: null,
  wake_consumed_at_unix_ms: null,
  first_post_resume_meaningful_at_unix_ms: null,
  last_post_resume_meaningful_at_unix_ms: null,
};
const step = (id, status) => ({ id, title: id, status, updated_at_unix_ms: 1000 });
const plan = {
  version: 3, goal_id, title: "Single-window workflow", controller_agent_id: agent_id,
  total_step_count: 5, completed_step_count: 2, current_step_id: "validate",
  steps: [step("inspect", "completed"), step("implement", "completed"), step("validate", "in_progress"), step("review", "pending"), step("closeout", "pending")],
  progress_summary: "Implementation complete; run focused validation", checkpoint_at_unix_ms: 1000,
  lifecycle: "active", revision: 3, updated_at_unix_ms: 1000, terminal_at_unix_ms: null,
  agent_task_count: 0, workflow_session_count: 1,
  activity: {
    available: true, state: "active", idle_threshold_ms: 300000, observation_lease_ms: 75000,
    last_seen_at_ms: 1000, last_meaningful_activity_at_ms: 1000, quiet_for_ms: 0,
    linked_window_count: 1, active_meaningful_request_count: 0, coverage_partial: false,
  },
  continuity: {
    available: true, state: "ready", production_auto_resume_available: true,
    wake_state: null, host_delivery: "not_started", fresh_turn: "not_confirmed",
    ...latency, last_resume_at_unix_ms: null,
  },
};

async function ready(value = plan) {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput({ goal_id });
  await view.reply(view.calls("goal_plan_sync")[0], toolResult({ goal_plan: value }));
  return view;
}

test("Goal Plan uses one exact effectful sync primitive and never performs a conditional second RPC", async () => {
  const view = await ready();
  assert.equal(view.nodes.progress.textContent, "Step 3 / 5");
  assert.equal(view.nodes.steps.textContent, "✓ inspect\n✓ implement\n→ validate\n   review\n   closeout");
  assert.equal(view.calls("goal_plan_sync").length, 1);
  assert.deepEqual({ ...view.calls("goal_plan_sync")[0].params.arguments }, { goal_id });
  assert.equal(view.calls("goal_plan_recheck_attention").length, 0);
  assert(!view.sent.some(message => message.method === "ui/message"));
});

test("visible normal polling is serial and uses the 12s cadence", async () => {
  const view = await ready();
  assert.equal(view.timers.size, 1);
  await view.fireTimers(5000);
  assert.equal(view.calls("goal_plan_sync").length, 1);
  await view.fireTimers(12000);
  assert.equal(view.calls("goal_plan_sync").length, 2);
  assert.equal(view.timers.size, 1);
  await view.reply(view.calls("goal_plan_sync")[1], toolResult({ goal_plan: plan }));
  assert.equal(view.timers.size, 1);
});

test("near-attention and active Wake transitions temporarily use the 5s cadence", async () => {
  const near = {
    ...plan,
    activity: { ...plan.activity, quiet_for_ms: 250000, last_seen_at_ms: 251000 },
  };
  const view = await ready(near);
  await view.fireTimers(5000);
  assert.equal(view.calls("goal_plan_sync").length, 2);
  const waking = {
    ...near,
    continuity: {
      ...near.continuity,
      state: "wake_queued", wake_state: "pending",
      attention_candidate_at_unix_ms: 301000,
      attention_created_at_unix_ms: 301100,
      wake_created_at_unix_ms: 301100,
    },
  };
  await view.reply(view.calls("goal_plan_sync")[1], toolResult({ goal_plan: waking }));
  await view.fireTimers(5000);
  assert.equal(view.calls("goal_plan_sync").length, 3);
  assert.equal(view.calls("goal_plan_recheck_attention").length, 0);
});

test("hidden cards use 60s polling, stay inside the 75s observation lease, and refresh immediately when visible", async () => {
  const view = await ready();
  await view.visibility(true);
  await view.fireTimers(12000);
  assert.equal(view.calls("goal_plan_sync").length, 1);
  await view.fireTimers(60000);
  assert.equal(view.calls("goal_plan_sync").length, 2);
  await view.reply(view.calls("goal_plan_sync")[1], toolResult({ goal_plan: plan }));
  await view.visibility(false);
  assert.equal(view.calls("goal_plan_sync").length, 3);
});

test("terminal Goal stops polling and late visibility changes cannot restart it", async () => {
  const view = await ready();
  await view.fireTimers(12000);
  const request = view.calls("goal_plan_sync")[1];
  const terminal = {
    ...plan, lifecycle: "completed", revision: 4, terminal_at_unix_ms: 302000,
    completed_step_count: 5, current_step_id: null,
    steps: plan.steps.map(item => ({ ...item, status: "completed" })),
    activity: {
      available: false, state: "not_applicable", idle_threshold_ms: 300000, observation_lease_ms: 75000,
      last_seen_at_ms: null, last_meaningful_activity_at_ms: null, quiet_for_ms: null,
      linked_window_count: null, active_meaningful_request_count: null, coverage_partial: false,
    },
    continuity: {
      available: true, state: "not_applicable", production_auto_resume_available: false,
      wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
      ...latency, last_resume_at_unix_ms: null,
    },
  };
  await view.reply(request, toolResult({ goal_plan: terminal }));
  assert.equal(view.timers.size, 0);
  await view.visibility(true);
  await view.visibility(false);
  assert.equal(view.calls("goal_plan_sync").length, 2);
  assert.equal(view.nodes.lifecycle.textContent, "Completed");
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test("Goal Plan " + method + " tears down the serial scheduler and ignores late sync results", async () => {
    const view = await ready();
    await view.fireTimers(12000);
    const request = view.calls("goal_plan_sync")[1];
    await view.teardown(method);
    await view.reply(request, toolResult({ goal_plan: { ...plan, title: "late" } }));
    assert.equal(view.timers.size, 0);
    assert.equal(view.nodes.title.textContent, plan.title);
  });
}

test("historical resume proof remains visible after current epoch returns to ready", async () => {
  const resumedHistory = {
    ...plan,
    continuity: {
      ...plan.continuity,
      state: "ready", wake_state: null,
      host_delivery: "accepted", fresh_turn: "confirmed",
      attention_candidate_at_unix_ms: 301000,
      attention_created_at_unix_ms: 301100,
      wake_created_at_unix_ms: 301100,
      host_dispatch_prepared_at_unix_ms: 301200,
      host_dispatch_accepted_at_unix_ms: 301250,
      host_dispatch_unknown_at_unix_ms: null,
      wake_consumed_at_unix_ms: 301500,
      first_post_resume_meaningful_at_unix_ms: 301800,
      last_post_resume_meaningful_at_unix_ms: 302000,
      last_resume_at_unix_ms: 301500,
    },
  };
  const view = await ready(resumedHistory);
  assert.equal(view.nodes["continuity-state"].textContent, "Ready");
  assert.equal(view.nodes["host-delivery"].textContent, "Accepted");
  assert.equal(view.nodes["fresh-turn"].textContent, "Confirmed");
  assert.match(view.nodes["last-resume"].textContent, /^Confirmed/);
});

test("latency fields are bounded projection only and do not change fresh-turn semantics", async () => {
  const resumed = {
    ...plan,
    continuity: {
      ...plan.continuity,
      state: "resume_confirmed", wake_state: "consumed",
      host_delivery: "accepted", fresh_turn: "confirmed",
      attention_candidate_at_unix_ms: 301000,
      attention_created_at_unix_ms: 301100,
      wake_created_at_unix_ms: 301100,
      host_dispatch_prepared_at_unix_ms: 301200,
      host_dispatch_accepted_at_unix_ms: 301250,
      host_dispatch_unknown_at_unix_ms: null,
      wake_consumed_at_unix_ms: 301500,
      first_post_resume_meaningful_at_unix_ms: 301800,
      last_post_resume_meaningful_at_unix_ms: 302000,
      last_resume_at_unix_ms: 301500,
    },
  };
  const view = await ready(resumed);
  assert.equal(view.nodes["fresh-turn"].textContent, "Confirmed");
  assert.equal(view.nodes["host-delivery"].textContent, "Accepted");
  assert.match(view.nodes["last-resume"].textContent, /^Confirmed/);
});
