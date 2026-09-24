export interface RuntimeContract { min_generation: number; max_generation: number }
export type ProtocolCompatibility = "compatible" | "incompatible" | "unknown";
export type BuildAlignment = "exact" | "different_version" | "different_commit" | "dirty" | "unknown";
export interface MachineBuildInfo {
  schema_version: number; binary: string; version: string; git_commit: string | null; git_dirty: boolean | null;
  built_at: string | null; target: string; architecture: string; desktop_runtime_contract: RuntimeContract;
  agent_protocol_generation?: number | null;
}
export type RuntimeSource = { kind: "bundled" } | { kind: "custom"; directory: string };
export interface BinaryProbe { name: string; present: boolean; executable: boolean; metadata: MachineBuildInfo | null; sha256: string | null; error_code: string | null }
export interface RuntimeCandidate {
  candidate_id: string; source: RuntimeSource; selection_revision: number; checked_at_ms: number; directory: string | null;
  binaries: BinaryProbe[]; compatibility: ProtocolCompatibility; build_alignment: BuildAlignment; advisories: string[];
  error_code: string | null; fingerprint: string | null;
}
export interface RuntimeSwitchResult {
  outcome: "activated" | "selected" | "rolled_back" | "recovery_required"; reason_code: string | null;
  rollback_reason_code: string | null; selection_revision: number; restart_required: boolean;
}
export interface RuntimeSettings {
  source: RuntimeSource; selection_revision: number; desktop_contract: RuntimeContract; selected: RuntimeCandidate | null;
  candidate: RuntimeCandidate | null; previous_source: RuntimeSource | null; last_switch: RuntimeSwitchResult | null;
  unavailable_code: string | null; active_jobs: number | null; can_switch: boolean; switch_unavailable_reason: string | null;
}
export interface RuntimeSwitchRequest { candidate_id: string; expected_selection_revision: number; confirm_interrupt: boolean }
export type TraceMode = "off" | "metadata" | "full";
export interface TraceSettings {
  mode: TraceMode; effective_mode: TraceMode | null; revision: string; available: boolean; restart_required: boolean;
  can_restart: boolean; error_code: string | null;
}
export interface TraceUpdate { mode: TraceMode; expected_revision: string; confirm_full: boolean; restart: boolean; confirm_interrupt: boolean }
export type DiagnosticResource = "app_data" | "server_configuration" | "trace_directory" | "runtime_directory" | "runtime_console" | "documentation" | "github" | "report_issue" | "contributing" | "desktop_development";
export interface ContinuationSummary {
  tool_name: string | null; execution: string; response_handoff: "not_confirmed" | "stream_started" | "handler_returned";
  request_observed_at_ms: number | null; response_handed_at_ms: number | null; service_ms: number | null;
  previous_response_gap_ms?: number | null;
  next_call_gap_ms: number | null; next_meaningful_call: "observed" | "not_observed"; elapsed_ms: number | null;
  window_transition_kind: string | null; active_request_count: number | null; history_partial: boolean; interpretation?: string;
}
export interface DiagnosticReport {
  schema_version: number; desktop: Partial<MachineBuildInfo>; last_webcodex_call: ContinuationSummary | null;
  selected_runtime: { source: string; selection_revision: number; binaries: Partial<MachineBuildInfo>[]; desktop_protocol_compatibility: ProtocolCompatibility | null; server_runner_protocol_compatibility: ProtocolCompatibility | null; build_alignment: BuildAlignment | null };
  runtime_health: { server: string; runner: string; project: string; connection: string; runtime_ready: boolean; last_observed_chatgpt_activity_at_ms: number | null };
}
export interface DiagnosticSnapshot {
  schema_version: number; observed_at_ms: number; trace: TraceSettings;
  configuration: { reason_code: string | null; backup_available: boolean; primary_fingerprint: string | null };
  resources: DiagnosticResource[]; can_copy_console_credential: boolean; credential_copy_fence: string | null;
  report: DiagnosticReport; markdown: string;
}
export interface ReleaseNotice { version: string; runtime_version: string; release_url: string; compatibility: "runtime_compatible" | "desktop_required" | "unknown" }
export interface UpdateStatus { state: string; latest: ReleaseNotice | null; update_available: boolean; show_banner: boolean; cached: boolean; last_check_at_ms: number | null; manual_error: string | null }
