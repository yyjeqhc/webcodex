import { MantineProvider } from "@mantine/core";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { Sidebar } from "./Sidebar";
import { LANGUAGES, LOCALE_STORAGE_KEY, LocaleProvider, messages } from "../i18n/locale";
import { APPEARANCE_STORAGE_KEY } from "../hooks/useAppearance";
import { ACCENT_STORAGE_KEY } from "../../../../frontend/src/ui/accent";
import type { DesktopState } from "../models/topology";

const state = {
  readiness: { server: "stopped", runner: "stopped", exposure: "disabled", runtime_ready: false },
} as DesktopState;

beforeEach(() => {
  localStorage.setItem(LOCALE_STORAGE_KEY, "en-US");
});

function show() {
  const setNavigation = vi.fn();
  const view = render(<MantineProvider env="test"><LocaleProvider>
    <Sidebar state={state} navigation="home" setNavigation={setNavigation} />
  </LocaleProvider></MantineProvider>);
  return { ...view, setNavigation };
}

it("keeps preference order and persists accent and appearance through the shared controls", async () => {
  const view = show();
  const controls = within(screen.getByRole("group", { name: messages["en-US"]["settings.interface"] })).getAllByRole("button");
  expect(controls.map(control => control.getAttribute("aria-label"))).toEqual(["Accent color", "Appearance", "Language"]);
  fireEvent.click(controls[0]);
  fireEvent.click(await screen.findByRole("button", { name: "Violet" }));
  expect(localStorage.getItem(ACCENT_STORAGE_KEY)).toBe("#7c3aed");
  fireEvent.click(controls[1]);
  expect(localStorage.getItem(APPEARANCE_STORAGE_KEY)).toBe("light");
  fireEvent.click(controls[1]);
  expect(localStorage.getItem(APPEARANCE_STORAGE_KEY)).toBe("dark");
  view.unmount();
  show();
  expect(screen.getByRole("button", { name: "Appearance" })).toHaveAttribute("aria-description", "Appearance · Dark");
  fireEvent.click(screen.getByRole("button", { name: "Accent color" }));
  expect(await screen.findByRole("button", { name: "Violet" })).toHaveAttribute("aria-pressed", "true");
});

it.each(LANGUAGES)("translates preference labels and custom color in $value", async ({ value }) => {
  localStorage.setItem(LOCALE_STORAGE_KEY, value);
  show();
  const t = messages[value];
  expect(screen.getByRole("button", { name: t["appearance.label"] })).toHaveAttribute("aria-description", `${t["appearance.label"]} · ${t["appearance.system"]}`);
  fireEvent.click(screen.getByRole("button", { name: t["accent.label"] }));
  expect(await screen.findByRole("textbox", { name: t["accent.custom"] })).toBeInTheDocument();
});

it("shows all seven languages as selected radios and persists the chosen locale", async () => {
  const view = show();
  fireEvent.click(screen.getByRole("button", { name: "Language" }));
  const radios = await screen.findAllByRole("menuitemradio");
  expect(radios.map(radio => radio.textContent)).toEqual(LANGUAGES.map(language => language.label));
  expect(screen.getByRole("menuitemradio", { name: "English" })).toHaveAttribute("aria-checked", "true");
  fireEvent.click(screen.getByRole("menuitemradio", { name: "日本語" }));
  await waitFor(() => expect(localStorage.getItem(LOCALE_STORAGE_KEY)).toBe("ja-JP"));
  expect(document.documentElement.lang).toBe("ja-JP");
  view.unmount();
  show();
  fireEvent.click(screen.getByRole("button", { name: messages["ja-JP"]["locale.label"] }));
  expect(await screen.findByRole("menuitemradio", { name: "日本語" })).toHaveAttribute("aria-checked", "true");
});

it("preserves navigation callbacks, active state and shortcut numbering", () => {
  const { setNavigation } = show();
  const home = screen.getByRole("button", { name: /^Home/ });
  expect(home).toHaveAttribute("aria-current", "page");
  expect(home).toHaveAttribute("aria-keyshortcuts", "Control+1 Meta+1");
  const projects = screen.getByRole("button", { name: /^Projects/ });
  fireEvent.click(projects);
  expect(setNavigation).toHaveBeenCalledWith("projects");
});

it("reveals shortcut keys only while Ctrl or Command is held on its own", () => {
  vi.useFakeTimers();
  try {
    const view = show();
    const root = document.documentElement;
    fireEvent.keyDown(window, { key: "Control", ctrlKey: true });
    expect(root).not.toHaveAttribute("data-shortcut-hints");
    act(() => { vi.advanceTimersByTime(400); });
    expect(root).toHaveAttribute("data-shortcut-hints");
    fireEvent.keyUp(window, { key: "Control" });
    expect(root).not.toHaveAttribute("data-shortcut-hints");

    // A chord such as Ctrl+C cancels the pending reveal instead of flashing hints.
    fireEvent.keyDown(window, { key: "Meta", metaKey: true });
    fireEvent.keyDown(window, { key: "c", metaKey: true });
    act(() => { vi.advanceTimersByTime(400); });
    expect(root).not.toHaveAttribute("data-shortcut-hints");

    fireEvent.keyDown(window, { key: "Control", ctrlKey: true });
    act(() => { vi.advanceTimersByTime(400); });
    fireEvent(window, new Event("blur"));
    expect(root).not.toHaveAttribute("data-shortcut-hints");

    fireEvent.keyDown(window, { key: "Control", ctrlKey: true });
    view.unmount();
    act(() => { vi.advanceTimersByTime(400); });
    expect(root).not.toHaveAttribute("data-shortcut-hints");
  } finally {
    vi.useRealTimers();
  }
});
