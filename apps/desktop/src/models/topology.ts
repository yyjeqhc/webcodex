export type Experience = "full" | "quick_share";

export type ServerTopology =
  | { kind: "local" }
  | { kind: "remote"; url: string };

export type RunnerTopology = { kind: "local" };

export type Exposure =
  | { kind: "none" }
  | { kind: "existing_https"; url: string }
  | { kind: "cloudflare" }
  | { kind: "open_ai_tunnel" };

export type Enrollment =
  | { kind: "managed_pairing" }
  | { kind: "shared_key" }
  | { kind: "existing_profile"; profile: string };

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
  workspace_runner?: SettingsTarget | null;
  configuration_issue?: string | null;
  saved_projects?: ProjectSelection[];
  topology?: RuntimeTopology | null;
  readiness: ReadinessSnapshot;
  project?: ProjectSelection | null;
  binaries?: BinaryInfo | null;
  powershell_runtime?: PowerShellRuntimeSnapshot | null;
  chatgpt_activity?: ChatGptActivitySnapshot | null;
  quick_share?: QuickShareState | null;
  connections?: import("./connections-tools").ConnectionsSnapshot;
  mcp_providers?: import("./connections-tools").McpProvidersSnapshot;
  coding_agents?: import("./runner-capabilities").CodingAgentsSnapshot;
  current_operation?: DesktopOperation | null;
  activity_sequence: number;
  openai_tunnel_configured: boolean;
  openai_tunnel_config: OpenAiTunnelConfigSnapshot;
  regular_tunnel_available: boolean;
  runtime_autostart: boolean;
  preferred_connection: RegularConnectionPreference;
  tunnel_proxy: TunnelProxySnapshot;
}

export interface DesktopError {
  code: string;
  message: string;
  next_action: string;
  details?: unknown;
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
export interface SettingsTarget { config_path: string; client_id: string; server_url: string }
export interface RunnerSettings { paths: RunnerPaths; plugin_ids: string[]; target: SettingsTarget; can_restart: boolean }
export interface PluginRegistration { id: string; name: string; command: string; args: string[]; cwd: string | null }
export interface ComputerPermissions { supported: boolean; foreground: boolean; desktop_accessibility: boolean; desktop_screen_recording: boolean }
