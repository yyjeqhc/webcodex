import { StrictMode } from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import type { DesktopState } from "../../models/topology";
import type { RuntimeCandidate, RuntimeSettings } from "../../models/runtime-shell";
import { RuntimePanel } from "./RuntimePanel";

const api = vi.hoisted(() => ({ runtimeSettings: vi.fn(), getState: vi.fn(), probeRuntime: vi.fn(), recheckRuntime: vi.fn(), switchRuntime: vi.fn() }));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
const state = { persistent_environment: "alpha", topology: { server: { kind: "local" }, runner: { kind: "local" } }, binaries: { directory: "/runtime", version: "0.5.0", git_commit: "current", source: "bundled" }, current_operation: null } as DesktopState;
const busy = { code: "desktop_operation_busy", message: "private contention detail", next_action: "Wait" };
const refreshing = { ...state, current_operation: { id: "refresh", kind: "runtime_refresh", phase: "running", started_at_ms: 1, cancellable: false } } as DesktopState;
const candidate: RuntimeCandidate = { candidate_id: "candidate-alpha", source: { kind: "custom", directory: "/candidate" }, selection_revision: 3, checked_at_ms: 1, directory: "/candidate", binaries: [], compatibility: "compatible", build_alignment: "exact", advisories: [], error_code: null, fingerprint: "fingerprint" };
const settings: RuntimeSettings = { source: { kind: "bundled" }, selection_revision: 3, desktop_contract: { min_generation: 1, max_generation: 1 }, selected: null, candidate: null, previous_source: null, last_switch: null, unavailable_code: null, active_jobs: 0, can_switch: true, switch_unavailable_reason: null };
const custom = (directory: string): RuntimeSettings => ({ ...settings, source: { kind: "custom", directory } });
const outcome = { outcome: "activated", reason_code: null, rollback_reason_code: null, selection_revision: 4, restart_required: false };
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
function view(value = state, onState = vi.fn()) { return <LocaleProvider><DesktopMantineProvider><RuntimePanel state={value} onState={onState} /></DesktopMantineProvider></LocaleProvider>; }
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.runtimeSettings.mockResolvedValue(settings); api.getState.mockResolvedValue(state);
  api.probeRuntime.mockResolvedValue({ ...settings, candidate }); api.recheckRuntime.mockResolvedValue(settings);
  api.switchRuntime.mockResolvedValue(outcome); dialog.open.mockResolvedValue("/candidate");
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("Runtime observation and action lifetimes", () => {
  it("recovers a passive refresh collision without probing or switching", async () => {
    vi.useFakeTimers(); api.getState.mockResolvedValue(refreshing);
    api.runtimeSettings.mockRejectedValueOnce(busy).mockResolvedValueOnce(settings);
    render(view()); await act(async () => {});
    expect(api.runtimeSettings).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
    expect(screen.getByRole("button", { name: "Select Runtime folder…" })).toBeEnabled();
    expect(api.runtimeSettings).toHaveBeenCalledTimes(2);
    expect(api.probeRuntime).not.toHaveBeenCalled(); expect(api.switchRuntime).not.toHaveBeenCalled();
  });

  it.each(["unmount", "identity"])("cancels a scheduled observation retry on %s", async change => {
    vi.useFakeTimers(); api.getState.mockResolvedValue(refreshing);
    api.runtimeSettings.mockRejectedValueOnce(busy).mockResolvedValue(settings);
    const rendered = render(view()); await act(async () => {});
    if (change === "unmount") rendered.unmount();
    else rendered.rerender(view({ ...state, persistent_environment: "beta" }));
    await act(async () => { await vi.advanceTimersByTimeAsync(1_000); });
    expect(api.runtimeSettings).toHaveBeenCalledTimes(change === "unmount" ? 1 : 2);
  });

  it("does not let a slow initial observation replace a newer explicit recheck", async () => {
    const first = deferred<RuntimeSettings>(); api.runtimeSettings.mockReturnValueOnce(first.promise);
    api.recheckRuntime.mockResolvedValue(custom("/new"));
    render(view());
    fireEvent.click(screen.getByRole("button", { name: "Recheck Runtime", hidden: true }));
    await screen.findByText("/new");
    await act(async () => first.resolve(custom("/old")));
    expect(screen.getByText("/new")).toBeVisible(); expect(screen.queryByText("/old")).not.toBeInTheDocument();
  });

  it("keeps StrictMode's discarded initial request invalid after the next effect starts", async () => {
    const first = deferred<RuntimeSettings>();
    api.runtimeSettings.mockReturnValueOnce(first.promise).mockResolvedValueOnce(custom("/current"));
    render(<StrictMode>{view()}</StrictMode>);
    await screen.findByText("/current");
    await act(async () => first.resolve(custom("/discarded")));
    expect(screen.queryByText("/discarded")).not.toBeInTheDocument();
    expect(screen.getByText("/current")).toBeVisible();
  });

  it.each(["unmount", "identity"])("abandons a late folder picker after %s", async change => {
    const picker = deferred<string | null>(); dialog.open.mockReturnValueOnce(picker.promise);
    const rendered = render(view());
    fireEvent.click(await screen.findByRole("button", { name: "Select Runtime folder…" }));
    if (change === "unmount") rendered.unmount();
    else rendered.rerender(view({ ...state, persistent_environment: "beta" }));
    await act(async () => picker.resolve("/stale-folder"));
    expect(api.probeRuntime).not.toHaveBeenCalled(); expect(api.switchRuntime).not.toHaveBeenCalled();
  });

  it.each([0, 2])("abandons an old probe with %s active Jobs before switching or asking for consent", async activeJobs => {
    const probe = deferred<RuntimeSettings>(); api.probeRuntime.mockReturnValueOnce(probe.promise);
    const rendered = render(view());
    fireEvent.click(await screen.findByRole("button", { name: "Use bundled Runtime" }));
    rendered.rerender(view({ ...state, persistent_environment: "beta" }));
    await act(async () => probe.resolve({ ...settings, candidate, active_jobs: activeJobs }));
    expect(api.switchRuntime).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText("/candidate")).not.toBeInTheDocument();
  });

  it("preserves same-target consent through a refresh but clears it on target change", async () => {
    api.probeRuntime.mockResolvedValue({ ...settings, candidate, active_jobs: 2 });
    const rendered = render(view());
    fireEvent.click(await screen.findByRole("button", { name: "Use bundled Runtime" }));
    await screen.findByRole("dialog", { name: "Runtime restart warning" });
    rendered.rerender(view(refreshing));
    expect(screen.getByRole("dialog", { name: "Runtime restart warning" })).toBeInTheDocument();
    rendered.rerender(view({ ...state, persistent_environment: "beta" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.switchRuntime).not.toHaveBeenCalled();
    await act(async () => {});
  });

  it.each(["unmount", "identity"])("does not publish a late state readback after %s", async change => {
    const readback = deferred<DesktopState>(); api.getState.mockReturnValueOnce(readback.promise);
    const onState = vi.fn(); const rendered = render(view(state, onState));
    fireEvent.click(await screen.findByRole("button", { name: "Use bundled Runtime" }));
    await act(async () => {});
    expect(api.switchRuntime).toHaveBeenCalledOnce(); expect(api.getState).toHaveBeenCalledOnce();
    if (change === "unmount") rendered.unmount();
    else rendered.rerender(view({ ...state, persistent_environment: "beta" }, onState));
    await act(async () => readback.resolve(state));
    expect(onState).not.toHaveBeenCalled();
    expect(api.runtimeSettings).toHaveBeenCalledTimes(change === "unmount" ? 1 : 2);
  });

  it("retries a post-switch observation without replaying the switch", async () => {
    vi.useFakeTimers();
    api.runtimeSettings.mockResolvedValueOnce(settings).mockRejectedValueOnce(busy).mockResolvedValueOnce(custom("/activated"));
    api.getState.mockResolvedValue(refreshing);
    render(view()); await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Use bundled Runtime" }));
    await act(async () => {});
    expect(api.runtimeSettings).toHaveBeenCalledTimes(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
    expect(screen.getByText("/activated")).toBeVisible();
    expect(api.switchRuntime).toHaveBeenCalledOnce(); expect(api.probeRuntime).toHaveBeenCalledOnce();
  });

  it("does not retry a native probe that reports contention", async () => {
    api.probeRuntime.mockRejectedValue(busy);
    render(view()); fireEvent.click(await screen.findByRole("button", { name: "Use bundled Runtime" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("desktop_operation_busy");
    expect(api.probeRuntime).toHaveBeenCalledOnce(); expect(api.switchRuntime).not.toHaveBeenCalled();
    expect(api.getState).not.toHaveBeenCalled();
  });
});
