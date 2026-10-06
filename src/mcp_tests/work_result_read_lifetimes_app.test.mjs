import test from "node:test";
import assert from "node:assert/strict";
import { app, flush, toolResult } from "./app_test_support.mjs";
import { project, session_id, baseState } from "./work_result_app_fixture.mjs";

const detail = {
  server_trace_id: "trace-reviewed", started_at_ms: 1_999_999_989_000,
  ended_at_ms: 1_999_999_990_000, duration_ms: 1000, method: "tools/call",
  tool_name: "read_workspace_changes", project, status: "success", meaningful: true,
  observed_job_ids: [], workflow_sessions: [{ workflow_session_id: session_id, project, relation: "recorded" }],
};
function activityRow(view) {
  return view.nodes.windowActivity.children.find(row => row.children[0]?.children[0]?.children[0]?.textContent === "read_workspace_changes");
}
async function pendingActivity() {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  const row = activityRow(view); row.open = true; row.ontoggle(); await flush();
  return { view, row, request: view.calls("read_work_result_activity_detail")[0] };
}
async function refreshActivity(view) {
  view.nodes.refresh.onclick();
  await view.reply(view.calls("get_work_result_state").at(-1), toolResult({
    work_result: { ...baseState, state_version: `wr2_${"b".repeat(64)}` },
  }));
  return activityRow(view);
}

test("activity refresh joins the existing detail request and updates only its current row", async () => {
  const { view, row, request } = await pendingActivity();
  const current = await refreshActivity(view);
  assert.notEqual(current, row);
  assert.equal(current.open, true);
  assert.equal(view.calls("read_work_result_activity_detail").length, 1);
  await view.reply(request, toolResult({ activity_detail: detail }));
  assert.equal(current.children[1].children[0]?.className, "detail-grid");
  assert.equal(row.children[1].children.length, 0, "detached row must not receive late DOM");
});

test("collapsed activity ignores late rendering but reopening uses the completed read", async () => {
  const { view, row, request } = await pendingActivity();
  row.open = false; row.ontoggle();
  await view.reply(request, toolResult({ activity_detail: detail }));
  assert.equal(row.children[1].children.length, 0);
  row.open = true; row.ontoggle(); await flush();
  assert.equal(row.children[1].children[0]?.className, "detail-grid");
  assert.equal(view.calls("read_work_result_activity_detail").length, 1);
});

test("a shared activity failure reaches the refreshed row without retrying automatically", async () => {
  const { view, row, request } = await pendingActivity();
  const current = await refreshActivity(view);
  await view.reject(request);
  assert.equal(current.children[1].textContent, "Details unavailable.");
  assert.notEqual(row.children[1].textContent, "Details unavailable.");
  assert.equal(view.calls("read_work_result_activity_detail").length, 1);
  current.open = false; current.ontoggle(); current.open = true; current.ontoggle(); await flush();
  assert.equal(view.calls("read_work_result_activity_detail").length, 2);
});

const snapshot_id = `wc_changes_snapshot_${"2".repeat(32)}`;
async function pendingDiff(final) {
  const state = structuredClone(baseState), path = "src/a.rs";
  if (final) state.final_changes = {
    snapshot_id, files_changed: 1, additions: 1, deletions: 1,
    files_total: 1, files_returned: 1, files_truncated: false,
    files: [{ path, kind: "modified", additions: 1, deletions: 1, binary: false }],
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: state }); await view.initialize();
  const row = view.nodes[final ? "frozenFiles" : "workspaceFiles"].children[0];
  row.children[0].onclick(); await flush();
  if (!final) await view.reply(view.calls("get_work_result_state")[0], toolResult({ work_result_files: {
    project, session_id: null, snapshot_id, offset: 0, next_offset: null,
    files_total: 1, source_truncated: false, files: [{ path, kind: "modified", additions: 1, deletions: 1, binary: false }],
  } }));
  const name = final ? "read_changed_file_diff" : "get_work_result_state";
  const request = view.calls(name).at(-1), count = view.calls(name).length;
  const diff = "+late\n";
  const response = toolResult(final ? { changes_file_diff: {
    version: 1, project, session_id, snapshot_id, path, previous_path: null,
    kind: "modified", binary: false, diff, bytes_total: 6, bytes_returned: 6,
    lines_total: 1, lines_returned: 1, truncated: false,
  } } : { work_result_files: { project, session_id: null, snapshot_id, path, diff, truncated: false } });
  return { view, row, request, response, name, count, pre: row.children[1].children[1] };
}

