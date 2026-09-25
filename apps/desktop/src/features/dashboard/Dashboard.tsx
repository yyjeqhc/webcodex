import type { DesktopState } from "../../models/topology";
import { ActionIcon, Badge, Button, NavLink, Tooltip } from "@mantine/core";
import { ArrowRight, History, MonitorSmartphone } from "lucide-react";
import { useProduct } from "../../i18n/product";
import { useConnectionsTools } from "../../i18n/connections-tools";
import { useLocale } from "../../i18n/locale";
import { displayProjectPath, projectName, sameProject, sessionTitle, useWorkspace } from "../workspace/WorkspaceContext";
import { WorkspaceStatus, ChatgptObservation, observationTime } from "../workspace/WorkspaceStatus";
import { ProjectRows } from "../projects/ProjectRows";

interface DashboardProps {
  state: DesktopState; refreshing: boolean; onRefresh: () => void;
  onResumeRuntime: () => void;
  onChooseProject: () => void; onChangeSetup: () => void;
  onNavigate: (page: "projects" | "connection" | "activity" | "extensions") => void;
  onStopQuickShare: () => void; onStopRuntime: () => void;
}
export function Dashboard(props: DashboardProps) {
  const { state, onNavigate } = props;
  const p = useProduct(); const c = useConnectionsTools(); const { locale } = useLocale();
  const workspace = useWorkspace();
  const recent = workspace.sessions.slice(0, 3);
  const busy = Boolean(state.current_operation);
  const currentProject = state.project ? workspace.projects.find(project => sameProject(project, state.project!)) : undefined;
  return <section className="page-section workspace-page dashboard-page" aria-labelledby="home-title" data-webcodex-page="home">
    <header className="page-heading-row">
      <div><span className="eyebrow">{p("workspace")}</span><h1 id="home-title">{currentProject ? projectName(currentProject) : "WebCodex"}</h1>{currentProject?.path && <code className="dashboard-project-path">{displayProjectPath(currentProject.path)}</code>}</div>
      <div className="dashboard-heading-actions">
        <Button className="secondary-button" variant="default" disabled={busy || props.refreshing} onClick={() => { workspace.refresh(); props.onRefresh(); }}>{p("refresh")}</Button>
      </div>
    </header>
    <WorkspaceStatus state={state} />
    <div className="workspace-quick-actions">
      <span>{workspace.projects.length} {p("projects")}</span>
      <Button className={currentProject ? "secondary-button" : "primary-button"} variant={currentProject ? "default" : "filled"} onClick={props.onChooseProject} disabled={busy}>{p("addProject")}</Button>
      <Button className="secondary-button" variant="default" aria-label={`${p("manage")} ${c("connections")}`} onClick={() => onNavigate("connection")}>{c("connections")}</Button>
      {!state.readiness.runtime_ready && <Button className="secondary-button" variant="default" onClick={state.readiness.next_action_kind === "restart_quick_share" ? props.onChangeSetup : props.onResumeRuntime} disabled={busy}>{state.readiness.next_action_kind === "restart_quick_share" ? p("restart") + " Quick Share" : p("start") + " WebCodex"}</Button>}
      {(state.readiness.project === "error" || state.readiness.project === "reload_required") && <Button className="secondary-button" variant="default" onClick={props.onChooseProject} disabled={busy}>{p("setup")} {p("projects")}</Button>}
    </div>
    <div className="dashboard-content-grid">
    <section className="workspace-section ui-workbench-surface" aria-labelledby="recent-projects-title">
      <header className="workspace-section-heading"><h2 id="recent-projects-title">{p("recentProjects")}</h2><Tooltip label={p("allProjects")} withArrow><ActionIcon variant="subtle" color="brand" aria-label={p("allProjects")} onClick={() => onNavigate("projects")}><ArrowRight size={17} /></ActionIcon></Tooltip></header>
      {workspace.error && <p role="alert" className="workspace-notice">{p("loadError")}</p>}
      <ProjectRows projects={workspace.projects.slice(0, 4)} />
      {!workspace.projects.length && <p className="workspace-empty">{p("noProjects")}</p>}
    </section>
    <section className="workspace-section ui-workbench-surface" aria-labelledby="recent-activity-title">
      <header className="workspace-section-heading"><h2 id="recent-activity-title">{p("recentActivity")}</h2><Tooltip label={p("open")} withArrow><ActionIcon variant="subtle" color="brand" aria-label={p("open")} onClick={() => onNavigate("activity")}><ArrowRight size={17} /></ActionIcon></Tooltip></header>
      <ChatgptObservation state={state} />
      <div className="dashboard-activity-list">{recent.map(session => <NavLink component="button" type="button" className="dashboard-activity-link" key={session.session_id} label={sessionTitle(session.title)} leftSection={<History size={16} />} rightSection={<time>{observationTime(session.updated_at * 1000, locale)}</time>}
        description={<Badge size="xs" variant="light" color="gray">{p("sessions")}</Badge>} onClick={() => {
        if (!session.project_id) return;
        workspace.setSelection({ kind: "session", project: session.project_id, id: session.session_id }); onNavigate("activity");
      }} />)}
      {workspace.windows.slice(0, 2).map(window => <NavLink component="button" type="button" className="dashboard-activity-link" key={window.client_window_key} label={window.client_window_key.slice(-12)} leftSection={<MonitorSmartphone size={16} />} rightSection={<time>{observationTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms, locale)}</time>}
        description={<Badge size="xs" variant="light" color="gray">{p("windows")}</Badge>} onClick={() => {
        workspace.setSelection({ kind: "window", id: window.client_window_key }); onNavigate("activity");
      }} />)}</div>
      {!recent.length && !workspace.windows.length && <p className="workspace-empty">{workspace.loading ? p("loading") : p("noActivity")}</p>}
    </section>
    </div>
    {state.topology?.experience === "quick_share" && state.quick_share && <button className="secondary-button" onClick={props.onStopQuickShare} disabled={busy}>{p("stop")} Quick Share</button>}
  </section>;
}
