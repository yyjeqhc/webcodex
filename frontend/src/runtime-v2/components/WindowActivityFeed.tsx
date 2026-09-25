import { displayProjectPath } from "../../ui/projectPresentation.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, durationText } from "../model/format.js";
import type { ProjectRow, WindowDetail } from "../model/types.js";

type Props = {
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
};

export function WindowActivityFeed({ detail, projects, language }: Props) {
  const t = (value: string) => translate(value, language);
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
      running: false,
    })),
    ...detail.active_requests.filter((row) => !completedTraces.has(row.server_trace_id)).map((row) => ({
      key: row.server_trace_id,
      tool: row.tool_name || row.method,
      project: row.project,
      startedAt: row.started_at_ms,
      duration: row.elapsed_ms,
      status: "running",
      running: true,
    })),
  ].sort((a, b) => a.startedAt - b.startedAt);

  return (
    <section className="window-detail-section window-workflow-section" aria-label={t("Window activity")}>
      <div className="section-heading activity-feed-heading">
        <div><h2>{t("Tool calls")}</h2><p>{t("Each call is shown separately, from first to last.")}</p></div>
      </div>
      {detail.activity_truncated && <div className="inventory-note">{t("Earlier calls are not available in this view. Showing retained activity from oldest to newest.")}</div>}
      <div className="window-workflow-list">
        {calls.map((call) => {
          const project = projects.find((row) => row.id === call.project);
          const path = project?.path ? displayProjectPath(project.path) : undefined;
          const success = ["ok", "success", "succeeded"].includes(call.status);
          const failed = ["error", "failed", "failure"].includes(call.status);
          const status = call.running ? "Running" : success ? "Succeeded" : failed ? "Failed" : call.status;
          return (
            <article className={"window-call-card" + (call.running ? " running" : "")} data-testid="window-workflow-step" key={call.key}>
              <header>
                <strong>{call.tool}</strong>
                <span className={"status-pill " + (call.running ? "" : success ? "good" : "warn")}>{t(status)}</span>
              </header>
              {path && <div className="window-call-context"><span className="window-project-tag" data-testid="window-project-tag" title={path}>{path}</span></div>}
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
