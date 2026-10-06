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
    tools: ["read_workspace_changes", "read_git_review_summary"],
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
    last: { label: "Edited files", kind: "edit", at_ms: 1789812009000 },
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
      { label: "Edited files", kind: "edit", status: "success", meaningful: true, started_at_ms: 1789812008000, ended_at_ms: 1789812009000, duration_ms: 1000 },
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

function threadResult(state, selectedSession = null) {
  const result = toolResult({ work_result: state });
  result._meta = { "webcodex/workResultThread": { session_id: selectedSession } };
  return result;
}

for (const resultFirst of [false, true]) test(`thread initialization preserves the explicit Session on refresh (result-first=${resultFirst})`, async () => {
  const view = app("mcp_work_result_app.html");
  if (!resultFirst) view.toolInput({});
  view.notification("ui/notifications/tool-result", threadResult(baseState, session_id));
  if (resultFirst) view.toolInput({});
  await view.initialize();
  view.nodes.refresh.onclick();
  const call = view.calls("get_work_result_state")[0];
  assert.deepEqual(JSON.parse(JSON.stringify(call.params.arguments)), input);
  await view.reply(call, toolResult({ work_result: nextState }));
  assert.equal(view.nodes.sessionIdentity.textContent, "Session · " + session_id);
});

test("thread initialization never promotes a Window-linked Session into refresh authority", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput({});
  view.notification("ui/notifications/tool-result", threadResult(baseState));
  await view.initialize();
  view.nodes.refresh.onclick();
  assert.deepEqual(JSON.parse(JSON.stringify(view.calls("get_work_result_state")[0].params.arguments)), { project });
});

for (const context of [undefined, {}, { session_id: "invalid" }, { session_id: `wc_sess_${"2".repeat(32)}` }]) {
  test(`thread rejects missing or conflicting initialization context: ${JSON.stringify(context)}`, async () => {
    const view = app("mcp_work_result_app.html");
    view.toolInput({});
    const result = toolResult({ work_result: baseState });
    if (context !== undefined) result._meta = { "webcodex/workResultThread": context };
    view.notification("ui/notifications/tool-result", result);
    await view.initialize();
    assert.equal(view.nodes.badge.textContent, "Unavailable");
    assert.equal(view.calls("get_work_result_state").length, 0);
  });
}

test("a mounted thread cannot be retargeted by a later presentation", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput({});
  view.notification("ui/notifications/tool-result", threadResult(baseState, session_id));
  await view.initialize();
  view.notification("ui/notifications/tool-result", threadResult({ ...baseState, session_id: `wc_sess_${"2".repeat(32)}` }, `wc_sess_${"2".repeat(32)}`));
  assert.equal(view.nodes.badge.textContent, "Unavailable");
});

test("thread review puts checks before long file lists while keeping diagnostics folded", async () => {
  const view = app("mcp_work_result_app.html");
  view.notification("ui/notifications/tool-result", threadResult(baseState, session_id));
  await view.initialize();
  assert.equal(view.nodes.panelResults.hidden, false);
  assert.equal(view.nodes.panelActivity.hidden, true);
  assert.equal(view.nodes.tabResults.textContent, "Review");
  assert.equal(view.nodes.workspaceHeading.textContent, "Changed files");
  assert.deepEqual(view.nodes.viewTabs.children, [view.nodes.tabResults, view.nodes.tabActivity, view.nodes.tabCollaboration]);
  assert.deepEqual(view.nodes.panelResults.children.slice(0, 4), [view.nodes.resultChecks, view.nodes.quotePanel, view.nodes.workspaceChangesSection, view.nodes.finalChanges]);
  assert.equal(view.nodes.quotePanel.hidden, true);
  assert.equal(view.nodes.diagnostics.open, false);
  assert.deepEqual(view.nodes.diagnosticContent.children, [view.nodes.taskContext]);
  view.nodes.tabResults.onkeydown({ key: "ArrowRight", preventDefault() {} });
  assert.equal(view.nodes.panelActivity.hidden, false);
});

test("thread refresh and repeated initialization preserve the user's pane and draft", async () => {
  const view = app("mcp_work_result_app.html");
  const initial = threadResult(baseState, session_id);
  view.notification("ui/notifications/tool-result", initial);
  await view.initialize();
  view.nodes.tabCollaboration.onclick();
  view.nodes.messageInput.value = "Review this change";
  view.nodes.refresh.onclick();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
  view.notification("ui/notifications/tool-result", initial);
  assert.equal(view.nodes.panelCollaboration.hidden, false);
  assert.equal(view.nodes.messageInput.value, "Review this change");
});

test("thread binding also initializes from the private Work Result fallback", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput({});
  const result = privateOnly(toolResult({ work_result: baseState }));
  result._meta["webcodex/workResultThread"] = { session_id };
  view.notification("ui/notifications/tool-result", result);
  await view.initialize();
  view.nodes.refresh.onclick();
  assert.equal(view.nodes.panelResults.hidden, false);
  assert.equal(view.calls("get_work_result_state")[0].params.arguments.session_id, session_id);
});

const outputState = {
  ...baseState,
  workspace: { ...baseState.workspace, git_available: false },
  task_outputs: {
    items: [{ path: "results/report.csv", status: "verified", file_bytes: 42,
      mime_type: "text/csv", sha256: "d".repeat(64) },
    { path: "results/missing.pdf", status: "missing" },
    { path: "results/unavailable.png", status: "unavailable" }],
    verified_count: 1, missing_count: 1, unavailable_count: 1, observed_at: 1789812000,
  },
};

test("non-Git task outputs show independently observed files and partial failures", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: outputState });
  await view.initialize();
  assert.equal(view.nodes.taskOutputsSection.hidden, false);
  assert.equal(view.nodes.workspaceChangesSection.hidden, true);
  assert.equal(view.nodes.taskOutputsList.children.length, 3);
  assert.match(view.nodes.taskOutputsSummary.textContent, /1 observed.*1 missing.*1 unavailable/);
  assert.match(view.nodes.taskOutputsObserved.textContent, /files may change after this observation/);
  const rows = view.nodes.taskOutputsList.children;
  assert.equal(rows[0].children[0].textContent, "results/report.csv");
  assert.equal(rows[0].children[2].children[1].textContent, "SHA-256 · " + "d".repeat(64));
  assert.equal(rows[1].children.length, 2);
  assert.equal(rows[2].children.length, 2);
  assert.equal(view.sent.filter(r => r.method === "ui/message").length, 0);
});

test("explicit output export request preserves Project path and observed SHA", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: outputState });
  await view.initialize();
  const button = view.nodes.taskOutputsList.children[0].children[3];
  const pending = button.onclick();
  const request = view.sent.find(r => r.method === "ui/message");
  assert.equal(button.disabled, true);
  assert.equal(request.params.role, "user");
  const text = request.params.content[0].text;
  assert.ok(text.includes(project));
  assert.ok(text.includes('"results/report.csv"'));
  assert.ok(text.includes("d".repeat(64)));
  assert.match(text, /First check.*if it changed/);
  assert.equal(view.calls("inspect_project_artifact").length, 0);
  await view.reply(request, {});
  await pending;
  assert.equal(button.disabled, false);
  assert.equal(view.nodes.taskOutputsAction.textContent, "File requested in chat.");
});

test("malformed or unlinked output evidence cannot populate the task card", async () => {
  for (const mutate of [
    s => { delete s.session_id; },
    s => { s.task_outputs.verified_count = 2; },
    s => { s.task_outputs.items[0].sha256 = "invalid"; },
    s => { s.task_outputs.items[0].path = "../report.csv"; },
    s => { s.task_outputs.items[0].path = "汉".repeat(171); },
    s => { s.task_outputs.items[1].sha256 = "d".repeat(64); },
  ]) {
    const state = structuredClone(outputState);
    mutate(state);
    const view = app("mcp_work_result_app.html");
    view.toolInput(input);
    view.toolResult({ work_result: state });
    await view.initialize();
    assert.equal(view.nodes.taskOutputsList?.children.length || 0, 0);
    assert.equal(view.sent.filter(r => r.method === "ui/message").length, 0);
  }
});

test("output request failure remains retryable and teardown ignores a late reply", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: outputState });
  await view.initialize();
  const button = view.nodes.taskOutputsList.children[0].children[3];
  const failed = button.onclick();
  await view.reject(view.sent.find(r => r.method === "ui/message"));
  await failed;
  assert.equal(button.disabled, false);
  assert.match(view.nodes.taskOutputsAction.textContent, /Try again/);
  const pending = button.onclick();
  await button.onclick();
  const requests = view.sent.filter(r => r.method === "ui/message");
  assert.equal(requests.length, 2);
  const previous = view.nodes.taskOutputsAction.textContent;
  await view.teardown();
  await view.reply(requests[1], {});
  await pending;
  assert.equal(view.nodes.taskOutputsAction.textContent, previous);
  assert.equal(button.disabled, true);
});

test("initial private Work Result fallback renders when Host omits structuredContent", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  view.notification("ui/notifications/tool-result", privateOnly(toolResult({ work_result: baseState })));
  await flush();
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.badge.textContent, "Waiting");
  assert.equal(view.nodes.refresh.disabled, false);
  assert.equal(view.calls("get_work_result_state").length, 0);
});
test("Window card renders and refreshes before any Workflow Session exists", async () => {
  const windowOnlyState = {
    version: 2,
    project,
    state_version: `wr2_${"7".repeat(64)}`,
    window: baseState.window,
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
  assert.equal(view.nodes.projectIdentity.textContent, "Project · " + project);
  assert.equal(view.nodes.windowIdentity.textContent, "Window · " + "f".repeat(64));
  assert.equal(view.nodes.sessionIdentity.textContent, "Session · not linked yet");
  assert.equal(view.nodes.windowSource.textContent, "Source · mcp");
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.collaborationMeta.textContent, "");
  assert.equal(view.nodes.messageInput.disabled, true);
  assert.equal(view.timers.size, 1);
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, automatic: true });
});

test("missing initial machine result recovers once through app-only content fallback", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  view.notification("ui/notifications/tool-result", {
    content: [{ type: "text", text: "WebCodex tool completed successfully." }],
  });
  await flush();
  assert.notEqual(view.nodes.status.textContent, "This Work Result card is unavailable");
  assert.equal(view.calls("get_work_result_state").length, 1);
  await view.reply(view.calls("get_work_result_state")[0], contentOnly(toolResult({ work_result: baseState })));
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /Updated|Up to date|Live/);
});

test("Activity stays focused while Results exposes live files", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.taskTitle.textContent, "Work Result test");
  assert.equal(view.nodes.activityStatus.textContent, "Reviewed changes");
  assert.doesNotMatch(view.nodes.activityStatus.textContent, /read_workspace_changes|session event/i);
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.panelResults.hidden, true);
  assert.match(view.nodes.workspaceFiles.children[0].children[0].textContent, /src\/a.rs/);
  assert.equal(view.nodes.collaborationMeta.textContent, "0 loaded");
});

