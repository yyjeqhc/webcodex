import test from "node:test";
import assert from "node:assert/strict";
import { app, flush, toolResult } from "./app_test_support.mjs";

import { project, session_id, input, baseState } from "./work_result_app_fixture.mjs";

const nextState = {
  ...baseState,
  state_version: `wr2_${"b".repeat(64)}`,
  workspace: {
    ...baseState.workspace,
    clean: true,
    files_total: 0,
    files: [],
    additions: 0,
    deletions: 0,
  },
  validation: {
    ...baseState.validation,
    status: "mixed",
    latest_status: "failed",
    current_status: "failed",
    failures: 1,
    unresolved_failures: 1,
  },
  review: {
    ...baseState.review,
    total: 2,
    read_only_inspection_count: 1,
    tools: ["show_changes", "git_review_summary"],
  },
  session: {
    ...baseState.session,
    events_total: 9,
    events_returned: 9,
    updated_at: 1789812010,
    latest_activity: {
      tool: "cargo_test", kind: "tool_call_finished", timestamp: 1789812010,
      status: "completed", duration_ms: 730,
    },
  },
  activity: {
    ...baseState.activity,
    active: true,
    current: { label: "Running checks", kind: "test", started_at_ms: 1789812010000 },
    last: { label: "Edited code", kind: "edit", at_ms: 1789812009000 },
    last_meaningful_activity_at_ms: 1789812009000,
  },
  window_activity: {
    ...baseState.window_activity,
    active: true,
    active_requests: [{ label: "Running checks", kind: "test", started_at_ms: 1789812010000 }],
    events_returned: 3,
    events_observed: 3,
    last_activity_at_ms: 1789812010000,
    events: [
      { label: "Edited code", kind: "edit", status: "success", meaningful: true, started_at_ms: 1789812008000, ended_at_ms: 1789812009000, duration_ms: 1000 },
      ...baseState.window_activity.events,
    ],
  },
};

function contentOnly(result) {
  return { content: [{ type: "text", text: JSON.stringify(result.structuredContent) }] };
}

function privateOnly(result) {
  return { _meta: { "webcodex/workResult": result.structuredContent } };
}

test("initial private Work Result fallback renders when Host omits structuredContent", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  view.notification("ui/notifications/tool-result", privateOnly(toolResult({ work_result: baseState })));
  await flush();
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.badge.textContent, "Waiting");
  assert.equal(view.nodes.refresh.disabled, false);
  assert.equal(view.calls("work_result_state").length, 0);
});
test("Window card renders and refreshes before any Workflow Session exists", async () => {
  const windowOnlyState = {
    version: 2,
    project,
    state_version: `wr2_${"7".repeat(64)}`,
    workspace: baseState.workspace,
    activity: baseState.activity,
    window_activity: baseState.window_activity,
    collaboration: { available: false, can_send: false, messages: [] },
  };
  const view = app("mcp_work_result_app.html");
  view.toolInput({ project });
  view.toolResult({ work_result: windowOnlyState });
  await view.initialize();
  assert.equal(view.nodes.taskTitle.textContent, "WebCodex activity");
  assert.equal(view.nodes.projectIdentity.textContent, "Window activity");
  assert.equal(view.nodes.sessionIdentity.textContent, "");
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.collaborationMeta.textContent, "");
  assert.equal(view.nodes.messageInput.disabled, true);
  assert.equal(view.timers.size, 1);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, { project });
});

test("missing initial machine result recovers once through app-only content fallback", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  view.notification("ui/notifications/tool-result", {
    content: [{ type: "text", text: "WebCodex tool completed successfully." }],
  });
  await flush();
  assert.notEqual(view.nodes.status.textContent, "This task card is unavailable");
  assert.equal(view.calls("work_result_state").length, 1);
  await view.reply(view.calls("work_result_state")[0], contentOnly(toolResult({ work_result: baseState })));
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /Updated|Up to date|Live/);
});

test("primary task card hides raw tool and live file details while keeping semantic activity", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.activityStatus.textContent, "Reviewed changes");
  assert.doesNotMatch(view.nodes.activityStatus.textContent, /show_changes|session event/i);
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.files, undefined);
  assert.equal(view.nodes.collaborationMeta.textContent, "");
});

test("recent inactive work keeps the lightweight last-active state before the idle threshold", async () => {
  const quietState = {
    ...baseState,
    state_version: `wr2_${"c".repeat(64)}`,
    activity: {
      ...baseState.activity,
      last: { label: "Edited code", kind: "edit", at_ms: 1_999_999_990_000 },
      last_meaningful_activity_at_ms: 1_999_999_990_000,
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: quietState });
  await view.initialize();
  assert.equal(view.nodes.activityStatus.textContent, "Edited code");
  assert.equal(view.nodes.activityAge.textContent, "Last active 10s ago");
  assert.equal(view.nodes.badge.textContent, "Waiting");
});

