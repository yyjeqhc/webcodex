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
