import {
  Bot,
  ChevronDown,
  CircleDot,
  Clock3,
  LoaderCircle,
  MessageSquare,
  TerminalSquare,
} from "lucide-react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { relativeTime, shortId } from "../model/format.js";
import { groupRecentProgress, type WorkItem } from "../model/work.js";
import type { SessionLocation, SessionWorkspaceState } from "../state/useSessionWorkspace.js";
import { BUCKET_LABEL } from "./WorkList.js";
import { ProgressCluster } from "./ProgressCluster.js";
import { SessionComposer } from "./SessionComposer.js";

const MUTABLE_MESSAGE_KINDS = new Set(["note", "guidance", "question", "todo"]);

type Props = {
  item: WorkItem;
  location: SessionLocation;
  session: SessionWorkspaceState;
  language: RuntimeLanguage;
};

export function SessionExecution({ item, location, session, language }: Props) {
  const t = (value: string) => translate(value, language);
  const progress = groupRecentProgress(session.detail);

  return (
    <main className="session-main">
      <header className="session-header">
        <div className="session-heading">
          <div className="breadcrumbs"><span>{location.runner}</span><span>/</span><span>{location.projectName}</span></div>
          <h2>{item.title}</h2>
        </div>
        <div className="session-actions">
          <span className={"quiet-pill " + (item.bucket === "running" ? "running" : "")}>
            <CircleDot size={12} /> {t(BUCKET_LABEL[item.bucket])} · {relativeTime(item.updatedAt)}
          </span>
          <button className="icon-button" type="button" onClick={session.refresh} aria-label={t("Refresh")}><Clock3 size={16} /></button>
        </div>
      </header>

      <div className="timeline-scroll">
        <div className="timeline-measure">
          <div className="task-run">
            <section className="task-prompt">
              <div className="task-prompt-label"><MessageSquare size={14} /> {t("Task")}</div>
              <p>{item.title}</p>
            </section>

            <section className="run-status-card">
              <div className="run-status-head">
                <span className="run-spinner">{item.bucket === "running" ? <LoaderCircle size={17} /> : <CircleDot size={17} />}</span>
                <div>
                  <strong>{item.bucket === "running" ? t("Working") : t(BUCKET_LABEL[item.bucket])}</strong>
                  <span>{item.phase}</span>
                </div>
                {session.detailAvailability === "stale" ? (
                  <span className="live-badge stale">{t("stale")}</span>
                ) : item.bucket === "running" ? (
                  <span className="live-badge"><span /> {t("live")}</span>
                ) : null}
              </div>

              {session.detail && (
                <div className="evidence-progress-grid" aria-label={t("Progress from retained evidence")}>
                  <div><strong>{session.detail.overview.work.exploration}</strong><span>{t("Explored")}</span></div>
                  <div><strong>{session.detail.overview.work.edits}</strong><span>{t("Edited")}</span></div>
                  <div><strong>{session.detail.overview.work.runs}</strong><span>{t("Ran")}</span></div>
                  <div><strong>{session.detail.overview.work.validations}</strong><span>{t("Tested")}</span></div>
                  <div><strong>{session.detail.overview.work.reviews}</strong><span>{t("Reviewed")}</span></div>
                </div>
              )}
            </section>

            {(item.currentActivity || item.runningJobs > 0) && (
              <section className="active-command">
                <div className="active-command-head">
                  <span><TerminalSquare size={15} /></span>
                  <div>
                    <strong>{t("Current execution")}</strong>
                    <code>
                      {item.currentActivity?.summary ||
                        item.currentActivity?.tool ||
                        item.currentActivity?.kind ||
                        String(item.runningJobs) + " running Job" + (item.runningJobs === 1 ? "" : "s")}
                    </code>
                  </div>
                  <span className="command-running"><LoaderCircle size={13} /> {t("running")}</span>
                </div>
                <div className="active-command-foot">
                  <span>{item.currentActivity?.tool || t("Session execution evidence")}</span>
                  <span>
                    {item.currentActivity?.job_id
                      ? "Job " + shortId(item.currentActivity.job_id)
                      : String(item.runningJobs) + " " + t("Running Jobs")}
                  </span>
                </div>
              </section>
            )}

            <section className="progress-section">
              <div className="progress-heading">
                <span>{t("Recent progress")}</span>
                <small>{t("Low-level calls grouped by intent")}</small>
              </div>
              <div className="timeline-clusters">
                {progress.length ? (
                  progress.map((group, index) => (
                    <ProgressCluster key={group.intent + "-" + group.latestAt + "-" + index} group={group} />
                  ))
                ) : (
                  <div className="empty-inline">
                    {session.detailAvailability === "loading"
                      ? t("Loading work evidence…")
                      : t("No retained activity in this Session.")}
                  </div>
                )}
              </div>
            </section>

            {item.reportedProgress?.text && (
              <article className="agent-working-note">
                <span className="message-avatar agent"><Bot size={15} /></span>
                <div>
                  <div className="message-meta">
                    <strong>{t("Agent progress report")}</strong>
                    <time>{relativeTime(item.reportedProgress.reported_at)}</time>
                  </div>
                  <p>{item.reportedProgress.text}</p>
                </div>
              </article>
            )}

            <details className="session-communication">
              <summary>
                <MessageSquare size={15} />
                <strong>{t("Session communication")}</strong>
                <span>{session.messages?.messages.length || 0}</span>
                <ChevronDown size={15} />
              </summary>
              <div className="message-list">
                {session.messages?.messages.map((message) => (
                  <article className="retained-message" key={message.message_id}>
                    <div className="message-meta">
                      <strong>{message.author_session_id ? t("Agent / Session") : t("Retained message")}</strong>
                      <time>{relativeTime(message.created_at)}</time>
                    </div>
                    <p>{message.message}</p>
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
                {session.messagesAvailability === "denied" && (
                  <p className="muted-copy">{t("Session messages are not available with this access key.")}</p>
                )}
                {session.messages?.messages.length === 0 && (
                  <p className="muted-copy">{t("No retained Session messages.")}</p>
                )}
              </div>
            </details>
          </div>
        </div>
      </div>

      <SessionComposer location={location} session={session} language={language} />
    </main>
  );
}
