import { act, cleanup, fireEvent, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import { useRuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import type { LocalUpdateStatus, UpdateConfirmation, MachineBuildInfo, RuntimeSettings, UpdateDownloadStatus, UpdateStatus } from "../../models/runtime-shell";
import type { DesktopState } from "../../models/topology";
import { PRODUCT_LOCALES, productText } from "../../i18n/product";
import { LocaleProvider } from "../../i18n/locale";
import { AboutPanel, UpdateBanner } from "./AboutPanel";
import { UpdateWorkflow } from "./UpdateWorkflow";

const api = vi.hoisted(() => ({
  localUpdateStatus: vi.fn(), checkForUpdates: vi.fn(), updateDownloadState: vi.fn(), downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(),
  setAutomaticUpdateDownload: vi.fn(), installVerifiedUpdate: vi.fn(), remindUpdateLater: vi.fn(), openLatestRelease: vi.fn(),
  desktopBuildInfo: vi.fn(), runtimeSettings: vi.fn(), recheckRuntime: vi.fn(), openDiagnosticResource: vi.fn(),
}));
vi.mock("../../lib/desktop-api", () => ({ desktopApi: api }));

function download(overrides: Partial<UpdateDownloadStatus> = {}): UpdateDownloadStatus {
  return { phase: "available", version: "0.5.0", platform: "darwin-arm64", downloaded_bytes: 0, total_bytes: null,
    error_kind: null, installation: "managed", can_install: false, pending_install: false, legacy_release: false,
    cancelled: false, ...overrides };
}
function status(value = download()): UpdateStatus {
  return { state: "available", latest: { version: "0.5.0", runtime_version: "0.4.9", release_url: "https://github.com/yyjeqhc/webcodex/releases/tag/v0.5.0", compatibility: "runtime_compatible" },
    update_available: true, show_banner: true, cached: false, last_check_at_ms: 100, manual_error: null,
    automatic_download: true, download: value };
}
const confirmation: UpdateConfirmation = { services: [], service_inventory_complete: true, candidate: { version: "0.5.0", target: { platform: "darwin-arm64", format: "pkg" }, source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), installer_sha256: "c".repeat(64) }, target: { environment_id: "local-env", manifest_sha256: "b".repeat(64), operation_id: null }, selection_revision: 1 };
function local(value = download()): LocalUpdateStatus {
  return { environment_id: "local-env", selection_revision: 1, installed_observed: false, installed_checked_at_ms: null, observation_error: false, confirmation: value.can_install ? confirmation : null, running: [], view: { schema_version: 1, download: value, installed: [], candidate: confirmation.candidate, candidate_components: [], upgrade: null, blockers: [], restart_required: false } };
}
function updates(value = download()): RuntimeUpdates {
  return { status: status(value), local: local(value), localError: false, refreshLocal: vi.fn().mockResolvedValue(undefined), checking: false, manualError: false, actionBusy: false, actionError: false,
    check: vi.fn().mockResolvedValue(undefined), download: vi.fn().mockResolvedValue(undefined),
    cancelDownload: vi.fn().mockResolvedValue(undefined), setAutomaticDownload: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined), remindLater: vi.fn().mockResolvedValue(undefined) };
}
const wrap = (value: React.ReactNode) => <LocaleProvider>{value}</LocaleProvider>;

beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.openLatestRelease.mockResolvedValue(undefined); api.installVerifiedUpdate.mockResolvedValue(undefined);
  api.desktopBuildInfo.mockResolvedValue(null); api.runtimeSettings.mockResolvedValue(null); api.localUpdateStatus.mockImplementation(async () => local((await api.updateDownloadState()) ?? download()));
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

