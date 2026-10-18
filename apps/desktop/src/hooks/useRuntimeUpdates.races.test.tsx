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
function status(automaticDownload: boolean): UpdateStatus {
  return {
    state: "available", latest: { version: "0.5.0", runtime_version: "0.4.9", release_url: "https://github.com/yyjeqhc/webcodex/releases/tag/v0.5.0", compatibility: "runtime_compatible" },
    update_available: true, show_banner: true, cached: false, last_check_at_ms: 100,
    manual_error: null, automatic_download: automaticDownload, download,
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
  await act(async () => { if (outcome === "success") finish(status(true)); else fail(new Error("Earlier check failed")); await pending; });
  expect(result.current.status?.automatic_download).toBe(false);
  expect(result.current.manualError).toBe(false);
  expect(result.current.checking).toBe(false);
  expect(api.setAutomaticUpdateDownload).toHaveBeenCalledTimes(1);
  expect(api.installVerifiedUpdate).not.toHaveBeenCalled();
});

it("accepts a later update check after the preference action completes", async () => {
  const { result } = renderHook(() => useRuntimeUpdates(true));
  await act(async () => { await result.current.check(); });
  await act(async () => { await result.current.setAutomaticDownload(false); });
  const newer: UpdateStatus = {
    ...status(false),
    latest: { ...status(false).latest!, version: "0.6.0" },
    download: { ...download, version: "0.6.0" },
  };
  api.checkForUpdates.mockResolvedValue(newer);
  await act(async () => { await result.current.check(); });
  expect(result.current.status).toEqual(newer);
  expect(result.current.manualError).toBe(false);
  expect(api.checkForUpdates).toHaveBeenCalledTimes(2);
});
