import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchSessionDetail,
  fetchSessionMessages,
  postSessionMessage,
  replaceSessionMessage,
  withdrawSessionMessage,
} from "../api/sessions.js";
import type { RuntimeV2Client } from "../api/client.js";
import type { Availability, MessagesResponse, SessionDetail } from "../model/types.js";
import { useObservationRequest } from "./useObservationRequest.js";
import { useVisibleRefresh } from "./useVisibleRefresh.js";

export type SessionLocation = {
  projectId: string;
  sessionId: string;
  runner: string;
  projectName: string;
};

export type SessionWorkspaceState = {
  detailAvailability: Availability;
  messagesAvailability: Availability;
  detail: SessionDetail | null;
  messages: MessagesResponse | null;
  sending: boolean;
  mutationNotice: string;
  mutationAllowed: boolean | null;
  send: (input: { message: string; kind?: string; priority?: string; requiresAck?: boolean; replyTo?: string }) => Promise<boolean>;
  replace: (messageId: string, message: string) => Promise<boolean>;
  withdraw: (messageId: string) => Promise<boolean>;
  refresh: () => void;
};

function sessionLocationIdentity(location: SessionLocation, authorityRevision = 0): string {
  return `${authorityRevision}\u0000${location.projectId}\u0000${location.sessionId}`;
}

export type SessionWorkspaceOptions = {
  loadMessages?: boolean;
};

