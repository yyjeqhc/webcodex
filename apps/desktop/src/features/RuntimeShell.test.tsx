import { useState } from "react";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../i18n/locale";
import { DesktopMantineProvider } from "../components/DesktopMantineProvider";
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

const api = vi.hoisted(() => ({ runnerSettings: vi.fn(), runtimeSettings: vi.fn(), probeRuntime: vi.fn(), recheckRuntime: vi.fn(), switchRuntime: vi.fn(), getState: vi.fn(), diagnostics: vi.fn(), setToolRequestTracing: vi.fn(), copyDiagnosticReport: vi.fn(), copyRuntimeConsoleCredential: vi.fn(), exportSupportBundle: vi.fn(), openDiagnosticResource: vi.fn(), restorePreviousConfiguration: vi.fn(), computerPermissions: vi.fn(), desktopBuildInfo: vi.fn(), getLaunchAtLogin: vi.fn(), setLaunchAtLogin: vi.fn(), checkForUpdates: vi.fn(), remindUpdateLater: vi.fn(), openLatestRelease: vi.fn() }));
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
const candidate: RuntimeCandidate = { candidate_id: "candidate-fence", source: { kind: "custom", directory: "/fixture/custom" }, selection_revision: 3, checked_at_ms: 100, directory: "/fixture/custom", compatibility: "compatible", build_alignment: "different_version", advisories: ["different_source_revisions", "dirty_build_operator_responsibility"], error_code: null, fingerprint: "hash", binaries: ["webcodex", "webcodex-server", "webcodex-runner"].map((name, index) => ({ name, present: true, executable: true, metadata: build(name, index), sha256: "hash", error_code: null })) };
const settings: RuntimeSettings = { source: { kind: "bundled" }, selection_revision: 3, desktop_contract: { min_generation: 1, max_generation: 1 }, selected: { ...candidate, source: { kind: "bundled" }, candidate_id: "" }, candidate: null, previous_source: null, last_switch: null, unavailable_code: null, active_jobs: 0, can_switch: true, switch_unavailable_reason: null };
const diagnostic = { schema_version: 1, observed_at_ms: Date.now(), trace: { mode: "off", effective_mode: "off", revision: "env-fence", available: true, restart_required: false, can_restart: true, error_code: null }, configuration: { reason_code: null, backup_available: true, primary_fingerprint: "state-fence" }, resources: ["runtime_console", "app_data"], can_copy_console_credential: true, credential_copy_fence: "identity-fence", report: { schema_version: 1, desktop: build("webcodex-desktop"), last_webcodex_call: null }, markdown: "# Safe report" } as DiagnosticSnapshot;
function wrap(child: React.ReactNode) { return <LocaleProvider><DesktopMantineProvider>{child}</DesktopMantineProvider></LocaleProvider>; }
beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.runnerSettings.mockResolvedValue({ target: { client_id: "fixture", config_path: "/fixture/runner.toml", server_url: "http://127.0.0.1:1" }, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: [], can_restart: true });
  api.runtimeSettings.mockResolvedValue(structuredClone(settings)); api.getState.mockResolvedValue(state);
  api.probeRuntime.mockResolvedValue({ ...settings, candidate }); api.recheckRuntime.mockResolvedValue(settings);
  api.switchRuntime.mockResolvedValue({ outcome: "activated", reason_code: null, rollback_reason_code: null, selection_revision: 4, restart_required: false });
  api.diagnostics.mockResolvedValue(structuredClone(diagnostic)); api.setToolRequestTracing.mockResolvedValue({ ...diagnostic.trace, mode: "full", restart_required: true });
  api.computerPermissions.mockResolvedValue({ supported: true, foreground: true, desktop_accessibility: true, desktop_screen_recording: true });
  api.openDiagnosticResource.mockResolvedValue(undefined);
  api.getLaunchAtLogin.mockResolvedValue(false); api.desktopBuildInfo.mockResolvedValue(build("webcodex-desktop"));
  dialog.open.mockResolvedValue("/fixture/custom"); dialog.save.mockResolvedValue(null);
});

describe("Runtime candidate and ownership semantics", () => {
  it("previews mixed revisions/versions without mutation and only activates after explicit confirmation", async () => {
    render(wrap(<RuntimePanel state={state} onState={vi.fn()} />));
    await screen.findByRole("heading", { name: "Current Runtime" });
    fireEvent.click(screen.getByRole("button", { name: "Select Runtime folder…" }));
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
  expect(await screen.findByText("Found a problem? Issues and pull requests are welcome. You can build current main from source and test a fix locally.")).toBeInTheDocument();
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
    render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Open Runtime Console" }));
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

it("keeps the six localized navigation labels and semantically pressable Settings disclosures", async () => {
  localStorage.setItem("webcodex.desktop.locale", "zh-CN");
  render(wrap(<><Sidebar navigation="settings" setNavigation={vi.fn()} state={state} /><SettingsPanel state={state} onState={vi.fn()} onChangeSetup={vi.fn()} /></>));
  for (const label of ["首页", "项目", "活动", "连接", "扩展", "设置"]) expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Desktop 设置" })).toBeInTheDocument();
  expect(await screen.findByRole("checkbox", { name: "登录时启动 WebCodex" })).toBeInTheDocument();
  for (const label of ["故障排查", "Runtime", "网络", "高级"]) {
    expect(screen.getByRole("button", { name: label })).toHaveAttribute("aria-expanded", "false");
  }
  const disclosure = screen.getByRole("button", { name: "故障排查" });
  expect(disclosure.parentElement).toHaveAttribute("data-webcodex-page", "settings");
  expect(disclosure).toHaveAttribute("aria-controls", "desktop-settings-diagnostics");
  fireEvent.click(disclosure);
  expect(disclosure).toHaveAttribute("aria-expanded", "true");
  const diagnosticsPanel = document.getElementById("desktop-settings-diagnostics");
  expect(diagnosticsPanel).toBeInTheDocument();
  expect(diagnosticsPanel).not.toHaveAttribute("role", "region");
  expect(diagnosticsPanel?.parentElement).toHaveAttribute("data-webcodex-page", "settings");
  expect(await screen.findByRole("combobox", { name: "工具请求追踪" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "打开 Runtime Console" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "复制诊断报告" })).toBeInTheDocument();

  const runtimeDisclosure = screen.getByRole("button", { name: "Runtime" });
  expect(runtimeDisclosure).toHaveAttribute("aria-controls", "desktop-settings-runtime");
  fireEvent.click(runtimeDisclosure);
  expect(runtimeDisclosure).toHaveAttribute("aria-expanded", "true");
  expect(await screen.findByRole("button", { name: "选择 Runtime 文件夹…" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "使用内置 Runtime" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新检查 Runtime" })).toBeInTheDocument();
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
