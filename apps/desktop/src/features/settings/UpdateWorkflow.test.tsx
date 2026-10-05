import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import { useRuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import type { MachineBuildInfo, RuntimeSettings, UpdateDownloadStatus, UpdateStatus } from "../../models/runtime-shell";
import type { DesktopState } from "../../models/topology";
import { PRODUCT_LOCALES, productText } from "../../i18n/product";
import { LocaleProvider } from "../../i18n/locale";
import { AboutPanel, UpdateBanner } from "./AboutPanel";
import { UpdateWorkflow } from "./UpdateWorkflow";

const api = vi.hoisted(() => ({
  checkForUpdates: vi.fn(), updateDownloadState: vi.fn(), downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(),
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
function updates(value = download()): RuntimeUpdates {
  return { status: status(value), checking: false, manualError: false, actionBusy: false, actionError: false,
    check: vi.fn().mockResolvedValue(undefined), download: vi.fn().mockResolvedValue(undefined),
    cancelDownload: vi.fn().mockResolvedValue(undefined), setAutomaticDownload: vi.fn().mockResolvedValue(undefined),
    install: vi.fn().mockResolvedValue(undefined), remindLater: vi.fn().mockResolvedValue(undefined) };
}
const wrap = (value: React.ReactNode) => <LocaleProvider>{value}</LocaleProvider>;

beforeEach(() => {
  vi.resetAllMocks(); localStorage.setItem("webcodex.desktop.locale", "en-US");
  api.openLatestRelease.mockResolvedValue(undefined); api.installVerifiedUpdate.mockResolvedValue(undefined);
  api.desktopBuildInfo.mockResolvedValue(null); api.runtimeSettings.mockResolvedValue(null);
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
    const confirmation = screen.getByRole("alertdialog");
    expect(within(confirmation).getByText(/Local services may stop/)).toBeInTheDocument();
    expect(value.install).not.toHaveBeenCalled();
    fireEvent.click(within(confirmation).getByRole("button", { name: "Install and close WebCodex" }));
    expect(value.install).toHaveBeenCalledExactlyOnceWith("0.5.0");
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
    expect(screen.getByText("The system installer is running. Installation is not yet confirmed.")).toBeInTheDocument();
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
  expect(api.installVerifiedUpdate).toHaveBeenCalledExactlyOnceWith("0.5.0", true);
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
