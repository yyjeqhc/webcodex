import { useState } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../i18n/locale";
import { DesktopMantineProvider } from "../components/DesktopMantineProvider";
import { RuntimeConsolePanel } from "./console/RuntimeConsolePanel";
import { LocalServicesPanel } from "./settings/LocalServicesPanel";
import { Sidebar } from "../components/Sidebar";
import { RuntimePanel } from "./settings/RuntimePanel";
import { DiagnosticsPanel } from "./settings/DiagnosticsPanel";
import { SettingsPanel } from "./settings/SettingsPanel";
import { ContinuationFacts } from "./activity/ContinuationFacts";
import { continuationFromWindow } from "./activity/window-evidence";
import { AccentPicker } from "../components/AccentPicker";
import { useRuntimeUpdates } from "../hooks/useRuntimeUpdates";
import { AboutPanel, UpdateBanner } from "./settings/AboutPanel";
import type { DesktopState } from "../models/topology";
import type { DiagnosticSnapshot, RuntimeSettings, RuntimeCandidate } from "../models/runtime-shell";
import type { WindowDetail } from "../models/workspace";

const api = vi.hoisted(() => ({ updateTunnelProxy: vi.fn(), runnerSettings: vi.fn(), runtimeSettings: vi.fn(), probeRuntime: vi.fn(), recheckRuntime: vi.fn(), switchRuntime: vi.fn(), getState: vi.fn(), diagnostics: vi.fn(), setToolRequestTracing: vi.fn(), copyDiagnosticReport: vi.fn(), copyRuntimeConsoleCredential: vi.fn(), exportSupportBundle: vi.fn(), openDiagnosticResource: vi.fn(), restorePreviousConfiguration: vi.fn(), environmentServiceAction: vi.fn(), repairEnvironmentUserCredential: vi.fn(), computerPermissions: vi.fn(), desktopBuildInfo: vi.fn(), getLaunchAtLogin: vi.fn(), setLaunchAtLogin: vi.fn(), checkForUpdates: vi.fn(), remindUpdateLater: vi.fn(), openLatestRelease: vi.fn() }));
const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
const state = {
  project: { path: "/fixture/project", runtime_project_id: "agent:fixture:project", is_git_repository: true, allowed_root: "/fixture/project" },
  topology: { experience: "full", server: { kind: "local" }, exposure: { kind: "none" } }, current_operation: null,
  readiness: { runtime_ready: true, server: "ready", runner: "ready", exposure: "disabled", project: "ready", next_action: "ready", next_action_kind: "none" },
  chatgpt_activity: { observed: false }, connections: { profiles: [], running: 0, needs_attention: 0, config_error: false },
  binaries: { directory: "/fixture/runtime", version: "0.4.1", git_commit: "aaaaaaa", source: "Bundled" }, tunnel_proxy: { mode: "system",custom_proxy: null, server_proxy: null },
  openai_tunnel_config: { source: "missing", configuration_error: false }, powershell_runtime: null,
} as unknown as DesktopState;
const build = (binary: string, index = 0) => ({ schema_version: 1, binary, version: `0.${index + 4}.0`, git_commit: `${index + 1}`.repeat(12), git_dirty: index === 2, built_at: "100", target: "aarch64-apple-darwin", architecture: "aarch64", desktop_runtime_contract: { min_generation: 1, max_generation: 1 } });
const candidate: RuntimeCandidate = { candidate_id: "candidate-fence", source: { kind: "custom", directory: "/fixture/custom" }, selection_revision: 3, checked_at_ms: 100, directory: "/fixture/custom", compatibility: "compatible", build_alignment: "different_version", advisories: ["different_source_revisions", "dirty_build_operator_responsibility"], error_code: null, fingerprint: "hash", binaries: ["webcodex", "webcodex-server", "webcodex-runner"].map((name, index) => ({ name, present: true, startup_check: "passed", metadata: build(name, index), sha256: "hash", error_code: null, diagnostics: null })) };
const settings: RuntimeSettings = { source: { kind: "bundled" }, selection_revision: 3, desktop_contract: { min_generation: 1, max_generation: 1 }, selected: { ...candidate, source: { kind: "bundled" }, candidate_id: "" }, candidate: null, previous_source: null, last_switch: null, unavailable_code: null, active_jobs: 0, can_switch: true, switch_unavailable_reason: null };
const diagnostic = { schema_version: 1, observed_at_ms: Date.now(), trace: { mode: "off", effective_mode: "off", revision: "env-fence", available: true, restart_required: false, can_restart: true, error_code: null }, configuration: { reason_code: null, backup_available: true, primary_fingerprint: "state-fence" }, resources: ["runtime_console", "app_data"], can_copy_console_credential: true, credential_copy_fence: "identity-fence", report: { schema_version: 1, desktop: build("webcodex-desktop"), last_webcodex_call: null }, markdown: "# Safe report" } as DiagnosticSnapshot;
function wrap(child: React.ReactNode) { return <LocaleProvider><DesktopMantineProvider>{child}</DesktopMantineProvider></LocaleProvider>; }
beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.runnerSettings.mockResolvedValue({ target: { client_id: "fixture", config_path: "/fixture/runner.toml", server_url: "http://127.0.0.1:1" }, paths: { instruction_files: [], skill_roots: [] }, file_access: { configured_roots: [], effective_roots: ["/Users/fixture"], using_default_roots: true, allow_cwd_anywhere: false }, plugin_ids: [], can_restart: true });
  api.runtimeSettings.mockResolvedValue(structuredClone(settings)); api.getState.mockResolvedValue(state);
  api.probeRuntime.mockResolvedValue({ ...settings, candidate }); api.recheckRuntime.mockResolvedValue(settings);
  api.switchRuntime.mockResolvedValue({ outcome: "activated", reason_code: null, rollback_reason_code: null, selection_revision: 4, restart_required: false });
  api.diagnostics.mockResolvedValue(structuredClone(diagnostic)); api.setToolRequestTracing.mockResolvedValue({ ...diagnostic.trace, mode: "full", restart_required: true });
  api.computerPermissions.mockResolvedValue({ supported: true, foreground: true, execution_process: "WebCodex Runner", execution_path: "/fixture/runtime/webcodex-runner", runner_accessibility: "unknown", runner_screen_recording: "unknown", desktop_accessibility: true, desktop_screen_recording: true });
  api.openDiagnosticResource.mockResolvedValue(undefined);
  api.getLaunchAtLogin.mockResolvedValue(false); api.desktopBuildInfo.mockResolvedValue(build("webcodex-desktop"));
  dialog.open.mockResolvedValue("/fixture/custom"); dialog.save.mockResolvedValue(null);
});

