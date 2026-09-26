import { useCallback, useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { fetchWindowCollaboration, postWindowCollaboration, type WindowCollaborationPost, type WindowCollaborationTranscript } from "../api/windowCollaboration.js";

export type WindowCollaborationSendState = "idle" | "sending" | "uncertain" | "error";
export type WindowCollaborationSendError = "conflict" | "context" | "unavailable" | "failed" | null;

export function useWindowCollaboration(client: RuntimeV2Client, windowKey: string, onUnauthorized: () => void) {
  const [transcript, setTranscript] = useState<WindowCollaborationTranscript | null>(null);
  const [error, setError] = useState(false);
  const [sendState, setSendState] = useState<WindowCollaborationSendState>("idle");
  const [sendError, setSendError] = useState<WindowCollaborationSendError>(null);
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
    if (sendState === "sending") return false;
    // Only an uncertain outcome retains exact retry identity. Deterministic failures clear it.
    pending.current ??= { client_window_key: windowKey, message, context_session_id: sessionId, delivery_key: crypto.randomUUID() };
    setSendState("sending");
    setSendError(null);
    try {
      const response = await postWindowCollaboration(client, pending.current);
      if (!alive.current) return false;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && response.data?.message_id) {
        pending.current = null;
        setSendState("idle");
        setSendError(null);
        await refresh();
        return true;
      }
      const failureKind = response?.data?.output?.failure_kind;
      const uncertain = response?.status === 0 || response?.status === 503 || failureKind === "outcome_unknown";
      if (uncertain) {
        setSendState("uncertain");
        setSendError(null);
        return false;
      }
      pending.current = null;
      setSendState("error");
      if (response?.status === 409 || failureKind === "conflict") setSendError("conflict");
      else if (failureKind === "invalid_context") setSendError("context");
      else if (response?.status === 401 || response?.status === 403 || response?.status === 404 || failureKind === "unavailable") {
        setSendError("unavailable");
        if (response?.status === 403 || response?.status === 404) setTranscript(null);
      } else setSendError("failed");
      return false;
    } catch {
      if (alive.current) {
        setSendState("uncertain");
        setSendError(null);
      }
      return false;
    }
  };
  return { transcript, error, sendState, sendError, send };
}
