import { useLayoutEffect } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GitSummary, WorkflowSession, WorkspaceProject, ServerRunnerSummary } from "../../models/workspace";
import type { DesktopState } from "../../models/topology";
import { LocaleProvider } from "../../i18n/locale";
import { PRODUCT_LOCALES, PRODUCT_MESSAGES, productText } from "../../i18n/product";
import { ProjectsPanel } from "../projects/ProjectsPanel";
import { ActivityPanel } from "../activity/ActivityPanel";
import { ExtensionsPanel } from "../extensions/ExtensionsPanel";
import { WorkspaceProvider, sameProjectPath, sameProject, mergeProjects, sessionTitle, projectName, displayProjectPath, useWorkspace } from "./WorkspaceContext";
import { ChatgptObservation, observationTime, WorkspaceStatus } from "./WorkspaceStatus";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { Sidebar } from "../../components/Sidebar";
import { Dashboard } from "../dashboard/Dashboard";
import { connectionFixture, connectionSnapshot } from "../../test/connections-fixtures";

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
const api = vi.hoisted(() => ({ managedInstructionsRead: vi.fn(async () => ({ path:"/fixture/desktop/instructions/AGENTS.md", exists:false, content:"", revision:"missing" })), prepareProjectUnregister: vi.fn(), unregisterProject: vi.fn(), runnerSettings: vi.fn(), updateRunnerSettings: vi.fn(), restartOwnedRunner: vi.fn(), sshResources: vi.fn(), addRunnerPlugin: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke, isTauri: () => false }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const alpha = { id: "agent:mini:alpha", path: "C:\\work\\alpha", name: "alpha", connected: true, sessions: { running_sessions: 0, active_sessions: 2, latest_updated_at: 100 } };
const beta = { id: "agent:mini:beta", path: "C:\\work\\beta", name: "beta", connected: true, sessions: { running_sessions: 0, active_sessions: 0, latest_updated_at: 90 } };
const session = { project_id: alpha.id, project_name: "alpha", session_id: "wc_sess_1234567890123456", title: "Fix export workflow", lifecycle: "active", updated_at: 100, running_call: false, running_jobs: 1, running_jobs_complete: true,
  overview: { attention: { open_todos: 2, open_questions: 1, open_risks: 0 }, reported_progress: { text: "Review the export changes", reported_at: 90 } }, last_activity: { kind: "Edited", state: "succeeded", summary: "Updated export handler" },
};
const windowRow = { client_window_key: "window-001", source: "chatgpt", last_project: alpha.id, last_seen_at_ms: 100_000, last_meaningful_activity_at_ms: 100_000, active_count: 1, linked_session_count: 1 };
const state = {
  project: { path: alpha.path, runtime_project_id: alpha.id, is_git_repository: true, valid: true },
  saved_projects: [{ path: alpha.path, runtime_project_id: alpha.id }, { path: beta.path, runtime_project_id: beta.id }],
  readiness: { runtime_ready: true, server: "ready", runner: "ready", exposure: "none" },
  topology: { server: { kind: "local" }, runner: { kind: "local" }, experience: "full" },
  workspace_runner: { client_id: "mini", server_url: "http://localhost", config_path: "fixture.toml" },
  current_operation: null, chatgpt_activity: { observed: false, last_meaningful_activity_at_ms: null },
} as unknown as DesktopState;
const overview = { projects_available: true, client_id: "mini", connected: true, projects: [alpha, beta], visible_project_count: 2, projects_truncated: false, recent_sessions: { sessions: [session], truncated: false, scan_truncated: false } };
const wrap = (children: React.ReactNode, selected = state) => <DesktopMantineProvider><LocaleProvider><WorkspaceProvider state={selected}>{children}</WorkspaceProvider></LocaleProvider></DesktopMantineProvider>;

beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  HTMLDialogElement.prototype.showModal = function () { this.setAttribute("open", ""); };
  HTMLDialogElement.prototype.close = function () { this.removeAttribute("open"); };
  api.runnerSettings.mockResolvedValue({ target: { client_id: "mini", config_path: "fixture.toml", server_url: "http://localhost" }, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: [], can_restart: true });
  api.sshResources.mockRejectedValue(new Error("fixture-unavailable"));
  native.invoke.mockImplementation(async (_command, { request }) => {
    switch (request.kind) {
      case "overview": return overview;
      case "projects": return { projects: overview.projects, total: overview.visible_project_count, truncated: overview.projects_truncated };
      case "windows": return { windows: [windowRow] };
      case "project_git": return { branch: "feat/export", clean: false, git_available: true, non_git_project: false, files: [{ path: "src/export.ts", status: " M" }] };
      case "window": return { ...windowRow, linked_sessions: [{ project: alpha.id, workflow_session_id: session.session_id, title: session.title }], activity: [{ tool_name: "apply_text_edits", meaningful: true, project: alpha.id, status: "succeeded", ended_at_ms: 100_000 }] };
      case "session": return { ...session, activity: [{ kind: "Edited", state: "succeeded", summary: "Updated export handler", started_at: 90, finished_at: 100 }] };
      case "extensions": return { project: alpha.id, runner: "mini", can_reload_plugins: true,
        instructions: { files: [{ source_scope: "project", path: "AGENTS.md", fingerprint: "revision-one", total_lines: 2 }], scan_complete: true },
        skills: { available: true, catalog: { skills: [{ skill_id: "skill-1", name: "Review changes", description: "Review project changes", source_scope: "project" }] } },
        plugins: { available: true, catalog: { plugins: [{ plugin: "sample", name: "Sample provider", status: "ready", errorCode: null }] } },
      };
      case "instruction": return { content: "# Project instructions\nUse existing tests.", truncated: false };
      case "plugin_reload": return { reloaded: true };
      default: throw new Error("Unexpected workspace query: " + request.kind);
    }
  });
});
afterEach(cleanup);

