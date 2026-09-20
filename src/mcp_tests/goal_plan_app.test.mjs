import test from "node:test";
import assert from "node:assert/strict";
import { app, flush, toolResult } from "./app_test_support.mjs";

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

const plan = {
  version: 3, goal_id: `wc_goal_ERERERERERERERER`, title: "Ship Goal",
  total_step_count: 0, completed_step_count: 0, current_step_id: null, steps: [],
  progress_summary: null, checkpoint_at_unix_ms: null, lifecycle: "active", revision: 1,
  updated_at_unix_ms: 1000, terminal_at_unix_ms: null,
  controller_agent_id: null,
  agent_task_count: 0, workflow_session_count: 0,
  activity: {
    available: true, state: "active", idle_threshold_ms: 300000, observation_lease_ms: 75000,
    last_seen_at_ms: 1000, last_meaningful_activity_at_ms: 1000, quiet_for_ms: 0,
    linked_window_count: 1, active_meaningful_request_count: 0, coverage_partial: false,
  },
  continuity: {
    available: true, state: "not_configured", production_auto_resume_available: false,
    wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
    ...latency, last_resume_at_unix_ms: null,
  },
};
const terminalPlan = (revision = 2) => ({
  ...plan, lifecycle: "completed", revision, terminal_at_unix_ms: 2000,
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
});
const input = { goal_id: plan.goal_id };

for (const outcome of ["success", "error", "timeout"]) {
  for (const early of [true, false]) {
    test(`Goal input ${early ? "before" : "after"} initialize ${outcome}, without initial result`, async () => {
      const view = app("mcp_goal_plan_app.html");
      assert.equal(view.nodes.status.textContent, "App active · initializing Host");
      if (early) view.toolInput(input);
      assert.equal(view.calls("goal_plan_sync").length, 0);
      await view.initialize(outcome);
      if (outcome === "success" && !early) {
        assert.equal(view.nodes.status.textContent, "Host initialized · waiting for exact tool identity");
      }
      if (!early) view.toolInput(input);
      assert.equal(view.calls("goal_plan_sync").length, outcome === "success" ? 1 : 0);
      assert.equal(view.nodes.status.textContent, outcome === "success"
        ? "Exact identity received · reading authoritative Goal state" : "Host initialization unavailable");
      if (outcome === "success") {
        assert.deepEqual({ ...view.calls("goal_plan_sync")[0].params.arguments }, input);
        await view.reply(view.calls("goal_plan_sync")[0], toolResult({ goal_plan: plan }));
        view.toolInput(input);
        assert.equal(view.calls("goal_plan_sync").length, 1);
        await view.fireTimers(12000);
        assert.equal(view.calls("goal_plan_sync").length, 2);
        await view.reply(view.calls("goal_plan_sync")[1], toolResult({ goal_plan: terminalPlan() }));
        assert.equal(view.nodes.lifecycle.textContent, "Completed");
        assert.equal(view.nodes.activity.textContent, "Window activity observation is not applicable to this terminal Goal.");
        assert.equal(view.timers.size, 0);
      }
    });
  }
}

for (const order of [
  ["initialize", "input", "result"], ["input", "initialize", "result"],
  ["result", "initialize", "input"], ["initialize", "result", "input"],
  ["input", "result", "initialize"], ["result", "input", "initialize"],
]) {
  test(`matching Goal bootstrap is idempotent: ${order.join(" -> ")}`, async () => {
    const view = app("mcp_goal_plan_app.html");
    for (const step of order) {
      if (step === "initialize") await view.initialize();
      else if (step === "input") view.toolInput(input);
      else view.toolResult({ goal_plan: plan });
    }
    await view.reply(view.calls("goal_plan_sync")[0], toolResult({ goal_plan: { ...plan, title: "Current", revision: 2 } }));
    view.toolInput(input);
    view.toolInput(input);
    view.toolResult({ goal_plan: plan });
    assert.equal(view.calls("goal_plan_sync").length, 1);
    assert.equal(view.nodes.title.textContent, "Current");
    await view.fireTimers(12000);
    assert.equal(view.calls("goal_plan_sync").length, 2);
  });
}

for (const stage of ["initialize", "poll", "idle"]) {
  for (const first of ["input", "result"]) {
    test(`Goal ${first}-first conflict while ${stage} is pending stops exact polling`, async () => {
      const view = app("mcp_goal_plan_app.html");
      if (first === "input") view.toolInput(input);
      else view.toolResult({ goal_plan: plan });
      let pending = view.sent[0];
      let response = { protocolVersion: "2026-01-26" };
      if (stage !== "initialize") {
        await view.initialize();
        pending = view.calls("goal_plan_sync")[0];
        response = toolResult({ goal_plan: plan });
      }
      if (stage === "idle") await view.reply(pending, response);
      const foreign = `wc_goal_IiIiIiIiIiIiIiIi`;
      if (first === "input") view.toolResult({ goal_plan: { ...plan, goal_id: foreign, title: "Foreign" } });
      else view.toolInput({ goal_id: foreign });
      const count = view.sent.length;
      await view.reply(pending, response);
      view.toolInput(input);
      view.toolResult({ goal_plan: { ...plan, title: "Late", revision: 2 } });
      await view.fireTimers(12000);
      await view.visibility(false);
      assert.equal(view.sent.length, count);
      assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
      assert.notEqual(view.nodes.title?.textContent, "Foreign");
      assert.notEqual(view.nodes.title?.textContent, "Late");
      assert.equal(view.timers.size, 0);
      for (const call of view.calls("goal_plan_sync")) assert.deepEqual({ ...call.params.arguments }, input);
    });
  }
}

