import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../lib/desktop-api";
import type { LocalUpdateStatus, UpdateConfirmation, UpdateDownloadStatus, UpdateStatus } from "../models/runtime-shell";

const ACTIVE_PHASES = new Set(["checking", "downloading", "verifying", "preparing", "installing_or_handed_off"]);

export function useRuntimeUpdates(ready: boolean) {
  const [local, setLocal] = useState<LocalUpdateStatus | null>(null);
  const [localError, setLocalError] = useState(false);
  const localLatest = useRef(local); localLatest.current = local;
  const localRequest = useRef(0);
  const inspectionRequest = useRef(0);
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [manualError, setManualError] = useState(false);
  const [actionBusy, setActionBusy] = useState(false);
  const [actionError, setActionError] = useState(false);
  const alive = useRef(true); const busy = useRef(false); const acting = useRef(false);
  const latest = useRef(status); latest.current = status;
  useEffect(() => { alive.current = true; return () => { alive.current = false; ++inspectionRequest.current; }; }, []);

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

  const refreshLocal = useCallback(async (inspectFiles = false) => {
    const request = ++localRequest.current;
    // Polls may finish before an inspection, but a newer explicit inspection
    // invalidates the older file observation even for the same local target.
    const inspection = inspectFiles ? ++inspectionRequest.current : null;
    try {
      const next = await desktopApi.localUpdateStatus(inspectFiles);
      if (inspectFiles && inspection !== inspectionRequest.current) return;
      if (alive.current && next && request !== localRequest.current && inspectFiles) {
        setLocal(current => inspection === inspectionRequest.current && current && current.environment_id === next.environment_id && current.selection_revision === next.selection_revision && current.view.upgrade?.operation_id === next.view.upgrade?.operation_id && current.view.upgrade?.phase === next.view.upgrade?.phase
          ? { ...current, installed_observed: true, installed_checked_at_ms: next.installed_checked_at_ms, view: { ...current.view, installed: next.view.installed, restart_required: next.view.restart_required } } : current);
      }
      if (alive.current && request === localRequest.current && next) {
        setLocal(current => {
          const preserveFiles = !next.installed_observed && current?.environment_id === next.environment_id && current?.selection_revision === next.selection_revision && current?.view.upgrade?.phase === next.view.upgrade?.phase && current?.view.upgrade?.operation_id === next.view.upgrade?.operation_id;
          return preserveFiles ? { ...next, installed_observed: current!.installed_observed, installed_checked_at_ms: current!.installed_checked_at_ms, view: { ...next.view, installed: current!.view.installed, restart_required: current!.view.restart_required || next.view.restart_required } } : next;
        }); setLocalError(false);
        acceptDownload(next.view.download);
        return next;
      }
    } catch { if (alive.current && request === localRequest.current) { setLocalError(true); setLocal(null); } }
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
  const observed = ready;
  useEffect(() => {
    if (!ready || !observed) return;
    let cancelled = false; let timer: number | undefined;
    const poll = async () => {
      await refreshLocal();
      if (!cancelled) timer = window.setTimeout(() => { void poll(); }, active ? 1000 : 5000);
    };
    void poll();
    return () => { cancelled = true; if (timer !== undefined) window.clearTimeout(timer); };
  }, [ready, observed, active, refreshLocal]);

  const action = useCallback(async (execute: () => Promise<void>) => {
    if (acting.current) return;
    acting.current = true; setActionBusy(true); setActionError(false);
    try { await execute(); }
    catch { if (alive.current) setActionError(true); }
    finally {
      await refreshLocal();
      acting.current = false; if (alive.current) setActionBusy(false);
    }
  }, [refreshLocal]);

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
  const install = (confirmation: UpdateConfirmation) => action(async () => {
    const current = localLatest.current?.view.download;
    const version = confirmation.candidate.version;
    if (!current?.can_install || current.phase !== "ready_to_install" || current.version !== version || current.pending_install || JSON.stringify(localLatest.current?.confirmation) !== JSON.stringify(confirmation)) {
      throw new Error("update_changed");
    }
    await desktopApi.installVerifiedUpdate(version, true, confirmation);
  });

  return { status, local, localError, refreshLocal, checking, manualError, actionBusy, actionError, check: () => check(true),
    download, cancelDownload, setAutomaticDownload, install, remindLater };
}
export type RuntimeUpdates = ReturnType<typeof useRuntimeUpdates>;
