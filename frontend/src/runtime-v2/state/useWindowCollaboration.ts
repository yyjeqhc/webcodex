import { useCallback, useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import {
  fetchWindowCollaboration,
  postWindowCollaboration,
  type WindowCollaborationKind,
  type WindowCollaborationPost,
  type WindowCollaborationPriority,
  type WindowCollaborationTranscript,
} from "../api/windowCollaboration.js";

export type WindowCollaborationSendState = "idle" | "sending" | "uncertain" | "error";
export type WindowCollaborationSendError = "conflict" | "context" | "unavailable" | "failed" | null;

function pageIsHidden(): boolean {
  return document.visibilityState === "hidden";
}

export function useWindowCollaboration(client: RuntimeV2Client, windowKey: string, onUnauthorized: () => void, active = true) {
  const [transcript, setTranscript] = useState<WindowCollaborationTranscript | null>(null);
  const [error, setError] = useState(false);
  const [sendState, setSendState] = useState<WindowCollaborationSendState>("idle");
  const [sendError, setSendError] = useState<WindowCollaborationSendError>(null);
  const pending = useRef<WindowCollaborationPost | null>(null);
  const alive = useRef(true);
  const activeRef = useRef(active);
  activeRef.current = active;
  const inFlight = useRef<AbortController | null>(null);
  const refresh = useCallback(async () => {
    if (!alive.current || !activeRef.current || pageIsHidden() || inFlight.current) return;
    const controller = new AbortController();
    const { signal } = controller;
    inFlight.current = controller;
    try {
      const response = await fetchWindowCollaboration(client, windowKey, signal);
      if (!alive.current || !activeRef.current || pageIsHidden() || signal.aborted || inFlight.current !== controller) return;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && response.data) { setTranscript(response.data); setError(false); }
      else { setError(true); if (response?.status === 403 || response?.status === 404) setTranscript(null); }
    } catch { if (alive.current && !signal.aborted) setError(true); }
    finally { if (inFlight.current === controller) inFlight.current = null; }
  }, [client, windowKey, onUnauthorized]);
  useEffect(() => {
    alive.current = true;
    // Hiding the panel pauses reads, but an outstanding send must still settle.
    return () => { alive.current = false; };
  }, []);
  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | undefined;
    const stop = () => {
      clearInterval(timer);
      timer = undefined;
      inFlight.current?.abort();
      inFlight.current = null;
    };
    const updateVisibility = () => {
      stop();
      if (!active || pageIsHidden()) return;
      void refresh();
      timer = setInterval(() => void refresh(), 3000);
    };
    updateVisibility();
    document.addEventListener("visibilitychange", updateVisibility);
    return () => {
      stop();
      document.removeEventListener("visibilitychange", updateVisibility);
    };
  }, [active, refresh]);
  const send = async (
    message: string,
    sessionId: string | null,
    kind: WindowCollaborationKind = "guidance",
    priority: WindowCollaborationPriority = "normal",
    requiresAck = true,
  ) => {
    if (sendState === "sending") return false;
    // Only an uncertain outcome retains exact retry identity. Deterministic failures clear it.
    pending.current ??= {
      client_window_key: windowKey,
      message,
      context_session_id: sessionId,
      kind,
      priority,
      requires_ack: requiresAck,
      delivery_key: crypto.randomUUID(),
    };
    setSendState("sending");
    setSendError(null);
    try {
      const response = await postWindowCollaboration(client, pending.current);
      if (!alive.current) return false;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && typeof response.data?.message_id === "string" && response.data.message_id.length > 0) {
        pending.current = null;
        setSendState("idle");
        setSendError(null);
        await refresh();
        return true;
      }
      const failureKind = response?.data?.output?.failure_kind;
      // A gateway error or missing receipt does not prove the write was rejected.
      // Retain the exact payload/key so an explicit retry can deduplicate it.
      const uncertain = !response || response.status === 0 || response.status >= 500
        || response.ok || failureKind === "outcome_unknown";
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
