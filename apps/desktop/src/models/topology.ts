export type Experience = "full" | "quick_share";

export type ServerTopology =
  | { kind: "local" }
  | { kind: "remote"; url: string };

export type RunnerTopology = { kind: "local" } | { kind: "none" };

export type Exposure =
  | { kind: "none" }
  | { kind: "existing_https"; url: string }
  | { kind: "cloudflare" }
  | { kind: "open_ai_tunnel" };

export type Enrollment =
  | { kind: "managed_pairing" }
  | { kind: "shared_key" }
  | { kind: "existing_profile"; profile: string }
  | { kind: "user_credential" };

export interface RuntimeTopology {
  experience: Experience;
  server: ServerTopology;
  runner: RunnerTopology;
  exposure: Exposure;
  enrollment: Enrollment;
}

export type ServerReadiness =
  | "stopped"
  | "starting"
  | "ready"
  | "error"
  | "unknown";
export type RunnerReadiness =
  | "stopped"
  | "connecting"
  | "ready"
  | "error"
  | "unknown";
export type ExposureReadiness =
  | "disabled"
  | "starting"
  | "local_ready"
  | "remote_ready"
  | "degraded"
  | "error"
  | "unknown";
export type ProjectReadiness =
  | "none"
  | "configured"
  | "reload_required"
  | "ready"
  | "error"
  | "unknown";

export type ReadinessSummaryKind =
  | "ready_for_chat_gpt"
  | "runtime_stopped"
  | "runtime_starting"
  | "service_needs_attention"
  | "runner_disconnected"
  | "project_not_ready"
  | "runtime_ready_local_only"
  | "tunnel_ready_waiting_for_chat_gpt"
  | "connection_unverified"
  | "quick_share_stopped";

export type ReadinessNextActionKind =
  | "start_or_reconnect_service"
  | "start_runner"
  | "add_or_reload_project"
  | "choose_connection"
  | "check_connection"
  | "restart_quick_share"
  | "restore_clipboard_handoff"
  | "restart_secure_tunnel";

export interface ReadinessSnapshot {
  server: ServerReadiness;
  runner: RunnerReadiness;
  exposure: ExposureReadiness;
  project: ProjectReadiness;
  runtime_ready: boolean;
  ready_for_chatgpt: boolean;
  summary: string;
  summary_kind: ReadinessSummaryKind;
  next_action?: string | null;
  next_action_kind?: ReadinessNextActionKind | null;
}

export interface ProjectSelection {
  path: string;
  allowed_root: string;
  is_git_repository: boolean;
  runtime_project_id?: string | null;
}

export interface ProjectInspection {
  project: ProjectSelection;
  authorization_required: boolean;
}

export interface BinaryInfo {
  directory: string;
  version: string;
  git_commit: string;
  source: string;
}

export interface QuickShareState {
  provider: string;
  project: string;
  mcp_url?: string | null;
  clipboard_state: string;
  clipboard_contains: string;
  ready_for_chatgpt: boolean;
}

export type { ConnectionsSnapshot, TunnelConnection, McpProvidersSnapshot, McpProviderProfile } from "./connections-tools";

export type RegularConnectionPreference = "no_chat_gpt" | "open_ai_tunnel";
export type TunnelProxyMode = "auto" | "direct" | "custom";

export interface TunnelProxySnapshot {
  mode: TunnelProxyMode;
  custom_url?: string | null;
  effective_source: string;
  effective_proxy_present: boolean;
  system_proxy_detected: boolean;
}

export type DesktopOperationKind =
  | "environment_invite"
  | "desktop_update"
  | "environment_migration"
  | "environment_service"
  | "local_setup"
  | "local_project_activate"
  | "project_unregister"
  | "remote_setup"
  | "quick_share_start"
  | "quick_share_stop"
  | "regular_tunnel_start"
  | "regular_tunnel_stop"
  | "local_runtime_stop"
  | "runtime_refresh"
  | "runtime_resume"
  | "tunnel_proxy_update"
  | "tunnel_config_update"
  | "runner_settings_update"
  | "runner_restart"
  | "runtime_probe" | "runtime_switch" | "trace_update" | "configuration_restore";

export type DesktopOperationPhase = "running" | "cancelling";

export interface DesktopOperation {
  id: string;
  kind: DesktopOperationKind;
  phase: DesktopOperationPhase;
  started_at_ms: number;
  cancellable: boolean;
}

export type SetupProgressStep =
  | "preflight"
  | "server_configuration"
  | "server_service_install"
  | "server_service_start"
  | "server_reachability"
  | "user_authentication"
  | "runner_enrollment"
  | "runner_configuration"
  | "project_registration"
  | "runner_service_install"
  | "runner_service_start"
  | "readiness";

export interface SetupProgress {
  operation_id: string;
  step: SetupProgressStep;
  state: "started" | "complete";
}

export interface OpenAiTunnelConfigSnapshot {
  tunnel_id_present: boolean;
  api_key_present: boolean;
  source: "file" | "environment" | "invalid";
  saved_tunnel_id: string | null;
  effective_tunnel_id?: string | null;
}

export interface PowerShellRuntimeSnapshot {
  pwsh_available: boolean;
  windows_powershell_available: boolean;
}

