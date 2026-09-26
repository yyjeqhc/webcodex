import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";
import { FirstRun } from "./FirstRun";

const api = vi.hoisted(() => ({ configureEnvironment: vi.fn(), inspectProject: vi.fn() }));
const picker = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => picker);

const state = { topology: null, project: null, current_operation: null } as DesktopState;
const project = { path: "/work/repo", allowed_root: "/work/repo", is_git_repository: true, runtime_project_id: null };
function mount(setupState: DesktopState = state) {
  const onState = vi.fn();
  const view = render(<LocaleProvider><FirstRun state={setupState} onState={onState} chooseModeFirst /></LocaleProvider>);
  return { ...view, onState };
}
function clickAction(container: HTMLElement, action: string) {
  const button = container.querySelector<HTMLButtonElement>('[data-webcodex-action="' + action + '"]');
  if (!button) throw new Error("Missing action " + action);
  fireEvent.click(button);
}

beforeEach(() => {
  vi.resetAllMocks();
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.configureEnvironment.mockResolvedValue(state);
  api.inspectProject.mockResolvedValue(project);
  picker.open.mockResolvedValue(project.path);
});

describe("shared environment first run", () => {
  it("offers only Create and Join, with Server-only when the folder is skipped", async () => {
    const { container, onState } = mount();
    expect(container.querySelectorAll(".entry-card")).toHaveLength(2);
    expect(container.querySelector('[data-webcodex-action="choose-quick-share-setup"]')).toBeNull();
    clickAction(container, "choose-local-setup");
    clickAction(container, "configure-local");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "create", serverUrl: null, projectPath: null, pairingCode: null,
      userToken: null, replacePairingCode: false,
    }));
    expect(onState).toHaveBeenCalledWith(state);
  });

  it("joins as a viewer with user authentication and no pairing", async () => {
    const { container } = mount();
    clickAction(container, "choose-remote-setup");
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://server.example" } });
    fireEvent.change(screen.getByLabelText("User API credential"), { target: { value: "wc_user_secret" } });
    expect(screen.queryByLabelText("One-time login code")).toBeNull();
    clickAction(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: null,
      pairingCode: null, userToken: "wc_user_secret", replacePairingCode: false,
    }));
  });

  it("joins with a local project using one one-time code", async () => {
    const { container } = mount();
    clickAction(container, "choose-remote-setup");
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://server.example" } });
    clickAction(container, "choose-project");
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(project.path));
    const code = screen.getByLabelText("One-time login code");
    fireEvent.change(code, { target: { value: "wc_pair_once" } });
    clickAction(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: project.path,
      pairingCode: "wc_pair_once", userToken: null, replacePairingCode: false,
    }));
    expect(code).toHaveValue("");
  });

  it("reuses a confirmed legacy Runner identity without another one-time code", async () => {
    const legacy = {
      ...state,
      topology: { experience: "full", server: { kind: "remote", url: "https://server.example" }, runner: { kind: "local" } },
      project,
      persistent_environment: null,
    } as DesktopState;
    const view = render(<LocaleProvider><FirstRun state={legacy} onState={vi.fn()} /></LocaleProvider>);
    expect(screen.queryByLabelText("One-time login code")).toBeNull();
    clickAction(view.container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: project.path,
      pairingCode: null, userToken: null, replacePairingCode: false,
    }));
  });

  it("reuses the saved user credential when reopening setup for the same persistent viewer Server", async () => {
    const viewer = {
      ...state,
      topology: { experience: "full", server: { kind: "remote", url: "https://server.example" }, runner: { kind: "none" } },
      persistent_environment: "environment-1",
    } as DesktopState;
    const { container } = mount(viewer);
    clickAction(container, "choose-remote-setup");
    expect(screen.queryByLabelText("User API credential")).toBeNull();
    expect(screen.getByText(/saved credential for this Server/i)).toBeInTheDocument();
    clickAction(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: null,
      pairingCode: null, userToken: null, replacePairingCode: false,
    }));
  });

  it("reuses the saved Runner registration for another project on the same persistent Server", async () => {
    const runner = {
      ...state,
      topology: { experience: "full", server: { kind: "remote", url: "https://server.example" }, runner: { kind: "local" } },
      persistent_environment: "environment-1",
    } as DesktopState;
    const { container } = mount(runner);
    clickAction(container, "choose-remote-setup");
    clickAction(container, "choose-project");
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(project.path));
    expect(screen.queryByLabelText("One-time login code")).toBeNull();
    expect(screen.getByText(/already registered as a Runner/i)).toBeInTheDocument();
    clickAction(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: project.path,
      pairingCode: null, userToken: null, replacePairingCode: false,
    }));
  });

  it("requires a pairing code when a persistent viewer adds its first project", async () => {
    const viewer = {
      ...state,
      topology: { experience: "full", server: { kind: "remote", url: "https://server.example" }, runner: { kind: "none" } },
      persistent_environment: "environment-1",
    } as DesktopState;
    const { container } = mount(viewer);
    clickAction(container, "choose-remote-setup");
    clickAction(container, "choose-project");
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(project.path));
    expect(screen.queryByLabelText("User API credential")).toBeNull();
    expect(screen.getByLabelText("One-time login code")).toBeInTheDocument();
    expect(container.querySelector('[data-webcodex-action="configure-remote"]')).toBeDisabled();
  });

  it("shows a translated Core setup step while busy without showing operation identifiers", () => {
    const inProgress = {
      ...state,
      current_operation: { id: "wc_private_operation_id", kind: "remote_setup", phase: "running", started_at_ms: 1, cancellable: true },
      setup_progress: { operation_id: "wc_private_operation_id", step: "runner_service_start", state: "started" },
    } as DesktopState;
    const { container } = mount(inProgress);
    clickAction(container, "choose-local-setup");
    expect(screen.getByRole("status")).toHaveTextContent("Current setup step: Starting the Runner service");
    expect(container).not.toHaveTextContent("wc_private_operation_id");
  });
});