test("inactive work becomes explicitly idle from local wall-clock time while preserving the last semantic activity", async () => {
  const idleState = {
    ...baseState,
    state_version: `wr2_${"f".repeat(64)}`,
    activity: {
      ...baseState.activity,
      last: { label: "Ran checks", kind: "test", at_ms: 1_999_999_820_000 },
      last_meaningful_activity_at_ms: 1_999_999_820_000,
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: idleState });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Idle");
  assert.equal(view.nodes.activityStatus.textContent, "No WebCodex activity");
  assert.equal(view.nodes.activityDetail.textContent, "Last activity: Ran checks");
  assert.equal(view.nodes.activityAge.textContent, "Idle for 3m");
});

test("missing meaningful activity time waits without inventing an idle age", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: {
    ...baseState,
    activity: { ...baseState.activity, last: null, last_meaningful_activity_at_ms: null },
  } });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Waiting");
  assert.equal(view.nodes.activityStatus.textContent, "Waiting for WebCodex activity");
  assert.equal(view.nodes.activityAge.textContent, "");
});

test("internal validation or Session lifecycle does not override Window idle truth", async () => {
  const idleActivity = {
    ...baseState.activity,
    last: { label: "Ran checks", kind: "test", at_ms: 1_999_999_820_000 },
    last_activity_at_ms: 1_999_999_820_000,
    last_meaningful_activity_at_ms: 1_999_999_820_000,
  };
  for (const state of [
    { ...baseState, activity: idleActivity, validation: { ...baseState.validation, current_status: "failed", unresolved_failures: 1, failures: 1 } },
    { ...baseState, activity: idleActivity, session: { ...baseState.session, lifecycle: "closed" } },
  ]) {
    const view = app("mcp_work_result_app.html");
    view.toolResult({ work_result: state });
    await view.initialize();
    assert.equal(view.nodes.badge.textContent, "Idle");
    assert.equal(view.nodes.activityStatus.textContent, "No WebCodex activity");
  }
});

test("unchanged state_version refresh still advances wall-clock idle copy without server mutation", async () => {
  const recent = {
    ...baseState,
    activity: {
      ...baseState.activity,
      last: { label: "Reviewed changes", kind: "review", at_ms: 1_999_999_950_000 },
      last_meaningful_activity_at_ms: 1_999_999_950_000,
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: recent });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Waiting");
  view.advanceTime(20_000);
  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: recent }));
  assert.equal(view.nodes.badge.textContent, "Idle");
  assert.equal(view.nodes.activityStatus.textContent, "No WebCodex activity");
  assert.equal(view.nodes.activityAge.textContent, "Idle for 1m");
});

test("Window messages render sender delivery state but not inbound peer delivery state", async () => {
  const messageState = {
    ...baseState,
    state_version: `wr2_${"d".repeat(64)}`,
    collaboration: {
      available: true,
      can_send: true,
      messages: [
        { message_id: "wc_msg_sent", created_at_ms: 1_999_999_997_000, message: "sent", source: "operator", direction: "inbound", requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
        { message_id: "wc_msg_seen", created_at_ms: 1_999_999_998_000, message: "seen", source: "operator", direction: "inbound", requires_ack: true, first_projected_at_ms: 1_999_999_999_000, first_ack_observed_at_ms: null },
        { message_id: "wc_msg_handled", created_at_ms: 1_999_999_999_000, message: "acknowledged", source: "operator", direction: "inbound", requires_ack: true, first_projected_at_ms: 1_999_999_999_000, first_ack_observed_at_ms: 2_000_000_000_000 },
        { message_id: "wc_msg_window_reply", created_at_ms: 1_999_999_999_500, message: "window reply", source: "window", direction: "outbound", reply_to_message_id: "wc_msg_handled", requires_ack: false, first_projected_at_ms: null, first_ack_observed_at_ms: null },
        { message_id: "wc_msg_peer_in", created_at_ms: 2_000_000_000_000, message: "peer inbound", source: "peer", direction: "inbound", peer_id: `wc_peer_${"2".repeat(32)}`, requires_ack: false, first_projected_at_ms: 2_000_000_000_000, first_ack_observed_at_ms: null },
        { message_id: "wc_msg_peer_out", created_at_ms: 2_000_000_001_000, message: "peer outbound", source: "peer", direction: "outbound", peer_id: `wc_peer_${"3".repeat(32)}`, requires_ack: false, first_projected_at_ms: 2_000_000_001_000, first_ack_observed_at_ms: null },
      ],
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: messageState });
  await view.initialize();
  const rows = view.nodes.messages.children;
  const lastMeta = row => row.children[1].children.at(-1).textContent;
  assert.equal(lastMeta(rows[0]), "Saved");
  assert.equal(lastMeta(rows[1]), "Included in tool result");
  assert.equal(lastMeta(rows[2]), "Acknowledged");
  assert.doesNotMatch(lastMeta(rows[3]), /Saved|Included in tool result|Acknowledged/);
  assert.equal(rows[3].children[1].children[0].textContent, "This Window");
  assert.doesNotMatch(lastMeta(rows[4]), /Saved|Included in tool result|Acknowledged/);
  assert.equal(lastMeta(rows[5]), "Included in tool result");
});

test("card composer retries uncertain delivery with the same payload across context changes", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.nodes.messageInput.value = "Use the existing retry mechanism.";
  view.nodes.messageInput.oninput();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  assert.equal(view.calls("work_result_send_message").length, 1);
  const first = view.calls("work_result_send_message")[0];
  assert.equal(first.params.arguments.project, project);
  assert.equal(first.params.arguments.session_id, session_id);
  assert.equal(first.params.arguments.message, "Use the existing retry mechanism.");
  assert.match(first.params.arguments.delivery_key, /^wrc_[0-9a-f]{32}$/);

  await view.fireTimers(10000);
  assert.match(view.nodes.composerState.textContent, /status unknown/);
  const changedContext = { ...baseState, session_id: `wc_sess_${"2".repeat(32)}`, state_version: `wr2_${"9".repeat(64)}` };
  view.toolResult({ work_result: changedContext });
  await flush();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  assert.equal(view.calls("work_result_send_message").length, 2);
  const second = view.calls("work_result_send_message")[1];
  assert.deepEqual(second.params.arguments, first.params.arguments);

  await view.reply(second, contentOnly(toolResult({
    success: true,
    session_id,
    message_id: "wc_msg_card",
    replayed: true,
    state_changed: false,
  })));
  assert.equal(view.calls("work_result_state").length, 1);
  const refreshed = {
    ...baseState,
    state_version: `wr2_${"e".repeat(64)}`,
    collaboration: {
      available: true,
      can_send: true,
      messages: [
        { message_id: "wc_msg_card", created_at_ms: 2_000_000_000_000, message: "Use the existing retry mechanism.", source: "operator", direction: "inbound", requires_ack: true, first_projected_at_ms: null, first_ack_observed_at_ms: null },
      ],
    },
  };
  await view.reply(view.calls("work_result_state")[0], contentOnly(toolResult({ work_result: refreshed })));
  assert.equal(view.nodes.messageInput.value, "");
  assert.equal(view.nodes.messages.children[0].children[1].children.at(-1).textContent, "Saved");
});