test("recent inactive work keeps the lightweight last-active state before the idle threshold", async () => {
  const quietState = {
    ...baseState,
    state_version: `wr2_${"c".repeat(64)}`,
    activity: {
      ...baseState.activity,
      last: { label: "Edited files", kind: "edit", at_ms: 1_999_999_990_000 },
      last_meaningful_activity_at_ms: 1_999_999_990_000,
    },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: quietState });
  await view.initialize();
  assert.equal(view.nodes.activityStatus.textContent, "Edited files");
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
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: recent }));
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
  const metadata = row => row.children.at(-1).children.map(child => child.textContent).join(' ');
  assert.match(metadata(rows[0]), /Saved/);
  assert.match(metadata(rows[1]), /Included in tool result/);
  assert.match(metadata(rows[2]), /Acknowledged/);
  assert.match(metadata(rows[2]), /Reply received/);
  assert.doesNotMatch(metadata(rows[3]), /Saved|Included in tool result|Acknowledged/);
  assert.equal(rows[3].children[0].children[0].textContent, "This Window → You");
  assert.match(rows[3].children[1].textContent, /Reply to · acknowledged/);
  assert.doesNotMatch(metadata(rows[4]), /Saved|Included in tool result|Acknowledged/);
  assert.match(metadata(rows[5]), /Included in tool result/);
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
  assert.equal(view.calls("send_work_result_message").length, 1);
  const first = view.calls("send_work_result_message")[0];
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
  assert.equal(view.calls("send_work_result_message").length, 2);
  const second = view.calls("send_work_result_message")[1];
  assert.deepEqual(second.params.arguments, first.params.arguments);

  await view.reply(second, contentOnly(toolResult({
    success: true,
    session_id,
    message_id: "wc_msg_card",
    replayed: true,
    state_changed: false,
  })));
  const historyRead = view.calls("get_work_result_state").find(call => call.params.arguments.collaboration);
  assert.ok(historyRead, "receipt must refresh history even while an earlier activity read is pending");
  assert.equal(view.nodes.sendMessage.textContent, "Send", "composer does not wait for the history read");
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
  await view.reply(historyRead, contentOnly(toolResult({ work_result_collaboration: refreshed.collaboration })));
  assert.equal(view.nodes.messageInput.value, "");
  assert.equal(view.nodes.messages.children[0].children.at(-1).children.at(-1).textContent, "Saved");
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
  const first = view.calls("send_work_result_message")[0];
  await view.reply(first, { structuredContent: { success: false, output: { failure_kind: "conflict", error_kind: "delivery_key_conflict", state_changed: false }, error: "delivery_key_conflict" } });
  assert.equal(view.nodes.messageInput.value, "Keep conflict draft");
  assert.equal(view.nodes.sendMessage.textContent, "Send");
  assert.equal(view.nodes.composerState.textContent, "Message could not be sent");
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const second = view.calls("send_work_result_message")[1];
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
  const first = view.calls("send_work_result_message")[0];
  await view.reply(first, { structuredContent: { success: false, output: { failure_kind: "invalid_context", error_kind: "session_context_unlinked", state_changed: false }, error: "Session context is not explicitly linked to this Window" } });
  assert.equal(view.nodes.messageInput.value, "Keep context draft");
  assert.equal(view.nodes.composerState.textContent, "Context is no longer available");
  const replacement = { ...baseState, session_id: `wc_sess_${"4".repeat(32)}`, state_version: `wr2_${"4".repeat(64)}` };
  view.toolResult({ work_result: replacement });
  await flush();
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const second = view.calls("send_work_result_message")[1];
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
  const sendCall = view.calls("send_work_result_message")[0];
  assert.equal(sendCall.params.arguments.project, project);
  assert.equal(sendCall.params.arguments.message, "Window only");
  assert.equal(Object.prototype.hasOwnProperty.call(sendCall.params.arguments, "session_id"), false);
});

for (const first of ["input", "result"]) {
  test(`Work Result ${first}-first bootstrap renders the initial snapshot without automatic refresh`, async () => {
    const view = app("mcp_work_result_app.html");
    if (first === "input") view.toolInput(input);
    else view.toolResult({ work_result: baseState });
    assert.equal(view.calls("get_work_result_state").length, 0);
    await view.initialize();
    if (first === "input") view.toolResult({ work_result: baseState });
    else view.toolInput(input);
    await flush();
    assert.equal(view.calls("get_work_result_state").length, 0);
    assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
    assert.equal(view.nodes.windowActivity.children.length, 2);
    assert.equal(view.nodes.refresh.disabled, false);
  });
}

test("input-only bootstrap waits for a user Refresh before reading state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  assert.equal(view.calls("get_work_result_state").length, 0);
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /Waiting for Window activity/);
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, session_id });
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: baseState }));
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
  assert.equal(view.calls("get_work_result_state").length, 0);
  assert.equal(view.nodes.activityDetail.textContent, "sentinel");
});

test("automatic workspace reuse is explicit and manual Refresh always requests a new observation", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); view.toolResult({ work_result: baseState }); await view.initialize();
  await view.fireTimers(10000);
  const automatic = view.calls("get_work_result_state").at(-1);
  assert.equal(automatic.params.arguments.automatic, true);
  await view.reply(automatic, toolResult({work_result: {...baseState,
    workspace_observation: {reused: true, max_reuse_ms: 30000, semantics: "bounded_snapshot_not_filesystem_freshness"}}}));
  assert.match(view.nodes.status.title, /at most 30 seconds/);
  view.nodes.refresh.onclick(); await flush();
  const manual = view.calls("get_work_result_state").at(-1);
  assert.equal(manual.params.arguments.automatic, undefined);
  await view.reply(manual, toolResult({work_result: {...baseState, workspace_observation: {reused: false}}}));
  assert.equal(view.nodes.status.title, "");
});

test("live progress performs bounded app-only polling and adapts to visibility", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.calls("get_work_result_state").length, 0);
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, session_id, automatic: true });
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.activityStatus.textContent, "Running checks");
  assert.equal(view.nodes.activityAge.textContent, "Active now");
  assert.equal(view.nodes.badge.textContent, "Working");
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 2);
  await view.reply(view.calls("get_work_result_state")[1], toolResult({ work_result: nextState }));
  await view.visibility(true);
  assert.equal(view.timers.size, 0);
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 2);
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
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, session_id, automatic: true });
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: closedState }));
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
  assert.equal(view.calls("get_work_result_state").length, 0);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 1);
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.status.textContent, "Up to date");
  assert.equal([...view.timers.values()].some(timer => timer.delay === 10000), true);
});

test("user Refresh performs one exact state read and updates the snapshot", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, session_id });
  assert.equal(view.nodes.refresh.disabled, true);
  assert.equal(view.nodes.refresh.textContent, "Refreshing…");
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
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
  assert.equal(view.calls("get_work_result_state").length, 1);
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.status.textContent, "Up to date");
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 2);
  assert.deepEqual({ ...view.calls("get_work_result_state")[1].params.arguments }, { project, session_id });
});

test("failed Refresh preserves the last valid snapshot and remains retryable", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);

  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("get_work_result_state")[0], { structuredContent: { success: false, output: { error_kind: "workspace_unavailable" } } });
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.match(view.nodes.status.textContent, /Refresh unavailable/);
  assert.equal(view.nodes.refresh.disabled, false);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 2);
  await view.reject(view.calls("get_work_result_state")[1]);
  assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  assert.match(view.nodes.status.textContent, /Refresh unavailable/);
  assert.equal(view.nodes.refresh.disabled, false);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("get_work_result_state").length, 3);
  await view.reply(view.calls("get_work_result_state")[2], toolResult({ work_result: baseState }));
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
    assert.equal(view.calls("get_work_result_state").length, 0);
    assert.equal(view.nodes.status.textContent, "This Work Result card is unavailable");
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
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: { ...baseState, state_version: "bad" } }));
  assert.equal(view.nodes.status.textContent, "This Work Result card is unavailable");
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
    assert.equal(view.nodes.status.textContent, "This Work Result card is unavailable");
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
  assert.equal(view.calls("get_work_result_state").length, 0);
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test(`${method} ignores a late in-flight Refresh response and leaves no timer`, async () => {
    const view = app("mcp_work_result_app.html");
    view.toolResult({ work_result: baseState });
    await view.initialize();
    view.toolInput(input);
    view.nodes.refresh.onclick();
    await flush();
    const request = view.calls("get_work_result_state")[0];
    await view.teardown(method);
    await view.reply(request, toolResult({ work_result: nextState }));
    await view.visibility(false);
    assert.equal(view.calls("get_work_result_state").length, 1);
    assert.equal(view.timers.size, 0);
    assert.equal(view.nodes.windowCoverage.textContent, "2 observed events");
  });
}

test("invalid Project input never refreshes", async () => {
  for (const bad of [{ project: "" }, { project: [project] }]) {
    const view = app("mcp_work_result_app.html");
    view.toolInput(bad);
    await flush();
    assert.equal(view.calls("get_work_result_state").length, 0);
    assert.equal(view.nodes.status.textContent, "This Work Result card is unavailable");
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.timers.size, 0);
  }
});

test("idle unchanged polling backs off without losing hidden-tab teardown", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  let elapsed = 0;
  while (true) {
    const timers = [...view.timers.values()];
    assert.equal(timers.length, 1);
    const delay = timers[0].delay;
    if (elapsed + delay > 600000) break;
    elapsed += delay; view.advanceTime(delay); await view.fireTimers(delay);
    const call = view.calls("get_work_result_state").at(-1);
    await view.reply(call, toolResult({ work_result: baseState }));
  }
  assert(view.calls("get_work_result_state").length <= 8, "steady idle card should not poll every 10/30 seconds forever");
  assert([...view.timers.values()].some(timer => timer.delay === 120000));
  await view.visibility(true);
  assert.equal(view.timers.size, 0);
  await view.teardown();
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
  return { root, button, wrap, state: wrap.children[0], get pre() { return wrap.children[1]; } };
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
    assert.equal(view.calls("read_changed_file_diff").length, 0);
    assert.equal(view.calls("get_work_result_state").length, 0);
    assert.equal(view.nodes.finalChanges.hidden, false);
    assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
    assert.equal(view.nodes.frozenFiles.children.length, 5);
    assert.equal(view.nodes.frozenMore.textContent, "Show more files");
    assert.equal(frozenNodes(view).pre, undefined, "no diff DOM before expansion");
    assert.match(frozenNodes(view, 3).button.textContent, /src\/old_name.rs → src\/new_name.rs/);
    assert.match(frozenNodes(view, 4).button.textContent, /binary/);
    assert.equal(view.timers.size, 1);
  });
}

test("both file lists fold independently and show-less preserves cached final nodes", async () => {
  const view = await frozenView();
  const row = frozenNodes(view);
  view.nodes.frozenMore.onclick(); await flush();
  assert.equal(view.nodes.frozenFiles.children.length, 7);
  view.nodes.frozenLess.onclick();
  assert.equal(view.nodes.frozenFiles.children.filter(node => !node.hidden).length, 5);
  assert.equal(view.nodes.frozenMore.hidden, false);
  assert.equal(frozenNodes(view).root, row.root);
  row.button.onclick(); await flush();
  view.nodes.frozenCollapse.onclick();
  assert.equal(row.wrap.hidden, true);
  assert.equal(row.button.getAttribute("aria-expanded"), "false");
  await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff());
  row.button.onclick(); await flush();
  assert.equal(view.calls("read_changed_file_diff").length, 1);
});

test("final file metadata loads subsequent pages only after explicit more", async () => {
  const files = Array.from({ length: 24 }, (_, index) => frozenFile(index));
  const view = await frozenView("input", frozenWork({ files, files_total: 30, files_changed: 30, files_returned: 24, files_truncated: true }));
  for (let i = 0; i < 4; i++) { view.nodes.frozenMore.onclick(); await flush(); }
  assert.equal(view.calls("get_work_result_state").length, 0);
  view.nodes.frozenMore.onclick(); await flush();
  const request = view.calls("get_work_result_state")[0];
  assert.equal(request.params.arguments.files.offset, 24);
  assert.equal(request.params.arguments.files.snapshot_id, snapshot_id);
  assert.equal(request.params.arguments.session_id, session_id);
  await view.reply(request, toolResult({ work_result_files: {
    project, session_id, snapshot_id, offset: 24, next_offset: null, files_total: 30, source_truncated: false,
    files: Array.from({ length: 6 }, (_, i) => frozenFile(i + 24)),
  } }));
  view.nodes.frozenMore.onclick(); await flush();
  assert.equal(view.nodes.frozenFiles.children.length, 30);
  assert.equal(view.nodes.frozenMore.hidden, true);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
});

