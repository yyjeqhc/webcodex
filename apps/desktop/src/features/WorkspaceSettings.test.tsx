import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "./workspace/WorkspaceContext";
import { LocaleProvider } from "../i18n/locale";
import type { DesktopState, RunnerSettings } from "../models/topology";
import { ExtensionsPanel } from "./extensions/ExtensionsPanel";
import { ComputerPermissions } from "./settings/ComputerPermissions";
import { TunnelConfigDiagnostics } from "./connection/TunnelConfigDiagnostics";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));

const api = vi.hoisted(() => ({ runnerSettings: vi.fn(), updateRunnerSettings: vi.fn(), restartOwnedRunner: vi.fn(), addRunnerPlugin: vi.fn(), computerPermissions: vi.fn(), requestComputerPermission: vi.fn(), updateTunnelConfig: vi.fn(), getState: vi.fn() }));
vi.mock("../lib/desktop-api", () => ({ desktopApi: api }));
const state: DesktopState = {
  project: { path: "/fixture/alpha", allowed_root: "/fixture/alpha", is_git_repository: false, runtime_project_id: "agent:fixture-runner:alpha" },
  readiness: { runtime_ready: true, ready_for_chatgpt: false, server: "ready", runner: "ready", exposure: "local_ready", project: "ready", summary: "Ready", summary_kind: "runtime_ready_local_only", next_action: "", next_action_kind: "choose_connection" },
  activity_sequence: 0, openai_tunnel_configured: true, regular_tunnel_available: true, runtime_autostart: false, preferred_connection: "no_chat_gpt",
  tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "direct", effective_url: null, detected_url: null },
  current_operation: null,
  openai_tunnel_config: { source: "file", saved_tunnel_id: "tunnel_fixture", effective_tunnel_id: "tunnel_fixture", tunnel_id_present: true, api_key_present: true },
};
const target = { config_path: "/fixture/runner.toml", client_id: "fixture-runner", server_url: "http://127.0.0.1:1" };
let settings: RunnerSettings;
const onState = vi.fn();
const wrap = (element: React.ReactNode) => <LocaleProvider><WorkspaceProvider state={state}>{element}</WorkspaceProvider></LocaleProvider>;

beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  native.invoke.mockImplementation(async (_name, args) => {
    if (args.request.kind === "overview") return { client_id: "fixture-runner", projects: [], recent_sessions: { sessions: [] } };
    if (args.request.kind === "windows") return { windows: [] };
    return { instructions: { files: [], scan_complete: true }, skills: { available: true, catalog: { skills: [] } }, plugins: { available: true, catalog: { plugins: [] } }, can_reload_plugins: true };
  });
  settings = { target, paths: { instruction_files: ["/fixture/global.md"], skill_roots: ["/fixture/skills"] }, plugin_ids: ["existing"], can_restart: true };
  api.runnerSettings.mockImplementation(async () => structuredClone(settings));
  api.updateRunnerSettings.mockImplementation(async (_target, _expected, paths) => { settings.paths = paths; return state; });
  api.addRunnerPlugin.mockImplementation(async (_target, provider) => { settings.plugin_ids.push(provider.id); return state; });
  api.restartOwnedRunner.mockResolvedValue(state); api.getState.mockResolvedValue(state); api.updateTunnelConfig.mockResolvedValue(state);
  api.computerPermissions.mockResolvedValue({ supported: true, foreground: false, desktop_accessibility: false, desktop_screen_recording: false });
  Object.defineProperty(HTMLDialogElement.prototype, "showModal", { configurable: true, value() { this.setAttribute("open", ""); } });
  Object.defineProperty(HTMLDialogElement.prototype, "close", { configurable: true, value() { this.removeAttribute("open"); } });
});

