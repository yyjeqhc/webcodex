import {
  ArrowUpRight,
  Bot,
  Check,
  Inbox,
  Link2,
  MessageSquare,
  Plus,
  RefreshCw,
  Unlink,
} from "lucide-react";
import { MultiSelect } from "@mantine/core";
import { FormEvent, useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, shortId } from "../model/format.js";
import type { RuntimeV2Client } from "../api/client.js";
import { useAgentWorkspace } from "../state/useAgentWorkspace.js";

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  onUnauthorized: () => void;
  selectedAgentId?: string;
  onSelectedAgentConsumed?: () => void;
};

export function AgentsPanel({ client, language, onUnauthorized, selectedAgentId, onSelectedAgentConsumed }: Props) {
  const t = (value: string) => translate(value, language);
  const state = useAgentWorkspace(client, true, onUnauthorized);
  const visibleConversations = state.selectedConversationId && !state.conversations.some((conversation) => conversation.conversation_id === state.selectedConversationId)
    ? [{ conversation_id: state.selectedConversationId, title: state.conversationDetail?.conversation?.title || t("Conversation") }, ...state.conversations]
    : state.conversations;
  const [section, setSection] = useState<"inbox" | "conversations" | "profile">("inbox");
  const agentOptions = state.agents.map((agent) => ({ value: agent.agent_id, label: `${agent.display_name} (@${agent.handle})` }));
  const [createHandle, setCreateHandle] = useState("");
  const [createName, setCreateName] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [createLabels, setCreateLabels] = useState("");
  const [updateHandle, setUpdateHandle] = useState("");
  const [updateName, setUpdateName] = useState("");
  const [updateDescription, setUpdateDescription] = useState("");
  const [updateLabels, setUpdateLabels] = useState("");
  const [conversationTitle, setConversationTitle] = useState("");
  const [conversationAgents, setConversationAgents] = useState<string[] | null>(null);
  const [messageBody, setMessageBody] = useState("");
  const [messageRecipients, setMessageRecipients] = useState("");
  const [sendAsAgent, setSendAsAgent] = useState(false);

  useEffect(() => {
    const agent = state.selectedAgent;
    setUpdateHandle(agent?.handle || "");
    setUpdateName(agent?.display_name || "");
    setUpdateDescription(agent?.description || "");
    setUpdateLabels((agent?.specialty_labels || []).join(", "));
  }, [state.selectedAgent?.agent_id, state.selectedAgent?.profile_revision]);

  useEffect(() => {
    if (selectedAgentId && state.agents.some((agent) => agent.agent_id === selectedAgentId)) {
      state.selectAgent(selectedAgentId);
      setSection("inbox");
      onSelectedAgentConsumed?.();
    }
  }, [onSelectedAgentConsumed, selectedAgentId, state.agents]);

  useEffect(() => { setSendAsAgent(false); }, [state.selectedAgentId]);

  const submitCreate = async (event: FormEvent) => {
    event.preventDefault();
    const ok = await state.createAgent({
      handle: createHandle,
      displayName: createName,
      description: createDescription,
      labels: createLabels,
    });
    if (ok) {
      setCreateHandle("");
      setCreateName("");
      setCreateDescription("");
      setCreateLabels("");
    }
  };

  const submitUpdate = async (event: FormEvent) => {
    event.preventDefault();
    await state.updateAgent({
      handle: updateHandle,
      displayName: updateName,
      description: updateDescription,
      labels: updateLabels,
    });
  };

  const submitConversation = async (event: FormEvent) => {
    event.preventDefault();
    const ok = await state.createConversation(conversationTitle, (conversationAgents ?? [state.selectedAgentId]).join(","));
    if (ok) {
      setConversationTitle("");
      setConversationAgents(null);
    }
  };

  const submitMessage = async (event: FormEvent) => {
    event.preventDefault();
    const ok = await state.postMessage(messageBody, messageRecipients, sendAsAgent);
    if (ok) setMessageBody("");
  };

  if (state.readAvailable === false) {
    return (
      <section className="runtime-section agents-denied">
        <div className="empty-panel wide">
          <Bot size={20} />
          <strong>{t("Durable Agent diagnostics require communication:read.")}</strong>
          <p>{t("Runtime and Project views remain available under their independent authority scopes.")}</p>
        </div>
      </section>
    );
  }

  return (
    <div className="agents-workbench" data-testid="agents-workbench">
      <aside className="agents-sidebar">
        <div className="window-list-head">
          <div>
            <strong>{t("Agents")}</strong>
          </div>
          <button className="icon-button" type="button" onClick={state.refresh} aria-label={t("Refresh")}>
            <RefreshCw size={14} />
          </button>
        </div>

        <div className="agent-list">
          {state.agents.map((agent) => (
            <button
              type="button"
              className={"agent-row" + (agent.agent_id === state.selectedAgentId ? " selected" : "")}
              key={agent.agent_id}
              disabled={state.busy}
              aria-pressed={agent.agent_id === state.selectedAgentId}
              onClick={() => { state.selectAgent(agent.agent_id); setSection("inbox"); setSendAsAgent(false); }}
            >
              <span className="window-icon"><Bot size={15} /></span>
              <span>
                <strong>{agent.display_name || agent.handle || "Agent"}</strong>
                <small>@{agent.handle}</small>
                <small>{agent.queued_delivery_count ?? "—"} {t("pending messages")} · {agent.active_endpoint_count ?? "—"} {t("connections")}</small>
              </span>
            </button>
          ))}
          {!state.agents.length && <div className="empty-inline">{t("No durable Agents are visible.")}</div>}
        </div>

        <details className="agent-create-disclosure">
          <summary><Plus size={14} /> {t("Create Agent")}</summary>
          <form className="compact-form" onSubmit={(event) => void submitCreate(event)}>
            <label>{t("Handle")}<input value={createHandle} onChange={(event) => setCreateHandle(event.target.value)} /></label>
            <label>{t("Display name")}<input value={createName} onChange={(event) => setCreateName(event.target.value)} /></label>
            <label>{t("Description")}<textarea rows={2} value={createDescription} onChange={(event) => setCreateDescription(event.target.value)} /></label>
            <label>{t("Specialty labels")}<input value={createLabels} onChange={(event) => setCreateLabels(event.target.value)} placeholder="rust, runtime" /></label>
            <button className="primary-button compact" type="submit" disabled={state.busy || state.manageAvailable === false}>{t("Create")}</button>
          </form>
        </details>
      </aside>

      <section className="agents-main">
        <div className="runtime-tabs agent-tabs" role="tablist" aria-label={t("Agent workspace")}>
          {([ ["inbox", "Inbox"], ["conversations", "All conversations"], ["profile", "Profile"] ] as const).map(([value, label]) =>
            <button key={value} role="tab" aria-selected={section === value} className={section === value ? "active" : ""} onClick={() => setSection(value)}>{t(label)}</button>)}
        </div>
        {section !== "conversations" && (state.selectedAgent ? (
          <>
            <header className="agent-detail-head">
              <div>
                <h2>{state.selectedAgent.display_name}</h2>
                <p>@{state.selectedAgent.handle}</p>
                <p>{state.selectedAgent.description}</p>
                <p>{state.selectedAgent.specialty_labels?.join(" · ")}</p>
              </div>
              <span className={"status-pill " + (state.endpoint ? "good" : "warn")}>
                {state.endpoint ? <Check size={12} /> : <Unlink size={12} />}
                {state.endpoint ? t("Connected in this browser") : t("Not connected in this browser")}
              </span>
            </header>

            {section === "inbox" && <section className="agent-card-grid">
              <div><span>{t("Pending messages")}</span><strong>{state.selectedAgent.queued_delivery_count ?? "—"}</strong></div>
              <div><span>{t("Connections")}</span><strong>{state.selectedAgent.active_endpoint_count ?? "—"}</strong></div>
            </section>}

            <section className="agent-section">
              <div className="section-heading">
                <div><h2>{t("Browser connection")}</h2><p>{t("Connect to read and acknowledge messages or send as this Agent. This does not start a model.")}</p></div>
                <div className="button-row">
                  {state.endpoint ? (
                    <button className="text-button" type="button" onClick={() => void state.detach()} disabled={state.busy || state.manageAvailable === false}><Unlink size={13} /> {t("Detach")}</button>
                  ) : (
                    <button className="text-button" type="button" onClick={() => void state.attach()} disabled={state.busy || state.manageAvailable === false}><Link2 size={13} /> {t("Connect as this Agent")}</button>
                  )}
                </div>
              </div>
            </section>

            {section === "profile" && <>
            <section className="agent-section edit-agent-card">
              <h3>{t("Edit profile")}</h3>
              <form className="compact-form inline-grid" onSubmit={(event) => void submitUpdate(event)}>
                <label>{t("Handle")}<input value={updateHandle} onChange={(event) => setUpdateHandle(event.target.value)} /></label>
                <label>{t("Display name")}<input value={updateName} onChange={(event) => setUpdateName(event.target.value)} /></label>
                <label>{t("Description")}<input value={updateDescription} onChange={(event) => setUpdateDescription(event.target.value)} /></label>
                <label>{t("Specialty labels")}<input value={updateLabels} onChange={(event) => setUpdateLabels(event.target.value)} /></label>
                <button className="primary-button compact" type="submit" disabled={state.busy || state.manageAvailable === false}>{t("Save")}</button>
              </form>
            </section>
            <details className="agent-section">
              <summary>{t("Technical details")}</summary>
              <div className="endpoint-evidence">
                <code>{state.selectedAgent.agent_id}</code>
                <span>{t("Profile revision")}: {state.selectedAgent.profile_revision}</span>
                <span>{t("Controller generation")}: {state.selectedAgent.current_controller_generation ?? "—"}</span>
                <span>{t("Unresolved Wakes")}: {state.selectedAgent.unresolved_wake_count ?? "—"}</span>
                {state.endpoint && <><code>{state.endpoint.endpoint_id}</code><span>{t("lease")}: {absoluteTime(state.endpoint.lease_expires_at_unix_ms)}</span></>}
              </div>
            </details>
            </>}
          </>
        ) : (
          <div className="empty-inline">{t("Select or create an Agent to get started.")}</div>
        ))}

        {section === "conversations" && <section className="agent-section conversations-section">
          <div className="section-heading">
            <div><h2>{t("All conversations")}</h2><p>{t("Shared conversations visible to your account.")}</p></div>
          </div>

          <div className="conversation-layout">
            <aside className="conversation-list">
              {visibleConversations.map((conversation) => (
                <button
                  type="button"
                  className={conversation.conversation_id === state.selectedConversationId ? "selected" : ""}
                  key={conversation.conversation_id}
                  disabled={state.busy}
                  onClick={() => { state.selectConversation(conversation.conversation_id); setMessageBody(""); setMessageRecipients(""); setSendAsAgent(false); }}
                >
                  <MessageSquare size={14} />
                  <span>
                    <strong>{conversation.title || t("Untitled Conversation")}</strong>
                    <small>{conversation.message_count ?? "—"} {t("messages")}</small>
                  </span>
                </button>
              ))}
              {!visibleConversations.length && <div className="empty-inline">{t("No durable Conversations.")}</div>}

              <details>
                <summary><Plus size={13} /> {t("New Conversation")}</summary>
                <form className="compact-form" onSubmit={(event) => void submitConversation(event)}>
                  <label>{t("Title")}<input value={conversationTitle} onChange={(event) => setConversationTitle(event.target.value)} /></label>
                  <MultiSelect label={t("Participants")} data={agentOptions} value={conversationAgents ?? (state.selectedAgentId ? [state.selectedAgentId] : [])} onChange={setConversationAgents} searchable />
                  <button className="primary-button compact" type="submit" disabled={state.busy || state.manageAvailable === false || !(conversationAgents ?? (state.selectedAgentId ? [state.selectedAgentId] : [])).length}>{t("Create")}</button>
                </form>
              </details>
            </aside>

            <div className="conversation-detail">
              <h3 className="conversation-title">{state.conversationDetail?.conversation?.title || state.selectedConversation?.title || t("Conversation")}</h3>
              <div className="conversation-transcript">
                {(state.conversationDetail?.messages || []).map((message) => (
                  <article className="conversation-message-v2" key={message.message_id}>
                    <header>
                      <strong>
                        {message.author?.participant_kind === "agent"
                          ? message.author.display_name || message.author.handle || shortId(message.author.agent_id || "")
                          : t("Human")}
                      </strong>
                      <span>#{message.seq} · {absoluteTime(message.created_at_unix_ms)}</span>
                    </header>
                    <p>{message.body}</p>
                    {!!message.deliveries?.length && (
                      <small>{t("Deliveries")}: {message.deliveries.map((delivery) => (state.agents.find((agent) => agent.agent_id === delivery.recipient_agent_id)?.display_name || shortId(delivery.recipient_agent_id || "")) + " " + (delivery.state || "")).join(" · ")}</small>
                    )}
                  </article>
                ))}
                {state.conversationDetail && !state.conversationDetail.messages?.length && <div className="empty-inline">{t("No retained messages in this Conversation.")}</div>}
              </div>

              {!state.conversationDetail && <div className="empty-inline">{t(state.selectedConversationId ? state.conversationLoading ? "Loading…" : "Messages unavailable. Refresh to retry." : "Select or create a conversation.")}</div>}
              <form className="conversation-composer" onSubmit={(event) => void submitMessage(event)}>
                <textarea aria-label={t("Message")} disabled={state.busy} rows={2} value={messageBody} onChange={(event) => setMessageBody(event.target.value)} placeholder={t("Write a message…")} />
                <div>
                  <MultiSelect label={t("Recipients (optional)")} data={agentOptions} value={messageRecipients ? messageRecipients.split(",") : []} onChange={(values) => setMessageRecipients(values.join(","))} searchable />
                  <label className="checkbox-line"><input type="checkbox" disabled={!state.endpoint} checked={sendAsAgent} onChange={(event) => setSendAsAgent(event.target.checked)} /> {t("Send as")}: {state.selectedAgent?.display_name || "Agent"}</label>
                  <button className="send-button" type="submit" aria-label={t("Send message")} disabled={state.busy || state.manageAvailable === false || !state.selectedConversationId || !messageBody.trim() || (sendAsAgent && !state.endpoint)}><ArrowUpRight size={15} /></button>
                </div>
              </form>
            </div>
          </div>
        </section>}

        {section === "inbox" && state.selectedAgent && (
          <section className="agent-section">
            <div className="section-heading">
              <div><h2>{t("Pending messages")}</h2></div>
              <span className="quiet-pill"><Inbox size={13} /> {state.inbox.length}</span>
            </div>
            <div className="inbox-list">
              {state.inbox.map((delivery) => (
                <article key={delivery.delivery_id}>
                  <span>
                    <strong>{delivery.conversation_title || delivery.conversation_id || t("Conversation")}</strong>
                    <small>{delivery.message?.body || delivery.delivery_id}</small>
                  </span>
                  <div className="inbox-actions">
                    {delivery.conversation_id && <button className="text-button" type="button" disabled={state.busy} onClick={() => { state.selectConversation(delivery.conversation_id!); setSection("conversations"); setMessageBody(""); setMessageRecipients(""); setSendAsAgent(false); }}>{t("Open conversation")}</button>}
                    <button className="text-button" type="button" onClick={() => void state.consume(delivery.delivery_id)} disabled={state.busy || state.manageAvailable === false}>{t("Acknowledge")}</button>
                  </div>
                </article>
              ))}
              {!state.endpoint && <div className="empty-inline">{t("Connect as this Agent to open its inbox.")}</div>}
              {state.endpoint && !state.inbox.length && <div className="empty-inline">{t("No queued Inbox deliveries.")}</div>}
            </div>
          </section>
        )}

        {state.status && <p className="agent-status" role="status">{t(state.status)}</p>}
        {state.manageAvailable === false && <p className="agent-status warn">{t("communication:manage is unavailable; diagnostics remain read-only.")}</p>}
      </section>
    </div>
  );
}