test("card conflict is deterministic and the next explicit send gets a new delivery key", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.nodes.messageInput.value = "Keep conflict draft";
  view.nodes.messageInput.oninput();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const first = view.calls("work_result_send_message")[0];
  await view.reply(first, { structuredContent: { success: false, output: { failure_kind: "conflict", error_kind: "delivery_key_conflict", state_changed: false }, error: "delivery_key_conflict" } });
  assert.equal(view.nodes.messageInput.value, "Keep conflict draft");
  assert.equal(view.nodes.sendMessage.textContent, "Send");
  assert.equal(view.nodes.composerState.textContent, "Message could not be sent");
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const second = view.calls("work_result_send_message")[1];
  assert.notEqual(second.params.arguments.delivery_key, first.params.arguments.delivery_key);
});

test("card invalid context preserves the draft and rebuilds payload on the next explicit send", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.nodes.messageInput.value = "Keep context draft";
  view.nodes.messageInput.oninput();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const first = view.calls("work_result_send_message")[0];
  await view.reply(first, { structuredContent: { success: false, output: { failure_kind: "invalid_context", error_kind: "session_context_unlinked", state_changed: false }, error: "Session context is not explicitly linked to this Window" } });
  assert.equal(view.nodes.messageInput.value, "Keep context draft");
  assert.equal(view.nodes.composerState.textContent, "Context is no longer available");
  const replacement = { ...baseState, session_id: `wc_sess_${"4".repeat(32)}`, state_version: `wr2_${"4".repeat(64)}` };
  view.toolResult({ work_result: replacement });
  await flush();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const second = view.calls("work_result_send_message")[1];
  assert.notEqual(second.params.arguments.delivery_key, first.params.arguments.delivery_key);
  assert.equal(second.params.arguments.session_id, replacement.session_id);
});

test("card sends normally before a Session exists", async () => {
  const windowOnly = { ...baseState, state_version: `wr2_${"5".repeat(64)}` };
  delete windowOnly.session_id;
  delete windowOnly.session;
  const view = app("mcp_work_result_app.html");
  view.toolInput({ project });
  view.toolResult({ work_result: windowOnly });
  await view.initialize();
  view.nodes.messageInput.value = "Window only";
  view.nodes.messageInput.oninput();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const sendCall = view.calls("work_result_send_message")[0];
  assert.equal(sendCall.params.arguments.project, project);
  assert.equal(sendCall.params.arguments.message, "Window only");
  assert.equal(Object.prototype.hasOwnProperty.call(sendCall.params.arguments, "session_id"), false);
});

for (const first of ["input", "result"]) {
  test(`Work Result ${first}-first bootstrap renders the initial snapshot without automatic refresh`, async () => {
    const view = app("mcp_work_result_app.html");
    if (first === "input") view.toolInput(input);
    else view.toolResult({ work_result: baseState });
    assert.equal(view.calls("work_result_state").length, 0);
    await view.initialize();
    if (first === "input") view.toolResult({ work_result: baseState });
    else view.toolInput(input);
    await flush();
    assert.equal(view.calls("work_result_state").length, 0);
    assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
    assert.equal(view.nodes.windowActivity.children.length, 2);
    assert.equal(view.nodes.refresh.disabled, false);
  });
}

