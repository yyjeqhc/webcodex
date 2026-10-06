import { act, fireEvent, render, screen, within, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ConfigurationDataPanel } from "./ConfigurationDataPanel";
import { LocaleProvider, LANGUAGES } from "../../i18n/locale";
import { CONFIGURATION_DATA_MESSAGES, configurationDataText } from "../../i18n/configuration-data";
import { desktopApi } from "../../lib/desktop-api";
import { save } from "@tauri-apps/plugin-dialog";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import type { DesktopState } from "../../models/topology";
import type { PathEntry, PathInventory } from "../../models/path-inventory";

vi.mock("../../lib/desktop-api", () => ({ desktopApi: { pathInventory: vi.fn(), openInventoryLocation: vi.fn(), exportInventoryDocument: vi.fn() } }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({ writeText: vi.fn() }));
const state = { persistent_environment: "alpha", topology: { server: { kind: "local" }, runner: { kind: "local" } }, binaries: { directory: "/runtime" } } as DesktopState;
const entry = (id: string, overrides: Partial<PathEntry> = {}): PathEntry => ({ id, component: id.split(".")[0], purpose: id.replaceAll(".", "_"), source: "configured", configured_path: `/private/${id}`, canonical_path: `/resolved/${id}`, directory_to_open: "/resolved", status: "present", kind: "file", category: "mixed_configuration", log_source: null, ...overrides });
const inventory = (overrides: Partial<PathInventory> = {}): PathInventory => ({ schema_version: 1, observed_at_ms: 1, environment_id: "alpha", local_server: true, local_runner: true, service_scope: "user", revision: "revision-1", roots: [entry("environment.root", { kind: "directory" }), entry("desktop.root", { kind: "directory" })], entries: [entry("server.configuration")], issues: [], builds: [], identities: [], ...overrides });
function view(value = state) { return <LocaleProvider><ConfigurationDataPanel state={value} /></LocaleProvider>; }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
beforeEach(() => { vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US"); vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory()); vi.mocked(save).mockResolvedValue(null); });

describe("configuration location observations", () => {
  it("keeps environment and actual Desktop data roots separate and copies only the explicit canonical path", async () => {
    render(view());
    const root = (await screen.findByRole("heading", { name: "Environment · Environment root" })).closest("article")!;
    const desktop = screen.getByRole("heading", { name: "Desktop · Desktop app-data root" }).closest("article")!;
    expect(within(root).getByText("/private/environment.root")).toBeVisible();
    expect(within(desktop).getByText("/private/desktop.root")).toBeVisible();
    expect(writeText).not.toHaveBeenCalled();
    fireEvent.click(within(desktop).getByRole("button", { name: "Copy path" }));
    await screen.findByText("Path copied");
    expect(writeText).toHaveBeenCalledExactlyOnceWith("/resolved/desktop.root");
  });
  it("preserves unconfirmed, missing, remote and unconfigured states without granting local actions", async () => {
    vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory({ roots: [], entries: [entry("server.configuration", { status: "unconfirmed" }), entry("runner.configuration", { status: "missing" }), entry("tunnel.configuration", { status: "remote", kind: "remote_reference" }), entry("desktop.settings", { status: "not_configured", configured_path: null, canonical_path: null })] }));
    render(view()); await screen.findByText("Unconfirmed");
    expect(screen.getByText("Local · confirmed missing")).toBeVisible();
    expect(screen.getByText("Remote reference")).toBeVisible(); expect(screen.getByText("Not configured")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Copy path" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open location" })).not.toBeInTheDocument();
  });
  it("uses ID and revision for opening, and invalidates metadata after a native revision rejection", async () => {
    vi.mocked(desktopApi.openInventoryLocation).mockRejectedValue({ code: "inventory_changed", message: "private native detail must not render", next_action: "refresh" });
    render(view());
    const panel = (await screen.findByRole("heading", { name: "Server · Configuration" })).closest("article")!;
    fireEvent.click(within(panel).getByRole("button", { name: "Open location" }));
    await screen.findByRole("alert");
    expect(desktopApi.openInventoryLocation).toHaveBeenCalledExactlyOnceWith("server.configuration", "revision-1");
    expect(screen.queryByText(/private native detail/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export path inventory" })).not.toBeInTheDocument();
  });
  it("does not duplicate an in-flight native location action", async () => {
    const pending = deferred<void>(); vi.mocked(desktopApi.openInventoryLocation).mockReturnValueOnce(pending.promise);
    render(view()); const panel = (await screen.findByRole("heading", { name: "Server · Configuration" })).closest("article")!;
    const button = within(panel).getByRole("button", { name: "Open location" });
    fireEvent.click(button); fireEvent.click(button);
    expect(desktopApi.openInventoryLocation).toHaveBeenCalledTimes(1);
    await act(async () => pending.resolve());
  });
  it("shows the privacy notice before native picker export and sends no file contents", async () => {
    vi.mocked(save).mockResolvedValue("/selected/manifest.json");
    render(view()); await screen.findByRole("button", { name: "Export backup manifest" });
    expect(screen.getByText(/Exports include private local paths/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Export backup manifest" }));
    await screen.findByText("Metadata exported");
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "webcodex-backup-manifest.json" }));
    expect(desktopApi.exportInventoryDocument).toHaveBeenCalledExactlyOnceWith("backup_manifest", "/selected/manifest.json", "revision-1");
  });
  it("native picker cancellation creates no document", async () => {
    render(view()); await screen.findByRole("button", { name: "Export path inventory" });
    fireEvent.click(screen.getByRole("button", { name: "Export path inventory" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Export path inventory" })).toBeEnabled());
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "webcodex-path-inventory.json" }));
    expect(desktopApi.exportInventoryDocument).not.toHaveBeenCalled();
  });
  it("discards late context responses and clears old paths before the new observation resolves", async () => {
    const pending = deferred<PathInventory>(); vi.mocked(desktopApi.pathInventory).mockReturnValueOnce(pending.promise);
    const rendered = render(view());
    rendered.rerender(view({ ...state, persistent_environment: "beta" }));
    await screen.findByText("/private/desktop.root");
    await act(async () => pending.resolve(inventory({ entries: [entry("server.configuration", { configured_path: "/stale" })] })));
    expect(screen.queryByText("/stale")).not.toBeInTheDocument();
    const next = deferred<PathInventory>(); vi.mocked(desktopApi.pathInventory).mockReturnValueOnce(next.promise);
    rendered.rerender(view({ ...state, persistent_environment: "gamma" }));
    expect(screen.queryByText("/private/desktop.root")).not.toBeInTheDocument();
    await act(async () => next.resolve(inventory()));
  });
  it("discards superseded refreshes", async () => {
    render(view()); await screen.findByText("/private/desktop.root");
    const old = deferred<PathInventory>(); vi.mocked(desktopApi.pathInventory).mockReturnValueOnce(old.promise).mockResolvedValueOnce(inventory({ revision: "new", entries: [entry("server.configuration", { configured_path: "/new" })] }));
    fireEvent.click(screen.getByRole("button", { name: "Refresh locations" }));
    fireEvent.click(screen.getByRole("button", { name: "Refresh locations" }));
    await screen.findByText("/new");
    await act(async () => old.resolve(inventory()));
    expect(screen.getByText("/new")).toBeVisible();
  });
  it("abandons a native save picker when topology changes", async () => {
    const picker = deferred<string | null>(); vi.mocked(save).mockReturnValueOnce(picker.promise);
    const rendered = render(view()); await screen.findByRole("button", { name: "Export path inventory" });
    fireEvent.click(screen.getByRole("button", { name: "Export path inventory" }));
    rendered.rerender(view({ ...state, topology: { ...state.topology!, server: { kind: "remote", url: "https://example.test" } } }));
    await act(async () => picker.resolve("/old-context.json"));
    expect(desktopApi.exportInventoryDocument).not.toHaveBeenCalled();
  });
  it("renders journal and memory viewing help without invented path actions", async () => {
    vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory({ roots: [], entries: [entry("server.trace", { kind: "system_log", configured_path: null, canonical_path: null }), entry("desktop.process_output", { kind: "in_memory", configured_path: null, canonical_path: null, source: "in_memory" })] }));
    render(view()); await screen.findByText(/operating system journal/); expect(screen.getByText(/Held in process memory/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Open location" })).not.toBeInTheDocument();
  });
  it("shows the authoritative system journal unit and user scope", async () => {
    vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory({ roots: [], entries: [entry("server.lifecycle_log", { kind: "system_log", configured_path: null, canonical_path: null, log_source: { kind: "systemd_journal", unit_name: "webcodex-server-alpha.service", service_scope: "user" } })] }));
    render(view()); await screen.findByText("webcodex-server-alpha.service");
    expect(screen.getByText("User service")).toBeVisible(); expect(screen.getByText(/View with journalctl/)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Open location" })).not.toBeInTheDocument();
  });
  it.each(["inventory_truncated", "tunnel_profiles_truncated", "project_references_truncated"])("blocks both exports for incomplete %s observations without blocking navigation", async code => {
    vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory({ issues: [{ code, entry_id: null }] }));
    render(view()); await screen.findByRole("button", { name: "Export backup manifest" });
    expect(screen.getByRole("button", { name: "Export backup manifest" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export path inventory" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Refresh locations" })).toBeEnabled();
    expect(screen.getAllByRole("button", { name: "Open location" })[0]).toBeEnabled();
    expect(screen.getByText(/Some locations could not be confirmed/)).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "Export backup manifest" }));
    expect(save).not.toHaveBeenCalled(); expect(desktopApi.exportInventoryDocument).not.toHaveBeenCalled();
  });
  it("reports partial observations with safe reasons without rendering unknown issue details", async () => {
    vi.mocked(desktopApi.pathInventory).mockResolvedValue(inventory({ issues: [{ code: "inventory_truncated", entry_id: null }, { code: "private arbitrary details", entry_id: null }] }));
    render(view()); await screen.findByText(/inventory reached its size limit/);
    expect(screen.getByText(/Some locations could not be confirmed/)).toBeVisible();
    expect(screen.queryByText("private arbitrary details")).not.toBeInTheDocument();
  });
  it.each(LANGUAGES.map(language => language.value))("renders translated controls in %s", async locale => {
    localStorage.setItem("webcodex.desktop.locale", locale); render(view());
    await screen.findByRole("button", { name: configurationDataText(locale, "manifest") });
    for (const row of Object.values(CONFIGURATION_DATA_MESSAGES)) expect(row).toHaveLength(7);
    expect(screen.getByRole("heading", { name: configurationDataText(locale, "title") })).toBeVisible();
  });
});
