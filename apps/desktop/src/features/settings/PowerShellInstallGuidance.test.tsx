import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import type { DesktopState } from "../../models/topology";
import { PowerShellInstallGuidance } from "./PowerShellInstallGuidance";

const api = vi.hoisted(() => ({ getState: vi.fn(), openPowerShellInstallGuide: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
const state = {
  persistent_environment: "first-environment", current_operation: null,
  powershell_runtime: { pwsh_available: false, windows_powershell_available: true },
} as DesktopState;
function view(value: DesktopState, onState: (state: DesktopState) => void) {
  return <LocaleProvider><PowerShellInstallGuidance state={value} onState={onState} /></LocaleProvider>;
}
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (value: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.getState.mockResolvedValue(state); api.openPowerShellInstallGuide.mockResolvedValue(undefined);
});
afterEach(cleanup);

it.each(["unmount", "identity", "identity round trip"])("discards a late PowerShell recheck after %s", async boundary => {
  const pending = deferred<DesktopState>(), onState = vi.fn();
  api.getState.mockReturnValueOnce(pending.promise);
  const rendered = render(view(state, onState));
  fireEvent.click(screen.getByRole("button", { name: "Recheck" }));
  expect(api.getState).toHaveBeenCalledTimes(1);
  if (boundary === "unmount") rendered.unmount();
  else {
    rendered.rerender(view({ ...state, persistent_environment: "second-environment" }, onState));
    if (boundary === "identity round trip") rendered.rerender(view(state, onState));
  }
  await act(async () => { pending.resolve(state); });
  expect(onState).not.toHaveBeenCalled();
  if (boundary !== "unmount") expect(screen.getByRole("button", { name: "Recheck" })).toBeEnabled();
});

it("does not carry a late installation-guide error into a replacement target", async () => {
  const pending = deferred<void>();
  api.openPowerShellInstallGuide.mockReturnValueOnce(pending.promise);
  const rendered = render(view(state, vi.fn()));
  fireEvent.click(screen.getByRole("button", { name: "Open Microsoft installation guide" }));
  rendered.rerender(view({ ...state, persistent_environment: "second-environment" }, vi.fn()));
  await act(async () => { pending.reject({ code: "guide_failed", message: "Old target failure", next_action: "Retry" }); });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(api.openPowerShellInstallGuide).toHaveBeenCalledTimes(1);
});

it("publishes a current recheck exactly once without restarting or installing anything", async () => {
  const pending = deferred<DesktopState>(), onState = vi.fn();
  api.getState.mockReturnValueOnce(pending.promise);
  const rendered = render(view(state, onState));
  const button = screen.getByRole("button", { name: "Recheck" });
  fireEvent.click(button); fireEvent.click(button);
  rendered.rerender(view({ ...state, activity_sequence: 2 }, onState));
  const next = { ...state, powershell_runtime: { ...state.powershell_runtime!, pwsh_available: true } };
  await act(async () => { pending.resolve(next); });
  expect(onState).toHaveBeenCalledExactlyOnceWith(next);
  expect(api.getState).toHaveBeenCalledTimes(1);
  expect(api.openPowerShellInstallGuide).not.toHaveBeenCalled();
});