test("input-only bootstrap waits for a user Refresh before reading state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /Waiting for Window activity/);
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, { project });
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.equal(view.nodes.status.textContent, "Updated");
});

test("matching Work input/result identity is idempotent and unchanged initial state avoids DOM rebuild", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);
  view.nodes.activityDetail.textContent = "sentinel";
  view.toolResult({ work_result: baseState });
  await flush();
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.nodes.activityDetail.textContent, "sentinel");
});

test("live progress performs bounded app-only polling and adapts to visibility", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.calls("work_result_state").length, 0);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, { project });
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.activityStatus.textContent, "Running checks");
  assert.equal(view.nodes.activityAge.textContent, "Active now");
  assert.equal(view.nodes.badge.textContent, "Working");
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 2);
  await view.reply(view.calls("work_result_state")[1], toolResult({ work_result: nextState }));
  await view.visibility(true);
  assert.equal([...view.timers.values()].some(timer => timer.delay === 12000), true);
  await view.visibility(false);
  assert.equal([...view.timers.values()].some(timer => timer.delay === 250), true);
  assert.equal(view.timers.size, 1);
});

test("closed linked Session does not stop Window-level automatic polling", async () => {
  const closedState = {
    ...baseState,
    state_version: `wr2_${"c".repeat(64)}`,
    session: { ...baseState.session, lifecycle: "closed" },
  };
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: closedState });
  await view.initialize();
  assert.equal(view.timers.size, 1);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, { project });
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: closedState }));
  assert.equal(view.nodes.refresh.disabled, false);
  assert.equal(view.nodes.status.textContent, "Live");
});

test("unchanged active Session pauses automatic polling after bounded idle time and manual Refresh resumes it", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.timers.size, 1);

  view.advanceTime(30 * 60 * 1000);
  await view.visibility(false);
  assert.equal(view.timers.size, 0);
  assert.match(view.nodes.status.textContent, /Updates paused/);
  assert.equal(view.calls("work_result_state").length, 0);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.status.textContent, "Up to date");
  assert.equal([...view.timers.values()].some(timer => timer.delay === 2500), true);
});

test("user Refresh performs one exact state read and updates the snapshot", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, { project });
  assert.equal(view.nodes.refresh.disabled, true);
  assert.equal(view.nodes.refresh.textContent, "Refreshing…");
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.windowCoverage.textContent, "3 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 4);
  assert.equal(view.nodes.status.textContent, "Updated");
  assert.equal(view.nodes.refresh.disabled, false);
});

test("in-flight Refresh clicks coalesce and a later user click performs one new request", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);
  view.nodes.refresh.onclick();
  view.nodes.refresh.onclick();
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.status.textContent, "Up to date");
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 2);
  assert.deepEqual({ ...view.calls("work_result_state")[1].params.arguments }, { project });
});

test("failed Refresh preserves the last valid snapshot and remains retryable", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);

  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("work_result_state")[0], { structuredContent: { success: false, output: { error_kind: "workspace_unavailable" } } });
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.match(view.nodes.status.textContent, /Refresh unavailable/);
  assert.equal(view.nodes.refresh.disabled, false);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 2);
  await view.reject(view.calls("work_result_state")[1]);
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.match(view.nodes.status.textContent, /Refresh unavailable/);
  assert.equal(view.nodes.refresh.disabled, false);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 3);
  await view.reply(view.calls("work_result_state")[2], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.status.textContent, "Up to date");
});

for (const first of ["input", "result"]) {
  test(`conflicting Work ${first}-first identity fails closed without a state request`, async () => {
    const view = app("mcp_work_result_app.html");
    if (first === "input") view.toolInput(input);
    else view.toolResult({ work_result: baseState });
    await view.initialize();
    const foreign = { project: "agent:special:other", session_id: `wc_sess_${"2".repeat(32)}` };
    if (first === "input") view.toolResult({ work_result: { ...baseState, ...foreign } });
    else view.toolInput(foreign);
    await flush();
    assert.equal(view.calls("work_result_state").length, 0);
    assert.equal(view.nodes.status.textContent, "This task card is unavailable");
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.timers.size, 0);
  });
}

test("malformed authoritative Refresh state fails closed", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);
  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: { ...baseState, state_version: "bad" } }));
  assert.equal(view.nodes.status.textContent, "This task card is unavailable");
  assert.equal(view.nodes.refresh.disabled, true);
});

test("collaboration context_project rejects controls and unbounded identities", async () => {
  for (const context_project of ["agent:special:bad\nproject", "p".repeat(513)]) {
    const view = app("mcp_work_result_app.html");
    view.toolResult({ work_result: {
      ...baseState,
      state_version: `wr2_${"8".repeat(64)}`,
      collaboration: {
        available: true,
        can_send: true,
        messages: [{
          message_id: "wc_msg_context",
          source: "operator",
          direction: "inbound",
          message: "context",
          created_at_ms: 1,
          context_session_id: session_id,
          context_project,
          requires_ack: true,
          first_projected_at_ms: null,
          first_ack_observed_at_ms: null,
        }],
      },
    } });
    await view.initialize();
    assert.equal(view.nodes.status.textContent, "This task card is unavailable");
    assert.equal(view.nodes.refresh.disabled, true);
  }
});