test("live rows share one snapshot acquisition, load one diff, and keep collapse cached", async () => {
  const state = structuredClone(baseState);
  state.workspace.files = Array.from({ length: 8 }, (_, i) => ({ path: `src/file_${i}.rs`, status: "modified" }));
  state.workspace.files_total = 30; state.workspace.truncated = true;
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  const first = view.nodes.workspaceFiles.children[0];
  assert.equal(first.children[1].children.length, 1);
  assert.equal(view.calls("get_work_result_state").length, 0);
  first.children[0].onclick(); first.children[0].onclick(); first.children[0].onclick(); await flush();
  assert.equal(view.calls("get_work_result_state").length, 1);
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: 24, files_total: 30, source_truncated: false,
    files: Array.from({ length: 24 }, (_, i) => frozenFile(i)),
  } }));
  const diff = view.calls("get_work_result_state")[1];
  assert.equal(diff.params.arguments.files.path, "src/file_0.rs");
  assert.equal(diff.params.arguments.files.snapshot_id, snapshot_id);
  await view.reply(diff, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, path: "src/file_0.rs", diff: "+literal <text>\n", truncated: false,
  } }));
  first.children[0].onclick(); first.children[0].onclick(); await flush();
  assert.equal(view.calls("get_work_result_state").length, 2);
  assert.equal(first.children[1].children[1].children[0].textContent, "+literal <text>");
  view.nodes.workspaceMore.onclick(); await flush();
  assert.equal(view.nodes.workspaceFiles.children.length, 10);
  view.nodes.workspaceLess.onclick();
  assert.equal(view.nodes.workspaceFiles.children.filter(row => !row.hidden).length, 5);
  assert.equal(view.nodes.workspaceFiles.children.length, 10, "hidden rows retain their reading state");
  assert.equal(view.nodes.workspaceFiles.children[0], first);
  view.nodes.workspaceCollapse.onclick();
  assert.equal(first.children[1].hidden, true);
  assert.equal(view.calls("get_work_result_state").length, 2);
});

test("refreshing file snapshot ignores an older page and leaves the new more button usable", async () => {
  const state = structuredClone(baseState);
  state.workspace.files_total = 30; state.workspace.truncated = true;
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  view.nodes.workspaceMore.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: 24, files_total: 30, source_truncated: false,
    files: Array.from({ length: 24 }, (_, i) => frozenFile(i)),
  } }));
  for (let i = 0; i < 4; i++) { view.nodes.workspaceMore.onclick(); await flush(); }
  const oldPage = view.calls("get_work_result_state")[1];
  assert.equal(oldPage.params.arguments.files.offset, 24);
  assert.equal(view.nodes.workspaceMore.disabled, true);
  view.nodes.workspaceReload.onclick();
  const row = view.nodes.workspaceFiles.children[0];
  assert.equal(view.nodes.workspaceMore.disabled, false);
  await view.reply(oldPage, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 24, next_offset: null, files_total: 30, source_truncated: false,
    files: Array.from({ length: 6 }, (_, i) => frozenFile(i + 24)),
  } }));
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  assert.equal(view.nodes.workspaceMore.disabled, false);
  assert.equal(view.calls("get_work_result_state").length, 2);
  assert.doesNotMatch(view.nodes.workspaceMeta.textContent, /unavailable/);
});

test("a file page cannot replace its Project and sealed changes cannot retarget their Session", async () => {
  const view = await frozenView();
  view.nodes.workspaceMore.hidden = false;
  view.nodes.workspaceMore.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project: "agent:special:foreign", session_id: null, snapshot_id, offset: 0, next_offset: null,
    files_total: 1, source_truncated: false, files: [frozenFile(0)],
  } }));
  assert.match(view.nodes.workspaceMeta.textContent, /unavailable/);
  assert.equal(view.calls("get_work_result_state").length, 1);
  view.toolResult({ work_result: { ...frozenWork(), session_id: `wc_sess_${"3".repeat(32)}` } });
  await flush();
  assert.equal(view.nodes.status.textContent, "This Work Result card is unavailable");
  assert.equal(view.nodes.refresh.disabled, true);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
});

test("frozen expansion reads the exact four-part identity once and re-expansion uses the local cache", async () => {
  const view = await frozenView();
  const nodes = frozenNodes(view);
  nodes.button.onclick(); nodes.button.onclick(); nodes.button.onclick();
  await flush();
  assert.equal(view.calls("read_changed_file_diff").length, 1);
  assert.deepEqual({ ...view.calls("read_changed_file_diff")[0].params.arguments }, { project, session_id, snapshot_id, path: "src/file_0.rs" });
  await view.reply(view.calls("read_changed_file_diff")[0], contentOnly(frozenDiff()));
  assert.equal(nodes.state.textContent, "File changes");
  assert.equal(nodes.pre.children.some(line => line.textContent === "+new" && line.className.includes("added")), true);
  assert.equal(nodes.pre.children.some(line => line.textContent === "-old" && line.className.includes("deleted")), true);
  nodes.button.onclick(); nodes.button.onclick();
  await flush();
  assert.equal(view.calls("read_changed_file_diff").length, 1);
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
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.equal(frozenNodes(view).root, nodes.root);
  await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff());
  assert.equal(nodes.state.textContent, "File changes");
  nodes.button.onclick(); nodes.button.onclick();
  assert.equal(view.calls("read_changed_file_diff").length, 1);
  assert.equal(view.calls("read_changed_file_diff")[0].params.arguments.snapshot_id, snapshot_id);
});

test("initial snapshot is idempotent and its late arrival cannot roll back an explicit live refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); await view.initialize();
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
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
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: frozenWork() }));
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.match(view.nodes.status.textContent, /Live · final changes available/);
  const root = frozenNodes(view).root;
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[1], toolResult({ work_result: frozenWork() }));
  assert.equal(frozenNodes(view).root, root);
  assert.equal(view.nodes.refresh.disabled, false);
});

test("metadata and lazy diff truncation remain truthful within bounded initial rows", async () => {
  const view = await frozenView("result", frozenWork({ files_changed: 30, files_total: 30, files_truncated: true }));
  assert.match(view.nodes.frozenFooter.textContent, /showing 5 of 30 files/);
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff({ truncated: true, bytes_total: 50000, lines_total: 2000 }));
  assert.match(frozenNodes(view).state.textContent, /partial \(\d+\/2000 lines\)/);
});

test("renamed and binary files retain their metadata in lazy frozen responses", async () => {
  const view = await frozenView();
  frozenNodes(view, 3).button.onclick(); await flush();
  assert.equal(view.calls("read_changed_file_diff")[0].params.arguments.path, "src/new_name.rs");
  await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff({ path: "src/new_name.rs", previous_path: "src/old_name.rs", kind: "renamed" }));
  assert.equal(frozenNodes(view, 3).state.textContent, "File changes");
  frozenNodes(view, 4).button.onclick(); await flush();
  await view.reply(view.calls("read_changed_file_diff")[1], frozenDiff({ path: "assets/blob.bin", binary: true, diff: "Binary files differ\n" }));
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
    await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff(change));
    assert.match(view.nodes.status.textContent, /Work Result card is unavailable/);
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
    assert.equal(view.calls("read_changed_file_diff").length, 0);
    assert.equal(view.nodes.refresh.disabled, true);
    assert.match(view.nodes.status.textContent, /Work Result card is unavailable/);
  });
}

test("expired/unavailable snapshots never fall back to live diff or automatically retry", async () => {
  const view = await frozenView();
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("read_changed_file_diff")[0], { structuredContent: { success: false, output: { error_kind: "changes_snapshot_unavailable" } } });
  assert.match(frozenNodes(view).state.textContent, /Changes unavailable/);
  await view.fireTimers(10000); await view.visibility(false);
  assert.equal(view.calls("read_changed_file_diff").length, 1);
  assert.equal(view.calls("get_work_result_state").length, 0);
  assert.equal(view.timers.size, 1);
});

for (const via of ["initial", "refresh"]) {
  test(`${via} cannot replace an existing frozen snapshot`, async () => {
    const view = await frozenView();
    const replacement = frozenWork({ snapshot_id: `wc_changes_snapshot_${"3".repeat(32)}` });
    if (via === "initial") view.toolResult({ work_result: replacement });
    else {
      view.nodes.refresh.onclick(); await flush();
      await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: replacement }));
    }
    assert.equal(view.nodes.finalChanges.hidden, true);
    assert.equal(view.nodes.refresh.disabled, true);
    assert.equal(view.calls("read_changed_file_diff").length, 0);
  });
}

test("same snapshot id cannot smuggle a changed advertised path list", async () => {
  const view = await frozenView();
  view.toolResult({ work_result: frozenWork({ files: finalChanges.files.map((file, index) => index ? file : { ...file, path: "src/other.rs" }) }) });
  assert.match(view.nodes.status.textContent, /Work Result card is unavailable/);
  assert.equal(view.nodes.finalChanges.hidden, true);
});

test("cached legacy Changes resource payload is not promoted into authoritative Work Result state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); await view.initialize();
  view.toolResult({ changes: { version: 3, project, session_id, ...finalChanges } });
  assert.match(view.nodes.status.textContent, /Work Result card is unavailable/);
  assert.equal(view.calls("get_work_result_state").length, 0);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
});

for (const method of ["ui/resource-teardown", "pagehide", "beforeunload"]) {
  test(`${method} ignores late frozen diff content and prevents re-expansion reads`, async () => {
    const view = await frozenView();
    const nodes = frozenNodes(view);
    nodes.button.onclick(); await flush();
    const request = view.calls("read_changed_file_diff")[0];
    await view.teardown(method);
    await view.reply(request, frozenDiff({ diff: "+late\n" }));
    nodes.button.onclick(); nodes.button.onclick();
    assert.equal(nodes.pre.children.length, 0);
    assert.equal(view.calls("read_changed_file_diff").length, 1);
    assert.equal(view.timers.size, 0);
  });
}


test("Activity and Collaboration tabs preserve Window activity and drafts across refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.projectIdentity.textContent, "Project · " + project);
  assert.equal(view.nodes.windowIdentity.textContent, "Window · " + "f".repeat(64));
  assert.equal(view.nodes.sessionIdentity.textContent, "Session · " + session_id);
  assert.equal(view.nodes.windowSource.textContent, "Source · mcp");
  assert.equal(view.nodes.windowActivity.children.length, 2);
  assert.equal(view.nodes.windowActivity.children[0].children[0].children[0].children[0].textContent, "get_runtime_status");
  assert.equal(view.nodes.windowActivity.children[0].children[0].children[1].textContent, "Observe · Succeeded");
  assert.equal(view.nodes.windowActivity.children[1].children[0].children[0].children[0].textContent, "read_workspace_changes");
  assert.equal(view.nodes.windowActivity.children.every(row => row.tagName === "DETAILS" && row.open === false), true);
  assert.equal(view.calls("read_work_result_activity_detail").length, 0);
  view.nodes.messageInput.value = "Keep my draft";
  view.nodes.tabCollaboration.onclick();
  assert.equal(view.nodes.panelCollaboration.hidden, false);
  assert.equal(view.nodes.panelActivity.hidden, true);
  assert.equal(view.nodes.messageInput.value, "Keep my draft");
  assert.equal(view.calls("send_work_result_message").length, 0);
  assert.equal(view.calls("get_work_result_state").length, 0);
  view.nodes.tabCollaboration.onkeydown({ key: "ArrowLeft", preventDefault() {} });
  assert.equal(view.nodes.tabResults.getAttribute("aria-selected"), "true");
  view.nodes.tabResults.onkeydown({ key: "ArrowLeft", preventDefault() {} });
  assert.equal(view.nodes.tabActivity.getAttribute("aria-selected"), "true");
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: nextState }));
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
  assert.equal(view.calls("get_work_result_state").length, 0);
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
  const first = view.calls("send_work_result_message")[0];
  assert.equal(first.params.arguments.session_id, undefined);
  await view.fireTimers(10000);
  view.toolResult({ work_result: { ...baseState, state_version: `wr2_${"a".repeat(64)}` } });
  view.nodes.composer.onsubmit({ preventDefault() {} });
  await flush();
  const retry = view.calls("send_work_result_message")[1];
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
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: { ...nextState, collaboration: { ...collaboration, can_send: false } } }));
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
    const first = view.calls("send_work_result_message")[0];
    await view.reply(first, receipt);
    assert.equal(view.nodes.sendMessage.textContent, "Retry");
    assert.equal(view.nodes.messageInput.value, "Keep this exact message");
    assert.equal(view.nodes.messageInput.disabled, true);
    view.nodes.composer.onsubmit({ preventDefault() {} });
    await flush();
    assert.deepEqual(view.calls("send_work_result_message")[1].params.arguments, first.params.arguments);
  });
}

