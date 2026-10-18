import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useRuntimeUpdates } from "./useRuntimeUpdates";
import type { LocalUpdateStatus, UpdateDownloadStatus, UpdateStatus } from "../models/runtime-shell";

const api = vi.hoisted(() => ({
  checkForUpdates: vi.fn(), localUpdateStatus: vi.fn(), setAutomaticUpdateDownload: vi.fn(),
  downloadUpdate: vi.fn(), cancelUpdateDownload: vi.fn(), installVerifiedUpdate: vi.fn(), remindUpdateLater: vi.fn(),
}));
vi.mock("../lib/desktop-api", () => ({ desktopApi: api }));

const download: UpdateDownloadStatus = {
  phase: "available", version: "0.5.0", platform: "darwin-arm64",
  downloaded_bytes: 0, total_bytes: null, error_kind: null, installation: "managed",
  can_install: false, pending_install: false, legacy_release: false, cancelled: false,
};
function status(automaticDownload: boolean, version = "0.6.0"): UpdateStatus {
  return {
    state: "update_available", latest: { version, runtime_version: version, release_url: `https://github.com/yyjeqhc/webcodex/releases/tag/v${version}`, compatibility: "runtime_compatible" },
    update_available: true, show_banner: true, cached: false, last_check_at_ms: 100,
    manual_error: null, automatic_download: automaticDownload, download: { ...download, version },
  };
}
const local: LocalUpdateStatus = {
  environment_id: "fixture-environment", selection_revision: 1, installed_observed: false,
  installed_checked_at_ms: null, observation_error: false, confirmation: null, running: [],
  view: { schema_version: 1, download, installed: [], candidate: null, candidate_components: [], upgrade: null, blockers: [], restart_required: false },
};

beforeEach(() => {
  vi.resetAllMocks(); vi.useFakeTimers();
  api.checkForUpdates.mockResolvedValue(status(true));
  api.localUpdateStatus.mockResolvedValue(local);
  api.setAutomaticUpdateDownload.mockImplementation(async (enabled: boolean) => status(enabled));
});
afterEach(() => { cleanup(); vi.useRealTimers(); });

it.each(["success", "failure"] as const)("keeps a saved automatic-download preference after an earlier check's late %s", async outcome => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await result.current.check(); });
  expect(result.current.status?.automatic_download).toBe(true);
  let finish!: (value: UpdateStatus) => void, fail!: (reason: unknown) => void;
  api.checkForUpdates.mockReturnValueOnce(new Promise<UpdateStatus>((resolve, reject) => { finish = resolve; fail = reject; }));
  let pending!: Promise<void>;
  act(() => { pending = result.current.check(); });
  expect(result.current.checking).toBe(true);
  await act(async () => { await result.current.setAutomaticDownload(false); });
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledExactlyOnceWith(false);
  expect(result.current.status?.automatic_download).toBe(false);
  expect(result.current.checking).toBe(true);
  api.checkForUpdates.mockResolvedValue(status(false));
  await act(async () => { if (outcome === "success") finish(status(true)); else fail(new Error("Earlier check failed")); await pending; });
  expect(result.current.status?.automatic_download).toBe(false);
  expect(result.current.manualError).toBe(false);
  expect(result.current.checking).toBe(false);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledTimes(1);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(3);
  expect(api.checkForUpdates).toHaveBeenLastCalledWith(false);
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
});

it("accepts a later update check after the preference action completes", async () => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await result.current.check(); });
  await act(async () => { await result.current.setAutomaticDownload(false); });
  const newer: UpdateStatus = {
    ...status(false),
    latest: { ...status(false).latest!, version: "0.7.0" },
    download: { ...download, version: "0.7.0" },
  };
  api.checkForUpdates.mockResolvedValue(newer);
  await act(async () => { await result.current.check(); });
  expect(result.current.status).toEqual(newer);
  expect(result.current.manualError).toBe(false);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(2);
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

