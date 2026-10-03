import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";
import { AddLocalProject } from "./AddLocalProject";

const api = vi.hoisted(() => ({ inspectProjectAccess: vi.fn(), activateLocalProject: vi.fn() }));
const picker = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => picker);
const state = { topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" } },
  readiness: { runtime_ready: true }, current_operation: null, persistent_environment: "saved-environment",
  workspace_runner: { client_id: "local", server_url: "http://localhost:3000" } } as DesktopState;
const folder = { path: "C:\\work\\project", allowed_root: "C:\\work\\project", is_git_repository: true, runtime_project_id: null };
function view(selected = state, onState = vi.fn(), onAdded = vi.fn()) {
  return <DesktopMantineProvider><LocaleProvider><AddLocalProject state={selected} onState={onState} onAdded={onAdded} /></LocaleProvider></DesktopMantineProvider>;
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  picker.open.mockResolvedValue(folder.path); api.inspectProjectAccess.mockResolvedValue({ project: folder, authorization_required: true });
  api.activateLocalProject.mockResolvedValue({ ...state, project: folder });
});

describe("manual local project registration", () => {
  it.each(["local", "remote"])("uses the existing %s Server connection and confirms folder access", async server => {
    const selected = server === "local" ? state : { ...state, topology: { ...state.topology!, server: { kind: "remote", url: "https://server.example" } } } as DesktopState;
    const onState = vi.fn(), onAdded = vi.fn(); render(view(selected, onState, onAdded));
    expect(screen.getByText(/No new Tunnel ID or API key/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add local folder" }));
    await screen.findByRole("dialog");
    expect(api.activateLocalProject).not.toHaveBeenCalled();
    expect(screen.getByText(folder.path)).toBeInTheDocument();
    expect(screen.queryByLabelText("API Key")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Authorize and add project" }));
    await waitFor(() => expect(onAdded).toHaveBeenCalledTimes(1));
    expect(api.activateLocalProject).toHaveBeenCalledExactlyOnceWith(folder.path);
    expect(onState).toHaveBeenCalledWith(expect.objectContaining({ project: folder }));
  });
  it("registers an already authorized folder without asking to expand file access", async () => {
    api.inspectProjectAccess.mockResolvedValue({ project: folder, authorization_required: false });
    render(view());
    fireEvent.click(screen.getByRole("button", { name: "Add local folder" }));
    await screen.findByRole("dialog");
    expect(screen.getByText(/already within the Runner’s authorized file access/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add project" }));
    await waitFor(() => expect(api.activateLocalProject).toHaveBeenCalledExactlyOnceWith(folder.path));
  });
  it("does not mutate when the directory picker is cancelled", async () => {
    picker.open.mockResolvedValue(null); render(view());
    fireEvent.click(screen.getByRole("button", { name: "Add local folder" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Add local folder" })).toBeEnabled());
    expect(api.inspectProjectAccess).not.toHaveBeenCalled(); expect(api.activateLocalProject).not.toHaveBeenCalled();
  });
  it("hides registration for viewers and temporary sharing, and blocks a stopped Runtime", () => {
    const mounted = render(view({ ...state, topology: { ...state.topology!, runner: { kind: "none" } } } as DesktopState));
    expect(screen.queryByRole("button", { name: "Add local folder" })).toBeNull();
    mounted.rerender(view({ ...state, topology: { ...state.topology!, experience: "quick_share" } } as DesktopState));
    expect(screen.queryByRole("button", { name: "Add local folder" })).toBeNull();
    mounted.rerender(view({ ...state, readiness: { ...state.readiness, runtime_ready: false } }));
    expect(screen.getByRole("button", { name: "Add local folder" })).toBeDisabled();
  });
  it("discards a folder selection if the environment changes while the picker is open", async () => {
    let resolve!: (path: string) => void;
    picker.open.mockImplementation(() => new Promise(done => { resolve = done; }));
    const mounted = render(view());
    fireEvent.click(screen.getByRole("button", { name: "Add local folder" }));
    mounted.rerender(view({ ...state, persistent_environment: "different-environment" }));
    await act(async () => resolve(folder.path));
    expect(api.inspectProjectAccess).not.toHaveBeenCalled(); expect(screen.queryByRole("dialog")).toBeNull();
  });
  it("prevents duplicate registration and never exposes arbitrary error or credential text", async () => {
    let reject!: (reason: unknown) => void;
    api.activateLocalProject.mockImplementation(() => new Promise((_done, fail) => { reject = fail; }));
    render(view()); fireEvent.click(screen.getByRole("button", { name: "Add local folder" }));
    await screen.findByRole("dialog");
    const add = screen.getByRole("button", { name: "Authorize and add project" });
    fireEvent.click(add); fireEvent.click(add);
    expect(api.activateLocalProject).toHaveBeenCalledTimes(1);
    await act(async () => reject(new Error("wc_user_do_not_render")));
    expect(screen.getByRole("alert")).toHaveTextContent("Do not create another connection.");
    expect(screen.queryByText(/wc_user_do_not_render/)).toBeNull();
  });
});