describe("verified update presentation", () => {
  it("shows known progress and allows cancellation but never installation", () => {
    const value = updates(download({ phase: "downloading", downloaded_bytes: 48 * 1024 * 1024, total_bytes: 77 * 1024 * 1024 }));
    render(wrap(<UpdateWorkflow updates={value} />));
    expect(screen.getByRole("progressbar")).toHaveAttribute("value", "62");
    expect(screen.getByText("62% · 48.0 MB / 77.0 MB")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel download" }));
    expect(value.cancelDownload).toHaveBeenCalledOnce(); expect(value.install).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  });

  it("keeps unknown-total progress indeterminate without a made-up percentage", () => {
    render(wrap(<UpdateWorkflow updates={updates(download({ phase: "downloading", downloaded_bytes: 1024 }))} />));
    expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
    expect(screen.getByText("1.0 KB")).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  });

  it("requires confirmation of the exact version and Desktop exit before calling Install", () => {
    const value = updates(download({ phase: "ready_to_install", can_install: true }));
    render(wrap(<UpdateWorkflow updates={value} />));
    expect(screen.getByText("Downloaded and verified")).toBeInTheDocument();
    expect(value.install).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));
    const dialog = screen.getByRole("alertdialog");
    expect(within(dialog).getByText(/Local services may stop/)).toBeInTheDocument();
    expect(value.install).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Install and close WebCodex" }));
    expect(value.install).toHaveBeenCalledExactlyOnceWith(confirmation);
  });

  it("Later and dismissed confirmation never install or cancel a verified download", () => {
    const value = updates(download({ phase: "ready_to_install", can_install: true }));
    render(wrap(<UpdateWorkflow updates={value} />));
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Later" }));
    expect(value.remindLater).toHaveBeenCalledOnce(); expect(value.install).not.toHaveBeenCalled(); expect(value.cancelDownload).not.toHaveBeenCalled();
  });

  it("invalidates confirmation when the target changes", () => {
    const value = updates(download({ phase: "ready_to_install", can_install: true }));
    const view = render(wrap(<UpdateWorkflow updates={value} />));
    fireEvent.click(screen.getByRole("button", { name: "Install update" }));
    const next = updates(download({ phase: "ready_to_install", version: "0.6.0", can_install: true }));
    next.status!.latest = { ...next.status!.latest!, version: "0.6.0" };
    view.rerender(wrap(<UpdateWorkflow updates={next} />));
    expect(screen.getByRole("button", { name: "Install and close WebCodex" })).toBeDisabled();
    expect(next.install).not.toHaveBeenCalled();
  });

  it.each(["checksum_mismatch", "source_manifest_invalid"] as const)("presents %s as a bounded recovery message", error_kind => {
    const value = updates(download({ phase: "failed", error_kind }));
    render(wrap(<UpdateWorkflow updates={value} />));
    expect(screen.getByText("Update download could not be verified. Nothing will be installed.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(value.download).toHaveBeenCalledOnce(); expect(value.install).not.toHaveBeenCalled();
  });

  it.each(["source_build", "unmanaged_installation"] as const)("explains %s rather than exposing a nonfunctional Install button", installation => {
    render(wrap(<UpdateWorkflow updates={updates(download({ phase: "ready_to_install", installation }))} />));
    expect(screen.getByText(/automatic replacement is disabled/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "View release" })).toBeInTheDocument();
  });

  it("preserves the legacy release link without treating missing installer metadata as failure", () => {
    const value = updates(download({ legacy_release: true }));
    render(wrap(<UpdateWorkflow updates={value} />));
    fireEvent.click(screen.getByRole("button", { name: "View release" }));
    expect(api.openLatestRelease).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download update" })).not.toBeInTheDocument();
  });

  it("unsupported platforms have an explanation and no download action", () => {
    render(wrap(<UpdateWorkflow updates={updates(download({ installation: "unsupported_platform", platform: null }))} />));
    expect(screen.getByText(/unavailable for this platform/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download update" })).not.toBeInTheDocument();
  });

  it("shows unresolved handoff even after Later and never claims installation success", () => {
    const value = updates(download({ phase: "installing_or_handed_off", pending_install: true }));
    value.status!.show_banner = false;
    render(wrap(<UpdateBanner updates={value} />));
    expect(screen.getByText("Installer handoff is recorded. Completion and executor activity are unconfirmed.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.queryByText(/installed successfully/i)).not.toBeInTheDocument();
  });

  it.each([false, true])("shows recovery without a pending record despite a snoozed or absent release: %s", hasRelease => {
    const value = updates(download({ phase: "failed", version: null, error_kind: "recovery_required" }));
    value.status!.update_available = false; value.status!.show_banner = false;
    if (!hasRelease) value.status!.latest = null;
    const view = render(wrap(<UpdateBanner updates={value} />));
    expect(screen.getByText(/previous installation needs attention/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download update" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remind me later" })).not.toBeInTheDocument();
    view.rerender(wrap(<AboutPanel state={{ binaries: null } as unknown as DesktopState} updates={value} />));
    expect(screen.getByText(/previous installation needs attention/)).toBeInTheDocument();
    expect(value.status!.download.pending_install).toBe(false);
  });

  it("recovery never offers installation even if a prior ready projection remains", () => {
    render(wrap(<UpdateWorkflow updates={updates(download({ phase: "ready_to_install", can_install: true, error_kind: "recovery_required" }))} />));
    expect(screen.getByText(/previous installation needs attention/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  });

  it("About exposes the existing persisted auto-download preference without opting into installation", () => {
    const value = updates();
    render(wrap(<AboutPanel state={{ binaries: null } as unknown as DesktopState} updates={value} />));
    const checkbox = screen.getByRole("checkbox", { name: "Automatically download stable updates" });
    expect(checkbox).toBeChecked(); fireEvent.click(checkbox);
    expect(value.setAutomaticDownload).toHaveBeenCalledWith(false); expect(value.install).not.toHaveBeenCalled();
  });
});

it("background discovery and verified progress never invoke the installer; only two explicit clicks do", async () => {
  vi.useFakeTimers();
  const ready = download({ phase: "ready_to_install", can_install: true, downloaded_bytes: 100, total_bytes: 100 });
  api.checkForUpdates.mockResolvedValue(status(ready)); api.updateDownloadState.mockResolvedValue(ready);
  function Harness() { const updates = useRuntimeUpdates(true); return <UpdateBanner updates={updates} />; }
  render(wrap(<Harness />));
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(api.checkForUpdates).toHaveBeenCalledExactlyOnceWith(false);
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled(); expect(api.downloadUpdate).not.toHaveBeenCalled();
  await act(async () => { await vi.advanceTimersByTimeAsync(6000); });
  expect(api.updateDownloadState).toHaveBeenCalled(); expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Install update" }));
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Install and close WebCodex" })); });
  expect(api.installVerifiedUpdate).toHaveBeenCalledExactlyOnceWith("0.5.0", true, confirmation);
});

it("reports a failed preference save when no newer release is available", async () => {
  vi.useFakeTimers();
  const current = status(download({ phase: "idle", version: null }));
  current.state = "up_to_date"; current.update_available = false; current.show_banner = false;
  api.checkForUpdates.mockResolvedValue(current); api.updateDownloadState.mockResolvedValue(current.download);
  api.setAutomaticUpdateDownload.mockRejectedValue(new Error("disk unavailable"));
  function Harness() {
    const value = useRuntimeUpdates(true);
    return <AboutPanel state={{ binaries: null } as unknown as DesktopState} updates={value} />;
  }
  render(wrap(<Harness />));
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  await act(async () => { fireEvent.click(screen.getByRole("checkbox", { name: "Automatically download stable updates" })); });
  expect(screen.getByText("The update action could not be completed. Review the update status.")).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "Automatically download stable updates" })).toBeChecked();
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
});

const aboutState = { binaries: { directory: "/fixture/runtime", version: "0.4.1", git_commit: "fixture-runtime" }, current_operation: null } as unknown as DesktopState;
const desktopBuild: MachineBuildInfo = { schema_version: 1, binary: "webcodex-desktop", version: "0.5.0", git_commit: "fixture-development", git_dirty: true, built_at: null, target: "fixture-target", architecture: "x86_64", desktop_runtime_contract: { min_generation: 1, max_generation: 1 } };
function localRuntime(compatibility: "compatible" | "incompatible" | "unknown" = "compatible"): RuntimeSettings {
  return { source: { kind: "bundled" }, selection_revision: 1, desktop_contract: desktopBuild.desktop_runtime_contract,
    selected: { candidate_id: "fixture-probe", source: { kind: "bundled" }, selection_revision: 1, checked_at_ms: 100,
      directory: "/fixture/runtime", compatibility, build_alignment: "different_version", advisories: [], error_code: null, fingerprint: "fixture-hash",
      binaries: [{ name: "webcodex-server", present: true, startup_check: "passed", metadata: { ...desktopBuild, binary: "webcodex-server", version: "0.5.1", git_dirty: false }, sha256: "fixture-hash", error_code: null, diagnostics: null }] },
    candidate: null, previous_source: null, last_switch: null, unavailable_code: null, active_jobs: 0, can_switch: true, switch_unavailable_reason: null };
}
function fact(label: string) { return screen.getByText(label, { selector: "dt" }).parentElement!; }

describe("About build, stable release and local Runtime identity", () => {
  it.each(["0.4.6", "0.5.0"])("separates the Desktop build from stable %s without claiming equality or offering a downgrade", async stableVersion => {
    api.desktopBuildInfo.mockResolvedValue(desktopBuild);
    const value = updates(download({ phase: "idle", version: null, installation: "source_build" }));
    value.status = { ...value.status!, state: "up_to_date", update_available: false, show_banner: false, latest: { ...value.status!.latest!, version: stableVersion } };
    render(wrap(<AboutPanel state={aboutState} updates={value} />));
    await waitFor(() => expect(fact("Desktop version")).toHaveTextContent("0.5.0"));
    expect(fact("Desktop revision")).toHaveTextContent("fixture-development · Dirty build");
    expect(fact("Latest stable release")).toHaveTextContent(stableVersion);
    expect(screen.getByText("No newer stable release available")).toBeInTheDocument();
    expect(screen.queryByText("Up to date")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download update" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
    expect(value.download).not.toHaveBeenCalled(); expect(value.install).not.toHaveBeenCalled();
  });

  it("preserves newer stable release actions separately from the current build", async () => {
    api.desktopBuildInfo.mockResolvedValue({ ...desktopBuild, version: "0.4.6" });
    const value = updates();
    render(wrap(<AboutPanel state={aboutState} updates={value} />));
    await waitFor(() => expect(fact("Desktop version")).toHaveTextContent("0.4.6"));
    expect(fact("Latest stable release")).toHaveTextContent("0.5.0");
    expect(screen.getByRole("button", { name: "Download update" })).toBeInTheDocument();
    expect(value.download).not.toHaveBeenCalled(); expect(value.install).not.toHaveBeenCalled();
  });

  it("uses Not checked for unobserved local files instead of the Runtime state version", async () => {
    render(wrap(<AboutPanel state={aboutState} />));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recheck Runtime" })).toBeEnabled());
    expect(fact("Local Runtime file versions")).toHaveTextContent("Not checked");
    expect(fact("Local Runtime file versions")).not.toHaveTextContent("0.4.1");
    expect(fact("Local Runtime compatibility")).toHaveTextContent("Not checked");
    expect(fact("Latest stable release")).toHaveTextContent("Not checked");
    expect(api.recheckRuntime).not.toHaveBeenCalled();
  });

  it("rechecks only on request, publishes the result and disables overlapping or operation-time probes", async () => {
    let resolve!: (value: RuntimeSettings) => void;
    api.recheckRuntime.mockImplementation(() => new Promise<RuntimeSettings>(done => { resolve = done; }));
    const view = render(wrap(<AboutPanel state={aboutState} />));
    const button = screen.getByRole("button", { name: "Recheck Runtime" });
    await waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button); fireEvent.click(button);
    expect(api.recheckRuntime).toHaveBeenCalledOnce(); expect(button).toBeDisabled();
    expect(screen.getByText("Checking local Runtime…")).toBeInTheDocument();
    await act(async () => resolve(localRuntime()));
    expect(fact("Local Runtime file versions")).toHaveTextContent("webcodex-server 0.5.1");
    expect(fact("Local Runtime compatibility")).toHaveTextContent("Compatible");
    expect(screen.getByText(/They do not identify a connected Server or Runner/)).toBeInTheDocument();
    view.rerender(wrap(<AboutPanel state={{ ...aboutState, current_operation: { kind: "runtime_switch" } } as unknown as DesktopState} />));
    expect(button).toBeDisabled(); fireEvent.click(button); expect(api.recheckRuntime).toHaveBeenCalledOnce();
  });

  it.each(["load", "recheck"])("reports %s errors without retaining a previous file version or rendering error bodies", async phase => {
    if (phase === "load") api.runtimeSettings.mockRejectedValue(new Error("private fixture error body"));
    else {
      api.runtimeSettings.mockResolvedValue(localRuntime());
      api.recheckRuntime.mockRejectedValue(new Error("private fixture error body"));
    }
    render(wrap(<AboutPanel state={aboutState} />));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recheck Runtime" })).toBeEnabled());
    if (phase === "recheck") {
      expect(fact("Local Runtime file versions")).toHaveTextContent("0.5.1");
      await act(async () => fireEvent.click(screen.getByRole("button", { name: "Recheck Runtime" })));
    }
    expect(await screen.findByRole("alert")).toHaveTextContent("Local Runtime could not be verified.");
    expect(fact("Local Runtime file versions")).toHaveTextContent("Unconfirmed");
    expect(fact("Local Runtime file versions")).not.toHaveTextContent("0.5.1");
    expect(screen.queryByText("private fixture error body")).not.toBeInTheDocument();
  });

  it.each(["incompatible", "unknown"] as const)("keeps a completed %s probe distinct from Not checked", async compatibility => {
    const probed = localRuntime(compatibility);
    probed.selected!.error_code = compatibility === "incompatible" ? "runtime_protocol_incompatible" : "webcodex_command_failed";
    if (compatibility === "unknown") probed.selected!.binaries[0] = { ...probed.selected!.binaries[0], startup_check: "failed", metadata: null, error_code: "webcodex_command_failed" };
    api.recheckRuntime.mockResolvedValue(probed);
    render(wrap(<AboutPanel state={aboutState} />));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recheck Runtime" })).toBeEnabled());
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Recheck Runtime" })));
    expect(fact("Local Runtime compatibility")).toHaveTextContent(compatibility === "incompatible" ? "Incompatible" : "Unconfirmed");
    expect(fact("Local Runtime file versions")).toHaveTextContent(compatibility === "incompatible" ? "webcodex-server 0.5.1" : "webcodex-server Unconfirmed");
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  it("keeps release check failure separate from local Runtime results", async () => {
    api.runtimeSettings.mockResolvedValue(localRuntime());
    const value = updates(); value.manualError = true;
    render(wrap(<AboutPanel state={aboutState} updates={value} />));
    await waitFor(() => expect(fact("Local Runtime compatibility")).toHaveTextContent("Compatible"));
    expect(screen.getByText("Update check unavailable; Runtime is unaffected.")).toBeInTheDocument();
    expect(fact("Latest known stable release")).toHaveTextContent("0.5.0");
    expect(screen.queryByText("No newer stable release available")).not.toBeInTheDocument();
    expect(value.install).not.toHaveBeenCalled();
  });

  it("discards a late recheck after the local Runtime changes and after unmount", async () => {
    let resolve!: (value: RuntimeSettings) => void;
    api.recheckRuntime.mockImplementation(() => new Promise<RuntimeSettings>(done => { resolve = done; }));
    const view = render(wrap(<AboutPanel state={aboutState} />));
    await waitFor(() => expect(screen.getByRole("button", { name: "Recheck Runtime" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "Recheck Runtime" }));
    view.rerender(wrap(<AboutPanel state={{ ...aboutState, binaries: { ...aboutState.binaries!, directory: "/fixture/other-runtime" } }} />));
    await waitFor(() => expect(api.runtimeSettings).toHaveBeenCalledTimes(2));
    await act(async () => resolve(localRuntime()));
    expect(fact("Local Runtime file versions")).toHaveTextContent("Not checked");
    fireEvent.click(screen.getByRole("button", { name: "Recheck Runtime" }));
    view.unmount();
    await act(async () => resolve(localRuntime()));
  });

  it.each(PRODUCT_LOCALES)("localizes build/release separation and the local check action in %s", async locale => {
    localStorage.setItem("webcodex.desktop.locale", locale);
    const value = updates(); value.status = { ...value.status!, state: "up_to_date", update_available: false };
    render(wrap(<AboutPanel state={aboutState} updates={value} />));
    expect(screen.getByText(productText(locale, "desktopBuildVersion"), { selector: "dt" })).toBeInTheDocument();
    expect(screen.getByText(productText(locale, "latestStableRelease"), { selector: "dt" })).toBeInTheDocument();
    expect(screen.getByText(productText(locale, "localRuntimeVersions"), { selector: "dt" })).toBeInTheDocument();
    expect(screen.getByText(productText(locale, "noNewerStableRelease"))).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: productText(locale, "recheckLocalRuntime") })).toBeEnabled());
  });
});

describe("local updater fences and durable outcomes", () => {
  it.each(["prepared", "stopping", "stopped", "snapshot_ready", "verifying", "committed", "restoring", "rolled_back", "recovery_required"] as const)("keeps %s visible after cache clearing", phase => {
    const value = updates(download({ phase: "idle", version: null })); value.status = { ...value.status!, latest: null, update_available: false, show_banner: false }; value.local!.view.candidate = null;
    value.local!.view.upgrade = { schema_version: 1, environment_id: "local-env", operation_id: "exact-operation", version: "0.5.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase, files: ["cli", "server", "runner", "desktop"], services: [], service_inventory_complete: true };
    render(wrap(<UpdateBanner updates={value} />));
    expect(screen.getByRole("button", { name: "Refresh local update status" })).toBeInTheDocument(); expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument(); expect(screen.queryByRole("button", { name: "Download update" })).not.toBeInTheDocument();
    if (phase === "restoring") expect(screen.getByText(/Its executor is not confirmed/)).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled();
  });
  it("shows restart after completed update with an older Desktop process", () => {
    const value = updates(download({ phase: "idle", version: null })); value.local!.view.restart_required = true; render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByText("Update completed. Restart Desktop to use the installed version.")).toBeInTheDocument(); expect(screen.queryByText(/Manual recovery/)).not.toBeInTheDocument();
  });
  it("invalidates same-version confirmation after identity changes", () => {
    const value = updates(download({ phase: "ready_to_install", can_install: true })); const view = render(wrap(<UpdateWorkflow updates={value} />)); fireEvent.click(screen.getByRole("button", { name: "Install update" }));
    value.local = { ...value.local!, confirmation: { ...confirmation, target: { ...confirmation.target, environment_id: "changed-env", operation_id: "changed-operation" }, candidate: { ...confirmation.candidate, manifest_sha256: "d".repeat(64) }, selection_revision: 2 } }; view.rerender(wrap(<UpdateWorkflow updates={value} />));
    const install = screen.getByRole("button", { name: "Install and close WebCodex" }); expect(install).toBeDisabled(); fireEvent.click(install); expect(value.install).not.toHaveBeenCalled();
  });
  it("restores focus after Escape and confines keyboard navigation", () => {
    render(wrap(<UpdateWorkflow updates={updates(download({ phase: "ready_to_install", can_install: true }))} />)); const opener = screen.getByRole("button", { name: "Install update" }); fireEvent.click(opener); const first = screen.getByRole("button", { name: "Install and close WebCodex" }); const last = screen.getByRole("button", { name: "Not now" }); expect(first).toHaveFocus(); fireEvent.keyDown(first, { key: "Tab", shiftKey: true }); expect(last).toHaveFocus(); fireEvent.keyDown(last, { key: "Tab" }); expect(first).toHaveFocus(); fireEvent.keyDown(first, { key: "Escape" }); expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument(); expect(opener).toHaveFocus();
  });
  it("refreshes blockers without automatically retrying installation", () => {
    const value = updates(download({ phase: "ready_to_install", can_install: false })); value.local!.view.blockers = ["active_tasks"]; render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByText("Waiting for active tasks to finish. Refresh status when ready.")).toBeInTheDocument(); fireEvent.click(screen.getByRole("button", { name: "Refresh local update status" })); expect(value.refreshLocal).toHaveBeenCalledOnce(); expect(value.install).not.toHaveBeenCalled(); expect(value.download).not.toHaveBeenCalled();
  });
  it("keeps all four installed, running and candidate identities separate", async () => {
    const value = updates(); const sha = "e".repeat(64); value.local!.view.installed = [{ binary: "webcodex-desktop", build: { ...desktopBuild, version: "0.5.0", git_commit: sha } }]; value.local!.running = [{ binary: "webcodex-desktop", version: "0.4.0", git_commit: "old-process", git_dirty: false, state: "observed" }, { binary: "webcodex-server", version: null, git_commit: null, git_dirty: null, state: "not_local" }]; value.local!.view.candidate_components = [{ binary: "webcodex-desktop", build: { ...desktopBuild, version: "0.6.0", git_dirty: false } }]; render(wrap(<AboutPanel state={aboutState} updates={value} />)); const table = screen.getByRole("table"); expect(within(table).getAllByRole("row")).toHaveLength(5); const row = within(table).getByRole("row", { name: /desktop 0.5.0/ }); expect(row).toHaveTextContent(sha); expect(row).toHaveTextContent("0.4.0"); expect(row).toHaveTextContent("0.6.0"); expect(within(table).getByText("Not local")).toBeInTheDocument(); fireEvent.click(screen.getByRole("button", { name: "Inspect installed files" })); expect(value.refreshLocal).toHaveBeenCalledWith(true); await act(async () => {});
  });
});

