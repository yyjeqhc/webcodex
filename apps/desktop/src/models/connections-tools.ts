export type TunnelProvider = { kind: "openai" } | { kind: "cloudflare_named"; public_origin: string; tunnel_id: string } | { kind: "cloudflare_quick" };
export interface CloudflareConnectionStatus {
  profile_id: string; server_instance_id: string; process_generation: number;
  lifecycle: "stopped" | "starting" | "running" | "disconnected" | "error";
  public_origin: string | null; oauth_configured: boolean; observed_authorization: boolean;
  configured_revision: number; applied_revision: number | null; local_target: string; reason_code: string | null;
}
export type CloudflareConnectionRequest =
  | { action: "status"; profile_id: string }
  | { action: "start"; profile_id: string; server_instance_id: string; expected_revision: number }
  | { action: "stop"; profile_id: string; server_instance_id: string; process_generation: number }
  | { action: "configure_oauth"; profile_id: string; server_instance_id: string; process_generation: number; redirect_uri: string; scopes: string[]; replace?: boolean };
// Only the explicit handoff response carries a secret; never add it to DesktopState.
export interface CloudflareOAuthHandoff { client_id: string; client_secret: string | null; already_configured?: boolean }
// Private values appear only in write-only requests, never in public snapshots.
export type ConnectionLifecycle = "stopped" | "starting" | "running" | "stopping" | "error";
export type TunnelHostMode = "standalone" | "embedded";
export type ConnectionError = "start_failed" | "startup_timeout" | "process_exited" | "protocol_invalid" | "health_stale" | "tunnel_unavailable" | "local_mcp_unavailable" | "stop_failed";
export interface TunnelConnection {
  /** Missing on legacy native snapshots means OpenAI. */
  provider?: TunnelProvider;
  configuration_id?: string | null;
  id: string;
  name: string;
  tunnel_id: string | null;
  credential_present: boolean;
  enabled: boolean;
  autostart: boolean;
  revision: number;
  source: "file" | "environment" | "invalid";
  host_mode: TunnelHostMode;
  server_restart_required: boolean;
  lifecycle: ConnectionLifecycle;
  pid: number | null;
  health: "unknown" | "healthy" | "degraded";
  last_error: ConnectionError | null;
  ready: boolean;
  process_started: boolean;
  process_ready: boolean;
  tunnel_ready: boolean | null;
  local_mcp_ready: boolean | null;
  failure_stage: string | null;
  reason_code: string | null;
  auto_proxy_used: boolean | null;
  runtime_directory: string | null;
  local_mcp_url: string | null;
  logs: { timestamp_ms: number; event: string }[];
}
export interface ConnectionsSnapshot {
  profiles: TunnelConnection[];
  running: number;
  needs_attention: number;
  config_error: boolean;
  server_restart_required: boolean;
}
export interface TunnelProfileRequest {
  provider?: TunnelProvider;
  cloudflare_token?: string | null;
  id: string | null;
  name: string;
  tunnel_id: string;
  api_key: string | null;
  autostart: boolean;
  host_mode: TunnelHostMode;
  expected_revision: number | null;
}
export type TunnelProfileAction = "start" | "stop" | "restart" | "delete";
export interface McpProviderProfile {
  id: string;
  name: string;
  command: string;
  args: string[];
  cwd: string | null;
  enabled: boolean;
  scope: "runner";
  env_keys: string[];
}
export interface McpProvidersSnapshot {
  revision: number;
  profiles: McpProviderProfile[];
  restart_required: boolean;
  config_error: boolean;
  max_enabled: number;
}
export interface McpProviderRequest {
  id: string | null;
  expected_revision: number;
  name: string;
  command: string;
  args: string[];
  cwd: string | null;
  enabled: boolean;
  env: Record<string, string | null>;
}
export const EMPTY_CONNECTIONS: ConnectionsSnapshot = { profiles: [], running: 0, needs_attention: 0, config_error: false, server_restart_required: false };
export const EMPTY_MCP_PROVIDERS: McpProvidersSnapshot = { revision: 0, profiles: [], restart_required: false, config_error: false, max_enabled: 8 };
export function connectionActive(profile: TunnelConnection): boolean {
  return profile.lifecycle === "starting" || profile.lifecycle === "running" || profile.lifecycle === "stopping" || (profile.lifecycle === "error" && profile.pid !== null);
}