describe("product workspace task flows", () => {
  it.each([[false, false], [true, false], [false, true], [true, true]])(
    "reports partial Session history for return truncation=%s and scan truncation=%s",
    async (truncated, scan_truncated) => {
      const normal = native.invoke.getMockImplementation()!;
      native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
        ? Promise.resolve({ ...overview, recent_sessions: { sessions: [session], truncated, scan_truncated } })
        : normal(command, value));
      render(wrap(<ActivityPanel activity={[]} />));
      fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
      await screen.findByText(session.title);
      expect(Boolean(screen.queryByText("History is partial"))).toBe(truncated || scan_truncated);
    },
  );

  it.each([[true, false], [false, true], [true, true]])(
    "does not claim empty history is complete when truncation=%s and scan truncation=%s",
    async (truncated, scan_truncated) => {
      const normal = native.invoke.getMockImplementation()!;
      native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
        ? Promise.resolve({ ...overview, recent_sessions: { sessions: [], truncated, scan_truncated } })
        : normal(command, value));
      render(wrap(<ActivityPanel activity={[]} />));
      fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
      expect(await screen.findByText("History is partial")).toBeInTheDocument();
      expect(screen.getByText("No sessions observed in this partial history")).toBeInTheDocument();
      expect(screen.queryByText("No Workflow Sessions yet")).not.toBeInTheDocument();
    },
  );

  it("retains the complete empty Session history message", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, recent_sessions: { sessions: [], truncated: false, scan_truncated: false } })
      : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    await screen.findByRole("button", { name: /Fix export workflow/ });
    fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
    expect(screen.getByText("No Workflow Sessions yet")).toBeInTheDocument();
    expect(screen.queryByText("History is partial")).not.toBeInTheDocument();
  });

  it("ignores failures from pre-refresh observations and resumes overview and Git reads", async () => {
    const pending = new Map<string, (error: Error) => void>();
    const normal = native.invoke.getMockImplementation()!;
    function Status() {
      const workspace = useWorkspace();
      return <><span>{workspace.error || workspace.windowsError ? "Observation failed" : "Observation healthy"}</span>
        <button onClick={workspace.refresh}>Refresh observations</button></>;
    }
    const content = (suspended: boolean) => <DesktopMantineProvider><LocaleProvider><WorkspaceProvider state={state} suspended={suspended}>
      <Status /><ProjectsPanel />
    </WorkspaceProvider></LocaleProvider></DesktopMantineProvider>;
    const view = render(content(false));
    expect(await screen.findAllByText("feat/export")).toHaveLength(2);
    native.invoke.mockImplementation((_command, { request }) => new Promise((_resolve, reject) => {
      pending.set(request.kind === "project_git" ? request.project : request.kind, reject);
    }));
    fireEvent.click(screen.getByRole("button", { name: "Refresh observations" }));
    expect(pending.size).toBe(5);
    view.rerender(content(true));
    await act(async () => { for (const reject of pending.values()) reject(new Error("workspace_unavailable")); });
    expect(screen.getByText("Observation healthy")).toBeInTheDocument();
    expect(screen.queryByText("feat/export")).not.toBeInTheDocument();
    native.invoke.mockClear();
    native.invoke.mockImplementation(normal);
    view.rerender(content(false));
    expect(await screen.findAllByText("feat/export")).toHaveLength(2);
    await waitFor(() => expect(native.invoke).toHaveBeenCalledTimes(5));
    expect(screen.getByText("Observation healthy")).toBeInTheDocument();
  });


  it("clears stale Git observations and shows an unconfirmed state when refresh cannot inspect a project", async () => {
    const normal = native.invoke.getMockImplementation()!;
    render(wrap(<ProjectsPanel onState={vi.fn()} />));
    const initial = await screen.findByRole("row", { name: "alpha" });
    expect(await within(initial).findByText("feat/export")).toBeInTheDocument();

    native.invoke.mockImplementation((command, value) => value.request.kind === "project_git" && value.request.project === alpha.id
      ? Promise.reject(new Error("project path unavailable")) : normal(command, value));
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

    const refreshed = await screen.findByRole("row", { name: "alpha" });
    await within(refreshed).findByText("Status unconfirmed");
    expect(refreshed).not.toHaveTextContent("feat/export");
    expect(screen.getByRole("button", { name: "Remove project alpha" })).toBeInTheDocument();
  });

  it.each([
    ["http://127.0.0.1:18080", null],
    ["http://192.0.2.10:18080", null],
    ["https://central.example", null],
    ["http://127.0.0.1:18080", "viewer-environment"],
    ["http://192.0.2.10:18080", "viewer-environment"],
    ["https://central.example", "viewer-environment"],
  ])("scopes viewer status without inferring ownership from %s (%s)", (url, persistent_environment) => {
    const viewer = { ...state, readiness: { ...state.readiness, runner: "stopped" as const },
      persistent_environment, connections: connectionSnapshot(),
      topology: { ...state.topology!, server: { kind: "remote" as const, url }, runner: { kind: "none" as const } } } as DesktopState;
    render(wrap(<><WorkspaceStatus state={viewer} /><Sidebar state={viewer} navigation="home" setNavigation={vi.fn()} /></>, viewer));
    expect(screen.getByRole("status")).toHaveTextContent("Server ConnectionRunning");
    expect(screen.getByRole("status")).not.toHaveTextContent("Local task service");
    expect(screen.getByRole("status")).toHaveTextContent("Desktop Tunnel connectionsNo Tunnel connections saved in Desktop");
    expect(screen.getByRole("complementary")).toHaveTextContent("Server Connection · Running");
    expect(screen.getByRole("complementary")).toHaveTextContent("No Tunnel connections saved in Desktop");
    expect(screen.getByRole("complementary")).not.toHaveTextContent("Local task service");
    expect(screen.queryByText("ChatGPT connected")).not.toBeInTheDocument();
    expect(native.invoke.mock.calls.every(([command]) => command === "workspace_query")).toBe(true);
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
    expect(viewer.readiness.runner).toBe("stopped");
  });

  it.each([null, "server-environment"])("hides an absent Runner on a Server-only environment (%s)", persistent_environment => {
    const serverOnly = { ...state, persistent_environment, topology: { ...state.topology!, runner: { kind: "none" as const } },
      connections: connectionSnapshot(connectionFixture({ pid: null })) };
    render(wrap(<><WorkspaceStatus state={serverOnly} /><Sidebar state={serverOnly} navigation="home" setNavigation={vi.fn()} /></>, serverOnly));
    expect(screen.getByRole("status")).toHaveTextContent("Server ConnectionRunning");
    expect(screen.getByRole("status")).not.toHaveTextContent("Local task service");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 1 Desktop tunnels locally ready");
    expect(screen.getByRole("complementary")).toHaveTextContent("Server Connection · Running");
  });

  it("does not invent a configured Runner before topology is available", () => {
    const unknown = { ...state, topology: null };
    render(wrap(<><WorkspaceStatus state={unknown} /><Sidebar state={unknown} navigation="home" setNavigation={vi.fn()} /></>, unknown));
    expect(screen.getByRole("status")).not.toHaveTextContent("Local task service");
    expect(screen.getByRole("complementary")).not.toHaveTextContent("Local task service");
  });

  it.each([null, "full-environment"])("keeps configured local Runner readiness in a Full Runtime (%s)", persistent_environment => {
    const full = { ...state, persistent_environment, readiness: { ...state.readiness, runner: "error" as const, runtime_ready: false } };
    render(wrap(<><WorkspaceStatus state={full} /><Sidebar state={full} navigation="home" setNavigation={vi.fn()} /></>, full));
    expect(screen.getByRole("status")).toHaveTextContent("Local task serviceUnavailable");
    expect(screen.getByRole("complementary")).toHaveTextContent("Local task service · Unavailable");
  });

  it("shows the configured Runner when joining an independently managed Server", () => {
    const joined = { ...state, persistent_environment: "joined-environment",
      topology: { ...state.topology!, server: { kind: "remote" as const, url: "https://central.example" } }, connections: connectionSnapshot() };
    render(wrap(<><WorkspaceStatus state={joined} /><Sidebar state={joined} navigation="home" setNavigation={vi.fn()} /></>, joined));
    expect(screen.getByRole("status")).toHaveTextContent("Local task serviceRunning");
    expect(screen.getByRole("complementary")).toHaveTextContent("Local task service · Running");
    expect(screen.getByRole("status")).toHaveTextContent("No Tunnel connections saved in Desktop");
  });

  it("keeps Quick Share exposure separate from saved Tunnel connections", () => {
    const quickShare = { ...state, topology: { ...state.topology!, experience: "quick_share" as const },
      readiness: { ...state.readiness, exposure: "remote_ready" as const }, connections: connectionSnapshot() };
    render(wrap(<><WorkspaceStatus state={quickShare} /><Sidebar state={quickShare} navigation="home" setNavigation={vi.fn()} /></>, quickShare));
    expect(screen.getByRole("status")).toHaveTextContent("Quick ShareRunning");
    expect(screen.getByRole("complementary")).toHaveTextContent("Quick Share · Running");
    expect(screen.queryByText("Desktop Tunnel connections")).not.toBeInTheDocument();
    expect(screen.queryByText("No Tunnel connections saved in Desktop")).not.toBeInTheDocument();
  });

  it("keeps a viewer read-only without offering a local project picker", () => {
    const viewer = { ...state, project: null, saved_projects: [], workspace_runner: null,
      topology: { ...state.topology!, server: { kind: "remote" as const, url: "https://central.example" }, runner: { kind: "none" as const } } } as DesktopState;
    render(wrap(<Dashboard state={viewer} refreshing={false} onRefresh={vi.fn()} onResumeRuntime={vi.fn()}
      onChangeSetup={vi.fn()} onNavigate={vi.fn()} onStopQuickShare={vi.fn()} />, viewer));
    expect(screen.queryByRole("button", { name: "Add Project" })).not.toBeInTheDocument();
  });
  it("describes locally ready tunnels without counting a live but unhealthy process as ready", () => {
    const selected = { ...state, connections: connectionSnapshot(connectionFixture({ ready: false, process_started: true, lifecycle: "running" })) };
    render(wrap(<><WorkspaceStatus state={selected} /><Sidebar state={selected} navigation="home" setNavigation={vi.fn()} /></>, selected));
    expect(screen.getByRole("status")).toHaveTextContent("0 of 1 Desktop tunnels locally ready");
    expect(screen.getByRole("complementary")).toHaveTextContent("0 of 1 Desktop tunnels locally ready");
    expect(screen.queryByText("ChatGPT connected")).not.toBeInTheDocument();
  });
  it.each([
    [connectionSnapshot(), "No Tunnel connections saved in Desktop"],
    [{ ...connectionSnapshot(), config_error: true }, "Desktop Tunnel status unconfirmed"],
    [undefined, "Desktop Tunnel status unconfirmed"],
  ])("distinguishes missing configuration from unavailable tunnel observations", (connections, label) => {
    const selected = { ...state, connections };
    render(wrap(<WorkspaceStatus state={selected} />, selected));
    expect(screen.getByRole("status")).toHaveTextContent(label);
    expect(screen.getByRole("status")).not.toHaveTextContent("0 / 0");
  });

  it("does not show cached readiness as healthy when Tunnel configuration is unreadable", () => {
    const selected = { ...state, connections: { ...connectionSnapshot(connectionFixture()), config_error: true } };
    render(wrap(<WorkspaceStatus state={selected} />, selected));
    expect(screen.getByRole("status")).toHaveTextContent("Desktop Tunnel status unconfirmed");
    const row = screen.getByText("Desktop Tunnel connections").closest("div")!;
    expect(row.querySelector(".status-dot.ready")).toBeNull();
  });
  it.each([
    ["workspace_authentication_required", "User authentication is missing or expired. Restore your Server credential."],
    ["workspace_permission_denied", "This user does not have permission to view this Server data."],
    ["workspace_server_unreachable", "Server unreachable. Check its address and your connection."],
  ])("distinguishes %s from an empty authorized inventory", async (code, message) => {
    native.invoke.mockRejectedValue({ code, message: "secret-response-body" });
    render(wrap(<ProjectsPanel />));
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    expect(screen.queryByText("secret-response-body")).not.toBeInTheDocument();
    expect(screen.queryByText("No project folders to show")).not.toBeInTheDocument();
    if (code !== "workspace_server_unreachable") expect(screen.queryByRole("row", { name: "alpha" })).not.toBeInTheDocument();
  });

  it("marks cached GUI availability stale on transport failure and clears data on authorization loss", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((name, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, runners: [{ client_id: "mini", connected: true, computer_session_availability: true }] }) : normal(name, value));
    render(wrap(<ProjectsPanel />));
    const fleet = await screen.findByRole("region", { name: "Task execution devices" });
    expect(fleet).toHaveTextContent("Desktop session connected");
    native.invoke.mockRejectedValue({ code: "workspace_server_unreachable" });
    fireEvent(document, new Event("visibilitychange"));
    await screen.findByRole("alert");
    expect(fleet).toHaveTextContent("Refresh to confirm status");
    expect(fleet).toHaveTextContent("Desktop session status needs a fresh check");
    expect(fleet).not.toHaveTextContent("Desktop session unavailable");
    native.invoke.mockRejectedValue({ code: "workspace_permission_denied" });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Task execution devices" })).not.toBeInTheDocument());
    expect(screen.queryByRole("row", { name: "alpha" })).not.toBeInTheDocument();
  });

  it.each([
    [true, "Desktop session connected"],
    [false, "Desktop session unavailable"],
    [undefined, "Desktop session status not reported"],
    [null, "Desktop session status not reported"],
  ])("explains the local device without treating %s desktop availability as a file-tool failure", async (availability, label) => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((name, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, runners: [{ client_id: "mini", connected: true, computer_session_availability: availability }] }) : normal(name, value));
    const openSettings = vi.fn();
    render(wrap(<ProjectsPanel onComputerSettings={openSettings} />));
    const devices = await screen.findByRole("region", { name: "Task execution devices" });
    expect(within(devices).getByRole("heading", { name: "This computer" })).toBeInTheDocument();
    expect(devices).toHaveTextContent(label);
    expect(screen.getByRole("row", { name: "alpha" })).toHaveTextContent("Runs on · This computer");
    if (availability === true) {
      expect(devices).toHaveTextContent("still require system permissions");
      expect(within(devices).queryByRole("button", { name: "Desktop access settings" })).not.toBeInTheDocument();
    } else {
      if (availability === false) expect(devices).toHaveTextContent("File and command tools do not depend on this session");
      else expect(devices).not.toHaveTextContent("Desktop session unavailable");
      fireEvent.click(within(devices).getByRole("button", { name: "Desktop access settings" }));
      expect(openSettings).toHaveBeenCalledTimes(1);
    }
    expect(native.invoke.mock.calls.every(([command]) => command === "workspace_query")).toBe(true);
  });

  it("shows the authorized B/C fleet and GUI availability on a viewer with no local Runner", async () => {
    const viewer = { ...state, project: null, saved_projects: [], workspace_runner: null,
      topology: { ...state.topology!, server: { kind: "remote", url: "https://central.example" }, runner: { kind: "none" } } } as DesktopState;
    const projects = [{ ...alpha, id: "agent:B:alpha", client_id: "B" }, { ...beta, id: "agent:C:beta", client_id: "C" }];
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((name, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, projects, runners: [
          { client_id: "B", connected: true, status: "online", computer_session_availability: true },
          { client_id: "C", connected: true, status: "stale", computer_session_availability: true },
        ] })
      : value.request.kind === "projects" ? Promise.resolve({ projects, total: 2, truncated: false }) : normal(name, value));
    render(wrap(<ProjectsPanel />, viewer));
    const fleet = await screen.findByRole("region", { name: "Task execution devices" });
    const rows = within(fleet).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("Runner identifier · B");
    expect(rows[0]).toHaveTextContent("Connected to Server");
    expect(rows[0]).toHaveTextContent("Desktop session connected");
    expect(rows[1]).toHaveTextContent("Runner identifier · C");
    expect(rows[1]).toHaveTextContent("Refresh to confirm status");
    expect(rows[1]).not.toHaveTextContent("Desktop session connected");
    expect(screen.getByRole("row", { name: "alpha" })).toHaveTextContent("Runs on · B");
    expect(screen.getByRole("row", { name: "beta" })).toHaveTextContent("Runs on · C");
    expect(screen.queryByRole("button", { name: "Add Project" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Remove project/ })).not.toBeInTheDocument();
  });

  it("never merges a remote path into a local saved project just because the strings match", () => {
    const remote = { ...alpha, id: "agent:other:alpha", client_id: "other" };
    const merged = mergeProjects([remote], [{ path: alpha.path }], false, "mini");
    expect(merged).toHaveLength(2);
    expect(merged[0].id).toBe(remote.id);
  });

  it("shows Runner Projects as read-only observed runtime inventory", async () => {
    render(wrap(<ProjectsPanel />));    await screen.findByLabelText("2 open sessions"); expect(await screen.findAllByText("feat/export")).toHaveLength(2);
    expect(screen.getAllByRole("row")).toHaveLength(3);
    for (const row of screen.getAllByRole("row")) expect(within(row).queryByRole("button", { name: /Use project|Select project/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Current")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add Project" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Remove project/ })).not.toBeInTheDocument();
    expect(screen.getByText(/Projects are registered working folders/)).toBeInTheDocument();
    expect(screen.getByText(/removing access does not remove a registered project/)).toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: "Search projects" }), { target: { value: "ALPHA" } });
    expect(screen.getAllByRole("row")).toHaveLength(2);
  });
  it("removes only a confirmed local project registration and keeps permission semantics explicit", async () => {
    const observation = {
      target: { config_path: "fixture.toml", client_id: "mini", server_url: "http://localhost" },
      project: beta.id,
      expected_revision: "sha256:" + "a".repeat(64),
      path: beta.path,
    };
    const onState = vi.fn();
    api.prepareProjectUnregister.mockResolvedValue(observation);
    api.unregisterProject.mockResolvedValue({ ...state, project: state.project, saved_projects: [state.saved_projects![0]] });
    render(wrap(<ProjectsPanel onState={onState} />));
    await screen.findByLabelText("2 open sessions");
    fireEvent.click(screen.getByRole("button", { name: "Remove project beta" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove project" });
    expect(dialog).toHaveTextContent("file access permissions are unchanged");
    expect(dialog).toHaveTextContent("Your folder and Git files stay on disk");
    fireEvent.click(within(dialog).getByRole("button", { name: "Remove project" }));
    await waitFor(() => expect(api.unregisterProject).toHaveBeenCalledExactlyOnceWith(observation));
    expect(onState).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("row", { name: "beta" })).not.toBeInTheDocument());
  });

  it.each([
    ["agent:msi:foo", "\\\\?\\D:\\repo", "D:\\repo"],
    [undefined, "\\\\?\\D:\\repo", "D:\\repo"],
    [undefined, "\\\\?\\UNC\\SERVER\\Share\\Repo", "\\\\server\\share\\repo\\"],
  ])("keeps Runner activity and Git identity with saved ID %s, path %s", async (savedId, runnerPath, savedPath) => {
    const runner = { ...alpha, id: "agent:msi:foo", name: "repo", path: runnerPath, sessions: { running_sessions: 0, active_sessions: 2, latest_updated_at: (Date.now() - 120_000) / 1000 } };
    const saved = { ...state.project!, runtime_project_id: savedId, path: savedPath };
    const selected = { ...state, project: saved, saved_projects: [saved] };
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((name, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, projects: [runner] })
      : value.request.kind === "projects"
        ? Promise.resolve({ projects: [runner], total: 1, truncated: false })
        : value.request.kind === "project_git" && !savedId
        ? Promise.reject(new Error("Git metadata unavailable")) : normal(name, value));
    render(wrap(<ProjectsPanel />, selected));
    await screen.findByLabelText("2 open sessions");
    const row = screen.getByRole("row", { name: "repo" });
    expect(screen.getAllByRole("row")).toHaveLength(2);
    expect(within(row).getByText(observationTime(runner.sessions.latest_updated_at * 1000, "en-US"))).toBeInTheDocument();
    expect(row).not.toHaveTextContent(productText("en-US", "noActivity"));
    expect(row).not.toHaveTextContent("\\\\?\\");
    await waitFor(() => expect(native.invoke).toHaveBeenCalledWith("workspace_query", { request: { kind: "project_git", project: runner.id } }));
    const rows = mergeProjects([runner], [saved]);
    expect(rows).toHaveLength(1); expect(rows[0]).toBe(runner);
    expect(sameProject(rows[0], saved)).toBe(true);
  });
  it("keeps the Runner inventory observable without a Desktop default project", async () => {
    const emptyDefault = { ...state, project: null, saved_projects: [], readiness: { ...state.readiness, project: "none" as const, runtime_ready: false } };
    render(wrap(<ProjectsPanel />, emptyDefault));
    await screen.findByLabelText("2 open sessions");
    expect(screen.getAllByRole("row")).toHaveLength(3);
  });
  it("opens a Window's associated Workflow Session and presents product activity rather than a ledger", async () => {
    render(wrap(<ActivityPanel activity={[]} />));
    expect(screen.getByRole("tab", { name: "Tool calls" })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(await screen.findByRole("button", { name: /Open call details:.*window-001/ }));
    const detail = await screen.findByRole("dialog", { name: "Request details · window-001" });
    fireEvent.click(await within(detail).findByRole("button", { name: /Fix export workflow/ }));
    const workflow = await screen.findByRole("dialog", { name: "Fix export workflow" });
    await within(workflow).findByText("Review the export changes");
    expect(within(workflow).getByText("Updated export handler")).toBeInTheDocument();
    expect(within(workflow).getByText("src/export.ts")).toBeInTheDocument();
    expect(workflow).not.toHaveTextContent("apply_text_edits");
    expect(native.invoke).toHaveBeenCalledWith("workspace_query", { request: { kind: "session", project: alpha.id, session_id: session.session_id } });
    fireEvent.click(within(workflow).getByRole("button", { name: "Close" }));
    fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
    const row = await screen.findByRole("button", { name: /Fix export workflow/ });
    expect(within(row).getAllByText("Active jobs 1")).toHaveLength(1);
    expect(row).not.toHaveTextContent("Active Jobs:");
    expect(screen.getByRole("tab", { name: "Service events" })).toHaveAttribute("aria-selected", "false");
  });
  it("labels tool completion separately from response handoff and keeps transport evidence in details", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "windows"
      ? Promise.resolve({ windows: [{ ...windowRow, active_count: 0 }] }) : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    const row = await screen.findByRole("button", { name: /Open call details: Fix export workflow/ });
    await within(row).findByText("Call completed");
    expect(row).not.toHaveTextContent("HTTP");
    expect(row).not.toHaveTextContent("Response handoff");
    fireEvent.click(row);
    const dialog = await screen.findByRole("dialog", { name: "Request details · window-001" });
    expect((await within(dialog).findAllByText("Response handoff not confirmed")).length).toBeGreaterThan(0);
  });
  it("does not infer a call result when the preview observation fails", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "window" ? Promise.reject(new Error("unreachable")) : value.request.kind === "windows"
      ? Promise.resolve({ windows: [{ ...windowRow, active_count: 0 }] }) : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    const row = await screen.findByRole("button", { name: /Open call details: Tool calls/ });
    expect(row).toHaveTextContent("Call result unconfirmed");
    expect(row).not.toHaveTextContent("Call completed");
  });
  it("uses the active call's project and does not borrow another project's session title", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "window"
      ? Promise.resolve({ ...windowRow, active_requests: [{ tool_name: "read_files", project: beta.id }], linked_sessions: [{ project: alpha.id, title: session.title }], activity: [] }) : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    const row = await screen.findByRole("button", { name: /Open call details: read_files/ });
    expect(row).toHaveTextContent("beta");
    expect(row).not.toHaveTextContent("alpha");
    expect(row).not.toHaveTextContent(session.title);
  });
  it("distinguishes an open idle session from running work and hides empty attention counts", async () => {
    const normal = native.invoke.getMockImplementation()!;
    const idle = { ...session, running_jobs: 0, overview: { attention: { open_todos: 0, open_questions: 0, open_risks: 0 } } };
    native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, recent_sessions: { sessions: [idle], truncated: false } }) : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
    const row = await screen.findByRole("button", { name: /Fix export workflow/ });
    expect(row).toHaveTextContent("Open");
    expect(row).not.toHaveTextContent("In progress");
    expect(row).not.toHaveTextContent("Active jobs");
    expect(row).not.toHaveTextContent("unknown");
  });
  it.each([
    ["passed", "檢查通過"], ["failed", "檢查失敗"], ["inconclusive", "檢查尚無明確結論"], ["not_run", "尚未執行檢查"], ["unavailable", "尚無可確認的檢查結果"],
  ])("localizes %s validation evidence in Traditional Chinese session details", async (validation, label) => {
    localStorage.setItem("webcodex.desktop.locale", "zh-TW");
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "session"
      ? Promise.resolve({ ...session, overview: { ...session.overview, validation: { state: validation } } }) : normal(command, value));
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.click(screen.getByRole("tab", { name: "工作會話" }));
    fireEvent.click(await screen.findByRole("button", { name: /Fix export workflow/ }));
    const dialog = await screen.findByRole("dialog", { name: "Fix export workflow" });
    expect(await within(dialog).findByText(label)).toBeInTheDocument();
    expect(dialog).not.toHaveTextContent(`: ${validation}`);
  });
  it("opens effective instructions, lists real Skills and reloads a provider only on request", async () => {
    render(wrap(<ExtensionsPanel state={state} onState={vi.fn()} />));
    fireEvent.click(screen.getByRole("tab", { name: "Instructions" }));
    fireEvent.click(await screen.findByRole("button", { name: "Open AGENTS.md" }));
    const document = await screen.findByRole("dialog", { name: "AGENTS.md" });
    await within(document).findByText(/Use existing tests/);
    expect(native.invoke).toHaveBeenCalledWith("workspace_query", { request: { kind: "instruction", project: alpha.id, source_scope: "project", path: "AGENTS.md", fingerprint: "revision-one" } });
    fireEvent.click(within(document).getByRole("button", { name: "Close" }));
    fireEvent.click(screen.getByRole("tab", { name: "Skills" })); expect(screen.getByText("Review changes")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Native Plugins" }));
    const pluginRow = screen.getByText("Sample provider").closest("article")!;
    expect(within(pluginRow).getByText("Available")).toBeInTheDocument();
    expect(pluginRow).not.toHaveTextContent("— tools");
    expect(native.invoke.mock.calls.some(([, value]) => value.request.kind === "plugin_reload")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(native.invoke).toHaveBeenCalledWith("workspace_query", { request: { kind: "plugin_reload", project: alpha.id, plugin: "sample" } }));
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });
  it("explains every extension category and exposes native registrations without a project", async () => {
    const runner = { target: state.workspace_runner!, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: ["safe-delete"], can_restart: true };
    api.runnerSettings.mockResolvedValue(runner);
    render(wrap(<ExtensionsPanel state={{ ...state, project: null }} onState={vi.fn()} />));
    for (const [label, purpose] of [["Coding Agents", "codingAgentsPurpose"], ["SSH Resources", "sshPurpose"], ["MCP servers", "mcpPurpose"], ["Native Plugins", "pluginsPurpose"], ["Skills", "skillsPurpose"], ["Instructions", "instructionsPurpose"]] as const) {
      fireEvent.click(screen.getByRole("tab", { name: label }));
      expect(screen.getByText(productText("en-US", purpose))).toBeVisible();
    }
    fireEvent.click(screen.getByRole("tab", { name: "Native Plugins" }));
    expect(await screen.findByText("safe-delete")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Reload" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add a native Tool Plugin" }));
    expect(await screen.findByRole("dialog", { name: "Add a native Tool Plugin" })).toBeInTheDocument();
    expect(api.addRunnerPlugin).not.toHaveBeenCalled();
  });
  it("shows canonical failed Plugin state with recovery guidance rather than a saved-registration status", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation(async (command, value) => {
      const result = await normal(command, value);
      return value.request.kind === "extensions" ? { ...result, plugins: { available: true, catalog: { plugins: [{ plugin: "artifact-tools", name: "Artifact tools", status: "failed", errorCode: "initialize_timeout" }] } } } : result;
    });
    render(wrap(<ExtensionsPanel state={state} onState={vi.fn()} />));
    fireEvent.click(screen.getByRole("tab", { name: "Native Plugins" }));
    const row = (await screen.findByText("Artifact tools")).closest("article")!;
    expect(within(row).getByText("Failed to load")).toBeVisible();
    expect(within(row).getByText(productText("en-US", "pluginFailureHelp"))).toBeVisible();
    expect(row).not.toHaveTextContent("Registered");
    expect(row).not.toHaveTextContent("— tools");
    expect(native.invoke.mock.calls.some(([, value]) => value.request.kind === "plugin_reload")).toBe(false);
    fireEvent.click(within(row).getByRole("button", { name: "Reload" }));
    await waitFor(() => expect(native.invoke).toHaveBeenCalledWith("workspace_query", { request: { kind: "plugin_reload", project: alpha.id, plugin: "artifact-tools" } }));
  });
  it("rejects an older workspace result after a Runner identity change", async () => {
    let complete!: (value: unknown) => void;
    const delayed = new Promise(resolve => { complete = resolve; }); const normal = native.invoke.getMockImplementation()!;
    let overviewCalls = 0;
    native.invoke.mockImplementation((name, value) => value.request.kind === "overview" && overviewCalls++ === 0 ? delayed : normal(name, value));
    const view = render(wrap(<ProjectsPanel />));
    await waitFor(() => expect(overviewCalls).toBe(1));
    const switched = { ...state, workspace_runner: { client_id: "mini", server_url: "http://localhost", config_path: "new-runner.toml" } };
    view.rerender(wrap(<ProjectsPanel />, switched));
    await screen.findByLabelText("2 open sessions");
    await act(async () => { complete({ ...overview, projects: [{ ...alpha, id: "agent:old:other", name: "Stale project", path: "/old" }] }); });
    expect(screen.queryByText("Stale project")).not.toBeInTheDocument();
    expect(screen.getByRole("row", { name: "beta" })).toBeInTheDocument();
  });
  it("uses timestamps, never inferred ChatGPT presence", () => {
    const view = render(<LocaleProvider><ChatgptObservation state={state} /></LocaleProvider>);
    expect(screen.getByText("No ChatGPT activity observed yet")).toBeInTheDocument();
    view.rerender(<LocaleProvider><ChatgptObservation state={{ ...state, chatgpt_activity: { observed: true, last_meaningful_activity_at_ms: Date.now() - 120_000 } }} /></LocaleProvider>);
    expect(screen.getByText(/Last ChatGPT activity/)).toHaveTextContent("2 minutes ago");
    expect(screen.queryByText(/connected|disconnected|Waiting for ChatGPT/i)).not.toBeInTheDocument();
  });
});