test("Window coverage stays explicit when retained activity is truncated", async () => {
  const partialState = {
    ...baseState,
    state_version: `wr2_${"c".repeat(64)}`,
    window_activity: {
      ...baseState.window_activity,
      events_observed: 41,
      events_returned: 2,
      truncated: true,
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: partialState });
  await view.initialize();
  assert.equal(view.nodes.windowCoverage.textContent, "Showing 2 of 41 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.calls("work_result_state").length, 0);
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test(`${method} ignores a late in-flight Refresh response and leaves no timer`, async () => {
    const view = app("mcp_work_result_app.html");
    view.toolResult({ work_result: baseState });
    await view.initialize();
    view.toolInput(input);
    view.nodes.refresh.onclick();
    await flush();
    const request = view.calls("work_result_state")[0];
    await view.teardown(method);
    await view.reply(request, toolResult({ work_result: nextState }));
    await view.visibility(false);
    assert.equal(view.calls("work_result_state").length, 1);
    assert.equal(view.timers.size, 0);
    assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  });
}

test("invalid Project input never refreshes", async () => {
  for (const bad of [{ project: "" }, { project: [project] }]) {
    const view = app("mcp_work_result_app.html");
    view.toolInput(bad);
    await flush();
    assert.equal(view.calls("work_result_state").length, 0);
    assert.equal(view.nodes.status.textContent, "This task card is unavailable");
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.timers.size, 0);
  }
});

// Frozen final changes are a domain of the same Work Result, not a second App.
const snapshot_id = `wc_changes_snapshot_${"2".repeat(32)}`;
function frozenFile(index, overrides = {}) {
  return { path: `src/file_${index}.rs`, kind: "modified", additions: index + 1, deletions: index, binary: false, ...overrides };
}
const finalChanges = {
  snapshot_id, files_changed: 7, additions: 35, deletions: 21,
  files_total: 7, files_returned: 7, files_truncated: false,
  files: [
    frozenFile(0),
    frozenFile(1, { path: "src/new.rs", kind: "added", additions: 8, deletions: 0 }),
    frozenFile(2, { path: "src/removed.rs", kind: "deleted", additions: 0, deletions: 9 }),
    frozenFile(3, { path: "src/new_name.rs", previous_path: "src/old_name.rs", kind: "renamed", additions: 2, deletions: 1 }),
    frozenFile(4, { path: "assets/blob.bin", binary: true, additions: null, deletions: null }),
    frozenFile(5), frozenFile(6),
  ],
};
const frozenWork = (overrides = {}) => ({ ...baseState, final_changes: { ...finalChanges, ...overrides } });
function frozenNodes(view, index = 0) {
  const root = view.nodes.frozenFiles.children[index];
  const button = root.children[0], wrap = root.children[1];
  return { root, button, wrap, state: wrap.children[0], pre: wrap.children[1] };
}
function frozenDiff(overrides = {}) {
  const diff = overrides.diff ?? "diff --git a/src/file_0.rs b/src/file_0.rs\n@@ -1 +1 @@\n-old\n+new\n";
  const bytes = Buffer.byteLength(diff);
  const lines = diff === "" ? 0 : diff.split("\n").length - Number(diff.endsWith("\n"));
  return toolResult({ changes_file_diff: {
    version: 1, project, session_id, snapshot_id, path: "src/file_0.rs", previous_path: null,
    kind: "modified", binary: false, diff, bytes_total: bytes, bytes_returned: bytes,
    lines_total: lines, lines_returned: lines, truncated: false, ...overrides,
  } });
}
async function frozenView(first = "input", state = frozenWork()) {
  const view = app("mcp_work_result_app.html");
  if (first === "input") view.toolInput(input);
  else view.toolResult({ work_result: state });
  await view.initialize();
  if (first === "input") view.toolResult({ work_result: state });
  else view.toolInput(input);
  await flush();
  return view;
}

for (const first of ["input", "result"]) {
  test(`Work Result frozen ${first}-first bootstrap renders bounded metadata without any lazy read`, async () => {
    const view = await frozenView(first);
    assert.equal(view.calls("changes_file_diff").length, 0);
    assert.equal(view.calls("work_result_state").length, 0);
    assert.equal(view.nodes.finalChanges.hidden, false);
    assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
    assert.equal(view.nodes.frozenFiles.children.length, 5);
    assert.equal(view.nodes.frozenMore.textContent, "Show 2 more files");
    assert.match(frozenNodes(view, 3).button.textContent, /src\/old_name.rs → src\/new_name.rs/);
    assert.match(frozenNodes(view, 4).button.textContent, /binary/);
    assert.equal(view.timers.size, 1);
  });
}

test("frozen expansion reads the exact four-part identity once and re-expansion uses the local cache", async () => {
  const view = await frozenView();
  const nodes = frozenNodes(view);
  nodes.button.onclick(); nodes.button.onclick(); nodes.button.onclick();
  await flush();
  assert.equal(view.calls("changes_file_diff").length, 1);
  assert.deepEqual({ ...view.calls("changes_file_diff")[0].params.arguments }, { project, session_id, snapshot_id, path: "src/file_0.rs" });
  await view.reply(view.calls("changes_file_diff")[0], contentOnly(frozenDiff()));
  assert.equal(nodes.state.textContent, "File changes");
  assert.equal(nodes.pre.children.some(line => line.textContent === "+new" && line.className.includes("added")), true);
  assert.equal(nodes.pre.children.some(line => line.textContent === "-old" && line.className.includes("deleted")), true);
  nodes.button.onclick(); nodes.button.onclick();
  await flush();
  assert.equal(view.calls("changes_file_diff").length, 1);
});

test("Show more and live Refresh preserve pending frozen nodes, initial identity, and cached diffs", async () => {
  const view = await frozenView();
  const nodes = frozenNodes(view);
  nodes.button.onclick();
  await flush();
  view.nodes.frozenMore.onclick();
  assert.equal(view.nodes.frozenFiles.children.length, 7);
  assert.equal(view.nodes.frozenMore.hidden, true);
  assert.equal(frozenNodes(view).root, nodes.root);
  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.equal(frozenNodes(view).root, nodes.root);
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff());
  assert.equal(nodes.state.textContent, "File changes");
  nodes.button.onclick(); nodes.button.onclick();
  assert.equal(view.calls("changes_file_diff").length, 1);
  assert.equal(view.calls("changes_file_diff")[0].params.arguments.snapshot_id, snapshot_id);
});

