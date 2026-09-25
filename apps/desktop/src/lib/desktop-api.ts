import type { MachineBuildInfo, RuntimeSettings, RuntimeSource, RuntimeSwitchRequest, RuntimeSwitchResult, DiagnosticSnapshot, DiagnosticResource, TraceUpdate, TraceSettings, UpdateStatus } from "../models/runtime-shell";
import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEntry,
  RunnerPaths,
  SettingsTarget,
  PluginRegistration,
  RunnerSettings,
  ComputerPermissions,
  DesktopState,
  ProjectSelection,
  TunnelProxyMode,
} from "../models/topology";

import type { McpProviderRequest, TunnelProfileAction, TunnelProfileRequest } from "../models/connections-tools";
import type { CodingAgentRequest, SshRegisterRequest, SshResourcesSnapshot, SshMutationResult, RunnerCapabilityAuthorizationSnapshot } from "../models/runner-capabilities";

export const desktopApi = {
  prepareProjectUnregister: (project: string) => invoke<import("../models/workspace").UnregisterObservation>("prepare_project_unregister", { project }),
  unregisterProject: ({ target, project, expected_revision }: import("../models/workspace").UnregisterObservation) => invoke<DesktopState>("unregister_project", { request: { target, project, expected_revision, confirmed: true } }),
  desktopBuildInfo: () => invoke<MachineBuildInfo>("get_desktop_build_info"),
  runtimeSettings: () => invoke<RuntimeSettings>("get_runtime_settings"),
  probeRuntime: (source: RuntimeSource) => invoke<RuntimeSettings>("probe_runtime", { source }),
  recheckRuntime: () => invoke<RuntimeSettings>("recheck_runtime"),
  switchRuntime: (request: RuntimeSwitchRequest) => invoke<RuntimeSwitchResult>("switch_runtime", { request }),
  diagnostics: () => invoke<DiagnosticSnapshot>("get_diagnostics"),
  setToolRequestTracing: (request: TraceUpdate) => invoke<TraceSettings>("set_tool_request_tracing", { request }),
  openDiagnosticResource: (kind: DiagnosticResource) => invoke<void>("open_diagnostic_resource", { kind }),
  copyRuntimeConsoleCredential: (expectedFence: string) => invoke<void>("copy_runtime_console_credential", { expectedFence }),
  copyDiagnosticReport: () => invoke<void>("copy_diagnostic_report"),
  exportSupportBundle: (path: string) => invoke<void>("export_support_bundle", { path }),
  restorePreviousConfiguration: (expectedPrimarySha256: string) => invoke<DesktopState>("restore_previous_configuration", { expectedPrimarySha256 }),
  checkForUpdates: (manual = false) => invoke<UpdateStatus>("check_for_updates", { manual }),
  remindUpdateLater: () => invoke<UpdateStatus>("remind_update_later"),
  openLatestRelease: () => invoke<void>("open_latest_release"),
  saveCodingAgent: (request: CodingAgentRequest) => invoke<DesktopState>("save_coding_agent", { request }),
  removeCodingAgent: (target: SettingsTarget, providerId: string, expectedRevision: number) =>
    invoke<DesktopState>("remove_coding_agent", { request: { target, provider_id: providerId, expected_revision: expectedRevision } }),
  sshResources: () => invoke<SshResourcesSnapshot>("ssh_resource_list"),
  runnerCapabilityAuthorization: (expected: SettingsTarget) =>
    invoke<RunnerCapabilityAuthorizationSnapshot>("runner_capability_authorization", { expected }),
  authorizeRunnerCapabilities: (expected: SettingsTarget) =>
    invoke<RunnerCapabilityAuthorizationSnapshot>("authorize_runner_capabilities", { request: { expected, confirmed: true } }),
  registerSshResource: (request: SshRegisterRequest) => invoke<SshMutationResult>("ssh_resource_register", { request }),
  removeSshResource: (expected: SettingsTarget, observationId: string, name: string) =>
    invoke<SshMutationResult>("ssh_resource_remove", { request: { expected, observation_id: observationId, name } }),
  saveTunnelProfile: (request: TunnelProfileRequest) => invoke<DesktopState>("save_tunnel_profile", { request }),
  tunnelProfileAction: (profileId: string, action: TunnelProfileAction) => invoke<DesktopState>("tunnel_profile_action", { profileId, action }),
  saveMcpProvider: (request: McpProviderRequest) => invoke<DesktopState>("save_mcp_provider", { request }),
  removeMcpProvider: (id: string, expectedRevision: number) => invoke<DesktopState>("remove_mcp_provider", { id, expectedRevision }),
  runnerSettings: () => invoke<RunnerSettings>("get_runner_settings"),
  updateRunnerSettings: (target: SettingsTarget, expected: RunnerPaths, paths: RunnerPaths) => invoke<DesktopState>("update_runner_settings", { request: { target, expected, paths } }),
  restartOwnedRunner: (target: SettingsTarget) => invoke<DesktopState>("restart_owned_runner", { target }),
  addRunnerPlugin: (target: SettingsTarget, provider: PluginRegistration) => invoke<DesktopState>("add_runner_plugin", { request: { target, provider } }),
  computerPermissions: () => invoke<ComputerPermissions>("get_computer_permissions"),
  requestComputerPermission: (action: "accessibility" | "screen_recording" | "open_settings") => invoke<ComputerPermissions>("request_computer_permission", { action }),
  updateTunnelConfig: (request: { action: "save"; tunnelId: string; apiKey: string | null } | { action: "use_environment" }) =>
    invoke<DesktopState>("update_tunnel_config", { request }),
  getState: () => invoke<DesktopState>("get_desktop_state"),
  openPowerShellInstallGuide: () => invoke<void>("open_powershell_install_guide"),
  getLaunchAtLogin: () => invoke<boolean>("get_launch_at_login"),
  setLaunchAtLogin: (enabled: boolean) =>
    invoke<boolean>("set_launch_at_login", { request: { enabled } }),
  refresh: () => invoke<DesktopState>("refresh_runtime_status"),
  observeChatgptActivity: () => invoke<DesktopState>("observe_chatgpt_activity"),
  resumeSavedRuntime: () => invoke<DesktopState>("resume_saved_runtime"),
  updateTunnelProxy: (mode: TunnelProxyMode, customUrl?: string | null) =>
    invoke<DesktopState>("update_tunnel_proxy", {
      request: { mode, customUrl: customUrl ?? null },
    }),
  inspectProject: (projectPath: string) =>
    invoke<ProjectSelection>("inspect_project", {
      request: { projectPath },
    }),
  configureLocal: (projectPath: string) =>
    invoke<DesktopState>("configure_local_setup", {
      request: { projectPath },
    }),
  activateLocalProject: (projectPath: string) =>
    invoke<DesktopState>("activate_local_project", {
      request: { projectPath },
    }),
  configureRemote: (
    serverUrl: string,
    pairingCode: string,
    projectPath: string,
  ) =>
    invoke<DesktopState>("configure_remote_setup", {
      request: { serverUrl, pairingCode, projectPath },
    }),
  startQuickShare: (projectPath: string, provider: QuickShareProvider) =>
    invoke<DesktopState>("start_quick_share", {
      request: { projectPath, provider },
    }),
  stopQuickShare: () => invoke<DesktopState>("stop_quick_share"),
  startRegularTunnel: () => invoke<DesktopState>("start_regular_tunnel"),
  stopRegularTunnel: () => invoke<DesktopState>("stop_regular_tunnel"),
  stopLocalRuntime: () => invoke<DesktopState>("stop_local_runtime"),
  cancelOperation: (operationId: string) =>
    invoke<DesktopState>("cancel_desktop_operation", {
      request: { operationId },
    }),
  activity: () => invoke<ActivityEntry[]>("get_bounded_activity"),
};

export type QuickShareProvider = "cloudflare" | "openai" | "none";

