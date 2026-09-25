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
import { SessionCollaboration } from "./SessionCollaboration.js";


type Props = {
  item: WorkItem;
  location: SessionLocation;
  session: SessionWorkspaceState;
  language: RuntimeLanguage;
  onOpenWindow?: (windowKey: string) => void;
};

export function SessionExecution({ item, location, session, language, onOpenWindow }: Props) {
  const t = (value: string) => translate(value, language);
  const progress = groupRecentProgress(session.detail);
  const signals = activitySignals(session.detail, item);
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
            <div className="session-work-context">
              <span title={location.projectId}>{t("Project")}: {location.projectName}</span>
              <code title={location.sessionId}>{shortId(location.sessionId)}</code>
              <span title={absoluteTime(item.updatedAt)}>{t("Updated")}: {absoluteTime(item.updatedAt)}</span>
              {session.detail?.linked_windows.map((row) => <button className="text-button" type="button" key={row.client_window_key} disabled={!onOpenWindow} onClick={() => onOpenWindow?.(row.client_window_key)} title={row.client_window_key}>
                <Monitor size={14} /> {t("Window")} {shortId(row.client_window_key)} · {relativeTime(row.last_seen_at_ms)}
              </button>)}
            </div>
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

            <div className="workflow-support">
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
            </div>

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
        <SessionCollaboration location={location} session={session} language={language} />
      </section>
    </main>
  );
}
