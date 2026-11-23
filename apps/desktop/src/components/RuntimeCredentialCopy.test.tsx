import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "./DesktopMantineProvider";
import { LocaleProvider } from "../i18n/locale";
import type { DesktopState } from "../models/topology";
import type { DiagnosticSnapshot } from "../models/runtime-shell";
import { RuntimeCredentialCopy } from "./RuntimeCredentialCopy";

const api = vi.hoisted(() => ({ diagnostics: vi.fn(), getState: vi.fn(), copyRuntimeConsoleCredential: vi.fn() }));
vi.mock("../lib/desktop-api", () => ({ desktopApi: api }));
const state = {
  persistent_environment: "local",
  topology: { experience: "full", server: {kind: "local"}, runner: {kind: "local"}},
  readiness: {server: "ready"},
} as DesktopState;
const snapshot = { resources: ["runtime_console"], can_copy_console_credential: true, credential_copy_fence: "credential-fence" } as DiagnosticSnapshot;
const wrap = (value: DesktopState = state) => <DesktopMantineProvider><LocaleProvider><RuntimeCredentialCopy state={value} label="Copy WebCodex sign-in credential" /></LocaleProvider></DesktopMantineProvider>;
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.diagnostics.mockResolvedValue(snapshot);
  api.copyRuntimeConsoleCredential.mockResolvedValue(undefined);
});

describe("Runtime credential handoff", () => {
  it("reads only on request and copies once through the native fenced command after confirmation", async () => {
    let finish!: () => void;
    api.copyRuntimeConsoleCredential.mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
    render(wrap());
    expect(api.diagnostics).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name: "Copy WebCodex sign-in credential"}));
    const dialog = await screen.findByRole("dialog", {name: "Sensitive clipboard action"});
    const confirm = within(dialog).getByRole("button", {name: "Copy sensitive credential"});
    await waitFor(() => expect(confirm).toBeEnabled());
    expect(api.copyRuntimeConsoleCredential).not.toHaveBeenCalled();
    fireEvent.click(confirm); fireEvent.click(confirm);
    expect(api.copyRuntimeConsoleCredential).toHaveBeenCalledExactlyOnceWith("credential-fence");
    await act(async () => finish());
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent("Copied");
    expect(document.body.textContent).not.toContain("credential-fence");
  });

  it("closes stale confirmation and discards delayed diagnostics after changing environments", async () => {
    let finish!: (value: DiagnosticSnapshot) => void;
    api.diagnostics.mockImplementationOnce(() => new Promise<DiagnosticSnapshot>(resolve => { finish = resolve; }));
    const view = render(wrap());
    fireEvent.click(screen.getByRole("button", {name: "Copy WebCodex sign-in credential"}));
    view.rerender(wrap({...state, persistent_environment: "other"}));
    await act(async () => finish(snapshot));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.copyRuntimeConsoleCredential).not.toHaveBeenCalled();
    api.diagnostics.mockResolvedValue({...snapshot, credential_copy_fence: "other-fence"});
    fireEvent.click(screen.getByRole("button", {name: "Copy WebCodex sign-in credential"}));
    const confirm = await screen.findByRole("button", {name: "Copy sensitive credential"});
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);
    await waitFor(() => expect(api.copyRuntimeConsoleCredential).toHaveBeenCalledExactlyOnceWith("other-fence"));
  });

  it("does not copy unavailable credentials or retry an uncertain native write", async () => {
    api.diagnostics.mockResolvedValueOnce({...snapshot, can_copy_console_credential: false, credential_copy_fence: null});
    render(wrap());
    fireEvent.click(screen.getByRole("button", {name: "Copy WebCodex sign-in credential"}));
    await screen.findByText(/Desktop has no account credential to copy/);
    expect(screen.getByRole("button", {name: "Copy sensitive credential"})).toBeDisabled();
    expect(api.copyRuntimeConsoleCredential).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", {name: "Cancel"}));
    api.copyRuntimeConsoleCredential.mockRejectedValue({code: "diagnostic_identity_changed", message: "private-credential-canary"});
    fireEvent.click(screen.getByRole("button", {name: "Copy WebCodex sign-in credential"}));
    const confirm = await screen.findByRole("button", {name: "Copy sensitive credential"});
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);
    await screen.findByRole("alert");
    expect(api.copyRuntimeConsoleCredential).toHaveBeenCalledOnce();
    expect(document.body.textContent).not.toContain("private-credential-canary");
  });
});