function deferredLocalStatus() {
  let resolve!: (value: LocalUpdateStatus) => void;
  const promise = new Promise<LocalUpdateStatus>(done => { resolve = done; });
  return { promise, resolve };
}

function inspectedLocalStatus(version: string, checkedAt: number, restartRequired: boolean): LocalUpdateStatus {
  const value = local();
  return { ...value, installed_observed: true, installed_checked_at_ms: checkedAt, view: {
    ...value.view, installed: [{ binary: "webcodex-desktop", build: { ...desktopBuild, version } }], restart_required: restartRequired,
    upgrade: { schema_version: 1, environment_id: "local-env", operation_id: "same-operation", version: "0.6.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase: "committed", files: ["desktop"], services: [], service_inventory_complete: true },
  } };
}

it("retains newer installed identities, inspection time and restart state after an older inspection returns", async () => {
  const older = deferredLocalStatus(); const newer = deferredLocalStatus();
  const latest = inspectedLocalStatus("0.6.0", 2000, true);
  api.localUpdateStatus.mockResolvedValueOnce(local()).mockReturnValueOnce(older.promise).mockReturnValueOnce(newer.promise);
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await waitFor(() => expect(result.current.local?.environment_id).toBe("local-env"));
  act(() => { void result.current.refreshLocal(true); void result.current.refreshLocal(true); });
  await act(async () => newer.resolve(latest));
  expect(result.current.local?.installed_checked_at_ms).toBe(2000);
  expect(result.current.local?.view.installed).toEqual(latest.view.installed);
  expect(result.current.local?.view.restart_required).toBe(true);
  await act(async () => older.resolve(inspectedLocalStatus("0.5.0", 1000, false)));
  expect(result.current.local?.installed_checked_at_ms).toBe(2000);
  expect(result.current.local?.view.installed).toEqual(latest.view.installed);
  expect(result.current.local?.view.restart_required).toBe(true);
});

it("merges a current explicit inspection after a faster ordinary status poll", async () => {
  const inspection = deferredLocalStatus(); const installed = inspectedLocalStatus("0.6.0", 2000, true);
  const poll = { ...installed, installed_observed: false, installed_checked_at_ms: null, view: { ...installed.view, installed: [], restart_required: false } };
  api.localUpdateStatus.mockResolvedValueOnce(local()).mockReturnValueOnce(inspection.promise).mockResolvedValue(poll);
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await waitFor(() => expect(result.current.local?.environment_id).toBe("local-env"));
  act(() => { void result.current.refreshLocal(true); });
  await act(async () => { await result.current.refreshLocal(); });
  expect(result.current.local?.installed_observed).toBe(false);
  await act(async () => inspection.resolve(installed));
  expect(result.current.local?.installed_checked_at_ms).toBe(2000);
  expect(result.current.local?.view.installed).toEqual(installed.view.installed);
  expect(result.current.local?.view.restart_required).toBe(true);
  await act(async () => { await result.current.refreshLocal(); });
  expect(result.current.local?.installed_checked_at_ms).toBe(2000);
  expect(result.current.local?.view.installed).toEqual(installed.view.installed);
  expect(result.current.local?.view.restart_required).toBe(true);
});

it("does not accept an inspection returned after unmount", async () => {
  const inspection = deferredLocalStatus();
  api.localUpdateStatus.mockResolvedValueOnce(local()).mockReturnValueOnce(inspection.promise);
  const { result, unmount } = renderHook(() => useRuntimeUpdates(true));
  await waitFor(() => expect(result.current.local?.environment_id).toBe("local-env"));
  let response!: Promise<LocalUpdateStatus | undefined>;
  act(() => { response = result.current.refreshLocal(true); });
  const previous = result.current.local;
  unmount();
  await act(async () => inspection.resolve(inspectedLocalStatus("0.6.0", 2000, true)));
  expect(await response).toBeUndefined();
  expect(result.current.local).toBe(previous);
});

it("drops installed-file observations returned after the selected environment changes", async () => {
  let resolve!: (value: LocalUpdateStatus) => void;
  const old = local(); const next = { ...local(), environment_id: "new-env", selection_revision: 2 };
  api.checkForUpdates.mockResolvedValue(status());
  api.localUpdateStatus.mockResolvedValueOnce(old).mockImplementationOnce(() => new Promise<LocalUpdateStatus>(done => { resolve = done; })).mockResolvedValue(next);
  function Harness() { const value = useRuntimeUpdates(true); return <><output>{value.local?.environment_id}</output><button onClick={() => void value.refreshLocal(true)}>Inspect</button><button onClick={() => void value.refreshLocal()}>Refresh observation</button></>; }
  render(wrap(<Harness />)); await screen.findByText("local-env");
  fireEvent.click(screen.getByRole("button", { name: "Inspect" }));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Refresh observation" })));
  expect(screen.getByText("new-env")).toBeInTheDocument();
  await act(async () => resolve({ ...old, installed_observed: true }));
  expect(screen.getByText("new-env")).toBeInTheDocument(); expect(screen.queryByText("local-env")).not.toBeInTheDocument();
});

it.each([
  ["en-US", "Install update", "Install and close WebCodex"],
  ["zh-CN", "安装更新", "安装并退出 WebCodex"],
  ["zh-TW", "安裝更新", "安裝並退出 WebCodex"],
  ["de-DE", "Update installieren", "Installieren und WebCodex schließen"],
  ["fr-FR", "Installer la mise à jour", "Installer et fermer WebCodex"],
  ["ja-JP", "アップデートをインストール", "インストールして WebCodex を閉じる"],
  ["ko-KR", "업데이트 설치", "설치 후 WebCodex 종료"],
])("localizes the exact installation confirmation in %s", (locale, action, confirmAction) => {
  localStorage.setItem("webcodex.desktop.locale", locale); const value = updates(download({ phase: "ready_to_install", can_install: true }));
  render(wrap(<UpdateWorkflow updates={value} />)); fireEvent.click(screen.getByRole("button", { name: action }));
  expect(screen.getByRole("button", { name: confirmAction })).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled();
});

it("allows discovery download for a newer release while retaining a terminal prior upgrade", () => {
  const value = updates(); value.status!.latest!.version = "0.6.0"; value.local!.view.candidate = null;
  value.local!.view.upgrade = { schema_version: 1, environment_id: "local-env", operation_id: "old-op", version: "0.5.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase: "committed", files: ["desktop"], services: [], service_inventory_complete: true };
  render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByRole("button", { name: "Download update" })).toBeInTheDocument(); expect(screen.getByText("Recorded upgrade · WebCodex 0.5.0")).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled();
});

it("requires explicit successful status review before repeating a restored candidate", async () => {
  const value = updates(download({ phase: "ready_to_install", can_install: true }));
  value.local!.view.upgrade = { schema_version: 1, environment_id: "local-env", operation_id: "restored-op", version: "0.5.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase: "rolled_back", files: ["desktop"], services: [], service_inventory_complete: true };
  value.refreshLocal = vi.fn().mockResolvedValue(value.local);
  render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Refresh local update status" })));
  expect(screen.getByRole("button", { name: "Install update" })).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled(); expect(value.download).not.toHaveBeenCalled();
});

it("does not hide an unknown installer handoff behind the saved snapshot stage", () => {
  const value = updates(download({ phase: "installing_or_handed_off", pending_install: true }));
  value.local!.view.upgrade = { schema_version: 1, environment_id: "local-env", operation_id: "op", version: "0.5.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase: "snapshot_ready", files: ["desktop"], services: [], service_inventory_complete: true };
  render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByText("Installer handoff is recorded. Completion and executor activity are unconfirmed.")).toBeInTheDocument(); expect(screen.queryByText("Upgrade snapshot is recorded. Installer handoff is unconfirmed.")).not.toBeInTheDocument(); expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
});

it("presents a saved preparation as a record instead of claiming an active executor", () => {
  const value = updates(download({ phase: "idle", version: null }));
  value.local!.view.upgrade = { schema_version: 1, environment_id: "local-env", operation_id: "op", version: "0.5.0", source_sha: "a".repeat(40), manifest_sha256: "b".repeat(64), phase: "prepared", files: ["desktop"], services: [], service_inventory_complete: true };
  render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByText("Upgrade preparation is recorded. Executor activity is unconfirmed.")).toBeInTheDocument(); expect(screen.queryByText("Preparing local services for replacement…")).not.toBeInTheDocument();
});

it("shows the exact local service categories and user/system scopes in confirmation", () => {
  const value = updates(download({ phase: "ready_to_install", can_install: true })); value.local!.confirmation = { ...confirmation, services: [{ component: "server", scope: "system" }, { component: "runner", scope: "user" }, { component: "tunnel", scope: "system" }] };
  render(wrap(<UpdateWorkflow updates={value} />)); fireEvent.click(screen.getByRole("button", { name: "Install update" })); const dialog = screen.getByRole("alertdialog"); expect(within(dialog).getByText("Server · System service scope")).toBeInTheDocument(); expect(within(dialog).getByText("Runner · User service scope")).toBeInTheDocument(); expect(within(dialog).getByText("Tunnel · System service scope")).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled();
});

it("keeps unknown service inventory unknown and prevents confirmation", () => {
  const value = updates(download({ phase: "ready_to_install", can_install: true })); value.local!.confirmation = { ...confirmation, service_inventory_complete: false };
  render(wrap(<UpdateWorkflow updates={value} />)); fireEvent.click(screen.getByRole("button", { name: "Install update" })); expect(screen.getByText("Local service scope is unconfirmed.")).toBeInTheDocument(); expect(screen.getByRole("button", { name: "Install and close WebCodex" })).toBeDisabled();
});

it("labels installed identities as the last explicit inspection", async () => {
  const value = updates(); value.local!.installed_observed = true; value.local!.installed_checked_at_ms = 1000;
  render(wrap(<AboutPanel state={aboutState} updates={value} />)); expect(screen.getByText("Installed file identities come from the last explicit inspection.", { exact: false })).toBeInTheDocument(); expect(document.querySelector("time")).toHaveAttribute("datetime", "1970-01-01T00:00:01.000Z"); await act(async () => {});
});

it.each(["available", "ready_to_install"] as const)("keeps a verified legacy release manual when handoff support is unavailable in %s", phase => {
  const value = updates(download({ phase, can_install: true, error_kind: null }));
  value.local!.view.blockers = ["guarded_handoff_unavailable"];
  render(wrap(<UpdateWorkflow updates={value} />));
  expect(screen.getByText("This release requires manual installation. Use the release instructions; automatic installation is unavailable.")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  const release = screen.getByRole("button", { name: "View release" }); fireEvent.click(release); expect(api.openLatestRelease).toHaveBeenCalledOnce();
  if (phase === "available") { fireEvent.click(screen.getByRole("button", { name: "Download update" })); expect(value.download).toHaveBeenCalledOnce(); }
  else expect(screen.getByText("Downloaded and verified")).toBeInTheDocument();
  expect(value.install).not.toHaveBeenCalled();
});

it.each(PRODUCT_LOCALES)("localizes unavailable automatic installation in %s", async locale => {
  const { UPDATE_TEXT } = await import("../../i18n/update-text");
  const message = "This release requires manual installation. Use the release instructions; automatic installation is unavailable.";
  localStorage.setItem("webcodex.desktop.locale", locale);
  const value = updates(download({ phase: "ready_to_install", can_install: false, error_kind: "guarded_handoff_unavailable" }));
  render(wrap(<UpdateWorkflow updates={value} />)); expect(screen.getByText(UPDATE_TEXT[locale][message])).toBeInTheDocument(); expect(value.install).not.toHaveBeenCalled();
});