test("initial snapshot is idempotent and its late arrival cannot roll back an explicit live refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); await view.initialize();
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  view.toolResult({ work_result: frozenWork() });
  const root = frozenNodes(view).root;
  view.toolResult({ work_result: frozenWork() });
  assert.equal(frozenNodes(view).root, root);
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
});

test("live progress card adopts the first sealed final snapshot from a later state refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.finalChanges.hidden, true);
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: frozenWork() }));
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.match(view.nodes.status.textContent, /Live · final changes available/);
  const root = frozenNodes(view).root;
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[1], toolResult({ work_result: frozenWork() }));
  assert.equal(frozenNodes(view).root, root);
  assert.equal(view.nodes.refresh.disabled, false);
});

test("metadata and lazy diff truncation remain truthful within bounded initial rows", async () => {
  const view = await frozenView("result", frozenWork({ files_changed: 30, files_total: 30, files_truncated: true }));
  assert.match(view.nodes.frozenFooter.textContent, /showing 7 of 30 files/);
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff({ truncated: true, bytes_total: 50000, lines_total: 2000 }));
  assert.match(frozenNodes(view).state.textContent, /partial \(\d+\/2000 lines\)/);
});

test("renamed and binary files retain their metadata in lazy frozen responses", async () => {
  const view = await frozenView();
  frozenNodes(view, 3).button.onclick(); await flush();
  assert.equal(view.calls("changes_file_diff")[0].params.arguments.path, "src/new_name.rs");
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff({ path: "src/new_name.rs", previous_path: "src/old_name.rs", kind: "renamed" }));
  assert.equal(frozenNodes(view, 3).state.textContent, "File changes");
  frozenNodes(view, 4).button.onclick(); await flush();
  await view.reply(view.calls("changes_file_diff")[1], frozenDiff({ path: "assets/blob.bin", binary: true, diff: "Binary files differ\n" }));
  assert.match(frozenNodes(view, 4).state.textContent, /Binary file/);
});

for (const change of [
  { project: "agent:special:other" }, { session_id: `wc_sess_${"9".repeat(32)}` },
  { snapshot_id: `wc_changes_snapshot_${"9".repeat(32)}` }, { path: "src/not_advertised.rs" },
  { previous_path: "src/other.rs" }, { kind: "added" }, { binary: true },
  { diff: "x".repeat(48 * 1024 + 1) }, { diff: "x\n".repeat(1201) },
  { bytes_returned: 1 }, { lines_returned: 1 }, { bytes_total: 0 },
]) {
  test(`frozen lazy response fails closed for ${Object.keys(change).join(",")}`, async () => {
    const view = await frozenView();
    frozenNodes(view).button.onclick(); await flush();
    await view.reply(view.calls("changes_file_diff")[0], frozenDiff(change));
    assert.match(view.nodes.status.textContent, /task card is unavailable/);
    assert.equal(view.nodes.finalChanges.hidden, true);
    assert.equal(view.nodes.frozenFiles.children.length, 0);
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.timers.size, 0);
  });
}

