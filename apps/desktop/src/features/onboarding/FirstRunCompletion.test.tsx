import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider, LANGUAGES } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import type { DesktopState } from "../../models/topology";
import { FirstRunCompletion, FirstReadGuide, firstReadTarget } from "./FirstRunCompletion";
const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));
const api = vi.hoisted(() => ({ saveTunnelProfile: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => clipboard);
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
const state = {
  persistent_environment: "env", topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" } },
  workspace_runner: { client_id: "real-runner", config_path: "/runner.toml", server_url: "http://localhost:8787" },
  project: { path: "/work/folder-name", runtime_project_id: "agent:real-runner:actual-project" },
  readiness: { server: "ready", runner: "ready", runtime_ready: true },
  connections: { profiles: [], config_error: false, running: 0 }, chatgpt_activity: { observed: false },
} as unknown as DesktopState;
function wrap(content: React.ReactNode) { return <DesktopMantineProvider><LocaleProvider>{content}</LocaleProvider></DesktopMantineProvider>; }
function mount() {
  const onComplete = vi.fn(), onProjects = vi.fn(), onConnection = vi.fn();
  return { ...render(wrap(<FirstRunCompletion {...{onComplete,onProjects,onConnection}} />)), onComplete,onProjects,onConnection };
}
beforeEach(() => { vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US"); clipboard.writeText.mockResolvedValue(undefined); });
describe("first-run completion", () => {
  it("offers connections, projects and overview independently after setup", () => {
    const {onConnection, onProjects, onComplete} = mount();
    expect(screen.getByRole("heading", { level: 1 })).toHaveFocus();
    expect(screen.queryByText(/ChatGPT/)).toBeNull();
    fireEvent.click(screen.getByRole("button", {name: "Connect an AI app"}));
    expect(onConnection).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", {name: "Projects"}));
    expect(onProjects).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", {name: "Open overview"}));
    expect(onComplete).toHaveBeenCalledOnce();
    expect(api.saveTunnelProfile).not.toHaveBeenCalled();
  });
  it("copies a read-only request with exact IDs and keeps user reports separate from observation", async () => {
    const {rerender}=render(wrap(<FirstReadGuide state={state} />));
    fireEvent.click(screen.getByRole("button",{name:"Copy read request"}));
    await waitFor(()=>expect(clipboard.writeText).toHaveBeenCalledWith(expect.stringContaining("Project agent:real-runner:actual-project")));
    expect(clipboard.writeText.mock.calls[0][0]).toContain("Do not edit files or run commands");
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("status")).toHaveTextContent("your confirmation, not an automatically observed connection test");
    expect(state.chatgpt_activity?.observed).toBe(false);
    rerender(wrap(<FirstReadGuide state={{...state,persistent_environment:"other"}} />));
    expect(screen.getByRole("checkbox")).not.toBeChecked();
  });
  it("does not invent a target from a folder name, wrong Runner, or viewer", () => {
    expect(firstReadTarget({...state,project:{...state.project!,runtime_project_id:null}})).toBeNull();
    expect(firstReadTarget({...state,project:{...state.project!,runtime_project_id:"agent:other:project"}})).toBeNull();
    expect(firstReadTarget({...state,topology:{...state.topology!,runner:{kind:"none"}}})).toBeNull();
  });
  it.each(LANGUAGES)("renders translated completion in $value", ({value}) => {
    localStorage.setItem("webcodex.desktop.locale",value);
    const {container}=mount();
    expect(container).not.toHaveTextContent("completion.");
    expect(screen.getByRole("heading",{level:1})).toHaveTextContent(/\S/);
  });
});