test("Window activity renders oldest-first and keeps the latest call last", async () => {
  const state = { ...baseState, window_activity: { ...baseState.window_activity,
    active: true,
    active_requests: [{ label: "Running checks", tool_name: "run_shell", server_trace_id: "trace-running", started_at_ms: 1_999_999_995_000 }],
  } };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state });
  await view.initialize();
  const titles = view.nodes.windowActivity.children.map(row => row.children[0].children[0].children[0].textContent);
  assert.deepEqual(titles, ["get_runtime_status", "read_workspace_changes", "run_shell"]);
  assert.equal(view.nodes.windowActivity.children.at(-1).children[0].children[1].textContent, "Running");
});

test("completed Window call details are folded and loaded once on first expansion", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  const row = view.nodes.windowActivity.children.find(item => item.children[0].children[0].children[0].textContent === "read_workspace_changes");
  assert.equal(row.tagName, "DETAILS");
  assert.equal(row.open, false);
  assert.equal(view.calls("read_work_result_activity_detail").length, 0);
  row.open = true;
  row.ontoggle();
  await flush();
  assert.equal(view.calls("read_work_result_activity_detail").length, 1);
  assert.deepEqual(
    { ...view.calls("read_work_result_activity_detail")[0].params.arguments },
    { project, server_trace_id: "trace-reviewed" },
  );
  await view.reply(view.calls("read_work_result_activity_detail")[0], toolResult({
    activity_detail: {
      server_trace_id: "trace-reviewed",
      started_at_ms: 1_999_999_989_000,
      ended_at_ms: 1_999_999_990_000,
      duration_ms: 1000,
      service_ms: 740,
      method: "tools/call",
      tool_name: "read_workspace_changes",
      project,
      status: "success",
      meaningful: true,
      observed_job_ids: [],
      workflow_sessions: [{ workflow_session_id: session_id, project, relation: "recorded" }],
    },
  }));
  assert.equal(row.children[1].children[0].className, "detail-grid");
  row.open = false; row.ontoggle();
  row.open = true; row.ontoggle();
  await flush();
  assert.equal(view.calls("read_work_result_activity_detail").length, 1);
});

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
  assert(rows.every(row => row.children[0].children[0].children[0].textContent === "read_files"));
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
  await view.fireTimers(10000);
  assert.equal(view.calls("get_work_result_state").length, 1);
  await view.reject(view.calls("get_work_result_state")[0]);
  assert.match(view.nodes.status.textContent, /Refresh unavailable.*last snapshot/);
  assert([...view.timers.values()].some(timer => timer.delay === 20000));
});

test("Results shows live file states and checks, preserving nodes on unrelated refresh", async () => {
  const view = app("mcp_work_result_app.html");
  const state = structuredClone(baseState);
  state.workspace.files = [
    {
      path: 'src/new.rs', old_path: 'src/old.rs', status: 'renamed', staged: true, unstaged: true, additions: 3, deletions: 2,
      content_kind: 'diff', content: '@@ -1 +1 @@\n-old\n+new\n', content_truncated: false,
    },
    {
      path: 'notes/<draft>.md', status: 'untracked',
      content_kind: 'preview', content: '<not markup>\nsecond line', content_truncated: true,
    },
    { path: 'src/gone.rs', status: 'deleted', unstaged: true, additions: 0, deletions: 8 },
  ];
  state.workspace.files_total = 12;
  state.workspace.truncated = true;
  view.toolResult({ work_result: state }); await view.initialize();
  view.nodes.viewResults.onclick();
  assert.equal(view.nodes.panelResults.hidden, false);
  assert.equal(view.nodes.workspaceFiles.children.length, 3);
  const row = view.nodes.workspaceFiles.children[0];
  assert.match(row.children[0].textContent, /src\/old.rs → src\/new.rs/);
  assert.equal(row.children[0].getAttribute('aria-expanded'), 'false');
  assert.equal(row.children[1].children.length, 1, 'no eager diff DOM');
  row.children[0].onclick(); await flush();
  const preview = view.nodes.workspaceFiles.children[1];
  preview.children[0].onclick(); await flush();
  assert.equal(row.children[1].children[0].textContent, 'Changed content');
  assert.equal(row.children[1].children[1].children[1].textContent, '-old');
  assert.equal(row.children[1].children[1].children[2].textContent, '+new');
  assert.equal(preview.children[1].children[0].textContent, 'New file preview · partial');
  assert.equal(preview.children[1].children[1].children[0].textContent, '<not markup>');
  assert.equal(view.nodes.validationStatus.textContent, 'Checks passed');
  assert.equal(view.nodes.finalChanges.hidden, true);
  assert.equal(view.calls('get_work_result_state').length, 0);
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls('get_work_result_state')[0], toolResult({ work_result: { ...state, state_version: `wr2_${'b'.repeat(64)}` } }));
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  assert.equal(view.nodes.panelResults.hidden, false);
  view.nodes.messageInput.value = 'Existing draft';
  view.nodes.discussResults.onclick();
  assert.equal(view.nodes.panelCollaboration.hidden, false);
  assert.equal(view.nodes.messageInput.value, 'Existing draft');
  assert.equal(view.calls('send_work_result_message').length, 0);
});

test("clean workspace refresh removes old files without claiming success", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  view.nodes.tabResults.onclick();
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls('get_work_result_state')[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.workspaceFiles.children.length, 0);
  assert.equal(view.nodes.workspaceStatus.textContent, 'No uncommitted changes');
  assert.equal(view.nodes.validationStatus.textContent, 'Checks need attention');
  assert.equal(view.nodes.finalChanges.hidden, true);
});

for (const reason of ['workspace_unavailable', 'non_git_project']) test(`unavailable files are explicit: ${reason}`, async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: { ...baseState, workspace: { ...baseState.workspace, git_available: false, reason_code: reason } } });
  await view.initialize();
  assert.equal(view.nodes.workspaceFiles.children.length, 0);
  assert.match(view.nodes.workspaceStatus.textContent, /unavailable|not a Git repository/);
  assert.equal(view.nodes.workspaceStats.children.length, 0);
});

test("Results works without a Session and does not invent check evidence", async () => {
  const view = app("mcp_work_result_app.html");
  const { session, session_id, validation, review, ...state } = baseState;
  view.toolResult({ work_result: state }); await view.initialize();
  view.nodes.tabResults.onclick();
  assert.equal(view.nodes.workspaceFiles.children.length, 1);
  assert.equal(view.nodes.resultChecks.hidden, true);
});

test("final results and current workspace remain separate across refresh", async () => {
  const view = await frozenView();
  view.nodes.viewResults.onclick();
  const frozen = frozenNodes(view);
  assert.equal(view.nodes.panelResults.hidden, false);
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.match(view.nodes.workspaceFiles.children[0].children[0].textContent, /src\/a.rs/);
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls('get_work_result_state')[0], toolResult({ work_result: { ...nextState, final_changes: finalChanges } }));
  assert.equal(frozenNodes(view).root, frozen.root);
  assert.equal(view.nodes.workspaceFiles.children.length, 0);
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.calls('read_changed_file_diff').length, 0);
});

test("unsafe live file paths fail closed and clear previously displayed results", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls('get_work_result_state')[0], toolResult({ work_result: { ...nextState, workspace: { ...baseState.workspace, files: [{ path: '../private' }] } } }));
  assert.equal(view.nodes.workspaceFiles.children.length, 0);
  assert.equal(view.nodes.workspaceStatus.textContent, 'Changes unavailable');
  assert.equal(view.nodes.refresh.disabled, true);
});

const runningJobs = { available: true, active: true, truncated: false, items: [
  { job_id: "job-one", tool: "cargo_test", status: "running", state: "active" },
] };

test("exact Session Job transitions through normal refresh without a model tool result", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: runningJobs } });
  await view.initialize();
  assert.equal(view.nodes.jobsSection.hidden, false);
  assert.equal(view.nodes.jobsList.children[0].children[0].children[1].textContent, "Running");
  assert.equal(view.nodes.badge.textContent, "Working");
  const terminal = { ...baseState, state_version: `wr2_${"c".repeat(64)}`, jobs: {
    available: true, active: false, truncated: false,
    items: [{ ...runningJobs.items[0], status: "completed", state: "terminal", outcome: "passed" }],
  } };
  await view.fireTimers(10000);
  const call = view.calls("get_work_result_state").at(-1);
  assert.deepEqual({ ...call.params.arguments }, { project, session_id, automatic: true });
  await view.reply(call, toolResult({ work_result: terminal }));
  await flush();
  assert.equal(view.nodes.jobResultsList.children[0].children[0].children[1].textContent, "Passed");
  assert.equal(view.calls("observe_jobs").length, 0);
  assert.equal(view.calls("ui/message").length, 0);
});

test("background executions separate live work from folded outcomes without declaring task completion", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: { ...runningJobs, items: [
    { ...runningJobs.items[0], job_id: "failed-one", state: "terminal", status: "failed", outcome: "failed" },
    runningJobs.items[0],
    { ...runningJobs.items[0], job_id: "passed-one", state: "terminal", status: "completed", outcome: "passed" },
  ] } } });
  await view.initialize();
  assert.equal(view.nodes.jobsList.children.length, 1);
  assert.equal(view.nodes.jobResultsList.children.length, 2);
  assert.equal(view.nodes.jobResults.hidden, false);
  assert.equal(view.nodes.jobResults.open, false);
  assert.equal(view.nodes.jobResultsSummary.textContent, "Recent background results · 1 failed · 1 passed");
  assert.equal(view.nodes.activityStatus.textContent, "Work continuing in the background");
  assert.equal(view.nodes.activityAge.textContent, "");
  view.nodes.jobResults.open = true;
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: {
    ...baseState, state_version: nextState.state_version, jobs: { ...runningJobs, active: false, items: [
      { ...runningJobs.items[0], state: "terminal", status: "completed", outcome: "passed" },
    ] },
  } }));
  assert.equal(view.nodes.jobsList.children.length, 0);
  assert.equal(view.nodes.jobResults.open, true);
  assert.equal(view.nodes.activityStatus.textContent, "Reviewed changes");
  assert.notEqual(view.nodes.badge.textContent, "Working");
});

test("live Window activity stays primary while background work is visible", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: runningJobs, activity: {
    ...baseState.activity, active: true,
    current: { label: "Reading files", kind: "read", started_at_ms: 1_999_999_999_000 },
  } } });
  await view.initialize();
  assert.equal(view.nodes.activityStatus.textContent, "Reading files");
  assert.equal(view.nodes.jobsList.children.length, 1);
  assert.equal(view.nodes.jobResults.hidden, true);
});

