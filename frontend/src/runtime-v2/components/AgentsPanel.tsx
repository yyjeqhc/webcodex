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
};

export function AgentsPanel({ client, language, onUnauthorized }: Props) {
  const t = (value: string) => translate(value, language);
  const state = useAgentWorkspace(client, true, onUnauthorized);
  const [createHandle, setCreateHandle] = useState("");
  const [createName, setCreateName] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [createLabels, setCreateLabels] = useState("");
  const [updateHandle, setUpdateHandle] = useState("");
  const [updateName, setUpdateName] = useState("");
  const [updateDescription, setUpdateDescription] = useState("");
  const [updateLabels, setUpdateLabels] = useState("");
  const [conversationTitle, setConversationTitle] = useState("");
  const [conversationAgents, setConversationAgents] = useState("");
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
    const ok = await state.createConversation(conversationTitle, conversationAgents);
    if (ok) {
      setConversationTitle("");
      setConversationAgents(state.selectedAgentId);
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
            <strong>{t("Durable Agents")}</strong>
            <small>{t("Agent identity, endpoint readiness and durable inbox state.")}</small>
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
              onClick={() => state.selectAgent(agent.agent_id)}
            >
              <span className="window-icon"><Bot size={15} /></span>
              <span>
                <strong>{agent.display_name || agent.handle || "Agent"}</strong>
                <small>@{agent.handle} · {shortId(agent.agent_id)}</small>
                <small>{agent.queued_delivery_count || 0} {t("queued")} · {agent.active_endpoint_count || 0} {t("endpoints")}</small>
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
            <button className="primary-button compact" type="submit" disabled={state.busy}>{t("Create")}</button>
          </form>
        </details>
      </aside>

      <section className="agents-main">
        {state.selectedAgent ? (
          <>
            <header className="agent-detail-head">
              <div>
                <span className="eyebrow">{t("Agent identity")}</span>
                <h2>{state.selectedAgent.display_name}</h2>
                <p>@{state.selectedAgent.handle} · <code>{state.selectedAgent.agent_id}</code></p>
              </div>
              <span className={"status-pill " + (state.endpoint ? "good" : "warn")}>
                {state.endpoint ? <Check size={12} /> : <Unlink size={12} />}
                {state.endpoint ? t("Browser Endpoint attached") : t("No browser Endpoint")}
              </span>
            </header>

            <section className="agent-card-grid">
              <div><span>{t("Profile revision")}</span><strong>{state.selectedAgent.profile_revision}</strong></div>
              <div><span>{t("Controller generation")}</span><strong>{state.selectedAgent.current_controller_generation || 0}</strong></div>
              <div><span>{t("Unresolved Wakes")}</span><strong>{state.selectedAgent.unresolved_wake_count || 0}</strong></div>
              <div><span>{t("Queued deliveries")}</span><strong>{state.selectedAgent.queued_delivery_count || 0}</strong></div>
            </section>

            <section className="agent-section">
              <div className="section-heading">
                <div><h2>{t("Browser Endpoint")}</h2><p>{t("Endpoint binding is window-local control state; durable Agent identity remains server-owned.")}</p></div>
                <div className="button-row">
                  {state.endpoint ? (
                    <button className="text-button" type="button" onClick={() => void state.detach()} disabled={state.busy}><Unlink size={13} /> {t("Detach")}</button>
                  ) : (
                    <button className="text-button" type="button" onClick={() => void state.attach()} disabled={state.busy}><Link2 size={13} /> {t("Continue as this Agent")}</button>
                  )}
                </div>
              </div>
              <div className="endpoint-evidence">
                {state.endpoint ? (
                  <>
                    <code>{state.endpoint.endpoint_id}</code>
                    <span>{t("generation")} {state.endpoint.controller_generation}</span>
                    <span>{t("lease")} {absoluteTime(state.endpoint.lease_expires_at_unix_ms)}</span>
                  </>
                ) : <span>{t("No Endpoint is attached from this browser tab.")}</span>}
              </div>
            </section>

            <details className="agent-section edit-agent-card">
              <summary>{t("Edit Agent Card")}</summary>
              <form className="compact-form inline-grid" onSubmit={(event) => void submitUpdate(event)}>
                <label>{t("Handle")}<input value={updateHandle} onChange={(event) => setUpdateHandle(event.target.value)} /></label>
                <label>{t("Display name")}<input value={updateName} onChange={(event) => setUpdateName(event.target.value)} /></label>
                <label>{t("Description")}<input value={updateDescription} onChange={(event) => setUpdateDescription(event.target.value)} /></label>
                <label>{t("Specialty labels")}<input value={updateLabels} onChange={(event) => setUpdateLabels(event.target.value)} /></label>
                <button className="primary-button compact" type="submit" disabled={state.busy}>{t("Save")}</button>
              </form>
            </details>
          </>
        ) : (
          <div className="empty-inline">{t("Select an Agent to inspect durable identity and endpoint readiness.")}</div>
        )}

        <section className="agent-section conversations-section">
          <div className="section-heading">
            <div><h2>{t("Durable Conversations")}</h2><p>{t("Transcript and Agent Inbox delivery are separate durable facts.")}</p></div>
          </div>

          <div className="conversation-layout">
            <aside className="conversation-list">
              {state.conversations.map((conversation) => (
                <button
                  type="button"
                  className={conversation.conversation_id === state.selectedConversationId ? "selected" : ""}
                  key={conversation.conversation_id}
                  onClick={() => state.selectConversation(conversation.conversation_id)}
                >
                  <MessageSquare size={14} />
                  <span>
                    <strong>{conversation.title || t("Untitled Conversation")}</strong>
                    <small>{conversation.message_count || 0} {t("messages")} · seq {conversation.last_seq || 0}</small>
                  </span>
                </button>
              ))}
              {!state.conversations.length && <div className="empty-inline">{t("No durable Conversations.")}</div>}

              <details>
                <summary><Plus size={13} /> {t("New Conversation")}</summary>
                <form className="compact-form" onSubmit={(event) => void submitConversation(event)}>
                  <label>{t("Title")}<input value={conversationTitle} onChange={(event) => setConversationTitle(event.target.value)} /></label>
                  <label>{t("Agent IDs")}<input value={conversationAgents} onChange={(event) => setConversationAgents(event.target.value)} placeholder={state.selectedAgentId || "wc_dagent_…"} /></label>
                  <button className="primary-button compact" type="submit" disabled={state.busy}>{t("Create")}</button>
                </form>
              </details>
            </aside>

            <div className="conversation-detail">
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
                      <small>{t("Deliveries")}: {message.deliveries.map((delivery) => shortId(delivery.recipient_agent_id || "") + " " + (delivery.state || "")).join(" · ")}</small>
                    )}
                  </article>
                ))}
                {!state.conversationDetail?.messages?.length && <div className="empty-inline">{t("No retained messages in this Conversation.")}</div>}
              </div>

              <form className="conversation-composer" onSubmit={(event) => void submitMessage(event)}>
                <textarea rows={2} value={messageBody} onChange={(event) => setMessageBody(event.target.value)} placeholder={t("Append a durable message…")} />
                <div>
                  <input value={messageRecipients} onChange={(event) => setMessageRecipients(event.target.value)} placeholder={t("Recipient Agent IDs (optional)")} />
                  <label className="checkbox-line"><input type="checkbox" checked={sendAsAgent} onChange={(event) => setSendAsAgent(event.target.checked)} /> {t("Send as selected Agent")}</label>
                  <button className="send-button" type="submit" disabled={state.busy || !messageBody.trim()}><ArrowUpRight size={15} /></button>
                </div>
              </form>
            </div>
          </div>
        </section>

        {state.selectedAgent && (
          <section className="agent-section">
            <div className="section-heading">
              <div><h2>{t("Agent Inbox")}</h2><p>{t("Inbox consumption requires the exact attached Endpoint generation.")}</p></div>
              <span className="quiet-pill"><Inbox size={13} /> {state.inbox.length}</span>
            </div>
            <div className="inbox-list">
              {state.inbox.map((delivery) => (
                <article key={delivery.delivery_id}>
                  <span>
                    <strong>{delivery.conversation_title || delivery.conversation_id || t("Conversation")}</strong>
                    <small>{delivery.message?.body || delivery.delivery_id}</small>
                  </span>
                  <button className="text-button" type="button" onClick={() => void state.consume(delivery.delivery_id)} disabled={state.busy}>{t("Consume")}</button>
                </article>
              ))}
              {!state.endpoint && <div className="empty-inline">{t("Attach this browser as the selected Agent to read its endpoint-scoped Inbox.")}</div>}
              {state.endpoint && !state.inbox.length && <div className="empty-inline">{t("No queued Inbox deliveries.")}</div>}
            </div>
          </section>
        )}

        {state.status && <p className="agent-status" role="status">{state.status}</p>}
        {state.manageAvailable === false && <p className="agent-status warn">{t("communication:manage is unavailable; diagnostics remain read-only.")}</p>}
      </section>
    </div>
  );
}
