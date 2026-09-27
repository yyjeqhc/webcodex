import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  attachEndpoint,
  consumeInbox,
  createAgent,
  createConversation,
  detachEndpoint,
  fetchAgents,
  fetchConversation,
  fetchConversations,
  fetchInbox,
  postConversationMessage,
  renewEndpoint,
  updateAgent,
} from "../api/agents.js";
import type { RuntimeV2Client } from "../api/client.js";
import {
  idempotencyKeyFor,
  operationKey,
  parseAgentIds,
  validateAgentCreate,
  validateConversationCreate,
  type AgentEndpoint,
  type ConversationDetail,
  type DurableAgent,
  type DurableConversation,
  type InboxDelivery,
} from "../model/agents.js";

type PendingKey = { fingerprint: string; key: string } | null;

export type AgentWorkspaceState = {
  readAvailable: boolean | null;
  manageAvailable: boolean | null;
  agents: DurableAgent[];
  conversations: DurableConversation[];
  selectedAgentId: string;
  selectedConversationId: string;
  selectedAgent: DurableAgent | null;
  selectedConversation: DurableConversation | null;
  conversationDetail: ConversationDetail | null;
  conversationLoading: boolean;
  endpoint: AgentEndpoint | null;
  inbox: InboxDelivery[];
  busy: boolean;
  status: string;
  selectAgent: (agentId: string) => void;
  selectConversation: (conversationId: string) => void;
  refresh: () => void;
  createAgent: (input: { handle: string; displayName: string; description: string; labels: string }) => Promise<boolean>;
  updateAgent: (input: { handle: string; displayName: string; description: string; labels: string }) => Promise<boolean>;
  attach: () => Promise<boolean>;
  detach: () => Promise<boolean>;
  createConversation: (title: string, agentIds: string) => Promise<boolean>;
  postMessage: (body: string, recipients: string, sendAsAgent: boolean) => Promise<boolean>;
  consume: (deliveryId: string) => Promise<boolean>;
};

