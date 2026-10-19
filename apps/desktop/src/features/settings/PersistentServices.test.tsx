import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import { LocalServicesPanel } from "./LocalServicesPanel";
import type { DesktopState } from "../../models/topology";

const api = vi.hoisted(() => ({ diagnostics: vi.fn(), environmentServiceAction: vi.fn(), repairEnvironmentUserCredential: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const snapshot = {
  observed_at_ms: 1,
  trace: { configured_mode: "off", effective_mode: "off", revision: "fence", can_edit: false, restart_required: false, can_restart: false, error_code: null },
  configuration: { reason_code: null, backup_available: false, primary_fingerprint: null },
  resources: [], can_copy_console_credential: false, credential_copy_fence: null,
  report: { schema_version: 1, desktop: {}, last_webcodex_call: null }, markdown: "",
} as never;

const baseState = {
  persistent_environment: "environment-1",
  topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" }, exposure: { kind: "none" }, enrollment: { kind: "managed_pairing" } },
  readiness: { server: "ready", runner: "stopped", exposure: "disabled", project: "none", runtime_ready: false, ready_for_chatgpt: false, summary: "", summary_kind: "runtime_stopped" },
  current_operation: null, can_repair_runner_credential: true,
} as unknown as DesktopState;
const wrap = (child: React.ReactNode) => <LocaleProvider><DesktopMantineProvider>{child}</DesktopMantineProvider></LocaleProvider>;

beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.diagnostics.mockResolvedValue(snapshot);
  api.environmentServiceAction.mockResolvedValue(baseState);
  api.repairEnvironmentUserCredential.mockResolvedValue(baseState);
});

describe("persistent local services", () => {
  it("shows local service actions and applies returned state after an explicit action", async () => {
    const onState = vi.fn();
    render(wrap(<LocalServicesPanel state={baseState} onState={onState} />));
    const section = await screen.findByTestId("persistent-services");
    expect(within(section).getByRole("heading", { name: "Server connection" })).toBeInTheDocument();
    expect(within(section).getByRole("heading", { name: "Local Runner" })).toBeInTheDocument();
    expect(api.environmentServiceAction).not.toHaveBeenCalled();
    fireEvent.click(within(section).getByRole("button", { name: "Restart Local Server" }));
    await waitFor(() => expect(api.environmentServiceAction).toHaveBeenCalledExactlyOnceWith({ environmentId: "environment-1", component: "server", action: "restart" }));
    await waitFor(() => expect(onState).toHaveBeenCalledWith(baseState));
  });

  it("does not publish a late service result after unmount", async () => {
    let finish!: (value: DesktopState) => void;
    api.environmentServiceAction.mockReturnValue(new Promise<DesktopState>(resolve => { finish = resolve; }));
    const onState = vi.fn();
    const mounted = render(wrap(<LocalServicesPanel state={baseState} onState={onState} />));
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Server" }));
    mounted.unmount();
    await act(async () => finish(baseState));
    expect(api.environmentServiceAction).toHaveBeenCalledOnce();
    expect(onState).not.toHaveBeenCalled();
  });

  it("keeps a new target's action independent of a late prior service result", async () => {
    let finishOld!: (value: DesktopState) => void, finishNew!: (value: DesktopState) => void;
    api.environmentServiceAction
      .mockReturnValueOnce(new Promise<DesktopState>(resolve => { finishOld = resolve; }))
      .mockReturnValueOnce(new Promise<DesktopState>(resolve => { finishNew = resolve; }));
    const onState = vi.fn();
    const mounted = render(wrap(<LocalServicesPanel state={baseState} onState={onState} />));
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Server" }));
    const nextState = { ...baseState, persistent_environment: "environment-2" };
    mounted.rerender(wrap(<LocalServicesPanel state={nextState} onState={onState} />));
    const restart = screen.getByRole("button", { name: "Restart Local Server" });
    expect(restart).toBeEnabled();
    fireEvent.click(restart); fireEvent.click(restart);
    expect(api.environmentServiceAction).toHaveBeenCalledTimes(2);
    expect(api.environmentServiceAction).toHaveBeenLastCalledWith({ environmentId: "environment-2", component: "server", action: "restart" });
    await act(async () => finishOld(baseState));
    expect(onState).not.toHaveBeenCalled();
    expect(restart).toBeDisabled();
    await act(async () => finishNew(nextState));
    expect(onState).toHaveBeenCalledExactlyOnceWith(nextState);
    expect(restart).toBeEnabled();
  });

  it("discards a prior target's late service error", async () => {
    let fail!: (reason: unknown) => void;
    api.environmentServiceAction.mockReturnValue(new Promise<DesktopState>((_resolve, reject) => { fail = reject; }));
    const onState = vi.fn();
    const mounted = render(wrap(<LocalServicesPanel state={baseState} onState={onState} />));
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Server" }));
    mounted.rerender(wrap(<LocalServicesPanel state={{ ...baseState, persistent_environment: "environment-2" }} onState={onState} />));
    await act(async () => fail({ code: "service_start_failed", message: "Old target failed", next_action: "Retry" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Restart Local Server" })).toBeEnabled();
    expect(onState).not.toHaveBeenCalled();
  });

  it("retains an admitted action across observation-only state changes", async () => {
    let finish!: (value: DesktopState) => void;
    api.environmentServiceAction.mockReturnValue(new Promise<DesktopState>(resolve => { finish = resolve; }));
    const onState = vi.fn();
    const mounted = render(wrap(<LocalServicesPanel state={baseState} onState={onState} />));
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Server" }));
    mounted.rerender(wrap(<LocalServicesPanel state={{ ...baseState, readiness: { ...baseState.readiness, runner: "connecting" } }} onState={onState} />));
    expect(screen.getByRole("button", { name: "Restart Local Server" })).toBeDisabled();
    await act(async () => finish(baseState));
    expect(onState).toHaveBeenCalledExactlyOnceWith(baseState);
  });

  it("hides controls for absent or remote persistent components", async () => {
    const remote = { ...baseState, topology: { ...baseState.topology!, server: { kind: "remote" as const, url: "https://example.test" }, runner: { kind: "none" as const } } } as DesktopState;
    render(wrap(<LocalServicesPanel state={remote} onState={vi.fn()} />));
    expect(await screen.findByTestId("persistent-services")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Server connection" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Local Runner" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^(Start|Stop|Restart) Local/ })).not.toBeInTheDocument();
    expect(api.environmentServiceAction).not.toHaveBeenCalled();
  });

  it("offers repair only when the backend capability is true and sends no credential to the webview", async () => {
    const state = { ...baseState, can_repair_runner_credential: false };
    const { rerender } = render(wrap(<LocalServicesPanel state={state} onState={vi.fn()} />));
    await screen.findByTestId("persistent-services");
    expect(screen.queryByRole("button", { name: "Repair Runner credential" })).not.toBeInTheDocument();
    rerender(wrap(<LocalServicesPanel state={baseState} onState={vi.fn()} />));
    fireEvent.click(screen.getAllByText("Advanced")[1]);
    fireEvent.click(await screen.findByRole("button", { name: "Repair Runner credential" }));
    await waitFor(() => expect(api.environmentServiceAction).toHaveBeenCalledWith({ environmentId: "environment-1", component: "runner", action: "repair_credential" }));
    expect(screen.queryByLabelText("Existing Server user API token")).not.toBeInTheDocument();
  });

  it("shows service observations without OS service controls when no persistent environment exists", async () => {
    render(wrap(<LocalServicesPanel state={{ ...baseState, persistent_environment: null }} onState={vi.fn()} />));
    expect(screen.getByRole("heading", { name: "Server connection" })).toBeInTheDocument();
    expect(screen.queryByTestId("persistent-services")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^(Start|Stop|Restart) Local/ })).not.toBeInTheDocument();
    expect(api.environmentServiceAction).not.toHaveBeenCalled();
  });

  it("surfaces only the safe error code returned by the service action", async () => {
    api.environmentServiceAction.mockRejectedValue({ code: "service_start_failed", message: "private host detail", next_action: "Retry" });
    render(wrap(<LocalServicesPanel state={baseState} onState={vi.fn()} />));
    fireEvent.click(screen.getByRole("button", { name: "Restart Local Server" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("service_start_failed");
    expect(screen.queryByText("private host detail")).not.toBeInTheDocument();
  });

  it("allows saved Server user credential recovery for a persistent environment and clears the password before a failed call", async () => {
    api.repairEnvironmentUserCredential.mockRejectedValue({ code: "credential_repair_failed", message: "private auth detail", next_action: "Retry" });
    const remote = {
      ...baseState,
      can_repair_runner_credential: false,
      topology: { ...baseState.topology!, server: { kind: "remote" as const, url: "https://server.example" }, runner: { kind: "none" as const } },
    } as DesktopState;
    render(wrap(<DiagnosticsPanel state={remote} onState={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Restore Server user credential" }));
    const input = await screen.findByLabelText("Existing Server user API token");
    fireEvent.change(input, { target: { value: "existing-api-token" } });
    fireEvent.click(screen.getByRole("button", { name: "Save credential" }));
    await waitFor(() => expect(api.repairEnvironmentUserCredential).toHaveBeenCalledExactlyOnceWith({ environmentId: "environment-1", userToken: "existing-api-token" }));
    expect(input).toHaveValue("");
    expect(await screen.findByRole("alert")).toHaveTextContent("credential_repair_failed");
    expect(screen.queryByText("existing-api-token")).not.toBeInTheDocument();
    expect(screen.queryByText("private auth detail")).not.toBeInTheDocument();
  });

  it("updates Desktop state after a successful saved user credential recovery", async () => {
    const onState = vi.fn();
    render(wrap(<DiagnosticsPanel state={baseState} onState={onState} />));
    fireEvent.click(await screen.findByRole("button", { name: "Restore Server user credential" }));
    fireEvent.change(await screen.findByLabelText("Existing Server user API token"), { target: { value: "replacement-token" } });
    fireEvent.click(screen.getByRole("button", { name: "Save credential" }));
    await waitFor(() => expect(api.repairEnvironmentUserCredential).toHaveBeenCalledWith({ environmentId: "environment-1", userToken: "replacement-token" }));
    await waitFor(() => expect(onState).toHaveBeenCalledWith(baseState));
    expect(screen.queryByLabelText("Existing Server user API token")).not.toBeInTheDocument();
  });
});
