import { ArrowUpRight, Check, LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { clearDraft, loadDraft, saveDraft } from "../../runtime_storage.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { SessionLocation, SessionWorkspaceState } from "../state/useSessionWorkspace.js";

type Props = {
  location: SessionLocation;
  session: SessionWorkspaceState;
  language: RuntimeLanguage;
};

export function SessionComposer({ location, session, language }: Props) {
  const t = (value: string) => translate(value, language);
  const [composer, setComposer] = useState("");
  const [kind, setKind] = useState("note");
  const [priority, setPriority] = useState("normal");
  const [requiresAck, setRequiresAck] = useState(false);
  const [editingMessageId, setEditingMessageId] = useState("");
  const [replyTo, setReplyTo] = useState("");
  const [replyPreview, setReplyPreview] = useState("");

  useEffect(() => {
    setComposer(loadDraft(location.projectId, location.sessionId));
    setEditingMessageId("");
    setReplyTo("");
    setReplyPreview("");
  }, [location.projectId, location.sessionId]);

  useEffect(() => {
    if (!editingMessageId) saveDraft(location.projectId, location.sessionId, composer);
  }, [composer, editingMessageId, location.projectId, location.sessionId]);

  useEffect(() => {
    const handler = (event: Event) => {
      const custom = event as CustomEvent<{ messageId?: string; message?: string }>;
      if (!custom.detail?.messageId) return;
      setEditingMessageId(custom.detail.messageId);
      setReplyTo("");
      setReplyPreview("");
      setComposer(custom.detail.message || "");
    };
    window.addEventListener("webcodex-runtime-edit-message", handler);
    return () => window.removeEventListener("webcodex-runtime-edit-message", handler);
  }, []);

  useEffect(() => {
    const handler = (event: Event) => {
      const custom = event as CustomEvent<{ messageId?: string; message?: string }>;
      if (!custom.detail?.messageId) return;
      setEditingMessageId("");
      setReplyTo(custom.detail.messageId);
      setReplyPreview(custom.detail.message || "");
    };
    window.addEventListener("webcodex-runtime-reply-message", handler);
    return () => window.removeEventListener("webcodex-runtime-reply-message", handler);
  }, []);

  const submit = async () => {
    if (!composer.trim()) return;
    const ok = editingMessageId
      ? await session.replace(editingMessageId, composer)
      : await session.send({ message: composer, kind, priority, requiresAck, replyTo: replyTo || undefined });
    if (!ok) return;
    if (!editingMessageId) clearDraft(location.projectId, location.sessionId);
    setComposer("");
    setEditingMessageId("");
    setReplyTo("");
    setReplyPreview("");
  };

  const cancelEdit = () => {
    setEditingMessageId("");
    setComposer(loadDraft(location.projectId, location.sessionId));
  };

  const cancelReply = () => {
    setReplyTo("");
    setReplyPreview("");
  };

  return (
    <div className="composer-row">
      <div className="composer">
        {editingMessageId && (
          <div className="composer-context">
            <span>{t("Editing retained message")}</span>
            <button type="button" onClick={cancelEdit} aria-label={t("Cancel edit")}><X size={14} /></button>
          </div>
        )}
        {replyTo && !editingMessageId && (
          <div className="composer-context">
            <span>{t("Replying to")}: {replyPreview.slice(0, 120)}</span>
            <button type="button" onClick={cancelReply} aria-label={t("Cancel reply")}><X size={14} /></button>
          </div>
        )}
        {session.mutationNotice && <div className="composer-notice" role="status">{t(session.mutationNotice)}</div>}
        <textarea
          aria-label={t("Send a message to this work session…")}
          placeholder={t("Send a message to this work session…")}
          rows={1}
          value={composer}
          onChange={(event) => setComposer(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              void submit();
            }
          }}
        />
        <div className="composer-footer">
          <details className="composer-options">
            <summary>{t("Options")}</summary>
            <div className="composer-options-popover">
              <label>
                {t("Kind")}
                <select value={kind} onChange={(event) => setKind(event.target.value)}>
                  <option value="note">note</option>
                  <option value="progress">progress</option>
                  <option value="guidance">guidance</option>
                  <option value="question">question</option>
                  <option value="risk">risk</option>
                  <option value="todo">todo</option>
                </select>
              </label>
              <label>
                {t("Priority")}
                <select value={priority} onChange={(event) => setPriority(event.target.value)}>
                  <option value="normal">normal</option>
                  <option value="high">high</option>
                </select>
              </label>
              <label className="checkbox-line">
                <input type="checkbox" checked={requiresAck} onChange={(event) => setRequiresAck(event.target.checked)} />
                {t("Requires acknowledgement")}
              </label>
            </div>
          </details>
          <button
            className="send-button"
            type="button"
            onClick={() => void submit()}
            disabled={!composer.trim() || session.sending}
            aria-label={editingMessageId ? t("Save") : t("Send")}
          >
            {session.sending ? <LoaderCircle size={16} /> : editingMessageId ? <Check size={16} /> : <ArrowUpRight size={16} />}
          </button>
        </div>
      </div>
    </div>
  );
}
