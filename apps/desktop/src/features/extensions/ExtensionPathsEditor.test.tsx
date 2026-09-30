import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { RunnerSettings } from "../../models/topology";
import { ExtensionPathsEditor } from "./ExtensionPathsEditor";

const fixture = (): RunnerSettings => ({
  target: { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:1" },
  paths: { instruction_files: ["/fixture/rules.md"], skill_roots: ["/fixture/skills"] },
  file_access: { configured_roots: [], effective_roots: [], using_default_roots: true, allow_cwd_anywhere: false },
  plugin_ids: [], can_restart: false,
});

it("preserves category drafts but saves only the selected category against observed paths", async () => {
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  const settings: RunnerSettings = {
    target: { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:1" },
    paths: { instruction_files: ["/fixture/rules.md"], skill_roots: ["/fixture/skills"] },
    file_access: { configured_roots: [], effective_roots: [], using_default_roots: true, allow_cwd_anywhere: false },
    plugin_ids: [], can_restart: false,
  };
  const save = vi.fn().mockResolvedValue(true);
  const view = (kind: "instructions" | "skills", snapshot = settings) => <DesktopMantineProvider><LocaleProvider><ExtensionPathsEditor settings={snapshot} kind={kind} disabled={false} onSave={save} onBrowse={vi.fn()} /></LocaleProvider></DesktopMantineProvider>;
  const rendered = render(view("instructions"));
  fireEvent.change(screen.getByLabelText("Global instruction files"), { target: { value: "/draft/rules.md" } });
  rendered.rerender(view("skills"));
  fireEvent.change(screen.getByLabelText("Configured Skill roots"), { target: { value: "/draft/skills" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith({ instruction_files: ["/fixture/rules.md"], skill_roots: ["/draft/skills"] }));
  const updated = { ...settings, paths: { ...settings.paths, skill_roots: ["/draft/skills"] } };
  rendered.rerender(view("instructions", updated));
  expect(screen.getByLabelText("Global instruction files")).toHaveValue("/draft/rules.md");
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(save).toHaveBeenLastCalledWith({ instruction_files: ["/draft/rules.md"], skill_roots: ["/draft/skills"] }));
});

it("stages picked folders, allows cancellation, and applies removal only on Save", async () => {
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  const settings = fixture(); const save = vi.fn().mockResolvedValue(true);
  const browse = vi.fn().mockResolvedValue("/picked/skills");
  render(<DesktopMantineProvider><LocaleProvider><ExtensionPathsEditor settings={settings} kind="skills" disabled={false} onSave={save} onBrowse={browse} /></LocaleProvider></DesktopMantineProvider>);
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Add Skill Folder" }));
  await waitFor(() => expect(screen.getByLabelText("Configured Skill roots 2")).toHaveValue("/picked/skills"));
  expect(browse).toHaveBeenCalledExactlyOnceWith("skills");
  expect(save).not.toHaveBeenCalled();
  expect(screen.getByText("Unsaved changes")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByLabelText("Configured Skill roots 2")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Remove Configured Skill roots 1" }));
  expect(save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith({ instruction_files: settings.paths.instruction_files, skill_roots: [] }));
});

it("discards a file picker result if the Runner target changes while it is open", async () => {
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  const settings = fixture(); const save = vi.fn();
  let resolve!: (path: string) => void;
  const browse = vi.fn(() => new Promise<string>(done => { resolve = done; }));
  const view = (snapshot: RunnerSettings) => <DesktopMantineProvider><LocaleProvider><ExtensionPathsEditor settings={snapshot} kind="skills" disabled={false} onSave={save} onBrowse={browse} /></LocaleProvider></DesktopMantineProvider>;
  const rendered = render(view(settings));
  fireEvent.click(screen.getByRole("button", { name: "Add Skill Folder" }));
  const replacement = { ...settings, target: { ...settings.target, client_id: "another-runner" }, paths: { ...settings.paths, skill_roots: ["/other/skills"] } };
  rendered.rerender(view(replacement));
  await act(async () => resolve("/picked/for-old-runner"));
  expect(screen.getByLabelText("Configured Skill roots")).toHaveValue("/other/skills");
  expect(screen.queryByLabelText("Configured Skill roots 2")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  expect(save).not.toHaveBeenCalled();
});
