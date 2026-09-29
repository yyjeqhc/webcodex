import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { RunnerSettings } from "../../models/topology";
import { ManagedInstructionsPanel } from "./ManagedInstructionsPanel";
const api = vi.hoisted(() => ({ managedInstructionsRead:vi.fn(), managedInstructionsSave:vi.fn(), managedInstructionsEnable:vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi:api }));
const path = "/fixture/desktop/instructions/AGENTS.md";
const target = { config_path:"/fixture/runner.toml", client_id:"fixture", server_url:"http://127.0.0.1:1" };
const settings: RunnerSettings = { target, paths:{ instruction_files:["/custom/company.md"], skill_roots:["/custom/skills"] }, file_access:{configured_roots:[],effective_roots:[],using_default_roots:true,allow_cwd_anywhere:false}, plugin_ids:[], can_restart:false };
const onState = vi.fn(), onEnabled = vi.fn();
const props = { active:true, settings, disabled:false, onState, onEnabled };
const wrap = (ui: React.ReactNode) => <DesktopMantineProvider><LocaleProvider>{ui}</LocaleProvider></DesktopMantineProvider>;
beforeEach(() => {
  vi.resetAllMocks(); localStorage.clear(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.managedInstructionsRead.mockResolvedValue({path,exists:false,content:"",revision:"missing"});
  api.managedInstructionsSave.mockImplementation(async (_revision,content) => ({path,exists:true,content,revision:"saved"}));
  api.managedInstructionsEnable.mockResolvedValue({});
});

it("ignores StrictMode's abandoned first read and still loads the editor", async () => {
  let first: (value: unknown) => void = () => {};
  api.managedInstructionsRead.mockImplementationOnce(() => new Promise(resolve => { first = resolve; }));
  render(<StrictMode>{wrap(<ManagedInstructionsPanel {...props} />)}</StrictMode>);
  expect(await screen.findByLabelText("Global instructions")).toHaveValue("");
  fireEvent.change(screen.getByLabelText("Global instructions"), {target:{value:"new draft"}});
  await act(async () => first({path,exists:true,content:"abandoned response",revision:"old"}));
  expect(screen.getByLabelText("Global instructions")).toHaveValue("new draft");
  expect(api.managedInstructionsEnable).not.toHaveBeenCalled();
});

it("loads only when opened, never enables by discovery and retains drafts across catalog/tab refresh", async () => {
  const view = render(wrap(<ManagedInstructionsPanel {...props} active={false} />));
  expect(api.managedInstructionsRead).not.toHaveBeenCalled();
  view.rerender(wrap(<ManagedInstructionsPanel {...props} />));
  const input = await screen.findByLabelText("Global instructions");
  fireEvent.change(input, {target:{value:"my unsaved draft"}});
  view.rerender(wrap(<ManagedInstructionsPanel {...props} active={false} settings={null} />));
  view.rerender(wrap(<ManagedInstructionsPanel {...props} settings={structuredClone(settings)} />));
  expect(screen.getByLabelText("Global instructions")).toHaveValue("my unsaved draft");
  expect(api.managedInstructionsRead).toHaveBeenCalledTimes(1);
  expect(api.managedInstructionsEnable).not.toHaveBeenCalled();
});

it("saves the full file using its observed revision with no config mutation", async () => {
  const content = "full non-truncated body\n".repeat(4096);
  api.managedInstructionsRead.mockResolvedValue({path,exists:true,content,revision:"original"});
  render(wrap(<ManagedInstructionsPanel {...props} />));
  const input = await screen.findByLabelText("Global instructions");
  expect(input).toHaveValue(content);
  fireEvent.change(input, {target:{value:content+"tail change"}});
  fireEvent.click(screen.getByRole("button", {name:"Save instructions"}));
  await waitFor(() => expect(api.managedInstructionsSave).toHaveBeenCalledWith("original", content+"tail change"));
  expect(api.managedInstructionsEnable).not.toHaveBeenCalled();
  expect(onState).not.toHaveBeenCalled();
});

it("enables explicitly using exact settings and never substitutes custom paths", async () => {
  render(wrap(<ManagedInstructionsPanel {...props} />));
  fireEvent.click(await screen.findByRole("button", {name:"Enable global instructions"}));
  await waitFor(() => expect(api.managedInstructionsEnable).toHaveBeenCalledWith(target, settings.paths, "missing"));
  expect(onEnabled).toHaveBeenCalledTimes(1);
  expect(api.managedInstructionsSave).not.toHaveBeenCalled();
});

it("preserves a conflicting draft and requires explicit discard before reload", async () => {
  api.managedInstructionsSave.mockRejectedValue({code:"managed_instructions_conflict",message:"File changed outside",next_action:"Keep draft and reload"});
  render(wrap(<ManagedInstructionsPanel {...props} />));
  fireEvent.change(await screen.findByLabelText("Global instructions"), {target:{value:"local draft"}});
  fireEvent.click(screen.getByRole("button", {name:"Save instructions"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("File changed outside");
  expect(screen.getByLabelText("Global instructions")).toHaveValue("local draft");
  fireEvent.click(screen.getByRole("button", {name:"Reload from file"}));
  expect(api.managedInstructionsRead).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByRole("button", {name:"Keep editing"}));
  expect(screen.getByLabelText("Global instructions")).toHaveValue("local draft");
  fireEvent.click(screen.getByRole("button", {name:"Reload from file"}));
  api.managedInstructionsRead.mockResolvedValue({path,exists:true,content:"external",revision:"external"});
  fireEvent.click(screen.getByRole("button", {name:"Discard draft and reload"}));
  await waitFor(() => expect(screen.getByLabelText("Global instructions")).toHaveValue("external"));
});

it("permits offline drafts but blocks oversized UTF-8 data and enabling without a Runner", async () => {
  render(wrap(<ManagedInstructionsPanel {...props} settings={null} />));
  fireEvent.change(await screen.findByLabelText("Global instructions"), {target:{value:"a local rule"}});
  expect(screen.getByRole("button", {name:"Save instructions"})).toBeEnabled();
  expect(screen.getByRole("button", {name:"Enable global instructions"})).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Global instructions"), {target:{value:"规".repeat(350_000)}});
  expect(screen.getByRole("button", {name:"Save instructions"})).toBeDisabled();
  expect(screen.getByRole("alert")).toHaveTextContent("exceeds 1 MiB");
});

it("does not replay uncertain enabling and keeps the created file usable", async () => {
  api.managedInstructionsEnable.mockRejectedValue({code:"runner_config_reconcile_required",message:"Reload uncertain",next_action:"Inspect current settings"});
  api.managedInstructionsRead.mockResolvedValueOnce({path,exists:false,content:"",revision:"missing"}).mockResolvedValue({path,exists:true,content:"",revision:"empty"});
  render(wrap(<ManagedInstructionsPanel {...props} />));
  fireEvent.click(await screen.findByRole("button", {name:"Enable global instructions"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("Reload uncertain");
  await waitFor(() => expect(api.managedInstructionsRead).toHaveBeenCalledTimes(2));
  expect(api.managedInstructionsEnable).toHaveBeenCalledTimes(1);
  expect(api.managedInstructionsSave).not.toHaveBeenCalled();
});
