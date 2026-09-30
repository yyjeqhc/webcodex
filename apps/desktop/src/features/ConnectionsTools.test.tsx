import { useState } from "react";
import { MantineProvider } from "@mantine/core";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocaleProvider } from "../i18n/locale";
import { CONNECTIONS_TOOLS_MESSAGES, connectionsToolsText } from "../i18n/connections-tools";
import { PRODUCT_LOCALES } from "../i18n/product";
import type { DesktopState, RunnerSettings } from "../models/topology";
import { EMPTY_MCP_PROVIDERS, type McpProviderRequest, type TunnelProfileRequest } from "../models/connections-tools";
import { connectionFixture, connectionSnapshot } from "../test/connections-fixtures";
import { ConnectionPanel } from "./connection/ConnectionPanel";
import { McpProvidersPanel } from "./extensions/McpProvidersPanel";

const api = vi.hoisted(() => ({ saveTunnelProfile: vi.fn(), tunnelProfileAction: vi.fn(), saveMcpProvider: vi.fn(), removeMcpProvider: vi.fn(), runnerSettings: vi.fn(), restartOwnedRunner: vi.fn(), stopQuickShare: vi.fn() }));
vi.mock("../lib/desktop-api", () => ({ desktopApi: api }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn() }));
const onSettings = vi.fn();
const target = { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:62645" };
const settings: RunnerSettings = { target, paths: { instruction_files: [], skill_roots: [] }, file_access: { configured_roots: [], effective_roots: ["/fixture"], using_default_roots: true, allow_cwd_anywhere: false }, plugin_ids: [], can_restart: true };
function state(): DesktopState {
  return {
    topology: { experience: "full", server: { kind: "local" }, runner: { kind: "local" }, exposure: { kind: "none" }, enrollment: { kind: "managed_pairing" } },
    project: { path: "/fixture", allowed_root: "/fixture", is_git_repository: true, runtime_project_id: "agent:fixture:project" },
    readiness: { runtime_ready: true, ready_for_chatgpt: false, server: "ready", runner: "ready", exposure: "local_ready", project: "ready", summary: "Ready", summary_kind: "runtime_ready_local_only", next_action: "", next_action_kind: "choose_connection" },
    openai_tunnel_config: { source: "file", tunnel_id_present: true, api_key_present: true, saved_tunnel_id: "tunnel_fixture", effective_tunnel_id: "tunnel_fixture" },
    activity_sequence: 0, openai_tunnel_configured: true, regular_tunnel_available: true, runtime_autostart: true, preferred_connection: "no_chat_gpt",
    tunnel_proxy: { mode: "auto", custom_url: null, effective_source: "direct", effective_proxy_present: false, system_proxy_detected: false },
    connections: connectionSnapshot(connectionFixture({ id: "personal", name: "ChatGPT Personal" }), connectionFixture({ id: "work", name: "ChatGPT Work", lifecycle: "error", pid: null, ready: false, last_error: "process_exited" }), connectionFixture({ id: "third", name: "Account 3", pid: 300 })),
    mcp_providers: { ...EMPTY_MCP_PROVIDERS, profiles: [] },
  };
}
function Harness({ mode, initial = state() }: { mode: "connections" | "mcp"; initial?: DesktopState }) {
  const [snapshot, setSnapshot] = useState(initial);
  return <MantineProvider><LocaleProvider>{mode === "connections" ? <ConnectionPanel state={snapshot} onState={setSnapshot} onSettings={onSettings} /> : <McpProvidersPanel state={snapshot} onState={setSnapshot} settings={settings} onRestarted={() => undefined} />}</LocaleProvider></MantineProvider>;
}
beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.tunnelProfileAction.mockResolvedValue(state());
  api.runnerSettings.mockResolvedValue(settings);
  api.restartOwnedRunner.mockResolvedValue(state());
});

