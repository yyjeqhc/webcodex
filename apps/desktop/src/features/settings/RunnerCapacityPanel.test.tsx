import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState, RunnerSettings } from "../../models/topology";
import { RunnerCapacityPanel } from "./RunnerCapacityPanel";
const api = vi.hoisted(() => ({ runnerSettings: vi.fn(), saveRunnerJobConcurrency: vi.fn(), restartOwnedRunner: vi.fn() }));
const query = vi.hoisted(() => vi.fn());
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("../workspace/WorkspaceContext", () => ({ workspaceQuery: query }));
const target = { config_path: "/fixture/runner.toml", client_id: "local", server_url: "http://localhost:1" };
const state = { workspace_runner: target, topology: { runner: { kind: "local" }, server: { kind: "local" } }, readiness: { server: "ready", runner: "ready" }, current_operation: null } as DesktopState;
let settings: RunnerSettings;
let runner: { client_id: string; connected: boolean; status: string; jobs_running: number; jobs_queued: number; job_concurrency_limit: number };
const onState = vi.fn();
function view(value = state, active = true) { return <DesktopMantineProvider><LocaleProvider><RunnerCapacityPanel state={value} onState={onState} active={active} /></LocaleProvider></DesktopMantineProvider>; }
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  settings = { target, paths: { instruction_files: [], skill_roots: [] }, file_access: { configured_roots: [], effective_roots: [], using_default_roots: true, allow_cwd_anywhere: false }, plugin_ids: [], can_restart: true, max_concurrent_jobs: null };
  runner = { client_id: "local", connected: true, status: "online", jobs_running: 4, jobs_queued: 2, job_concurrency_limit: 4 };
  api.runnerSettings.mockImplementation(async () => structuredClone(settings));
  query.mockImplementation(async () => structuredClone(runner));
  api.saveRunnerJobConcurrency.mockImplementation(async (_target, _expected, limit) => { if (_expected !== settings.max_concurrent_jobs) throw { code: "runner_job_concurrency_invalid", message: "Saved value changed", next_action: "Reload settings" }; settings.max_concurrent_jobs = limit; return state; });
  api.restartOwnedRunner.mockResolvedValue(state);
});
afterEach(() => vi.restoreAllMocks());
describe("local Runner concurrency settings", () => {
  it("separates saved and effective limits without restarting while saving", async () => {
    render(view()); const input = await screen.findByRole("spinbutton", { name: "Maximum concurrent Jobs" });
    expect(input).toHaveValue(4); expect(screen.getByText("4 running · 2 queued · 4 max")).toBeInTheDocument();
    expect(screen.getByText("Saved limit: — · Default: 4")).toBeInTheDocument();
    expect(screen.queryByText("Saved limit is in effect.")).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "12" } }); fireEvent.click(screen.getByRole("button", { name: "Save for next restart" }));
    await waitFor(() => expect(api.saveRunnerJobConcurrency).toHaveBeenCalledExactlyOnceWith(target, null, 12));
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
    expect(await screen.findByText("Restart required to apply the saved limit.")).toBeInTheDocument();
    expect(screen.getByText("4 running · 2 queued · 4 max")).toBeInTheDocument();
  });
  it("requires an interruption warning and supports Cancel", async () => {
    settings.max_concurrent_jobs = 12; render(view());
    fireEvent.click(await screen.findByRole("button", { name: "Restart Runner…" }));
    let dialog = await screen.findByRole("dialog"); expect(within(dialog).getByText(/browser sessions and handoffs/)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" })); expect(api.restartOwnedRunner).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Restart Runner…" })); dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Restart Runner now" }));
    await waitFor(() => expect(api.restartOwnedRunner).toHaveBeenCalledExactlyOnceWith(target));
    expect(await screen.findByText("Restart required to apply the saved limit.")).toBeInTheDocument();
  });
  it("rejects invalid input and does not replay a failed save", async () => {
    render(view()); const input = await screen.findByRole("spinbutton");
    for (const value of ["", "0", "65", "1.5"]) { fireEvent.change(input, { target: { value } }); expect(screen.getByRole("button", { name: "Save for next restart" })).toBeDisabled(); }
    api.saveRunnerJobConcurrency.mockRejectedValue({ code: "runner_job_concurrency_invalid", message: "Saved value changed", next_action: "Reload settings" });
    fireEvent.change(input, { target: { value: "12" } }); fireEvent.click(screen.getByRole("button", { name: "Save for next restart" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Saved value changed");
    expect(api.saveRunnerJobConcurrency).toHaveBeenCalledTimes(1); expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });
  it.each(["offline", "stale", "unsupported", "failed"])("does not show live usage when %s", async mode => {
    if (mode === "offline") runner.connected = false;
    if (mode === "stale") runner.status = "stale";
    if (mode === "unsupported") runner.job_concurrency_limit = undefined as unknown as number;
    if (mode === "failed") query.mockRejectedValue(new Error("offline"));
    render(view()); await screen.findByRole("spinbutton");
    expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();
    expect(screen.queryByText("Saved limit is in effect.")).not.toBeInTheDocument();
  });
  it("shows authorized Runner Jobs without exposing stale inventory", async () => {
    query.mockImplementation(async () => ({
      ...runner,
      jobs: [{ job_id: "wc_job_test", kind: "run_process", status: "stop_requested",
        terminal: false, created_at: 1, started_at: 2, elapsed_secs: 14,
        project_id: "agent:local:demo" }],
      jobs_truncated: true,
    }));
    const mounted = render(view());
    expect(await screen.findByText(/wc_job_test/)).toBeInTheDocument();
    expect(screen.getByText(/stop_requested/)).toBeInTheDocument();
    expect(screen.getByText("Inventory incomplete; some Jobs are not shown.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Stop Job/ })).not.toBeInTheDocument();
    mounted.rerender(view({ ...state, readiness: { ...state.readiness, server: "stopped" } }));
    expect(screen.queryByText(/wc_job_test/)).not.toBeInTheDocument();
  });
  it("keeps unowned Runners read-only", async () => {
    settings.can_restart = false; render(view()); expect(await screen.findByRole("spinbutton")).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Restart Runner…" })).not.toBeInTheDocument();
  });
  it("fences an open confirmation when the selected Runner changes", async () => {
    const mounted = render(view()); fireEvent.click(await screen.findByRole("button", { name: "Restart Runner…" })); await screen.findByRole("dialog");
    mounted.rerender(view({ ...state, workspace_runner: { ...target, client_id: "other" } }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument()); expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });
  it("does not submit repeated saves while one is pending", async () => {
    let resolve!: (value: DesktopState) => void;
    api.saveRunnerJobConcurrency.mockReturnValue(new Promise(done => { resolve = done; }));
    render(view()); const input = await screen.findByRole("spinbutton");
    fireEvent.change(input, { target: { value: "12" } });
    const save = screen.getByRole("button", { name: "Save for next restart" });
    fireEvent.click(save); fireEvent.click(save);
    expect(api.saveRunnerJobConcurrency).toHaveBeenCalledTimes(1);
    await act(async () => resolve(state));
  });
  it("keeps the original edit fence during background refreshes", async () => {
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
    render(view()); const input = await screen.findByRole("spinbutton");
    fireEvent.change(input, { target: { value: "12" } });
    settings.max_concurrent_jobs = 8;
    fireEvent(document, new Event("visibilitychange"));
    await screen.findByText("Saved limit: 8 · Default: 4");
    fireEvent.click(screen.getByRole("button", { name: "Save for next restart" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Saved value changed");
    expect(api.saveRunnerJobConcurrency).toHaveBeenCalledExactlyOnceWith(target, null, 12);
  });
  it("recovers a conflicting save through an explicit reload and a new edit", async () => {
    render(view()); const input = await screen.findByRole("spinbutton");
    fireEvent.change(input, { target: { value: "12" } });
    settings.max_concurrent_jobs = 8;
    fireEvent.click(screen.getByRole("button", { name: "Save for next restart" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Saved value changed");
    await waitFor(() => expect(screen.getByRole("button", { name: "Refresh" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(input).toHaveValue(8));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    fireEvent.change(input, { target: { value: "12" } });
    fireEvent.click(screen.getByRole("button", { name: "Save for next restart" }));
    await waitFor(() => expect(api.saveRunnerJobConcurrency).toHaveBeenLastCalledWith(target, 8, 12));
    expect(api.saveRunnerJobConcurrency).toHaveBeenCalledTimes(2);
  });
  it("shows a settings read failure separately from a read-only Runner", async () => {
    api.runnerSettings.mockRejectedValue({ code: "runner_settings_unavailable", message: "Cannot read settings", next_action: "Refresh" });
    render(view());
    expect(await screen.findByRole("alert")).toHaveTextContent("could not be read or verified");
    expect(screen.queryByText(/Read-only/)).not.toBeInTheDocument();
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();
    api.runnerSettings.mockResolvedValue(settings);
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByRole("spinbutton")).toHaveValue(4);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
  it("rechecks the saved value before restarting", async () => {
    render(view()); fireEvent.click(await screen.findByRole("button", { name: "Restart Runner…" }));
    settings.max_concurrent_jobs = 8;
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Restart Runner now" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Runner settings changed");
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });
  it("does not retarget a late settings response or display another Runner's counts", async () => {
    settings.target = { ...target, client_id: "other" }; runner.client_id = "other";
    render(view()); expect(await screen.findByRole("alert")).toHaveTextContent("could not be read or verified");
    expect(screen.queryByRole("spinbutton")).not.toBeInTheDocument();
    expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();
  });
  it("marks observations stale when the Server goes offline", async () => {
    const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
    mounted.rerender(view({ ...state, readiness: { ...state.readiness, server: "stopped" } }));
    expect(screen.getByText("Capacity needs refresh.")).toBeInTheDocument();
    expect(screen.queryByText("Saved limit is in effect.")).not.toBeInTheDocument();
  });
  it("can still read saved local settings while the Server is offline", async () => {
    settings.max_concurrent_jobs = 12;
    query.mockRejectedValue(new Error("offline"));
    render(view({ ...state, readiness: { ...state.readiness, server: "stopped" } }));
    expect(await screen.findByRole("spinbutton")).toHaveValue(12);
    expect(screen.getByRole("spinbutton")).toBeEnabled();
    expect(screen.getByText("Saved limit: 12 · Default: 4")).toBeInTheDocument();
    expect(screen.queryByText("Saved limit is in effect.")).not.toBeInTheDocument();
  });
  it.each(["restart", "server offline"])("keeps capacity stale after %s until a fresh observation arrives", async boundary => {
    const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
    const interrupted = boundary === "restart"
      ? { ...state, current_operation: { id: "restart", kind: "runner_restart", phase: "running" } as DesktopState["current_operation"] }
      : { ...state, readiness: { ...state.readiness, server: "stopped" as const } };
    mounted.rerender(view(interrupted));
    expect(screen.getByText("Capacity needs refresh.")).toBeInTheDocument();
    let resolve!: (value: typeof runner) => void;
    query.mockReturnValue(new Promise(done => { resolve = done; }));
    settings.max_concurrent_jobs = 12;
    mounted.rerender(view());
    expect(screen.getByText("Capacity needs refresh.")).toBeInTheDocument();
    expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();
    expect(screen.queryByText("Saved limit is in effect.")).not.toBeInTheDocument();
    await act(async () => resolve({ ...runner, jobs_running: 1, jobs_queued: 0, job_concurrency_limit: 12 }));
    expect(await screen.findByText("1 running · 0 queued · 12 max")).toBeInTheDocument();
    expect(screen.getByText("Saved limit is in effect.")).toBeInTheDocument();
  });
  it("discards a pre-operation read without blocking the post-operation refresh", async () => {
    vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
    const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
    let resolveOld!: (value: typeof runner) => void, resolveFresh!: (value: typeof runner) => void;
    query.mockReturnValueOnce(new Promise(done => { resolveOld = done; }));
    fireEvent(document, new Event("visibilitychange"));
    mounted.rerender(view({ ...state, current_operation: { id: "restart", kind: "runner_restart", phase: "running" } as DesktopState["current_operation"] }));
    query.mockReturnValueOnce(new Promise(done => { resolveFresh = done; }));
    mounted.rerender(view());
    expect(query).toHaveBeenCalledTimes(3);
    await act(async () => resolveOld({ ...runner, jobs_running: 3, jobs_queued: 1 }));
    expect(screen.getByText("Capacity needs refresh.")).toBeInTheDocument();
    expect(screen.queryByText("3 running · 1 queued · 4 max")).not.toBeInTheDocument();
    await act(async () => resolveFresh({ ...runner, jobs_running: 2, jobs_queued: 0 }));
    expect(await screen.findByText("2 running · 0 queued · 4 max")).toBeInTheDocument();
  });
  it("does not revive expired counts while a hidden settings page refreshes", async () => {
    const clock = vi.spyOn(Date, "now").mockReturnValue(1000000);
    const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
    mounted.rerender(view(state, false));
    clock.mockReturnValue(1060000); query.mockReturnValue(new Promise(() => undefined));
    mounted.rerender(view());
    expect(screen.getByText("Capacity needs refresh.")).toBeInTheDocument();
    expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();
  });
  it("ignores a late observation after unmount", async () => {
    let resolve!: (value: unknown) => void; query.mockReturnValue(new Promise(done => { resolve = done; }));
    const mounted = render(view()); mounted.unmount(); await act(async () => resolve(runner)); expect(onState).not.toHaveBeenCalled();
  });
});