test("truncated background state does not mistake a terminal visible slice for completion", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: { ...runningJobs, truncated: true, items: [
    { ...runningJobs.items[0], state: "terminal", status: "failed", outcome: "failed" },
  ] } } });
  await view.initialize();
  assert.equal(view.nodes.activityStatus.textContent, "Work continuing in the background");
  assert.equal(view.nodes.jobsList.children.length, 0);
  assert.equal(view.nodes.badge.textContent, "Working");
  assert.match(view.nodes.jobsMeta.textContent, /more may be active or finished/);
  assert.match(view.nodes.jobResultsSummary.textContent, /1 failed/);
});

for (const status of ["queued", "recovering", "stop_requested"]) test(`background ${status} does not claim execution is running`, async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: { ...runningJobs, items: [
    { ...runningJobs.items[0], status },
  ] } } });
  await view.initialize();
  assert.equal(view.nodes.activityAge.textContent, "");
  assert.notEqual(view.nodes.jobsList.children[0].children[0].children[1].textContent, "Running");
  assert.equal(view.nodes.jobResults.hidden, true);
});

test("Window-linked Session does not become Job authority on refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput({ project });
  view.toolResult({ work_result: { ...baseState, jobs: { available: false } } });
  await view.initialize();
  assert.equal(view.nodes.jobsSection.hidden, true);
  await view.fireTimers(10000);
  assert.deepEqual({ ...view.calls("get_work_result_state")[0].params.arguments }, { project, automatic: true });
});

test("Job failures and recovery remain bounded labels and conflicting Session fails closed", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: { ...runningJobs, active: false, items: [
    { ...runningJobs.items[0], state: "terminal", status: "failed", outcome: "failed", recovery_state: "recovered" },
  ] } } });
  await view.initialize();
  assert.equal(view.nodes.jobResultsList.children[0].children[0].children[1].textContent, "Failed · recovered");
  view.toolInput({ project, session_id: `wc_sess_${"2".repeat(32)}` });
  assert.equal(view.nodes.badge.textContent, "Unavailable");
  assert.equal(view.nodes.jobsSection.hidden, true);
  assert.equal(view.timers.size, 0);
});

test("active Jobs use the existing timer and preserve hidden and teardown fences", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs: runningJobs } });
  await view.initialize();
  await view.fireTimers(10000);
  const call = view.calls("get_work_result_state")[0];
  view.notification("ui/resource-teardown", {});
  await view.reply(call, toolResult({ work_result: nextState }));
  await flush();
  assert.equal(view.timers.size, 0);
  assert.equal(view.nodes.jobsList.children[0].children[0].children[1].textContent, "Running");
});

test("result-first Job state cannot be rebound by a different explicit Session", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: { ...baseState, jobs: runningJobs } });
  view.toolInput({ project, session_id: `wc_sess_${"2".repeat(32)}` });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Unavailable");
  assert.equal(view.calls("get_work_result_state").length, 0);
});

for (const jobs of [
  { ...runningJobs, items: Array.from({ length: 9 }, () => runningJobs.items[0]) },
  { ...runningJobs, items: [{ ...runningJobs.items[0], state: "guessed" }] },
]) test("invalid Job state fails closed", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: { ...baseState, jobs } });
  await view.initialize();
  assert.equal(view.nodes.badge.textContent, "Unavailable");
});

function filePreview(view, final = false, index = 0) {
  const row = view.nodes[final ? "frozenFiles" : "workspaceFiles"].children[index];
  row.children[0].onclick();
  const wrap = row.children[1], controls = wrap.children[2], preview = wrap.children[3];
  return { row, wrap, controls, state: preview.children[0], text: preview.children[1], markdown: preview.children[2],
    retry: controls.children.find(node => node.textContent === "Retry preview"), hint: controls.children.at(-1) };
}
function previewReply(request, text, extra = {}) {
  const args = request.params.arguments;
  const offset = args.files.byte_offset;
  return toolResult({ work_result_files: {
    project: args.project, session_id: args.session_id ?? null,
    snapshot_id: args.files.snapshot_id, path: args.files.path, view: "content",
    byte_offset: offset, bytes_total: offset + new TextEncoder().encode(text).length,
    content: text, complete: true, limited: false, next_byte_offset: null, ...extra,
  } });
}
function descendants(node) { return [node, ...node.children.flatMap(descendants)]; }

async function readingView(state = baseState) {
  const view = app("mcp_work_result_app.html");
  view.toolInput({});
  view.notification("ui/notifications/tool-result", threadResult(state, session_id));
  await view.initialize();
  return view;
}

function readingFile(view, final = false, index = 0) {
  const row = view.nodes[final ? "frozenFiles" : "workspaceFiles"].children[index];
  row.expand();
  const controls = row.children[1].children[0], pre = row.children[1].children[2], preview = row.children[1].children[3];
  return { row, controls, pre, text: preview.children[1], markdown: preview.children[2], button: label => controls.children.find(node => node.textContent === label) };
}

test("thread section navigation moves focus without reading files or resetting an expanded row", async () => {
  const view = await readingView(frozenWork());
  assert.equal(view.nodes.reviewSections.hidden, false);
  assert.equal(view.nodes.jumpWorkspace.hidden, false);
  assert.equal(view.nodes.jumpFinal.hidden, false);
  assert.equal(view.nodes.jumpOutputs.hidden, true);
  const moves = [];
  for (const id of ["workspaceChangesSection", "finalChanges", "resultChecks"]) {
    view.nodes[id].scrollIntoView = () => moves.push(id);
    view.nodes[id].focus = options => assert.equal(options.preventScroll, true);
  }
  view.nodes.jumpWorkspace.onclick();
  view.nodes.jumpFinal.onclick();
  view.nodes.workspaceNavigator.children[3].onclick();
  view.nodes.frozenNavigator.children[3].onclick();
  assert.deepEqual(moves, ["workspaceChangesSection", "finalChanges", "resultChecks", "resultChecks"]);
  assert.equal(view.calls("get_work_result_state").length, 0);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
  const file = readingFile(view, true);
  const requests = view.sent.length;
  view.nodes.frozenNavigator.children[3].onclick();
  assert.equal(file.row.children[0].getAttribute("aria-expanded"), "true");
  assert.equal(view.sent.length, requests);
  await view.teardown();
  const before = moves.length;
  view.nodes.jumpWorkspace.onclick();
  assert.equal(moves.length, before, "teardown stops section actions");
});

test("thread overview preserves failed, stale, missing and unrun evidence", async () => {
  for (const [status, label] of [["passed", "Checks passed"], ["failed", "Checks need attention"],
    ["stale", "Checks are out of date"], ["not_run", "Checks not run"], ["inconclusive", "Checks inconclusive"]]) {
    const state = structuredClone(baseState);
    state.validation.current_status = status;
    state.workspace = structuredClone(nextState.workspace);
    const view = await readingView(state);
    assert.equal(view.nodes.validationStatus.textContent, label);
    assert.equal(view.nodes.resultChecks.hidden, false);
    assert.equal(view.nodes.jumpFinal.hidden, true);
  }
  const state = structuredClone(baseState);
  for (const key of ["session_id", "session", "validation", "review"]) delete state[key];
  const view = app("mcp_work_result_app.html");
  view.notification("ui/notifications/tool-result", threadResult(state, null));
  await view.initialize();
  assert.equal(view.nodes.resultChecks.hidden, false);
  assert.equal(view.nodes.validationStatus.textContent, "Check status unavailable");
  assert.equal(view.nodes.reviewStatus.textContent, "Review status unavailable");
  assert.match(view.nodes.checksScope.textContent, /No Session is linked/);
  view.toolInput({ project: "other-project" });
  assert.equal(view.nodes.resultChecks.hidden, true, "invalid binding clears the overview");
});

test("thread file navigation is scoped to visible loaded files and preserves mounted rows", async () => {
  const state = structuredClone(frozenWork());
  state.workspace.files = Array.from({ length: 8 }, (_, i) => ({ path: `src/file_${i}.rs`, status: "modified" }));
  state.workspace.files_total = 30; state.workspace.truncated = true;
  const view = await readingView(state);
  const [select, previous, next] = view.nodes.workspaceNavigator.children;
  assert.equal(select.children.length, 5);
  assert.equal(previous.disabled, true); assert.equal(next.disabled, false);
  assert.equal(view.calls("get_work_result_state").length, 0, "navigation does not eagerly read files");
  const first = view.nodes.workspaceFiles.children[0];
  let navigation = 0;
  view.nodes.workspaceFiles.children[1].scrollIntoView = () => navigation++;
  next.onclick(); await flush();
  assert.equal(select.value, "src/file_1.rs"); assert.equal(navigation, 1);
  const read = view.calls("get_work_result_state")[0];
  await view.reply(read, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: 24, files_total: 30, source_truncated: false,
    files: Array.from({ length: 24 }, (_, i) => frozenFile(i)),
  } }));
  const second = view.nodes.workspaceFiles.children[1];
  first.remove = second.remove = () => { throw new Error("Reading row detached during paging"); };
  view.nodes.workspaceMore.onclick(); await flush();
  assert.equal(select.children.length, 10);
  assert.equal(select.value, "src/file_1.rs");
  view.nodes.workspaceLess.onclick();
  assert.equal(select.children.length, 5);
  assert.equal(second.children[0].getAttribute("aria-expanded"), "true");
  assert.equal(view.nodes.frozenNavigator.children[0].value, finalChanges.files[0].path, "final navigation is independent");
  view.nodes.frozenMore.onclick(); await flush();
  assert.equal(view.nodes.frozenNavigator.children[0].children.length, 7);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
});

test("path filtering covers loaded metadata without fetching, detaching rows or losing reading state", async () => {
  const state = structuredClone(frozenWork());
  state.workspace.files = Array.from({ length: 8 }, (_, i) => ({ path: `docs/文件_[${i}].md`, status: "modified", content: "+saved", content_kind: "diff" }));
  state.workspace.files_total = 30; state.workspace.truncated = true;
  const view = await readingView(state), first = readingFile(view);
  first.button("Wrap lines").onclick();
  const nav = view.nodes.workspaceNavigator, filter = nav.children[4].children[0];
  const requests = view.sent.length;
  filter.value = "[7]"; filter.oninput();
  assert.equal(nav.children[0].children.length, 1);
  assert.equal(nav.children[0].value, "docs/文件_[7].md");
  assert.match(nav.children[5].textContent, /1 shown · 8 loaded · 30 total · filtering loaded paths only/);
  assert.equal(view.sent.length, requests);
  assert.equal(view.nodes.workspaceFiles.children[0], first.row);
  filter.value = "missing"; filter.oninput();
  assert.equal(nav.hidden, false, "empty results leave the filter usable");
  assert.equal(nav.children[0].disabled, true);
  assert.equal(nav.children[1].disabled, true);
  assert.equal(nav.children[2].disabled, true);
  assert.match(nav.children[5].textContent, /no matching loaded files/);
  filter.onkeydown({ key: "Escape", preventDefault() {} });
  assert.equal(nav.children[0].children.length, 5);
  assert.equal(first.row.hidden, false);
  assert.equal(first.pre.className, "diff no-wrap");
  assert.equal(first.row.children[0].getAttribute("aria-expanded"), "true");
  assert.equal(view.nodes.frozenNavigator.children[0].children.length, 5, "final filter is independent");
  await view.teardown(); filter.value = "[7]"; filter.oninput();
  assert.equal(nav.children[0].children.length, 5);
});

