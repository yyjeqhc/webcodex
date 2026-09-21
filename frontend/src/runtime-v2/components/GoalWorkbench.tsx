import {
  Activity,
  ArrowUpRight,
  Bot,
  Check,
  CircleDot,
  Clock3,
  Flag,
  GitMerge,
  ListChecks,
  Monitor,
  RefreshCw,
  Search,
  TerminalSquare,
  Users,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import type { RuntimeV2Client } from "../api/client.js";
import { absoluteTime, projectDisplayName, relativeTime, shortId } from "../model/format.js";
import type { GoalDetailResponse, GoalListItem, GoalStep, GoalTask, GoalWait } from "../model/goals.js";
import type { ProjectRow } from "../model/types.js";
import { useGoalWorkspace } from "../state/useGoalWorkspace.js";
import type { SessionLocation } from "../state/useSessionWorkspace.js";

export type WorkSurface = "goals" | "sessions";

type Props = {
  client: RuntimeV2Client;
  language: RuntimeLanguage;
  projects: ProjectRow[];
  surface: WorkSurface;
  onSurfaceChange: (surface: WorkSurface) => void;
  onOpenSession: (location: SessionLocation) => void;
  onOpenAgent: (agentId: string) => void;
  onOpenWindow: (windowKey: string) => void;
  onUnauthorized: () => void;
};

const STEP_GLYPH: Record<string, string> = {
  completed: "✓",
  in_progress: "→",
  pending: "·",
};

function stepClass(step: GoalStep): string {
  return step.status === "completed" ? "completed" : step.status === "in_progress" ? "current" : "pending";
}

function lifecycleClass(lifecycle: string): string {
  return lifecycle === "active" ? "active" : lifecycle === "completed" ? "completed" : "cancelled";
}

function taskTone(task: GoalTask): string {
  const state = task.summary.state.toLowerCase();
  if (/succeed|complete/.test(state)) return "good";
  if (/fail|expire|cancel/.test(state)) return "warn";
  if (/active|running|leased|ready/.test(state)) return "active";
  return "quiet";
}

function waitLabel(wait: GoalWait): string {
  return `${wait.mode.toUpperCase()} ${wait.match_count} / ${wait.source_count}`;
}

function WorkSurfaceSwitch({
  surface,
  onSurfaceChange,
  language,
}: {
  surface: WorkSurface;
  onSurfaceChange: (surface: WorkSurface) => void;
  language: RuntimeLanguage;
}) {
  const t = (value: string) => translate(value, language);
  return (
    <div className="work-surface-switch" role="tablist" aria-label={t("Work level")}>
      <button type="button" role="tab" aria-selected={surface === "goals"} className={surface === "goals" ? "active" : ""} onClick={() => onSurfaceChange("goals")}>
        <Flag size={13} /> {t("Goals")}
      </button>
      <button type="button" role="tab" aria-selected={surface === "sessions"} className={surface === "sessions" ? "active" : ""} onClick={() => onSurfaceChange("sessions")}>
        <Activity size={13} /> {t("Sessions")}
      </button>
    </div>
  );
}

export { WorkSurfaceSwitch };

export function GoalWorkbench({
  client,
  language,
  projects,
  surface,
  onSurfaceChange,
  onOpenSession,
  onOpenAgent,
  onOpenWindow,
  onUnauthorized,
}: Props) {
  const t = (value: string) => translate(value, language);
  const state = useGoalWorkspace(client, true, onUnauthorized);
  const [search, setSearch] = useState("");
  const [projectFilter, setProjectFilter] = useState("");
  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    return state.goals.filter((goal) => {
      if (projectFilter && !goal.project_ids.includes(projectFilter)) return false;
      if (!query) return true;
      return [
        goal.title,
        goal.goal_id,
        goal.current_step_title || "",
        goal.progress_summary || "",
        ...goal.project_ids,
      ].some((value) => value.toLowerCase().includes(query));
    });
  }, [projectFilter, search, state.goals]);
  const active = filtered.filter((goal) => goal.lifecycle === "active");
  const history = filtered.filter((goal) => goal.lifecycle !== "active");
  const detail = state.detail;
  const controllerId = detail?.goal.controller_agent_id || detail?.goal_plan.controller_agent_id || "";
  const controller = controllerId ? detail?.agents.find((agent) => agent.agent_id === controllerId) : undefined;

  const renderGoalRow = (goal: GoalListItem) => (
    <button
      type="button"
      className={"goal-list-row " + (state.selectedGoalId === goal.goal_id ? "selected" : "")}
      data-testid={"goal-row-" + goal.goal_id}
      onClick={() => state.selectGoal(goal.goal_id)}
      key={goal.goal_id}
    >
      <span className={"goal-list-icon " + lifecycleClass(goal.lifecycle)}><Flag size={14} /></span>
      <span className="goal-list-copy">
        <strong>{goal.title}</strong>
        <small>{goal.current_step_title || goal.progress_summary || t(goal.lifecycle)}</small>
        <span className="goal-list-projects">
          {goal.project_ids.slice(0, 2).map((projectId) => (
            <em key={projectId}>{projectDisplayName(projects.find((project) => project.id === projectId)?.name, projectId)}</em>
          ))}
          {goal.project_ids.length > 2 && <em>+{goal.project_ids.length - 2}</em>}
        </span>
      </span>
      <span className="goal-list-progress">
        <strong>{goal.completed_step_count}/{goal.total_step_count}</strong>
        <small>{relativeTime(goal.updated_at_unix_ms)}</small>
      </span>
    </button>
  );

  return (
    <div className="work-layout goal-work-layout">
      <aside className="work-list-panel goal-list-panel">
        <div className="work-list-header goal-list-header">
          <div><span className="eyebrow">{t("Durable work")}</span><h1>{t("Work")}</h1></div>
          <button className="icon-button" type="button" onClick={state.refresh} aria-label={t("Refresh")}><RefreshCw size={14} /></button>
          <WorkSurfaceSwitch surface={surface} onSurfaceChange={onSurfaceChange} language={language} />
        </div>
        <div className="goal-list-filters">
          <label><Search size={14} /><input aria-label={t("Search Goals")} value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("Search Goals…")} /></label>
          <select aria-label={t("Filter Goals by Project")} value={projectFilter} onChange={(event) => setProjectFilter(event.target.value)}>
            <option value="">{t("All Projects")}</option>
            {projects.map((project) => <option value={project.id} key={project.id}>{projectDisplayName(project.name, project.id)}</option>)}
          </select>
        </div>
        <div className="work-list-scroll goal-list-scroll">
          {state.truncated && <div className="inventory-note">{t("Goal inventory is bounded by the durable store.")}</div>}
          {!!active.length && <section className="work-group"><div className="work-group-heading"><span>{t("Active Goals")}</span><small>{active.length}</small></div><div className="work-group-list">{active.map(renderGoalRow)}</div></section>}
          {!!history.length && <section className="work-group"><div className="work-group-heading"><span>{t("Goal history")}</span><small>{history.length}</small></div><div className="work-group-list">{history.map(renderGoalRow)}</div></section>}
          {state.availability === "loading" && <div className="empty-inline">{t("Loading Goals…")}</div>}
          {state.availability === "denied" && <div className="empty-inline">{t("Goals require communication and Project read access.")}</div>}
          {state.availability === "available" && !filtered.length && <div className="empty-panel"><Flag size={18} /><strong>{t("No matching Goals")}</strong></div>}
        </div>
      </aside>

      <main className="goal-main">
        {detail ? (
          <GoalDetail
            detail={detail}
            language={language}
            controller={controller}
            onOpenSession={onOpenSession}
            onOpenAgent={onOpenAgent}
            onOpenWindow={onOpenWindow}
          />
        ) : (
          <div className="empty-work">
            <Flag size={23} />
            <h2>{state.detailAvailability === "loading" ? t("Loading Goal…") : t("Select a Goal")}</h2>
            <p>{t("Goals are durable work truth above Sessions, workers, waits and Window evidence.")}</p>
          </div>
        )}
      </main>

      <aside className="inspector goal-inspector">
        <div className="inspector-header"><div><span className="eyebrow">{t("Goal context")}</span><strong>{detail?.goal.summary.title || t("No Goal selected")}</strong></div></div>
        <div className="inspector-content">
          {detail && <GoalInspector detail={detail} controller={controller} language={language} onOpenAgent={onOpenAgent} />}
        </div>
      </aside>
    </div>
  );
}