export interface ChatGptActivitySnapshot {
  observed: boolean;
  last_meaningful_activity_at_ms?: number | null;
}

export interface DesktopState {
  environment_setup?: {
    environment_id: string;
    mode: "create" | "join";
    server_url: string;
    runner: boolean;
    runner_display_name?: string | null;
    project_path: string | null;
    service_scope: "user" | "system";
    configured: boolean;
  } | null;
  persistent_environment?: string | null;
  can_repair_runner_credential?: boolean;
  workspace_runner?: SettingsTarget | null;
  configuration_issue?: string | null;
  saved_projects?: ProjectSelection[];
  topology?: RuntimeTopology | null;
  readiness: ReadinessSnapshot;
  runtime_error?: DesktopError | null;
  component_errors?: Partial<Record<ErrorComponent, DesktopError>>;
  state_generation?: number;
  project?: ProjectSelection | null;
  binaries?: BinaryInfo | null;
  powershell_runtime?: PowerShellRuntimeSnapshot | null;
  chatgpt_activity?: ChatGptActivitySnapshot | null;
  quick_share?: QuickShareState | null;
  connections?: import("./connections-tools").ConnectionsSnapshot;
  mcp_providers?: import("./connections-tools").McpProvidersSnapshot;
  coding_agents?: import("./runner-capabilities").CodingAgentsSnapshot;
  current_operation?: DesktopOperation | null;
  setup_progress?: SetupProgress | null;
  activity_sequence: number;
  openai_tunnel_configured: boolean;
  openai_tunnel_config: OpenAiTunnelConfigSnapshot;
  regular_tunnel_available: boolean;
  runtime_autostart: boolean;
  preferred_connection: RegularConnectionPreference;
  tunnel_proxy: TunnelProxySnapshot;
}

export type ErrorComponent =
  | "runtime"
  | "server"
  | "runner"
  | "service"
  | "tunnel"
  | "auth"
  | "project"
  | "environment"
  | "migration"
  | "config"
  | "desktop"
  | "unknown";

export type RecoveryKind =
  | "auto_recovering"
  | "user_action"
  | "manual_check"
  | "confirmation_required";

export type SafeRecoveryAction =
  | "wait_auto_recovery"
  | "retry"
  | "refresh_state"
  | "start_runtime"
  | "start_server"
  | "restart_server"
  | "start_runner"
  | "restart_runner"
  | "repair_runner_credential"
  | "repair_user_credential"
  | "check_connection"
  | "open_network_settings"
  | "open_runtime_settings"
  | "open_diagnostics"
  | "open_activity"
  | "open_projects"
  | "open_setup"
  | "inspect_service_ownership"
  | "resolve_migration"
  | "observe_before_retry"
  | "restore_configuration"
  | "manage_providers"
  | "none";

export interface ErrorDiagnostic {
  component: ErrorComponent;
  stage?: string | null;
  reason_code?: string | null;
  recovery_kind: RecoveryKind;
  safe_action: SafeRecoveryAction;
  retry_allowed: boolean;
  http_status?: number | null;
  exit_code?: number | null;
  operation_id?: string | null;
  generation?: number | null;
  resource_id?: string | null;
  activity_sequence?: number | null;
}

export interface DesktopError {
  code: string;
  message: string;
  next_action: string;
  details?: unknown;
  diagnostic?: ErrorDiagnostic | null;
}

export interface ActivityEntry {
  sequence: number;
  timestamp_ms: number;
  source: string;
  tunnel_profile_id?: string | null;
  level: "info" | "warning" | "error";
  event_kind:
    | "process_started"
    | "process_exited"
    | "process_observation_failed"
    | "process_stopping"
    | "process_stopped"
    | "local_setup_preparing"
    | "local_runtime_ready"
    | "remote_connecting"
    | "remote_connected"
    | "quick_share_starting"
    | "quick_share_ready"
    | "quick_share_stopped"
    | "regular_tunnel_starting"
    | "regular_tunnel_ready"
    | "regular_tunnel_stopped"
    | "runtime_stopped"
    | "project_activated"
    | "state_recovered"
    | "operation_started"
    | "operation_cancel_requested"
    | "operation_cancelled"
    | "operation_failed";
  message: string;
}


export interface RunnerPaths { instruction_files: string[]; skill_roots: string[] }
export interface RunnerFileAccess { configured_roots: string[]; default_roots?: string[]; effective_roots: string[]; using_default_roots: boolean; allow_cwd_anywhere: boolean }
export interface SettingsTarget { config_path: string; client_id: string; server_url: string }
export interface RunnerSettings { max_concurrent_jobs?: number | null; paths: RunnerPaths; file_access: RunnerFileAccess; plugin_ids: string[]; target: SettingsTarget; can_restart: boolean }
export interface PluginRegistration { id: string; name: string; command: string; args: string[]; cwd: string | null }
export type PermissionStatus = "granted" | "denied" | "unknown";
export interface ComputerPermissions { supported: boolean; foreground: boolean; execution_process: string | null; execution_path: string | null; runner_accessibility: PermissionStatus; runner_screen_recording: PermissionStatus; desktop_accessibility: boolean; desktop_screen_recording: boolean }
