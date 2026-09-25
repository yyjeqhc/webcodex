import { displayProjectPath } from "../../ui/projectPresentation.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, durationText, shortId } from "../model/format.js";
import type { ProjectRow, WindowDetail } from "../model/types.js";
import { windowSessionCatalog } from "../model/windowSessions.js";

type Props = {
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  onOpenSession?: (sessionId: string) => void;
};

export function WindowActivityFeed({ detail, projects, language, onOpenSession }: Props) {
  const t = (value: string) => translate(value, language);
  const orderedSessions = windowSessionCatalog(detail);
  const sessionOrder = new Map(orderedSessions.map((session, index) => [session.workflow_session_id, index]));
  const sessionMeta = new Map(orderedSessions.map((session) => [session.workflow_session_id, session]));
  // Keep each invocation, including repeated observation calls. The trace only
  // reconciles a completed call with the same call in the live snapshot.
  const completedTraces = new Set(detail.activity.map((row) => row.server_trace_id).filter(Boolean));
  const calls = [
    ...detail.activity.map((row, index) => ({
      key: row.server_trace_id || `completed-${row.started_at_ms}-${index}`,
      tool: row.tool_name || row.method,
      project: row.project,
      startedAt: row.started_at_ms,
      duration: row.duration_ms,
      status: row.status,
      sessions: row.workflow_sessions
        .map((link) => link.workflow_session_id)
        .filter((sessionId, index, values) => values.indexOf(sessionId) === index)
        .sort((left, right) => (sessionOrder.get(left) ?? Number.MAX_SAFE_INTEGER) - (sessionOrder.get(right) ?? Number.MAX_SAFE_INTEGER)),
      running: false,
    })),
    ...detail.active_requests.filter((row) => !completedTraces.has(row.server_trace_id)).map((row) => ({
      key: row.server_trace_id,
      tool: row.tool_name || row.method,
      project: row.project,
      startedAt: row.started_at_ms,
      duration: row.elapsed_ms,
      status: "running",
      sessions: [] as string[],
      running: true,
    })),
  ].sort((a, b) => a.startedAt - b.startedAt);

  return (
    <section className="window-detail-section window-workflow-section" aria-label={t("Window activity")}>
      {detail.activity_truncated && <div className="inventory-note">{t("Earlier calls are not available in this view. Showing retained activity from oldest to newest.")}</div>}
      <div className="window-workflow-list">
        {calls.map((call) => {
          const project = projects.find((row) => row.id === call.project);
          const path = project?.path ? displayProjectPath(project.path) : undefined;
          const success = ["ok", "success", "succeeded"].includes(call.status);
          const failed = ["error", "failed", "failure"].includes(call.status);
          const status = call.running ? "Running" : success ? "Succeeded" : failed ? "Failed" : call.status;
          const primarySession = call.sessions.find((sessionId) => sessionMeta.has(sessionId));
          const selectable = Boolean(primarySession && onOpenSession);
          return (
            <article
              className={"window-call-card" + (call.running ? " running" : "") + (selectable ? " session-linked" : "")}
              data-testid="window-workflow-step"
              key={call.key}
              onClick={() => primarySession && onOpenSession?.(primarySession)}
            >
              <header>
                <strong>{call.tool}</strong>
                <span className={"status-pill " + (call.running ? "" : success ? "good" : "warn")}>{t(status)}</span>
              </header>
              {(path || call.sessions.length > 0) && (
                <div className="window-call-context">
                  {path && <span className="window-project-tag" data-testid="window-project-tag" title={path}>{path}</span>}
                  {call.sessions.map((sessionId) => {
                    const linked = sessionMeta.get(sessionId);
                    const tone = (sessionOrder.get(sessionId) ?? 0) % 8;
                    const tagSelectable = Boolean(linked && onOpenSession);
                    return (
                      <button
                        className="window-session-tag"
                        data-session-tone={tone}
                        data-testid="window-session-tag"
                        key={sessionId}
                        type="button"
                        disabled={!tagSelectable}
                        title={(linked?.title ? linked.title + " · " : "") + sessionId}
                        onClick={(event) => {
                          event.stopPropagation();
                          if (tagSelectable) onOpenSession?.(sessionId);
                        }}
                      >
                        <span className="window-session-color-dot" />
                        {shortId(sessionId, 14, 6)}
                      </button>
                    );
                  })}
                </div>
              )}
              <div className="window-call-timing">
                <span>{t("Started")} <time dateTime={new Date(call.startedAt).toISOString()} title={absoluteTime(call.startedAt)}>{absoluteTime(call.startedAt)}</time></span>
                <span>{t(call.running ? "Elapsed" : "Duration")} <strong>{durationText(call.duration)}</strong></span>
              </div>
            </article>
          );
        })}
        {!calls.length && <div className="empty-inline">{t("No tool calls yet")}</div>}
      </div>
    </section>
  );
}
