import type {
  RecentSession,
  RuntimeOverview,
  SessionDetail,
  SessionListItem,
  WindowDetail,
} from "../src/runtime-v2/model/types.js";

export function sessionItem(overrides: Partial<SessionListItem> = {}): SessionListItem {
  return {
    session_id: "wc_sess_1234567890abcdef",
    title: "Runtime E2E private-path hardening",
    lifecycle: "active",
    mode: "normal",
    updated_at: 1_790_000_000,
    running_call: false,
    running_jobs: 0,
    running_jobs_complete: true,
    current_activity: undefined,
    last_activity: {
      kind: "review",
      tool: "show_changes",
      state: "success",
      execution_state: "completed",
      job_handoff: false,
      summary: "Reviewed current diff",
      paths: [],
    },
    overview: {
      work: {
        exploration: 12,
        edits: 5,
        reviews: 1,
        validations: 2,
        runs: 3,
        history_complete: true,
        history_truncated: false,
      },
      validation: {
        state: "pass",
        latest_kind: "test",
        latest_at: 1_790_000_000,
        unresolved_failure_count: 0,
        tests_run_count: 18,
        history_complete: true,
        history_truncated: false,
      },
      attention: {
        open_guidance: 0,
        open_questions: 0,
        open_risks: 0,
        open_todos: 0,
      },
      reported_progress: {
        reported_at: 1_790_000_000,
        text: "Implementation is stable; final review remains.",
      },
    },
    ...overrides,
  };
}

export function recentSession(overrides: Partial<RecentSession> = {}): RecentSession {
  return {
    ...sessionItem(),
    client_id: "special",
    project_id: "agent:special:webcodex",
    project_name: "WebCodex",
    ...overrides,
  };
}

export function sessionDetail(overrides: Partial<SessionDetail> = {}): SessionDetail {
  return {
    ...sessionItem(),
    created_at: 1_789_999_000,
    activity: [
      {
        kind: "exploration",
        tool: "read_files",
        state: "success",
        execution_state: "completed",
        job_handoff: false,
        started_at: 1_789_999_100,
        finished_at: 1_789_999_101,
        duration_ms: 1000,
        summary: "Read reconnect implementation",
        paths: ["src/runtime.rs"],
        group_kinds: [],
        group_tools: [],
      },
      {
        kind: "edit",
        tool: "apply_text_edits",
        state: "success",
        execution_state: "completed",
        job_handoff: false,
        started_at: 1_789_999_200,
        finished_at: 1_789_999_201,
        duration_ms: 1000,
        summary: "Patched reconnect implementation",
        paths: ["src/runtime.rs"],
        group_kinds: [],
        group_tools: [],
      },
    ],
    activity_total: 2,
    activity_returned: 2,
    activity_truncated: false,
    window_activity_available: true,
    linked_windows: [],
    window_activity_after_last_session_record: [],
    ...overrides,
  };
}

export function runtimeOverview(overrides: Partial<RuntimeOverview> = {}): RuntimeOverview {
  const recent = recentSession();
  return {
    service: "WebCodex",
    version: "0.4.0",
    build_git_commit: "0123456789abcdef0123456789abcdef01234567",
    build_git_dirty: false,
    runner_count: 1,
    runners_online: 1,
    runners_stale: 0,
    runners_unavailable: 0,
    source_mismatched_runners: 0,
    mixed_builds_present: false,
    active_jobs: 0,
    projects_available: true,
    visible_projects: 1,
    projects_truncated: false,
    workflow_sessions: {
      active: 1,
      running: 0,
      open_guidance: 0,
      open_questions: 0,
      open_risks: 0,
      open_todos: 0,
      projects_scanned: 1,
      projects_total: 1,
      truncated: false,
    },
    recent_sessions: {
      sessions: [recent],
      returned: 1,
      candidate_count: 1,
      truncated: false,
      scan_truncated: false,
    },
    runners: [{
      client_id: "special",
      connected: true,
      status: "online",
      source_alignment: "aligned",
      active_jobs: 0,
      jobs_running: 0,
      jobs_queued: 0,
      projects_scanned: 1,
      projects_scan_partial: false,
      sessions: {
        retained_sessions: 1,
        returned_sessions: 1,
        sessions_truncated: false,
        active_sessions: 1,
        running_sessions: 0,
        attention: {
          open_guidance: 0,
          open_questions: 0,
          open_risks: 0,
          open_todos: 0,
        },
      },
    }],
    projects: [{
      id: "agent:special:webcodex",
      client_id: "special",
      project_ref: "~p1",
      name: "WebCodex",
      path: "/root/git/webcodex",
      connected: true,
      agent_status: "online",
      sessions: {
        retained_sessions: 1,
        returned_sessions: 1,
        sessions_truncated: false,
        active_sessions: 1,
        running_sessions: 0,
        latest_updated_at: 1_790_000_000,
        attention: {
          open_guidance: 0,
          open_questions: 0,
          open_risks: 0,
          open_todos: 0,
        },
      },
    }],
    ...overrides,
  };
}

export function windowDetail(overrides: Partial<WindowDetail> = {}): WindowDetail {
  return {
    client_window_key: "a".repeat(64),
    source: "openai-session",
    last_seen_at_ms: 1_790_000_000_000,
    last_meaningful_activity_at_ms: 1_790_000_000_000,
    active_count: 0,
    active_requests: [],
    linked_sessions: [],
    sessions_returned: 0,
    sessions_truncated: false,
    activity: [],
    activity_returned: 0,
    activity_truncated: false,
    visibility: { scope: "principal" },
    ...overrides,
  };
}