describe("workspace configuration boundaries", () => {
  it("edits running Tunnel credentials and clears the write-only key", async () => {
    render(wrap(<TunnelConfigDiagnostics state={state} onState={onState} />));
    const id = screen.getByLabelText("Tunnel ID"), key = screen.getByLabelText("API Key");
    expect(id).not.toBeDisabled(); expect(key).not.toBeDisabled();
    fireEvent.change(id, { target: { value: "tunnel_replacement" } });
    fireEvent.change(key, { target: { value: "fixture-only-key" } });
    fireEvent.click(screen.getByRole("button", { name: "Save and Apply" }));
    await waitFor(() => expect(api.updateTunnelConfig).toHaveBeenCalledWith({ action: "save", tunnelId: "tunnel_replacement", apiKey: "fixture-only-key" }));
    expect(key).toHaveValue(""); expect(document.body.textContent).not.toContain("fixture-only-key");
  });

  it("reconciles a saved-but-not-applied error without replaying the save", async () => {
    api.updateTunnelConfig.mockRejectedValue({ code: "tunnel_config_apply_failed", message: "Saved", next_action: "Retry" });
    api.getState.mockResolvedValue({ ...state, openai_tunnel_config: { ...state.openai_tunnel_config, saved_tunnel_id: "tunnel_saved", effective_tunnel_id: "tunnel_saved" } });
    render(wrap(<TunnelConfigDiagnostics state={state} onState={onState} />));
    fireEvent.click(screen.getByRole("button", { name: "Save and Apply" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Configuration saved, but the tunnel needs recovery");
    await waitFor(() => expect(api.getState).toHaveBeenCalledTimes(1));
    expect(api.updateTunnelConfig).toHaveBeenCalledTimes(1);
  });

  it("saves exact-target instruction and Skill paths and fences Runner restart", async () => {
    render(wrap(<ExtensionsPanel state={state} onState={onState} />));
    fireEvent.click(screen.getByRole("tab", { name: "Instructions" }));
    fireEvent.click(await screen.findByRole("button", { name: "Manage" }));
    const input = await screen.findByLabelText("Global instruction files");
    fireEvent.change(input, { target: { value: "/fixture/new.md" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.updateRunnerSettings).toHaveBeenCalledWith(target, { instruction_files: ["/fixture/global.md"], skill_roots: ["/fixture/skills"] }, { instruction_files: ["/fixture/new.md"], skill_roots: ["/fixture/skills"] }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Restart Runner" })).not.toBeDisabled());
    fireEvent.click(screen.getByRole("button", { name: "Restart Runner" }));
    await waitFor(() => expect(api.restartOwnedRunner).toHaveBeenCalledWith(target));
  });

  it("validates native Plugin arguments and writes only a new explicit registration", async () => {
    render(wrap(<ExtensionsPanel state={state} onState={onState} />));
    fireEvent.click(screen.getByRole("tab", { name: "MCP Providers" }));
    fireEvent.click(screen.getByText("Advanced: Native Tool Plugins", { selector: "summary" }));
    fireEvent.click(await screen.findByText("Add a native Tool Plugin"));
    fireEvent.change(screen.getByLabelText("Plugin ID"), { target: { value: "new-plugin" } });
    fireEvent.change(screen.getByLabelText("Display name"), { target: { value: "New plugin" } });
    fireEvent.change(screen.getByLabelText("Executable"), { target: { value: "node" } });
    const args = screen.getByLabelText("Arguments (JSON array)");
    fireEvent.change(args, { target: { value: "[42]" } });
    fireEvent.click(screen.getByRole("button", { name: "Save registration" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("only strings");
    expect(api.addRunnerPlugin).not.toHaveBeenCalled();
    fireEvent.change(args, { target: { value: '["/fixture/plugin.js"]' } });
    fireEvent.click(screen.getByRole("button", { name: "Save registration" }));
    await waitFor(() => expect(api.addRunnerPlugin).toHaveBeenCalledWith(target, { id: "new-plugin", name: "New plugin", command: "node", args: ["/fixture/plugin.js"], cwd: null }));
    expect(args).toHaveValue("[]");
  });

  it("opens the permission explanation only after foreground observation, never auto-grants", async () => {
    render(wrap(<ComputerPermissions welcome />));
    await waitFor(() => expect(api.computerPermissions).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    api.computerPermissions.mockResolvedValue({ supported: true, foreground: true, desktop_accessibility: false, desktop_screen_recording: false });
    fireEvent.focus(window);
    expect(await screen.findByRole("dialog")).toHaveAccessibleName("Computer Use");
    expect(api.requestComputerPermission).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Continue · available in Settings" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(localStorage.getItem("desktop-permissions-explained")).toBe("1");
  });

  it("keeps permission probe failures visible and allows an explicit retry", async () => {
    api.computerPermissions.mockRejectedValueOnce(new Error("fixture unavailable"));
    render(wrap(<ComputerPermissions />));
    expect(await screen.findByRole("alert")).toHaveTextContent("Permission request unavailable");
    fireEvent.click(screen.getByText("Troubleshooting", { selector: "summary" }));
    fireEvent.click(screen.getByRole("button", { name: "Recheck permissions" }));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(screen.getAllByText("Permission needed")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Grant Permission · Screen Recording" })).toBeEnabled();
    expect(api.requestComputerPermission).not.toHaveBeenCalled();
  });
});
