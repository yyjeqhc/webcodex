import test from "node:test";
import assert from "node:assert/strict";
import { app, flush, toolResult } from "./app_test_support.mjs";

const project = "agent:special:demo";
const session_id = `wc_sess_${"1".repeat(32)}`;
const input = { project, session_id };
const baseState = {
  version: 1,
  project,
  session_id,
  state_version: `wr1_${"a".repeat(64)}`,
  workspace: {
    git_available: true,
    clean: false,
    branch: "feature/work",
    counts: {
      modified: 1, added: 0, deleted: 0, renamed: 0, copied: 0,
      untracked: 0, conflicted: 0, staged: 0, unstaged: 1,
    },
    files_total: 1,
    files: [{ path: "src/a.rs", status: "modified", kind: "tracked", staged: false, unstaged: true, additions: 4, deletions: 1 }],
    additions: 4,
    deletions: 1,
    line_stats_partial: false,
    truncated: false,
  },
  validation: {
    status: "passed", latest_status: "passed", current_status: "passed", history_partial: false,
    successes: 3, failures: 0, unresolved_failures: 0, evidence_gaps: 0,
  },
  review: {
    available: true, total: 1, history_partial: false, read_only_inspection_count: 0, search_count: 0,
    diff_review_count: 1, workspace_review_count: 1, hygiene_review_count: 0,
    tools: ["show_changes"],
  },
  session: {
    lifecycle: "active", events_total: 7, events_returned: 7, history_partial: false,
    updated_at: 1789812000, title: "Work Result test",
    latest_activity: {
      tool: "show_changes", kind: "tool_call_finished", timestamp: 1789812000,
      status: "completed", duration_ms: 42,
    },
  },
};

const nextState = {
  ...baseState,
  state_version: `wr1_${"b".repeat(64)}`,
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
};

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
    assert.equal(view.nodes.changesTitle.textContent, "Changed 1 file");
    assert.equal(view.nodes.validationStatus.textContent, "Passed");
    assert.equal(view.nodes.reviewStatus.textContent, "Workspace reviewed · Diff inspected");
    assert.equal(view.nodes.refresh.disabled, false);
  });
}

test("input-only bootstrap waits for a user Refresh before reading state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  await view.initialize();
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /Waiting for Work snapshot/);
  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, input);
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: baseState }));
  assert.equal(view.nodes.changesTitle.textContent, "Changed 1 file");
  assert.equal(view.nodes.status.textContent, "Updated");
});

test("matching Work input/result identity is idempotent and unchanged initial state avoids DOM rebuild", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);
  view.nodes.files.textContent = "sentinel";
  view.toolResult({ work_result: baseState });
  await flush();
  assert.equal(view.calls("work_result_state").length, 0);
  assert.equal(view.nodes.files.textContent, "sentinel");
});

test("live progress performs bounded app-only polling and adapts to visibility", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.calls("work_result_state").length, 0);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 1);
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, input);
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.progressStatus.textContent, "cargo_test · completed");
  assert.match(view.nodes.progressMeta.textContent, /9 session events · live/);
  await view.fireTimers(2500);
  assert.equal(view.calls("work_result_state").length, 2);
  await view.reply(view.calls("work_result_state")[1], toolResult({ work_result: nextState }));
  await view.visibility(true);
  assert.equal([...view.timers.values()].some(timer => timer.delay === 12000), true);
  await view.visibility(false);
  assert.equal([...view.timers.values()].some(timer => timer.delay === 250), true);
  assert.equal(view.timers.size, 1);
});

test("closed Session stops automatic polling but remains manually refreshable", async () => {
  const closedState = {
    ...baseState,
    state_version: `wr1_${"c".repeat(64)}`,
    session: { ...baseState.session, lifecycle: "closed" },
  };
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: closedState });
  await view.initialize();
  assert.equal(view.timers.size, 0);
  assert.match(view.nodes.status.textContent, /Closed/);
  await view.fireTimers(12000);
  assert.equal(view.calls("work_result_state").length, 0);
  await view.visibility(false);
  assert.equal(view.timers.size, 0);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 1);
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: closedState }));
  assert.equal(view.timers.size, 0);
  assert.equal(view.nodes.refresh.disabled, false);
  assert.match(view.nodes.status.textContent, /closed/i);
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
  assert.match(view.nodes.status.textContent, /auto refresh paused/);
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
  assert.deepEqual({ ...view.calls("work_result_state")[0].params.arguments }, input);
  assert.equal(view.nodes.refresh.disabled, true);
  assert.equal(view.nodes.refresh.textContent, "Refreshing…");
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: nextState }));
  assert.equal(view.nodes.changesTitle.textContent, "Workspace clean");
  assert.equal(view.nodes.validationStatus.textContent, "Failed");
  assert.match(view.nodes.reviewStatus.textContent, /Committed range mapped/);
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
  assert.deepEqual({ ...view.calls("work_result_state")[1].params.arguments }, input);
});

test("failed Refresh preserves the last valid snapshot and remains retryable", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: baseState });
  await view.initialize();
  view.toolInput(input);

  view.nodes.refresh.onclick();
  await flush();
  await view.reply(view.calls("work_result_state")[0], { structuredContent: { success: false, output: { error_kind: "workspace_unavailable" } } });
  assert.equal(view.nodes.changesTitle.textContent, "Changed 1 file");
  assert.match(view.nodes.status.textContent, /Refresh unavailable/);
  assert.equal(view.nodes.refresh.disabled, false);

  view.nodes.refresh.onclick();
  await flush();
  assert.equal(view.calls("work_result_state").length, 2);
  await view.reject(view.calls("work_result_state")[1]);
  assert.equal(view.nodes.changesTitle.textContent, "Changed 1 file");
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
    assert.equal(view.nodes.status.textContent, "Invalid or conflicting Work identity");
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
  assert.equal(view.nodes.status.textContent, "Invalid authoritative Work state");
  assert.equal(view.nodes.refresh.disabled, true);
});

