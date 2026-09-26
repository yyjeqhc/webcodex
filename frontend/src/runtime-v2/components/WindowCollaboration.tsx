import { useState } from "react";
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

export function WindowCollaboration({ client, windowKey, selectedSessionId, language, onUnauthorized }: {
  client: RuntimeV2Client; windowKey: string; selectedSessionId: string; language: RuntimeLanguage; onUnauthorized: () => void;
}) {
  const zh = language === "zh-CN";
  const [message, setMessage] = useState("");
  const state = useWindowCollaboration(client, windowKey, onUnauthorized);
  const sending = state.sendState === "sending";
  const uncertain = state.sendState === "uncertain";
  const sendError = state.sendError === "conflict" ? (zh ? "消息未能发送，请重新发送。" : "Message could not be sent. Send it again.")
    : state.sendError === "context" ? (zh ? "上下文已不可用。请重新选择 Session 或全部调用后再发送。" : "Context is no longer available. Choose a Session or All calls, then send again.")
    : state.sendError === "unavailable" ? (zh ? "此窗口当前无法协作。" : "Collaboration is unavailable for this Window.")
    : state.sendError === "failed" ? (zh ? "消息未能发送。" : "Message could not be sent.") : null;
  return <section className="window-collaboration" aria-label={zh ? "协作" : "Collaboration"}>
    {state.error && <p role="status">{zh ? "暂时无法刷新消息。" : "Messages could not be refreshed."}</p>}
    {state.transcript?.messages.map(row => {
      const status = deliveryLabel(row, zh);
      return <article className="window-collaboration-message" key={row.message_id}>
        <strong>{row.source === "operator" ? (zh ? "你" : "You") : row.direction === "outbound"
          ? (zh ? "此窗口 → Window · " : "This Window → Window · ") + shortId((row.peer_id || "").replace(/^wc_peer_/, ""))
          : "Window · " + shortId((row.peer_id || "").replace(/^wc_peer_/, ""))}</strong>
        <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{row.message}</p>
        <small>{clockTime(row.created_at_ms)}{status ? ` · ${status}` : ""}</small>
        {row.context_session_id && <small className="quiet-pill">Session · {shortId(row.context_session_id)}</small>}
      </article>;
    })}
    {state.transcript && !state.transcript.messages.length && <p>{zh ? "暂无消息" : "No messages yet"}</p>}
    {state.transcript?.truncated && <p>{zh ? "显示最近的消息" : "Showing recent messages"}</p>}
    <form onSubmit={event => { event.preventDefault(); void state.send(message.trim(), selectedSessionId || null).then(ok => { if (ok) setMessage(""); }); }}>
      <textarea aria-label={zh ? "给这个窗口发消息" : "Message this Window"} placeholder={zh ? "给这个窗口发消息…" : "Send a message to this Window…"}
        value={message} maxLength={8000} disabled={sending || uncertain || !state.transcript?.can_send}
        onChange={event => setMessage(event.currentTarget.value)} />
      <button type="submit" disabled={sending || !message.trim() || !state.transcript?.can_send}>
        {sending ? (zh ? "发送中…" : "Sending…") : uncertain ? (zh ? "重试" : "Retry") : (zh ? "发送" : "Send")}
      </button>
    </form>
    {uncertain && <p role="status">{zh ? "发送状态未知，请重试此消息。" : "Send status unknown. Retry this message."}</p>}
    {!uncertain && sendError && <p role="status">{sendError}</p>}
  </section>;
}
