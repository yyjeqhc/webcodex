import { useCallback, useRef, useState } from "react";
import { useObservationRequest } from "./useObservationRequest.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

/** Inventory observations share lifecycle rules, never mutation/replay state. */
export function useInventoryRefresh(enabled: boolean, intervalMs: number, shouldPoll = true) {
  const request = useObservationRequest();
  const [revision, setRevision] = useState(0);
  const active = useRef(enabled);
  active.current = enabled;
  const interval = useRef(intervalMs);
  interval.current = intervalMs;
  const retry = useRef({ failures: 0, after: 0 });
  const advance = useCallback(() => {
    if (active.current && document.visibilityState !== "hidden") setRevision(value => value + 1);
  }, []);
  // Explicit refreshes are not lost behind a slow response. The slot keeps
  // only one follow-up and cancels it with the old authority/filter incarnation.
  const refresh = useCallback(() => {
    if (active.current) request.requestRefresh(advance);
  }, [advance, request]);
  const invalidate = useCallback(() => {
    if (!active.current || Date.now() < retry.current.after) return;
    request.requestRefresh(() => {
      // Generation changes and resume events are automatic observations. A
      // failure/denial received while they were queued must still stop them.
      // They may not replace an already queued deliberate user refresh.
      if (Date.now() >= retry.current.after) advance();
    }, false);
  }, [advance, request]);
  const reset = useCallback(() => { retry.current = { failures: 0, after: 0 }; }, []);
  const stopAutomatic = useCallback(() => { retry.current.after = Infinity; }, []);
  const failed = useCallback(() => {
    const failures = Math.min(8, retry.current.failures + 1);
    retry.current = { failures, after: Date.now() + Math.min(120_000, interval.current * 2 ** failures) };
  }, []);
  useVisibleRefresh(enabled && shouldPoll, () => {
    if (!request.pending && Date.now() >= retry.current.after) advance();
  }, intervalMs, undefined, invalidate);
  return { request, revision, refresh, invalidate, reset, failed, stopAutomatic };
}