function GoalDetail({
  detail,
  language,
  controller,
  onOpenSession,
  onOpenAgent,
  onOpenWindow,
}: {
  detail: GoalDetailResponse;
  language: RuntimeLanguage;
  controller: GoalDetailResponse["agents"][number] | undefined;
  onOpenSession: (location: SessionLocation) => void;
  onOpenAgent: (agentId: string) => void;
  onOpenWindow: (windowKey: string) => void;
}) {
  const t = (value: string) => translate(value, language);
  const plan = detail.goal_plan;
  const continuity = plan.continuity;
  const activity = plan.activity;
  return (
    <>
      <header className="goal-detail-header">
        <div className="goal-detail-heading">
          <div className="breadcrumbs"><span>{t("Goal")}</span><span>/</span><span>{shortId(plan.goal_id)}</span></div>
          <h2>{plan.title}</h2>
        </div>
        <div className="goal-header-meta">
          <span className={"goal-lifecycle-pill " + lifecycleClass(plan.lifecycle)}><CircleDot size={11} /> {t(plan.lifecycle)}</span>
          <time>{absoluteTime(plan.updated_at_unix_ms)}</time>
        </div>
      </header>
      <div className="goal-detail-scroll">
        <div className="goal-detail-measure">
          <section className="goal-progress-card">
            <div className="goal-progress-head">
              <span><ListChecks size={17} /></span>
              <div><strong>{t("Plan")}</strong><small>{plan.completed_step_count} / {plan.total_step_count} {t("steps completed")}</small></div>
              <strong>{plan.total_step_count ? Math.round(plan.completed_step_count / plan.total_step_count * 100) : 0}%</strong>
            </div>
            <div className="goal-progress-track"><span style={{ width: `${plan.total_step_count ? plan.completed_step_count / plan.total_step_count * 100 : 0}%` }} /></div>
            <div className="goal-step-list">
              {plan.steps.map((step, index) => (
                <div className={"goal-step " + stepClass(step)} key={step.id} data-testid={"goal-step-" + step.id}>
                  <span>{STEP_GLYPH[step.status] || index + 1}</span>
                  <div><strong>{step.title}</strong><small>{step.id} · {t(step.status)}</small></div>
                </div>
              ))}
            </div>
            {plan.progress_summary && <p className="goal-progress-summary">{plan.progress_summary}</p>}
          </section>

          <div className="goal-status-grid">
            <section className="goal-status-card">
              <div className="goal-section-title"><Bot size={16} /><strong>{t("Controller")}</strong></div>
              {controller ? (
                <button className="goal-link-card" type="button" onClick={() => onOpenAgent(controller.agent_id)}>
                  <span><strong>{controller.display_name || controller.handle}</strong><small>@{controller.handle} · {shortId(controller.agent_id)}</small></span><ArrowUpRight size={14} />
                </button>
              ) : plan.controller_agent_id ? (
                <button className="goal-link-card" type="button" onClick={() => onOpenAgent(plan.controller_agent_id!)}><span><strong>{shortId(plan.controller_agent_id)}</strong><small>{t("Controller identity")}</small></span><ArrowUpRight size={14} /></button>
              ) : <div className="empty-inline compact">{t("No controller configured")}</div>}
              <div className="goal-status-facts">
                <span>{t("Auto-resume")}</span><strong>{continuity.production_auto_resume_available ? t("Ready") : t("Not ready")}</strong>
              </div>
            </section>
            <section className="goal-status-card">
              <div className="goal-section-title"><Clock3 size={16} /><strong>{t("Continuity")}</strong></div>
              <strong className={"goal-continuity-state " + continuity.state}>{t(continuity.state)}</strong>
              <p>{t("Host delivery")} · {t(continuity.host_delivery)} &nbsp; {t("Fresh turn")} · {t(continuity.fresh_turn)}</p>
              <div className="goal-status-facts"><span>{t("Last resume")}</span><strong>{absoluteTime(continuity.last_resume_at_unix_ms || undefined)}</strong></div>
            </section>
            <section className="goal-status-card">
              <div className="goal-section-title"><Activity size={16} /><strong>{t("Goal activity")}</strong></div>
              <strong className="goal-continuity-state">{t(activity.state)}</strong>
              <p>{activity.linked_window_count ?? 0} {t("observed Windows")} · {activity.active_meaningful_request_count ?? 0} {t("active requests")}</p>
              <div className="goal-status-facts"><span>{t("Last meaningful work")}</span><strong>{absoluteTime(activity.last_meaningful_activity_at_ms || undefined)}</strong></div>
            </section>
          </div>

          <GoalSection title="Sessions" icon={<Activity size={16} />} count={detail.sessions.length}>
            <div className="goal-resource-list">
              {detail.sessions.map((session) => (
                <button className="goal-resource-row" type="button" key={session.session_id} onClick={() => onOpenSession({ projectId: session.project_id, projectName: session.project_name || session.project_id, runner: session.client_id, sessionId: session.session_id })}>
                  <span className={"goal-resource-icon " + (session.running_jobs ? "active" : "")}><Activity size={15} /></span>
                  <span><strong>{session.title}</strong><small>{projectDisplayName(session.project_name || undefined, session.project_id)} · {shortId(session.session_id)}</small></span>
                  <span className="goal-resource-state">{session.running_jobs ? `${session.running_jobs} ${t("jobs running")}` : t(session.lifecycle)}</span>
                  <time>{relativeTime(session.updated_at)}</time><ArrowUpRight size={14} />
                </button>
              ))}
              {!detail.sessions.length && <div className="empty-inline">{t("No correlated Sessions")}</div>}
            </div>
          </GoalSection>

          <GoalSection title="Workers" icon={<Users size={16} />} count={detail.tasks.length}>
            <div className="goal-resource-list">
              {detail.tasks.map((task) => <GoalTaskRow key={task.summary.task_id} task={task} language={language} onOpenAgent={onOpenAgent} />)}
              {!detail.tasks.length && <div className="empty-inline">{t("No correlated AgentTasks")}</div>}
            </div>
          </GoalSection>

          <GoalSection title="Join / fan-in" icon={<GitMerge size={16} />} count={detail.waits.length}>
            <div className="goal-wait-list">
              {detail.waits.map((wait) => <GoalWaitRow key={wait.wait_id} wait={wait} language={language} />)}
              {!detail.waits.length && <div className="empty-inline">{t("No Goal-scoped AgentWait")}</div>}
              {detail.waits_truncated && <div className="inventory-note">{t("Goal Wait inventory is bounded.")}</div>}
            </div>
          </GoalSection>

          <GoalSection title="Windows" icon={<Monitor size={16} />} count={detail.windows.length}>
            <div className="goal-resource-list">
              {detail.windows.map((window) => (
                <button className="goal-resource-row" type="button" key={window.client_window_key} onClick={() => onOpenWindow(window.client_window_key)}>
                  <span className={"goal-resource-icon " + (window.active_count ? "active" : "")}><Monitor size={15} /></span>
                  <span><strong>Window {shortId(window.client_window_key)}</strong><small>{window.source} · {window.session_ids.length} {t("linked Sessions")}</small></span>
                  <span className="goal-resource-state">{window.active_count ? `${window.active_count} ${t("active requests")}` : t("Observed")}</span>
                  <time>{absoluteTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms)}</time><ArrowUpRight size={14} />
                </button>
              ))}
              {!detail.windows.length && <div className="empty-inline">{t("No linked Window evidence")}</div>}
            </div>
          </GoalSection>
        </div>
      </div>
    </>
  );
}