it.each(["action", "check"] as const)("reconciles a newly discovered release when the %s finishes first", async first => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });
  const discovery = deferred<UpdateStatus>(); const preference = deferred<UpdateStatus>();
  const newer = status(false, "0.7.0");
  api.checkForUpdates.mockReturnValueOnce(discovery.promise).mockResolvedValue(newer);
  api.setAutomaticUpdateDownload.mockReturnValueOnce(preference.promise);
  api.localUpdateStatus.mockResolvedValue({ ...local, view: { ...local.view, download: newer.download } });
  let check!: Promise<void>, action!: Promise<void>;
  act(() => { check = result.current.check(); action = result.current.setAutomaticDownload(false); });
  if (first === "action") {
    await act(async () => { preference.resolve(status(false)); await action; });
    expect(api.checkForUpdates).toHaveBeenCalledTimes(2);
    await act(async () => { discovery.resolve(newer); await check; });
  } else {
    await act(async () => { discovery.resolve(newer); await check; });
    expect(api.checkForUpdates).toHaveBeenCalledTimes(2);
    await act(async () => { preference.resolve(status(false)); await action; });
  }
  expect(result.current.status).toEqual(newer);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(3);
  expect(api.checkForUpdates).toHaveBeenLastCalledWith(false);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledExactlyOnceWith(false);
  api.downloadUpdate.mockResolvedValue(newer);
  await act(async () => { await result.current.download(); });
  expect(api.downloadUpdate).toHaveBeenCalledExactlyOnceWith("0.7.0");
  expect(result.current.actionError).toBe(false);
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
});

it("coalesces multiple actions during one discovery into one follow-up", async () => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });
  const discovery = deferred<UpdateStatus>();
  api.checkForUpdates.mockReturnValueOnce(discovery.promise);
  let check!: Promise<void>;
  act(() => { check = result.current.check(); });
  await act(async () => { await result.current.setAutomaticDownload(false); });
  await act(async () => { await result.current.setAutomaticDownload(true); });
  await act(async () => { discovery.resolve(status(false)); await check; });
  expect(result.current.status?.automatic_download).toBe(true);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(3);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledTimes(2);
});

it("fences a follow-up superseded by another explicit action", async () => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });
  const discovery = deferred<UpdateStatus>(); const followUp = deferred<UpdateStatus>();
  const current = status(true, "0.7.0");
  api.checkForUpdates.mockReturnValueOnce(discovery.promise).mockReturnValueOnce(followUp.promise).mockResolvedValue(current);
  let check!: Promise<void>;
  act(() => { check = result.current.check(); });
  await act(async () => { await result.current.setAutomaticDownload(false); });
  await act(async () => { discovery.resolve(status(true)); });
  expect(api.checkForUpdates).toHaveBeenCalledTimes(3);
  await act(async () => { await result.current.setAutomaticDownload(true); });
  await act(async () => { followUp.resolve(status(false)); await check; });
  expect(result.current.status).toEqual(current);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(4);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledTimes(2);
});

it("does not loop or flash an obsolete manual error when the follow-up fails", async () => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });
  const discovery = deferred<UpdateStatus>();
  api.checkForUpdates.mockReturnValueOnce(discovery.promise).mockRejectedValue(new Error("unavailable"));
  let check!: Promise<void>;
  act(() => { check = result.current.check(); });
  await act(async () => { await result.current.setAutomaticDownload(false); });
  await act(async () => { discovery.resolve(status(true)); await check; await vi.advanceTimersByTimeAsync(60_000); });
  expect(api.checkForUpdates).toHaveBeenCalledTimes(3);
  expect(result.current.status?.automatic_download).toBe(false);
  expect(result.current.manualError).toBe(false);
  expect(result.current.checking).toBe(false);
});

it.each(["check", "action"] as const)("drops the pending follow-up if unmounted before the %s settles", async last => {
  const { result, unmount } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(1200); });
  const discovery = deferred<UpdateStatus>(); const preference = deferred<UpdateStatus>();
  api.checkForUpdates.mockReturnValueOnce(discovery.promise);
  api.setAutomaticUpdateDownload.mockReturnValueOnce(preference.promise);
  let check!: Promise<void>, action!: Promise<void>;
  act(() => { check = result.current.check(); action = result.current.setAutomaticDownload(false); });
  if (last === "check") await act(async () => { preference.resolve(status(false)); await action; });
  else await act(async () => { discovery.resolve(status(true)); await check; });
  unmount();
  await act(async () => { discovery.resolve(status(true)); preference.resolve(status(false)); await Promise.all([check, action]); });
  expect(api.checkForUpdates).toHaveBeenCalledTimes(2);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledTimes(1);
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
});
