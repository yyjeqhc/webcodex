import type { ConnectionsSnapshot, TunnelConnection } from "../models/connections-tools";

export function connectionFixture(overrides: Partial<TunnelConnection> = {}): TunnelConnection {
  return {
    id: "default", name: "ChatGPT", tunnel_id: "tunnel_fixture", credential_present: true,
    enabled: true, autostart: true, revision: 1, source: "file",
    lifecycle: "running", pid: 100, health: "healthy", last_error: null, ready: true,
    process_started: true, process_ready: true, tunnel_ready: true, local_mcp_ready: true,
    failure_stage: null, reason_code: null,
    runtime_directory: null, health_url: null, log_file: null, tunnel_client_pid: 101,
    local_mcp_url: "http://127.0.0.1:62645/mcp", logs: [], ...overrides,
  };
}
export function connectionSnapshot(...profiles: TunnelConnection[]): ConnectionsSnapshot {
  return { profiles, running: profiles.filter(profile => profile.ready).length, needs_attention: profiles.filter(profile => profile.last_error !== null).length, config_error: false };
}
