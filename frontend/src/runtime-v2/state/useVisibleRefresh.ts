import { useEffect, useRef } from "react";

/** Shared visible-only inventory refresh. Callers own single-flight and request fences. */
export function useVisibleRefresh(enabled: boolean, refresh: () => void, intervalMs: number, onPause?: () => void) {
  const latest = useRef(refresh);
  latest.current = refresh;
  const pause = useRef(onPause);
  pause.current = onPause;
  useEffect(() => {
    if (!enabled) { pause.current?.(); return; }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const clear = () => { if (timer !== undefined) clearTimeout(timer); timer = undefined; };
    const schedule = (delay: number) => {
      clear();
      if (document.visibilityState === "hidden") return;
      timer = setTimeout(() => {
        timer = undefined;
        if (document.visibilityState !== "hidden") latest.current();
        schedule(intervalMs);
      }, delay);
    };
    const resume = () => schedule(150);
    const visibility = () => {
      if (document.visibilityState === "hidden") { clear(); pause.current?.(); }
      else resume();
    };
    schedule(intervalMs);
    window.addEventListener("focus", resume);
    window.addEventListener("online", resume);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      clear();
      window.removeEventListener("focus", resume);
      window.removeEventListener("online", resume);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, [enabled, intervalMs]);
}
