import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../lib/desktop-api";
import type { UpdateDownloadStatus, UpdateStatus } from "../models/runtime-shell";

const ACTIVE_PHASES = new Set(["checking", "downloading", "verifying", "preparing", "installing_or_handed_off"]);

export function useRuntimeUpdates(ready: boolean) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [manualError, setManualError] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionError, setActionError] = useState(false);
  const alive = useRef(true); const busy = useRef(false); const acting = useRef(false);
  const latest = useRef(status); latest.current = status;
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);

  const acceptDownload = useCallback((download: UpdateDownloadStatus) => {
    if (!alive.current || !download) return;
    setStatus(current => {
      if (!current) return current;
      // A prior-version poll cannot roll a newer discovery back. Pending
      // transactions remain visible even if an even newer release appears.
      if (download.version && !download.pending_install && current.latest?.version !== download.version) return current;
      return { ...current, download };
    });
  }, []);

  const refreshDownload = useCallback(async () => {
    try { acceptDownload(await desktopApi.updateDownloadState()); }
    catch { /* Local update observations never become Runtime health errors. */ }
  }, [acceptDownload]);

  const check = useCallback(async (manual: boolean) => {
    if (busy.current || acting.current) return;
    busy.current = true; setChecking(true); if (manual) setManualError(false);
    try {
      const next = await desktopApi.checkForUpdates(manual);
      if (alive.current) { setStatus(next); if (manual) setManualError(Boolean(next.manual_error)); }
    } catch { if (alive.current && manual) setManualError(true); }
    finally { busy.current = false; if (alive.current) setChecking(false); }
  }, []);

  useEffect(() => {
    if (!ready) return;
    const start = window.setTimeout(() => { void check(false); }, 1200);
    // The backend retains the 24-hour discovery limit and separate one-hour
    // download retry limit. This also wakes an available update after resume.
    const interval = window.setInterval(() => { void check(false); }, 15 * 60 * 1000);
    return () => { window.clearTimeout(start); window.clearInterval(interval); };
  }, [ready, check]);

  const active = Boolean(status?.download && ACTIVE_PHASES.has(status.download.phase));
  const observed = Boolean(status);
  useEffect(() => {
    if (!ready || !observed) return;
    let cancelled = false; let timer: number | undefined;
    const poll = async () => {
      await refreshDownload();
      if (!cancelled) timer = window.setTimeout(() => { void poll(); }, active ? 1000 : 5000);
    };
    void poll();
    return () => { cancelled = true; if (timer !== undefined) window.clearTimeout(timer); };
  }, [ready, observed, active, refreshDownload]);

  const action = useCallback(async (execute: () => Promise<void>) => {
    if (acting.current) return;
    acting.current = true; setActionBusy(true); setActionError(false);
    try { await execute(); }
    catch { if (alive.current) setActionError(true); }
    finally {
      await refreshDownload();
      acting.current = false; if (alive.current) setActionBusy(false);
    }
  }, [refreshDownload]);

  const download = () => action(async () => {
    const version = latest.current?.latest?.version;
    if (!version || latest.current?.download?.pending_install) return;
    const next = await desktopApi.downloadUpdate(version); if (alive.current) setStatus(next);
  });
  const cancelDownload = () => action(async () => { acceptDownload(await desktopApi.cancelUpdateDownload()); });
  const setAutomaticDownload = (enabled: boolean) => action(async () => {
    const next = await desktopApi.setAutomaticUpdateDownload(enabled); if (alive.current) setStatus(next);
  });
  const remindLater = () => action(async () => {
    const next = await desktopApi.remindUpdateLater(); if (alive.current) setStatus(next);
  });
  // Called only after the confirmation UI records the exact target displayed
  // to the user. No effect, timer, preference or download callback installs.
  const install = (version: string) => action(async () => {
    const current = latest.current?.download;
    if (!current?.can_install || current.phase !== "ready_to_install" || current.version !== version || current.pending_install) {
      throw new Error("update_changed");
    }
    await desktopApi.installVerifiedUpdate(version, true);
  });

  return { status, checking, manualError, actionBusy, actionError, check: () => check(true),
    download, cancelDownload, setAutomaticDownload, install, remindLater };
}
export type RuntimeUpdates = ReturnType<typeof useRuntimeUpdates>;
