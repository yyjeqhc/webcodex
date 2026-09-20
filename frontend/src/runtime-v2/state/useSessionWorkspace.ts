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

export function useSessionWorkspace(
  client: RuntimeV2Client,
  enabled: boolean,
  location: SessionLocation | null,
  onUnauthorized: () => void,
): SessionWorkspaceState {
  const [detailAvailability, setDetailAvailability] = useState<Availability>("idle");
  const [messagesAvailability, setMessagesAvailability] = useState<Availability>("idle");
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [messages, setMessages] = useState<MessagesResponse | null>(null);
  const [sending, setSending] = useState(false);
  const [mutationNotice, setMutationNotice] = useState("");
  const [mutationAllowed, setMutationAllowed] = useState<boolean | null>(null);
  const [revision, setRevision] = useState(0);
  const detailRequest = useRef<AbortController | null>(null);
  const messageRequest = useRef<AbortController | null>(null);
  const refresh = useCallback(() => setRevision((value) => value + 1), []);

  useEffect(() => {
    detailRequest.current?.abort();
    messageRequest.current?.abort();
    if (!enabled || !location) {
      setDetail(null);
      setMessages(null);
      setDetailAvailability("idle");
      setMessagesAvailability("idle");
      setMutationNotice("");
      setMutationAllowed(null);
      return;
    }

    const detailController = new AbortController();
    const messageController = new AbortController();
    detailRequest.current = detailController;
    messageRequest.current = messageController;
    setDetailAvailability((value) => (value === "idle" ? "loading" : value));
    setMessagesAvailability((value) => (value === "idle" ? "loading" : value));

    void fetchSessionDetail(client, location.projectId, location.sessionId, detailController.signal).then((response) => {
      if (detailRequest.current !== detailController || !response) return;
      detailRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403 || response.status === 404) {
        setDetail(null);
        setDetailAvailability("denied");
        return;
      }
      if (!response.ok || !response.data || response.data.session_id !== location.sessionId) {
        setDetailAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      setDetail(response.data);
      setDetailAvailability("available");
    });

    void fetchSessionMessages(client, location.projectId, location.sessionId, messageController.signal).then((response) => {
      if (messageRequest.current !== messageController || !response) return;
      messageRequest.current = null;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403 || response.status === 404) {
        setMessages(null);
        setMessagesAvailability("denied");
        return;
      }
      if (!response.ok || !response.data || response.data.session_id !== location.sessionId) {
        setMessagesAvailability((current) => current === "available" || current === "stale" ? "stale" : "error");
        return;
      }
      setMessages(response.data);
      setMessagesAvailability("available");
      setMutationNotice("");
    });

    return () => {
      detailController.abort();
      messageController.abort();
    };
  }, [client, enabled, location?.projectId, location?.sessionId, onUnauthorized, revision]);

  useEffect(() => {
    if (!enabled || !location || !detail) return;
    const shouldPoll = detail.lifecycle === "active" || detail.running_call || detail.running_jobs > 0;
    if (!shouldPoll) return;
    const timer = window.setInterval(refresh, 5_000);
    return () => window.clearInterval(timer);
  }, [detail, enabled, location, refresh]);

  const send = useCallback(async (input: { message: string; kind?: string; priority?: string; requiresAck?: boolean; replyTo?: string }) => {
    if (!location || !input.message.trim()) return false;
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
      if (response?.status === 401) {
        onUnauthorized();
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
      setSending(false);
    }
  }, [client, location, onUnauthorized, refresh]);

  const replace = useCallback(async (messageId: string, message: string) => {
    if (!location || !message.trim()) return false;
    const response = await replaceSessionMessage(client, location.projectId, location.sessionId, messageId, message.trim());
    if (response?.status === 401) {
      onUnauthorized();
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
  }, [client, location, onUnauthorized, refresh]);

  const withdraw = useCallback(async (messageId: string) => {
    if (!location) return false;
    const response = await withdrawSessionMessage(client, location.projectId, location.sessionId, messageId);
    if (response?.status === 401) {
      onUnauthorized();
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
  }, [client, location, onUnauthorized, refresh]);

  return {
    detailAvailability,
    messagesAvailability,
    detail,
    messages,
    sending,
    mutationNotice,
    mutationAllowed,
    send,
    replace,
    withdraw,
    refresh,
  };
}
