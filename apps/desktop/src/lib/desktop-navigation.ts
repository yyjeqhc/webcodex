import { listen } from "@tauri-apps/api/event";
import { desktopApi } from "./desktop-api";

export type NativeNavigationTarget = "activity" | "settings" | "connections";
export interface NativeNavigationIntent { sequence: number; target: NativeNavigationTarget }

function isTarget(value: unknown): value is NativeNavigationTarget {
  return value === "activity" || value === "settings" || value === "connections";
}

function isIntent(value: unknown): value is NativeNavigationIntent {
  if (!value || typeof value !== "object") return false;
  const intent = value as Partial<NativeNavigationIntent>;
  return typeof intent.sequence === "number" && Number.isInteger(intent.sequence)
    && intent.sequence > 0 && intent.sequence <= 0xffff_ffff && isTarget(intent.target);
}

/** Subscribe before reading the bounded native slot. Events only wake this
 * single-flight consumer; ACK follows application, never merely IPC delivery.
 * Disposed/StrictMode mounts leave an unacknowledged intent for the next mount.
 */
export function subscribeDesktopNavigation(apply: (target: NativeNavigationTarget) => void): () => void {
  let disposed = false;
  let stop: (() => void) | undefined;
  let reading = false;
  let requested = false;
  let lastApplied = 0;
  const wake = () => {
    requested = true;
    if (reading || disposed) return;
    reading = true;
    void (async () => {
      try {
        while (requested && !disposed) {
          requested = false;
          const intent = await desktopApi.readDesktopNavigation();
          if (disposed || !isIntent(intent)) continue;
          if (intent.sequence > lastApplied) {
            apply(intent.target);
            lastApplied = intent.sequence;
          }
          await desktopApi.acknowledgeDesktopNavigation(intent.sequence);
        }
      } catch {
        // Keep native intent retained on delivery failure. A later native event
        // or new mount may retry; no sleeps, timers, or unbounded retry loop.
      } finally { reading = false; }
    })();
  };
  void listen<unknown>("desktop:navigate", event => {
    if (isTarget(event.payload)) wake();
  }).then(unlisten => {
    if (disposed) unlisten();
    else { stop = unlisten; wake(); }
  }).catch(() => {
    // Existing in-window navigation does not depend on the optional native bridge.
  });
  return () => { disposed = true; stop?.(); };
}