test("partial evidence stays honest and exact files_total is not decorated as a lower bound", async () => {
  const files = Array.from({ length: 8 }, (_, index) => ({
    path: index === 0 ? "src/future.rs" : `src/file_${index}.rs`,
    status: index === 0 ? "future_status" : "modified",
    kind: "tracked",
    staged: false,
    unstaged: true,
  }));
  const partialState = {
    ...baseState,
    state_version: `wr1_${"c".repeat(64)}`,
    workspace: { ...baseState.workspace, files_total: 14, files, truncated: true, additions: undefined, deletions: undefined, line_stats_partial: true },
    validation: { ...baseState.validation, status: "unknown", latest_status: "unknown", current_status: "unknown", history_partial: true, successes: 0 },
    review: { ...baseState.review, available: false, total: 0, history_partial: true, tools: [] },
  };
  const view = app("mcp_work_result_app.html");
  view.toolResult({ work_result: partialState });
  await view.initialize();
  assert.equal(view.nodes.changesTitle.textContent, "Changed 14 files");
  assert.match(view.nodes.files.textContent, /^\? src\/future\.rs/m);
  assert.equal(view.nodes.validationStatus.textContent, "Unknown");
  assert.match(view.nodes.validationMeta.textContent, /history partial/);
  assert.equal(view.nodes.reviewStatus.textContent, "Review history partial");
  assert.match(view.nodes.reviewMeta.textContent, /Earlier review evidence/);
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
    assert.equal(view.nodes.changesTitle.textContent, "Changed 1 file");
  });
}

test("invalid Work input never refreshes", async () => {
  for (const bad of [
    { project: "", session_id },
    { project, session_id: "wc_sess_bad!" },
    { project: [project], session_id },
  ]) {
    const view = app("mcp_work_result_app.html");
    view.toolInput(bad);
    await flush();
    assert.equal(view.calls("work_result_state").length, 0);
    assert.equal(view.nodes.status.textContent, "Invalid or conflicting Work identity");
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
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff());
  assert.equal(nodes.state.textContent, "Frozen diff");
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
  assert.equal(view.nodes.changesTitle.textContent, "Workspace clean");
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.equal(frozenNodes(view).root, nodes.root);
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff());
  assert.equal(nodes.state.textContent, "Frozen diff");
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
  assert.equal(view.nodes.changesTitle.textContent, "Workspace clean");
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
});

test("live progress card adopts the first sealed final snapshot from a later state refresh", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input);
  view.toolResult({ work_result: baseState });
  await view.initialize();
  assert.equal(view.nodes.finalChanges, undefined);
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[0], toolResult({ work_result: frozenWork() }));
  assert.equal(view.nodes.finalChanges.hidden, false);
  assert.equal(view.nodes.frozenSummary.textContent, "Changed 7 files");
  assert.match(view.nodes.status.textContent, /Final changes sealed/);
  const root = frozenNodes(view).root;
  view.nodes.refresh.onclick(); await flush();
  await view.reply(view.calls("work_result_state")[1], toolResult({ work_result: frozenWork() }));
  assert.equal(frozenNodes(view).root, root);
  assert.equal(view.nodes.refresh.disabled, false);
});

test("metadata and lazy diff truncation remain truthful within bounded initial rows", async () => {
  const view = await frozenView("result", frozenWork({ files_changed: 30, files_total: 30, files_truncated: true }));
  assert.match(view.nodes.frozenFooter.textContent, /metadata truncated \(7\/30 files advertised\)/);
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff({ truncated: true, bytes_total: 50000, lines_total: 2000 }));
  assert.match(frozenNodes(view).state.textContent, /truncated \(\d+\/50000 bytes, \d+\/2000 lines\)/);
});

test("renamed and binary files retain their metadata in lazy frozen responses", async () => {
  const view = await frozenView();
  frozenNodes(view, 3).button.onclick(); await flush();
  assert.equal(view.calls("changes_file_diff")[0].params.arguments.path, "src/new_name.rs");
  await view.reply(view.calls("changes_file_diff")[0], frozenDiff({ path: "src/new_name.rs", previous_path: "src/old_name.rs", kind: "renamed" }));
  assert.equal(frozenNodes(view, 3).state.textContent, "Frozen diff");
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
    assert.match(view.nodes.status.textContent, /Invalid frozen diff/);
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
    assert.match(view.nodes.status.textContent, /Invalid Work Result/);
  });
}

test("expired/unavailable snapshots never fall back to live diff or automatically retry", async () => {
  const view = await frozenView();
  frozenNodes(view).button.onclick(); await flush();
  await view.reply(view.calls("changes_file_diff")[0], { structuredContent: { success: false, output: { error_kind: "changes_snapshot_unavailable" } } });
  assert.match(frozenNodes(view).state.textContent, /snapshot may have expired/);
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
  assert.match(view.nodes.status.textContent, /Conflicting sealed Work identity/);
  assert.equal(view.nodes.finalChanges.hidden, true);
});

test("cached legacy Changes resource payload is not promoted into authoritative Work Result state", async () => {
  const view = app("mcp_work_result_app.html");
  view.toolInput(input); await view.initialize();
  view.toolResult({ changes: { version: 3, project, session_id, ...finalChanges } });
  assert.match(view.nodes.status.textContent, /Invalid Work Result state/);
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