for (const final of [false, true]) test(`${final ? "final" : "workspace"} diff ignores collapsed DOM then reuses the completed exact read`, async () => {
  const { view, row, request, response, name, count, pre } = await pendingDiff(final);
  row.collapse(); await view.reply(request, response);
  assert.equal(pre.children.length, 0);
  row.expand(); await flush();
  assert.equal(pre.children[0]?.textContent, "+late");
  assert.equal(view.calls(name).length, count, "reopening must not rerun the read");
});

for (const final of [false, true]) test(`${final ? "final" : "workspace"} diff teardown fences pending writes and prevents new reads`, async () => {
  const { view, row, request, response, name, count, pre } = await pendingDiff(final);
  await view.teardown(); await view.reply(request, response);
  assert.equal(pre.children.length, 0);
  row.collapse(); row.expand(); await flush();
  assert.equal(view.calls(name).length, count);
});

const files = view => view.calls("get_work_result_state").filter(call => call.params.arguments.files);
const inventory = (id = snapshot_id) => toolResult({ work_result_files: {
  project, session_id: null, snapshot_id: id, offset: 0, next_offset: null,
  files_total: 1, source_truncated: false,
  files: [{ path: "src/a.rs", kind: "modified", additions: 1, deletions: 1, binary: false }],
} });
async function observeWorkspace(view, { reused = false, workspace = baseState.workspace } = {}) {
  view.nodes.refresh.onclick(); await flush();
  const request = view.calls("get_work_result_state").filter(call => !call.params.arguments.files).at(-1);
  await view.reply(request, toolResult({ work_result: {
    ...baseState, workspace, workspace_observation: { reused, max_reuse_ms: 30000, semantics: "bounded_snapshot_not_filesystem_freshness" },
  } }));
}
const newerSnapshot = `wc_changes_snapshot_${"3".repeat(32)}`;

test("following fresh observations updates equal-count diffs and fences the older pending read", async () => {
  const { view, row, request, response, pre } = await pendingDiff(false);
  assert.equal(view.nodes.workspacePin.disabled, false);
  assert.equal(view.nodes.workspacePin.getAttribute("aria-pressed"), "false");
  await observeWorkspace(view, { reused: true });
  assert.equal(files(view).length, 2, "reused metadata cannot trigger another Git snapshot");
  await observeWorkspace(view);
  const capture = files(view).at(-1);
  assert.equal(capture.params.arguments.files.snapshot_id, undefined);
  assert.equal(view.nodes.workspaceFiles.children[0], row, "keep the previous view until capture succeeds");
  await view.reply(capture, inventory(newerSnapshot));
  const replacement = view.nodes.workspaceFiles.children[0];
  assert.notEqual(replacement, row);
  assert.equal(replacement.children[0].getAttribute("aria-expanded"), "true");
  const diff = files(view).at(-1);
  assert.equal(diff.params.arguments.files.snapshot_id, newerSnapshot);
  await view.reply(request, response);
  assert.equal(pre.children.length, 0, "old completion cannot populate the new view");
  await view.reply(diff, toolResult({ work_result_files: {
    project, session_id: null, snapshot_id: newerSnapshot, path: "src/a.rs", diff: "+current\n", truncated: false,
  } }));
  assert.equal(replacement.children[1].children[1].children[0].textContent, "+current");
});

test("an unchanged exact snapshot preserves pending diffs and repeated observations share one capture", async () => {
  const { view, row, request, response, pre } = await pendingDiff(false);
  await observeWorkspace(view);
  const capture = files(view).at(-1), count = files(view).length;
  await observeWorkspace(view);
  assert.equal(files(view).length, count, "only one renewal may run at once");
  await view.reply(capture, inventory());
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  await view.reply(request, response);
  assert.equal(pre.children[0].textContent, "+late");
  assert.equal(files(view).length, count, "the same code state needs no new diff read");
});

test("explicit pin retains its code and pending diff even when current workspace becomes clean", async () => {
  const { view, row, request, response, pre } = await pendingDiff(false);
  await view.nodes.workspacePin.onclick();
  const count = files(view).length;
  await observeWorkspace(view, { workspace: { ...baseState.workspace, clean: true, files: [], files_total: 0, additions: 0, deletions: 0 } });
  assert.equal(view.nodes.workspacePin.getAttribute("aria-pressed"), "true");
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  assert.equal(view.nodes.workspaceStatus.textContent, "No uncommitted changes");
  assert.match(view.nodes.workspaceTracking.textContent, /Pinned for review/);
  assert.equal(files(view).length, count);
  await view.reply(request, response);
  assert.equal(pre.children[0].textContent, "+late");
});

