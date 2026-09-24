import { useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, durationText, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import type { ProjectRow, WindowActivity, WindowDetail } from "../model/types.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";

type Props = {
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  onOpenSession: (location: SessionLocation) => void;
};

// An invocation can record one Session and target another. Preserve both identities.
function activityProjects(activity: WindowActivity): string[] {
  return [...new Set([activity.project, ...activity.workflow_sessions.map((row) => row.project)]
    .filter((id): id is string => Boolean(id)))];
}

export function WindowActivityFeed({ detail, projects, language, onOpenSession }: Props) {
  const t = (value: string) => translate(value, language);
  const [projectFilter, setProjectFilter] = useState("");
  const projectIds = [...new Set([
    ...detail.activity.flatMap(activityProjects),
    ...detail.active_requests.flatMap((row) => row.project ? [row.project] : []),
  ])].sort();
  const projectName = (id: string) => projectDisplayName(projects.find((row) => row.id === id)?.name, id);
  const activity = detail.activity.filter((row) => !projectFilter || activityProjects(row).includes(projectFilter))
    .slice().sort((a, b) => b.started_at_ms - a.started_at_ms || b.ended_at_ms - a.ended_at_ms);
  const running = detail.active_requests.filter((row) => !projectFilter || row.project === projectFilter);

  const projectTags = (ids: string[]) => ids.length
    ? ids.map((id) => <span className="window-project-tag" data-testid="window-project-tag" key={id} title={id}>{projectName(id)}</span>)
    : <span className="muted-copy">{t("No Project evidence")}</span>;

  return (
    <section className="window-detail-section window-workflow-section">
      <div className="section-heading activity-feed-heading">
        <div><h2>{t("Tool activity")}</h2><p>{t("Newest first. Project, Session and timing stay visible.")}</p></div>
        <label className="activity-project-filter">
          <span>{t("Project filter")}</span>
          <select value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)}>
            <option value="">{t("All Projects in this Window")}</option>
            {projectIds.map((id) => <option key={id} value={id}>{projectName(id)} · {id}</option>)}
          </select>
        </label>
      </div>
      <div className="window-workflow-list">
        {running.map((request) => (
          <article className="window-call-card running" key={request.server_trace_id}>
            <header><strong>{request.tool_name || request.method}</strong><span className="status-pill good">{t("Running")}</span></header>
            <div className="window-call-context">{projectTags(request.project ? [request.project] : [])}</div>
            <div className="window-call-timing"><span>{t("Started")} <time title={absoluteTime(request.started_at_ms)}>{absoluteTime(request.started_at_ms)}</time></span><span>{t("Elapsed")} <strong>{durationText(request.elapsed_ms)}</strong></span></div>
          </article>
        ))}
        {activity.map((row, index) => (
          <article className="window-call-card" data-testid="window-workflow-step" key={row.server_trace_id || `${row.started_at_ms}-${index}`}>
            <header>
              <div><strong>{row.activity_presentation || row.tool_name || row.method}</strong>{row.tool_name && row.activity_presentation && row.activity_presentation !== row.tool_name && <code>{row.tool_name}</code>}</div>
              <span className={"status-pill " + (["ok", "success"].includes(row.status) ? "good" : "warn")}>{t(row.status)}</span>
            </header>
            <div className="window-call-context">{projectTags(activityProjects(row))}</div>
            <div className="window-call-sessions">
              {row.workflow_sessions.length ? row.workflow_sessions.map((relation) => {
                const linked = detail.linked_sessions.find((session) => session.workflow_session_id === relation.workflow_session_id);
                const project = projects.find((project) => project.id === (relation.project || linked?.project));
                return <button className="text-button" type="button" key={relation.workflow_session_id + ":" + relation.relation}
                  disabled={!project} title={relation.workflow_session_id}
                  onClick={() => project && onOpenSession({ projectId: project.id, projectName: project.name || project.id, runner: project.client_id, sessionId: relation.workflow_session_id })}>
                  {t("Session")} · {linked?.title || shortId(relation.workflow_session_id)} <small>{relation.relation} · {shortId(relation.workflow_session_id)}</small>
                </button>;
              }) : <span className="muted-copy">{t("No explicit Session link")}</span>}
            </div>
            <div className="window-call-timing">
              <span>{t("Started")} <time title={absoluteTime(row.started_at_ms)}>{absoluteTime(row.started_at_ms)}</time></span>
              <span>{t("Duration")} <strong>{durationText(row.duration_ms)}</strong></span>
              <span title={absoluteTime(row.ended_at_ms)}>{t("Finished")} {relativeTime(row.ended_at_ms)}</span>
            </div>
            <details className="window-call-evidence">
              <summary>{t("Technical details")}</summary>
              <div className="evidence-chip-row"><code>{row.method}</code>{row.activity_kind && <code>{row.activity_kind}</code>}</div>
              {row.service_ms !== undefined && <p>{t("Service time")}: {durationText(row.service_ms)}</p>}
              {row.next_call_gap_ms !== undefined && <p>{t("Outside WebCodex")}: {durationText(row.next_call_gap_ms)}</p>}
              {row.cycle_ms !== undefined && <p>{t("Cycle")}: {durationText(row.cycle_ms)}</p>}
              {row.server_trace_id && <code className="trace-id">trace {row.server_trace_id}</code>}
            </details>
          </article>
        ))}
        {!activity.length && !running.length && <div className="empty-inline">{t("No activity observed yet")}</div>}
        {detail.activity_truncated && <div className="inventory-note">{t("Server activity history is bounded; older Window activity is not loaded.")}</div>}
      </div>
    </section>
  );
}
