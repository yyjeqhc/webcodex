import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import { useRuntimeUpdates } from "../../hooks/useRuntimeUpdates";
import type { UpdateDownloadStatus, UpdateStatus } from "../../models/runtime-shell";
import type { DesktopState } from "../../models/topology";
import { LocaleProvider } from "../../i18n/locale";
import { AboutPanel, UpdateBanner } from "./AboutPanel";
import { UpdateWorkflow } from "./UpdateWorkflow";

const api = vi.hoisted(() => ({
  checkForUpdates: vi.fn(), updateDownloadState: vi.fn(), downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(),
  setAutomaticUpdateDownload: vi.fn(), installVerifiedUpdate: vi.fn(), remindUpdateLater: vi.fn(), openLatestRelease: vi.fn(),
  desktopBuildInfo: vi.fn(), runtimeSettings: vi.fn(), openDiagnosticResource: vi.fn(),
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
