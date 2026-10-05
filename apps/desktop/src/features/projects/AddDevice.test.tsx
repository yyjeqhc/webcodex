import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { deviceInvitationText } from "../../i18n/device-invitation";
import { LocaleProvider } from "../../i18n/locale";
import { PRODUCT_LOCALES } from "../../i18n/product";
import type { DesktopState } from "../../models/topology";
import { AddDevice } from "./AddDevice";

const api = vi.hoisted(() => ({ createEnvironmentInvitation: vi.fn() }));
const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => clipboard);
const code = "test-one-time-pairing-secret";
const state = { persistent_environment: "saved-environment", current_operation: null,
  topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" } },
  environment_setup: { environment_id: "saved-environment", mode: "create", runner: true, server_url: "https://server.example", project_path: null, service_scope: "user", configured: true },
} as DesktopState;
function view(selected = state, onViewDevices = vi.fn()) {
  return <DesktopMantineProvider><LocaleProvider><AddDevice state={selected} onViewDevices={onViewDevices} /></LocaleProvider></DesktopMantineProvider>;
}
async function openDialog() {
  fireEvent.click(screen.getByRole("button", { name: "Add device" }));
  return screen.findByRole("dialog");
}
async function invite() {
  fireEvent.click(screen.getByRole("button", { name: "Create invitation" }));
  return screen.findByLabelText("One-time pairing code");
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.createEnvironmentInvitation.mockResolvedValue({ environmentId: state.persistent_environment, pairingCode: code });
  clipboard.writeText.mockResolvedValue(undefined);
});
afterEach(() => vi.restoreAllMocks());

