import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";
import type { ServerOverview } from "../../models/workspace";
import { WorkspaceProvider } from "../workspace/WorkspaceContext";
import { RunnerDevices } from "./RunnerDevices";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke, isTauri: () => false }));
const state: DesktopState = {
  workspace_runner: { config_path: "/fixture/runner.toml", client_id: "local", server_url: "http://localhost:1" },
  topology: { experience: "full", runner: { kind: "local" }, server: { kind: "local" }, exposure: { kind: "none" }, enrollment: { kind: "shared_key" } },
  readiness: { server: "ready", runner: "ready", exposure: "disabled", project: "none", runtime_ready: true, ready_for_chatgpt: false, summary: "", summary_kind: "runtime_ready_local_only" },
  current_operation: null, saved_projects: [], activity_sequence: 0,
  openai_tunnel_configured: false,
  openai_tunnel_config: { tunnel_id_present: false, api_key_present: false, source: "file", saved_tunnel_id: null },
  regular_tunnel_available: false, runtime_autostart: false, preferred_connection: "no_chat_gpt",
  tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "none", effective_proxy_present: false, system_proxy_detected: false },
};
function view(value = state, suspended = false) {
  return <DesktopMantineProvider><LocaleProvider><WorkspaceProvider state={value} suspended={suspended}><RunnerDevices /></WorkspaceProvider></LocaleProvider></DesktopMantineProvider>;
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  native.invoke.mockImplementation(async (_command, { request }) => request.kind === "overview"
    ? { projects_available: true, visible_projects: 0, projects: [], projects_truncated: false,
        runners: [{ client_id: "local", connected: true, status: "online", jobs_running: 4, jobs_queued: 2, job_concurrency_limit: 4 }] }
    : request.kind === "projects" ? { projects: [], total: 0, truncated: false } : { windows: [] });
});
afterEach(() => vi.restoreAllMocks());

it("invalidates capacity during a restart and until a fresh observation arrives", async () => {
  const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
  mounted.rerender(view({ ...state, current_operation: { id: "restart", kind: "runner_restart", phase: "running" } as DesktopState["current_operation"] }));
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "stale");
  native.invoke.mockReturnValue(new Promise(() => undefined));
  mounted.rerender(view());
  expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();
});

it("keeps capacity stale after Server recovery until a fresh observation arrives", async () => {
  vi.spyOn(Date, "now").mockReturnValue(1000000);
  const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
  mounted.rerender(view({ ...state, readiness: { ...state.readiness, server: "stopped", runtime_ready: false } }));
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "stale");
  expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();

  const normal = native.invoke.getMockImplementation()!;
  let resolve!: (value: ServerOverview) => void;
  const fresh = new Promise<ServerOverview>(done => { resolve = done; });
  native.invoke.mockImplementation((command, args) => args.request.kind === "overview" ? fresh : normal(command, args));
  const queriesBeforeRecovery = native.invoke.mock.calls.length;
  mounted.rerender(view());
  expect(native.invoke.mock.calls.length).toBeGreaterThan(queriesBeforeRecovery);
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "stale");
  expect(screen.queryByText("4 running · 2 queued · 4 max")).not.toBeInTheDocument();

  await act(async () => resolve({ projects_available: true, visible_projects: 0, projects: [], projects_truncated: false,
    runners: [{ client_id: "local", connected: true, status: "online", jobs_running: 1, jobs_queued: 0, job_concurrency_limit: 4 }] }));
  expect(await screen.findByText("1 running · 0 queued · 4 max")).toBeInTheDocument();
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "available");
});

it("does not show live capacity while polling is paused", async () => {
  const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
  mounted.rerender(view(state, true));
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "stale");
});

it("does not revive expired counts before an overdue refresh finishes", async () => {
  const clock = vi.spyOn(Date, "now").mockReturnValue(1000000);
  const mounted = render(view()); await screen.findByText("4 running · 2 queued · 4 max");
  clock.mockReturnValue(1060000);
  mounted.rerender(view());
  expect(document.querySelector("[data-webcodex-capacity]")).toHaveAttribute("data-webcodex-capacity", "stale");
});