function GoalSection({ title, icon, count, children }: { title: string; icon: React.ReactNode; count: number; children: React.ReactNode }) {
  return <section className="goal-section"><div className="goal-section-heading"><span>{icon}<strong>{title}</strong></span><small>{count}</small></div>{children}</section>;
}

function GoalTaskRow({ task, language, onOpenAgent }: { task: GoalTask; language: RuntimeLanguage; onOpenAgent: (agentId: string) => void }) {
  const t = (value: string) => translate(value, language);
  return (
    <details className="goal-task-row">
      <summary>
        <span className={"goal-resource-icon " + taskTone(task)}><TerminalSquare size={15} /></span>
        <span><strong>{task.summary.title}</strong><small>{shortId(task.summary.task_id)} · {task.summary.execution_kind || t("No execution binding")}</small></span>
        <span className={"goal-resource-state " + taskTone(task)}>{t(task.summary.state)}</span>
        <time>{relativeTime(task.summary.updated_at_unix_ms)}</time>
      </summary>
      <div className="goal-task-detail">
        <p>{task.instruction}</p>
        <dl>
          <div><dt>{t("Task ID")}</dt><dd><code>{task.summary.task_id}</code></dd></div>
          <div><dt>{t("Execution")}</dt><dd>{task.summary.execution_status || task.summary.execution_kind || "—"}</dd></div>
          <div><dt>{t("Recovery")}</dt><dd>{task.summary.recovery_kind}</dd></div>
        </dl>
        {task.summary.assignee_agent_id && <button className="text-button" type="button" onClick={() => onOpenAgent(task.summary.assignee_agent_id!)}><Bot size={13} /> {t("Open worker Agent")} <ArrowUpRight size={13} /></button>}
      </div>
    </details>
  );
}

