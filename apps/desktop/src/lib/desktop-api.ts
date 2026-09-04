import { invoke } from "@tauri-apps/api/core";
import type {
  ActivityEntry,
  DesktopState,
  ProjectSelection,
} from "../models/topology";

export const desktopApi = {
  getState: () => invoke<DesktopState>("get_desktop_state"),
  refresh: () => invoke<DesktopState>("refresh_runtime_status"),
  inspectProject: (projectPath: string) =>
    invoke<ProjectSelection>("inspect_project", {
      request: { projectPath },
    }),
  configureLocal: (projectPath: string) =>
    invoke<DesktopState>("configure_local_setup", {
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
  activity: () => invoke<ActivityEntry[]>("get_bounded_activity"),
};

export type QuickShareProvider = "cloudflare" | "openai" | "none";

