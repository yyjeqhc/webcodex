import { Bot, CheckCircle2, MessageSquare, Send, UserRound } from "lucide-react";
import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import type {
  WindowCollaborationKind,
  WindowCollaborationMessage,
  WindowCollaborationPriority,
} from "../api/windowCollaboration.js";
import { translate, type RuntimeLanguage } from "../../runtime_i18n.js";
import { clockTime, shortId } from "../model/format.js";
import { useWindowCollaboration } from "../state/useWindowCollaboration.js";

function deliveryLabel(row: WindowCollaborationMessage, t: (source: string) => string): string | null {
  if (row.source === "peer" && row.direction === "inbound") return null;
  if (row.source === "window") return null;
  if (row.first_ack_observed_at_ms != null) return t("Acknowledged");
  if (row.first_projected_at_ms != null) return t("Included in tool result");
  return t("Saved");
}

function participantLabel(row: WindowCollaborationMessage, t: (source: string) => string): string {
  if (row.source === "operator") return t("You");
  if (row.source === "window") return t("This Window");
  if (row.direction === "outbound") return t("This Window → Peer Window");
  return t("Peer Window");
}

function kindLabel(kind: WindowCollaborationKind, t: (source: string) => string): string {
  const labels: Record<WindowCollaborationKind, string> = {
    note: "Note",
    proposal: "Proposal",
    question: "Question",
    answer: "Answer",
    decision: "Decision",
    risk: "Risk",
    progress: "Progress",
    guidance: "Guidance",
    todo: "Todo",
  };
  return t(labels[kind] || kind);
}

function priorityLabel(priority: WindowCollaborationPriority, t: (source: string) => string): string {
  return priority === "high"
    ? t("High priority")
    : priority === "low"
      ? t("Low priority")
      : t("Normal");
}