test("filtered final files page only on explicit request and extend the same literal filter", async () => {
  const state = structuredClone(frozenWork());
  state.final_changes.files_total = 8; state.final_changes.files_changed = 8; state.final_changes.files_truncated = true;
  const view = await readingView(state), nav = view.nodes.frozenNavigator;
  const filter = nav.children[4].children[0];
  filter.value = "file_7"; filter.oninput();
  assert.equal(nav.children[0].children.length, 0);
  assert.equal(view.nodes.frozenMore.hidden, false);
  assert.equal(view.calls("get_work_result_state").length, 0);
  view.nodes.frozenMore.onclick(); await flush();
  const request = view.calls("get_work_result_state")[0];
  assert.equal(request.params.arguments.files.offset, 7);
  await view.reply(request, toolResult({ work_result_files: {
    project, session_id, snapshot_id, offset: 7, next_offset: null, files_total: 8, source_truncated: false,
    files: [frozenFile(7)],
  } }));
  assert.equal(nav.children[0].children.length, 1);
  assert.equal(view.nodes.frozenMore.hidden, true);
  assert.equal(view.calls("read_changed_file_diff").length, 0);
});

test("complete file paths copy explicitly with a manual fallback when clipboard access fails", async () => {
  for (const denied of [false, true]) {
    const copied = [];
    const view = app("mcp_work_result_app.html", { navigator: { clipboard: { async writeText(text) { if (denied) throw new Error("denied"); copied.push(text); } } } });
    const state = structuredClone(baseState);
    const path = "docs/很长的目录/".repeat(10) + "name with spaces.md";
    state.workspace.files[0].path = path;
    view.notification("ui/notifications/tool-result", threadResult(state, session_id)); await view.initialize();
    const [summary, field, copy, status] = view.nodes.workspaceNavigator.children[6].children;
    assert.equal(field.value, path); assert.equal(copied.length, 0);
    let selected = false; field.select = () => { selected = true; };
    await copy.onclick();
    assert.equal(selected, denied);
    assert.equal(denied ? status.textContent.includes("manually") : copied[0] === path, true);
    await view.teardown(); await copy.onclick();
    assert.equal(copied.length, denied ? 0 : 1);
  }
});

async function quoteView({ capabilities = { updateModelContext: { text: {} } }, hostContext, navigator, state = baseState } = {}) {
  const copy = structuredClone(state);
  Object.assign(copy.workspace.files[0], { content: "@@ -1 +1 @@\n-before\n+after", content_kind: "diff" });
  const view = app("mcp_work_result_app.html", { navigator });
  view.notification("ui/notifications/tool-result", threadResult(copy, session_id));
  await view.reply(view.sent[0], { protocolVersion: "2026-01-26", hostCapabilities: capabilities, hostContext });
  return view;
}
function selectExcerpt(view, file, text = "+after", node = file.pre.children.at(-1)) {
  view.selection.value = { rangeCount: 1, isCollapsed: false, toString: () => text,
    getRangeAt: () => ({ startContainer: node, endContainer: node, toString: () => text.replaceAll("\n", "") }) };
  file.button("Quote selection").onclick();
}
const contextUpdates = view => view.sent.filter(request => request.method === "ui/update-model-context");

test("quoting previews exact text and provenance without reads or sending, then adds only on confirmation", async () => {
  const view = await quoteView(), file = readingFile(view), before = view.sent.length;
  selectExcerpt(view, file, "-before\n+after <script>literal</script> 😀");
  assert.equal(view.sent.length, before);
  assert.equal(view.nodes.quotePanel.hidden, false);
  assert.equal(view.nodes.quoteText.textContent, "-before\n+after <script>literal</script> 😀", "rendered line breaks are preserved");
  assert.equal(view.nodes.quoteText.children.length, 0);
  assert.match(view.nodes.quoteCopy.value, new RegExp(`Observation: ${baseState.state_version}`));
  assert.match(view.nodes.quoteCopy.value, /not a pinned file snapshot/);
  const adding = view.nodes.quoteAdd.onclick();
  view.nodes.quoteAdd.onclick();
  assert.equal(contextUpdates(view).length, 1);
  const request = contextUpdates(view)[0];
  assert.equal(request.params.content.length, 1);
  assert.equal(request.params.content[0].text, view.nodes.quoteCopy.value);
  assert.match(request.params.content[0].text, /File: "src\/a.rs"/);
  await view.reply(request, {}); await adding;
  assert.equal(view.nodes.quoteDraft.hidden, true);
  assert.equal(view.nodes.quoteReferences.children.length, 1);
  assert.match(view.nodes.quoteReferences.children[0].children[0].textContent, /-before\n\+after/);
  selectExcerpt(view, file, "-before\n+after <script>literal</script> 😀");
  await view.nodes.quoteAdd.onclick();
  assert.equal(contextUpdates(view).length, 1, "duplicate quote does not replace context again");
  assert.equal(view.sent.filter(request => request.method === "ui/message").length, 0);
  assert.equal(view.calls("send_work_result_message").length, 0);
});

test("quotes reject cross-file and oversized selections, cancel locally and discard stale draft sources", async () => {
  const view = await quoteView(), file = readingFile(view);
  selectExcerpt(view, file, "outside", view.nodes.resultChecks);
  assert.equal(view.nodes.quotePanel.hidden, true);
  selectExcerpt(view, file, "😀".repeat(4001));
  assert.equal(view.nodes.quotePanel.hidden, true);
  view.selection.value = { rangeCount: 0, isCollapsed: true };
  file.button("Quote selection").onclick();
  assert.equal(view.nodes.quotePanel.hidden, true);
  selectExcerpt(view, file, "😀".repeat(4000));
  assert.equal(Array.from(view.nodes.quoteText.textContent).length, 4000);
  view.nodes.quoteCancel.onclick();
  assert.equal(view.nodes.quotePanel.hidden, true);
  view.viewport.scrollY = 300;
  let returned = false; file.button("Quote selection").focus = () => { returned = true; };
  selectExcerpt(view, file); view.viewport.scrollY = 0;
  view.nodes.quoteCancel.onclick();
  assert.equal(view.viewport.scrollY, 300);
  assert.equal(returned, true);
  selectExcerpt(view, file);
  view.nodes.workspaceReload.onclick();
  await view.nodes.quoteAdd.onclick();
  assert.equal(view.nodes.quoteDraft.hidden, true);
  assert.match(view.nodes.quoteStatus.textContent, /file view changed/);
  assert.equal(contextUpdates(view).length, 0);
});

test("full text and Markdown quotes retain the exact working-tree snapshot without promoting Session scope", async () => {
  const state = structuredClone(baseState); state.workspace.files[0].path = "README.md";
  const view = await quoteView({ state }), file = readingFile(view);
  file.button("Full text").onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null, files_total: 1, source_truncated: false,
    files: [frozenFile(0, { path: "README.md" })],
  } }));
  const read = view.calls("get_work_result_state")[1];
  await view.reply(read, previewReply(read, "# Snapshot text\n\nQuoted paragraph."));
  const requests = view.sent.length;
  selectExcerpt(view, file, "Quoted paragraph.", file.text);
  assert.match(view.nodes.quoteCopy.value, new RegExp(`Snapshot: ${snapshot_id}`));
  assert.doesNotMatch(view.nodes.quoteCopy.value, /Session:/);
  assert.match(view.nodes.quoteCopy.value, /View: Full text/);
  view.nodes.quoteCancel.onclick();
  file.button("Markdown").onclick();
  selectExcerpt(view, file, "Snapshot text", file.markdown.children[0]);
  assert.match(view.nodes.quoteCopy.value, /View: Markdown/);
  assert.equal(view.sent.length, requests);
});

test("final Diff quotes carry the frozen snapshot and explicit Session", async () => {
  const view = await quoteView({ state: frozenWork() }), file = readingFile(view, true);
  await view.reply(view.calls("read_changed_file_diff")[0], frozenDiff());
  selectExcerpt(view, file, "+new");
  assert.match(view.nodes.quoteCopy.value, new RegExp(`Snapshot: ${snapshot_id}`));
  assert.match(view.nodes.quoteCopy.value, new RegExp(`Session: ${session_id}`));
  assert.match(view.nodes.quoteCopy.value, /Source: Final changes/);
});

test("Hosts without text context get a copyable quote and never receive a fabricated resource or message", async () => {
  for (const capabilities of [{}, { updateModelContext: { resource: {}, resourceLink: {} } }]) {
    const view = await quoteView({ capabilities }), file = readingFile(view);
    selectExcerpt(view, file);
    assert.equal(view.nodes.quoteAdd.textContent, "Copy quote");
    assert.equal(view.nodes.quoteCopy.hidden, false);
    let selected = false; view.nodes.quoteCopy.select = () => { selected = true; };
    await view.nodes.quoteAdd.onclick();
    assert.equal(selected, true);
    assert.equal(contextUpdates(view).length, 0);
    assert.equal(view.sent.filter(request => request.method === "ui/message").length, 0);
  }
  const copied = [], view = await quoteView({ capabilities: {}, navigator: { clipboard: { async writeText(text) { copied.push(text); } } } });
  selectExcerpt(view, readingFile(view));
  assert.equal(copied.length, 0);
  await view.nodes.quoteAdd.onclick();
  assert.equal(copied[0], view.nodes.quoteCopy.value);
});

test("context update uncertainty retries only the exact set and teardown stops all quote actions", async () => {
  const view = await quoteView(), file = readingFile(view);
  selectExcerpt(view, file);
  const first = view.nodes.quoteAdd.onclick(), request = contextUpdates(view)[0];
  await view.reject(request); await first;
  assert.equal(view.nodes.quoteRetry.hidden, false);
  assert.equal(view.nodes.quoteAdd.disabled, true);
  selectExcerpt(view, file, "different");
  assert.equal(view.nodes.quoteText.textContent, "+after");
  view.nodes.quoteRetry.onclick();
  assert.deepEqual(contextUpdates(view)[1].params, request.params);
  await view.reply(contextUpdates(view)[1], {});
  assert.equal(view.nodes.quoteReferences.children.length, 1);
  await view.teardown();
  const requests = view.sent.length;
  file.button("Quote selection").onclick();
  await view.nodes.quoteAdd.onclick(); view.nodes.quoteRetry.onclick();
  assert.equal(view.sent.length, requests);
  assert.equal(view.nodes.quoteText.textContent, "");
});

test("advertised Host context restores other references and canonical removal wins over a delayed acknowledgment", async () => {
  const original = { type: "resource_link", uri: "webcodex-resource://file/example", name: "Existing file" };
  const capabilities = { updateModelContext: { text: {} }, experimental: { "openai/modelContext": {} } };
  const view = await quoteView({ capabilities, hostContext: { "openai/modelContext": { updateId: "initial", content: [original] } } });
  const file = readingFile(view); selectExcerpt(view, file);
  const adding = view.nodes.quoteAdd.onclick(), request = contextUpdates(view)[0];
  assert.deepEqual(JSON.parse(JSON.stringify(request.params.content[0])), original);
  view.notification("ui/notifications/host-context-changed", { "openai/modelContext": null });
  await view.reply(request, { _meta: { "openai/modelContext": { updateId: "late" } } }); await adding;
  assert.equal(view.nodes.quoteReferences.children.length, 0);
  assert.match(view.nodes.quoteStatus.textContent, /cleared in the chat/);
  const second = view.nodes.quoteAdd.onclick();
  assert.equal(contextUpdates(view)[1].params.content.length, 1, "cleared resource is not resurrected");
  await view.reply(contextUpdates(view)[1], { _meta: { "openai/modelContext": { updateId: "accepted" } } });
  await second;
  view.notification("ui/notifications/host-context-changed", { "openai/modelContext": { updateId: "accepted" } });
  assert.equal(view.nodes.quoteReferences.children.length, 1, "matching acknowledgment does not clear content");
  const reference = view.nodes.quoteReferences.children[0];
  view.notification("ui/notifications/host-context-changed", { "openai/modelContext": { updateId: "same-content", content: JSON.parse(JSON.stringify(contextUpdates(view)[1].params.content)) } });
  assert.equal(view.nodes.quoteReferences.children[0], reference, "unchanged context preserves keyboard focus");
  view.nodes.quoteReferences.children[0].children[1].onclick();
  assert.equal(contextUpdates(view)[2].params.content.length, 0);
  await view.reply(contextUpdates(view)[2], {});
  assert.equal(view.nodes.quoteReferences.children.length, 0);
});

