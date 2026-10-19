import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import type { DesktopState, RunnerSettings } from "../../models/topology";
import { SettingsPanel } from "./SettingsPanel";

const api = vi.hoisted(() => ({
  runnerSettings: vi.fn(), getState: vi.fn(), getLaunchAtLogin: vi.fn(),
  setLaunchAtLogin: vi.fn(), updateTunnelProxy: vi.fn(), restartOwnedRunner: vi.fn(),
  updateRunnerAllowedRoots: vi.fn(),
}));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./ComputerPermissions", () => ({ ComputerPermissions: () => null }));
vi.mock("./RunnerCapacityPanel", () => ({ RunnerCapacityPanel: () => null }));
vi.mock("./RuntimePanel", () => ({ RuntimePanel: () => null }));
vi.mock("./DiagnosticsPanel", () => ({ DiagnosticsPanel: () => null }));
vi.mock("./AboutPanel", () => ({ AboutPanel: () => null }));
vi.mock("./LocalServicesPanel", () => ({ LocalServicesPanel: () => null }));
vi.mock("./ConfigurationDataPanel", () => ({ ConfigurationDataPanel: () => null }));

const target = { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:1" };
const fixture: DesktopState = {
  workspace_runner: target,
  project: { path: "/fixture/project", allowed_root: "/fixture/project", is_git_repository: false, runtime_project_id: "agent:fixture:project" },
  topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" }, exposure: { kind: "none" }, enrollment: { kind: "shared_key" } },
  readiness: { runtime_ready: true, ready_for_chatgpt: false, server: "ready", runner: "ready", exposure: "local_ready", project: "ready", summary: "Ready", summary_kind: "runtime_ready_local_only", next_action_kind: "choose_connection" },
  activity_sequence: 0, openai_tunnel_configured: false, regular_tunnel_available: false, runtime_autostart: false, preferred_connection: "no_chat_gpt",
  openai_tunnel_config: { source: "file", saved_tunnel_id: null, effective_tunnel_id: null, tunnel_id_present: false, api_key_present: false },
  tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "direct", effective_proxy_present: false, system_proxy_detected: false },
  current_operation: null,
};
const fixtureSettings: RunnerSettings = {
  target, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: [], can_restart: true,
  file_access: { configured_roots: [], default_roots: ["/Users/fixture"], effective_roots: ["/Users/fixture"], using_default_roots: true, allow_cwd_anywhere: false },
};
const refreshOperation = { id: "refresh", kind: "runtime_refresh", phase: "running", started_at_ms: 1, cancellable: false } as const;
const busyError = { code: "desktop_state_unavailable", message: "The core is being refreshed.", next_action: "Refresh" };
let state: DesktopState, settings: RunnerSettings;
const wrap = (child: React.ReactNode) => <LocaleProvider><DesktopMantineProvider>{child}</DesktopMantineProvider></LocaleProvider>;
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
function retarget() {
  const nextTarget = { ...target, config_path: "/other/runner.toml", client_id: "other", server_url: "http://127.0.0.1:2" };
  state = { ...state, workspace_runner: nextTarget, tunnel_proxy: { ...state.tunnel_proxy, mode: "direct", custom_url: null } };
  settings = { ...structuredClone(fixtureSettings), target: nextTarget, file_access: { ...fixtureSettings.file_access, default_roots: ["/Users/other"], effective_roots: ["/Users/other"] } };
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  state = structuredClone(fixture); settings = structuredClone(fixtureSettings);
  api.runnerSettings.mockImplementation(async () => structuredClone(settings));
  api.getState.mockImplementation(async () => state);
  api.getLaunchAtLogin.mockResolvedValue(false);
  api.setLaunchAtLogin.mockImplementation(async value => value);
  api.updateTunnelProxy.mockImplementation(async () => state);
  api.restartOwnedRunner.mockImplementation(async () => state);
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("Settings observation and action ownership", () => {
  it.each(["unmount", "retarget"])("does not restore an old Desktop state from PowerShell guidance after %s", async boundary => {
    state = { ...state, powershell_runtime: { pwsh_available: false, windows_powershell_available: true } };
    const pending = deferred<DesktopState>(), onState = vi.fn(), original = state;
    api.getState.mockReturnValueOnce(pending.promise);
    const view = render(wrap(<SettingsPanel state={state} onState={onState} initialSection="runtime" />));
    fireEvent.click(screen.getByRole("button", { name: "Recheck" }));
    expect(api.getState).toHaveBeenCalledTimes(1);
    if (boundary === "unmount") view.unmount();
    else { retarget(); view.rerender(wrap(<SettingsPanel state={state} onState={onState} initialSection="runtime" />)); }
    await act(async () => { pending.resolve(original); });
    expect(onState).not.toHaveBeenCalled();
  });

  it("recovers file access when a first settings read races with runtime refresh", async () => {
    vi.useFakeTimers();
    state = { ...state, current_operation: refreshOperation };
    api.runnerSettings.mockRejectedValueOnce(busyError);
    const view = render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    await act(async () => {});
    state = { ...state, current_operation: null };
    view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    await act(async () => { await vi.advanceTimersByTimeAsync(250); });
    expect(api.runnerSettings).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("button", { name: "Authorize folder" })).toBeEnabled();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows a failed settings read and allows recovery without leaving the page", async () => {
    api.runnerSettings.mockRejectedValueOnce({ code: "runner_settings_read_failed", message: "Fixture settings could not be read", next_action: "Refresh" });
    render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("runner_settings_read_failed");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Authorize folder" })).toBeDisabled();
    fireEvent.click(within(alert).getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Authorize folder" })).toBeEnabled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(api.runnerSettings).toHaveBeenCalledTimes(2);
  });

  it("retries a bounded-out settings observation when the published runtime refresh ends", async () => {
    vi.useFakeTimers();
    state = { ...state, current_operation: refreshOperation };
    api.runnerSettings.mockRejectedValue(busyError);
    const view = render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
    expect(screen.getByRole("alert")).toHaveTextContent("desktop_state_unavailable");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    const attempts = api.runnerSettings.mock.calls.length;
    expect(attempts).toBeGreaterThan(1);
    api.runnerSettings.mockResolvedValue(settings);
    state = { ...state, current_operation: null };
    view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    await act(async () => {});
    expect(api.runnerSettings).toHaveBeenCalledTimes(attempts + 1);
    expect(screen.getByRole("button", { name: "Authorize folder" })).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("does not let a slow old settings read replace the newly selected Runner", async () => {
    const old = deferred<RunnerSettings>();
    api.runnerSettings.mockReturnValueOnce(old.promise);
    const view = render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    retarget();
    view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    await screen.findByText("/Users/other");
    await act(async () => { old.resolve(fixtureSettings); });
    expect(screen.getByText("/Users/other")).toBeInTheDocument();
    expect(screen.queryByText("/Users/fixture")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Authorize folder" })).toBeEnabled();
  });

  it("refuses a read for a different Runner even if the published state has not changed", async () => {
    api.runnerSettings.mockResolvedValue({ ...settings, target: { ...target, client_id: "other" } });
    render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="access" />));
    expect(await screen.findByRole("alert")).toHaveTextContent("runner_settings_changed");
    expect(screen.getByRole("button", { name: "Authorize folder" })).toBeDisabled();
  });

  it("preserves same-target proxy drafts but discards them when the Runner changes", async () => {
    const view = render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="network" />));
    fireEvent.change(screen.getByLabelText("Proxy mode"), { target: { value: "custom" } });
    fireEvent.change(screen.getByLabelText("Proxy URL"), { target: { value: "http://proxy-a.invalid:8080" } });
    state = { ...state, activity_sequence: 2 };
    view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="network" />));
    fireEvent.click(screen.getByRole("tab", { name: "General" }));
    fireEvent.click(screen.getByRole("tab", { name: "Network" }));
    expect(screen.getByLabelText("Proxy URL")).toHaveValue("http://proxy-a.invalid:8080");
    retarget();
    view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="network" />));
    expect(screen.getByRole("tab", { name: "Network" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByLabelText("Proxy mode")).toHaveValue("direct");
    fireEvent.change(screen.getByLabelText("Proxy mode"), { target: { value: "custom" } });
    expect(screen.getByLabelText("Proxy URL")).toHaveValue("");
    expect(api.updateTunnelProxy).not.toHaveBeenCalled();
  });

  it.each(["unmount", "retarget"] as const)("ignores a proxy save response after %s", async boundary => {
    const save = deferred<DesktopState>(), onState = vi.fn(), original = state;
    api.updateTunnelProxy.mockReturnValueOnce(save.promise);
    const view = render(wrap(<SettingsPanel state={state} onState={onState} initialSection="network" />));
    const button = screen.getByRole("button", { name: "Save and apply" });
    fireEvent.click(button); fireEvent.click(button);
    expect(api.updateTunnelProxy).toHaveBeenCalledTimes(1);
    if (boundary === "unmount") view.unmount();
    else { retarget(); view.rerender(wrap(<SettingsPanel state={state} onState={onState} initialSection="network" />)); }
    await act(async () => { save.resolve(original); });
    expect(onState).not.toHaveBeenCalled();
  });

  it.each(["unmount", "retarget"] as const)("ignores a Runner restart response after %s", async boundary => {
    const restart = deferred<DesktopState>(), onState = vi.fn(), original = state;
    api.restartOwnedRunner.mockReturnValueOnce(restart.promise);
    const view = render(wrap(<SettingsPanel state={state} onState={onState} initialSection="runtime" />));
    await waitFor(() => expect(document.querySelector('[data-webcodex-action="restart-owned-runner"]')).not.toBeNull());
    const button = document.querySelector<HTMLButtonElement>('[data-webcodex-action="restart-owned-runner"]')!;
    fireEvent.click(button); fireEvent.click(button);
    expect(api.restartOwnedRunner).toHaveBeenCalledExactlyOnceWith(target);
    if (boundary === "unmount") view.unmount();
    else { retarget(); view.rerender(wrap(<SettingsPanel state={state} onState={onState} initialSection="runtime" />)); }
    await act(async () => { restart.resolve(original); });
    expect(onState).not.toHaveBeenCalled();
  });
});
