import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DesktopState } from "./models/topology";
import { connectionFixture, connectionSnapshot } from "./test/connections-fixtures";
import { LocaleProvider } from "./i18n/locale";

const workspace = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: workspace.invoke }));

const api = vi.hoisted(() => ({
  getState: vi.fn(),
  computerPermissions: vi.fn(),
  requestComputerPermission: vi.fn(),
  runnerSettings: vi.fn(),
  updateRunnerSettings: vi.fn(),
  restartOwnedRunner: vi.fn(),
  updateTunnelConfig: vi.fn(),
  openPowerShellInstallGuide: vi.fn(),
  refresh: vi.fn(),
  observeChatgptActivity: vi.fn(),
  resumeSavedRuntime: vi.fn(),
  updateTunnelProxy: vi.fn(),
  activity: vi.fn(),
  configureLocal: vi.fn(),
  activateLocalProject: vi.fn(),
  configureRemote: vi.fn(),
  startQuickShare: vi.fn(),
  stopQuickShare: vi.fn(),
  stopLocalRuntime: vi.fn(),
  startRegularTunnel: vi.fn(),
  stopRegularTunnel: vi.fn(),
  saveTunnelProfile: vi.fn(),
  tunnelProfileAction: vi.fn(),
  saveMcpProvider: vi.fn(),
  removeMcpProvider: vi.fn(),
  cancelOperation: vi.fn(),
  inspectProject: vi.fn(),
  getLaunchAtLogin: vi.fn(),
  setLaunchAtLogin: vi.fn(),
}));

const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => clipboard);

const tauriEvents = vi.hoisted(() => ({
  handler: null as null | ((event: { payload: unknown }) => void),
  listen: vi.fn(),
}));

vi.mock("./lib/desktop-api", () => ({
  desktopApi: api,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: tauriEvents.listen,
}));

import App from "./App";
import { open } from "@tauri-apps/plugin-dialog";

const readyState: DesktopState = {
  topology: {
    experience: "full",
    server: { kind: "local" },
    runner: { kind: "local" },
    exposure: { kind: "none" },
    enrollment: { kind: "managed_pairing" },
  },
  readiness: {
    server: "ready",
    runner: "ready",
    exposure: "local_ready",
    project: "ready",
    runtime_ready: true,
    ready_for_chatgpt: false,
    summary: "Runtime ready on this computer",
    summary_kind: "runtime_ready_local_only",
    next_action: "Choose a ChatGPT connection in Connection.",
    next_action_kind: "choose_connection",
  },
  project: {
    path: "C:\\fixture\\repo",
    allowed_root: "C:\\fixture",
    is_git_repository: true,
    runtime_project_id: "agent:desktop:repo",
  },
  binaries: {
    directory: "C:\\fixture\\bin",
    version: "0.3.9",
    git_commit: "0123456789abcdef",
    source: "WEBCODEX_DESKTOP_BIN_DIR",
  },
  quick_share: null,
  connections: connectionSnapshot(connectionFixture({ lifecycle: "stopped", ready: false, pid: null, health: "unknown" })),
  activity_sequence: 0,
  openai_tunnel_configured: true,
  openai_tunnel_config: {
    tunnel_id_present: true,
    api_key_present: true,
    source: "environment",
    saved_tunnel_id: null,
  },
  regular_tunnel_available: true,
  runtime_autostart: false,
  preferred_connection: "no_chat_gpt",
  tunnel_proxy: {
    mode: "auto",
    custom_url: null,
    effective_source: "direct",
    effective_proxy_present: false,
    system_proxy_detected: false,
  },
};