describe("Runtime candidate and ownership semantics", () => {
  it("distinguishes present, missing and unconfirmed files from failed startup without claiming incompatibility", async () => {
    const failed: RuntimeCandidate = { ...candidate, compatibility: "unknown", error_code: "webcodex_command_failed", binaries: [
      { ...candidate.binaries[0], metadata: null, startup_check: "failed", error_code: "webcodex_command_failed", diagnostics: { exit_code: -1073741790, io_kind: null } },
      { ...candidate.binaries[1], metadata: null, present: false, startup_check: "not_checked", error_code: "binary_missing" },
      { ...candidate.binaries[2], metadata: null, present: null, startup_check: "not_checked", error_code: "runtime_file_unreadable", diagnostics: { exit_code: null, io_kind: "PermissionDenied" } },
    ] };
    api.runtimeSettings.mockResolvedValue({ ...settings, selected: failed, candidate: failed, unavailable_code: "webcodex_command_failed" });
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    await screen.findAllByRole("heading", { name: "webcodex" });
    const panel = (await screen.findByRole("heading", { name: "Current Runtime" })).parentElement!;
    const articles = panel.querySelectorAll(".runtime-binary-list article");
    expect(within(articles[0] as HTMLElement).getByText("Present")).toBeVisible();
    expect(within(articles[0] as HTMLElement).getByText("Failed")).toBeVisible();
    expect(within(articles[0] as HTMLElement).getByText(/The system denied access/)).toBeVisible();
    expect(within(articles[0] as HTMLElement).getByText(/0xC0000022/)).toBeInTheDocument();
    expect(within(articles[1] as HTMLElement).getByText("Missing")).toBeVisible();
    expect(within(articles[1] as HTMLElement).getByText("Not checked")).toBeVisible();
    expect(within(articles[2] as HTMLElement).getByText("Unconfirmed")).toBeVisible();
    expect(within(articles[2] as HTMLElement).queryByText("Missing")).not.toBeInTheDocument();
    expect(screen.queryByText("Executable")).not.toBeInTheDocument();
    expect(screen.queryByText("Passed")).not.toBeInTheDocument();
    expect(screen.queryByText("Incompatible")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use this Runtime" })).toBeDisabled();
    expect(api.switchRuntime).not.toHaveBeenCalled();
    expect(api.probeRuntime).not.toHaveBeenCalled();
  });
  it("explains failed startup in Traditional Chinese without hiding the file checks", async () => {
    localStorage.setItem("webcodex.desktop.locale", "zh-TW");
    api.runtimeSettings.mockResolvedValue({ ...settings, selected: { ...candidate, compatibility: "unknown", error_code: "webcodex_command_start_failed", binaries: [{ ...candidate.binaries[0], startup_check: "failed", metadata: null, error_code: "webcodex_command_start_failed", diagnostics: { exit_code: null, io_kind: "PermissionDenied" } }] } });
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    expect(await screen.findByText("啟動檢查")).toBeVisible();
    expect(screen.getByText("失敗")).toBeVisible();
    expect(screen.getByText(/系統拒絕了存取/)).toBeVisible();
    expect(screen.getByText(/相容性仍未確認/)).toBeVisible();
  });
  it("previews mixed revisions/versions without mutation and only activates after explicit confirmation", async () => {
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    await screen.findByRole("heading", { name: "Current Runtime" });
    fireEvent.click(await screen.findByRole("button", { name: "Select Runtime folder…" }));
    const previewHeading = await screen.findByRole("heading", { name: "Candidate Runtime" });
    const preview = previewHeading.parentElement as HTMLElement;
    expect(api.switchRuntime).not.toHaveBeenCalled();
    expect(within(preview).getByText("Compatible")).toBeInTheDocument();
    expect(within(preview).getByText("Different versions")).toBeInTheDocument();
    fireEvent.click(within(preview).getByRole("button", { name: "Use this Runtime" }));
    await waitFor(() => expect(api.switchRuntime).toHaveBeenCalledExactlyOnceWith({ candidate_id: "candidate-fence", expected_selection_revision: 3, confirm_interrupt: false }));
  });
  it("rejects incompatible candidates while the selected Runtime remains visible", async () => {
    api.probeRuntime.mockResolvedValue({ ...settings, candidate: { ...candidate, compatibility: "incompatible", error_code: "runtime_contract_incompatible" } });
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Select Runtime folder…" }));
    const previewHeading = await screen.findByRole("heading", { name: "Candidate Runtime" });
    const preview = previewHeading.parentElement as HTMLElement;
    expect(within(preview).getByRole("button", { name: "Use this Runtime" })).toBeDisabled();
    expect(api.switchRuntime).not.toHaveBeenCalled(); expect(screen.getByText("Bundled")).toBeInTheDocument();
  });
  it("warns for active or unknown Jobs and supports cancellation before any switch", async () => {
    api.probeRuntime.mockResolvedValue({ ...settings, candidate, active_jobs: 2 });
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Use bundled Runtime" }));
    fireEvent.click(await screen.findByRole("button", { name: "Use this Runtime" }));
    const confirm = await screen.findByRole("dialog", { name: "Runtime restart warning" });
    expect(within(confirm).getByText("Active Jobs: 2")).toBeInTheDocument(); expect(api.switchRuntime).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(api.switchRuntime).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Use this Runtime" }));
    const switchButton = await screen.findByRole("button", { name: "Switch anyway" });
    await waitFor(() => expect(switchButton).toBeEnabled());
    fireEvent.click(switchButton);
    await waitFor(() => expect(api.switchRuntime).toHaveBeenCalledWith(expect.objectContaining({ confirm_interrupt: true })));
  });
  it("reports rolled-back activation rather than claiming a successful switch", async () => {
    const outcome = { outcome: "rolled_back", reason_code: "server_start_failed", rollback_reason_code: null, selection_revision: 3, restart_required: false };
    api.runtimeSettings.mockResolvedValue({ ...settings, last_switch: outcome });
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    expect(await screen.findByText("Previous Runtime restored")).toBeInTheDocument();
    expect(screen.queryByText("Runtime activated")).not.toBeInTheDocument();
  });
});

it("links users from About to issues, source builds, and contribution guidance", async () => {
  render(wrap(<AboutPanel state={state} />));
  expect(await screen.findByRole("heading", { name: "About WebCodex" })).toBeInTheDocument();
  for (const [label, resource] of [
    ["Report issue", "report_issue"],
    ["Build from source", "desktop_development"],
    ["Contribute", "contributing"],
  ] as const) {
    fireEvent.click(screen.getByRole("button", { name: label }));
    await waitFor(() => expect(api.openDiagnosticResource).toHaveBeenCalledWith(resource));
  }
});

describe("Diagnostics are explicit and secret-free", () => {
  it("shows rejected Console credential copies inside the confirmation without repeating the action", async () => {
    api.copyRuntimeConsoleCredential.mockRejectedValueOnce({ code: "diagnostic_identity_changed", message: "Identity changed", next_action: "Refresh" });
    render(wrap(<RuntimeConsolePanel state={state} onSettings={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Copy Runtime Console credential" }));
    const confirmation = await screen.findByRole("dialog", { name: "Sensitive clipboard action" });
    fireEvent.click(within(confirmation).getByRole("button", { name: "Copy sensitive credential" }));
    expect(await within(confirmation).findByRole("alert")).toBeVisible();
    expect(api.copyRuntimeConsoleCredential).toHaveBeenCalledExactlyOnceWith("identity-fence");
    expect(screen.queryByText("Copied")).not.toBeInTheDocument();
  });
  it("explains unavailable Console access and sends a stopped Server to service settings", async () => {
    const onSettings = vi.fn();
    const view = render(wrap(<RuntimeConsolePanel state={{ ...state, readiness: { ...state.readiness, server: "stopped" } }} onSettings={onSettings} />));
    await screen.findByText(/The Server is not ready/);
    expect(screen.getByRole("button", { name: "Open in browser" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Runtime & services" }));
    expect(onSettings).toHaveBeenCalledOnce();
    expect(api.openDiagnosticResource).not.toHaveBeenCalled();
    api.diagnostics.mockResolvedValue({ ...diagnostic, resources: [], can_copy_console_credential: false, credential_copy_fence: null });
    view.rerender(wrap(<RuntimeConsolePanel state={{ ...state, topology: { ...state.topology!, server: { kind: "remote", url: "https://fixture-server.test" } } }} onSettings={onSettings} />));
    await screen.findByText(/For a remote Server/);
    expect(screen.queryByRole("button", { name: "Copy Runtime Console credential" })).not.toBeInTheDocument();
  });
  it("rejects an older Console observation after the Server identity changes", async () => {
    let finish!: (value: DiagnosticSnapshot) => void;
    api.diagnostics.mockReturnValueOnce(new Promise<DiagnosticSnapshot>(resolve => { finish = resolve; }));
    const view = render(wrap(<RuntimeConsolePanel state={state} onSettings={vi.fn()} />));
    api.diagnostics.mockResolvedValue({ ...diagnostic, resources: [], can_copy_console_credential: false, credential_copy_fence: null });
    view.rerender(wrap(<RuntimeConsolePanel state={{ ...state, topology: { ...state.topology!, server: { kind: "remote", url: "https://fixture-server.test" } } }} onSettings={vi.fn()} />));
    await screen.findByText(/For a remote Server/);
    await act(async () => finish(diagnostic));
    expect(screen.getByRole("button", { name: "Open in browser" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Copy Runtime Console credential" })).not.toBeInTheDocument();
  });
  it("uses the exact installed environment for local service controls without applying effects on load", async () => {
    const persistent = { ...state, persistent_environment: "env-fixture", topology: { ...state.topology!, runner: { kind: "local" as const } } };
    api.environmentServiceAction.mockResolvedValue(persistent);
    render(wrap(<LocalServicesPanel state={persistent} onState={vi.fn()} />));
    expect(api.environmentServiceAction).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Runner" }));
    await waitFor(() => expect(api.environmentServiceAction).toHaveBeenCalledExactlyOnceWith({ environmentId: "env-fixture", component: "runner", action: "restart" }));
  });
  it("keeps program upgrades separate from tracing edits for an owned Environment", async () => {
    api.runtimeSettings.mockResolvedValue({ ...settings, can_switch: false, switch_unavailable_reason: "persistent_runtime_upgrade_required" });
    const onUpdates = vi.fn();
    render(wrap(<><RuntimePanel state={{ ...state, persistent_environment: "env-fixture" }} onState={vi.fn()} onUpdates={onUpdates} /><DiagnosticsPanel state={{ ...state, persistent_environment: "env-fixture" }} onState={vi.fn()} /></>));
    await screen.findByText(/Update them through About & updates/);
    expect(screen.queryByRole("button", { name: "Select Runtime folder…" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "About & updates" }));
    expect(onUpdates).toHaveBeenCalledOnce();
    const selector = await screen.findByRole("combobox", { name: "Tool Request Tracing" });
    expect(selector).toBeEnabled();
    fireEvent.change(selector, { target: { value: "metadata" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.setToolRequestTracing).toHaveBeenCalledExactlyOnceWith({ mode: "metadata", expected_revision: "env-fence", confirm_full: false, restart: false, confirm_interrupt: false }));
  });
  it("shows remote tracing as an observation with specific guidance instead of disabled controls", async () => {
    api.diagnostics.mockResolvedValue({ ...diagnostic, trace: { ...diagnostic.trace, mode: "off", effective_mode: "metadata", available: false, can_restart: false, error_code: "server_not_owned" } });
    const remote = { ...state, persistent_environment: "joined-environment", topology: { ...state.topology!, server: { kind: "remote" as const, url: "https://fixture-server.test" } } };
    render(wrap(<DiagnosticsPanel state={remote} onState={vi.fn()} />));
    await screen.findByText(/Change its recording setting on the Server's machine/);
    expect(screen.queryByRole("combobox", { name: "Tool Request Tracing" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(within(screen.getByText("Server recording now").parentElement!).getByText("Timing & status only")).toBeVisible();
    expect(screen.queryByText("Saved recording setting")).not.toBeInTheDocument();
    expect(api.setToolRequestTracing).not.toHaveBeenCalled();
  });
  it("explains unconfirmed mode without treating it as recording being off", async () => {
    api.diagnostics.mockResolvedValue({ ...diagnostic, trace: { ...diagnostic.trace, effective_mode: null, restart_required: true } });
    render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} />));
    expect(await screen.findByText("Not yet confirmed")).toBeVisible();
    expect(screen.getByText(/This does not mean recording is off/)).toBeVisible();
    expect(screen.queryByText(/The saved setting is not active yet/)).not.toBeInTheDocument();
    expect(within(screen.getByText("Saved recording setting").parentElement!).getByText("Off")).toBeVisible();
    expect(api.setToolRequestTracing).not.toHaveBeenCalled();
  });
  it("keeps the running mode separate from the draft and a saved pending setting", async () => {
    api.setToolRequestTracing.mockResolvedValue({ ...diagnostic.trace, mode: "metadata", effective_mode: "off", restart_required: true });
    render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} />));
    fireEvent.change(await screen.findByRole("combobox", { name: "Tool Request Tracing" }), { target: { value: "metadata" } });
    expect(within(screen.getByText("Server recording now").parentElement!).getByText("Off")).toBeVisible();
    expect(within(screen.getByText("Saved recording setting").parentElement!).getByText("Off")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByText(/The saved setting is not active yet/);
    expect(within(screen.getByText("Saved recording setting").parentElement!).getByText("Timing & status only")).toBeVisible();
    expect(within(screen.getByText("Server recording now").parentElement!).getByText("Off")).toBeVisible();
  });
  it("confirms Server interruption before applying tracing through a service restart", async () => {
    const persistent = { ...state, persistent_environment: "env-fixture" };
    api.setToolRequestTracing.mockResolvedValue({ ...diagnostic.trace, mode: "metadata", effective_mode: "metadata" });
    render(wrap(<DiagnosticsPanel state={persistent} onState={vi.fn()} />));
    fireEvent.change(await screen.findByRole("combobox", { name: "Tool Request Tracing" }), { target: { value: "metadata" } });
    fireEvent.click(screen.getByRole("button", { name: "Save & Restart Server" }));
    const confirmation = await screen.findByRole("dialog", { name: "Runtime restart warning" });
    expect(api.setToolRequestTracing).not.toHaveBeenCalled();
    fireEvent.click(within(confirmation).getByRole("button", { name: "Save & Restart Server" }));
    await waitFor(() => expect(api.setToolRequestTracing).toHaveBeenCalledExactlyOnceWith({ mode: "metadata", expected_revision: "env-fence", confirm_full: false, restart: true, confirm_interrupt: true }));
  });
  it("links problem-specific guidance to the matching page and keeps reports visible", async () => {
    const onConnection = vi.fn(), onActivity = vi.fn(), onRuntime = vi.fn();
    render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} onConnection={onConnection} onActivity={onActivity} onRuntime={onRuntime} />));
    fireEvent.click(screen.getByRole("button", { name: "Connections" }));
    fireEvent.click(screen.getByRole("button", { name: "Activity" }));
    fireEvent.click(screen.getByRole("button", { name: "Runtime & services" }));
    expect(onConnection).toHaveBeenCalledOnce(); expect(onActivity).toHaveBeenCalledOnce(); expect(onRuntime).toHaveBeenCalledOnce();
    expect(await screen.findByRole("button", { name: "Copy Diagnostic Report" })).toBeVisible();
    expect(screen.getByText("Latest tool call details", { selector: "summary" }).parentElement).not.toHaveAttribute("open");
  });
  it("requires explicit Full trace confirmation, preserving the expected env revision", async () => {
    render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} />));
    fireEvent.change(await screen.findByRole("combobox", { name: "Tool Request Tracing" }), { target: { value: "full" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    const modal = await screen.findByRole("dialog", { name: "Confirm full tracing" });
    expect(api.setToolRequestTracing).not.toHaveBeenCalled();
    fireEvent.click(await screen.findByRole("button", { name: "Confirm full tracing" }));
    await waitFor(() => expect(api.setToolRequestTracing).toHaveBeenCalledExactlyOnceWith({ mode: "full", expected_revision: "env-fence", confirm_full: true, restart: false, confirm_interrupt: false }));
  });
  it("opens only the canonical Console resource and copies reports without returning credential bytes", async () => {
    render(wrap(<><RuntimeConsolePanel state={state} onSettings={vi.fn()} /><DiagnosticsPanel state={state} onState={vi.fn()} /></>));
    const openConsole = await screen.findByRole("button", { name: "Open in browser" });
    await waitFor(() => expect(openConsole).toBeEnabled());
    fireEvent.click(openConsole);
    await waitFor(() => expect(api.openDiagnosticResource).toHaveBeenCalledExactlyOnceWith("runtime_console"));
    fireEvent.click(screen.getByRole("button", { name: "Copy Diagnostic Report" }));
    await waitFor(() => expect(api.copyDiagnosticReport).toHaveBeenCalledOnce());
    expect(api.copyRuntimeConsoleCredential).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Copy Runtime Console credential" }));
    const modal = await screen.findByRole("dialog", { name: "Sensitive clipboard action" });
    fireEvent.click(within(modal).getByRole("button", { name: "Copy sensitive credential" }));
    await waitFor(() => expect(api.copyRuntimeConsoleCredential).toHaveBeenCalledExactlyOnceWith("identity-fence"));
    expect(screen.queryByText("wc_agent_")).not.toBeInTheDocument();
  });
});

it("keeps the six localized navigation labels and task-based Settings categories", async () => {
  localStorage.setItem("webcodex.desktop.locale", "zh-CN");
  render(wrap(<><Sidebar navigation="settings" setNavigation={vi.fn()} state={state} /><SettingsPanel state={state} onState={vi.fn()} onChangeSetup={vi.fn()} /></>));
  for (const label of ["首页", "项目", "活动", "连接", "扩展", "设置"]) expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Desktop 设置" })).toBeInTheDocument();
  expect(await screen.findByRole("checkbox", { name: "登录时启动 WebCodex" })).toBeInTheDocument();
  for (const label of ["文件访问与权限", "故障排查", "Runtime 与服务", "网络", "关于与更新"]) {
    expect(screen.getByRole("tab", { name: label })).toHaveAttribute("aria-selected", "false");
  }
  const diagnostics = screen.getByRole("tab", { name: "故障排查" });
  expect(diagnostics).toHaveAttribute("aria-controls", "desktop-settings-diagnostics");
  fireEvent.click(diagnostics);
  expect(diagnostics).toHaveAttribute("aria-selected", "true");
  const diagnosticsPanel = document.getElementById("desktop-settings-diagnostics");
  expect(diagnosticsPanel).toBeVisible();
  expect(diagnosticsPanel).not.toHaveAttribute("role", "region");
  expect(screen.queryByRole("checkbox", { name: "登录时启动 WebCodex" })).not.toBeInTheDocument();
  expect(await screen.findByRole("combobox", { name: "工具请求追踪" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "运行控制台" })).toHaveAttribute("aria-keyshortcuts", "Control+7 Meta+7");
  expect(screen.getByRole("button", { name: "复制诊断报告" })).toBeInTheDocument();

  const runtimeTab = screen.getByRole("tab", { name: "Runtime 与服务" });
  expect(runtimeTab).toHaveAttribute("aria-controls", "desktop-settings-runtime");
  fireEvent.click(runtimeTab);
  expect(runtimeTab).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByRole("button", { name: "选择 Runtime 文件夹…" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "使用内置 Runtime" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新检查 Runtime" })).toBeInTheDocument();
});

it("preserves network and tracing drafts while categories change without applying effects", async () => {
  render(wrap(<SettingsPanel state={state} onState={vi.fn()} />));
  expect(api.runtimeSettings).not.toHaveBeenCalled();
  expect(api.diagnostics).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("tab", { name: "Network" }));
  fireEvent.change(screen.getByLabelText("Proxy mode"), { target: { value: "custom" } });
  const proxy = screen.getByLabelText("Proxy URL");
  fireEvent.change(proxy, { target: { value: "http://127.0.0.1:7890" } });
  fireEvent.click(screen.getByRole("tab", { name: "Troubleshooting" }));
  const trace = await screen.findByRole("combobox", { name: "Tool Request Tracing" });
  fireEvent.change(trace, { target: { value: "metadata" } });
  fireEvent.click(screen.getByRole("tab", { name: "Network" }));
  expect(screen.getByLabelText("Proxy URL")).toHaveValue("http://127.0.0.1:7890");
  fireEvent.click(screen.getByRole("tab", { name: "Troubleshooting" }));
  expect(screen.getByRole("combobox", { name: "Tool Request Tracing" })).toHaveValue("metadata");
  expect(api.diagnostics).toHaveBeenCalledTimes(1);
  expect(api.setToolRequestTracing).not.toHaveBeenCalled();
  expect(api.updateTunnelProxy).not.toHaveBeenCalled();
  expect(api.switchRuntime).not.toHaveBeenCalled();
});

it("opens requested recovery categories and supports keyboard category navigation", async () => {
  const view = render(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="diagnostics" />));
  expect(screen.getByRole("tab", { name: "Troubleshooting" })).toHaveAttribute("aria-selected", "true");
  await screen.findByRole("combobox", { name: "Tool Request Tracing" });
  view.rerender(wrap(<SettingsPanel state={state} onState={vi.fn()} initialSection="runtime" />));
  const runtimeTab = screen.getByRole("tab", { name: "Runtime & services" });
  expect(runtimeTab).toHaveAttribute("aria-selected", "true");
  fireEvent.keyDown(runtimeTab, { key: "Home" });
  expect(screen.getByRole("tab", { name: "General" })).toHaveFocus();
  fireEvent.keyDown(screen.getByRole("tab", { name: "General" }), { key: "ArrowDown" });
  expect(screen.getByRole("tab", { name: "Files & permissions" })).toHaveFocus();
  expect(screen.getByRole("button", { name: "Add folder" })).toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("tab", { name: "Files & permissions" }), { key: "End" });
  expect(screen.getByRole("tab", { name: "About & updates" })).toHaveFocus();
});

it("explains configured Tunnel mode, detected proxy, and effective Auto connection path", async () => {
  const networkState = { ...state, tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true }, readiness: { ...state.readiness, exposure: "degraded" } } as DesktopState;
  render(wrap(<SettingsPanel state={networkState} onState={vi.fn()} />));
  const disclosure = screen.getByRole("tab", { name: "Network" });
  fireEvent.click(disclosure);
  const panel = document.getElementById("desktop-settings-network") as HTMLElement;
  const routing = panel.querySelector("[data-webcodex-tunnel-routing]") as HTMLElement;
  expect(within(routing).getByText("Configured mode")).toBeInTheDocument();
  expect(within(routing).getByText("Automatic (recommended)")).toBeInTheDocument();
  expect(within(routing).getByText("Detected proxy")).toBeInTheDocument();
  expect(within(routing).getAllByText("System proxy")).toHaveLength(2);
  expect(within(routing).getByText("Effective connection path")).toBeInTheDocument();
  expect(within(routing).getByText("Tunnel status")).toBeInTheDocument();
  expect(within(routing).getByText("degraded")).toBeInTheDocument();
});

it("shows an environment proxy as detected when Auto selects it", async () => {
  const networkState = { ...state, tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "environment", effective_proxy_present: true, system_proxy_detected: false } } as DesktopState;
  render(wrap(<SettingsPanel state={networkState} onState={vi.fn()} />));
  fireEvent.click(screen.getByRole("tab", { name: "Network" }));
  const panel = document.getElementById("desktop-settings-network") as HTMLElement;
  const routing = panel.querySelector("[data-webcodex-tunnel-routing]") as HTMLElement;
  expect(within(routing).getAllByText("Desktop proxy environment")).toHaveLength(2);
  expect(within(routing).queryByText("Not configured")).not.toBeInTheDocument();
});

it("renders factual handoff uncertainty rather than Host failure", () => {
  const detail = { active_count: 0, activity_truncated: false, activity: [{ meaningful: true, tool_name: "read_files", status: "succeeded", started_at_ms: 100, ended_at_ms: 200 }] } as WindowDetail;
  const value = continuationFromWindow(detail, 400);
  render(wrap(<ContinuationFacts value={value} observedAt={Date.now()} />));
  expect(screen.getByText("Completed")).toBeInTheDocument();
  expect(screen.getByText("Response handoff not confirmed")).toBeInTheDocument();
  expect(screen.getByText("Not observed")).toBeInTheDocument();
  expect(screen.queryByText(/ChatGPT.*stuck/i)).not.toBeInTheDocument();
});

it("portals the accent palette with selected semantics and restores keyboard focus on Escape", async () => {
  function Palette() { const [color, setColor] = useState("#2563eb"); return <AccentPicker color={color} onChange={setColor} compact />; }
  render(wrap(<div style={{ overflow: "hidden", width: 80 }}><Palette /></div>));
  const trigger = screen.getByLabelText("Accent color", { selector: "summary" });
  fireEvent.click(trigger);
  const palette = await screen.findByRole("dialog", { name: "Accent color" });
  expect(palette.parentElement).toBe(document.body);
  expect(within(palette).getByRole("button", { name: "Blue" })).toHaveAttribute("aria-pressed", "true");
  fireEvent.click(within(palette).getByRole("button", { name: "Violet" }));
  expect(within(palette).getByRole("button", { name: "Violet" })).toHaveAttribute("aria-pressed", "true");
  fireEvent.keyDown(document, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});

it("a failed startup update check never changes Runtime readiness or opens a modal", async () => {
  vi.useFakeTimers(); api.checkForUpdates.mockRejectedValue(new Error("offline"));
  function Harness() { const updates = useRuntimeUpdates(true); return <><h1>WebCodex Ready</h1><UpdateBanner updates={updates} /><button onClick={() => void updates.check()}>Manual update check</button>{updates.manualError && <p>Manual check unavailable</p>}</>; }
  const view = render(wrap(<Harness />));
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(api.checkForUpdates).toHaveBeenCalledWith(false);
  expect(screen.getByRole("heading", { name: "WebCodex Ready" })).toBeInTheDocument();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.queryByText("Manual check unavailable")).not.toBeInTheDocument();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Manual update check" })); });
  expect(screen.getByText("Manual check unavailable")).toBeInTheDocument();
  view.unmount(); vi.useRealTimers();
});


it("does not treat an incoming request gap as proof of a later meaningful call", () => {
  const detail = { active_count: 0, activity_truncated: false, activity: [{ meaningful: true, tool_name: "read_files", status: "succeeded", request_observed_at_ms: 100, response_handed_at_ms: 200, started_at_ms: 100, ended_at_ms: 190, next_call_gap_ms: 5000 }] } as WindowDetail;
  const value = continuationFromWindow(detail, 400);
  expect(value?.next_meaningful_call).toBe("not_observed");
  expect(value?.previous_response_gap_ms).toBe(5000);
  expect(value?.next_call_gap_ms).toBeNull();
});