test("unadvertised synchronization and oversized Host context cannot overwrite references", async () => {
  const view = await quoteView();
  view.notification("ui/notifications/host-context-changed", { "openai/modelContext": { updateId: "ignored", content: [{ type: "text", text: "ignored" }] } });
  assert.equal(view.nodes.quoteReferences.children.length, 0);
  const capabilities = { updateModelContext: { text: {} }, experimental: { "openai/modelContext": {} } };
  for (const content of [Array.from({ length: 13 }, () => ({ type: "text", text: "old" })), [{ type: "text", text: "x".repeat(65536) }]]) {
    const blocked = await quoteView({ capabilities, hostContext: { "openai/modelContext": { updateId: "large", content } } });
    selectExcerpt(blocked, readingFile(blocked));
    await blocked.nodes.quoteAdd.onclick();
    assert.equal(blocked.nodes.quoteAdd.disabled, true);
    assert.equal(contextUpdates(blocked).length, 0);
  }
  const full = await quoteView({ capabilities, hostContext: { "openai/modelContext": { updateId: "full", content: Array.from({ length: 12 }, () => ({ type: "text", text: "old" })) } } });
  selectExcerpt(full, readingFile(full)); await full.nodes.quoteAdd.onclick();
  assert.equal(contextUpdates(full).length, 0);
  assert.match(full.nodes.quoteStatus.textContent, /Context is full/);
});

test("a late context acknowledgment cannot restore a torn-down quote", async () => {
  const view = await quoteView(); selectExcerpt(view, readingFile(view));
  const adding = view.nodes.quoteAdd.onclick(), request = contextUpdates(view)[0];
  await view.teardown(); await adding;
  await view.reply(request, {});
  assert.equal(view.nodes.quoteText.textContent, "");
  assert.equal(view.nodes.quoteReferences.children.length, 0);
  assert.equal(view.nodes.quoteAdd.disabled, true);
  assert.equal(contextUpdates(view).length, 1);
});

test("context acknowledgment restores disabled-button focus without stealing it after the user moves away", async () => {
  for (const movedAway of [false, true]) {
    const view = await quoteView(), file = readingFile(view);
    view.nodes.quotePanel.append(view.nodes.quoteAdd);
    selectExcerpt(view, file); view.nodes.quoteAdd.focus();
    const adding = view.nodes.quoteAdd.onclick();
    // Browsers blur a focused button as soon as the pending action disables it.
    view.document.activeElement = view.document.body;
    if (movedAway) file.button("Quote selection").focus();
    await view.reply(contextUpdates(view)[0], {}); await adding;
    assert.equal(view.document.activeElement, movedAway ? file.button("Quote selection") : view.nodes.quotePanel);
  }
});

test("thread Diff numbers follow hunk ranges, zero-length sides and literal plus/minus content", async () => {
  const diff = ["diff --git a/demo b/demo", "--- a/demo", "+++ b/demo", "@@ -10,3 +20,4 @@ title", " same", "---literal", "+++literal", " after", "+extra", "\\ No newline at end of file", "@@ -0,0 +1 @@", "+new", "@@ -6 +0,0 @@", "-gone", "@@ malformed", "+unknown"].join("\n");
  const state = structuredClone(baseState);
  Object.assign(state.workspace.files[0], { content: diff, content_kind: "diff" });
  const view = await readingView(state), { pre, button } = readingFile(view);
  const rows = pre.children.map(line => [line.getAttribute("data-old-line"), line.getAttribute("data-new-line"), line.children[0].textContent]);
  assert.deepEqual(rows.slice(4, 9), [["10", "20", " same"], ["11", "", "---literal"], ["", "21", "+++literal"], ["12", "22", " after"], ["", "23", "+extra"]]);
  assert.deepEqual(rows[11], ["", "1", "+new"]);
  assert.deepEqual(rows[13], ["6", "", "-gone"]);
  assert.deepEqual(rows[15], ["", "", "+unknown"]);
  assert.equal(pre.children[5].className, "diff-line deleted");
  assert.equal(pre.children[6].className, "diff-line added");
  button("Wrap lines").onclick();
  assert.equal(pre.className, "diff no-wrap");
  assert.equal(button("Wrap lines").getAttribute("aria-pressed"), "false");
  assert.equal(view.calls("get_work_result_state").length, 0);
});

test("thread keeps Diff DOM and wrap state through refresh and collapse", async () => {
  const state = structuredClone(baseState);
  Object.assign(state.workspace.files[0], { content: "@@ -1 +1 @@\n-old\n+new", content_kind: "diff" });
  const view = await readingView(state), file = readingFile(view);
  const firstLine = file.pre.children[0];
  file.button("Wrap lines").onclick();
  file.row.collapse(); file.row.expand();
  assert.equal(file.pre.children[0], firstLine);
  assert.equal(file.pre.className, "diff no-wrap");
  view.nodes.refresh.onclick();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result: { ...state, state_version: nextState.state_version } }));
  assert.equal(view.nodes.workspaceFiles.children[0], file.row);
  assert.equal(file.pre.children[0], firstLine);
  assert.equal(file.row.children[0].getAttribute("aria-expanded"), "true");
});

test("thread restores each reading mode and fences asynchronous previews after snapshot reset", async () => {
  const state = structuredClone(baseState);
  Object.assign(state.workspace.files[0], { path: "README.md", content: "@@ -1 +1 @@\n-old\n+new", content_kind: "diff" });
  const view = await readingView(state), file = readingFile(view);
  const bounds = () => ({ top: 100 - view.viewport.scrollY, bottom: 1100 - view.viewport.scrollY, height: 1000 });
  file.pre.getBoundingClientRect = file.text.getBoundingClientRect = bounds;
  view.viewport.scrollY = 300;
  file.button("Full text").onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null, files_total: 1, source_truncated: false,
    files: [{ path: "README.md", kind: "modified", additions: 4, deletions: 1 }],
  } }));
  const read = view.calls("get_work_result_state")[1];
  await view.reply(read, previewReply(read, "# Safe preview\n"));
  view.viewport.scrollY = 450;
  file.button("Changes").onclick();
  assert.equal(view.viewport.scrollY, 300, "Diff restores its own offset");
  file.button("Full text").onclick();
  assert.equal(view.viewport.scrollY, 450, "Full text restores its separate offset");
  assert.equal(view.calls("get_work_result_state").length, 2, "cached modes do not refetch");
  view.nodes.workspaceReload.onclick();
  assert.equal(view.nodes.workspaceNavigator.children[0].value, "README.md", "a new snapshot starts with a valid file selection");
  const replacement = readingFile(view);
  assert.notEqual(replacement.row, file.row);
  assert.equal(replacement.pre.className, "diff");
  replacement.button("Full text").onclick(); await flush();
  const pending = view.calls("get_work_result_state").at(-1);
  view.nodes.workspaceReload.onclick();
  const currentRow = view.nodes.workspaceFiles.children[0];
  await view.reply(pending, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null, files_total: 1, source_truncated: false,
    files: [{ path: "README.md", kind: "modified", additions: 4, deletions: 1 }],
  } }));
  assert.equal(view.nodes.workspaceFiles.children[0], currentRow);
  assert.equal(currentRow.children[0].getAttribute("aria-expanded"), "false");
  assert.equal(view.calls("get_work_result_state").at(-1), pending, "late snapshot cannot start another content read");
});

test("thread failure removes navigation together with file content", async () => {
  const view = await readingView(frozenWork());
  view.toolInput({ project: "other-project" });
  assert.equal(view.nodes.badge.textContent, "Unavailable");
  assert.equal(view.nodes.workspaceNavigator.hidden, true);
  assert.equal(view.nodes.frozenNavigator.hidden, true);
});

async function workspacePreview() {
  const view = app("mcp_work_result_app.html");
  view.toolInput({ project });
  const state = structuredClone(baseState);
  for (const key of ["session_id", "session", "validation", "review"]) delete state[key];
  state.workspace.files[0].path = "README.md";
  view.toolResult({ work_result: state });
  await view.initialize();
  const nodes = filePreview(view);
  nodes.controls.children[1].onclick(); await flush();
  await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null, files_total: 1, source_truncated: false,
    files: [{ path: "README.md", kind: "modified", additions: 4, deletions: 1 }],
  } }));
  return { view, nodes, request: view.calls("get_work_result_state").find(call => call.params.arguments.files?.view === "content") };
}

async function pdfReadingView() {
  const state = structuredClone(baseState);
  state.workspace.files[0].path = "report.pdf";
  state.workspace.files[0].binary = true;
  const view = await readingView(state);
  assert.equal(view.calls("get_work_result_state").length, 0, "inventory alone never reads PDF bytes");
  const file = readingFile(view); await flush();
  const inventory = view.calls("get_work_result_state")[0];
  await view.reply(inventory, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null, files_total: 1, source_truncated: false,
    files: [{ path: "report.pdf", kind: "modified", additions: null, deletions: null, binary: true }],
  } }));
  return { view, file, request: view.calls("get_work_result_state").find(call => call.params.arguments.files?.view === "pdf") };
}

test("PDF rows read the pinned working-tree source and fence late replies on close, mode change and teardown", async () => {
  for (const close of ["collapse", "mode", "teardown", "refresh"]) {
    const { view, file, request } = await pdfReadingView();
    assert.ok(request, "expanded PDF defaults to the PDF view");
    assert.deepEqual(JSON.parse(JSON.stringify(request.params.arguments)), { project,
      files: { snapshot_id, path: "report.pdf", view: "pdf", byte_offset: 0 } });
    assert.equal(file.button("PDF").getAttribute("aria-pressed"), "true");
    if (close === "collapse") file.row.collapse();
    if (close === "mode") file.button("Full text").onclick();
    if (close === "teardown") await view.teardown();
    if (close === "refresh") view.nodes.workspaceReload.onclick();
    const before = view.calls("get_work_result_state").length;
    const reply = toolResult({ work_result_files: {
      project, session_id: null, snapshot_id, path: "report.pdf", view: "pdf",
      byte_offset: 0, bytes_total: 200_000, complete: false, next_byte_offset: 131_072,
    } });
    reply._meta = { "webcodex/pdfChunk": { content_base64: Buffer.alloc(131_072).toString("base64") } };
    await view.reply(request, reply);
    assert.equal(view.calls("get_work_result_state").length, before, "closed preview cannot fetch the next PDF segment");
    await view.teardown();
  }
});

test("PDF reads continue through temporary path filtering without changing the pinned source", async () => {
  const { view, file, request } = await pdfReadingView();
  const filter = view.nodes.workspaceNavigator.children[4].children[0];
  filter.value = "no-match"; filter.oninput();
  assert.equal(file.row.hidden, true);
  const reply = toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, path: "report.pdf", view: "pdf",
    byte_offset: 0, bytes_total: 200_000, complete: false, next_byte_offset: 131_072,
  } });
  reply._meta = { "webcodex/pdfChunk": { content_base64: Buffer.alloc(131_072).toString("base64") } };
  await view.reply(request, reply);
  const reads = view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "pdf");
  assert.equal(reads.length, 2, "hidden rows retain the live PDF transfer");
  assert.deepEqual(JSON.parse(JSON.stringify(reads[1].params.arguments)), { project,
    files: { snapshot_id, path: "report.pdf", view: "pdf", byte_offset: 131_072 } });
  filter.value = ""; filter.oninput();
  assert.equal(file.row.hidden, false);
  file.button("PDF").onclick(); await flush();
  assert.equal(view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "pdf").length, 2,
    "restoring visibility and selecting the same tab reuse the pending transfer");
  await view.teardown();
});

