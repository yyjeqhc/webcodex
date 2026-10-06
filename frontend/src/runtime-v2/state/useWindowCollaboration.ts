import { useCallback, useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";
import {
  fetchWindowCollaboration, postWindowCollaboration,
  type WindowCollaborationKind, type WindowCollaborationPost,
  type WindowCollaborationPriority, type WindowCollaborationTranscript,
} from "../api/windowCollaboration.js";

export type WindowCollaborationSendState = "idle" | "sending" | "uncertain" | "error";
export type WindowCollaborationSendError = "conflict" | "context" | "unavailable" | "failed" | null;
export type WindowCollaborationReadError = "access" | "unavailable" | "failed" | null;
const MAX_VISIBLE_MESSAGES = 200;
function pageIsHidden(): boolean { return document.visibilityState === "hidden"; }

export function useWindowCollaboration(client: RuntimeV2Client, windowKey: string, onUnauthorized: () => void, active = true) {
  const [transcript, setTranscript] = useState<WindowCollaborationTranscript | null>(null);
  const transcriptRef = useRef(transcript);
  const [readError, setReadError] = useState<WindowCollaborationReadError>(null);
  const [sendState, setSendState] = useState<WindowCollaborationSendState>("idle");
  const [sendError, setSendError] = useState<WindowCollaborationSendError>(null);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [browsingHistory, setBrowsingHistory] = useState(false);
  const history = useRef(false);
  const pending = useRef<WindowCollaborationPost | null>(null);
  const alive = useRef(true);
  const activeRef = useRef(active); activeRef.current = active;
  const target = useRef({ client, windowKey });
  if (target.current.client !== client || target.current.windowKey !== windowKey) target.current = { client, windowKey };
  const inFlight = useRef<AbortController | null>(null);
  const replace = useCallback((value: WindowCollaborationTranscript | null) => {
    transcriptRef.current = value; setTranscript(value);
  }, []);
  const cancelRead = useCallback(() => {
    inFlight.current?.abort(); inFlight.current = null;
  }, []);

  const read = useCallback(async (before?: string) => {
    if (!alive.current || !activeRef.current || pageIsHidden() || inFlight.current) return;
    const controller = new AbortController();
    const requestTarget = target.current;
    inFlight.current = controller;
    if (before) setLoadingHistory(true);
    try {
      const response = await fetchWindowCollaboration(client, windowKey, controller.signal, before);
      if (!alive.current || !activeRef.current || pageIsHidden() || controller.signal.aborted
          || inFlight.current !== controller || target.current !== requestTarget) return;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && response.data?.available && Array.isArray(response.data.messages)) {
        const page = response.data;
        const previous = transcriptRef.current;
        // A new recipient principal is a new history namespace, never a merge.
        if (before && previous?.history_scope !== page.history_scope) {
          replace(null); history.current = false; setBrowsingHistory(false);
          setReadError("unavailable"); return;
        }
        if (before && previous) {
          const rows = new Map([...previous.messages, ...page.messages].map(row => [row.message_id, row]));
          const messages = [...rows.values()].sort((a, b) => a.created_at_ms - b.created_at_ms || (a.message_id < b.message_id ? -1 : a.message_id > b.message_id ? 1 : 0));
          // Explicit history navigation keeps the oldest visible edge; newer rows
          // may leave this bounded DOM window, but remain accessible via Latest.
          replace({ ...page, messages: messages.slice(0, MAX_VISIBLE_MESSAGES) });
          history.current = true; setBrowsingHistory(true);
        } else if (previous && previous.history_scope === page.history_scope) {
          const rows = new Map([...previous.messages, ...page.messages].map(row => [row.message_id, row]));
          const messages = [...rows.values()].sort((a, b) => a.created_at_ms - b.created_at_ms || (a.message_id < b.message_id ? -1 : a.message_id > b.message_id ? 1 : 0));
          const clipped = messages.length > MAX_VISIBLE_MESSAGES;
          const visible = messages.slice(-MAX_VISIBLE_MESSAGES);
          const truncated = clipped || previous.truncated || page.truncated;
          // A head refresh may skip an entire page while this view was hidden.
          // Keep the unvisited coverage edge, not simply the oldest cached row.
          const previousIds = new Set(previous.messages.map(row => row.message_id));
          const overlaps = page.messages.some(row => previousIds.has(row.message_id));
          const boundary = !overlaps || clipped ? page.next_before || page.messages[0]?.message_id
            : previous.next_before || visible[0]?.message_id;
          replace({ ...page, messages: visible, truncated,
            next_before: truncated ? boundary : null });
        } else replace(page);
        setReadError(null);
      } else {
        setReadError(response?.status === 403 ? "access" : response?.status === 404 ? "unavailable" : "failed");
        if ([401, 403, 404].includes(response?.status || 0) || response?.data?.available === false) replace(null);
      }
    } catch { if (alive.current && !controller.signal.aborted && target.current === requestTarget) setReadError("failed"); }
    finally {
      if (inFlight.current === controller) { inFlight.current = null; setLoadingHistory(false); }
    }
  }, [client, windowKey, onUnauthorized, replace]);
  const refresh = useCallback(() => {
    // History remains pinned; no receipt or newer head page erases the reader's page.
    if (!history.current) void read();
  }, [read]);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);
  useEffect(() => {
    cancelRead(); replace(null); history.current = false; setBrowsingHistory(false);
    pending.current = null; setSendState("idle"); setSendError(null); setLoadingHistory(false);
    void read();
    return cancelRead;
  }, [client, windowKey]); // Target changes invalidate reads and uncertain send identity together.
  useVisibleRefresh(active, refresh, 3000, cancelRead);
  useEffect(() => { refresh(); return cancelRead; }, [active, refresh, cancelRead]);

  const loadOlder = async () => {
    const page = transcriptRef.current;
    const before = page?.next_before || (page?.truncated ? page.messages[0]?.message_id : undefined);
    if (!before || loadingHistory) return;
    cancelRead(); await read(before);
  };
  const latest = () => {
    cancelRead(); history.current = false; setBrowsingHistory(false); setLoadingHistory(false);
    void read();
  };
  const send = async (message: string, sessionId: string | null, kind: WindowCollaborationKind = "guidance",
    priority: WindowCollaborationPriority = "normal", requiresAck = true) => {
    if (sendState === "sending") return false;
    const requestTarget = target.current;
    pending.current ??= { client_window_key: windowKey, message, context_session_id: sessionId,
      kind, priority, requires_ack: requiresAck, delivery_key: crypto.randomUUID() };
    setSendState("sending"); setSendError(null);
    try {
      const response = await postWindowCollaboration(client, pending.current);
      if (!alive.current || target.current !== requestTarget) return false;
      if (response?.status === 401) onUnauthorized();
      if (response?.ok && typeof response.data?.message_id === "string" && response.data.message_id.length > 0) {
        pending.current = null; setSendState("idle"); setSendError(null);
        latest(); return true;
      }
      const failureKind = response?.data?.output?.failure_kind;
      const uncertain = !response || response.status === 0 || response.status >= 500
        || response.ok || failureKind === "outcome_unknown";
      if (uncertain) { setSendState("uncertain"); setSendError(null); return false; }
      pending.current = null; setSendState("error");
      if (response?.status === 409 || failureKind === "conflict") setSendError("conflict");
      else if (failureKind === "invalid_context") setSendError("context");
      else if ([401, 403, 404].includes(response?.status || 0) || failureKind === "unavailable") {
        setSendError("unavailable"); replace(null);
      } else setSendError("failed");
      return false;
    } catch {
      if (alive.current && target.current === requestTarget) { setSendState("uncertain"); setSendError(null); }
      return false;
    }
  };
  return { transcript, error: readError !== null, readError, sendState, sendError, send,
    loadOlder, latest, loadingHistory, browsingHistory };
}