export function WindowCollaboration({ client, windowKey, selectedSessionId, language, onUnauthorized, active = true }: {
  client: RuntimeV2Client;
  windowKey: string;
  selectedSessionId: string;
  language: RuntimeLanguage;
  onUnauthorized: () => void;
  active?: boolean;
}) {
  const t = (source: string) => translate(source, language);
  const [message, setMessage] = useState("");
  const [messageKind, setMessageKind] = useState<WindowCollaborationKind>("guidance");
  const [priority, setPriority] = useState<WindowCollaborationPriority>("normal");
  const [requiresAck, setRequiresAck] = useState(true);
  const followLatest = useRef(true);
  const [hasNewMessages, setHasNewMessages] = useState(false);
  const threadRef = useRef<HTMLDivElement | null>(null);
  const historyAnchor = useRef<{ height: number; top: number } | null>(null);
  const state = useWindowCollaboration(client, windowKey, onUnauthorized, active);
  const sending = state.sendState === "sending";
  const uncertain = state.sendState === "uncertain";
  const messages = state.transcript?.messages || [];
  const byId = new Map(messages.map(row => [row.message_id, row]));
  const answered = new Set(messages.map(row => row.reply_to_message_id).filter(Boolean));
  useLayoutEffect(() => {
    const node = threadRef.current, anchor = historyAnchor.current;
    if (node && anchor) {
      node.scrollTop = anchor.top + Math.max(0, node.scrollHeight - anchor.height);
      historyAnchor.current = null;
    }
  }, [messages]);
  useEffect(() => {
    setMessage(""); followLatest.current = true; setHasNewMessages(false); historyAnchor.current = null;
  }, [client, windowKey]);
  const sendError = state.sendError === "conflict"
    ? t("Message could not be sent. Send it again.")
    : state.sendError === "context"
      ? t("Context is no longer available. Choose a Session or All calls, then send again.")
      : state.sendError === "unavailable"
        ? t("Collaboration is unavailable for this Window.")
        : state.sendError === "failed"
          ? t("Message could not be sent.")
          : null;

  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (state.browsingHistory) return;
    if (followLatest.current) node.scrollTop = node.scrollHeight;
    else setHasNewMessages(true);
  }, [messages.length, messages.at(-1)?.message_id]);

  const submit = () => {
    const text = message.trim();
    if (!text || sending || !state.transcript?.can_send) return;
    void state.send(
      text,
      selectedSessionId || null,
      messageKind,
      priority,
      requiresAck,
    ).then(ok => {
      if (ok) setMessage("");
    });
  };

  return (
    <section className="window-collaboration" aria-label={t("Collaboration")}>
      <header className="window-collaboration-toolbar">
        <span className="window-collaboration-title-icon"><MessageSquare size={16} /></span>
        <div>
          <strong>{t("Collaboration")}</strong>
          <small aria-live="polite">
            {messages.length
              ? t("Messages: {count}").replace("{count}", new Intl.NumberFormat(language).format(messages.length))
              : t("Leave a message or instruction for this Window")}
          </small>
          {hasNewMessages && <button type="button" className="text-button" onClick={() => {
            const node = threadRef.current;
            if (node) node.scrollTop = node.scrollHeight;
            followLatest.current = true;
            setHasNewMessages(false);
          }}>{t("View new messages")}</button>}
        </div>
        {selectedSessionId && (
          <span className="window-collaboration-context" title={selectedSessionId}>
            {t("Context")} · {shortId(selectedSessionId)}
          </span>
        )}
      </header>

      <div className="window-collaboration-thread" ref={threadRef} onScroll={() => {
        const node = threadRef.current;
        if (!node) return;
        followLatest.current = node.scrollHeight - node.scrollTop - node.clientHeight < 48;
        if (followLatest.current) setHasNewMessages(false);
      }}>
        {state.error && (
          <div className="window-collaboration-banner" role="status">
            {state.readError === "access"
              ? t("The current access key cannot collaborate (session:collaborate is required). Reconnect with a collaboration-enabled key.")
              : state.readError === "unavailable"
                ? t("This Window is not available to the current access key.")
                : t("Messages could not be refreshed.")}
          </div>
        )}

        <div className="window-collaboration-history-note">
          {state.transcript?.truncated && <button type="button" className="text-button" disabled={state.loadingHistory} onClick={() => {
            const node = threadRef.current;
            if (node) historyAnchor.current = { height: node.scrollHeight, top: node.scrollTop };
            followLatest.current = false;
            void state.loadOlder();
          }}>{state.loadingHistory ? t("Loading history…") : t("Load earlier messages")}</button>}
          {state.browsingHistory && <button type="button" className="text-button" onClick={() => {
            historyAnchor.current = null; followLatest.current = true; setHasNewMessages(false); state.latest();
          }}>{t("Return to latest messages")}</button>}
        </div>
        {messages.map((row, index) => {
          const status = deliveryLabel(row, t);
          const outgoing =
            row.source === "operator" || (row.source === "peer" && row.direction === "outbound");
          const date = new Date(row.created_at_ms);
          const previousDate = index > 0 ? new Date(messages[index - 1].created_at_ms).toDateString() : null;
          const reply = row.reply_to_message_id ? byId.get(row.reply_to_message_id) : null;
          return (
            <Fragment key={row.message_id}>
            {date.toDateString() !== previousDate && <div className="window-collaboration-history-note"><time dateTime={date.toISOString()}>{date.toLocaleDateString(language)}</time></div>}
            <article
              className={"window-collaboration-message " + (outgoing ? "outgoing" : "incoming")}
              data-message-id={row.message_id}
            >
              <span className="window-collaboration-avatar" aria-hidden="true">
                {row.source === "operator" ? <UserRound size={15} /> : <Bot size={15} />}
              </span>
              <div className="window-collaboration-bubble">
                <header>
                  <strong title={row.peer_id || undefined}>{participantLabel(row, t)}</strong>
                  <time dateTime={date.toISOString()} title={date.toLocaleString(language)}>{clockTime(row.created_at_ms)}</time>
                </header>
                {row.reply_to_message_id && <blockquote className="window-collaboration-reply">
                  <strong>{t("Reply to")}</strong>{" "}{reply ? reply.message.slice(0, 180) : t("Earlier message")}
                </blockquote>}
                <p>{row.message}</p>
                <footer>
                  <span className="window-collaboration-meta-chip">{kindLabel(row.kind, t)}</span>
                  {answered.has(row.message_id) && <span className="window-collaboration-meta-chip">{t("Reply received")}</span>}
                  {row.priority !== "normal" && (
                    <span className={"window-collaboration-meta-chip priority-" + row.priority}>
                      {priorityLabel(row.priority, t)}
                    </span>
                  )}
                  {status && (
                    <span className="window-collaboration-delivery">
                      <CheckCircle2 size={12} /> {status}
                    </span>
                  )}
                  {row.requires_ack && row.first_ack_observed_at_ms == null && (
                    <span className="window-collaboration-meta-chip ack">
                      {t("ACK requested")}
                    </span>
                  )}
                  {row.context_session_id && (
                    <span className="window-collaboration-session" title={row.context_session_id}>
                      {t("Session")} · {shortId(row.context_session_id)}
                    </span>
                  )}
                </footer>
              </div>
            </article>
            </Fragment>
          );
        })}

        {state.transcript && !messages.length && (
          <div className="window-collaboration-empty">
            <span><MessageSquare size={20} /></span>
            <strong>{t("No messages yet")}</strong>
            <small>{t("Send a note or instruction to this Window.")}</small>
          </div>
        )}
        {state.browsingHistory && <p className="window-collaboration-history-note">{t("Reading saved history. New messages do not replace this page.")}</p>}
      </div>

      <form
        className="window-collaboration-composer"
        onSubmit={event => {
          event.preventDefault();
          submit();
        }}
      >
        <details className="window-collaboration-semantics"><summary>{t("Delivery states")}</summary>
          <p>{t("Saved messages remain in history. ACK confirms model context, not acceptance or completion. A reply is shown separately.")}</p>
        </details>
        <div className="window-collaboration-compose-box">
          <div className="window-collaboration-compose-options">
            <label>
              <span>{t("Type")}</span>
              <select
                aria-label={t("Message type")}
                value={messageKind}
                disabled={sending || uncertain}
                onChange={event => setMessageKind(event.currentTarget.value as WindowCollaborationKind)}
              >
                <option value="guidance">{t("Guidance")}</option>
                <option value="note">{t("Note")}</option>
                <option value="question">{t("Question")}</option>
                <option value="todo">{t("Todo")}</option>
              </select>
            </label>
            <label>
              <span>{t("Priority")}</span>
              <select
                aria-label={t("Message priority")}
                value={priority}
                disabled={sending || uncertain}
                onChange={event => setPriority(event.currentTarget.value as WindowCollaborationPriority)}
              >
                <option value="normal">{t("Normal")}</option>
                <option value="high">{t("High")}</option>
                <option value="low">{t("Low")}</option>
              </select>
            </label>
            <label className="window-collaboration-ack-toggle">
              <input
                type="checkbox"
                checked={requiresAck}
                disabled={sending || uncertain}
                onChange={event => setRequiresAck(event.currentTarget.checked)}
              />
              <span>{t("Require ACK")}</span>
            </label>
          </div>
          {selectedSessionId && (
            <span className="window-collaboration-compose-context" title={selectedSessionId}>
              {t("Current Session")} · {shortId(selectedSessionId)}
            </span>
          )}
          <textarea
            aria-label={t("Message this Window")}
            placeholder={t("Send a message to this Window…")}
            value={message}
            maxLength={8000}
            rows={2}
            disabled={sending || uncertain || !state.transcript?.can_send}
            onChange={event => setMessage(event.currentTarget.value)}
            onKeyDown={event => {
              if (!event.nativeEvent.isComposing && (event.metaKey || event.ctrlKey) && event.key === "Enter") {
                event.preventDefault();
                submit();
              }
            }}
          />
          <div className="window-collaboration-compose-footer">
            <small>{t("⌘/Ctrl + Enter to send")}</small>
            <button type="submit" disabled={sending || !message.trim() || !state.transcript?.can_send}>
              <Send size={14} />
              {sending ? t("Sending…") : uncertain ? t("Retry") : t("Send")}
            </button>
          </div>
        </div>
        {uncertain && (
          <p className="window-collaboration-send-status" role="status">
            {t("Send status unknown. Retry this message.")}
          </p>
        )}
        {!uncertain && sendError && (
          <p className="window-collaboration-send-status error" role="status">{sendError}</p>
        )}
      </form>
    </section>
  );
}