describe("cross-platform product vocabulary", () => {
  it("matches Windows drive and UNC paths without collapsing Unix case or root", () => {
    expect(sameProjectPath("C:\\Work\\Repo\\", "c:/work/repo")).toBe(true);
    expect(sameProjectPath("\\\\SERVER\\Share\\Repo", "\\\\server\\share\\repo\\")).toBe(true);
    expect(sameProjectPath("/", "/")).toBe(true);
    expect(sameProjectPath("/work/A", "/work/a")).toBe(false);
    expect(sameProjectPath("", "")).toBe(false);
  });
  it("has a non-empty value for every product term in all six supported Desktop languages", () => {
    for (const [key, values] of Object.entries(PRODUCT_MESSAGES)) {
      expect(values, key).toHaveLength(PRODUCT_LOCALES.length);
      for (const locale of PRODUCT_LOCALES) expect(productText(locale, key as keyof typeof PRODUCT_MESSAGES).trim(), `${locale}:${key}`).not.toBe("");
    }
    expect(sessionTitle("A short title\n\nLong root instruction")).toBe("A short title");
    expect(sessionTitle("x".repeat(4000)).length).toBe(110);
    expect(observationTime(60_000, "en-US", 120_000)).toBe("1 minute ago");
  });
});

describe("Windows project identity and presentation", () => {
  it.each([
    [String.raw`C:\Work\Repo`, "c:/work/repo/"],
    [String.raw`C:\Work\Repo`, String.raw`\\?\C:\Work\Repo`],
    ["C:\\", "\\\\?\\C:\\"], ["D:\\", "\\\\?\\D:\\"],
    [String.raw`\\SERVER\Share\Repo`, "\\\\?\\UNC\\server\\share\\repo\\"],
    [String.raw`\\server\share`, "\\\\?\\UNC\\SERVER\\Share\\"],
  ])("matches %s and %s", (a, b) => expect(sameProjectPath(a, b)).toBe(true));
  it("never replaces distinct authoritative IDs with path identity", () => {
    expect(mergeProjects([alpha], [{ path: alpha.path, runtime_project_id: "agent:other:alpha" }])).toHaveLength(2);
    expect(mergeProjects([alpha], [{ path: "D:\\different", runtime_project_id: alpha.id }])[0]).toBe(alpha);
    expect(mergeProjects([{ ...alpha, id: "" }], [{ path: alpha.path, runtime_project_id: alpha.id }])).toHaveLength(1);
  });
  it.each([
    [String.raw`\\?\C:\foo`, String.raw`C:\foo`],
    [String.raw`\\?\UNC\server\share`, String.raw`\\server\share`],
    ["/work/A", "/work/A"],
  ])("displays %s as %s", (path, expected) => expect(displayProjectPath(path)).toBe(expected));
  it.each([
    ["C:\\", "C:"], ["D:\\", "D:"], ["\\\\?\\C:\\", "C:"], ["\\\\?\\D:\\", "D:"],
    [String.raw`\\?\UNC\server\share`, "share"],
    [String.raw`\\server\share\repo`, "repo"],
  ])("gives %s a useful name", (path, expected) => {
    expect(projectName({ path })).toBe(expected);
    if (expected !== "repo") expect(projectName({ path, name: "Project" })).toBe(expected);
  });
});

