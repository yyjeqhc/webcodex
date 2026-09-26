import { Bot, CheckCircle2, MessageSquare, Send, UserRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import type {
  WindowCollaborationKind,
  WindowCollaborationMessage,
  WindowCollaborationPriority,
} from "../api/windowCollaboration.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { clockTime, shortId } from "../model/format.js";
import { useWindowCollaboration } from "../state/useWindowCollaboration.js";

function deliveryLabel(row: WindowCollaborationMessage, zh: boolean): string | null {
  if (row.source === "peer" && row.direction === "inbound") return null;
  if (row.source === "window") return null;
  if (row.first_ack_observed_at_ms != null) return zh ? "已确认" : "Acknowledged";
  if (row.first_projected_at_ms != null) return zh ? "已附入工具结果" : "Included in tool result";
  return zh ? "已保存" : "Saved";
}

function participantLabel(row: WindowCollaborationMessage, zh: boolean): string {
  if (row.source === "operator") return zh ? "你" : "You";
  if (row.source === "window") return zh ? "此窗口" : "This Window";
  if (row.direction === "outbound") return zh ? "此窗口 → 其他窗口" : "This Window → Peer Window";
  return zh ? "其他窗口" : "Peer Window";
}

function kindLabel(kind: WindowCollaborationKind, zh: boolean): string {
  const labels: Record<WindowCollaborationKind, [string, string]> = {
    note: ["备注", "Note"],
    proposal: ["建议", "Proposal"],
    question: ["问题", "Question"],
    answer: ["回答", "Answer"],
    decision: ["决定", "Decision"],
    risk: ["风险", "Risk"],
    progress: ["进展", "Progress"],
    guidance: ["指令", "Guidance"],
    todo: ["待办", "Todo"],
  };
  return labels[kind]?.[zh ? 0 : 1] || kind;
}

function priorityLabel(priority: WindowCollaborationPriority, zh: boolean): string {
  return priority === "high"
    ? (zh ? "高优先级" : "High priority")
    : priority === "low"
      ? (zh ? "低优先级" : "Low priority")
      : (zh ? "普通" : "Normal");
}

export function WindowCollaboration({ client, windowKey, selectedSessionId, language, onUnauthorized }: {
  client: RuntimeV2Client;
  windowKey: string;
  selectedSessionId: string;
  language: RuntimeLanguage;
  onUnauthorized: () => void;
}) {
  const zh = language === "zh-CN";
  const [message, setMessage] = useState("");
  const [messageKind, setMessageKind] = useState<WindowCollaborationKind>("guidance");
  const [priority, setPriority] = useState<WindowCollaborationPriority>("normal");
  const [requiresAck, setRequiresAck] = useState(true);
  const followLatest = useRef(true);
  const [hasNewMessages, setHasNewMessages] = useState(false);
  const threadRef = useRef<HTMLDivElement | null>(null);
  const state = useWindowCollaboration(client, windowKey, onUnauthorized);
  const sending = state.sendState === "sending";
  const uncertain = state.sendState === "uncertain";
  const messages = state.transcript?.messages || [];
  const sendError = state.sendError === "conflict"
    ? (zh ? "消息未能发送，请重新发送。" : "Message could not be sent. Send it again.")
    : state.sendError === "context"
      ? (zh ? "上下文已不可用。请重新选择 Session 或全部调用后再发送。" : "Context is no longer available. Choose a Session or All calls, then send again.")
      : state.sendError === "unavailable"
        ? (zh ? "此窗口当前无法协作。" : "Collaboration is unavailable for this Window.")
        : state.sendError === "failed"
          ? (zh ? "消息未能发送。" : "Message could not be sent.")
          : null;

  useEffect(() => {
    const node = threadRef.current;
    if (!node) return;
    if (followLatest.current) node.scrollTop = node.scrollHeight;
    else setHasNewMessages(true);
  }, [messages.at(-1)?.message_id]);

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
    <section className="window-collaboration" aria-label={zh ? "协作" : "Collaboration"}>
      <header className="window-collaboration-toolbar">
        <span className="window-collaboration-title-icon"><MessageSquare size={16} /></span>
        <div>
          <strong>{zh ? "协作" : "Collaboration"}</strong>
          <small aria-live="polite">
            {messages.length
              ? `${messages.length} ${zh ? "条消息" : messages.length === 1 ? "message" : "messages"}`
              : (zh ? "给这个窗口留下消息或指令" : "Leave a message or instruction for this Window")}
          </small>
          {hasNewMessages && <button type="button" className="text-button" onClick={() => {
            const node = threadRef.current;
            if (node) node.scrollTop = node.scrollHeight;
            followLatest.current = true;
            setHasNewMessages(false);
          }}>{zh ? "查看新消息" : "View new messages"}</button>}
        </div>
        {selectedSessionId && (
          <span className="window-collaboration-context" title={selectedSessionId}>
            {zh ? "上下文" : "Context"} · {shortId(selectedSessionId)}
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
            {zh ? "暂时无法刷新消息。" : "Messages could not be refreshed."}
          </div>
        )}

        {messages.map(row => {
          const status = deliveryLabel(row, zh);
          const outgoing =
            row.source === "operator" || (row.source === "peer" && row.direction === "outbound");
          return (
            <article
              className={"window-collaboration-message " + (outgoing ? "outgoing" : "incoming")}
              key={row.message_id}
            >
              <span className="window-collaboration-avatar" aria-hidden="true">
                {row.source === "operator" ? <UserRound size={15} /> : <Bot size={15} />}
              </span>
              <div className="window-collaboration-bubble">
                <header>
                  <strong title={row.peer_id || undefined}>{participantLabel(row, zh)}</strong>
                  <time>{clockTime(row.created_at_ms)}</time>
                </header>
                <p>{row.message}</p>
                <footer>
                  <span className="window-collaboration-meta-chip">{kindLabel(row.kind, zh)}</span>
                  {row.priority !== "normal" && (
                    <span className={"window-collaboration-meta-chip priority-" + row.priority}>
                      {priorityLabel(row.priority, zh)}
                    </span>
                  )}
                  {status && (
                    <span className="window-collaboration-delivery">
                      <CheckCircle2 size={12} /> {status}
                    </span>
                  )}
                  {row.requires_ack && row.first_ack_observed_at_ms == null && (
                    <span className="window-collaboration-meta-chip ack">
                      {zh ? "要求确认" : "ACK requested"}
                    </span>
                  )}
                  {row.context_session_id && (
                    <span className="window-collaboration-session" title={row.context_session_id}>
                      Session · {shortId(row.context_session_id)}
                    </span>
                  )}
                </footer>
              </div>
            </article>
          );
        })}

        {state.transcript && !messages.length && (
          <div className="window-collaboration-empty">
            <span><MessageSquare size={20} /></span>
            <strong>{zh ? "暂无消息" : "No messages yet"}</strong>
            <small>{zh ? "在这里给这个窗口留下消息或指令。" : "Send a note or instruction to this Window."}</small>
          </div>
        )}
        {state.transcript?.truncated && (
          <div className="window-collaboration-history-note">
            {zh ? "当前显示最近的消息" : "Showing recent messages"}
          </div>
        )}
      </div>

      <form
        className="window-collaboration-composer"
        onSubmit={event => {
          event.preventDefault();
          submit();
        }}
      >
        <div className="window-collaboration-compose-box">
          <div className="window-collaboration-compose-options">
            <label>
              <span>{zh ? "类型" : "Type"}</span>
              <select
                aria-label={zh ? "消息类型" : "Message type"}
                value={messageKind}
                disabled={sending || uncertain}
                onChange={event => setMessageKind(event.currentTarget.value as WindowCollaborationKind)}
              >
                <option value="guidance">{zh ? "指令" : "Guidance"}</option>
                <option value="note">{zh ? "备注" : "Note"}</option>
                <option value="question">{zh ? "问题" : "Question"}</option>
                <option value="todo">{zh ? "待办" : "Todo"}</option>
              </select>
            </label>
            <label>
              <span>{zh ? "优先级" : "Priority"}</span>
              <select
                aria-label={zh ? "消息优先级" : "Message priority"}
                value={priority}
                disabled={sending || uncertain}
                onChange={event => setPriority(event.currentTarget.value as WindowCollaborationPriority)}
              >
                <option value="normal">{zh ? "普通" : "Normal"}</option>
                <option value="high">{zh ? "高" : "High"}</option>
                <option value="low">{zh ? "低" : "Low"}</option>
              </select>
            </label>
            <label className="window-collaboration-ack-toggle">
              <input
                type="checkbox"
                checked={requiresAck}
                disabled={sending || uncertain}
                onChange={event => setRequiresAck(event.currentTarget.checked)}
              />
              <span>{zh ? "要求确认" : "Require ACK"}</span>
            </label>
          </div>
          {selectedSessionId && (
            <span className="window-collaboration-compose-context" title={selectedSessionId}>
              {zh ? "关联当前 Session" : "Current Session"} · {shortId(selectedSessionId)}
            </span>
          )}
          <textarea
            aria-label={zh ? "给这个窗口发消息" : "Message this Window"}
            placeholder={zh ? "给这个窗口发消息…" : "Send a message to this Window…"}
            value={message}
            maxLength={8000}
            rows={3}
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
            <small>{zh ? "⌘/Ctrl + Enter 发送" : "⌘/Ctrl + Enter to send"}</small>
            <button type="submit" disabled={sending || !message.trim() || !state.transcript?.can_send}>
              <Send size={14} />
              {sending ? (zh ? "发送中…" : "Sending…") : uncertain ? (zh ? "重试" : "Retry") : (zh ? "发送" : "Send")}
            </button>
          </div>
        </div>
        {uncertain && (
          <p className="window-collaboration-send-status" role="status">
            {zh ? "发送状态未知，请重试此消息。" : "Send status unknown. Retry this message."}
          </p>
        )}
        {!uncertain && sendError && (
          <p className="window-collaboration-send-status error" role="status">{sendError}</p>
        )}
      </form>
    </section>
  );
}