describe("Add device invitation", () => {
  it("requires explicit creation and copy; never persists the response or claims the device is online", async () => {
    const storage = vi.spyOn(Storage.prototype, "setItem"), onViewDevices = vi.fn();
    const mounted = render(view(state, onViewDevices));
    await openDialog();
    expect(api.createEnvironmentInvitation).not.toHaveBeenCalled();
    expect(screen.getByText("No invitation created")).toBeInTheDocument();
    expect(screen.getByText(/ChatGPT uses the separate OpenAI Tunnel/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Server URL for the other machine"), { target: { value: "https://other-machine.example:8787" } });
    const input = await invite();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledExactlyOnceWith("saved-environment");
    expect(input).toHaveAttribute("type", "password"); expect(input).toHaveValue(code);
    expect(screen.getByText(/Invitation created · device connection not yet confirmed/)).toBeInTheDocument();
    expect(clipboard.writeText).not.toHaveBeenCalled();
    expect(storage.mock.calls.every(args => !JSON.stringify(args).includes(code))).toBe(true);
    expect(JSON.stringify(state)).not.toContain(code);
    fireEvent.click(screen.getByRole("button", { name: "Show code" })); expect(input).toHaveAttribute("type", "text");
    fireEvent.click(screen.getByRole("button", { name: "Hide code" })); expect(input).toHaveAttribute("type", "password");
    fireEvent.click(screen.getByRole("button", { name: "Copy pairing code" }));
    await waitFor(() => expect(clipboard.writeText).toHaveBeenCalledExactlyOnceWith(code));
    // Correcting connection guidance retains the same invitation and target.
    fireEvent.change(screen.getByLabelText("Server URL for the other machine"), { target: { value: "http://127.0.0.1" } });
    expect(screen.getByRole("button", { name: "Copy Server URL" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Server URL for the other machine"), { target: { value: "https://corrected-server.example" } });
    fireEvent.click(screen.getByRole("button", { name: "Copy Server URL" }));
    await waitFor(() => expect(clipboard.writeText).toHaveBeenLastCalledWith("https://corrected-server.example"));
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1); expect(input).toHaveValue(code);
    fireEvent.click(screen.getByRole("button", { name: "View devices and projects" }));
    expect(onViewDevices).toHaveBeenCalledTimes(1);
    await openDialog(); expect(screen.queryByLabelText("One-time pairing code")).toBeNull();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1);
    mounted.unmount(); expect(storage.mock.calls.every(args => !JSON.stringify(args).includes(code))).toBe(true);
  });
  it.each([
    ["http://127.0.0.1:8787", /Loopback and wildcard/],
    ["http://[::1]:8787", /Loopback and wildcard/],
    ["https://tunnel.openai.com", /OpenAI or ChatGPT address/],
    ["https://user:secret@server.example", /without a path, credentials/],
    ["not a URL", /without a path, credentials/],
  ])("blocks misleading remote address %s without signing an invitation", async (url, message) => {
    render(view()); await openDialog();
    fireEvent.change(screen.getByLabelText("Server URL for the other machine"), { target: { value: url } });
    expect(screen.getByRole("alert")).toHaveTextContent(message);
    expect(screen.getByRole("button", { name: "Create invitation" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "Create invitation" }).closest("form")!);
    expect(api.createEnvironmentInvitation).not.toHaveBeenCalled();
  });
  it.each(["permission_denied", "authentication_required", "server_admin_required", "server_unreachable", "response_uncertain"])("reports %s without logging secrets or retrying", async errorCode => {
    const logger = vi.spyOn(console, "error");
    api.createEnvironmentInvitation.mockRejectedValue({ code: errorCode, message: code, next_action: code });
    render(view()); await openDialog(); fireEvent.click(screen.getByRole("button", { name: "Create invitation" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("alert")).toHaveTextContent(errorCode.includes("permission") || errorCode.includes("authentication") || errorCode.includes("admin") ? "did not authorize" : "result is unconfirmed");
    expect(screen.queryByLabelText("One-time pairing code")).toBeNull(); expect(screen.queryByText(code)).toBeNull();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1); expect(logger).not.toHaveBeenCalled();
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]); await openDialog();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1);
  });
  it("locks duplicate creation and discards late responses after close or environment change", async () => {
    let finish!: (value: { environmentId: string; pairingCode: string }) => void;
    api.createEnvironmentInvitation.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const mounted = render(view()); await openDialog();
    const create = screen.getByRole("button", { name: "Create invitation" }); fireEvent.click(create); fireEvent.click(create);
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1);
    expect(screen.getByText("Creating invitation…")).toBeInTheDocument();
    mounted.rerender(view({ ...state, persistent_environment: "other-environment" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    await act(async () => finish({ environmentId: "saved-environment", pairingCode: code }));
    mounted.rerender(view());
    expect(screen.queryByRole("dialog")).toBeNull();
    mounted.rerender(view({ ...state, persistent_environment: "other-environment" }));
    await openDialog(); expect(screen.queryByLabelText("One-time pairing code")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Create invitation" }));
    fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0]);
    await act(async () => finish({ environmentId: "other-environment", pairingCode: code }));
    await openDialog(); expect(screen.queryByLabelText("One-time pairing code")).toBeNull();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(2);
  });
  it("rejects a mismatched response and never displays its secret", async () => {
    api.createEnvironmentInvitation.mockResolvedValue({ environmentId: "other-environment", pairingCode: code });
    render(view()); await openDialog(); fireEvent.click(screen.getByRole("button", { name: "Create invitation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("result is unconfirmed");
    expect(screen.queryByLabelText("One-time pairing code")).toBeNull();
  });
  it("clears expired display information on resume and never automatically creates another code", async () => {
    let now = 0; vi.spyOn(performance, "now").mockImplementation(() => now);
    render(view()); await openDialog(); await invite();
    // The expiry listener is installed by an effect after the created view commits.
    // Flush that effect before simulating the later app-resume event.
    await act(async () => {});
    now = 600_001; fireEvent(document, new Event("visibilitychange"));
    expect(screen.queryByLabelText("One-time pairing code")).toBeNull();
    expect(screen.getByText(/local display period ended/)).toBeInTheDocument();
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(1); expect(clipboard.writeText).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Create invitation" })); await screen.findByLabelText("One-time pairing code");
    expect(api.createEnvironmentInvitation).toHaveBeenCalledTimes(2);
  });
  it("keeps Core authorization authoritative for saved Server-only environments and hides joined viewers", async () => {
    const mounted = render(view({ ...state, topology: { ...state.topology!, runner: { kind: "none" } }, environment_setup: { ...state.environment_setup!, runner: false } } as DesktopState));
    await openDialog(); expect(api.createEnvironmentInvitation).not.toHaveBeenCalled();
    mounted.rerender(view({ ...state, environment_setup: { ...state.environment_setup!, mode: "join" } }));
    expect(screen.queryByRole("button", { name: "Add device" })).toBeNull(); expect(screen.queryByRole("dialog")).toBeNull();
    mounted.rerender(view({ ...state, persistent_environment: null }));
    expect(screen.queryByRole("button", { name: "Add device" })).toBeNull();
  });
  it("supports keyboard form submission and returns focus to its trigger after Escape", async () => {
    render(view()); const trigger = screen.getByRole("button", { name: "Add device" }); trigger.focus();
    const dialog = await openDialog();
    const url = screen.getByLabelText("Server URL for the other machine");
    await waitFor(() => expect(url).toHaveFocus());
    fireEvent.submit(within(dialog).getByRole("button", { name: "Create invitation" }).closest("form")!);
    await screen.findByLabelText("One-time pairing code");
    fireEvent.keyDown(document.activeElement!, { key: "Escape", code: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(trigger).toHaveFocus());
  });
  it.each(PRODUCT_LOCALES)("renders the full flow in %s", async locale => {
    localStorage.setItem("webcodex.desktop.locale", locale); render(view());
    fireEvent.click(screen.getByRole("button", { name: deviceInvitationText(locale, "add") }));
    await screen.findByRole("dialog");
    expect(screen.getByLabelText(deviceInvitationText(locale, "address"))).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: deviceInvitationText(locale, "create") }));
    expect(await screen.findByLabelText(deviceInvitationText(locale, "code"))).toHaveAttribute("type", "password");
    for (const key of ["network", "validity", "install", "join", "verify", "created"] as const) {
      expect(deviceInvitationText(locale, key).trim()).not.toBe("");
      expect(screen.getByText(deviceInvitationText(locale, key))).toBeInTheDocument();
    }
  });
});