for (const goal_id of [undefined, null, "", "wc_goal_1", `wc_goal_${"A".repeat(32)}`, plan.goal_id + "extra", [plan.goal_id], 1]) {
  test(`invalid Goal input cannot start polling: ${JSON.stringify(goal_id)}`, async () => {
    const view = app("mcp_goal_plan_app.html");
    view.toolInput({ goal_id });
    await view.initialize();
    view.toolResult({ goal_plan: plan });
    await view.fireTimers(12000);
    assert.equal(view.calls("goal_plan_sync").length, 0);
    assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
    assert.equal(view.timers.size, 0);
  });
}

test("Goal accepts only parent complete input with canonical arguments", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput(input, {});
  view.notification("ui/notifications/tool-input-partial", { arguments: input });
  assert.equal(view.calls("goal_plan_sync").length, 0);
  assert.equal(view.nodes.status.textContent, "Host initialized · waiting for exact tool identity");
  view.notification("ui/notifications/tool-input", { input });
  assert.equal(view.calls("goal_plan_sync").length, 0);
  assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
});

test("an input-only Goal retries an unavailable first read without another notification", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput(input);
  await view.fireTimers(10000);
  assert.equal(view.nodes.status.textContent, "State refresh pending");
  await view.fireTimers(12000);
  assert.equal(view.calls("goal_plan_sync").length, 2);
  await view.reply(view.calls("goal_plan_sync")[1], toolResult({ goal_plan: plan }));
  assert.equal(view.nodes.status.textContent, "Tracking authoritative Goal state");
});


test("Goal accepts a nested CallToolResult returned by the Host bridge", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput(input);
  await view.reply(view.calls("goal_plan_sync")[0], { result: toolResult({ goal_plan: plan }) });
  assert.equal(view.nodes.title.textContent, plan.title);
  assert.equal(view.nodes.lifecycle.textContent, "Active");
  assert.equal(view.nodes.status.textContent, "Tracking authoritative Goal state");
});

test("a conflicting nested CallToolResult still stops Goal polling", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput(input);
  await view.reply(view.calls("goal_plan_sync")[0], { result: toolResult({ goal_plan: {
    ...plan, goal_id: `wc_goal_IiIiIiIiIiIiIiIi`,
  } }) });
  assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
  assert.equal(view.timers.size, 0);
});

test("a conflicting authoritative Goal response stops polling", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolInput(input);
  await view.reply(view.calls("goal_plan_sync")[0], toolResult({ goal_plan: {
    ...plan, goal_id: `wc_goal_IiIiIiIiIiIiIiIi`,
  } }));
  assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
  assert.equal(view.timers.size, 0);
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test(`input-only Goal ${method} discards a pending poll and stops timers`, async () => {
    const view = app("mcp_goal_plan_app.html");
    view.toolInput(input);
    await view.initialize();
    await view.teardown(method);
    await view.reply(view.calls("goal_plan_sync")[0], toolResult({ goal_plan: plan }));
    const count = view.sent.length;
    view.toolInput(input);
    view.toolResult({ goal_plan: plan });
    await view.visibility(false);
    await view.fireTimers(12000);
    assert.equal(view.sent.length, count);
    assert.equal(view.timers.size, 0);
    assert.notEqual(view.nodes.title?.textContent, plan.title);
  });
}

for (const outcome of ["success", "error", "timeout"]) {
  for (const early of [true, false]) {
    test(`Goal result ${early ? "before" : "after"} initialize ${outcome}`, async () => {
      const view = app("mcp_goal_plan_app.html");
      if (early) {
        view.toolResult({ goal_plan: plan });
        await view.fireTimers(12000);
        assert.equal(view.calls("goal_plan_sync").length, 0);
      }
      await view.initialize(outcome);
      if (!early) view.toolResult({ goal_plan: plan });
      await view.fireTimers(12000);
      assert.equal(view.nodes.title.textContent, plan.title);
      assert.equal(view.calls("goal_plan_sync").length, outcome === "success" ? 1 : 0);
    });
  }
}

