import { useEffect, useRef } from "react";

/** One read-only request incarnation. Never use this slot for sends or lease renewal. */
export class ObservationRequest {
  private controller: AbortController | null = null;
  private queuedRefresh: (() => void) | null = null;

  get pending(): boolean { return this.controller !== null; }

  cancel(): void {
    const controller = this.controller;
    this.controller = null;
    this.queuedRefresh = null;
    controller?.abort();
  }

  /** Explicit refreshes coalesce behind a slow read rather than canceling it. */
  requestRefresh(refresh: () => void): void {
    if (this.pending) this.queuedRefresh = refresh;
    else refresh();
  }

  async run<T>(load: (signal: AbortSignal) => Promise<T | null>, receive: (result: T | null) => void): Promise<void> {
    if (this.pending) return;
    const controller = new AbortController();
    this.controller = controller;
    const current = () => this.controller === controller && !controller.signal.aborted;
    try {
      let result: T | null;
      try { result = await load(controller.signal); }
      catch { result = null; }
      // A consumer exception is not a network failure and must not replay receive.
      if (current()) receive(result);
    } finally {
      // A late old completion may not release a newer request or revive its refresh.
      if (this.controller === controller) {
        this.controller = null;
        const refresh = this.queuedRefresh;
        this.queuedRefresh = null;
        refresh?.();
      }
    }
  }
}

export function useObservationRequest(): ObservationRequest {
  const slot = useRef<ObservationRequest | null>(null);
  slot.current ??= new ObservationRequest();
  const request = slot.current;
  useEffect(() => () => request.cancel(), [request]);
  return request;
}
