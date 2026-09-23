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
const target = { config_path: "/fixture/runner.toml", client_id: "fixture", server_url: "http://127.0.0.1:62645" };
const settings: RunnerSettings = { target, paths: { instruction_files: [], skill_roots: [] }, plugin_ids: [], can_restart: true };
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
  return <MantineProvider><LocaleProvider>{mode === "connections" ? <ConnectionPanel state={snapshot} onState={setSnapshot} /> : <McpProvidersPanel state={snapshot} onState={setSnapshot} settings={settings} onRestarted={() => undefined} />}</LocaleProvider></MantineProvider>;
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
    expect(screen.getByRole("status", { name: "Workspace" })).toHaveTextContent("ServerRunningRunnerRunningConnections2 / 3 Running");
    expect(within(screen.getByRole("article", { name: "ChatGPT Work" })).getByText("Needs attention")).toBeInTheDocument();
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
    expect(screen.getByRole("status", { name: "Workspace" })).toHaveTextContent("ServerRunningRunnerRunningConnections2 / 3 Running");
    fireEvent.click(within(work).getByRole("button", { name: "Start ChatGPT Work" }));
    await waitFor(() => expect(api.tunnelProfileAction).toHaveBeenLastCalledWith("work", "start"));
    expect(api.tunnelProfileAction.mock.calls.every(([id]) => id === "work")).toBe(true);
    expect(api.restartOwnedRunner).not.toHaveBeenCalled();
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
