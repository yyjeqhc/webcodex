import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import type { DesktopState } from "../../models/topology";
import { FirstRun } from "./FirstRun";

const api = vi.hoisted(() => ({ configureEnvironment: vi.fn(), inspectProject: vi.fn(), startQuickShare: vi.fn() }));
const picker = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => picker);
const state = { topology: null, project: null, current_operation: null } as DesktopState;
const project = { path: "/work/repo", allowed_root: "/work/repo", is_git_repository: true, runtime_project_id: null };
function mount(setupState = state, chooseModeFirst = true) {
  const onState = vi.fn();
  return { ...render(<DesktopMantineProvider><LocaleProvider><FirstRun state={setupState} onState={onState} chooseModeFirst={chooseModeFirst} /></LocaleProvider></DesktopMantineProvider>), onState };
}
function action(container: HTMLElement, name: string) {
  const button = container.querySelector<HTMLButtonElement>(`[data-webcodex-action="${name}"]`);
  if (!button) throw new Error("Missing action " + name);
  fireEvent.click(button);
}
function remote(runner: boolean, persistent = true): DesktopState {
  return { ...state, topology: { experience: "full", server: { kind: "remote", url: "https://server.example/" },
    runner: { kind: runner ? "local" : "none" } }, persistent_environment: persistent ? "environment-1" : null } as DesktopState;
}
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.configureEnvironment.mockResolvedValue(state);
  api.startQuickShare.mockResolvedValue(state);
  api.inspectProject.mockResolvedValue(project);
  picker.open.mockResolvedValue(project.path);
});