for (const change of [
  { snapshot_id: "bad" }, { snapshot_id: [snapshot_id] }, { files_total: 6 },
  { files_changed: 8 }, { files_returned: 6 }, { files_total: 8, files_changed: 8 },
  { files: Array.from({ length: 25 }, (_, index) => frozenFile(index)), files_total: 25, files_changed: 25, files_returned: 25 },
  { files: [frozenFile(0), frozenFile(0)], files_total: 2, files_changed: 2, files_returned: 2 },
  ...[frozenFile(0, { path: "../outside" }), frozenFile(0, { kind: "renamed" }), frozenFile(0, { binary: true })]
    .map(file => ({ files: [file], files_total: 1, files_changed: 1, files_returned: 1 })),
  { project: "agent:special:other" },
]) {
  test(`invalid initial frozen metadata is never an identity or file capability: ${JSON.stringify(change).slice(0, 70)}`, async () => {
    const view = await frozenView("input", frozenWork(change));
    assert.equal(view.calls("changes_file_diff").length, 0);
    assert.equal(view.nodes.refresh.disabled, true);
    assert.match(view.nodes.status.textContent, /task card is unavailable/);
  });
}

test("expired/unavailable snapshots never fall back to live diff or automatically retry", async () => {
  const view = await frozenView();
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("changes_file_diff")[0], { structuredContent: { success: false, output: { error_kind: "changes_snapshot_unavailable" } } });
  assert.match(frozenNodes(view).state.textContent, /Changes unavailable/);
  await view.fireTimers(10000); await view.visibility(false);
  assert.equal(view.calls("changes_file_diff").length, 1);
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.timers.size, 1);
});

for (const via of ["initial", "refresh"]) {
  test(`${via} cannot replace an existing frozen snapshot`, async () => {
    const view = await frozenView();
    const replacement = frozenWork({ snapshot_id: `wc_changes_snapshot_${"3".repeat(32)}` });
    if (via === "initial") view.toolResult({ work_result: replacement });
    else {
      view.nodes.refresh.onclick(); await flush();
      await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: replacement }));
    }
    assert.equal(view.nodes.finalChanges.hidden, true);
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.calls("changes_file_diff").length, 0);
  });
}

test("same snapshot id cannot smuggle a changed advertised path list", async () => {
  const view = await frozenView();
  view.toolResult({ work_result: frozenWork({ files: finalChanges.files.map((file, index) => index ? file : { ...file, path: "src/other.rs" }) }) });
  assert.match(view.nodes.status.textContent, /task card is unavailable/);
  assert.equal(view.nodes.finalChanges.hidden, true);
});

test("cached legacy Changes resource payload is not promoted into authoritative Work Result state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); await view.initialize();
  view.toolResult({ changes: { version: 3, project, session_id, ...finalChanges } });
  assert.match(view.nodes.status.textContent, /task card is unavailable/);
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.calls("changes_file_diff").length, 0);
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test(`${method} ignores late frozen diff content and prevents re-expansion reads`, async () => {
    const view = await frozenView();
    const nodes = frozenNodes(view);
    nodes.button.onclick(); await flush();
    const request = view.calls("changes_file_diff")[0];
    await view.teardown(method);
    await view.reply(request, frozenDiff({ diff: "+late\n" }));
    nodes.button.onclick(); nodes.button.onclick();
    assert.equal(nodes.pre.children.length, 0);
    assert.equal(view.calls("changes_file_diff").length, 1);
    assert.equal(view.timers.size, 0);
  });
}


test("Activity and Collaboration tabs preserve Window activity and drafts across refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.projectIdentity.textContent, "Window activity");
  assert.equal(view.nodes.sessionIdentity.textContent, "Context · " + session_id.slice(8, 14));
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.windowActivity.children[1].children[0].children[0].textContent, "Reviewed changes");
  assert.equal(view.nodes.windowActivity.children[0].children[0].children[1].textContent, "Observe · Succeeded");
  view.nodes.messageInput.value = "Keep my draft";
  view.nodes.tabCollaboration.onclick();
  assert.equal(view.nodes.panelCollaboration.hidden, false);
  assert.equal(view.nodes.panelActivity.hidden, true);
  assert.equal(view.nodes.messageInput.value, "Keep my draft");
  assert.equal(view.calls("work_result_send_message").length, 0);
  assert.equal(view.calls("work_result_state").length, 0);
  view.nodes.tabCollaboration.onkeydown({ key: "ArrowLeft", preventDefault() {} });
  assert.equal(view.nodes.tabActivity.getAttribute("aria-selected"), "true");
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.panelActivity.hidden, false);
  assert.equal(view.nodes.windowActivity.children.length, 4);
  assert.equal(view.nodes.messageInput.value, "Keep my draft");
});

for (const workflow of [
  { history_partial: false, activity: Array.from({ length: 25 }, () => ({})) },
  { history_partial: false, activity: [{ label: "Untrusted", stage: "invented", state: "success", started_at: 1, finished_at: 2, duration_ms: 1, count: 1 }] },
]) test("invalid or oversized workflow cannot enter the task card", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: { ...baseState, workflow } });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Unavailable");
  assert.equal(view.calls("work_result_state").length, 0);
});

