import { useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { clockTime, shortId } from "../model/format.js";
import { useWindowCollaboration } from "../state/useWindowCollaboration.js";

export function WindowCollaboration({ client, windowKey, selectedSessionId, language, onUnauthorized }: {
  client: RuntimeV2Client; windowKey: string; selectedSessionId: string; language: RuntimeLanguage; onUnauthorized: () => void;
}) {
  const zh = language === "zh-CN";
  const [message, setMessage] = useState("");
  const state = useWindowCollaboration(client, windowKey, onUnauthorized);
  return <section className="window-collaboration" aria-label={zh ? "协作" : "Collaboration"}>
    {state.error && <p role="status">{zh ? "暂时无法刷新消息。" : "Messages could not be refreshed."}</p>}
    {state.transcript?.messages.map(row => <article className="window-collaboration-message" key={row.message_id}>
      <strong>{row.source === "operator" ? (zh ? "你" : "You") : row.direction === "outbound"
        ? (zh ? "此窗口 → Window · " : "This Window → Window · ") + shortId((row.peer_id || "").replace(/^wc_peer_/, ""))
        : "Window · " + shortId((row.peer_id || "").replace(/^wc_peer_/, ""))}</strong>
      <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{row.message}</p>
      <small>{clockTime(row.created_at_ms)} · {row.first_ack_observed_at_ms != null ? (zh ? "已确认" : "Acknowledged")
        : row.first_projected_at_ms != null ? (zh ? "已送达" : "Delivered") : (zh ? "已发送" : "Sent")}</small>
      {row.context_session_id && <small className="quiet-pill">Context · {shortId(row.context_session_id)}</small>}
    </article>)}
    {state.transcript && !state.transcript.messages.length && <p>{zh ? "暂无消息" : "No messages yet"}</p>}
    {state.transcript?.truncated && <p>{zh ? "显示最近的消息" : "Showing recent messages"}</p>}
    <form onSubmit={event => { event.preventDefault(); void state.send(message.trim(), selectedSessionId || null).then(ok => { if (ok) setMessage(""); }); }}>
      <textarea aria-label={zh ? "给这个窗口发消息" : "Message this Window"} placeholder={zh ? "给这个窗口发消息…" : "Send a message to this Window…"}
        value={message} maxLength={8000} disabled={state.sending || state.uncertain || !state.transcript?.can_send}
        onChange={event => setMessage(event.currentTarget.value)} />
      <button type="submit" disabled={state.sending || !message.trim() || !state.transcript?.can_send}>
        {state.sending ? (zh ? "发送中…" : "Sending…") : state.uncertain ? (zh ? "重试" : "Retry") : (zh ? "发送" : "Send")}
      </button>
    </form>
    {state.uncertain && <p role="status">{zh ? "尚未确认发送结果，请重试此消息。" : "Send status unknown. Retry this message."}</p>}
  </section>;
}
