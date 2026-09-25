export type Availability = "idle" | "loading" | "available" | "stale" | "denied" | "error";

export type WorkCounts = {
  exploration: number;
  edits: number;
  reviews: number;
  validations: number;
  runs: number;
  history_complete: boolean;
  history_truncated: boolean;
};

export type ValidationOverview = {
  state: string;
  latest_kind?: string;
  latest_at?: number;
  unresolved_failure_count: number;
  tests_run_count?: number;
  history_complete: boolean;
  history_truncated: boolean;
};

export type AttentionOverview = {
  open_guidance: number;
  open_questions: number;
  open_risks: number;
  open_todos: number;
};

export type ReportedProgress = {
  reported_at: number;
  text: string;
};

export type SessionOverview = {
  work: WorkCounts;
  validation: ValidationOverview;
  attention: AttentionOverview;
  reported_progress?: ReportedProgress;
};

export type ActivityPreview = {
  kind: string;
  tool?: string;
  state: string;
  execution_state?: string;
  job_handoff: boolean;
  job_id?: string;
  summary?: string;
  paths?: string[];
};

export type SessionListItem = {
  session_id: string;
  title: string;
  lifecycle: string;
  mode: string;
  updated_at: number;
  running_call: boolean;
  running_jobs: number;
  running_jobs_complete: boolean;
  current_activity?: ActivityPreview;
  last_activity?: ActivityPreview;
  overview: SessionOverview;
};

export type SessionActivity = {
  kind: string;
  tool?: string;
  state: string;
  execution_state?: string;
  job_handoff: boolean;
  started_at: number;
  finished_at?: number;
  duration_ms?: number;
  exit_code?: number;
  job_id?: string;
  summary?: string;
  paths?: string[];
  group_count?: number;
  group_kinds?: string[];
  group_tools?: string[];
};

export type SessionWindow = {
  client_window_key: string;
  source: string;
  first_linked_at_ms: number;
  last_linked_at_ms: number;
  last_seen_at_ms: number;
  last_meaningful_activity_at_ms?: number;
  active_count?: number;
  relations: string[];
  relation_count: number;
  recorder_gap_count: number;
};

export type WorkspaceActivityPreview = {
  created_at: number;
  tool: string;
  success: boolean;
  client_id?: string;
  session_id?: string;
};

export type SessionJobActivity = {
  job_id: string;
  kind: string;
  status: string;
  terminal: boolean;
  created_at: number;
  started_at?: number;
  ended_at?: number;
  activity_state?: string;
  activity_phase?: string;
};

export type SessionDetail = SessionListItem & {
  created_at: number;
  activity: SessionActivity[];
  activity_total: number;
  activity_returned: number;
  activity_truncated: boolean;
  window_activity_available: boolean;
  linked_windows: SessionWindow[];
  window_activity_after_last_session_record: WindowActivity[];
  window_activity_after_last_session_record_truncated?: boolean;
  workspace_activity_available?: boolean;
  workspace_last_activity?: WorkspaceActivityPreview;
  job_activity_available?: boolean;
  jobs?: SessionJobActivity[];
  jobs_truncated?: boolean;
};

export type RecentSession = SessionListItem & {
  client_id: string;
  project_id: string;
  project_name?: string;
};

export type RecentSessions = {
  sessions: RecentSession[];
  returned: number;
  candidate_count: number;
  truncated: boolean;
  scan_truncated: boolean;
};

export type SessionAggregate = {
  retained_sessions: number;
  returned_sessions: number;
  sessions_truncated: boolean;
  active_sessions: number;
  running_sessions: number;
  latest_updated_at?: number;
  attention: AttentionOverview;
};

export type ProjectRow = {
  id: string;
  client_id: string;
  project_ref?: string;
  name?: string;
  path?: string;
  registration_source?: string;
  lineage?: {
    kind: "managed_worktree_source";
    source_project_id: string;
    base_sha: string;
  };
  connected: boolean;
  agent_status?: string;
  sessions?: SessionAggregate;
};

export type ProjectsResponse = {
  projects: ProjectRow[];
  total: number;
  truncated: boolean;
};

export type ProjectGit = {
  branch?: string | null;
  clean?: boolean;
  git_available?: boolean;
  non_git_project?: boolean;
  files?: unknown[];
  files_total?: number;
  files_truncated?: boolean;
};