describe("Connections + Tools control surfaces", () => {
  it("shows 2/3 connections without degrading runtime and routes each action by identity", async () => {
    render(<Harness mode="connections" />);
    expect(screen.getByText("2 / 3 Running")).toBeInTheDocument();
    expect(within(screen.getByRole("article", { name: "ChatGPT Work" })).getByText("Connection unavailable")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Restart ChatGPT Personal" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("personal", "restart"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Stop Account 3" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Stop Account 3" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("third", "stop"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Restart ChatGPT Work" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Restart ChatGPT Work" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("work", "restart"));
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it("can stop a failed enabled profile after its child exits and start it again without touching peers", async () => {
    const initial = state();
    const stopped = state();
    stopped.connections = connectionSnapshot(...initial.connections!.profiles.map(profile => profile.id === "work"
      ? { ...profile, enabled: false, lifecycle: "stopped" as const, last_error: null, pid: null, ready: false }
      : profile));
    api.tunnelProfileAction.mockResolvedValue(stopped);
    render(<Harness mode="connections" initial={initial} />);
    const work = screen.getByRole("article", { name: "ChatGPT Work" });
    expect(within(work).getByRole("button", { name: "Restart ChatGPT Work" })).toBeEnabled();
    fireEvent.click(within(work).getByRole("button", { name: "Stop ChatGPT Work" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenCalledExactlyOnceWith("work", "stop"));
    await waitFor(() => expect(within(work).getByRole("button", { name: "Start ChatGPT Work" })).toBeEnabled());
    expect(within(work).queryByRole("button", { name: "Stop ChatGPT Work" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("article", { name: "ChatGPT Personal" })).getByText("Running")).toBeInTheDocument();
    expect(screen.getByText("2 / 3 Running")).toBeInTheDocument();
    fireEvent.click(within(work).getByRole("button", { name: "Start ChatGPT Work" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("work", "start"));
    expect(api.tunnelProfileAction.mock.calls.every(([id]) => id === "work")).toBe(true);
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it("offers Direct as a contextual recovery check only when Auto failed through a detected proxy", async () => {
    const initial = state();
    initial.tunnel_proxy = { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true };
    api.tunnelProfileAction.mockRejectedValueOnce({ code: "tunnel_unavailable", message: "Tunnel unavailable", next_action: "Retry" });
    render(<Harness mode="connections" initial={initial} />);
    fireEvent.click(screen.getByRole("button", { name: "Restart ChatGPT Personal" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Auto currently uses a detected proxy");
    expect(alert).toHaveTextContent("try Direct if a VPN or TUN already routes your traffic");
  });

  it("shows proxy recovery for asynchronous readiness failure without a rejected start call", () => {
    const initial = state();
    initial.tunnel_proxy = { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true };
    initial.connections = connectionSnapshot(connectionFixture({ id: "work", name: "ChatGPT Work", lifecycle: "error", pid: null, ready: false, last_error: "tunnel_unavailable", failure_stage: "tunnel_daemon_readiness", reason_code: "tunnel_daemon_not_ready", auto_proxy_used: true }));
    render(<Harness mode="connections" initial={initial} />);
    const card = screen.getByRole("article", { name: "ChatGPT Work" });
    expect(within(card).getByText(/The failed attempt used an automatically detected proxy/)).toHaveTextContent("try Direct");
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it.each([
    ["tunnel_client_download_failed", "tunnel_client_download", "could not be downloaded", true],
    ["tunnel_client_install_failed", "tunnel_client_install", "could not be installed", false],
    ["tunnel_client_verification_failed", "tunnel_client_verification", "failed integrity verification", false],
  ] as const)("shows specific recovery for %s instead of blaming credentials", (reason_code, failure_stage, message, proxyHint) => {
    const initial = state();
    initial.tunnel_proxy = { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true };
    initial.connections = connectionSnapshot(connectionFixture({ id: "work", name: "ChatGPT Work", lifecycle: "error", ready: false, last_error: "tunnel_unavailable", reason_code, failure_stage, auto_proxy_used: true }));
    render(<Harness mode="connections" initial={initial} />);
    const card = screen.getByRole("article", { name: "ChatGPT Work" });
    expect(card).toHaveTextContent(message);
    expect(card).not.toHaveTextContent("Check this connection’s credentials");
    expect(within(card).queryByText(/The failed attempt used an automatically detected proxy/) !== null).toBe(proxyHint);
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
  });

  it("uses the failed attempt's proxy evidence rather than the current global setting", () => {
    const afterAutoFailure = state();
    afterAutoFailure.tunnel_proxy = { mode: "direct", custom_url: null, effective_source: "direct", effective_proxy_present: false, system_proxy_detected: true };
    afterAutoFailure.connections = connectionSnapshot(connectionFixture({ lifecycle: "error", ready: false, last_error: "tunnel_unavailable", failure_stage: "tunnel_daemon_readiness", reason_code: "tunnel_daemon_not_ready", auto_proxy_used: true }));
    const { unmount } = render(<Harness mode="connections" initial={afterAutoFailure} />);
    expect(screen.getByText(/The failed attempt used an automatically detected proxy/)).toHaveTextContent("try Direct");
    unmount();

    const afterDirectFailure = state();
    afterDirectFailure.tunnel_proxy = { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true };
    afterDirectFailure.connections = connectionSnapshot(connectionFixture({ lifecycle: "error", ready: false, last_error: "tunnel_unavailable", failure_stage: "tunnel_daemon_readiness", reason_code: "tunnel_daemon_not_ready", auto_proxy_used: false }));
    render(<Harness mode="connections" initial={afterDirectFailure} />);
    expect(screen.queryByText(/The failed attempt used an automatically detected proxy/)).not.toBeInTheDocument();
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
  });

  it("does not attribute unrelated Tunnel failures to Clash or proxy detection", async () => {
    const initial = state();
    initial.tunnel_proxy = { mode: "auto", custom_url: null, effective_source: "system", effective_proxy_present: true, system_proxy_detected: true };
    api.tunnelProfileAction.mockRejectedValueOnce({ code: "process_failed", message: "Process failed", next_action: "Retry" });
    render(<Harness mode="connections" initial={initial} />);
    fireEvent.click(screen.getByRole("button", { name: "Restart ChatGPT Personal" }));
    const alert = await screen.findByRole("alert");
    expect(alert).not.toHaveTextContent("Clash");
    expect(alert).not.toHaveTextContent("Direct mode");
  });

  it.each([
    ["tunnel_control_plane_unreachable", "tunnel_unavailable", "Cannot reach the OpenAI tunnel service", "network", "Proxy settings"],
    ["tunnel_control_plane_probe_failed", "tunnel_unavailable", "Cannot reach the OpenAI tunnel service", "network", "Proxy settings"],
    [null, "local_mcp_unavailable", "Local MCP service is unreachable", "runtime", "Runtime"],
    [null, "health_stale", "Tunnel health reports stopped arriving", "diagnostics", "Troubleshooting"],
    [null, "protocol_invalid", "Tunnel client returned an invalid status", "diagnostics", "Troubleshooting"],
    [null, "startup_timeout", "Tunnel did not become ready", "network", "Proxy settings"],
    [null, "stop_failed", "Tunnel could not be stopped", "diagnostics", "Troubleshooting"],
    [null, null, "Connection unavailable", "diagnostics", "Troubleshooting"],
  ] as const)("explains %s / %s and opens recovery without a process effect", (reason_code, last_error, title, section, button) => {
    const initial = state();
    initial.connections = connectionSnapshot(connectionFixture({ lifecycle: "error", ready: false, pid: null, tunnel_ready: false, local_mcp_ready: true, reason_code, last_error }));
    render(<Harness mode="connections" initial={initial} />);
    const card = screen.getByRole("article", { name: "ChatGPT" });
    expect(card).toHaveTextContent(title);
    expect(card).toHaveTextContent("App creation or tool refresh in ChatGPT may fail");
    expect(card).toHaveTextContent(last_error === "health_stale" ? "Awaiting current status" : "Unreachable");
    fireEvent.click(within(card).getByRole("button", { name: button }));
    expect(onSettings).toHaveBeenCalledExactlyOnceWith(section);
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it("shows a failed Stop before the previous network failure and avoids stale healthy checks", () => {
    const initial = state();
    initial.connections = connectionSnapshot(connectionFixture({ lifecycle: "error", ready: false, last_error: "stop_failed", reason_code: "tunnel_control_plane_unreachable" }));
    const rendered = render(<Harness mode="connections" initial={initial} />);
    expect(screen.getByText("Tunnel could not be stopped")).toBeInTheDocument();
    expect(screen.queryByText("Cannot reach the OpenAI tunnel service")).not.toBeInTheDocument();
    rendered.unmount();
    initial.connections = connectionSnapshot(connectionFixture({ lifecycle: "running", ready: false, last_error: "health_stale", tunnel_ready: true, local_mcp_ready: true }));
    render(<Harness mode="connections" initial={initial} />);
    const card = screen.getByRole("article", { name: "ChatGPT" });
    expect(within(card).getAllByText("Awaiting current status")).toHaveLength(2);
    expect(within(card).queryByText("Reachable")).not.toBeInTheDocument();
  });

  it("explains missing credentials instead of leaving a disabled Start unexplained", () => {
    const initial = state();
    initial.connections = connectionSnapshot(connectionFixture({ lifecycle: "stopped", ready: false, pid: null, credential_present: false }));
    render(<Harness mode="connections" initial={initial} />);
    expect(screen.getByText("Add a Tunnel ID and API Key to start this connection.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start ChatGPT" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Edit ChatGPT" })).toBeEnabled();
  });

  it("requires confirmation to delete exactly one profile and leaves other cards", async () => {
    const next = state(); next.connections!.profiles = next.connections!.profiles.filter(p => p.id !== "personal"); next.connections!.running = 1;
    api.tunnelProfileAction.mockResolvedValue(next);
    render(<Harness mode="connections" />);
    fireEvent.click(screen.getByRole("button", { name: "Delete ChatGPT Personal" }));
    const dialog = screen.getByRole("dialog", { name: "Delete ChatGPT Personal" });
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(screen.queryByRole("article", { name: "ChatGPT Personal" })).not.toBeInTheDocument());
    expect(screen.getByRole("article", { name: "Account 3" })).toBeInTheDocument();
    expect(api.tunnelProfileAction).toHaveBeenCalledExactlyOnceWith("personal", "delete");
  });

  it("keeps profile editing write-only and fences against the opened revision", async () => {
    const captured: TunnelProfileRequest[] = [];
    api.saveTunnelProfile.mockImplementation(async request => { captured.push(structuredClone(request)); return state(); });
    render(<Harness mode="connections" />);
    fireEvent.click(screen.getByRole("button", { name: "Edit ChatGPT Personal" }));
    expect(screen.getByLabelText("API Key")).toHaveValue("");
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Personal renamed" } });
    fireEvent.change(screen.getByLabelText("API Key"), { target: { value: "write-only-fixture-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Save & Apply" }));
    await waitFor(() => expect(captured).toHaveLength(1));
    expect(captured[0]).toMatchObject({ id: "personal", expected_revision: 1, name: "Personal renamed", api_key: "write-only-fixture-secret" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(document.body.textContent).not.toContain("write-only-fixture-secret");
    expect(api.tunnelProfileAction).not.toHaveBeenCalled(); expect(api.restartOwnedRunner).not.toHaveBeenCalled();
  });

  it("saves MCP desired configuration then applies only an explicit Runner restart", async () => {
    const captured: McpProviderRequest[] = [];
    const saved = state(); saved.mcp_providers = { revision: 1, config_error: false, restart_required: true, max_enabled: 8, profiles: [{ id: "provider", name: "Playwright", command: "npx", args: ["-y", "@playwright/mcp"], enabled: true, scope: "runner", env_keys: ["API_KEY"], cwd: null }] };
    api.saveMcpProvider.mockImplementation(async request => { captured.push(structuredClone(request)); return saved; });
    api.restartOwnedRunner.mockResolvedValue({ ...saved, mcp_providers: { ...saved.mcp_providers, restart_required: false } });
    render(<Harness mode="mcp" />);
    fireEvent.click(screen.getByRole("button", { name: "Add MCP Provider" }));
    fireEvent.change(screen.getByLabelText("Provider Name"), { target: { value: "Playwright" } });
    fireEvent.change(screen.getByLabelText("Command"), { target: { value: "npx" } });
    fireEvent.change(screen.getByLabelText("Arguments"), { target: { value: '["-y","@playwright/mcp"]' } });
    fireEvent.click(screen.getByRole("button", { name: "Add Environment Variable" }));
    fireEvent.change(screen.getByLabelText("Variable name 1"), { target: { value: "API_KEY" } });
    fireEvent.change(screen.getByLabelText("Private value API_KEY"), { target: { value: "mcp-ui-private-fixture" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(captured[0]).toMatchObject({ id: null, expected_revision: 0, command: "npx", args: ["-y", "@playwright/mcp"], env: { API_KEY: "mcp-ui-private-fixture" } });
    expect(document.body.textContent).not.toContain("mcp-ui-private-fixture");
    expect(screen.getByText("Credentials present")).toBeInTheDocument();
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Restart Runner" }));
    await waitFor(() => expect(api.restartOwnedRunner).toHaveBeenCalledExactlyOnceWith(target));
    expect(api.tunnelProfileAction).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("button", { name: "Restart Runner" })).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Edit Playwright" }));
    expect(screen.getByLabelText("Private value API_KEY")).toHaveValue("");
  });

  it("rejects invalid provider arguments without invoking or exposing backend errors", async () => {
    render(<Harness mode="mcp" />);
    fireEvent.click(screen.getByRole("button", { name: "Add MCP Provider" }));
    fireEvent.change(screen.getByLabelText("Provider Name"), { target: { value: "Database" } });
    fireEvent.change(screen.getByLabelText("Command"), { target: { value: "node" } });
    fireEvent.change(screen.getByLabelText("Arguments"), { target: { value: '{"invalid":true}' } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Check the arguments array");
    expect(api.saveMcpProvider).not.toHaveBeenCalled();
  });

  it("has complete labels for every supported Desktop locale", () => {
    for (const [key, values] of Object.entries(CONNECTIONS_TOOLS_MESSAGES)) {
      expect(values).toHaveLength(PRODUCT_LOCALES.length);
      for (const locale of PRODUCT_LOCALES) expect(connectionsToolsText(locale, key as keyof typeof CONNECTIONS_TOOLS_MESSAGES).trim().length).toBeGreaterThan(0);
    }
  });
});
