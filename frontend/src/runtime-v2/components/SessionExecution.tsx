import {
  Bot,
  CircleDot,
  Clock3,
  HardDrive,
  LoaderCircle,
  MessageSquare,
  Monitor,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, relativeTime, shortId } from "../model/format.js";
import { activitySignals, groupRecentProgress, type WorkItem } from "../model/work.js";
import type { SessionLocation, SessionWorkspaceState } from "../state/useSessionWorkspace.js";
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
  const signals = activitySignals(session.detail, item);
  const messagesById = new Map((session.messages?.messages || []).map((message) => [message.message_id, message]));
  const [centerTab, setCenterTab] = useState<"workflow" | "collaboration">("workflow");

  useEffect(() => {
    setCenterTab("workflow");
  }, [location.projectId, location.sessionId]);

  useEffect(() => {
    const openCollaboration = () => setCenterTab("collaboration");
    window.addEventListener("webcodex-runtime-compose-message", openCollaboration);
    return () => window.removeEventListener("webcodex-runtime-compose-message", openCollaboration);
  }, []);

  return (
    <main className="session-main">
      <header className="session-header">
        <div className="session-heading">
          <div className="breadcrumbs"><span>{location.runner}</span><span>/</span><span>{location.projectName}</span></div>
          <h2>{item.title}</h2>
        </div>
        <div className="session-actions">
          <span className="quiet-pill">
            <CircleDot size={12} /> {t("Workflow Session")} · {t(item.lifecycle)} · {relativeTime(item.updatedAt)}
          </span>
          <button className="icon-button" type="button" onClick={session.refresh} aria-label={t("Refresh")}><Clock3 size={16} /></button>
        </div>
      </header>

      <div className="session-view-tabs" role="tablist" aria-label={t("Work view")}>
        <button
          id="workflow-tab"
          role="tab"
          aria-selected={centerTab === "workflow"}
          aria-controls="workflow-panel"
          className={centerTab === "workflow" ? "active" : ""}
          type="button"
          onClick={() => setCenterTab("workflow")}
        >
          {t("Workflow")}
        </button>
        <button
          id="collaboration-tab"
          role="tab"
          aria-selected={centerTab === "collaboration"}
          aria-controls="collaboration-panel"
          className={centerTab === "collaboration" ? "active" : ""}
          type="button"
          onClick={() => setCenterTab("collaboration")}
        >
          {t("Collaboration")}
          <span>{session.messages?.messages.length || 0}</span>
        </button>
      </div>

      <div
        id="workflow-panel"
        className="timeline-scroll session-center-pane workflow-pane"
        role="tabpanel"
        aria-labelledby="workflow-tab"
        hidden={centerTab !== "workflow"}
      >
        <div className="timeline-measure">
          <div className="task-run">
            <section className="task-prompt">
              <div className="task-prompt-label"><MessageSquare size={14} /> {t("Task")}</div>
              <p>{item.title}</p>
            </section>

            <section className="activity-signals-card" aria-label={t("Activity signals")}>
              <div className="activity-signals-heading">
                <div><strong>{t("Activity signals")}</strong><small>{t("Independent evidence layers; sparse Session links never imply Window idleness.")}</small></div>
                {session.detailAvailability === "stale" && <span className="live-badge stale">{t("stale")}</span>}
              </div>
              <div className="activity-signal-list">
                {signals.map((signal) => {
                  const icon = signal.source === "window" ? <Monitor size={15} />
                    : signal.source === "workspace" ? <HardDrive size={15} />
                    : signal.source === "job" ? <TerminalSquare size={15} />
                    : <CircleDot size={15} />;
                  return (
                    <div className="activity-signal-row" data-testid={"activity-signal-" + signal.source} key={signal.source}>
                      <span className={"activity-signal-icon " + signal.source}>{icon}</span>
                      <span className="activity-signal-copy">
                        <strong>{t(signal.label)}</strong>
                        <small>{t(signal.detail)}</small>
                      </span>
                      <span className={"activity-signal-status " + signal.tone}>{t(signal.status)}</span>
                      <time>{signal.observedAt !== undefined ? absoluteTime(signal.observedAt) : "—"}</time>
                    </div>
                  );
                })}
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

            {item.runningJobs > 0 && (
              <section className="active-command">
                <div className="active-command-head">
                  <span><TerminalSquare size={15} /></span>
                  <div>
                    <strong>{t("Current execution")}</strong>
                    <code>{String(item.runningJobs) + " " + t("Running Jobs")}</code>
                  </div>
                  <span className="command-running"><LoaderCircle size={13} /> {t("running")}</span>
                </div>
                <div className="active-command-foot">
                  <span>{t("Runner-owned Job execution")}</span>
                  <span>{String(item.runningJobs) + " " + t("Running Jobs")}</span>
                </div>
              </section>
            )}

            <section className="progress-section">
              <div className="progress-heading">
                <span>{t("Activity timeline")}</span>
                <small>{t("Unified Session, Window, Workspace, and Job evidence")}</small>
              </div>
              <div className="timeline-clusters">
                {progress.length ? (
                  progress.map((group, index) => (
                    <ProgressCluster key={group.source + "-" + group.intent + "-" + group.latestAt + "-" + index} group={group} language={language} />
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

            {session.detail?.activity_truncated && (
              <div className="inventory-note wide">{t("Session activity history is bounded by the retained ledger.")}</div>
            )}
            {session.detail?.window_activity_after_last_session_record_truncated && (
              <div className="inventory-note wide">{t("Window activity reached the server history bound; older Window evidence may be omitted.")}</div>
            )}

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
          </div>
        </div>
      </div>

      <section
        id="collaboration-panel"
        className="collaboration-workspace session-center-pane"
        role="tabpanel"
        aria-labelledby="collaboration-tab"
        hidden={centerTab !== "collaboration"}
      >
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
      </section>
    </main>
  );
}
