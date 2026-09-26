export const project = "agent:special:demo";
export const session_id = `wc_sess_${"1".repeat(32)}`;
export const input = { project, session_id };
export const baseState = {
  version: 2,
  project,
  session_id,
  state_version: `wr2_${"a".repeat(64)}`,
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
  activity: {
    available: true, scope: "window", active: false, current: null,
    last: { label: "Reviewed changes", kind: "review", at_ms: 1_999_999_990_000 },
    last_meaningful_activity_at_ms: 1_999_999_990_000, coverage_partial: false,
  },
  window_activity: {
    available: true, active: false, active_requests: [],
    events_returned: 2, events_observed: 2, truncated: false,
    last_activity_at_ms: 1_999_999_990_000,
    events: [
      { label: "Reviewed changes", kind: "review", status: "success", meaningful: true, started_at_ms: 1_999_999_989_000, ended_at_ms: 1_999_999_990_000, duration_ms: 1000 },
      { label: "Observed Runtime status", kind: null, status: "success", meaningful: false, started_at_ms: 1_999_999_980_000, ended_at_ms: 1_999_999_980_100, duration_ms: 100 },
    ],
  },
  collaboration: { available: true, can_send: true, messages: [] },
};