function GoalWaitRow({ wait, language }: { wait: GoalWait; language: RuntimeLanguage }) {
  const t = (value: string) => translate(value, language);
  return (
    <details className="goal-wait-row">
      <summary><GitMerge size={15} /><span><strong>{waitLabel(wait)}</strong><small>{shortId(wait.wait_id)} · {t(wait.state)}</small></span><span className="goal-resource-state">{t(wait.mode)}</span></summary>
      <div className="goal-wait-detail">
        <div className="goal-join-progress"><span style={{ width: `${wait.source_count ? Math.min(100, wait.match_count / wait.source_count * 100) : 0}%` }} /></div>
        {wait.sources.map((source) => {
          const match = wait.matches.find((entry) => entry.task_id === source.task_id);
          return <div className="goal-wait-source" key={source.task_id}><span>{match ? <Check size={13} /> : <CircleDot size={13} />}</span><code>{shortId(source.task_id)}</code><small>{match ? t(match.terminal_task_state) : t("Waiting")}</small></div>;
        })}
      </div>
    </details>
  );
}

function GoalInspector({ detail, controller, language, onOpenAgent }: { detail: GoalDetailResponse; controller: GoalDetailResponse["agents"][number] | undefined; language: RuntimeLanguage; onOpenAgent: (agentId: string) => void }) {
  const t = (value: string) => translate(value, language);
  const goal = detail.goal;
  const plan = detail.goal_plan;
  return (
    <>
      <section className="context-hero goal-context-hero"><div className="context-kicker"><Flag size={13} /> {t(plan.lifecycle)}</div><strong>{goal.summary.title}</strong><p>{goal.objective}</p></section>
      <section className="inspector-section"><h3>{t("Goal identity")}</h3><div className="fact-list"><div><span>{t("Goal")}</span><strong><code>{goal.summary.goal_id}</code></strong></div><div><span>{t("Revision")}</span><strong>{plan.revision}</strong></div><div><span>{t("Checkpoint")}</span><strong>{absoluteTime(plan.checkpoint_at_unix_ms || undefined)}</strong></div><div><span>{t("Updated")}</span><strong>{absoluteTime(plan.updated_at_unix_ms)}</strong></div></div></section>
      <section className="inspector-section"><h3>{t("Projects")}</h3><div className="goal-inspector-list">{detail.projects.map((project) => <div key={project.id}><strong>{projectDisplayName(project.name, project.id)}</strong><small>{project.path || project.id}</small></div>)}{!detail.projects.length && <div className="empty-inline">{t("No authorized Project correlation")}</div>}</div></section>
      <section className="inspector-section"><h3>{t("Completion conditions")}</h3><ol className="goal-condition-list">{goal.plan.completion_conditions.map((condition, index) => <li key={index}>{condition}</li>)}</ol></section>
      <section className="inspector-section"><h3>{t("Controller")}</h3>{controller ? <button className="goal-inspector-agent" type="button" onClick={() => onOpenAgent(controller.agent_id)}><Bot size={15} /><span><strong>{controller.display_name || controller.handle}</strong><small>{plan.continuity.production_auto_resume_available ? t("Auto-resume ready") : t("Auto-resume not ready")}</small></span><ArrowUpRight size={13} /></button> : <div className="empty-inline">{t("No controller configured")}</div>}</section>
    </>
  );
}