for (const failed of [false, true]) test(`pinning while a follow capture is pending fences its late ${failed ? "failure" : "success"} and permits later following`, async () => {
  const { view, row } = await pendingDiff(false);
  await observeWorkspace(view);
  const oldCapture = files(view).at(-1);
  await view.nodes.workspacePin.onclick();
  await view.nodes.workspacePin.onclick(); await flush();
  const newCapture = files(view).at(-1);
  assert.notEqual(newCapture, oldCapture);
  await view.reply(oldCapture, failed ? { structuredContent: { success: false, output: {} } } : inventory(newerSnapshot));
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  assert.doesNotMatch(view.nodes.workspaceMeta.textContent, /unavailable/);
  await view.reply(newCapture, inventory(newerSnapshot));
  assert.notEqual(view.nodes.workspaceFiles.children[0], row);
});

test("failed renewal retains the previous view and teardown ignores late captures", async () => {
  const { view, row } = await pendingDiff(false);
  await observeWorkspace(view);
  await view.reply(files(view).at(-1), { structuredContent: { success: false, output: {} } });
  assert.equal(view.nodes.workspaceFiles.children[0], row);
  assert.match(view.nodes.workspaceMeta.textContent, /retaining the previous file snapshot/);
  await observeWorkspace(view);
  await view.reply(files(view).at(-1), inventory());
  assert.doesNotMatch(view.nodes.workspaceMeta.textContent, /unavailable/, "successful unchanged capture clears a previous failure");
  await observeWorkspace(view);
  const capture = files(view).at(-1), count = files(view).length;
  await view.teardown(); await view.reply(capture, inventory(newerSnapshot));
  await view.nodes.workspacePin.onclick();
  assert.equal(files(view).length, count);
});

test("pin before the first diff captures code without being retargeted by a newer status", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  assert.equal(files(view).length, 0, "early presentation stays lightweight");
  const pin = view.nodes.workspacePin.onclick(); await flush();
  const capture = files(view)[0];
  await observeWorkspace(view, { workspace: { ...baseState.workspace, files_total: 2 } });
  await view.reply(capture, inventory()); await pin;
  assert.equal(view.nodes.workspaceFiles.children.length, 1);
  assert.equal(view.nodes.workspaceFiles.children[0].filePath, "src/a.rs");
  assert.equal(view.nodes.workspacePin.getAttribute("aria-pressed"), "true");
  assert.equal(files(view).length, 1);
});

test("failed pin before the first diff remains in Follow mode", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState }); await view.initialize();
  const pin = view.nodes.workspacePin.onclick(); await flush();
  const capture = files(view)[0];
  assert.equal(view.nodes.workspacePin.getAttribute("aria-pressed"), "false");
  assert.equal(view.nodes.workspacePin.textContent, "Pin snapshot for review");
  assert.match(view.nodes.workspaceTracking.textContent, /Following changes/);
  await view.reply(capture, { structuredContent: { success: false, output: {} } }); await pin;
  assert.equal(view.nodes.workspacePin.getAttribute("aria-pressed"), "false");
  assert.equal(view.nodes.workspacePin.textContent, "Pin snapshot for review");
  assert.match(view.nodes.workspaceTracking.textContent, /Following changes/);
  assert.match(view.nodes.workspaceMeta.textContent, /still following changes/);
});

test("following working changes leaves a sealed Full text preview and its pending read intact", async () => {
  const { view, row: finalRow } = await pendingDiff(true);
  const controls = finalRow.children[1].children[2];
  controls.children.find(button => button.textContent === "Full text").onclick(); await flush();
  const textRead = files(view).at(-1), args = textRead.params.arguments;
  assert.equal(args.session_id, session_id);
  const liveRow = view.nodes.workspaceFiles.children[0];
  liveRow.expand(); await flush();
  await view.reply(files(view).at(-1), inventory());
  await observeWorkspace(view);
  await view.reply(files(view).at(-1), inventory(newerSnapshot));
  assert.equal(view.nodes.frozenFiles.children[0], finalRow);
  await view.reply(textRead, toolResult({ work_result_files: {
    project, session_id, snapshot_id: args.files.snapshot_id, path: "src/a.rs", view: "content",
    byte_offset: 0, bytes_total: 6, content: "sealed", complete: true, limited: false, next_byte_offset: null,
  } }));
  assert.equal(finalRow.children[1].children[3].children[1].textContent, "sealed");
  const reads = files(view).filter(call => call.params.arguments.files.view === "content").length;
  controls.children.find(button => button.textContent === "Full text").onclick(); await flush();
  assert.equal(files(view).filter(call => call.params.arguments.files.view === "content").length, reads);
});
