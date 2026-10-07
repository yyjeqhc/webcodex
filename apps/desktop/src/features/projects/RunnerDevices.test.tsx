import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";
import { WorkspaceProvider } from "../workspace/WorkspaceContext";
import { RunnerDevices } from "./RunnerDevices";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke, isTauri: () => false }));
const state = {
  workspace_runner: { config_path: "/fixture/runner.toml", client_id: "local", server_url: "http://localhost:1" },
  topology: { runner: { kind: "local" }, server: { kind: "local" } },
  readiness: { server: "ready", runner: "ready" }, current_operation: null, saved_projects: [],
} as DesktopState;
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
