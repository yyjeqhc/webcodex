export type DurableAgent = {
  agent_id: string;
  handle: string;
  display_name: string;
  description?: string | null;
  specialty_labels?: string[];
  profile_revision: number;
  current_controller_generation?: number;
  active_endpoint_count?: number;
  queued_delivery_count?: number;
  unresolved_wake_count?: number;
  latest_wake_state?: string;
  updated_at_unix_ms?: number;
};

export type AgentEndpoint = {
  endpoint_id: string;
  agent_id: string;
  wake_capable: boolean;
  controller_generation: number;
  lifecycle: string;
  attached_at_unix_ms?: number;
  last_seen_at_unix_ms?: number;
  lease_expires_at_unix_ms?: number;
};

export type DurableConversation = {
  conversation_id: string;
  title?: string | null;
  participant_count?: number;
  message_count?: number;
  last_seq?: number;
};

export type ConversationMessage = {
  message_id: string;
  seq: number;
  body: string;
  created_at_unix_ms?: number;
  reply_to?: string | null;
  author?: {
    participant_kind?: string;
    agent_id?: string | null;
    handle?: string | null;
    display_name?: string | null;
    principal_kind?: string | null;
  };
  deliveries?: Array<{
    delivery_id?: string;
    recipient_agent_id?: string;
    state?: string;
  }>;
};

export type ConversationDetail = {
  conversation?: DurableConversation;
  messages?: ConversationMessage[];
  after_seq?: number;
  truncated?: boolean;
};

export type InboxDelivery = {
  delivery_id: string;
  conversation_id?: string;
  conversation_title?: string;
  message?: ConversationMessage;
};

export function parseAgentIds(value: string): string[] {
  return Array.from(new Set(value.split(/[\s,]+/).map((item) => item.trim()).filter(Boolean)));
}

export function operationKey(prefix: string): string {
  const random = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return prefix + "-" + random;
}

export function idempotencyKeyFor(
  pending: { fingerprint: string; key: string } | null,
  fingerprint: string,
  prefix: string,
): { fingerprint: string; key: string } {
  return pending && pending.fingerprint === fingerprint
    ? pending
    : { fingerprint, key: operationKey(prefix) };
}

export function validateAgentCreate(
  handle: string,
  displayName: string,
  description: string,
  labelsRaw: string,
): { ok: true; value: { handle: string; displayName: string; description: string; labels: string[]; fingerprint: string } } | { ok: false; error: string } {
  const cleanHandle = handle.trim();
  const cleanName = displayName.trim();
  const cleanDescription = description.trim();
  const labels = parseAgentIds(labelsRaw);
  if (!cleanHandle || !cleanName) return { ok: false, error: "Handle and display name are required." };
  const fingerprint = JSON.stringify({ cleanHandle, cleanName, cleanDescription, labels });
  return { ok: true, value: { handle: cleanHandle, displayName: cleanName, description: cleanDescription, labels, fingerprint } };
}

export function validateConversationCreate(
  title: string,
  agentIdsRaw: string,
  defaultAgentId = "",
): { ok: true; value: { title: string; agentIds: string[]; fingerprint: string } } | { ok: false; error: string } {
  const cleanTitle = title.trim();
  const agentIds = parseAgentIds(agentIdsRaw || defaultAgentId);
  if (!agentIds.length) return { ok: false, error: "At least one Agent id is required." };
  const fingerprint = JSON.stringify({ cleanTitle, agentIds: [...agentIds].sort() });
  return { ok: true, value: { title: cleanTitle, agentIds, fingerprint } };
}
