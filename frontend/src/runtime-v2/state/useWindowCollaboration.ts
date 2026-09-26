import { useCallback, useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { fetchWindowCollaboration, postWindowCollaboration, type WindowCollaborationPost, type WindowCollaborationTranscript } from "../api/windowCollaboration.js";

export function useWindowCollaboration(client: RuntimeV2Client, windowKey: string, onUnauthorized: () => void) {
  const [transcript, setTranscript] = useState<WindowCollaborationTranscript | null>(null);
  const [error, setError] = useState(false);
  const [sending, setSending] = useState(false);
  const [uncertain, setUncertain] = useState(false);
  const pending = useRef<WindowCollaborationPost | null>(null);
  const alive = useRef(true);
  const inFlight = useRef(false);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      const response = await fetchWindowCollaboration(client, windowKey, signal);
      if (!alive.current || signal?.aborted) return;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && response.data) { setTranscript(response.data); setError(false); }
      else { setError(true); if (response?.status === 403 || response?.status === 404) setTranscript(null); }
    } catch { if (alive.current && !signal?.aborted) setError(true); }
    finally { inFlight.current = false; }
  }, [client, windowKey, onUnauthorized]);
  useEffect(() => {
    alive.current = true;
    const controller = new AbortController();
    void refresh(controller.signal);
    const timer = setInterval(() => void refresh(controller.signal), 3000);
    return () => { alive.current = false; controller.abort(); clearInterval(timer); };
  }, [refresh]);
  const send = async (message: string, sessionId: string | null) => {
    if (sending) return false;
    // Retain the entire original payload across uncertain outcomes, including context.
    pending.current ??= { client_window_key: windowKey, message, context_session_id: sessionId, delivery_key: crypto.randomUUID() };
    setSending(true);
    try {
      const response = await postWindowCollaboration(client, pending.current);
      if (!alive.current) return false;
      if (response?.status === 401) onUnauthorized();
      if (!response?.ok || !response?.data?.message_id) { setUncertain(true); return false; }
      pending.current = null;
      setUncertain(false);
      await refresh();
      return true;
    } catch { if (alive.current) setUncertain(true); return false; }
    finally { if (alive.current) setSending(false); }
  };
  return { transcript, error, sending, uncertain, send };
}