test("PDF failures while path-filtered remain actionable after the row returns", async () => {
  const { view, file, request } = await pdfReadingView();
  const filter = view.nodes.workspaceNavigator.children[4].children[0];
  filter.value = "no-match"; filter.oninput();
  await view.reply(request, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, path: "report.pdf", view: "pdf", unavailable_reason: "too_large",
  } }));
  filter.value = ""; filter.oninput();
  const retry = file.button("Retry preview");
  assert.equal(file.row.hidden, false); assert.equal(retry.hidden, false);
  retry.onclick(); await flush();
  assert.deepEqual(JSON.parse(JSON.stringify(view.calls("get_work_result_state").at(-1).params.arguments)),
    JSON.parse(JSON.stringify(request.params.arguments)));
  await view.teardown();
});

test("PDF error retry keeps the same exact snapshot and never requests text for a binary PDF", async () => {
  const { view, file, request } = await pdfReadingView();
  await view.reply(request, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, path: "report.pdf", view: "pdf", unavailable_reason: "too_large",
  } }));
  const retry = file.button("Retry preview"); assert.equal(retry.hidden, false);
  retry.onclick(); await flush();
  assert.deepEqual(JSON.parse(JSON.stringify(view.calls("get_work_result_state").at(-1).params.arguments)),
    JSON.parse(JSON.stringify(request.params.arguments)));
  file.button("Full text").onclick();
  assert.equal(view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content").length, 0);
  await view.teardown();
});

test("workspace full text uses exact pinned snapshot, explicit pages and complete-only safe Markdown", async () => {
  const { view, nodes, request } = await workspacePreview();
  assert.deepEqual(JSON.parse(JSON.stringify(request.params.arguments)), { project,
    files: { snapshot_id, path: "README.md", view: "content", byte_offset: 0 } });
  assert.equal(nodes.controls.children[2].disabled, true);
  assert.equal(nodes.controls.children[2].getAttribute("aria-describedby"), nodes.hint.id);
  assert.equal(nodes.hint.hidden, false);
  const first = "# title\n\n" + "a".repeat(32 * 1024 - 10);
  const tail = '\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n~~gone~~ **bold**\n\n<script>alert(1)</script>\n\n![alt](https://example.com/track.png)\n\n[link](https://example.com/) [bad](javascript:alert(1))\n\n```js\n<script>literal</script>\n```';
  const total = new TextEncoder().encode(first + tail).length;
  const bytes = new TextEncoder().encode(first).length;
  await view.reply(request, previewReply(request, first, { bytes_total: total, complete: false, next_byte_offset: bytes }));
  assert.match(nodes.state.textContent, /Partial content/);
  assert.match(nodes.hint.textContent, /remaining file content/);
  assert.equal(nodes.controls.children[2].disabled, true);
  assert.equal(view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content").length, 1, "no automatic page loop");
  nodes.controls.children[3].onclick(); await flush();
  const next = view.calls("get_work_result_state").at(-1);
  assert.equal(next.params.arguments.files.byte_offset, bytes);
  await view.reply(next, previewReply(next, tail));
  assert.equal(nodes.text.textContent, first + tail);
  assert.match(nodes.state.textContent, /Complete file/);
  assert.equal(nodes.hint.hidden, true);
  assert.equal(nodes.controls.children[2].disabled, false);
  nodes.controls.children[2].onclick();
  const dom = descendants(nodes.markdown);
  assert.equal(dom.some(node => node.tagName === "H1" && descendants(node).some(child => child.textContent === "title")), true);
  assert.equal(dom.some(node => node.tagName === "TABLE"), true);
  assert.equal(dom.some(node => node.tagName === "S"), true);
  assert.equal(dom.some(node => node.tagName === "STRONG"), true);
  assert.equal(dom.some(node => ["SCRIPT", "IMG", "IFRAME", "A"].includes(node.tagName)), false);
  assert.equal(dom.some(node => /external image not loaded/.test(node.textContent)), true);
  assert.equal(dom.some(node => /<script>/.test(node.textContent)), true);
  assert.equal(view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content").length, 2, "rendering needs no external resource request");
});

test("final full text retains explicit Session and rejects a mismatched content scope", async () => {
  const view = await frozenView();
  const nodes = filePreview(view, true);
  nodes.controls.children[1].onclick(); await flush();
  const request = view.calls("get_work_result_state")[0];
  assert.equal(request.params.arguments.session_id, session_id);
  await view.reply(request, previewReply(request, "wrong source", { session_id: null }));
  assert.equal(nodes.text.textContent, "");
  assert.match(nodes.state.textContent, /Content unavailable/);
});

test("preview ignores late responses after switching view, refreshing snapshot and teardown", async () => {
  for (const action of ["switch", "refresh", "teardown"]) {
    const { view, nodes, request } = await workspacePreview();
    if (action === "switch") nodes.controls.children[0].onclick();
    if (action === "refresh") view.nodes.workspaceReload.onclick();
    if (action === "teardown") await view.teardown();
    await view.reply(request, previewReply(request, "late content"));
    assert.equal(nodes.text.textContent, "", action);
  }
});

test("preview marks the byte cap and unsupported encoding without enabling Markdown", async () => {
  for (const reason of ["binary", "non_utf8", "symlink", "submodule"]) {
    const { view, nodes, request } = await workspacePreview();
    await view.reply(request, previewReply(request, "", { unavailable_reason: reason }));
    assert.match(nodes.state.textContent, /preview unavailable/);
    assert.equal(nodes.controls.children[2].disabled, true);
    assert.match(nodes.hint.textContent, /preview unavailable/);
    assert.equal(nodes.retry.hidden, true, "an unsupported file is not a transient read failure");
  }
  const { view, nodes, request } = await workspacePreview();
  const chunk = "x".repeat(32 * 1024);
  for (let index = 0; index < 8; index++) {
    const call = index === 0 ? request : view.calls("get_work_result_state").at(-1);
    await view.reply(call, previewReply(call, chunk, { bytes_total: 256 * 1024 + 1,
      complete: false, limited: index === 7, next_byte_offset: index === 7 ? null : (index + 1) * 32 * 1024 }));
    if (index < 7) { nodes.controls.children[3].onclick(); await flush(); }
  }
  assert.match(nodes.state.textContent, /256 KiB preview limit/);
  assert.equal(nodes.text.textContent.length, 256 * 1024);
  assert.equal(nodes.controls.children[3].hidden, true);
  assert.equal(nodes.controls.children[2].disabled, true);
  assert.match(nodes.hint.textContent, /exceeds the 256 KiB preview limit/);
});

test("preview retry retains content and retries only the failed page of the same snapshot", async () => {
  const { view, nodes, request } = await workspacePreview();
  await view.reply(request, previewReply(request, "first", { bytes_total: 9, complete: false, next_byte_offset: 5 }));
  nodes.controls.children[3].onclick(); await flush();
  const failed = view.calls("get_work_result_state").at(-1);
  await view.reject(failed);
  assert.equal(nodes.text.textContent, "first");
  assert.match(nodes.state.textContent, /5 bytes retained/);
  assert.equal(nodes.retry.hidden, false);
  assert.equal(nodes.retry.disabled, false);
  assert.equal(nodes.controls.children[2].disabled, true);
  assert.match(nodes.hint.textContent, /Retry preview/);
  nodes.retry.onclick(); nodes.retry.onclick(); await flush();
  const retries = view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content");
  assert.equal(retries.length, 3, "rapid retries share one owning request");
  assert.deepEqual(retries.at(-1).params.arguments, failed.params.arguments);
  assert.equal(nodes.retry.disabled, true);
  await view.reply(retries.at(-1), previewReply(retries.at(-1), "tail"));
  assert.equal(nodes.text.textContent, "firsttail");
  assert.equal(nodes.retry.hidden, true);
  assert.equal(nodes.controls.children[2].disabled, false);
  assert.equal(nodes.hint.hidden, true);
});

test("initial preview retry stays pinned and a replaced snapshot cannot receive its late error", async () => {
  const { view, nodes, request } = await workspacePreview();
  await view.reject(request);
  assert.equal(nodes.retry.hidden, false);
  nodes.retry.onclick(); await flush();
  const retried = view.calls("get_work_result_state").at(-1);
  assert.deepEqual(retried.params.arguments, request.params.arguments);
  view.nodes.workspaceReload.onclick();
  await view.reject(retried);
  assert.equal(nodes.retry.hidden, true, "an obsolete retry does not revive its action");
  assert.equal(nodes.text.textContent, "");
});

test("rapid Full text and next-page clicks share their owning request and settle together", async () => {
  const { view, nodes, request } = await workspacePreview();
  nodes.controls.children[1].onclick(); nodes.controls.children[1].onclick();
  await flush();
  const calls = () => view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content");
  assert.equal(calls().length, 1);
  await view.reply(request, previewReply(request, "first", { bytes_total: 9, complete: false, next_byte_offset: 5 }));
  assert.equal(nodes.text.textContent, "first");
  nodes.controls.children[3].onclick(); nodes.controls.children[3].onclick(); await flush();
  assert.equal(calls().length, 2);
  await view.reply(calls()[1], previewReply(calls()[1], "tail"));
  assert.equal(nodes.text.textContent, "firsttail");
  assert.equal(nodes.controls.children[3].disabled, false);
});

test("collapse clears active content DOM; re-expansion joins the original request without stale display", async () => {
  const { view, nodes, request } = await workspacePreview();
  nodes.row.collapse();
  await view.reply(request, previewReply(request, "cached content"));
  assert.equal(nodes.text.textContent, "", "collapsed preview ignores late display");
  nodes.row.children[0].onclick(); await flush();
  assert.equal(nodes.text.textContent, "cached content");
  assert.equal(view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content").length, 1);
  nodes.row.collapse(); assert.equal(nodes.text.textContent, "");
  const pending = await workspacePreview();
  pending.nodes.row.collapse(); pending.nodes.row.children[0].onclick(); await flush();
  await pending.view.reply(pending.request, previewReply(pending.request, "joined content"));
  assert.equal(pending.nodes.text.textContent, "joined content");
  assert.equal(pending.nodes.controls.children[3].disabled, false);
  assert.equal(pending.view.calls("get_work_result_state").filter(call => call.params.arguments.files?.view === "content").length, 1);
});

test("malformed content pages cannot claim complete, skip bytes or exceed UTF-8 bounds", async () => {
  for (const change of [
    { content: "\ud800" }, { byte_offset: 1 }, { bytes_total: 0 },
    { next_byte_offset: 100, complete: false }, { limited: true },
    { content: "x".repeat(32 * 1024 + 1), bytes_total: 32 * 1024 + 1 },
  ]) {
    const { view, nodes, request } = await workspacePreview();
    await view.reply(request, previewReply(request, "safe", change));
    assert.equal(nodes.text.textContent, "");
    assert.match(nodes.state.textContent, /Content unavailable/);
    assert.equal(nodes.controls.children[2].disabled, true);
  }
});

test("activity preserves raw outcomes and distinguishes expected failures and mismatches", async () => {
  for (const [status, classification, label] of [
    ["failed", "matched_expected_failure", "Failed · Expected result"],
    ["failed", "matched_expected_result", "Failed · Expected result"],
    ["success", "unexpected_success", "Succeeded · Expectation not met"],
    ["failed", "expectation_mismatch", "Failed · Expectation not met"],
    ["failed", undefined, "Failed"],
  ]) {
    const view = app("mcp_work_result_app.html");
    view.toolInput(input);
    view.toolResult({ work_result: { ...baseState, window_activity: {
      ...baseState.window_activity, events: [{ ...baseState.window_activity.events[0], status, failure_expectation_result: classification }],
      events_returned: 1, events_observed: 1,
    } } });
    await view.initialize();
    const summary = view.nodes.windowActivity.children[0].children[0];
    assert.equal(summary.children[1].textContent, label);
  }
});
