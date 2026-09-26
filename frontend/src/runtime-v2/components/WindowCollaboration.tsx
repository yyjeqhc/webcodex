import { Bot, CheckCircle2, MessageSquare, Send, UserRound } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import type { WindowCollaborationMessage } from "../api/windowCollaboration.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { clockTime, shortId } from "../model/format.js";
import { useWindowCollaboration } from "../state/useWindowCollaboration.js";

function deliveryLabel(row: WindowCollaborationMessage, zh: boolean): string | null {
  if (row.source === "peer" && row.direction === "inbound") return null;
  if (row.first_ack_observed_at_ms != null) return zh ? "已确认" : "Acknowledged";
  if (row.first_projected_at_ms != null) return zh ? "已送达" : "Delivered";
  return zh ? "已发送" : "Sent";
}

function participantLabel(row: WindowCollaborationMessage, zh: boolean): string {
  if (row.source === "operator") return zh ? "你" : "You";
  if (row.direction === "outbound") return zh ? "此窗口 → 其他窗口" : "This Window → Peer Window";
  return zh ? "其他窗口" : "Peer Window";
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
    node.scrollTop = node.scrollHeight;
  }, [messages.length]);

  const submit = () => {
    const text = message.trim();
    if (!text || sending || !state.transcript?.can_send) return;
    void state.send(text, selectedSessionId || null).then(ok => {
      if (ok) setMessage("");
    });
  };

  return (
    <section className="window-collaboration" aria-label={zh ? "协作" : "Collaboration"}>
      <header className="window-collaboration-toolbar">
        <span className="window-collaboration-title-icon"><MessageSquare size={16} /></span>
        <div>
          <strong>{zh ? "协作" : "Collaboration"}</strong>
          <small>
            {messages.length
              ? `${messages.length} ${zh ? "条消息" : messages.length === 1 ? "message" : "messages"}`
              : (zh ? "给这个窗口留下消息或指令" : "Leave a message or instruction for this Window")}
          </small>
        </div>
        {selectedSessionId && (
          <span className="window-collaboration-context" title={selectedSessionId}>
            {zh ? "上下文" : "Context"} · {shortId(selectedSessionId)}
          </span>
        )}
      </header>

      <div className="window-collaboration-thread" ref={threadRef} aria-live="polite">
        {state.error && (
          <div className="window-collaboration-banner" role="status">
            {zh ? "暂时无法刷新消息。" : "Messages could not be refreshed."}
          </div>
        )}

        {messages.map(row => {
          const status = deliveryLabel(row, zh);
          const outgoing = row.source === "operator" || row.direction === "outbound";
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
                {(status || row.context_session_id) && (
                  <footer>
                    {status && (
                      <span className="window-collaboration-delivery">
                        <CheckCircle2 size={12} /> {status}
                      </span>
                    )}
                    {row.context_session_id && (
                      <span className="window-collaboration-session" title={row.context_session_id}>
                        Session · {shortId(row.context_session_id)}
                      </span>
                    )}
                  </footer>
                )}
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
              if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
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