export function useAgentWorkspace(
  client: RuntimeV2Client,
  enabled: boolean,
  onUnauthorized: () => void,
): AgentWorkspaceState {
  const [readAvailable, setReadAvailable] = useState<boolean | null>(null);
  const [manageAvailable, setManageAvailable] = useState<boolean | null>(null);
  const [agents, setAgents] = useState<DurableAgent[]>([]);
  const [conversations, setConversations] = useState<DurableConversation[]>([]);
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [selectedConversationId, setSelectedConversationId] = useState("");
  const [conversationLoading, setConversationLoading] = useState(false);
  const [conversationDetail, setConversationDetail] = useState<ConversationDetail | null>(null);
  const [endpoints, setEndpoints] = useState<Map<string, AgentEndpoint>>(new Map());
  const [inbox, setInbox] = useState<InboxDelivery[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState("");
  const [revision, setRevision] = useState(0);
  const request = useRef<AbortController | null>(null);
  const refreshQueued = useRef(false);
  const pendingAgentCreate = useRef<PendingKey>(null);
  const pendingConversationCreate = useRef<PendingKey>(null);
  const pendingMessage = useRef<PendingKey>(null);
  const pendingAttach = useRef<Map<string, { fingerprint: string; key: string; attachment: string }>>(new Map());
  const pageAttachment = useRef("runtime-v2-" + operationKey("page"));

  const selectedAgent = useMemo(
    () => agents.find((agent) => agent.agent_id === selectedAgentId) || null,
    [agents, selectedAgentId],
  );
  const selectedConversation = useMemo(
    () => conversations.find((conversation) => conversation.conversation_id === selectedConversationId) || null,
    [conversations, selectedConversationId],
  );
  const endpoint = selectedAgentId ? endpoints.get(selectedAgentId) || null : null;

  const refresh = useCallback(() => {
    if (request.current) {
      refreshQueued.current = true;
      return;
    }
    setRevision((value) => value + 1);
  }, []);

  useEffect(() => {
    if (!enabled) {
      request.current?.abort();
      request.current = null;
      refreshQueued.current = false;
      return;
    }
    const controller = new AbortController();
    request.current?.abort();
    request.current = controller;
    let disposed = false;

    void Promise.all([
      fetchAgents(client, controller.signal),
      fetchConversations(client, controller.signal),
    ]).then(([agentResponse, conversationResponse]) => {
      if (disposed || request.current !== controller) return;
      if (agentResponse?.status === 401 || conversationResponse?.status === 401) {
        onUnauthorized();
        return;
      }
      if (agentResponse?.status === 403 || conversationResponse?.status === 403) {
        setReadAvailable(false);
        setAgents([]);
        setConversations([]);
        setConversationDetail(null);
        setInbox([]);
        return;
      }
      if (!agentResponse?.ok || !agentResponse.data || !conversationResponse?.ok || !conversationResponse.data) {
        setStatus("Durable communication refresh failed; previous data retained.");
        return;
      }
      setReadAvailable(true);
      const nextAgents = Array.isArray(agentResponse.data.agents) ? agentResponse.data.agents : [];
      const nextConversations = Array.isArray(conversationResponse.data.conversations)
        ? conversationResponse.data.conversations
        : [];
      setAgents(nextAgents);
      setConversations(nextConversations);
      setSelectedAgentId((current) => nextAgents.some((agent) => agent.agent_id === current)
        ? current
        : nextAgents[0]?.agent_id || "");
      setSelectedConversationId((current) => current || nextConversations[0]?.conversation_id || "");
      setStatus("");
    }).finally(() => {
      if (request.current !== controller) return;
      request.current = null;
      if (!disposed && refreshQueued.current) {
        refreshQueued.current = false;
        setRevision((value) => value + 1);
      }
    });

    return () => {
      disposed = true;
      controller.abort();
      if (request.current === controller) request.current = null;
    };
  }, [client, enabled, onUnauthorized, revision]);

  useEffect(() => { setConversationDetail(null); }, [selectedConversationId]);

  useEffect(() => {
    if (!enabled || !selectedConversationId) {
      if (!selectedConversationId) setConversationDetail(null);
      return;
    }
    const controller = new AbortController();
    setConversationLoading(true);
    const afterSeq = Math.max(0, Number(selectedConversation?.last_seq || 0) - 100);
    void fetchConversation(client, selectedConversationId, afterSeq, controller.signal).then((response) => {
      if (controller.signal.aborted) return;
      setConversationLoading(false);
      if (!response) return;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setReadAvailable(false);
        return;
      }
      if (response.status === 404) {
        setConversationDetail(null);
        return;
      }
      if (response.ok && response.data) setConversationDetail(response.data);
      else setStatus("Conversation refresh failed; previous messages retained.");
    });
    return () => controller.abort();
  }, [client, enabled, onUnauthorized, refresh, selectedConversation?.last_seq, selectedConversationId, revision]);

  useEffect(() => {
    if (!enabled || !selectedAgentId || !endpoint) {
      setInbox([]);
      return;
    }
    const controller = new AbortController();
    void fetchInbox(client, selectedAgentId, endpoint, controller.signal).then((response) => {
      if (controller.signal.aborted || !response) return;
      if (response.status === 401) {
        onUnauthorized();
        return;
      }
      if (response.status === 403) {
        setReadAvailable(false);
        return;
      }
      if (response.status === 400 || response.status === 404) {
        setEndpoints((current) => {
          const updated = new Map(current);
          updated.delete(selectedAgentId);
          return updated;
        });
        setInbox([]);
        return;
      }
      if (response.ok && response.data) {
        setInbox(Array.isArray(response.data.deliveries) ? response.data.deliveries : []);
      }
    });
    return () => controller.abort();
  }, [client, enabled, endpoint?.controller_generation, endpoint?.endpoint_id, onUnauthorized, selectedAgentId, revision]);

  useEffect(() => {
    if (!enabled) return;
    const timer = window.setInterval(refresh, 30_000);
    return () => window.clearInterval(timer);
  }, [enabled, refresh]);

  useEffect(() => {
    if (!enabled || !endpoints.size) return;
    const controller = new AbortController();
    let renewing = false;
    const timer = window.setInterval(() => {
      if (renewing) return;
      renewing = true;
      void (async () => {
        try {
          for (const [agentId, value] of Array.from(endpoints.entries())) {
            const response = await renewEndpoint(client, value.endpoint_id, value.controller_generation, controller.signal);
            // Ignore heartbeats from a detached/replaced Endpoint or disposed view.
            if (controller.signal.aborted) return;
            if (response?.status === 401) {
              onUnauthorized();
              return;
            }
            if (response?.status === 403) {
              setManageAvailable(false);
              return;
            }
            if (response?.status === 400 || response?.status === 404) {
              setEndpoints((current) => {
                const latest = current.get(agentId);
                if (latest?.endpoint_id !== value.endpoint_id ||
                    latest.controller_generation !== value.controller_generation) return current;
                const updated = new Map(current);
                updated.delete(agentId);
                return updated;
              });
            } else if (response?.ok && response.data?.endpoint) {
              setManageAvailable(true);
              setEndpoints((current) => {
                const latest = current.get(agentId);
                if (latest?.endpoint_id !== value.endpoint_id ||
                    latest.controller_generation !== value.controller_generation) return current;
                return new Map(current).set(agentId, response.data!.endpoint!);
              });
            }
          }
        } finally {
          renewing = false;
        }
      })();
    }, 30_000);
    return () => {
      window.clearInterval(timer);
      controller.abort();
    };
  }, [client, enabled, endpoints, onUnauthorized]);

  const createAgentAction = useCallback(async (input: {
    handle: string;
    displayName: string;
    description: string;
    labels: string;
  }) => {
    const validation = validateAgentCreate(input.handle, input.displayName, input.description, input.labels);
    if (!validation.ok) {
      setStatus(validation.error);
      return false;
    }
    const pending = idempotencyKeyFor(pendingAgentCreate.current, validation.value.fingerprint, "runtime-agent");
    pendingAgentCreate.current = pending;
    setBusy(true);
    setStatus("Creating durable Agent…");
    try {
      const response = await createAgent(client, {
        handle: validation.value.handle,
        display_name: validation.value.displayName,
        description: validation.value.description || null,
        specialty_labels: validation.value.labels,
        idempotency_key: pending.key,
      });
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key.");
        return false;
      }
      if (!response.ok || !response.data?.agent) {
        setStatus("Agent creation failed.");
        return false;
      }
      setManageAvailable(true);
      setSelectedAgentId(response.data.agent.agent_id);
      pendingAgentCreate.current = null;
      setStatus(response.data.replayed ? "Existing idempotent Agent replayed." : "Agent created.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, onUnauthorized, refresh]);

  const updateAgentAction = useCallback(async (input: {
    handle: string;
    displayName: string;
    description: string;
    labels: string;
  }) => {
    if (!selectedAgent) return false;
    const handle = input.handle.trim();
    const displayName = input.displayName.trim();
    if (!handle || !displayName) {
      setStatus("Handle and display name are required.");
      return false;
    }
    setBusy(true);
    setStatus("Updating Agent Card…");
    try {
      const response = await updateAgent(client, {
        agent_id: selectedAgent.agent_id,
        expected_profile_revision: selectedAgent.profile_revision,
        handle,
        display_name: displayName,
        description: input.description.trim() || null,
        specialty_labels: parseAgentIds(input.labels),
      });
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Outcome uncertain. Refresh the Card before deciding whether to retry.");
        return false;
      }
      if (!response.ok) {
        setStatus("Agent Card update failed; refresh before retrying a stale revision.");
        return false;
      }
      setManageAvailable(true);
      setStatus("Agent Card updated.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, onUnauthorized, refresh, selectedAgent]);

  const attachAction = useCallback(async () => {
    if (!selectedAgentId) return false;
    setBusy(true);
    try {
      for (const [otherAgentId, otherEndpoint] of Array.from(endpoints.entries())) {
        if (otherAgentId === selectedAgentId) continue;
        const detached = await detachEndpoint(client, otherEndpoint.endpoint_id);
        if (detached?.status === 401) {
          onUnauthorized();
          return false;
        }
        if (detached?.status === 403) {
          setManageAvailable(false);
          setStatus("communication:manage required.");
          return false;
        }
        if (!detached || detached.status === 0 || detached.status === 503) {
          setStatus("Previous Endpoint detach is uncertain. Refresh before switching Agents.");
          return false;
        }
        if (!detached.ok && detached.status !== 404) return false;
      }

      const fingerprint = selectedAgentId;
      const existing = pendingAttach.current.get(selectedAgentId);
      const pending = existing?.fingerprint === fingerprint
        ? existing
        : {
            fingerprint,
            key: operationKey("runtime-endpoint"),
            attachment: pageAttachment.current + "-" + selectedAgentId.slice(-8),
          };
      pendingAttach.current.set(selectedAgentId, pending);
      setStatus("Attaching browser Endpoint…");
      const response = await attachEndpoint(client, {
        agent_id: selectedAgentId,
        host: "Runtime Console",
        client_attachment_id: pending.attachment,
        idempotency_key: pending.key,
      });
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Outcome uncertain. Retry Attach to replay the same idempotency key.");
        return false;
      }
      const attached = response.data?.endpoint;
      if (!response.ok || !attached?.endpoint_id) {
        setStatus("Endpoint attach failed.");
        return false;
      }
      if (attached.lifecycle !== "attached") {
        pendingAttach.current.delete(selectedAgentId);
        setStatus("The exact Attach replay was already replaced. Attach again for a fresh generation.");
        return false;
      }
      setManageAvailable(true);
      setEndpoints(new Map([[selectedAgentId, attached]]));
      pendingAttach.current.delete(selectedAgentId);
      setStatus("Browser Endpoint attached.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, endpoints, onUnauthorized, refresh, selectedAgentId]);

  const detachAction = useCallback(async () => {
    if (!selectedAgentId || !endpoint) return false;
    setBusy(true);
    setStatus("Detaching browser Endpoint…");
    try {
      const response = await detachEndpoint(client, endpoint.endpoint_id);
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Detach outcome uncertain. Refresh before retry.");
        return false;
      }
      if (!response.ok && response.status !== 404) {
        setStatus("Endpoint detach failed.");
        return false;
      }
      setEndpoints((current) => {
        const updated = new Map(current);
        updated.delete(selectedAgentId);
        return updated;
      });
      setInbox([]);
      setStatus("Browser Endpoint detached.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, endpoint, onUnauthorized, refresh, selectedAgentId]);

  const createConversationAction = useCallback(async (title: string, agentIds: string) => {
    const validation = validateConversationCreate(title, agentIds, selectedAgentId);
    if (!validation.ok) {
      setStatus(validation.error);
      return false;
    }
    const pending = idempotencyKeyFor(
      pendingConversationCreate.current,
      validation.value.fingerprint,
      "runtime-conversation",
    );
    pendingConversationCreate.current = pending;
    setBusy(true);
    setStatus("Creating durable Conversation…");
    try {
      const response = await createConversation(client, {
        title: validation.value.title || null,
        agent_ids: validation.value.agentIds,
        idempotency_key: pending.key,
      });
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key.");
        return false;
      }
      const conversationId = response.data?.conversation?.conversation?.conversation_id;
      if (!response.ok || !conversationId) {
        setStatus("Conversation creation failed.");
        return false;
      }
      setManageAvailable(true);
      setSelectedConversationId(conversationId);
      pendingConversationCreate.current = null;
      setStatus(response.data?.replayed ? "Existing idempotent Conversation replayed." : "Conversation created.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, onUnauthorized, refresh, selectedAgentId]);

  const postMessageAction = useCallback(async (body: string, recipients: string, sendAsAgent: boolean) => {
    const text = body.trim();
    if (!selectedConversationId || !text) {
      setStatus("Select a Conversation and enter a message.");
      return false;
    }
    if (sendAsAgent && (!selectedAgent || !endpoint)) {
      setStatus("Select an Agent and attach this browser Endpoint before sending as it.");
      return false;
    }
    const recipientAgentIds = recipients.trim() ? parseAgentIds(recipients) : null;
    const fingerprint = JSON.stringify({
      selectedConversationId,
      text,
      recipientAgentIds,
      authorAgentId: sendAsAgent ? selectedAgent?.agent_id : null,
      endpointId: sendAsAgent ? endpoint?.endpoint_id : null,
      generation: sendAsAgent ? endpoint?.controller_generation : null,
    });
    const pending = idempotencyKeyFor(pendingMessage.current, fingerprint, "runtime-message");
    pendingMessage.current = pending;
    setBusy(true);
    setStatus("Appending durable Message…");
    try {
      const response = await postConversationMessage(client, {
        conversation_id: selectedConversationId,
        body: text,
        author_agent_id: sendAsAgent ? selectedAgent?.agent_id || null : null,
        endpoint_id: sendAsAgent ? endpoint?.endpoint_id || null : null,
        expected_controller_generation: sendAsAgent ? endpoint?.controller_generation || null : null,
        recipient_agent_ids: recipientAgentIds,
        idempotency_key: pending.key,
      });
      if (response?.status === 401) {
        onUnauthorized();
        return false;
      }
      if (response?.status === 403) {
        setManageAvailable(false);
        setStatus("communication:manage required.");
        return false;
      }
      if (!response || response.status === 0 || response.status === 503) {
        setStatus("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key.");
        return false;
      }
      if (!response.ok || !response.data?.message) {
        setStatus("Message append failed.");
        return false;
      }
      setManageAvailable(true);
      pendingMessage.current = null;
      setStatus(response.data?.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent.");
      refresh();
      return true;
    } finally {
      setBusy(false);
    }
  }, [client, endpoint, onUnauthorized, refresh, selectedAgent, selectedConversationId]);

  const consumeAction = useCallback(async (deliveryId: string) => {
    if (!selectedAgentId || !endpoint || !deliveryId) return false;
    const response = await consumeInbox(client, selectedAgentId, endpoint, [deliveryId]);
    if (response?.status === 401) {
      onUnauthorized();
      return false;
    }
    if (response?.status === 403) {
      setManageAvailable(false);
      setStatus("communication:manage required to consume deliveries.");
      return false;
    }
    if (!response || response.status === 0 || response.status === 503) {
      setStatus("Consume outcome uncertain. Refresh before retry; desired-state replay is safe.");
      return false;
    }
    if (!response.ok) {
      setStatus("Delivery consume failed.");
      return false;
    }
    setManageAvailable(true);
    setStatus("Delivery consumed.");
    refresh();
    return true;
  }, [client, endpoint, onUnauthorized, refresh, selectedAgentId]);

  return {
    readAvailable,
    manageAvailable,
    agents,
    conversations,
    selectedAgentId,
    selectedConversationId,
    selectedAgent,
    selectedConversation,
    conversationDetail,
    conversationLoading,
    endpoint,
    inbox,
    busy,
    status,
    selectAgent: setSelectedAgentId,
    selectConversation: setSelectedConversationId,
    refresh,
    createAgent: createAgentAction,
    updateAgent: updateAgentAction,
    attach: attachAction,
    detach: detachAction,
    createConversation: createConversationAction,
    postMessage: postMessageAction,
    consume: consumeAction,
  };
}