describe("inventory convergence", () => {
  it.each([false, true])("hides stale saved rows after complete inventory (empty=%s)", async empty => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, projects: empty ? [] : [beta], visible_project_count: empty ? 0 : 1 })
      : value.request.kind === "projects"
        ? Promise.resolve({ projects: empty ? [] : [beta], total: empty ? 0 : 1, truncated: false })
        : normal(command, value));
    const selected = { ...state, project: null };
    render(wrap(<ProjectsPanel />, selected));
    await waitFor(() => expect(screen.queryByRole("row", { name: "alpha" })).not.toBeInTheDocument());
    if (empty) expect(screen.getByText("No project folders to show")).toBeInTheDocument();    else expect(screen.getByRole("row", { name: "beta" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add Project" })).not.toBeInTheDocument();
  });
  it.each([{ projects_truncated: true }, { connected: false }, { projects_available: false }, { visible_project_count: 1 }])("retains history on incomplete observation %s", async flags => {
    native.invoke.mockResolvedValue({ ...overview, ...flags, projects: [], windows: [] });
    render(wrap(<ProjectsPanel />));
    await waitFor(() => expect(native.invoke).toHaveBeenCalled());
    expect(screen.getByRole("row", { name: "alpha" })).toBeInTheDocument();
  });
});