export function useSessionWorkspace(
  client: RuntimeV2Client,
  enabled: boolean,
  location: SessionLocation | null,
  onUnauthorized: () => void,
  options: SessionWorkspaceOptions = {},
): SessionWorkspaceState {
  const loadMessages = options.loadMessages ?? true;
  const authorityRevision = client.authorityRevision ?? 0;
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [messagesAvailability, setMessagesAvailability] = useState<Availability>("idle");
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [messages, setMessages] = useState<MessagesResponse | null>(null);
  const [sending, setSending] = useState(false);
  const [mutationNotice, setMutationNotice] = useState("");
  const [mutationAllowed, setMutationAllowed] = useState<boolean | null>(null);
  const [detailRevision, setDetailRevision] = useState(0);
  const [messageRevision, setMessageRevision] = useState(0);
  const detailRequest = useObservationRequest();
  const messageRequest = useObservationRequest();
  const loadedLocation = useRef({ client, identity: "", pending: false });
  const projectId = location?.projectId;
  const sessionId = location?.sessionId;
  const refresh = useCallback(() => {
    detailRequest.requestRefresh(() => setDetailRevision(value => value + 1));
    if (loadMessages) messageRequest.requestRefresh(() => setMessageRevision(value => value + 1));
  }, [detailRequest, messageRequest, loadMessages]);

  useEffect(() => {
    const identity = enabled && projectId && sessionId ? `${authorityRevision}\u0000${projectId}\u0000${sessionId}` : "";
    if (loadedLocation.current.client === client && loadedLocation.current.identity === identity) return;
    loadedLocation.current = { client, identity, pending: false };
    setDetail(null); setMessages(null); setMutationNotice(""); setMutationAllowed(null); setSending(false);
    setDetailAvailability(identity ? "loading" : "idle");
    setMessagesAvailability(identity && loadMessages ? "loading" : "idle");
  }, [client, enabled, projectId, sessionId, loadMessages, authorityRevision]);

  useEffect(() => {
    detailRequest.cancel();
    if (!enabled || !projectId || !sessionId) return;
    setDetailAvailability(value => value === "idle" ? "loading" : value);
    void detailRequest.run(signal => fetchSessionDetail(client, projectId, sessionId, signal), response => {
      if (response?.status === 401) { if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized(); return; }
      if (response?.status === 403 || response?.status === 404) {
        setDetail(null); setDetailAvailability("denied"); return;
      }
      if (!response?.ok || !response.data || response.data.session_id !== sessionId) {
        setDetailAvailability(value => value === "available" || value === "stale" ? "stale" : "error"); return;
      }
      setDetail(response.data); setDetailAvailability("available");
    });
    return () => detailRequest.cancel();
  }, [client, authorityRevision, enabled, projectId, sessionId, onUnauthorized, detailRevision, detailRequest]);

  useEffect(() => {
    messageRequest.cancel();
    if (!enabled || !projectId || !sessionId || !loadMessages) {
      setMessages(null); setMessagesAvailability("idle"); return;
    }
    setMessagesAvailability(value => value === "idle" ? "loading" : value);
    void messageRequest.run(signal => fetchSessionMessages(client, projectId, sessionId, signal), response => {
      if (response?.status === 401) { if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized(); return; }
      if (response?.status === 403 || response?.status === 404) {
        setMessages(null); setMessagesAvailability("denied"); return;
      }
      if (!response?.ok || !response.data || response.data.session_id !== sessionId) {
        setMessagesAvailability(value => value === "available" || value === "stale" ? "stale" : "error"); return;
      }
      setMessages(response.data); setMessagesAvailability("available"); setMutationNotice("");
    });
    return () => messageRequest.cancel();
  }, [client, authorityRevision, enabled, loadMessages, projectId, sessionId, onUnauthorized, messageRevision, messageRequest]);

  // Transient missing/error detail remains recoverable through polling, but a
  // deterministic authority/not-found result must not become a 5-second retry loop.
  const shouldPoll = detailAvailability !== "denied"
    && (!detail || detail.lifecycle === "active" || detail.running_call || detail.running_jobs > 0);
  useVisibleRefresh(Boolean(enabled && location && shouldPoll), () => {
    if (!detailRequest.pending) setDetailRevision(value => value + 1);
    if (loadMessages && !messageRequest.pending) setMessageRevision(value => value + 1);
  }, 5_000);

  const send = useCallback(async (input: { message: string; kind?: string; priority?: string; requiresAck?: boolean; replyTo?: string }) => {
    if (!enabled || !location || !input.message.trim()) return false;
    const requestScope = loadedLocation.current;
    if (requestScope.client !== client || requestScope.identity !== sessionLocationIdentity(location, authorityRevision) || requestScope.pending) return false;
    requestScope.pending = true;
    setSending(true);
    try {
      const response = await postSessionMessage(client, {
        project: location.projectId,
        session_id: location.sessionId,
        message: input.message.trim(),
        kind: input.kind,
        priority: input.priority,
        requires_ack: input.requiresAck,
        reply_to: input.replyTo,
      });
      if (loadedLocation.current !== requestScope) return false;
      if (response?.status === 401) {
        if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized();
        return false;
      }
      if (response?.status === 0) {
        setMutationNotice("Send outcome unknown. Refresh and review retained messages before retrying.");
        return false;
      }
      if (response?.status === 403) {
        setMutationAllowed(false);
        setMutationNotice("Session collaboration access required.");
        return false;
      }
      if (!response?.ok) {
        setMutationNotice("Send failed.");
        return false;
      }
      setMutationAllowed(true);
      setMutationNotice("");
      refresh();
      return true;
    } finally {
      if (loadedLocation.current === requestScope) {
        requestScope.pending = false;
        setSending(false);
      }
    }
  }, [client, authorityRevision, enabled, location, onUnauthorized, refresh]);

  const replace = useCallback(async (messageId: string, message: string) => {
    if (!enabled || !location || !message.trim()) return false;
    const requestScope = loadedLocation.current;
    if (requestScope.client !== client || requestScope.identity !== sessionLocationIdentity(location, authorityRevision) || requestScope.pending) return false;
    requestScope.pending = true;
    setSending(true);
    try {
      const response = await replaceSessionMessage(client, location.projectId, location.sessionId, messageId, message.trim());
      if (loadedLocation.current !== requestScope) return false;
      if (response?.status === 401) {
        if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized();
        return false;
      }
      if (response?.status === 0) {
        setMutationNotice("Message mutation outcome unknown. Refresh retained messages before retrying.");
        return false;
      }
      if (response?.status === 403) {
        setMutationAllowed(false);
        setMutationNotice("Session collaboration access required.");
        return false;
      }
      if (!response?.ok) {
        setMutationNotice("Message replacement failed.");
        return false;
      }
      setMutationAllowed(true);
      setMutationNotice("");
      refresh();
      return true;
    } finally {
      if (loadedLocation.current === requestScope) {
        requestScope.pending = false;
        setSending(false);
      }
    }
  }, [client, authorityRevision, enabled, location, onUnauthorized, refresh]);

  const withdraw = useCallback(async (messageId: string) => {
    if (!enabled || !location) return false;
    const requestScope = loadedLocation.current;
    if (requestScope.client !== client || requestScope.identity !== sessionLocationIdentity(location, authorityRevision)) return false;
    const response = await withdrawSessionMessage(client, location.projectId, location.sessionId, messageId);
    if (loadedLocation.current !== requestScope) return false;
    if (response?.status === 401) {
      if ((client.authorityRevision ?? 0) === authorityRevision) onUnauthorized();
      return false;
    }
    if (response?.status === 0) {
      setMutationNotice("Message mutation outcome unknown. Refresh retained messages before retrying.");
      return false;
    }
    if (response?.status === 403) {
      setMutationAllowed(false);
      setMutationNotice("Session collaboration access required.");
      return false;
    }
    if (!response?.ok) {
      setMutationNotice("Message withdrawal failed.");
      return false;
    }
    setMutationAllowed(true);
    setMutationNotice("");
    refresh();
    return true;
  }, [client, authorityRevision, enabled, location, onUnauthorized, refresh]);

  // Effects cancel/reset the previous owner after commit. The render itself
  // must already hide that owner's evidence, including same-Session client swaps.
  const ownsLocation = Boolean(enabled && location
    && loadedLocation.current.client === client
    && loadedLocation.current.identity === sessionLocationIdentity(location, authorityRevision));
  return {
    detailAvailability: ownsLocation ? detailAvailability : enabled && location ? "loading" : "idle",
    messagesAvailability: ownsLocation && loadMessages ? messagesAvailability : enabled && location && loadMessages ? "loading" : "idle",
    detail: ownsLocation ? detail : null,
    messages: ownsLocation && loadMessages ? messages : null,
    sending: ownsLocation && sending,
    mutationNotice: ownsLocation ? mutationNotice : "",
    mutationAllowed: ownsLocation ? mutationAllowed : null,
    send,
    replace,
    withdraw,
    refresh,
  };
}
