import { useCallback, useEffect, useRef, useState } from "react";
import { desktopApi } from "../lib/desktop-api";
import type { UpdateStatus } from "../models/runtime-shell";

export function useRuntimeUpdates(ready: boolean) {
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [manualError, setManualError] = useState(false);
  const started = useRef(false); const alive = useRef(true); const busy = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const check = useCallback(async (manual: boolean) => {
    if (busy.current) return;
    busy.current = true; setChecking(true); if (manual) setManualError(false);
    try { const next = await desktopApi.checkForUpdates(manual); if (alive.current) { setStatus(next); if (manual) setManualError(Boolean(next.manual_error)); } }
    catch { if (alive.current && manual) setManualError(true); }
    finally { busy.current = false; if (alive.current) setChecking(false); }
  }, []);
  useEffect(() => {
    if (!ready || started.current) return;
    const timer = window.setTimeout(() => { started.current = true; void check(false); }, 1200);
    return () => window.clearTimeout(timer);
  }, [ready, check]);
  const remindLater = async () => { try { const next = await desktopApi.remindUpdateLater(); if (alive.current) setStatus(next); } catch { /* An update preference does not change Runtime health. */ } };
  return { status, checking, manualError, check: () => check(true), remindLater };
}
export type RuntimeUpdates = ReturnType<typeof useRuntimeUpdates>;