it("keeps Runner overview when only the default display Project disappears", async () => {
  const view = render(wrap(<ProjectsPanel />));
  await screen.findByLabelText("2 open sessions");
  const calls = native.invoke.mock.calls.filter(([, value]) => value.request.kind === "overview").length;
  const projectless = { ...state, project: null, saved_projects: [] };
  view.rerender(wrap(<ProjectsPanel />, projectless));
  expect(screen.getByRole("row", { name: "beta" })).toBeInTheDocument();
  expect(native.invoke.mock.calls.filter(([, value]) => value.request.kind === "overview")).toHaveLength(calls);
});

describe("Workflow Session loaded-record filters", () => {
  function record(title: string, changes: Partial<WorkflowSession> = {}): WorkflowSession {
    return { ...session, session_id: `fixture-${title}`, title, running_call: false, running_jobs: 0,
      overview: { attention: { open_todos: 0, open_questions: 0, open_risks: 0 } }, ...changes };
  }
  function inventory(rows: WorkflowSession[], truncated = false, scanTruncated = false) {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "overview"
      ? Promise.resolve({ ...overview, recent_sessions: { sessions: rows, truncated, scan_truncated: scanTruncated } }) : normal(command, value));
  }
  async function openSessions(count: number) {
    fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
    await waitFor(() => expect(screen.getByText("Loaded sessions")).toHaveTextContent(`Loaded sessions ${count}`));
    return screen.getByRole("combobox", { name: "Session filter" });
  }

  it("counts loaded records and selects actual calls or jobs, not idle open sessions or uncertain job inventories", async () => {
    inventory([record("Idle fixture"), record("Call fixture", { running_call: true }), record("Job fixture", { running_jobs: 1 }), record("Unknown jobs fixture", { running_jobs_complete: false })]);
    render(wrap(<ActivityPanel activity={[]} />));
    const filter = await openSessions(4);
    expect(screen.getByRole("button", { name: /Idle fixture/ })).toHaveTextContent("Open");
    native.invoke.mockClear();
    fireEvent.change(filter, { target: { value: "running" } });
    expect(screen.getByRole("button", { name: /Call fixture/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Job fixture/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Idle fixture/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Unknown jobs fixture/ })).not.toBeInTheDocument();
    expect(screen.getByText("Matching loaded sessions")).toHaveTextContent("Matching loaded sessions 2");
    expect(native.invoke).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(filter).toHaveValue("all");
    expect(screen.getByText("Matching loaded sessions")).toHaveTextContent("Matching loaded sessions 4");
    expect(native.invoke).not.toHaveBeenCalled();
  });

  it.each(["open_todos", "open_questions", "open_risks"] as const)("finds positive %s without requiring running work", async field => {
    inventory([record("Attention fixture", { overview: { attention: { open_todos: 0, open_questions: 0, open_risks: 0, [field]: 1 } } }), record("Idle fixture")]);
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.change(await openSessions(2), { target: { value: "attention" } });
    expect(screen.getByRole("button", { name: /Attention fixture/ })).toHaveTextContent("Open");
    expect(screen.queryByRole("button", { name: /Idle fixture/ })).not.toBeInTheDocument();
    expect(screen.getByText("Matching loaded sessions")).toHaveTextContent("Matching loaded sessions 1");
  });

  it("includes failed checks and excludes unavailable, inconclusive, not-run and passed checks", async () => {
    const states = ["failed", "unavailable", "inconclusive", "not_run", "passed"];
    inventory(states.map(state => record(`${state} fixture`, { overview: { attention: { open_todos: 0, open_questions: 0, open_risks: 0 }, validation: { state } } })));
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.change(await openSessions(5), { target: { value: "attention" } });
    expect(screen.getByRole("button", { name: /failed fixture/ })).toHaveTextContent("Checks failed");
    for (const state of states.slice(1)) expect(screen.queryByRole("button", { name: new RegExp(`${state} fixture`) })).not.toBeInTheDocument();
  });

  it("combines Project and session filters, clears both on no matches, and preserves partial-history warnings", async () => {
    inventory([record("Alpha running fixture", { running_call: true }), record("Beta idle fixture", { project_id: beta.id })], true);
    render(wrap(<ActivityPanel activity={[]} />));
    const filter = await openSessions(2);
    fireEvent.change(filter, { target: { value: "running" } });
    fireEvent.click(screen.getByRole("button", { name: "Project: All Projects" }));
    const picker = screen.getByRole("dialog", { name: "Project" });
    fireEvent.click(within(picker).getByRole("button", { name: /beta/ }));
    expect(screen.getByText("No matching sessions")).toBeInTheDocument();
    expect(screen.queryByText(productText("en-US", "noSessions"))).not.toBeInTheDocument();
    expect(screen.getByText("Loaded sessions")).toHaveTextContent("Loaded sessions 2");
    expect(screen.getByText("Matching loaded sessions")).toHaveTextContent("Matching loaded sessions 0");
    expect(screen.getByText("History is partial")).toBeInTheDocument();
    expect(screen.getByText(/Counts describe loaded records only/)).toBeInTheDocument();
    native.invoke.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(filter).toHaveValue("all");
    expect(screen.getByRole("button", { name: "Project: All Projects" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Alpha running fixture/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Beta idle fixture/ })).toBeInTheDocument();
    expect(native.invoke).not.toHaveBeenCalled();
  });

  it.each([
    { truncated: true, scanTruncated: false }, { truncated: false, scanTruncated: true },
  ])("keeps filtered empty-state precedence and loaded counts on partial history %j", async flags => {
    inventory([], flags.truncated, flags.scanTruncated);
    render(wrap(<ActivityPanel activity={[]} />));
    const filter = await openSessions(0);
    await screen.findByText(productText("en-US", "noObservedSessions"));
    expect(screen.getByText("History is partial")).toBeInTheDocument();
    native.invoke.mockClear();
    fireEvent.change(filter, { target: { value: "running" } });
    expect(screen.getByText("No matching sessions")).toBeInTheDocument();
    expect(screen.queryByText(productText("en-US", "noObservedSessions"))).not.toBeInTheDocument();
    expect(screen.getByText("Loaded sessions")).toHaveTextContent("Loaded sessions 0");
    expect(screen.getByText("Matching loaded sessions")).toHaveTextContent("Matching loaded sessions 0");
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(filter).toHaveValue("all");
    expect(screen.getByText(productText("en-US", "noObservedSessions"))).toBeInTheDocument();
    expect(screen.queryByText("No matching sessions")).not.toBeInTheDocument();
    expect(screen.getByText("History is partial")).toBeInTheDocument();
    expect(native.invoke).not.toHaveBeenCalled();
  });

  it("keeps Window and System views independent of the session filter", async () => {
    inventory([record("Idle fixture")]);
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.change(await openSessions(1), { target: { value: "running" } });
    expect(screen.getByText("No matching sessions")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Tool calls" }));
    expect(screen.queryByRole("combobox", { name: "Session filter" })).not.toBeInTheDocument();
    expect(await screen.findByRole("button", { name: /Open call details/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Service events" }));
    expect(screen.queryByRole("combobox", { name: "Session filter" })).not.toBeInTheDocument();
    expect(screen.queryByText("No matching sessions")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Workflow Sessions" }));
    expect(screen.getByRole("combobox", { name: "Session filter" })).toHaveValue("running");
  });

  it("resets session filters on Server changes and authorization loss", async () => {
    inventory([record("Idle fixture")]);
    const normal = native.invoke.getMockImplementation()!;
    const boundaryFrames: { kind: string; status: string; project: string }[] = [];
    let observedContext: string | undefined;
    function BoundaryFrames() {
      const current = useWorkspace();
      useLayoutEffect(() => {
        const changed = observedContext !== undefined && observedContext !== current.contextKey;
        observedContext = current.contextKey;
        if (changed || current.error) boundaryFrames.push({ kind: changed ? "context" : "authorization", status: (screen.getByRole("combobox", { name: "Session filter" }) as HTMLSelectElement).value, project: screen.getByRole("button", { name: /^Project:/ }).getAttribute("aria-label") || "" });
      });
      return null;
    }
    const content = <><ActivityPanel activity={[]} /><BoundaryFrames /></>;
    const view = render(wrap(content));
    fireEvent.change(await openSessions(1), { target: { value: "running" } });
    fireEvent.click(screen.getByRole("button", { name: "Project: All Projects" }));
    fireEvent.click(within(screen.getByRole("dialog", { name: "Project" })).getByRole("button", { name: /beta/ }));
    view.rerender(wrap(content, { ...state, topology: { ...state.topology!, server: { kind: "remote", url: "https://fixture.example" } } }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Session filter" })).toHaveValue("all"));
    await screen.findByRole("button", { name: /Idle fixture/ });
    fireEvent.change(screen.getByRole("combobox", { name: "Session filter" }), { target: { value: "attention" } });
    native.invoke.mockRejectedValue({ code: "workspace_permission_denied" });
    fireEvent.click(within(screen.getByRole("heading", { name: "Activity" }).parentElement!).getByRole("button", { name: "Refresh" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("combobox", { name: "Session filter" })).toHaveValue("all");
    expect(screen.queryByRole("button", { name: /Idle fixture/ })).not.toBeInTheDocument();
    native.invoke.mockImplementation(normal);
    fireEvent.click(within(screen.getByRole("heading", { name: "Activity" }).parentElement!).getByRole("button", { name: "Refresh" }));
    await screen.findByRole("button", { name: /Idle fixture/ });
    expect(screen.getByRole("combobox", { name: "Session filter" })).toHaveValue("all");
    expect(boundaryFrames.some(frame => frame.kind === "context")).toBe(true);
    expect(boundaryFrames.some(frame => frame.kind === "authorization")).toBe(true);
    for (const frame of boundaryFrames) {
      expect(frame.status).toBe("all");
      expect(frame.project).toBe("Project: All Projects");
    }
  });

  it("keeps focus and Project keyboard navigation usable at a narrow viewport", async () => {
    const width = window.innerWidth;
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 375 });
    try {
      inventory([record("Idle fixture"), record("Beta running fixture", { project_id: beta.id, running_jobs: 1 })]);
      render(wrap(<ActivityPanel activity={[]} />));
      const first = screen.getByRole("tab", { name: "Tool calls" }); first.focus();
      fireEvent.keyDown(first, { key: "ArrowRight" });
      expect(screen.getByRole("tab", { name: "Workflow Sessions" })).toHaveFocus();
      await waitFor(() => expect(screen.getByText("Loaded sessions")).toHaveTextContent("Loaded sessions 2"));
      const filter = screen.getByRole("combobox", { name: "Session filter" }); filter.focus();
      expect(filter).toHaveFocus(); fireEvent.change(filter, { target: { value: "running" } });
      const trigger = screen.getByRole("button", { name: "Project: All Projects" });
      fireEvent.click(trigger);
      const picker = screen.getByRole("dialog", { name: "Project" });
      fireEvent.keyDown(picker, { key: "ArrowDown" });
      expect(within(picker).getByRole("button", { name: /alpha/ })).toHaveFocus();
      fireEvent.keyDown(picker, { key: "ArrowDown" });
      expect(within(picker).getByRole("button", { name: /beta/ })).toHaveFocus();
      fireEvent.click(document.activeElement!);
      expect(trigger).toHaveFocus();
      expect(screen.getByRole("button", { name: /Beta running fixture/ })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Clear filters" })).toBeInTheDocument();
    } finally { Object.defineProperty(window, "innerWidth", { configurable: true, value: width }); }
  });

  it.each(PRODUCT_LOCALES)("localizes the session filters and loaded-record counts in %s", async locale => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    inventory([record("Idle fixture")]);
    render(wrap(<ActivityPanel activity={[]} />));
    fireEvent.click(screen.getByRole("tab", { name: productText(locale, "sessions") }));
    const filter = screen.getByRole("combobox", { name: productText(locale, "sessionFilter") });
    expect(within(filter).getByRole("option", { name: productText(locale, "allSessions") })).toBeInTheDocument();
    expect(within(filter).getByRole("option", { name: productText(locale, "runningSessions") })).toBeInTheDocument();
    expect(within(filter).getByRole("option", { name: productText(locale, "sessionsNeedAttention") })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(productText(locale, "loadedSessions"))).toHaveTextContent("1"));
    fireEvent.change(filter, { target: { value: "attention" } });
    expect(screen.getByText(productText(locale, "noMatchingSessions"))).toBeInTheDocument();
    expect(screen.getByRole("button", { name: productText(locale, "clearActivityFilters") })).toBeInTheDocument();
  });
});

describe("project Git overview", () => {
  const dirty: GitSummary = { branch: "feat/overview", clean: false, git_available: true, non_git_project: false,
    files: [{ path: "src/export.ts", status: " M" }], files_total: 1, files_truncated: false };
  const observe = (git: GitSummary) => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "project_git" && value.request.project === alpha.id
      ? Promise.resolve(git) : normal(command, value));
  };
  it("shows exact change counts and expands returned files without another query", async () => {
    observe({ ...dirty, files_total: 205, files_truncated: true });
    render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    const summary = await within(row).findByText("Changed files: 205", { selector: "summary" });
    expect(row).toHaveTextContent("feat/overview");
    expect(row).toHaveTextContent("Showing part of the changed file list");
    const reads = native.invoke.mock.calls.length;
    fireEvent.click(summary);
    expect(summary.parentElement).toHaveAttribute("open");
    expect(within(row).getByText("src/export.ts")).toBeInTheDocument();
    expect(within(row).getByText("M")).toBeInTheDocument();
    expect(native.invoke).toHaveBeenCalledTimes(reads);
    fireEvent.click(summary);
    expect(summary.parentElement).not.toHaveAttribute("open");
  });

  it.each([
    ["clean", { ...dirty, clean: true, files: [], files_total: 0 }, "clean"],
    ["non-Git", { ...dirty, branch: null, non_git_project: true, clean: null, files: [], files_total: null }, "notGit"],
    ["unavailable Git", { ...dirty, branch: null, git_available: false, clean: null, files: [], files_total: null }, "projectStatusUnconfirmed"],
    ["unknown cleanliness", { ...dirty, clean: null, files: [], files_total: null }, "projectStatusUnconfirmed"],
    ["unknown count", { ...dirty, files_total: null }, "projectGitHasChanges"],
  ] as const)("distinguishes %s from zero changes", async (_scenario, git, key) => {
    observe({ ...git, files: git.files ? [...git.files] : git.files }); render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    expect(await within(row).findByText(productText("en-US", key))).toBeInTheDocument();
    expect(within(row).queryByText("Changed files: 0")).not.toBeInTheDocument();
    if (key !== "clean") expect(within(row).queryByText(productText("en-US", "clean"))).not.toBeInTheDocument();
  });

  it("keeps dirty state with no returned files and marks a truncated list", async () => {
    observe({ ...dirty, files: [], files_total: 20, files_truncated: true });
    render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    expect(await within(row).findByText("Changed files: 20")).toBeInTheDocument();
    expect(within(row).getByText("Showing part of the changed file list")).toBeInTheDocument();
    expect(within(row).queryByText(productText("en-US", "clean"))).not.toBeInTheDocument();
  });

  it("clears stale changes when refreshing fails", async () => {
    observe(dirty); render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    await within(row).findByText("Changed files: 1");
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "project_git" && value.request.project === alpha.id
      ? Promise.reject(new Error("private probe failure")) : normal(command, value));
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await within(row).findByText(productText("en-US", "projectStatusUnconfirmed"));
    expect(row).not.toHaveTextContent("Changed files: 1");
    expect(row).not.toHaveTextContent("src/export.ts");
    expect(row).not.toHaveTextContent("private probe failure");
  });

  it("shows loading without declaring the workspace clean", async () => {
    let resolve!: (value: GitSummary) => void;
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => value.request.kind === "project_git" && value.request.project === alpha.id
      ? new Promise<GitSummary>(done => { resolve = done; }) : normal(command, value));
    render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    await within(row).findByText(productText("en-US", "loading"));
    expect(row).not.toHaveTextContent(productText("en-US", "clean"));
    await act(async () => resolve(dirty));
    expect(row).toHaveTextContent("Changed files: 1");
  });

  it("does not probe disconnected projects or call them clean", async () => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => {
      const projects = [{ ...alpha, connected: false }, beta];
      if (value.request.kind === "overview") return Promise.resolve({ ...overview, projects });
      if (value.request.kind === "projects") return Promise.resolve({ projects, total: 2, truncated: false });
      return normal(command, value);
    });
    render(wrap(<ProjectsPanel />));
    await screen.findAllByText("feat/export");
    const row = screen.getByRole("row", { name: "alpha" });
    expect(row).toHaveTextContent(productText("en-US", "projectStatusUnconfirmed"));
    expect(native.invoke.mock.calls.some(([, args]) => args.request.kind === "project_git" && args.request.project === alpha.id)).toBe(false);
  });

  it("renders the same Git overview in narrow-screen project cards", async () => {
    const original = window.matchMedia;
    const media = vi.spyOn(window, "matchMedia").mockImplementation(query => ({ ...original(query), matches: query === "(max-width: 600px)" }));
    try {
      observe(dirty); render(wrap(<ProjectsPanel />));
      const card = await screen.findByRole("article", { name: "alpha" });
      const summary = await within(card).findByText("Changed files: 1", { selector: "summary" });
      fireEvent.click(summary);
      expect(within(card).getByText("src/export.ts")).toBeInTheDocument();
      expect(summary.parentElement).toHaveAttribute("open");
    } finally { media.mockRestore(); }
  });

  it.each(PRODUCT_LOCALES)("localizes Git changes in %s", async locale => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    observe({ ...dirty, files_total: 205, files_truncated: true }); render(wrap(<ProjectsPanel />));
    const row = await screen.findByRole("row", { name: "alpha" });
    expect(await within(row).findByText(productText(locale, "projectGitChangeCount").replace("{count}", "205"))).toBeInTheDocument();
    expect(within(row).getByText(productText(locale, "projectGitPartialFiles"))).toBeInTheDocument();
  });
});

