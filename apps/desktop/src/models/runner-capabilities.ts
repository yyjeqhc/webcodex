import type { SettingsTarget } from "./topology";

export interface CodingAgentProvider { provider_id: string; name: string }
export interface CodingAgentInventory {
  client_id: string;
  connected: boolean;
  coding_agent_providers?: CodingAgentProvider[];
}
export interface CodingAgentProfile extends CodingAgentProvider {
  executable: string;
  args: string[];
  enabled: boolean;
  env_from_env: Record<string, string>;
  allowed_config_options: string[];
}
export interface AcpGlobalSettings { max_concurrent_runs: number; permission_timeout_secs: number }
export interface CodingAgentsSnapshot {
  revision: number;
  profiles: CodingAgentProfile[];
  global_settings: AcpGlobalSettings | null;
  restart_required: boolean;
  config_error: boolean;
  max_enabled: number;
}
export const EMPTY_CODING_AGENTS: CodingAgentsSnapshot = {
  revision: 0, profiles: [], global_settings: null, restart_required: false, config_error: false, max_enabled: 8,
};
export interface CodingAgentRequest {
  target: SettingsTarget;
  expected_revision: number;
  previous_id: string | null;
  profile: CodingAgentProfile;
  global_settings: AcpGlobalSettings | null;
}
export interface SshResource {
  name: string;
  source: "static" | "managed";
  active: boolean;
  pending_restart: boolean;
}
export type SshResourceError = "insufficient_scope" | "authorization_unavailable" | "runner_replaced"
  | "ssh_resource_registry_stale" | "ssh_resource_outcome_unknown" | "ssh_resource_registry_unavailable"
  | "ssh_resource_invalid" | "ssh_resource_static_read_only" | "ssh_resource_static_conflict"
  | "ssh_resource_name_conflict" | "ssh_resource_not_found";
export interface SshResourcesSnapshot {
  runner: string;
  available: boolean;
  can_authorize?: boolean;
  observation_id: string | null;
  resources: SshResource[];
  error_kind: SshResourceError | null;
}
export interface SshMutationResult {
  success: boolean;
  error_kind: SshResourceError | null;
  restart_required: boolean;
  inventory: SshResourcesSnapshot;
}
export interface SshRegisterRequest {
  expected: SettingsTarget;
  observation_id: string;
  name: string;
  target: string;
  default_cwd: string | null;
}

/** Active is observed, never inferred from the Desktop manifest. */
export function codingAgentIsActive(profile: CodingAgentProvider, inventory: CodingAgentInventory | null): boolean {
  return Boolean(inventory?.connected && inventory.coding_agent_providers?.some(
    current => current.provider_id === profile.provider_id && current.name === profile.name,
  ));
}