export type RunnerSummary = {
  protocol_compatibility?: "compatible" | "incompatible" | "unknown";
  build_alignment?: "exact" | "different_version" | "different_commit" | "dirty" | "unknown";
  client_id: string;
  connected: boolean;
  status?: string;
  transport?: string;
  runner_protocol_generation?: number;
  last_seen_age_secs?: number;
  version?: string;
  build_git_commit?: string;
  build_git_dirty?: boolean;
  source_alignment?: string;
  version_matches_server?: boolean;
  active_jobs: number;
  job_concurrency_limit?: number;
  jobs_running: number;
  jobs_queued: number;
  projects_scanned: number;
  projects_scan_partial: boolean;
  sessions: SessionAggregate;
};

export type RuntimeOverview = {
  service?: string;
  version?: string;
  build_git_commit?: string;
  build_git_dirty?: boolean;
  runner_count: number;
  runners_online: number;
  runners_stale: number;
  runners_unavailable: number;
  source_mismatched_runners: number;
  mixed_builds_present: boolean;
  active_jobs: number;
  active_windows: number;
  projects_available: boolean;
  visible_projects: number;
  projects_truncated: boolean;
  workflow_sessions: {
    active: number;
    running: number;
    open_guidance: number;
    open_questions: number;
    open_risks: number;
    open_todos: number;
    projects_scanned: number;
    projects_total: number;
    truncated: boolean;
  };
  recent_sessions: RecentSessions;
  runners: RunnerSummary[];
  projects: ProjectRow[];
};

export type WindowSummary = {
  client_window_key: string;
  last_project?: string;
  source: string;
  last_seen_at_ms: number;
  last_tool_call_at_ms?: number;
  last_meaningful_activity_at_ms?: number;
  last_activity_name?: string;
  last_activity_status?: string;
  last_activity_meaningful?: boolean;
  active_count: number;
  linked_session_count: number;
  recorder_gap_count: number;
};

export type WindowActivitySession = {
  workflow_session_id: string;
  project?: string;
  relation: string;
};

export type WindowActivity = {
  started_at_ms: number;
  ended_at_ms: number;
  duration_ms: number;
  service_ms?: number;
  next_call_gap_ms?: number;
  cycle_ms?: number;
  window_transition_kind?: string;
  response_streaming?: boolean;
  method: string;
  tool_name?: string;
  activity_presentation?: string;
  activity_kind?: string;
  project?: string;
  status: string;
  meaningful: boolean;
  async_job_id?: string;
  observed_job_ids?: string[];
  recorder_gap_session_id?: string;
  server_trace_id?: string;
  workflow_sessions: WindowActivitySession[];
};

export type WindowLinkedSession = {
  workflow_session_id: string;
  project?: string;
  first_linked_at_ms: number;
  last_linked_at_ms: number;
  relations: string[];
  relation_count: number;
  title?: string;
  lifecycle?: string;
};

export type WindowJob = {
  job_id: string;
  status: string;
  active: boolean;
  terminal: boolean;
  started_at?: number;
  ended_at?: number;
  duration_ms?: number;
  elapsed_secs?: number;
};

export type WindowDetail = {
  client_window_key: string;
  source: string;
  last_seen_at_ms: number;
  last_tool_call_at_ms?: number;
  last_meaningful_activity_at_ms?: number;
  active_count: number;
  active_requests: Array<{
    server_trace_id: string;
    method: string;
    tool_name?: string;
    project?: string;
    started_at_ms: number;
    elapsed_ms: number;
  }>;
  linked_sessions: WindowLinkedSession[];
  sessions_returned: number;
  sessions_truncated: boolean;
  activity: WindowActivity[];
  activity_returned: number;
  activity_truncated: boolean;
  jobs?: WindowJob[];
  jobs_truncated?: boolean;
  visibility: { scope: "global" | "principal" };
};

export type WindowsResponse = {
  windows: WindowSummary[];
  returned: number;
  total: number;
  truncated: boolean;
  visibility: { scope: "global" | "principal" };
};

export type SessionMessage = {
  message_id: string;
  kind: string;
  status: string;
  priority: string;
  created_at: number;
  message: string;
  requires_ack: boolean;
  first_ack_observed_at?: number;
  author_session_id?: string;
  reply_to?: string;
  resolved_at?: number;
  resolution?: string;
  closure_kind?: string;
  superseded_by_message_id?: string;
  supersedes_message_id?: string;
  resolved_by_message_id?: string;
};

export type MessagesResponse = {
  session_id: string;
  messages: SessionMessage[];
};

export type LocatedSession = SessionDetail & {
  client_id: string;
  project_id: string;
  project_name?: string;
};