describe("project device filters", () => {
  const sharedLocal = { ...alpha, id: "agent:mini:shared", path: "/work/shared", name: "shared", client_id: "mini" };
  const sharedRemote = { ...sharedLocal, id: "agent:remote:shared", client_id: "remote" };
  const explicitRemote = { ...beta, id: "agent:mini:override", client_id: "remote", name: "override" };
  const devices = [{ client_id: "mini", connected: true }, { client_id: "remote", connected: false }, { client_id: "empty", connected: false }];
  const viewer = { ...state, project: null, saved_projects: [], workspace_runner: null,
    topology: { ...state.topology!, server: { kind: "remote" as const, url: "https://central.example" }, runner: { kind: "none" as const } } } as DesktopState;
  const fleet = (projects: WorkspaceProject[] = [sharedLocal, sharedRemote, explicitRemote], runners: ServerRunnerSummary[] = devices) => {
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => {
      if (value.request.kind === "overview") return Promise.resolve({ ...overview, runners, projects, recent_sessions: undefined });
      if (value.request.kind === "projects") return Promise.resolve({ projects, total: projects.length, truncated: false });
      return normal(command, value);
    });
  };

  it("filters exact Runner identities, combines search, and clears both filters", async () => {
    fleet(); render(wrap(<ProjectsPanel />, viewer));
    const filter = screen.getByRole("combobox", { name: productText("en-US", "executionDevice") });
    await waitFor(() => expect(within(filter).getByRole("option", { name: "remote" })).toBeInTheDocument());
    fireEvent.change(filter, { target: { value: "remote" } });
    expect(screen.getAllByRole("row", { name: "shared" })).toHaveLength(1);
    expect(screen.getByRole("row", { name: "shared" })).toHaveTextContent("remote");
    expect(screen.getByRole("row", { name: "override" })).toBeInTheDocument();
    expect(screen.getByText("Showing 2 of 3 loaded projects")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("searchbox", { name: productText("en-US", "search") }), { target: { value: "shared" } });
    expect(screen.queryByRole("row", { name: "override" })).not.toBeInTheDocument();
    expect(screen.getByText("Showing 1 of 3 loaded projects")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(filter).toHaveValue("");
    expect(screen.getByRole("searchbox", { name: productText("en-US", "search") })).toHaveValue("");
    expect(screen.getAllByRole("row", { name: "shared" })).toHaveLength(2);
    expect(within(filter).queryByRole("option", { name: new RegExp(productText("en-US", "thisComputer")) })).not.toBeInTheDocument();
  });

  it("links offline device cards to the focused filter and supports devices without projects", async () => {
    fleet(); render(wrap(<ProjectsPanel onState={vi.fn()} />, viewer));
    const remote = await screen.findByRole("button", { name: "View projects · remote" });
    expect(remote).toHaveAttribute("type", "button");
    fireEvent.click(remote);
    const filter = screen.getByRole("combobox", { name: productText("en-US", "executionDevice") });
    expect(filter).toHaveFocus(); expect(filter).toHaveValue("remote");
    expect(remote).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: /Remove project/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "View projects · empty" }));
    expect(filter).toHaveValue("empty");
    expect(screen.getByText("Showing 0 of 3 loaded projects")).toBeInTheDocument();
    expect(screen.getByText(productText("en-US", "noMatches"))).toBeInTheDocument();
  });

  it("uses canonical agent IDs when client_id is absent and labels only a configured local Runner", async () => {
    fleet([{ ...alpha, client_id: undefined }, { ...beta, client_id: undefined }], devices);
    render(wrap(<ProjectsPanel onState={vi.fn()} />));
    const filter = screen.getByRole("combobox", { name: productText("en-US", "executionDevice") });
    await waitFor(() => expect(within(filter).getByRole("option", { name: `${productText("en-US", "thisComputer")} · mini` })).toBeInTheDocument());
    fireEvent.change(filter, { target: { value: "mini" } });
    expect(screen.getByRole("row", { name: "alpha" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove project alpha" })).toBeInTheDocument();
  });

  it("resets device selection when changing Server context", async () => {
    fleet(); const view = render(wrap(<ProjectsPanel />, viewer));
    fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
    const next = { ...viewer, topology: { ...viewer.topology!, server: { kind: "remote" as const, url: "https://other.example" } } };
    view.rerender(wrap(<ProjectsPanel />, next));
    await waitFor(() => expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveValue(""));
    view.rerender(wrap(<ProjectsPanel />, viewer));
    await waitFor(() => expect(screen.getAllByRole("row", { name: "shared" })).toHaveLength(2));
    expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveValue("");
  });

  it("preserves a selected offline device during stale refresh but clears it on authorization loss", async () => {
    fleet(); render(wrap(<ProjectsPanel />, viewer));
    fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
    native.invoke.mockRejectedValue({ code: "workspace_server_unreachable" });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveValue("remote");
    native.invoke.mockRejectedValue({ code: "workspace_permission_denied" });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveValue(""));
    expect(screen.queryByRole("option", { name: "remote" })).not.toBeInTheDocument();
  });

  it("retains the selected option when a partial refresh omits that device", async () => {
    fleet(); render(wrap(<ProjectsPanel />, viewer));
    fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => {
      if (value.request.kind === "overview") return Promise.resolve({ ...overview, runners: [devices[0]], projects: [sharedLocal], recent_sessions: undefined });
      if (value.request.kind === "projects") return Promise.resolve({ projects: [sharedLocal], total: 3, truncated: true });
      return normal(command, value);
    });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByText(productText("en-US", "partial"));
    const filter = screen.getByRole("combobox", { name: productText("en-US", "executionDevice") });
    expect(filter).toHaveValue("remote");
    expect(within(filter).getByRole("option", { name: "remote" })).toBeInTheDocument();
    expect(screen.getByText("Showing 0 of 1 loaded projects")).toBeInTheDocument();
  });

  it("shows device-filtered project cards on narrow screens", async () => {
    const original = window.matchMedia;
    const media = vi.spyOn(window, "matchMedia").mockImplementation(query => ({ ...original(query), matches: query === "(max-width: 600px)" }));
    try {
      fleet(); render(wrap(<ProjectsPanel />, viewer));
      fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
      expect(await screen.findByRole("article", { name: "shared" })).toHaveTextContent("remote");
      expect(screen.getAllByRole("article", { name: "shared" })).toHaveLength(1);
      expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveFocus();
    } finally { media.mockRestore(); }
  });

  it("keeps the select and results consistent in the first complete refresh frame", async () => {
    const frames: Array<{ device: string; count: string | null }> = [];
    function ObserveFrame() {
      const workspace = useWorkspace();
      useLayoutEffect(() => {
        if (!workspace.loading && workspace.projects.length === 1) frames.push({
          device: (screen.getByRole("combobox", { name: productText("en-US", "executionDevice") }) as HTMLSelectElement).value,
          count: screen.getByRole("status").textContent,
        });
      });
      return null;
    }
    fleet(); render(wrap(<><ProjectsPanel /><ObserveFrame /></>, viewer));
    fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => {
      if (value.request.kind === "overview") return Promise.resolve({ ...overview, runners: [devices[0]], projects: [sharedLocal], recent_sessions: undefined });
      if (value.request.kind === "projects") return Promise.resolve({ projects: [sharedLocal], total: 1, truncated: false });
      return normal(command, value);
    });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByText("Showing 1 of 1 loaded projects");
    expect(frames.length).toBeGreaterThan(0);
    expect(frames.every(frame => frame.device === "" && frame.count === "Showing 1 of 1 loaded projects")).toBe(true);
  });

  it("clears a device absent from a complete refreshed inventory", async () => {
    fleet(); render(wrap(<ProjectsPanel />, viewer));
    fireEvent.click(await screen.findByRole("button", { name: "View projects · remote" }));
    const normal = native.invoke.getMockImplementation()!;
    native.invoke.mockImplementation((command, value) => {
      if (value.request.kind === "overview") return Promise.resolve({ ...overview, runners: [devices[0]], projects: [sharedLocal], recent_sessions: undefined });
      if (value.request.kind === "projects") return Promise.resolve({ projects: [sharedLocal], total: 1, truncated: false });
      return normal(command, value);
    });
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: productText("en-US", "executionDevice") })).toHaveValue(""));
    expect(screen.getByText("Showing 1 of 1 loaded projects")).toBeInTheDocument();
  });
});
