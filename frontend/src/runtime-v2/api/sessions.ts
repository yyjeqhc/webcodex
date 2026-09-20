import type {
  LocatedSession,
  MessagesResponse,
  SessionDetail,
  SessionListItem,
} from "../model/types.js";
import type { RuntimeV2Client } from "./client.js";

export function fetchProjectSessions(
  client: RuntimeV2Client,
  project: string,
  signal?: AbortSignal,
) {
  return client.post<{ sessions: SessionListItem[]; total: number; returned: number; truncated: boolean }>(
    "workflow-sessions",
    { project },
    signal,
  );
}

export function locateSession(client: RuntimeV2Client, sessionId: string, signal?: AbortSignal) {
  return client.post<LocatedSession>("workflow-session-locate", { session_id: sessionId }, signal);
}

export function fetchSessionDetail(
  client: RuntimeV2Client,
  project: string,
  sessionId: string,
  signal?: AbortSignal,
  activityLimit?: number,
) {
  return client.post<SessionDetail>(
    "workflow-session",
    { project, session_id: sessionId, ...(activityLimit !== undefined ? { limit: activityLimit } : {}) },
    signal,
  );
}

export function fetchSessionMessages(
  client: RuntimeV2Client,
  project: string,
  sessionId: string,
  signal?: AbortSignal,
) {
  return client.post<MessagesResponse>(
    "workflow-session-messages",
    { project, session_id: sessionId, limit: 100 },
    signal,
  );
}

export function postSessionMessage(
  client: RuntimeV2Client,
  input: {
    project: string;
    session_id: string;
    message: string;
    kind?: string;
    priority?: string;
    requires_ack?: boolean;
    reply_to?: string;
  },
  signal?: AbortSignal,
) {
  return client.post(
    "workflow-session-post-message",
    {
      project: input.project,
      session_id: input.session_id,
      message: input.message,
      kind: input.kind || "note",
      priority: input.priority || "normal",
      requires_ack: Boolean(input.requires_ack),
      ...(input.reply_to ? { reply_to: input.reply_to } : {}),
    },
    signal,
  );
}

export function replaceSessionMessage(
  client: RuntimeV2Client,
  project: string,
  sessionId: string,
  messageId: string,
  message: string,
  signal?: AbortSignal,
) {
  return client.post("workflow-session-replace-message", {
    project,
    session_id: sessionId,
    message_id: messageId,
    message,
  }, signal);
}

export function withdrawSessionMessage(
  client: RuntimeV2Client,
  project: string,
  sessionId: string,
  messageId: string,
  signal?: AbortSignal,
) {
  return client.post("workflow-session-withdraw-message", {
    project,
    session_id: sessionId,
    message_id: messageId,
  }, signal);
}
