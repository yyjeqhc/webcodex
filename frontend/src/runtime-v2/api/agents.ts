import type { RuntimeV2Client } from "./client.js";
import type {
  AgentEndpoint,
  ConversationDetail,
  DurableAgent,
  DurableConversation,
  InboxDelivery,
} from "../model/agents.js";

export function fetchAgents(client: RuntimeV2Client, signal?: AbortSignal) {
  return client.post<{ agents: DurableAgent[]; total?: number; returned?: number }>(
    "communication/agents",
    { offset: 0, limit: 100 },
    signal,
  );
}

export function createAgent(
  client: RuntimeV2Client,
  input: {
    handle: string;
    display_name: string;
    description?: string | null;
    specialty_labels: string[];
    idempotency_key: string;
  },
  signal?: AbortSignal,
) {
  return client.post<{ agent?: DurableAgent; replayed?: boolean }>("communication/agent/create", input, signal);
}

export function updateAgent(
  client: RuntimeV2Client,
  input: {
    agent_id: string;
    expected_profile_revision: number;
    handle?: string;
    display_name?: string;
    description?: string | null;
    specialty_labels?: string[];
  },
  signal?: AbortSignal,
) {
  return client.post<DurableAgent>("communication/agent/update", input, signal);
}

export function attachEndpoint(
  client: RuntimeV2Client,
  input: {
    agent_id: string;
    host: string;
    client_attachment_id?: string;
    idempotency_key: string;
  },
  signal?: AbortSignal,
) {
  return client.post<{ endpoint?: AgentEndpoint; replayed?: boolean }>("communication/endpoint/attach", input, signal);
}

export function renewEndpoint(
  client: RuntimeV2Client,
  endpointId: string,
  generation: number,
  signal?: AbortSignal,
) {
  return client.post<{ endpoint?: AgentEndpoint }>(
    "communication/endpoint/renew",
    { endpoint_id: endpointId, expected_controller_generation: generation },
    signal,
  );
}

export function detachEndpoint(client: RuntimeV2Client, endpointId: string, signal?: AbortSignal) {
  return client.post("communication/endpoint/detach", { endpoint_id: endpointId }, signal);
}

export function fetchConversations(client: RuntimeV2Client, signal?: AbortSignal) {
  return client.post<{ conversations: DurableConversation[]; total?: number; returned?: number }>(
    "communication/conversations",
    { offset: 0, limit: 100 },
    signal,
  );
}

export function fetchConversation(
  client: RuntimeV2Client,
  conversationId: string,
  afterSeq = 0,
  signal?: AbortSignal,
) {
  return client.post<ConversationDetail>(
    "communication/conversation",
    { conversation_id: conversationId, after_seq: Math.max(0, afterSeq), limit: 100 },
    signal,
  );
}

export function createConversation(
  client: RuntimeV2Client,
  input: { title?: string | null; agent_ids: string[]; idempotency_key: string },
  signal?: AbortSignal,
) {
  return client.post<{ conversation?: { conversation?: DurableConversation }; replayed?: boolean }>(
    "communication/conversation/create",
    input,
    signal,
  );
}

export function postConversationMessage(
  client: RuntimeV2Client,
  input: {
    conversation_id: string;
    body: string;
    author_agent_id?: string | null;
    endpoint_id?: string | null;
    expected_controller_generation?: number | null;
    recipient_agent_ids?: string[] | null;
    reply_to?: string | null;
    idempotency_key?: string | null;
  },
  signal?: AbortSignal,
) {
  return client.post<{ message?: unknown; replayed?: boolean }>("communication/message/post", input, signal);
}

export function fetchInbox(
  client: RuntimeV2Client,
  agentId: string,
  endpoint: AgentEndpoint,
  signal?: AbortSignal,
) {
  return client.post<{ deliveries: InboxDelivery[] }>(
    "communication/inbox",
    {
      agent_id: agentId,
      endpoint_id: endpoint.endpoint_id,
      expected_controller_generation: endpoint.controller_generation,
      after_delivery_order: 0,
      limit: 100,
    },
    signal,
  );
}

export function consumeInbox(
  client: RuntimeV2Client,
  agentId: string,
  endpoint: AgentEndpoint,
  deliveryIds: string[],
  signal?: AbortSignal,
) {
  return client.post(
    "communication/inbox/consume",
    {
      agent_id: agentId,
      endpoint_id: endpoint.endpoint_id,
      expected_controller_generation: endpoint.controller_generation,
      delivery_ids: deliveryIds,
    },
    signal,
  );
}