describe("explicit environment setup", () => {
  it("chooses machine startup explicitly and never retries a user failure as a system install", async () => {
    api.configureEnvironment.mockRejectedValue({ code:"missing_prerequisite", message:"Sign in first", next_action:"Inspect user manager" });
    const {container}=mount(); action(container,"choose-local-setup");
    action(container,"configure-local");
    await screen.findByRole("alert");
    expect(api.configureEnvironment).toHaveBeenCalledTimes(1);
    expect(api.configureEnvironment.mock.calls[0][0].serviceScope).toBe("user");
    fireEvent.change(screen.getByLabelText("Background startup"),{target:{value:"system"}});
    action(container,"configure-local");
    await waitFor(()=>expect(api.configureEnvironment).toHaveBeenCalledTimes(2));
    expect(api.configureEnvironment.mock.calls[1][0].serviceScope).toBe("system");
  });

  it("does not reinterpret an already saved environment's native manager", async () => {
    const {container}=mount(remote(true),false);
    expect(screen.queryByLabelText("Background startup")).toBeNull();
    action(container,"configure-remote");
    await waitFor(()=>expect(api.configureEnvironment).toHaveBeenCalledTimes(1));
    expect(api.configureEnvironment.mock.calls[0][0]).not.toHaveProperty("serviceScope");
  });
  it("offers persistent local/join and temporary sharing without registering a default project", async () => {
    const { container, onState } = mount();
    expect(container.querySelectorAll(".entry-card")).toHaveLength(3);
    expect(api.configureEnvironment).not.toHaveBeenCalled();
    action(container, "choose-local-setup");
    expect(screen.getByLabelText("Allow AI to work on this computer")).toBeChecked();
    expect(container.querySelector('[data-webcodex-action="choose-project"]')).toBeNull();
    action(container, "configure-local");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "create", serverUrl: null, projectPath: null, runner: true, serviceScope: "user",
      pairingCode: null, userToken: null, replacePairingCode: false,
    }));
    expect(onState).toHaveBeenCalledWith(state);
    expect(picker.open).not.toHaveBeenCalled();
  });

  it("requires an explicit role choice for a Server-only machine", async () => {
    const { container } = mount(); action(container, "choose-local-setup");
    fireEvent.click(screen.getByLabelText("Allow AI to work on this computer"));
    expect(screen.getByText("This computer will run Server only.")).toBeInTheDocument();
    action(container, "configure-local");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith(expect.objectContaining({ runner: false, projectPath: null })));
  });

  it("joins as a viewer using a user credential rather than a Runner pairing code", async () => {
    const { container } = mount(); action(container, "choose-remote-setup");
    fireEvent.click(screen.getByLabelText("Allow AI to work on this computer"));
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://server.example/" } });
    fireEvent.change(screen.getByLabelText("User API credential"), { target: { value: "wc_user_secret" } });
    expect(screen.queryByLabelText("One-time login code")).toBeNull();
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith({
      mode: "join", serverUrl: "https://server.example", projectPath: null, runner: false, serviceScope: "user",
      pairingCode: null, userToken: "wc_user_secret", replacePairingCode: false,
    }));
    expect(screen.getByLabelText("User API credential")).toHaveValue("");
  });

  it("joins a projectless Runner and clears submitted one-time secrets on failure", async () => {
    api.configureEnvironment.mockRejectedValue({ code: "pairing_code_invalid", message: "Invalid code", next_action: "Use a new code." });
    const { container } = mount(); action(container, "choose-remote-setup");
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://server.example" } });
    const code = screen.getByLabelText("One-time login code");
    fireEvent.change(code, { target: { value: "wc_pair_once" } });
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith(expect.objectContaining({ runner: true, projectPath: null, pairingCode: "wc_pair_once" })));
    expect(await screen.findByRole("alert")).toHaveTextContent("pairing_code_invalid");
    expect(code).toHaveValue(""); expect(picker.open).not.toHaveBeenCalled();
  });

  it.each([true, false])("reuses a saved projectless Runner with canonical Server matching (persistent=%s)", async persistent => {
    const { container } = mount(remote(true, persistent), false);
    expect(screen.queryByLabelText("One-time login code")).toBeNull();
    expect(screen.getByLabelText("Allow AI to work on this computer")).toBeDisabled();
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith(expect.objectContaining({
      runner: true, serverUrl: "https://server.example", projectPath: null, pairingCode: null,
    })));
  });

  it("preserves a legacy migration's recorded default without asking for a new project", async () => {
    const { container } = mount({ ...remote(true, false), project }, false);
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith(expect.objectContaining({ projectPath: project.path, runner: true })));
    expect(api.inspectProject).not.toHaveBeenCalled();
  });

  it("reuses a viewer credential but requires pairing when local work is explicitly enabled", async () => {
    const { container } = mount(remote(false), false);
    expect(screen.queryByLabelText("User API credential")).toBeNull();
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenCalledWith(expect.objectContaining({ runner: false, userToken: null })));
    await waitFor(() => expect(screen.getByLabelText("Allow AI to work on this computer")).toBeEnabled());
    fireEvent.click(screen.getByLabelText("Allow AI to work on this computer"));
    expect(screen.getByLabelText("One-time login code")).toBeInTheDocument();
    expect(container.querySelector('[data-webcodex-action="configure-remote"]')).toBeDisabled();
  });

  it("shows a replacement-code field even when a saved Runner registration normally hides it", async () => {
    api.configureEnvironment.mockRejectedValueOnce({ code: "pairing_recovery_required", message: "Recovery required", next_action: "Use a new code." });
    const { container } = mount(remote(true), false); action(container, "configure-remote");
    fireEvent.click(await screen.findByRole("button", { name: "Use a new one-time pairing code" }));
    fireEvent.change(screen.getByLabelText("One-time login code"), { target: { value: "wc_pair_replacement" } });
    action(container, "configure-remote");
    await waitFor(() => expect(api.configureEnvironment).toHaveBeenLastCalledWith(expect.objectContaining({
      pairingCode: "wc_pair_replacement", replacePairingCode: true,
    })));
  });

  it("clears secrets when the target changes and does not submit invalid URL forms", () => {
    const { container } = mount(); action(container, "choose-remote-setup");
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://server.example" } });
    const code = screen.getByLabelText("One-time login code");
    fireEvent.change(code, { target: { value: "wc_pair_once" } });
    fireEvent.change(screen.getByLabelText("Server URL"), { target: { value: "https://other.example/path" } });
    expect(code).toHaveValue("");
    fireEvent.submit(container.querySelector("form")!);
    expect(api.configureEnvironment).not.toHaveBeenCalled();
  });

  it("shows translated setup progress and blocks mutations while busy", () => {
    const progress = { ...remote(true), current_operation: { id: "private-operation", kind: "remote_setup", phase: "running", started_at_ms: 1, cancellable: true },
      setup_progress: { operation_id: "private-operation", step: "runner_service_start", state: "started" } } as DesktopState;
    const { container } = mount(progress, false);
    expect(screen.getByRole("status")).toHaveTextContent("Current setup step: Starting the Runner service");
    expect(container).not.toHaveTextContent("private-operation");
    fireEvent.submit(container.querySelector("form")!); expect(api.configureEnvironment).not.toHaveBeenCalled();
  });

  it("keeps Quick Share temporary and independent of persistent environment setup", async () => {
    const { container } = mount(); action(container, "choose-quick-share-setup");
    expect(container.querySelector('[data-webcodex-action="start-quick-share"]')).toBeDisabled();
    action(container, "choose-project");
    await waitFor(() => expect(api.inspectProject).toHaveBeenCalledWith(project.path));
    await waitFor(() => expect(container.querySelector('[data-webcodex-action="start-quick-share"]')).toBeEnabled());
    action(container, "start-quick-share");
    await waitFor(() => expect(api.startQuickShare).toHaveBeenCalledWith(project.path, "cloudflare"));
    expect(api.configureEnvironment).not.toHaveBeenCalled();
  });
});
