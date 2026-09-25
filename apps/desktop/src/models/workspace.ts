export interface Attention { open_todos: number; open_questions: number; open_risks: number }
export interface SessionActivity {
  kind: string; state: string; summary?: string; tool?: string; paths?: string[];
  started_at?: number; finished_at?: number; job_id?: string; job_handoff?: boolean;
}
export interface WorkflowSession {
  session_id: string; title: string; lifecycle: string; updated_at: number;
  project_id?: string; project_name?: string; running_call: boolean;
  running_jobs: number; running_jobs_complete: boolean;
  current_activity?: SessionActivity; last_activity?: SessionActivity;
  overview: { attention: Attention; reported_progress?: { text: string; reported_at: number }; validation?: { state: string; unresolved_failure_count?: number; history_complete?: boolean } };
  activity?: SessionActivity[]; activity_truncated?: boolean;
}
export interface WorkspaceProject {
  id: string; name?: string; path?: string; connected: boolean;
  sessions?: { active_sessions: number; running_sessions: number; latest_updated_at?: number; sessions_truncated?: boolean };
}
export interface RunnerOverview {
  projects_available?: boolean;
  client_id: string; connected: boolean; status?: string; visible_project_count: number;
  projects: WorkspaceProject[]; projects_truncated: boolean;
  recent_sessions?: { sessions: WorkflowSession[]; truncated: boolean; scan_truncated: boolean };
}
export interface WindowSummary {
  client_window_key: string; source: string; last_project?: string; last_seen_at_ms: number;
  last_meaningful_activity_at_ms?: number; active_count: number; linked_session_count: number;
}
export interface WindowDetail extends WindowSummary {
  linked_sessions: { session_id?: string; workflow_session_id?: string; title?: string; project?: string; lifecycle?: string; last_linked_at_ms?: number }[];
  active_requests?: { server_trace_id: string; tool_name?: string; project?: string; started_at_ms: number; elapsed_ms: number }[];
  activity: WindowCall[];
  sessions_truncated: boolean; activity_truncated: boolean;
}
export interface GitSummary {
  branch?: string; clean?: boolean; git_available: boolean; non_git_project: boolean;
  files?: { path: string; status?: string }[]; files_total: number; files_truncated: boolean;
}
export interface InstructionSummary { source_scope: "runner" | "project"; path: string; fingerprint: string; truncated: boolean; total_lines: number }
export interface SkillSummary { skill_id: string; name: string; description?: string; source?: string; source_scope?: string; trust?: string; available?: boolean }
export interface PluginSummary { id?: string; plugin?: string; name?: string; status?: string; tool_count?: number; tools?: unknown[] }
export interface ExtensionsSnapshot {
  project: string; runner: string; can_reload_plugins: boolean;
  instructions: { files: InstructionSummary[]; scan_complete: boolean; truncated: boolean };
  skills: { available: boolean; catalog?: { skills: SkillSummary[]; truncated?: boolean } };
  plugins: { available: boolean; catalog?: { providers?: PluginSummary[]; plugins?: PluginSummary[]; truncated?: boolean } };
}
export type WorkspaceRequest =
  | { kind: "overview" | "windows" }
  | { kind: "sessions" | "extensions" | "project_git"; project: string }
  | { kind: "session"; project: string; session_id: string }
  | { kind: "window"; client_window_key: string }
  | { kind: "instruction"; project: string; source_scope: string; path: string; fingerprint: string }
  | { kind: "plugin_reload"; project: string; plugin: string };

export interface WindowCall {
  server_trace_id?: string;
  tool_name?: string; meaningful?: boolean; status: string; project?: string;
  ended_at_ms?: number; started_at_ms?: number; request_observed_at_ms?: number; response_handed_at_ms?: number;
  service_ms?: number; next_call_gap_ms?: number; window_transition_kind?: string; response_streaming?: boolean;
  activity_presentation?: string; activity_kind?: string;
}

export interface UnregisterObservation {
  target: import("./topology").SettingsTarget;
  project: string;
  expected_revision: string;
  path: string;
}
