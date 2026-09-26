import type { RuntimeV2Client } from "./client.js";

export type WindowCollaborationMessage = {
  message_id: string;
  source: "operator" | "peer";
  direction: "inbound" | "outbound";
  peer_id?: string;
  message: string;
  created_at_ms: number;
  context_session_id?: string;
  context_project?: string;
  requires_ack: boolean;
  first_projected_at_ms: number | null;
  first_ack_observed_at_ms: number | null;
};
export type WindowCollaborationTranscript = {
  available: boolean;
  can_send: boolean;
  messages: WindowCollaborationMessage[];
  truncated: boolean;
};
export type WindowCollaborationPost = {
  client_window_key: string;
  message: string;
  delivery_key: string;
  context_session_id: string | null;
};
export type WindowCollaborationPostResult = {
  message_id?: string;
  success?: boolean;
  error?: string;
  output?: {
    failure_kind?: string;
    error_kind?: string;
    state_changed?: boolean | null;
    retry_same_delivery?: boolean;
  };
};
export const fetchWindowCollaboration = (client: RuntimeV2Client, key: string, signal?: AbortSignal) =>
  client.post<WindowCollaborationTranscript>("window-collaboration", { client_window_key: key, limit: 50 }, signal);
export const postWindowCollaboration = (client: RuntimeV2Client, payload: WindowCollaborationPost) =>
  client.post<WindowCollaborationPostResult>("window-collaboration-post", payload);
