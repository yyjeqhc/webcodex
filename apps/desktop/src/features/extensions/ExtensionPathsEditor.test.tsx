import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { LocaleProvider } from "../../i18n/locale";
import type { RunnerSettings } from "../../models/topology";
import { ExtensionPathsEditor } from "./ExtensionPathsEditor";

it("preserves category drafts but saves only the selected category against observed paths", async () => {
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  const settings: RunnerSettings = {
    target: { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:1" },
    paths: { instruction_files: ["/fixture/rules.md"], skill_roots: ["/fixture/skills"] },
    file_access: { configured_roots: [], effective_roots: [], using_default_roots: true, allow_cwd_anywhere: false },
    plugin_ids: [], can_restart: false,
  };
  const save = vi.fn().mockResolvedValue(true);
  const view = (kind: "instructions" | "skills", snapshot = settings) => <DesktopMantineProvider><LocaleProvider><ExtensionPathsEditor settings={snapshot} kind={kind} disabled={false} onSave={save} /></LocaleProvider></DesktopMantineProvider>;
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