test("Window card sends without a Session and keeps pending context on uncertain retry", async () => {
  const state = { ...baseState, session_id: undefined, session: undefined };
  const view = app("mcp_work_result_app.html");
  view.toolInput({ project });
  view.toolResult({ work_result: state });
  await view.initialize();
  assert.equal(view.nodes.messageInput.disabled, false);
  view.nodes.messageInput.value = "Before any Session";
  view.nodes.messageInput.oninput();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const first = view.calls("work_result_send_message")[0];
  assert.equal(first.params.arguments.session_id, undefined);
  await view.fireTimers(10000);
  view.toolResult({ work_result: { ...baseState, state_version: `wr2_${"a".repeat(64)}` } });
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const retry = view.calls("work_result_send_message")[1];
  assert.deepEqual(retry.params.arguments, first.params.arguments);
});

test("unrelated activity refresh preserves message nodes and the user's draft", async () => {
  const collaboration = { available: true, can_send: true, messages: [
    { message_id: "wc_msg_reading", created_at_ms: 1_999_999_997_000, message: "Keep reading this reply", source: "window", direction: "outbound", requires_ack: false, first_projected_at_ms: null, first_ack_observed_at_ms: null },
  ] };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: { ...baseState, collaboration } });
  await view.initialize();
  const article = view.nodes.messages.children[0];
  view.nodes.messageInput.value = "My draft";
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: { ...nextState, collaboration: { ...collaboration, can_send: false } } }));
  assert.equal(view.nodes.messages.children[0], article);
  assert.equal(view.nodes.messageInput.value, "My draft");
  assert.equal(view.nodes.messageInput.disabled, true);
});

for (const receipt of [{}, toolResult({}), toolResult({ message_id: 42 })]) {
  test(`missing or invalid message receipt preserves exact retry: ${JSON.stringify(receipt)}`, async () => {
    const view = app("mcp_work_result_app.html");
    view.toolResult({ work_result: baseState });
    await view.initialize();
    view.nodes.messageInput.value = "Keep this exact message";
    view.nodes.messageInput.oninput();
    view.nodes.composer.onsubmit({ preventDefault() {} });
    await flush();
    const first = view.calls("work_result_send_message")[0];
    await view.reply(first, receipt);
    assert.equal(view.nodes.sendMessage.textContent, "Retry");
    assert.equal(view.nodes.messageInput.value, "Keep this exact message");
    assert.equal(view.nodes.messageInput.disabled, true);
    view.nodes.composer.onsubmit({ preventDefault() {} });
    await flush();
    assert.deepEqual(view.calls("work_result_send_message")[1].params.arguments, first.params.arguments);
  });
}

test("card renders each concurrent call and reconciles completion by exact trace", async () => {
  const completed = { ...baseState.window_activity.events[0], tool_name: "read_files", server_trace_id: "trace-completed" };
  const state = { ...baseState, window_activity: { ...baseState.window_activity,
    active: true, events: [completed], events_returned: 1, events_observed: 1,
    active_requests: [
      { label: "Read files", tool_name: "read_files", server_trace_id: "trace-completed", started_at_ms: completed.started_at_ms },
      { label: "Read files", tool_name: "read_files", server_trace_id: "trace-2", started_at_ms: completed.started_at_ms },
      { label: "Read files", tool_name: "read_files", server_trace_id: "trace-3", started_at_ms: completed.started_at_ms },
    ],
  } };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  const rows = view.nodes.windowActivity.children;
  assert.equal(rows.length, 3);
  assert(rows.every(row => row.children[0].children[0].textContent === "read_files"));
  assert.equal(rows.filter(row => row.children[0].children[1].textContent === "Running").length, 2);
});

test("card keeps repeated fast calls as individual rows and exposes the active display bound", async () => {
  const event = baseState.window_activity.events[0];
  const state = { ...baseState, window_activity: { ...baseState.window_activity,
    events: Array.from({ length: 200 }, (_, i) => ({ ...event, tool_name: "observe_jobs", server_trace_id: "trace-" + i })),
    events_returned: 200, events_observed: 230, truncated: true,
    active_requests: Array.from({ length: 8 }, (_, i) => ({ label: "Running", tool_name: "run_shell", server_trace_id: "active-" + i, started_at_ms: event.started_at_ms })),
  } };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  assert.equal(view.nodes.windowActivity.children.length, 208);
  assert.match(view.nodes.windowCoverage.textContent, /Showing 200 of 230/);
  assert.match(view.nodes.windowCoverage.textContent, /up to 8 active/);
});

test("long-running calls keep polling and failed automatic reads identify stale snapshots", async () => {
  const state = { ...baseState, window_activity: { ...baseState.window_activity, active: true,
    active_requests: [{ label: "Running tests", tool_name: "run_shell", started_at_ms: 1_999_999_990_000 }],
  } };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  view.advanceTime(31 * 60 * 1000);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 1);
  await view.reject(view.calls("work_result_state")[0]);
  assert.match(view.nodes.status.textContent, /Refresh unavailable.*last snapshot/);
  assert([...view.timers.values()].some(timer => timer.delay === 2500));
});