const firstRunState: DesktopState = {
  readiness: {
    server: "unknown",
    runner: "unknown",
    exposure: "unknown",
    project: "none",
    runtime_ready: false,
    ready_for_chatgpt: false,
    summary: "WebCodex Service needs attention",
    summary_kind: "service_needs_attention",
    next_action: "Start or reconnect the WebCodex Service.",
    next_action_kind: "start_or_reconnect_service",
  },
  activity_sequence: 0,
  openai_tunnel_configured: true,
  openai_tunnel_config: {
    tunnel_id_present: true,
    api_key_present: true,
    source: "environment",
    saved_tunnel_id: null,
  },
  regular_tunnel_available: true,
  runtime_autostart: false,
  preferred_connection: "no_chat_gpt",
  tunnel_proxy: {
    mode: "auto",
    custom_url: null,
    effective_source: "direct",
    effective_proxy_present: false,
    system_proxy_detected: false,
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function setupState(): DesktopState {
  return {
    ...readyState,
    readiness: {
      ...readyState.readiness,
      server: "stopped",
      runner: "stopped",
      exposure: "disabled",
      project: "configured",
      runtime_ready: false,
      ready_for_chatgpt: false,
      summary: "WebCodex Service needs attention",
      summary_kind: "service_needs_attention",
      next_action: "Start or reconnect the WebCodex Service.",
      next_action_kind: "start_or_reconnect_service",
    },
    current_operation: null,
  };
}

function localSetupOperationState(
  phase: "running" | "cancelling" = "running",
  activitySequence = 1,
): DesktopState {
  return {
    ...setupState(),
    activity_sequence: activitySequence,
    current_operation: {
      id: "desktop-operation-a",
      kind: "local_setup",
      phase,
      started_at_ms: 1_000,
      cancellable: true,
    },
  };
}

function renderApp() {
  return render(
    <LocaleProvider>
      <App />
    </LocaleProvider>,
  );
}

describe("semantic Desktop UI", () => {
  async function changeServerConnection() {
  fireEvent.click(screen.getByRole("button", { name: "设置" }));
  fireEvent.click(screen.getByRole("button", { name: "高级" }));
  fireEvent.click(screen.getByRole("button", { name: "Server 连接" }));
}
async function editTunnel() {
  fireEvent.click(await screen.findByRole("button", { name: "连接" }));
  fireEvent.click(screen.getByRole("button", { name: "编辑 ChatGPT" }));
  await screen.findByRole("dialog");
}

beforeEach(() => {
    vi.resetAllMocks();
    window.localStorage.removeItem("webcodex.desktop.appearance.v1");
    document.documentElement.removeAttribute("data-theme");
    document.documentElement.removeAttribute("data-appearance");
    tauriEvents.handler = null;
    tauriEvents.listen.mockImplementation(
      async (_eventName: string, handler: (event: { payload: unknown }) => void) => {
        tauriEvents.handler = handler;
        return () => {
          if (tauriEvents.handler === handler) tauriEvents.handler = null;
        };
      },
    );
    workspace.invoke.mockImplementation(async (_command: string, args: { request: { kind: string } }) => {
    switch (args.request.kind) {
      case "overview": return { client_id: "desktop", connected: true, visible_project_count: 0, projects: [], recent_sessions: { sessions: [], truncated: false, scan_truncated: false } };
      case "windows": return { windows: [] };
      case "extensions": return { project: "agent:desktop:repo", runner: "desktop", instructions: { files: [], scan_complete: true, truncated: false }, skills: { available: true, catalog: { skills: [] } }, plugins: { available: true, catalog: { plugins: [] } }, can_reload_plugins: true };
      case "sessions": return { sessions: [], truncated: false };
      default: throw new Error("Unexpected workspace request");
    }
  });

  api.runnerSettings.mockResolvedValue({ target: { config_path: "C:/fixture/runner.toml", client_id: "desktop", server_url: "http://localhost:1234" }, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: [], can_restart: true });
  api.computerPermissions.mockResolvedValue({ supported: false, desktop_accessibility: false, desktop_screen_recording: false });
    api.activity.mockResolvedValue([]);
    api.getLaunchAtLogin.mockResolvedValue(false);
    api.openPowerShellInstallGuide.mockResolvedValue(undefined);
    api.setLaunchAtLogin.mockImplementation(async (enabled: boolean) => enabled);
    api.resumeSavedRuntime.mockResolvedValue(readyState);
    api.observeChatgptActivity.mockResolvedValue(readyState);
    api.updateTunnelProxy.mockResolvedValue(readyState);
    api.startRegularTunnel.mockResolvedValue(readyState);
    api.stopRegularTunnel.mockResolvedValue(readyState);
    api.tunnelProfileAction.mockResolvedValue(readyState);
    api.saveTunnelProfile.mockResolvedValue(readyState);
  });

  it("groups navigation while preserving shortcut order and persists the selected appearance", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await screen.findByLabelText("外观 · 跟随系统");

    expect(screen.getByText("工作")).toBeInTheDocument();
    expect(screen.getByText("配置")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "首页" })).toHaveAttribute("aria-keyshortcuts", "Control+1 Meta+1");
    expect(screen.getByRole("button", { name: "设置" })).toHaveAttribute("aria-keyshortcuts", "Control+6 Meta+6");

    fireEvent.click(screen.getByLabelText("外观 · 跟随系统"));
    fireEvent.click(screen.getByLabelText("外观 · 浅色"));
    await waitFor(() => expect(document.documentElement).toHaveAttribute("data-theme", "dark"));
    expect(document.documentElement).toHaveAttribute("data-appearance", "dark");
    expect(window.localStorage.getItem("webcodex.desktop.appearance.v1")).toBe("dark");
  });

  it("keeps a user stop consistent after refresh and offers Start", async () => {
    const stopped = { ...setupState(), readiness: { ...setupState().readiness, summary_kind: "runtime_stopped" as const } };
    api.getState.mockResolvedValue(stopped); api.refresh.mockResolvedValue(stopped);
    renderApp();
    await screen.findByRole("heading", { name: "repo", level: 1 });
    expect(screen.getByRole("status")).toHaveTextContent("Server已停止Runner已停止");
    expect(screen.getByRole("button", { name: "启动 WebCodex" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await waitFor(() => expect(api.refresh).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("status")).toHaveTextContent("Runner已停止");
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("retries and repeats Tunnel ID copying without restarting or exposing an API key", async () => {
    clipboard.writeText.mockRejectedValueOnce(new Error("clipboard denied")).mockResolvedValue(undefined);
    api.getState.mockResolvedValue({ ...readyState,
      openai_tunnel_config: { ...readyState.openai_tunnel_config, effective_tunnel_id: "tunnel_fixture" },
      connections: connectionSnapshot(connectionFixture()),
    });
    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    const copy = screen.getByRole("button", { name: "复制 ID ChatGPT" });
    fireEvent.click(copy);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    fireEvent.click(copy); await waitFor(() => expect(copy).toHaveTextContent("已复制"));
    fireEvent.click(copy);
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledTimes(3));
    expect(clipboard.writeText).toHaveBeenLastCalledWith("tunnel_fixture");
    expect(api.startRegularTunnel).not.toHaveBeenCalled(); expect(api.stopRegularTunnel).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "编辑 ChatGPT" }));
    expect(await screen.findByLabelText("API Key")).toHaveValue("");
  });

  it("shows named project results and hides routine process events", async () => {
    api.getState.mockResolvedValue(readyState);
    api.activity.mockResolvedValue([
      { sequence: 1, timestamp_ms: 1, source: "desktop", level: "info", event_kind: "operation_started", message: "Desktop operation started: local_project_activate" },
      { sequence: 2, timestamp_ms: 2, source: "runner", level: "info", event_kind: "process_started", message: "" },
      { sequence: 3, timestamp_ms: 3, source: "desktop", level: "info", event_kind: "project_activated", message: "sample-project" },
    ]);
    renderApp(); await screen.findByRole("heading", { name: /^(WebCodex|repo)/, level: 1 });
    fireEvent.click(screen.getByRole("button", { name: "活动" }));
    expect(screen.getByRole("tab", { name: "ChatGPT 调用" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByText("已切换到 sample-project")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "系统事件" }));
    await screen.findByText("已切换到 sample-project");
    expect(screen.getAllByRole("article")).toHaveLength(2);
    expect(screen.getAllByRole("article")[0]).toHaveTextContent("已切换到 sample-project");
  });

  it("starts the named profile only after explicit action and permits retry", async () => {
    api.getState.mockResolvedValue(readyState);
    api.tunnelProfileAction.mockRejectedValueOnce({ code: "tunnel_unavailable", message: "private-error-must-not-render" });
    renderApp(); fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
    const start = screen.getByRole("button", { name: "启动 ChatGPT" });
    fireEvent.click(start); expect(await screen.findByRole("alert")).toHaveTextContent("未能应用更改");
    expect(document.body.textContent).not.toContain("private-error-must-not-render");
    await waitFor(() => expect(start).toBeEnabled()); fireEvent.click(start);
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenCalledTimes(2));
    expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("default", "start");
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("keeps a failed owned profile stoppable without duplicating its process", async () => {
    api.getState.mockResolvedValue({ ...readyState, connections: connectionSnapshot(connectionFixture({
      lifecycle: "error", ready: false, last_error: "stop_failed",
      process_started: true, process_ready: true, tunnel_ready: false, local_mcp_ready: true,
      failure_stage: "tunnel_control_plane", reason_code: "tunnel_control_plane_probe_failed",
    })) });
    api.tunnelProfileAction.mockRejectedValueOnce({ code: "tunnel_unavailable", message: "Stop failed" });
    renderApp(); fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(screen.queryByRole("button", { name: "启动 ChatGPT" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("高级 · ChatGPT"));
    expect(screen.getByText("tunnel_control_plane")).toBeInTheDocument();
    expect(screen.getByText("tunnel_control_plane_probe_failed")).toBeInTheDocument();
    const stop = screen.getByRole("button", { name: "停止 ChatGPT" });
    fireEvent.click(stop); expect(await screen.findByRole("alert")).toHaveTextContent("未能应用更改");
    await waitFor(() => expect(stop).toBeEnabled()); fireEvent.click(stop);
    await waitFor(() => expect(screen.getByRole("button", { name: "启动 ChatGPT" })).toBeEnabled());
    expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("default", "stop");
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("shows the current project before optional diagnostics and handles picker errors", async () => {
    api.getState.mockResolvedValue(readyState);
    vi.mocked(open).mockRejectedValueOnce({ code: "project_invalid", message: "Picker unavailable", next_action: "Retry." });
    renderApp(); await screen.findByRole("heading", { level: 3, name: "repo" });
    expect(screen.queryByText("当前项目")).not.toBeInTheDocument();
    expect(screen.queryByText(/responsible process|Runtime Bearer|Runner 中执行/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "添加项目" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Picker unavailable");
    expect(screen.getByRole("heading", { level: 3, name: "repo" })).toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
  });

  it("saves a selected profile with write-only credentials through one backend mutation", async () => {
    let observed = readyState;
    api.getState.mockImplementation(async () => observed);
    const saved = { ...readyState, connections: connectionSnapshot(connectionFixture({ tunnel_id: "tunnel_saved", revision: 2 })) };
    const submitted: unknown[] = [];
    api.saveTunnelProfile.mockImplementation(async request => {
      submitted.push(structuredClone(request));
      observed = saved;
      return saved;
    });
    renderApp(); await editTunnel();
    fireEvent.change(screen.getByLabelText("Tunnel ID"), { target: { value: "tunnel_saved" } });
    const key = screen.getByLabelText("API Key"); expect(key).toHaveAttribute("type", "password");
    fireEvent.change(key, { target: { value: "test-only-key" } });
    fireEvent.click(screen.getByRole("button", { name: "保存并应用" }));
    await waitFor(() => expect(submitted).toHaveLength(1));
    expect(submitted[0]).toMatchObject({ id: "default", tunnel_id: "tunnel_saved", api_key: "test-only-key", expected_revision: 1 });
    await waitFor(() => expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument());
    expect(document.body.textContent).not.toContain("test-only-key");
    expect(api.startRegularTunnel).not.toHaveBeenCalled(); expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "编辑 ChatGPT" }));
    expect(screen.getByLabelText("API Key")).toHaveValue("");
    fireEvent.click(screen.getByRole("button", { name: "保存并应用" }));
    await waitFor(() => expect(submitted).toHaveLength(2));
    expect(submitted[1]).toMatchObject({ id: "default", api_key: null, expected_revision: 2 });
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it("clears submitted keys on save failure and never submits runtime setup from a credential input", async () => {
    api.getState.mockResolvedValue(firstRunState);
    api.updateTunnelConfig.mockRejectedValue({ code: "tunnel_config_save_failed", message: "Could not save", next_action: "Retry." });
    renderApp(); fireEvent.click(await screen.findByRole("button", { name: /在此电脑使用 WebCodex/ }));
    fireEvent.click(screen.getByText("可选：检查 ChatGPT 安全隧道配置", { selector: "summary" }));
    const tunnelInput = screen.getByLabelText("Tunnel ID");
    fireEvent.change(tunnelInput, { target: { value: "tunnel_test" } });
    const key = screen.getByLabelText("API Key"); fireEvent.change(key, { target: { value: "test-only-key" } });
    const form = key.closest("form")!;
    expect(form.parentElement?.closest("form")).toBeNull();
    fireEvent.submit(form);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled(); expect(key).toHaveValue(""); expect(tunnelInput).toHaveValue("tunnel_test");
  });

  it("supports keyboard navigation without intercepting activity search typing", async () => {
    api.getState.mockResolvedValue(readyState);
    api.activity.mockResolvedValue([
      { sequence: 1, timestamp_ms: 1, source: "runner", level: "info", event_kind: "process_started", message: "" },
      { sequence: 2, timestamp_ms: 2, source: "service", level: "error", event_kind: "process_exited", message: "" },
    ]);
    renderApp();
    await screen.findByRole("button", { name: "活动" });
    const language = screen.getByRole("button", { name: "界面语言" });
    language.focus();
    fireEvent.keyDown(language, { key: "3", metaKey: true });
    fireEvent.click(screen.getByRole("tab", { name: "系统事件" }));
    const search = screen.getByRole("searchbox");
    await waitFor(() => expect(screen.getAllByRole("article")).toHaveLength(1));
    fireEvent.click(screen.getByRole("checkbox", { name: "显示进程详情" }));
    expect(screen.getAllByRole("article")).toHaveLength(2);
    fireEvent.click(screen.getByRole("checkbox", { name: "只看警告和错误" }));
    expect(screen.getAllByRole("article")).toHaveLength(1);
    fireEvent.change(search, { target: { value: "no matching source" } });
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    expect(screen.getByText("没有匹配的活动，请调整搜索或筛选条件。")).toBeInTheDocument();
    fireEvent.keyDown(search, { key: "a", ctrlKey: true });
    expect(screen.getByRole("searchbox")).toBeInTheDocument();
    fireEvent.change(search, { target: { value: "" } });
    fireEvent.click(screen.getByRole("checkbox", { name: "只看警告和错误" }));
    expect(screen.getAllByRole("article")).toHaveLength(2);
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    expect(screen.getByRole("button", { name: "项目" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("adds a local project while preserving an active Tunnel", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      connections: connectionSnapshot(connectionFixture()),
      preferred_connection: "open_ai_tunnel",
    };
    const projectC = {
      path: "C:\\work\\next",
      allowed_root: "C:\\work\\next",
      is_git_repository: true,
      runtime_project_id: "agent:desktop:next",
    };
    const switched: DesktopState = { ...tunneledState, project: projectC };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.activateLocalProject.mockResolvedValue(switched);
    vi.mocked(open).mockResolvedValue(projectC.path);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    expect(screen.getByRole("heading", { level: 1, name: /此 Runner 的项目/ })).toBeInTheDocument();
    expect(screen.getByRole("main")).toHaveFocus();
    expect(screen.queryByRole("button", { name: /切换项目|Use project|Select project/ })).not.toBeInTheDocument();
    expect(api.activateLocalProject).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "添加项目" }));

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith(projectC.path));
    expect(api.configureLocal).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText(projectC.path)).toBeInTheDocument());
    expect(screen.queryByRole("button", { name: /在此电脑使用 WebCodex/ })).not.toBeInTheDocument();
    expect(screen.getByText("连接 · 1 / 1")).toBeInTheDocument();
    expect(screen.queryByText(/等待 ChatGPT|ChatGPT 未连接/)).not.toBeInTheDocument();
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
  });

  it("adds a project to a ready projectless Runtime without opening setup", async () => {
    const noProject: DesktopState = { ...readyState, project: null, readiness: { ...readyState.readiness, project: "none" } };
    vi.mocked(open).mockResolvedValue("/tmp/new-project");
    api.activateLocalProject.mockResolvedValue(readyState);
    api.getState.mockResolvedValue(noProject);
    api.observeChatgptActivity.mockResolvedValue(noProject);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: /添加项目/ }));

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith("/tmp/new-project"));
    expect(screen.queryByRole("button", { name: /在此电脑使用 WebCodex/ })).not.toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
  });

  it("reuses a saved remote Runner when adding a project after the default was removed", async () => {
    const serverUrl = "https://server.example.test";
    const projectlessRemote: DesktopState = {
      ...readyState,
      workspace_runner: { config_path: "C:/fixture/runner.toml", client_id: "desktop", server_url: serverUrl },
      topology: {
        experience: "full",
        server: { kind: "remote", url: serverUrl },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: serverUrl },
        enrollment: { kind: "managed_pairing" },
      },
      project: null,
      saved_projects: [],
      readiness: { ...readyState.readiness, project: "none" },
    };
    const projectB = {
      path: "C:\\fixture\\project-b",
      allowed_root: "C:\\fixture\\project-b",
      is_git_repository: true,
      runtime_project_id: null,
    };
    api.getState.mockResolvedValue(projectlessRemote);
    api.observeChatgptActivity.mockResolvedValue(projectlessRemote);
    api.inspectProject.mockResolvedValue(projectB);
    api.configureRemote.mockResolvedValue({
      ...projectlessRemote,
      project: { ...projectB, runtime_project_id: "agent:desktop:project-b" },
      readiness: { ...projectlessRemote.readiness, project: "ready" },
    });
    vi.mocked(open).mockResolvedValue(projectB.path);

    renderApp();
    await screen.findByRole("heading", { level: 1, name: "WebCodex" });
    await changeServerConnection();
    fireEvent.click(screen.getByRole("button", { name: /连接现有 Server/ }));
    expect(screen.getByText("将复用现有连接")).toBeInTheDocument();
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "选择文件夹" }));
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(projectB.path));
    fireEvent.click(screen.getByRole("button", { name: "重新连接电脑" }));
    await waitFor(() => expect(api.configureRemote).toHaveBeenCalledWith(serverUrl, "", projectB.path));
  });

  it("reports a legacy Runner restart requirement without implicitly restarting any process", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      connections: connectionSnapshot(connectionFixture()),
      preferred_connection: "open_ai_tunnel",
    };
    const projectC = { ...readyState.project!, path: "C:\\work\\legacy", allowed_root: "C:\\work\\legacy" };
    const switched: DesktopState = { ...tunneledState, project: projectC };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.activateLocalProject.mockRejectedValue({
      code: "project_activation_restart_required",
      message: "Runner restart required",
      next_action: "Refresh this Runner.",
    });
    api.configureLocal.mockResolvedValue(switched);
    vi.mocked(open).mockResolvedValue(projectC.path);

    renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "项目" }));
    fireEvent.click(screen.getByRole("button", { name: "添加项目" }));

    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledWith(projectC.path));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(screen.getByText("连接 · 1 / 1")).toBeInTheDocument();
    expect(screen.queryByText(/等待 ChatGPT|ChatGPT 未连接/)).not.toBeInTheDocument();
  });

  it("does not start a duplicate Tunnel when local setup runs while one is already active", async () => {
    const tunneledState: DesktopState = {
      ...readyState,
      topology: { ...readyState.topology!, exposure: { kind: "open_ai_tunnel" } },
      connections: connectionSnapshot(connectionFixture()),
      preferred_connection: "open_ai_tunnel",
    };
    api.getState.mockResolvedValue(tunneledState);
    api.observeChatgptActivity.mockResolvedValue(tunneledState);
    api.configureLocal.mockResolvedValue(tunneledState);

    renderApp();
    await screen.findByRole("heading", { level: 1, name: "repo" });
    await changeServerConnection();
    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 WebCodex/ }));
    expect(screen.queryByRole("checkbox", { name: "配置完成后连接 ChatGPT" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "配置 WebCodex" }));

    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledWith(tunneledState.project!.path));
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ })).toBeInTheDocument();
  });

  it("navigates by accessible role/name and marks the current page", async () => {
    api.getState.mockResolvedValue(readyState); renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    const home = screen.getByRole("button", { name: "首页" }); expect(home).toHaveAttribute("aria-current", "page");
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(screen.getByRole("heading", { level: 1, name: "连接" })).toBeInTheDocument();
    expect(home).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("button", { name: "连接" })).toHaveAttribute("aria-current", "page");
    expect(within(screen.getByRole("article", { name: "ChatGPT" })).getByRole("heading", { name: "ChatGPT", level: 2 })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "编辑 ChatGPT" })).toBeEnabled();
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  });

  it("separates observed ChatGPT use from Desktop-managed tunnel state", async () => {
    const observed = { ...readyState, chatgpt_activity: { observed: true, last_meaningful_activity_at_ms: Date.now() - 40_000 } };
    api.getState.mockResolvedValue(observed); api.observeChatgptActivity.mockResolvedValue(observed); renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    expect(screen.getByText(/最近 ChatGPT 活动/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("连接0 / 1 运行中");
    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(screen.getByText(/最近 ChatGPT 活动/)).toBeInTheDocument();
    expect(screen.queryByText(/ChatGPT 未连接|等待 ChatGPT|已验证 ChatGPT 使用/)).not.toBeInTheDocument();
  });

  it("does not equate an unmanaged Desktop tunnel with ChatGPT being disconnected", async () => {
    api.getState.mockResolvedValue(readyState); renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    expect(screen.getByText("尚未观察到 ChatGPT 活动")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Server运行中Runner运行中连接0 / 1 运行中");
    expect(screen.queryByText(/ChatGPT 未连接|等待 ChatGPT|不代表 ChatGPT/)).not.toBeInTheDocument();
  });

  it("rechecks ChatGPT use when the user returns to an already-open Desktop", async () => {
    vi.useFakeTimers();
    try {
      const observed: DesktopState = {
        ...readyState,
        chatgpt_activity: {
          observed: true,
          last_meaningful_activity_at_ms: 5_000,
        },
      };
      let serverObserved = false;
      api.getState.mockImplementation(async () => serverObserved ? observed : readyState);
      api.observeChatgptActivity.mockImplementation(async () => serverObserved ? observed : readyState);
      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(1);
      expect(screen.queryByText("已验证 ChatGPT 使用")).not.toBeInTheDocument();

      await act(async () => {
        window.dispatchEvent(new Event("blur"));
        await Promise.resolve();
      });
      serverObserved = true;
      await act(async () => {
        window.dispatchEvent(new Event("focus"));
        await Promise.resolve();
        await Promise.resolve();
      });

      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(2);
      expect(screen.getByText(/最近 ChatGPT 活动/)).toBeInTheDocument();
      expect(screen.queryByText(/ChatGPT 未连接|等待 ChatGPT/)).not.toBeInTheDocument();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(60_000);
      });
      expect(api.observeChatgptActivity).toHaveBeenCalledTimes(4);
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens a write-only Tunnel editor only on request and cancels without effects", async () => {
    const missing = { ...readyState, connections: connectionSnapshot() };
    api.getState.mockResolvedValue(missing); renderApp();
    fireEvent.click(await screen.findByRole("button", { name: "连接" }));
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "添加连接" }));
    expect(await screen.findByLabelText("API Key")).toHaveValue(""); expect(screen.getByLabelText("Tunnel ID")).toHaveValue("");
    expect(screen.getByRole("button", { name: "保存并应用" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("名称"), { target: { value: "ChatGPT Personal" } });
    fireEvent.change(screen.getByLabelText("Tunnel ID"), { target: { value: "tunnel_fixture" } });
    expect(screen.getByRole("button", { name: "保存并应用" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "test-only" } });
    expect(screen.getByRole("button", { name: "保存并应用" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
    expect(api.saveTunnelProfile).not.toHaveBeenCalled(); expect(api.tunnelProfileAction).not.toHaveBeenCalled();
  });

  it("navigates to existing Activity and Settings pages from the tray host event", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    await waitFor(() => expect(tauriEvents.handler).not.toBeNull());

    act(() => {
      tauriEvents.handler?.({ payload: "settings" });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Desktop 设置" })).toBeInTheDocument();

    act(() => {
      tauriEvents.handler?.({ payload: "activity" });
    });
    expect(await screen.findByRole("heading", { level: 1, name: "活动" })).toBeInTheDocument();
    await waitFor(() => expect(api.activity).toHaveBeenCalled());

    act(() => {
      tauriEvents.handler?.({ payload: "https://example.invalid" });
    });
    expect(screen.getByRole("button", { name: "活动" })).toHaveAttribute("aria-current", "page");
  });

  it("reads and updates Launch at Login through the narrow Desktop host API", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    const launchAtLogin = await screen.findByRole("checkbox", { name: "登录时启动 WebCodex" });
    await waitFor(() => expect(launchAtLogin).toBeEnabled());
    expect(launchAtLogin).not.toBeChecked();
    expect(screen.getByText("继续在后台运行")).toBeInTheDocument();

    fireEvent.click(launchAtLogin);
    await waitFor(() => expect(api.setLaunchAtLogin).toHaveBeenCalledWith(true));
    await waitFor(() => expect(launchAtLogin).toBeChecked());
  });

  it("keeps a fresh Desktop in product setup until the user chooses its real project", async () => {
    api.getState.mockResolvedValue(firstRunState);
    renderApp();

    expect(await screen.findByRole("button", { name: /在此电脑使用 WebCodex/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /连接现有 Server/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /快速共享项目/ })).toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 WebCodex/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "在此电脑配置 WebCodex" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "选择文件夹" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "配置 WebCodex" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "配置 WebCodex" }).closest("form")!);
    expect(api.configureLocal).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "← 返回全部配置方式" }));
    fireEvent.click(screen.getByRole("button", { name: /快速共享项目/ }));
    expect(screen.getByRole("radiogroup", { name: "Quick Share 连接方式" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Cloudflare/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /OpenAI Secure Tunnel/ })).not.toBeChecked();
  });

  it("guides Windows users to PowerShell 7 without blocking the 5.1 fallback", async () => {
    const missingPwsh: DesktopState = {
      ...firstRunState,
      powershell_runtime: {
        pwsh_available: false,
        windows_powershell_available: true,
      },
    };
    const detectedPwsh: DesktopState = {
      ...missingPwsh,
      powershell_runtime: {
        pwsh_available: true,
        windows_powershell_available: true,
      },
    };
    api.getState.mockResolvedValueOnce(missingPwsh).mockResolvedValueOnce(detectedPwsh);
    renderApp();

    fireEvent.click(await screen.findByRole("button", { name: /在此电脑使用 WebCodex/ }));
    const guidance = screen.getByText("建议安装 PowerShell 7").closest("article");
    expect(guidance).not.toBeNull();
    expect(guidance).toHaveTextContent("Windows PowerShell 5.1");
    expect(guidance).toHaveTextContent("winget install --id Microsoft.PowerShell --source winget");
    expect(screen.getByRole("button", { name: "配置 WebCodex" })).toBeDisabled();

    vi.mocked(open).mockResolvedValue(readyState.project!.path);
    api.inspectProject.mockResolvedValue(readyState.project);
    fireEvent.click(screen.getByRole("button", { name: "选择文件夹" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "配置 WebCodex" })).toBeEnabled());

    fireEvent.click(within(guidance!).getByRole("button", { name: "打开 Microsoft 安装说明" }));
    await waitFor(() => expect(api.openPowerShellInstallGuide).toHaveBeenCalledTimes(1));
    fireEvent.click(within(guidance!).getByRole("button", { name: "重新检测" }));
    await waitFor(() => expect(screen.queryByText("建议安装 PowerShell 7")).not.toBeInTheDocument());
    expect(api.configureLocal).not.toHaveBeenCalled();
  });

  it("keeps first-run errors and the selected project through intermediate polling and Tunnel failure", async () => {
    vi.useFakeTimers();
    try {
      const setupResult = deferred<DesktopState>();
      api.getState.mockResolvedValue(firstRunState);
      api.configureLocal.mockReturnValueOnce(setupResult.promise).mockResolvedValue(readyState);
      api.startRegularTunnel.mockRejectedValue({ code: "tunnel_unavailable", message: "Tunnel failed", next_action: "Retry." });
      vi.mocked(open).mockResolvedValue(readyState.project!.path);
      api.inspectProject.mockResolvedValue(readyState.project);
      const view = renderApp();
      await act(async () => {});
      fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 WebCodex/ }));
      fireEvent.click(screen.getByRole("button", { name: "选择文件夹" }));
      await act(async () => {});
      fireEvent.click(screen.getByRole("button", { name: "配置 WebCodex" }));
      expect(api.configureLocal).toHaveBeenCalledWith(readyState.project!.path);

      api.getState.mockResolvedValue(localSetupOperationState());
      await act(async () => { await vi.advanceTimersByTimeAsync(1_500); });
      expect(screen.getByRole("heading", { level: 1, name: "在此电脑配置 WebCodex" })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "更改文件夹" })).toBeDisabled();

      api.getState.mockResolvedValue(setupState());
      await act(async () => {
        setupResult.reject({ code: "project_not_loaded", message: "Project failed", next_action: "Retry." });
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(screen.getByRole("alert")).toHaveTextContent("project_not_loaded");
      fireEvent.click(screen.getByRole("button", { name: "重新激活项目" }));
      await act(async () => {});
      expect(screen.getByRole("alert")).toHaveTextContent("tunnel_unavailable");
      expect(screen.getByRole("heading", { level: 1, name: "在此电脑配置 WebCodex" })).toBeInTheDocument();
      api.startRegularTunnel.mockResolvedValue(readyState);
      fireEvent.click(screen.getByRole("button", { name: "配置 WebCodex" }));
      await act(async () => {});
      expect(screen.getByRole("heading", { level: 1, name: /^(WebCodex|repo)/ })).toBeInTheDocument();
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows an initial status failure and retries the complete fresh-start bootstrap", async () => {
    const retryState = deferred<DesktopState>();
    api.getState
      .mockRejectedValueOnce({
        code: "desktop_state_unavailable",
        message: "Desktop status is temporarily unavailable",
        next_action: "Retry reading the Desktop status.",
      })
      .mockReturnValueOnce(retryState.promise)
      .mockResolvedValue(readyState);
    renderApp();

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("desktop_state_unavailable");
    expect(screen.queryByText("正在加载 WebCodex…")).not.toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("status")).toHaveTextContent("正在加载 WebCodex…");
    expect(screen.queryByRole("button", { name: "重试" })).not.toBeInTheDocument();
    await waitFor(() => expect(api.getState).toHaveBeenCalledTimes(2));

    await act(async () => {
      retryState.resolve(firstRunState);
    });
    expect(await screen.findByRole("button", { name: /在此电脑使用 WebCodex/ })).toBeInTheDocument();
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps repeated initial status failures visible without automatically retrying", async () => {
    api.getState.mockRejectedValue({
      code: "desktop_state_unavailable",
      message: "Desktop status is unavailable",
      next_action: "Retry reading the Desktop status.",
    });
    renderApp();

    expect(await screen.findByRole("alert")).toHaveTextContent("desktop_state_unavailable");
    expect(api.getState).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("desktop_state_unavailable");
    expect(screen.getByRole("button", { name: "重试" })).toBeEnabled();
    expect(api.getState).toHaveBeenCalledTimes(2);
    expect(api.configureLocal).not.toHaveBeenCalled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("renders localized failures as an alert while keeping safe diagnostics available", async () => {
    const setupState: DesktopState = {
      ...readyState,
      readiness: {
        ...readyState.readiness,
        runtime_ready: false,
        ready_for_chatgpt: false,
        server: "stopped",
        runner: "stopped",
        project: "configured",
        exposure: "disabled",
        summary_kind: "service_needs_attention",
        next_action_kind: "start_or_reconnect_service",
      },
    };
    api.getState.mockResolvedValue(setupState);
    api.refresh.mockResolvedValue(setupState);
    api.resumeSavedRuntime.mockRejectedValue({
      code: "server_start_failed",
      message: "The Desktop-owned WebCodex Server exited during startup (exit code 1). failed to bind HTTP listener 127.0.0.1:54611 (os error 10013)",
      next_action: "Check diagnostics.",
    });

    renderApp();
    const submit = await screen.findByRole("button", { name: "启动 WebCodex" });
    fireEvent.click(submit);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("WebCodex 服务不可用");
    expect(alert).toHaveTextContent("server_start_failed");
    expect(alert).toHaveTextContent("127.0.0.1:54611 (os error 10013)");
    expect(alert).toHaveTextContent("exit code 1");
  });

  it("offers a product recovery action when the selected project did not load", async () => {
    api.getState.mockResolvedValue(readyState);
    api.configureLocal
      .mockRejectedValueOnce({
        code: "project_not_loaded",
        message: "The selected project did not become ready in the Desktop-owned Runner",
        next_action: "Retry project setup.",
      })
      .mockResolvedValueOnce(readyState);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });
    await changeServerConnection();
    fireEvent.click(screen.getByRole("button", { name: /在此电脑使用 WebCodex/ }));
    fireEvent.click(screen.getByRole("button", { name: "配置 WebCodex" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("项目尚未就绪");
    expect(alert).not.toHaveTextContent("registry");
    fireEvent.click(screen.getByRole("button", { name: "重新激活项目" }));
    await waitFor(() => expect(api.configureLocal).toHaveBeenCalledTimes(2));
  });

  it("uses the backend restart_quick_share presentation identity after a stopped share", async () => {
    const stoppedShare: DesktopState = {
      ...readyState,
      topology: {
        experience: "quick_share",
        server: { kind: "local" },
        runner: { kind: "local" },
        exposure: { kind: "cloudflare" },
        enrollment: { kind: "existing_profile", profile: "temporary_share" },
      },
      readiness: {
        server: "stopped",
        runner: "stopped",
        exposure: "error",
        project: "configured",
        runtime_ready: false,
        ready_for_chatgpt: false,
        summary: "Quick Share stopped",
        summary_kind: "quick_share_stopped",
        next_action: "Start Quick Share again.",
        next_action_kind: "restart_quick_share",
      },
      quick_share: {
        provider: "cloudflare",
        project: "C:\\fixture\\repo",
        mcp_url: null,
        clipboard_state: "unavailable",
        clipboard_contains: "bearer_credential",
        ready_for_chatgpt: false,
      },
    };
    api.getState.mockResolvedValue(stoppedShare);
    api.refresh.mockResolvedValue(stoppedShare);
    renderApp();

    expect(await screen.findByRole("button", { name: "重新启动 Quick Share" })).toBeEnabled();
    expect(api.resumeSavedRuntime).not.toHaveBeenCalled();
  });

  it("does not offer a duplicate start when the regular tunnel is running but handoff is degraded", async () => {
    const degradedTunnel: DesktopState = {
      ...readyState,
      topology: {
        ...readyState.topology!,
        exposure: { kind: "open_ai_tunnel" },
      },
      readiness: {
        ...readyState.readiness,
        exposure: "degraded",
        ready_for_chatgpt: false,
        summary: "ChatGPT connection is not verified",
        summary_kind: "connection_unverified",
        next_action: "Restore clipboard access, then restart the secure tunnel handoff.",
        next_action_kind: "restore_clipboard_handoff",
      },
      connections: connectionSnapshot(connectionFixture()),
    };
    api.getState.mockResolvedValue(degradedTunnel);
    api.refresh.mockResolvedValue(degradedTunnel);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(await screen.findByRole("status", { name: "工作区" })).toHaveTextContent("连接1 / 1 运行中");
    expect(screen.queryByRole("button", { name: "启动" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止 ChatGPT" })).toBeInTheDocument();
  });

  it("keeps navigation and Activity usable while a pending setup is globally observable", async () => {
    vi.useFakeTimers();
    try {
      const initial = setupState();
      const running = localSetupOperationState("running", 1);
      const runningWithNewActivity = localSetupOperationState("running", 2);
      const setupResult = deferred<DesktopState>();
      api.getState
        .mockResolvedValueOnce(initial)
        .mockResolvedValueOnce(running)
        .mockResolvedValueOnce(runningWithNewActivity)
        .mockResolvedValue(runningWithNewActivity);
      api.refresh.mockResolvedValue(initial);
      api.resumeSavedRuntime.mockReturnValue(setupResult.promise);
      api.computerPermissions.mockResolvedValue({ supported: false, desktop_accessibility: false, desktop_screen_recording: false });
    api.activity.mockResolvedValue([]);

      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      fireEvent.click(screen.getByRole("button", { name: "启动 WebCodex" }));
      expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_500);
      });

      const operationStatus = screen.getByRole("status", { name: "当前 Desktop 操作" });
      expect(operationStatus).toHaveTextContent("正在配置本机 WebCodex");
      expect(operationStatus).toHaveTextContent("停止当前操作不会自动重复执行尚未确认的步骤");

      fireEvent.click(screen.getByRole("button", { name: "活动" }));
      expect(screen.getByRole("heading", { level: 1, name: "活动" })).toBeInTheDocument();
      expect(api.activity).toHaveBeenCalledTimes(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(api.activity).toHaveBeenCalledTimes(2);

      setupResult.resolve(readyState);
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("cancels only the observed operation id and presents the cancelling phase", async () => {
    const running = localSetupOperationState("running", 1);
    const cancelling = localSetupOperationState("cancelling", 2);
    api.getState.mockResolvedValueOnce(running).mockResolvedValue(cancelling);
    api.cancelOperation.mockResolvedValue(cancelling);

    renderApp();
    const cancel = await screen.findByRole("button", { name: "停止当前操作" });
    fireEvent.click(cancel);

    await waitFor(() => {
      expect(api.cancelOperation).toHaveBeenCalledWith("desktop-operation-a");
    });
    const cancellingStatus = screen.getByRole("status", { name: "当前 Desktop 操作" });
    expect(cancellingStatus).toHaveTextContent("正在停止当前操作…");
    const disabledCancel = screen.getByRole("button", { name: "正在停止当前操作…" });
    expect(disabledCancel).toBeDisabled();
    fireEvent.click(disabledCancel);
    expect(api.cancelOperation).toHaveBeenCalledTimes(1);
  });

  it("does not let an older polling response resurrect a completed operation", async () => {
    vi.useFakeTimers();
    try {
      const running = localSetupOperationState("running", 1);
      const stalePoll = deferred<DesktopState>();
      const terminal = { ...readyState, current_operation: null, activity_sequence: 2 };
      api.getState
        .mockResolvedValueOnce(running)
        .mockReturnValueOnce(stalePoll.promise)
        .mockResolvedValue(terminal);
      api.cancelOperation.mockResolvedValue(terminal);

      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.getByRole("status", { name: "当前 Desktop 操作" })).toBeInTheDocument();

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_000);
      });
      expect(api.getState).toHaveBeenCalledTimes(2);

      fireEvent.click(screen.getByRole("button", { name: "停止当前操作" }));
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();

      stalePoll.resolve(running);
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
      });
      expect(screen.queryByRole("status", { name: "当前 Desktop 操作" })).not.toBeInTheDocument();
      view.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("never promotes a locally ready Regular Tunnel to ChatGPT-connected state", async () => {
    vi.useFakeTimers();
    try {
      const locallyReadyTunnel: DesktopState = {
        ...readyState,
        topology: {
          ...readyState.topology!,
          exposure: { kind: "open_ai_tunnel" },
        },
        readiness: {
          ...readyState.readiness,
          exposure: "local_ready",
          ready_for_chatgpt: false,
          summary: "OpenAI Secure Tunnel is ready; waiting for ChatGPT to connect",
          summary_kind: "tunnel_ready_waiting_for_chat_gpt",
          next_action: "Connect the Tunnel in ChatGPT, then verify it with one real project read.",
          next_action_kind: "check_connection",
        },
        connections: connectionSnapshot(connectionFixture()),
      };
      const failedTunnel: DesktopState = {
        ...locallyReadyTunnel,
        readiness: {
          ...locallyReadyTunnel.readiness,
          exposure: "error",
          ready_for_chatgpt: false,
          summary: "ChatGPT connection is not verified",
          summary_kind: "connection_unverified",
          next_action: "Restart the secure tunnel.",
          next_action_kind: "restart_secure_tunnel",
        },
        connections: connectionSnapshot(connectionFixture({ lifecycle: "error", ready: false, pid: null, health: "degraded", last_error: "process_exited" })),
      };

      api.getState
        .mockResolvedValueOnce(locallyReadyTunnel)
        .mockResolvedValueOnce(failedTunnel);
      api.refresh.mockResolvedValue(locallyReadyTunnel);
      api.observeChatgptActivity.mockResolvedValue(locallyReadyTunnel);
      const view = renderApp();
      await act(async () => {
        await Promise.resolve();
        await Promise.resolve();
        await Promise.resolve();
      });

      const readyStatus = screen.getByRole("status", { name: "工作区" });
      expect(readyStatus).toHaveTextContent("Server运行中Runner运行中连接1 / 1 运行中");
      expect(screen.getByText("尚未观察到 ChatGPT 活动")).toBeInTheDocument();
      expect(screen.queryByText(/等待 ChatGPT|ChatGPT 未连接|外部连接已验证/)).not.toBeInTheDocument();
      expect(api.getState).toHaveBeenCalledTimes(1);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1_500);
      });

      expect(api.getState).toHaveBeenCalledTimes(2);
      expect(api.refresh).toHaveBeenCalledTimes(0);
      expect(screen.getByRole("status", { name: "工作区" })).toHaveTextContent("Server运行中Runner运行中连接0 / 1 运行中");
      expect(screen.queryByText(/ChatGPT 连接尚未验证|ChatGPT 未连接|等待 ChatGPT/)).not.toBeInTheDocument();

      view.unmount();
      const callsAfterUnmount = api.getState.mock.calls.length;
      await act(async () => {
        await vi.advanceTimersByTimeAsync(3_000);
      });
      expect(api.getState).toHaveBeenCalledTimes(callsAfterUnmount);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps regular tunnel controls off a remote Server connection page", async () => {
    const remote: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      readiness: {
        ...readyState.readiness,
        exposure: "unknown",
        ready_for_chatgpt: false,
        summary_kind: "connection_unverified",
        next_action_kind: "check_connection",
      },
    };
    api.getState.mockResolvedValue(remote);
    api.refresh.mockResolvedValue(remote);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });

    fireEvent.click(screen.getByRole("button", { name: "连接" }));
    expect(await screen.findByRole("heading", { level: 1, name: "连接" })).toBeInTheDocument();
    expect(screen.getByText("https://server.example.test")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启动 ChatGPT" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "启动" })).not.toBeInTheDocument();
  });

  it("automatically resumes a saved full runtime on Desktop launch", async () => {
    const stopped = { ...setupState(), runtime_autostart: true };
    api.getState.mockResolvedValue(stopped);
    api.resumeSavedRuntime.mockResolvedValue(readyState);

    renderApp();

    await waitFor(() => expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "配置 WebCodex" })).not.toBeInTheDocument();
  });

  it("leaves all profile autostart to backend reconciliation when resuming", async () => {
    const stopped: DesktopState = { ...setupState(), runtime_autostart: true, preferred_connection: "open_ai_tunnel" };
    const resumed: DesktopState = { ...readyState, runtime_autostart: true, preferred_connection: "open_ai_tunnel", connections: connectionSnapshot(connectionFixture(), connectionFixture({ id: "work", name: "Work", pid: 200 })) };
    api.getState.mockResolvedValue(stopped); api.resumeSavedRuntime.mockResolvedValue(resumed);
    renderApp();
    await waitFor(() => expect(api.resumeSavedRuntime).toHaveBeenCalledTimes(1));
    await screen.findByRole("heading", { name: /^(WebCodex|repo)/, level: 1 });
    expect(api.startRegularTunnel).not.toHaveBeenCalled();
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
  });

  it("reuses an existing remote enrollment when the user selects a different project", async () => {
    const remoteState: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      project: {
        path: "C:\\fixture\\project-a",
        allowed_root: "C:\\fixture\\project-a",
        is_git_repository: true,
        runtime_project_id: "agent:desktop:project-a",
      },
    };
    const projectB = {
      path: "C:\\fixture\\project-b",
      allowed_root: "C:\\fixture\\project-b",
      is_git_repository: true,
      runtime_project_id: null,
    };
    api.getState.mockResolvedValue(remoteState);
    api.inspectProject.mockResolvedValue(projectB);
    api.configureRemote.mockResolvedValue({
      ...remoteState,
      project: { ...projectB, runtime_project_id: "agent:desktop:project-b" },
    });
    vi.mocked(open).mockResolvedValue(projectB.path);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: "project-a" });

    await changeServerConnection();
    fireEvent.click(screen.getByRole("button", { name: /连接现有 Server/ }));
    expect(screen.getByText("将复用现有连接")).toBeInTheDocument();
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "更改文件夹" }));
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(projectB.path));
    expect(screen.getByText("将复用现有连接")).toBeInTheDocument();
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新连接电脑" }));
    await waitFor(() =>
      expect(api.configureRemote).toHaveBeenCalledWith(
        "https://server.example.test",
        "",
        projectB.path,
      ),
    );
  });

  it("exposes pairing recovery when a saved remote enrollment is no longer reusable", async () => {
    const remoteState: DesktopState = {
      ...readyState,
      topology: {
        experience: "full",
        server: { kind: "remote", url: "https://server.example.test" },
        runner: { kind: "local" },
        exposure: { kind: "existing_https", url: "https://server.example.test" },
        enrollment: { kind: "managed_pairing" },
      },
      project: {
        path: "C:\\fixture\\project-a",
        allowed_root: "C:\\fixture\\project-a",
        is_git_repository: true,
        runtime_project_id: "agent:desktop:project-a",
      },
    };
    const projectB = {
      path: "C:\\fixture\\project-b",
      allowed_root: "C:\\fixture\\project-b",
      is_git_repository: true,
      runtime_project_id: null,
    };
    api.getState.mockResolvedValue(remoteState);
    api.inspectProject.mockResolvedValue(projectB);
    api.configureRemote
      .mockRejectedValueOnce({
        code: "pairing_code_invalid",
        message: "The saved Runner identity is not reusable and no new WebCodex pairing code was provided",
        next_action: "Refresh this Runner connection with a new code.",
      })
      .mockResolvedValueOnce({
        ...remoteState,
        project: { ...projectB, runtime_project_id: "agent:desktop:project-b" },
      });
    vi.mocked(open).mockResolvedValue(projectB.path);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: "project-a" });

    await changeServerConnection();
    fireEvent.click(screen.getByRole("button", { name: /连接现有 Server/ }));
    fireEvent.click(screen.getByRole("button", { name: "更改文件夹" }));
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(projectB.path));
    expect(screen.queryByLabelText("一次性登录码")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "重新连接电脑" }));
    expect(await screen.findByLabelText("一次性登录码")).toBeInTheDocument();
    expect(screen.queryByText("将复用现有连接")).not.toBeInTheDocument();
    const submit = screen.getByRole("button", { name: "连接电脑" });
    expect(submit).toBeDisabled();

    fireEvent.change(screen.getByLabelText("一次性登录码"), {
      target: { value: "wc_pair_recovery" },
    });
    expect(submit).toBeEnabled();
    fireEvent.click(submit);
    await waitFor(() =>
      expect(api.configureRemote).toHaveBeenLastCalledWith(
        "https://server.example.test",
        "wc_pair_recovery",
        projectB.path,
      ),
    );
  });

  it("keeps Remote Server as a first-class runtime choice after local setup", async () => {
    api.getState.mockResolvedValue(readyState);
    renderApp();
    await screen.findByRole("heading", { level: 1, name: /^(WebCodex|repo)/ });

    await changeServerConnection();

    expect(await screen.findByRole("button", { name: /连接现有 Server/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /在此电脑使用 WebCodex/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /快速共享项目/ })).toBeInTheDocument();
  });
});
