import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../../i18n/locale";
import { DesktopMantineProvider } from "../../components/DesktopMantineProvider";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import type { DesktopState } from "../../models/topology";

const api = vi.hoisted(() => ({ diagnostics: vi.fn(), environmentServiceAction: vi.fn(), repairEnvironmentUserCredential: vi.fn() }));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const snapshot = {
  observed_at_ms: 1,
  trace: { mode: "off", effective_mode: "off", revision: "fence", available: false, restart_required: false, can_restart: false, error_code: null },
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
    render(wrap(<DiagnosticsPanel state={baseState} onState={onState} />));
    const section = await screen.findByTestId("persistent-services");
    expect(within(section).getByRole("heading", { name: "Local Server" })).toBeInTheDocument();
    expect(within(section).getByRole("heading", { name: "Local Runner" })).toBeInTheDocument();
    fireEvent.click(within(section).getAllByRole("button", { name: "Start" })[0]);
    await waitFor(() => expect(api.environmentServiceAction).toHaveBeenCalledWith({ environmentId: "environment-1", component: "server", action: "start" }));
    await waitFor(() => expect(onState).toHaveBeenCalledWith(baseState));
  });

  it("hides controls for absent or remote persistent components", async () => {
    const remote = { ...baseState, topology: { ...baseState.topology!, server: { kind: "remote" as const, url: "https://example.test" }, runner: { kind: "none" as const } } } as DesktopState;
    render(wrap(<DiagnosticsPanel state={remote} onState={vi.fn()} />));
    expect(await screen.findByTestId("persistent-services")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Local Server" })).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Local Runner" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
  });

  it("offers repair only when the backend capability is true and sends no credential to the webview", async () => {
    const state = { ...baseState, can_repair_runner_credential: false };
    const { rerender } = render(wrap(<DiagnosticsPanel state={state} onState={vi.fn()} />));
    await screen.findByTestId("persistent-services");
    expect(screen.queryByRole("button", { name: "Repair Runner credential" })).not.toBeInTheDocument();
    rerender(wrap(<DiagnosticsPanel state={baseState} onState={vi.fn()} />));
    fireEvent.click(await screen.findByRole("button", { name: "Repair Runner credential" }));
    await waitFor(() => expect(api.environmentServiceAction).toHaveBeenCalledWith({ environmentId: "environment-1", component: "runner", action: "repair_credential" }));
    expect(screen.queryByLabelText("Existing Server user API token")).not.toBeInTheDocument();
  });

  it("hides the section when no persistent environment exists", async () => {
    render(wrap(<DiagnosticsPanel state={{ ...baseState, persistent_environment: null }} onState={vi.fn()} />));
    await screen.findByRole("heading", { name: "Diagnostics Center" });
    expect(screen.queryByTestId("persistent-services")).not.toBeInTheDocument();
  });

  it("surfaces only the safe error code returned by the service action", async () => {
    api.environmentServiceAction.mockRejectedValue({ code: "service_start_failed", message: "private host detail", next_action: "Retry" });
    render(wrap(<DiagnosticsPanel state={baseState} onState={vi.fn()} />));
    fireEvent.click((await screen.findAllByRole("button", { name: "Start" }))[0]);
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
