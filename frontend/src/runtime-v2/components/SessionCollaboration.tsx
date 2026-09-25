import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, relativeTime, shortId } from "../model/format.js";
import type { SessionLocation, SessionWorkspaceState } from "../state/useSessionWorkspace.js";
import { SessionComposer } from "./SessionComposer.js";

const MUTABLE_MESSAGE_KINDS = new Set(["note", "guidance", "question", "todo"]);

export function SessionCollaboration({ location, session, language }: {
  location: SessionLocation;
  session: SessionWorkspaceState;
  language: RuntimeLanguage;
}) {
  const t = (value: string) => translate(value, language);
  const messagesById = new Map((session.messages?.messages || []).map((message) => [message.message_id, message]));
  return <>
    <div className="collaboration-message-scroll" aria-label={t("Session communication")}>
      <div className="message-list">
        {session.messages?.messages.map((message) => (
          <article className="retained-message" key={message.message_id}>
            <div className="message-meta">
              <strong>{message.author_session_id ? t("Agent / Session") : t("Retained message")}</strong>
              <span className="message-kind">{t(message.kind)}</span>
              {message.requires_ack && (
                <span className={"message-state " + (message.first_ack_observed_at ? "good" : "warn")}>
                  {t(message.first_ack_observed_at ? "ACK observed" : "Awaiting ACK")}
                </span>
              )}
              {message.status !== "open" && (
                <span className="message-state resolved">
                  {t(message.closure_kind === "withdrawn" ? "Withdrawn" : message.closure_kind === "superseded" ? "Edited" : "Resolved")}
                </span>
              )}
              <time title={absoluteTime(message.created_at)}>{relativeTime(message.created_at)}</time>
            </div>
            {message.reply_to && (
              <div className="message-reply-context">
                <span>{t("Reply to")}</span>
                <span>{messagesById.get(message.reply_to)?.message.slice(0, 120) || shortId(message.reply_to)}</span>
              </div>
            )}
            <p>{message.message}</p>
            {message.first_ack_observed_at && (
              <div className="message-observation-note">
                {t("ACK first observed")} · <time title={absoluteTime(message.first_ack_observed_at)}>{relativeTime(message.first_ack_observed_at)}</time>
              </div>
            )}
            {message.resolution && (
              <div className="message-resolution">
                <div>
                  <strong>{t("Agent resolution")}</strong>
                  {message.resolved_at && <time title={absoluteTime(message.resolved_at)}>{relativeTime(message.resolved_at)}</time>}
                </div>
                <p>{message.resolution}</p>
              </div>
            )}
            <div className="message-actions">
              <button
                type="button"
                onClick={() => window.dispatchEvent(new CustomEvent("webcodex-runtime-reply-message", {
                  detail: { messageId: message.message_id, message: message.message },
                }))}
              >
                {t("Reply")}
              </button>
              {message.status === "open" && MUTABLE_MESSAGE_KINDS.has(message.kind) && session.mutationAllowed !== false && (
                <span className="message-mutable-hint">{t("Open · editable")}</span>
              )}
              {message.status === "open" && MUTABLE_MESSAGE_KINDS.has(message.kind) && session.mutationAllowed !== false && (
                <>
                  <button
                    type="button"
                    onClick={() => window.dispatchEvent(new CustomEvent("webcodex-runtime-edit-message", {
                      detail: { messageId: message.message_id, message: message.message },
                    }))}
                  >
                    {t("Edit")}
                  </button>
                  <button type="button" onClick={() => void session.withdraw(message.message_id)}>{t("Withdraw")}</button>
                </>
              )}
            </div>
          </article>
        ))}
        {session.messagesAvailability === "loading" && !session.messages && (
          <p className="muted-copy">{t("Loading Session messages…")}</p>
        )}
        {session.messagesAvailability === "denied" && (
          <p className="muted-copy">{t("Session messages are not available with this access key.")}</p>
        )}
        {session.messages?.messages.length === 0 && (
          <p className="muted-copy">{t("No retained Session messages.")}</p>
        )}
      </div>
    </div>
    <SessionComposer location={location} session={session} language={language} />
  </>;
}