test("Goal activity refreshes on the same authoritative revision in both directions", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolResult({ goal_plan: plan });
  assert.match(view.nodes.activity.textContent, /observed recently/);
  assert.equal(view.nodes.revision.textContent, "1");

  await view.fireTimers(12000);
  const attention = {
    ...plan,
    activity: {
      ...plan.activity,
      state: "attention_needed",
      last_seen_at_ms: 480000,
      last_meaningful_activity_at_ms: 1000,
      quiet_for_ms: 480000,
    },
  };
  await view.reply(view.calls("goal_plan_sync").at(-1), toolResult({ goal_plan: attention }));
  assert.match(view.nodes.activity.textContent, /does not establish that the Window is offline/);
  assert.match(view.nodes.activity.textContent, /may need attention/);
  assert.equal(view.nodes.revision.textContent, "1");

  await view.fireTimers(5000);
  const running = {
    ...plan,
    activity: { ...plan.activity, active_meaningful_request_count: 1 },
  };
  await view.reply(view.calls("goal_plan_sync").at(-1), toolResult({ goal_plan: running }));
  assert.equal(view.nodes.activity.textContent, "Meaningful WebCodex work is currently running.");
  assert.equal(view.nodes.revision.textContent, "1");
  assert.equal(view.timers.size, 1);
});

test("Goal continuity keeps Host-carrier readiness visible when activity evidence fails closed", async () => {
  const controller = `wc_dagent_PaPaPaPaPaPaPaPa`;
  const unavailable = {
    ...plan,
    controller_agent_id: controller,
    activity: {
      available: false, state: "unobserved", idle_threshold_ms: 300000, observation_lease_ms: 75000,
      last_seen_at_ms: null, last_meaningful_activity_at_ms: null, quiet_for_ms: null,
      linked_window_count: null, active_meaningful_request_count: null, coverage_partial: true,
    },
    continuity: {
      available: false, state: "unavailable", production_auto_resume_available: true,
      wake_state: null, host_delivery: "not_applicable", fresh_turn: "not_applicable",
      ...latency, last_resume_at_unix_ms: null,
    },
  };
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolResult({ goal_plan: unavailable });
  assert.equal(view.nodes["auto-resume"].textContent, "Ready");
  assert.equal(view.nodes["continuity-state"].textContent, "Unavailable");
  assert.equal(view.nodes.activity.textContent, "Window activity observation unavailable.");
  assert.notEqual(view.nodes.status.textContent, "Invalid Goal Plan state");
});

test("Goal continuity refreshes on the same authoritative revision without equating Host acceptance to a fresh turn", async () => {
  const controller = `wc_dagent_AgAgAgAgAgAgAgAg`;
  const readyPlan = {
    ...plan, controller_agent_id: controller,
    continuity: {
      available: true, state: "ready", production_auto_resume_available: true,
      wake_state: null, host_delivery: "not_started", fresh_turn: "not_confirmed",
      ...latency, last_resume_at_unix_ms: null,
    },
  };
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolResult({ goal_plan: readyPlan });
  assert.equal(view.nodes["auto-resume"].textContent, "Ready");
  assert.equal(view.nodes["continuity-state"].textContent, "Ready");

  await view.fireTimers(12000);
  const accepted = {
    ...readyPlan,
    continuity: {
      ...readyPlan.continuity, state: "host_accepted", wake_state: "delivered",
      host_delivery: "accepted", fresh_turn: "not_confirmed",
    },
  };
  await view.reply(view.calls("goal_plan_sync").at(-1), toolResult({ goal_plan: accepted }));
  assert.equal(view.nodes["host-delivery"].textContent, "Accepted");
  assert.equal(view.nodes["fresh-turn"].textContent, "Not confirmed");
  assert.equal(view.nodes.revision.textContent, "1");

  await view.fireTimers(5000);
  const resumed = {
    ...accepted,
    continuity: {
      ...accepted.continuity, state: "resume_confirmed", wake_state: "consumed",
      fresh_turn: "confirmed", last_resume_at_unix_ms: 5000,
    },
  };
  await view.reply(view.calls("goal_plan_sync").at(-1), toolResult({ goal_plan: resumed }));
  assert.equal(view.nodes["continuity-state"].textContent, "Resume confirmed");
  assert.equal(view.nodes["fresh-turn"].textContent, "Confirmed");
  assert.match(view.nodes["last-resume"].textContent, /^Confirmed/);
  assert.equal(view.nodes.revision.textContent, "1");
});

test("terminal Goal stays terminal and stops polling after late active results and visibility changes", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolResult({ goal_plan: plan });
  await view.fireTimers(12000);
  const request = view.calls("goal_plan_sync").at(-1);
  await view.reply(request, toolResult({ goal_plan: terminalPlan() }));
  view.toolResult({ goal_plan: plan });
  await view.fireTimers(12000);
  await view.visibility(false);
  assert.equal(view.nodes.lifecycle.textContent, "Completed");
  assert.equal(view.calls("goal_plan_sync").length, 1);
});

test("Goal rejects a different identity and ignores results after teardown", async () => {
  const view = app("mcp_goal_plan_app.html");
  await view.initialize();
  view.toolResult({ goal_plan: plan });
  view.toolResult({ goal_plan: { ...plan, goal_id: `wc_goal_IiIiIiIiIiIiIiIi`, title: "Foreign" } });
  assert.equal(view.nodes.status.textContent, "Invalid or conflicting Goal identity");
  await view.teardown();
  view.toolResult({ goal_plan: { ...plan, title: "Late", revision: 2 } });
  await flush();
  assert.equal(view.nodes.title.textContent, plan.title);
  assert.equal(view.timers.size, 0);
});
