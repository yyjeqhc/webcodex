import {
  Activity,
  ArrowUpRight,
  Bot,
  Check,
  ChevronDown,
  HardDrive,
  Monitor,
  Play,
  Server,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, durationText, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import type { Availability, ProjectRow, RuntimeOverview } from "../model/types.js";
import { useAgentInventory } from "../state/useAgentInventory.js";
import { AgentsPanel } from "../components/AgentsPanel.js";
import { useLinkedSessionWindowCounts } from "../state/useLinkedSessionWindowCounts.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";
import { useWindowWorkspace } from "../state/useWindowWorkspace.js";

type RuntimeMode = "overview" | "windows" | "agents";

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  overview: RuntimeOverview | null;
  overviewAvailability: Availability;
  projects: ProjectRow[];
  onOpenSession: (location: SessionLocation) => void;
  onUnauthorized: () => void;
};

export function RuntimeView({
  client,
  language,
  overview,
  overviewAvailability,
  projects,
  onOpenSession,
  onUnauthorized,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [mode, setMode] = useState<RuntimeMode>("overview");
  const [visibleActivityLimit, setVisibleActivityLimit] = useState(200);
  const windows = useWindowWorkspace(client, true, onUnauthorized, {
    refreshMs: mode === "windows" ? 3_000 : 30_000,
    loadDetail: mode === "windows",
  });
  const agents = useAgentInventory(client, mode === "overview");
  const observedBy = useLinkedSessionWindowCounts(client, mode === "windows", windows.detail?.linked_sessions || []);
  const chronologicalActivity = windows.detail
    ? windows.detail.activity.slice().sort((a, b) => a.ended_at_ms - b.ended_at_ms || a.started_at_ms - b.started_at_ms)
    : [];
  const visibleActivity = chronologicalActivity.slice(-visibleActivityLimit);
  const remainingActivity = Math.max(0, chronologicalActivity.length - visibleActivity.length);
  const overviewStatus = overviewAvailability === "available"
    ? { className: "good", label: "connected" }
    : overviewAvailability === "stale"
      ? { className: "warn", label: "stale" }
      : overviewAvailability === "loading" || overviewAvailability === "idle"
        ? { className: "", label: "Loading…" }
        : { className: "warn", label: "Runtime overview unavailable" };

  useEffect(() => setVisibleActivityLimit(200), [windows.selectedKey]);

  const projectFor = (projectId: string | undefined) =>
    projectId ? projects.find((project) => project.id === projectId) : undefined;

  return (
    <main className="page runtime-page">
      <header className="page-heading runtime-heading">
        <div>
          <span className="eyebrow">{t("System evidence")}</span>
          <h1>{t("Runtime")}</h1>
          <p>{t("Infrastructure, Window observation and low-level evidence stay below task-oriented Work.")}</p>
        </div>
        <span className="quiet-pill">
          <span className={"status-dot " + overviewStatus.className} />
          {t(overviewStatus.label)}
        </span>
      </header>

      <div className="runtime-tabs" role="tablist">
        <button className={mode === "overview" ? "active" : ""} role="tab" aria-selected={mode === "overview"} onClick={() => setMode("overview")}>
          <Server size={15} /> {t("Overview")}
        </button>
        <button className={mode === "windows" ? "active" : ""} role="tab" aria-selected={mode === "windows"} onClick={() => setMode("windows")}>
          <Monitor size={15} /> {t("Window Activity")} <span>{windows.total || windows.windows.length}</span>
        </button>
        <button className={mode === "agents" ? "active" : ""} role="tab" aria-selected={mode === "agents"} onClick={() => setMode("agents")}>
          <Bot size={15} /> {t("Agents")} {agents.count !== null && <span>{agents.count}</span>}
        </button>
      </div>

      {mode === "overview" ? (
        <>
          <div className="runtime-metrics">
            <div>
              <span><Server size={17} /> {t("Runners")}</span>
              <strong>{overview?.runner_count ?? "—"}</strong>
              <small>{overview ? String(overview.runners_online) + " " + t("online") : t("Loading…")}</small>
            </div>
            <div>
              <span><Play size={17} /> {t("Active jobs")}</span>
              <strong>{overview?.active_jobs ?? "—"}</strong>
              <small>{overview ? String(overview.workflow_sessions.running) + " " + t("running Sessions") : "—"}</small>
            </div>
            <div>
              <span><Monitor size={17} /> {t("Observed windows")}</span>
              <strong>{windows.availability === "denied" ? "—" : windows.total || windows.windows.length}</strong>
              <small>{t("many-to-many Session evidence")}</small>
            </div>
            <div>
              <span><Bot size={17} /> {t("Durable agents")}</span>
              <strong>{agents.available === false ? "—" : agents.count ?? "…"}</strong>
              <small>{agents.available === false ? t("communication:read required") : t("runtime inventory")}</small>
            </div>
          </div>

          <section className="runtime-section">
            <div className="section-heading">
              <div><h2>{t("Runner fleet")}</h2><p>{t("Execution capacity and source/build alignment.")}</p></div>
            </div>
            {overview?.runners.map((runner) => (
              <div className="runtime-row" key={runner.client_id}>
                <span className="runner-icon"><Monitor size={17} /></span>
                <span>
                  <strong>{runner.client_id}</strong>
                  <small>
                    {runner.connected ? t("Runner online") : t("Runner unavailable")}
                    {runner.version ? " · " + runner.version : ""}
                  </small>
                </span>
                <span className="runtime-row-meta">
                  {runner.jobs_running} {t("jobs running")} · {runner.projects_scanned} {t("projects")}
                </span>
                <span className={"status-pill " + (runner.source_alignment === "aligned" ? "good" : "warn")}>
                  {runner.source_alignment === "aligned" ? <Check size={12} /> : <HardDrive size={12} />}
                  {runner.source_alignment || t("unknown")}
                </span>
              </div>
            ))}
            {!overview?.runners.length && <div className="empty-inline">{t("Runtime overview unavailable")}</div>}
          </section>

          <section className="runtime-section">
            <div className="section-heading">
              <div><h2>{t("Meaningful runtime status")}</h2><p>{t("Only evidence available from the current Runtime projection is shown.")}</p></div>
              <button className="text-button" type="button" onClick={() => setMode("windows")}>
                {t("Open Window activity")} <ArrowUpRight size={13} />
              </button>
            </div>
            <div className="event-log">
              <div>
                <Activity size={15} />
                <span>
                  <strong>{t("Workflow Sessions")}</strong>
                  <small>{overview ? String(overview.workflow_sessions.active) + " " + t("active") : "—"}</small>
                </span>
                <time>{overview?.recent_sessions.sessions[0] ? relativeTime(overview.recent_sessions.sessions[0].updated_at) : "—"}</time>
              </div>
              <div>
                <TerminalSquare size={15} />
                <span><strong>{t("Active jobs")}</strong><small>{overview ? String(overview.active_jobs) : "—"}</small></span>
                <time>{overview?.mixed_builds_present ? t("mixed builds") : t("builds observed")}</time>
              </div>
              <div>
                <HardDrive size={15} />
                <span>
                  <strong>{t("Source alignment")}</strong>
                  <small>{overview ? String(overview.source_mismatched_runners) + " " + t("mismatched runners") : "—"}</small>
                </span>
                <time>{overview?.build_git_commit ? shortId(overview.build_git_commit) : "—"}</time>
              </div>
            </div>
          </section>
        </>
      ) : mode === "agents" ? (
        <AgentsPanel client={client} language={language} onUnauthorized={onUnauthorized} />
      ) : (
        <div className="windows-workbench" data-testid="window-workbench">
          <aside className="window-list">
            <div className="window-list-head">
              <div>
                <strong>{t("Observed Windows")}</strong>
                <small>{t("Observation evidence; Windows do not own Sessions.")}</small>
              </div>
              <span className="count-badge">{windows.windows.length}</span>
            </div>
            {(windows.availability === "available" || windows.availability === "stale") && (
              <div className={"window-scope-note " + windows.scope} data-testid="window-scope-note">
                {windows.scope === "global"
                  ? t("Global Runtime scope. Only observed WebCodex requests appear here; no Project selection is required.")
                  : t("This credential sees only its observation principal's Windows within currently authorized Projects. Global Window observation requires an administrator Runtime credential.")}
              </div>
            )}
            {(windows.availability === "available" || windows.availability === "stale") && windows.truncated && (
              <div className="inventory-note">{t("Window inventory is bounded; not all observed Windows are loaded.")}</div>
            )}
            {windows.windows.map((window) => {
              const project = projectFor(window.last_project);
              return (
                <button
                  type="button"
                  className={"window-row" + (windows.selectedKey === window.client_window_key ? " selected" : "")}
                  onClick={() => windows.select(window.client_window_key)}
                  key={window.client_window_key}
                  data-testid={"window-row-" + window.client_window_key}
                >
                  <span className="window-icon"><Monitor size={16} /></span>
                  <span className="window-row-main">
                    <strong>Window {shortId(window.client_window_key)}</strong>
                    <small>{window.source} · {project?.client_id || t("Runner not observed")}</small>
                    <small>{project?.name || window.last_project || t("No current Project evidence")}</small>
                  </span>
                  <time>{relativeTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms)}</time>
                </button>
              );
            })}
            {windows.availability === "loading" && <div className="empty-inline">{t("Loading Window activity…")}</div>}
            {windows.availability === "denied" && <div className="empty-inline">{t("Window activity unavailable")}</div>}
            {windows.availability === "available" && !windows.windows.length && <div className="empty-inline">{t("No Window activity observed yet.")}</div>}
          </aside>

          <section className="window-detail">
            {windows.detail ? (
              <>
                <header className="window-detail-head">
                  <div>
                    <span className="eyebrow">{t("Window evidence")}</span>
                    <h2>Window {shortId(windows.detail.client_window_key)}</h2>
                    <p>{windows.detail.source} · {t("last observed")} {relativeTime(windows.detail.last_seen_at_ms)}</p>
                  </div>
                  <span className="quiet-pill">{windows.detail.active_count} {t("active requests")}</span>
                </header>

                <section className="window-relation-note">
                  <Monitor size={16} />
                  <p>{t("This Window is observation evidence. Linked Sessions remain Project-scoped resources and may be observed by other Windows too.")}</p>
                </section>

                <details className="window-relations-disclosure">
                  <summary>
                    <span><Monitor size={15} /><strong>{t("Linked Sessions")}</strong></span>
                    <span className="count-badge">{windows.detail.sessions_returned}</span>
                    <ChevronDown size={15} />
                  </summary>
                  <p>{t("Relations describe how this Window observed each Session; they are not ownership.")}</p>
                  <div className="linked-session-list">
                    {windows.detail.linked_sessions.map((session) => {
                      const project = projectFor(session.project);
                      const count = observedBy.get(session.workflow_session_id);
                      const canOpen = Boolean(session.project && project);
                      return (
                        <button
                          type="button"
                          className="linked-session-row"
                          key={session.workflow_session_id}
                          disabled={!canOpen}
                          onClick={() => {
                            if (!session.project || !project) return;
                            onOpenSession({
                              projectId: session.project,
                              projectName: project.name || project.id,
                              runner: project.client_id,
                              sessionId: session.workflow_session_id,
                            });
                          }}
                        >
                          <span className="session-live-dot running" />
                          <span className="project-session-main">
                            <strong>{session.title || session.workflow_session_id}</strong>
                            <small>{project?.name || session.project || t("Project not exposed in relation")}</small>
                          </span>
                          <span className="relation-kind">{session.relations.join(" · ") || t("linked")}</span>
                          <span className="project-session-windows">
                            <Monitor size={13} /> {count === undefined ? "…" : count === null ? "—" : count} {t("Windows")}
                          </span>
                          <time>{relativeTime(session.last_linked_at_ms)}</time>
                          {canOpen && <ArrowUpRight size={14} />}
                        </button>
                      );
                    })}
                    {!windows.detail.linked_sessions.length && <div className="empty-inline">{t("Window with no current Session")}</div>}
                    {windows.detail.sessions_truncated && (
                      <div className="inventory-note">{t("Linked Session inventory is bounded; additional relations are not loaded.")}</div>
                    )}
                  </div>
                </details>

                <section className="window-detail-section window-workflow-section">
                  <div className="section-heading">
                    <div><h2>{t("Observed workflow")}</h2><p>{t("Each observed action is collapsed by default. Project and status stay visible; expand for bounded low-level evidence.")}</p></div>
                    <span className="quiet-pill">{visibleActivity.length} / {windows.detail.activity_returned}</span>
                  </div>
                  <div className="window-workflow-list">
                    {visibleActivity.map((activity, index) => {
                      const projectId = activity.project || activity.workflow_sessions.find((session) => session.project)?.project;
                      const project = projectFor(projectId);
                      return (
                        <details className="window-workflow-step" data-testid="window-workflow-step" key={String(activity.started_at_ms) + "-" + index}>
                          <summary>
                            <span className="activity-glyph"><Activity size={15} /></span>
                            <span className="window-workflow-title">
                              <strong>{activity.activity_presentation || activity.tool_name || activity.method}</strong>
                              <small>{activity.tool_name || activity.activity_kind || activity.method}</small>
                            </span>
                            {projectId && <span className="window-project-tag" data-testid="window-project-tag" title={projectId}>{projectDisplayName(project?.name, projectId)}</span>}
                            <span className={"status-pill " + (activity.status === "ok" || activity.status === "success" ? "good" : "")}>{activity.status}</span>
                            <time>{absoluteTime(activity.ended_at_ms)}</time>
                            <ChevronDown size={15} />
                          </summary>
                          <div className="window-workflow-detail">
                            <div className="evidence-chip-row">
                              {activity.tool_name && <code>{activity.tool_name}</code>}
                              {activity.activity_kind && <code>{activity.activity_kind}</code>}
                              <code>{activity.method}</code>
                            </div>
                            <dl>
                              <div><dt>{t("Started")}</dt><dd>{absoluteTime(activity.started_at_ms)}</dd></div>
                              <div><dt>{t("Duration")}</dt><dd>{durationText(activity.duration_ms)}</dd></div>
                              {activity.service_ms !== undefined && <div><dt>{t("Service time")}</dt><dd>{durationText(activity.service_ms)}</dd></div>}
                              {activity.cycle_ms !== undefined && <div><dt>{t("Cycle")}</dt><dd>{durationText(activity.cycle_ms)}</dd></div>}
                              {projectId && <div><dt>{t("Project")}</dt><dd><code>{projectId}</code></dd></div>}
                            </dl>
                            {!!activity.workflow_sessions.length && (
                              <div className="window-workflow-relations">
                                {activity.workflow_sessions.map((relation) => (
                                  <span key={relation.workflow_session_id + ":" + relation.relation}>
                                    {relation.relation} · {shortId(relation.workflow_session_id)}
                                  </span>
                                ))}
                              </div>
                            )}
                            {activity.server_trace_id && <code className="trace-id" title={activity.server_trace_id}>trace {shortId(activity.server_trace_id)}</code>}
                          </div>
                        </details>
                      );
                    })}
                    {!windows.detail.activity.length && <div className="empty-inline">{t("No activity observed yet")}</div>}
                    {remainingActivity > 0 && (
                      <button className="activity-load-more" type="button" onClick={() => setVisibleActivityLimit((current) => current + 200)}>
                        {t("Show more activity")} · {remainingActivity} {t("remaining")}
                      </button>
                    )}
                    {windows.detail.activity_truncated && (
                      <div className="inventory-note">{t("Server activity history is bounded; older Window activity is not loaded.")}</div>
                    )}
                  </div>
                </section>
              </>
            ) : (
              <div className="empty-work">
                <Monitor size={22} />
                <h2>{t("Select an observed Window")}</h2>
                <p>{windows.detailAvailability === "denied" ? t("This Window is no longer visible to the current credential. Refresh to check available activity.") : t("Open a Window to see its project and Workflow Sessions.")}</p>
              </div>
            )}
          </section>
        </div>
      )}
    </main>
  );
}
