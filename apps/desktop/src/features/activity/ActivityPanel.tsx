import { useEffect, useMemo, useState } from "react";
import { NativeSelect } from "@mantine/core";
import { MessageSquare, TerminalSquare, ArrowRight } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import type { ActivityEntry } from "../../models/topology";
import type { WindowDetail, WindowSummary, WorkflowSession } from "../../models/workspace";
import { displayProjectPath, projectName, sessionTitle, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { observationTime } from "../workspace/WorkspaceStatus";
import { WindowActivityDetail } from "./WindowActivityDetail";
import { WorkflowSessionDetail, sessionLifecycle, SessionAttention, validationLabel } from "./WorkflowSessionDetail";
import { SystemActivity } from "./SystemActivity";
import { WorkspaceEmptyState } from "../../components/WorkspaceEmptyState";
import { ProjectPicker } from "../../../../../frontend/src/ui/ProjectPicker";
import { executionLabel, recentMeaningfulCalls } from "./window-evidence";

type Tab = "windows" | "sessions" | "system";
const TABS: Tab[] = ["windows", "sessions", "system"];
const PAGE_SIZE = 8;
type SessionFilter = "all" | "running" | "attention";
function sessionMatchesFilter(session: WorkflowSession, filter: SessionFilter): boolean {
  if (filter === "running") return session.running_call || session.running_jobs > 0;
  if (filter === "attention") {
    const attention = session.overview.attention;
    return attention.open_todos > 0 || attention.open_questions > 0 || attention.open_risks > 0 || session.overview.validation?.state === "failed";
  }
  return true;
}

function useWindowPreviews(rows: WindowSummary[], enabled: boolean, revision: number) {
  const [data, setData] = useState<Record<string, WindowDetail>>({});
  const key = rows.map(row => `${row.client_window_key}:${row.last_meaningful_activity_at_ms ?? row.last_seen_at_ms}:${row.active_count}`).join("|");
  useEffect(() => {
    if (!enabled) return;
    let disposed = false; let index = 0;
    setData({});
    // Bounded demand loading: at most eight authorized Window details per page,
    // two in flight. This reuses the canonical projection, not a second trace.
    const worker = async () => {
      while (!disposed && index < rows.length) {
        const row = rows[index++];
        try { const value = await workspaceQuery<WindowDetail>({ kind: "window", client_window_key: row.client_window_key });
          if (!disposed) setData(current => ({ ...current, [row.client_window_key]: value }));
        } catch { /* A revoked/failed detail supplies no new facts. Opening it can retry. */ }
      }
    };
    void worker(); void worker();
    return () => { disposed = true; };
    // The key contains the exact visible identities and observation boundaries.
  }, [key, enabled, revision]);
  return data;
}

export function ActivityPanel({ activity }: { activity: ActivityEntry[] }) {
  const p = useProduct(); const s = useShellText(); const { locale } = useLocale(); const workspace = useWorkspace();
  const [tab, setTab] = useState<Tab>("windows"); const [selectedProject, setProject] = useState(""); const [page, setPage] = useState(0);
  const [selectedSessionFilter, setSessionFilter] = useState<SessionFilter>("all");
  const [filterContext, setFilterContext] = useState(workspace.contextKey);
  const authorizationLost = workspace.error && ["authenticationRequired", "permissionDenied"].includes(workspace.errorReason);
  // A new inventory or permission boundary must never render with the previous
  // context's filters while the cleanup effect is waiting to run.
  const filtersInvalid = filterContext !== workspace.contextKey || authorizationLost;
  const project = filtersInvalid ? "" : selectedProject;
  const sessionFilter = filtersInvalid ? "all" : selectedSessionFilter;
  useEffect(() => { setFilterContext(workspace.contextKey); setSessionFilter("all"); setProject(""); }, [workspace.contextKey]);
  useEffect(() => {
    if (authorizationLost) { setSessionFilter("all"); setProject(""); }
  }, [authorizationLost]);
  const clearSessionFilters = () => { setSessionFilter("all"); setProject(""); };
  const windows = useMemo(() => workspace.windows.filter(row => !project || row.last_project === project)
    .slice().sort((a, b) => (b.active_count > 0 ? 1 : 0) - (a.active_count > 0 ? 1 : 0) || (b.last_meaningful_activity_at_ms ?? b.last_seen_at_ms) - (a.last_meaningful_activity_at_ms ?? a.last_seen_at_ms)), [workspace.windows, project]);
  const historyPartial = Boolean(workspace.runner?.recent_sessions?.truncated || workspace.runner?.recent_sessions?.scan_truncated);
  const sessions = workspace.sessions.filter(row => (!project || row.project_id === project) && sessionMatchesFilter(row, sessionFilter));
  const visible = windows.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const previews = useWindowPreviews(visible, tab === "windows" && !workspace.state.current_operation, workspace.revision);
  useEffect(() => { setPage(0); }, [project, tab]);
  useEffect(() => { if (project && !workspace.projects.some(row => row.id === project)) setProject(""); }, [workspace.projects, project]);
  useEffect(() => { if (page * PAGE_SIZE >= windows.length && page > 0) setPage(0); }, [windows.length, page]);
  const projectLabel = (id?: string) => projectName(workspace.projects.find(row => row.id === id) || { id: id || "—" });
  const labels: Record<Tab, string> = { windows: p("toolCalls"), sessions: p("sessions"), system: p("serviceEvents") };
  const descriptions: Record<Tab, string> = { windows: p("callHistoryHelp"), sessions: p("sessionHistoryHelp"), system: p("serviceHistoryHelp") };
  return <div className="page-section workspace-page" data-webcodex-page="activity">
    <header className="page-heading-row"><h1>{p("activity")}</h1><button type="button" className="secondary-button" onClick={workspace.refresh} disabled={workspace.busy}>{p("refresh")}</button></header>
    <div className="workspace-tabs" role="tablist" aria-label={p("activity")}>{TABS.map(value => <button key={value} type="button" role="tab" id={`activity-tab-${value}`} aria-controls={`activity-view-${value}`} aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} onClick={() => setTab(value)} onKeyDown={event => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault(); const next = event.key === "Home" ? TABS[0] : event.key === "End" ? TABS[TABS.length - 1] : TABS[(TABS.indexOf(value) + (event.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length]; setTab(next); document.getElementById(`activity-tab-${next}`)?.focus();
    }}>{labels[value]}</button>)}</div>
    <p className="activity-description">{descriptions[tab]}</p>
    {tab !== "system" && <div className="activity-view-toolbar">
      <div className="activity-project-filter"><span className="filter-label">{s("Project")}</span><ProjectPicker label={s("Project")} allLabel={p("allProjects")} emptyLabel={p("noMatches")} searchLabel={p("search")} value={project} onChange={setProject} options={workspace.projects.filter(row => row.id).map(row => ({ value: row.id, label: projectName(row), detail: displayProjectPath(row.path) }))} /></div>
      {tab === "sessions" && <div className="activity-session-filter"><NativeSelect id="activity-session-filter" size="md" classNames={{ input: "ui-mantine-input", label: "ui-mantine-label" }} label={p("sessionFilter")} value={sessionFilter} onChange={event => setSessionFilter(event.currentTarget.value as SessionFilter)} data={[
        { value: "all", label: p("allSessions") }, { value: "running", label: p("runningSessions") }, { value: "attention", label: p("sessionsNeedAttention") },
      ]} /></div>}
      <div className="activity-counts" aria-live={tab === "sessions" ? "polite" : undefined}>{tab === "windows" ? <><span>{p("callSources")} <strong>{windows.length}</strong></span><span>{p("callsInProgress")} <strong>{windows.reduce((total, row) => total + row.active_count, 0)}</strong></span></> : <><span>{p("loadedSessions")} <strong>{workspace.sessions.length}</strong></span><span>{p("matchingLoadedSessions")} <strong>{sessions.length}</strong></span></>}</div>
    </div>}
    {tab === "sessions" && <div className="activity-session-filter-help"><p className="field-help">{p("sessionFilterHelp")}</p>
      {(project || sessionFilter !== "all") && sessions.length > 0 && <button type="button" className="secondary-button" onClick={clearSessionFilters}>{p("clearActivityFilters")}</button>}
    </div>}
    {workspace.loading && <p role="status" className="workspace-notice">{p("loading")}</p>}
    <section role="tabpanel" id={`activity-view-${tab}`} aria-labelledby={`activity-tab-${tab}`}>
    {tab === "windows" && <>
      {workspace.windowsError && <p role="alert">{p(workspace.windowsErrorReason)}</p>}
      {visible.map(row => {
        const detail = previews[row.client_window_key]; const latest = detail ? recentMeaningfulCalls(detail)[0] : undefined;
        const active = detail?.active_requests?.[0]; const tool = active?.tool_name ?? latest?.tool_name;
        const observedProject = active?.project ?? latest?.project ?? row.last_project;
        const linked = detail?.linked_sessions.slice().sort((a, b) => (b.last_linked_at_ms ?? 0) - (a.last_linked_at_ms ?? 0)).find(link => link.title && (!observedProject || link.project === observedProject));
        const title = linked?.title ? sessionTitle(linked.title) : tool ?? p("toolCalls");
        const outcome = latest ? executionLabel(latest.status) : "Unknown";
        const status = row.active_count ? p("inProgress") : p(outcome === "Completed" ? "callCompleted" : outcome === "Failed" ? "callFailed" : outcome === "Cancelled" ? "callCancelled" : "callNotConfirmed");
        return <button type="button" className="activity-work-row" key={row.client_window_key} aria-label={`${s("Open call details")}: ${title} · ${row.client_window_key.slice(-12)}`} onClick={() => workspace.setSelection({ kind: "window", id: row.client_window_key })}>
          <span className="activity-work-icon" aria-hidden="true">{row.active_count ? <TerminalSquare size={18} /> : <MessageSquare size={18} />}</span>
          <span className="activity-work-copy">
            <span className="session-row-heading"><strong>{title}</strong><span className={`workspace-badge ${row.active_count ? "working" : outcome === "Failed" ? "failed" : ""}`}>{status}</span></span>
            {tool && <code className="activity-tool-name">{tool}</code>}
            {row.active_count > 0 && <span className="activity-call-count">{p("callsInProgress")} {row.active_count}</span>}
            <span className="activity-row-meta"><span>{projectLabel(observedProject)}</span><time>{observationTime(row.last_meaningful_activity_at_ms || row.last_seen_at_ms, locale)}</time><ArrowRight size={16} aria-hidden="true" /></span>
          </span>
        </button>;
      })}
      {windows.length > PAGE_SIZE && <nav className="shell-actions activity-pagination" aria-label={p("toolCalls")}><button type="button" className="secondary-button" disabled={page === 0} onClick={() => setPage(value => value - 1)}>{s("Previous")}</button><span>{page + 1} / {Math.ceil(windows.length / PAGE_SIZE)}</span><button type="button" className="secondary-button" disabled={(page + 1) * PAGE_SIZE >= windows.length} onClick={() => setPage(value => value + 1)}>{s("Next")}</button></nav>}
      {!windows.length && !workspace.loading && <WorkspaceEmptyState kind="activity" message={p("noWindows")} action={<button type="button" className="secondary-button" onClick={workspace.refresh}>{p("refresh")}</button>} />}
    </>}
    {tab === "sessions" && <>
      {workspace.error && <p role="alert">{p(workspace.errorReason)}</p>}
      {sessions.map(session => {
        const running = session.running_call || session.running_jobs > 0;
        const validation = session.overview.validation?.state;
        return <button type="button" className="activity-work-row workspace-session-row" key={`${session.project_id}:${session.session_id}`} onClick={() => session.project_id && workspace.setSelection({ kind: "session", project: session.project_id, id: session.session_id })}>
          <span className="activity-work-icon" aria-hidden="true">{running ? <TerminalSquare size={18} /> : <MessageSquare size={18} />}</span>
          <span className="activity-work-copy">
            <span className="session-row-heading"><strong>{sessionTitle(session.title)}</strong><span className={`workspace-badge ${running ? "working" : ""}`}>{running ? p("inProgress") : sessionLifecycle(session.lifecycle, p)}</span></span>
            {session.overview.reported_progress?.text && <span className="session-row-task">{session.overview.reported_progress.text}</span>}
            <SessionAttention session={session} />
            {validation && ["passed", "failed", "inconclusive", "not_run"].includes(validation) && <span className={`session-validation ${validation === "failed" ? "failed" : ""}`}>{validationLabel(validation, p)}</span>}
            <span className="activity-row-meta"><span>{projectLabel(session.project_id)}</span><time>{observationTime(session.updated_at * 1000, locale)}</time><ArrowRight size={16} aria-hidden="true" /></span>
          </span>
        </button>;
      })}
      {!sessions.length && !workspace.loading && <WorkspaceEmptyState kind="activity" message={p(project || sessionFilter !== "all" ? "noMatchingSessions" : historyPartial ? "noObservedSessions" : "noSessions")} action={project || sessionFilter !== "all" ? <button type="button" className="secondary-button" onClick={clearSessionFilters}>{p("clearActivityFilters")}</button> : <button type="button" className="secondary-button" onClick={workspace.refresh}>{p("refresh")}</button>} />}
      {historyPartial && <p className="field-help">{s("History is partial")}</p>}
    </>}
    {tab === "system" && <SystemActivity activity={activity} />}
    </section>
    {workspace.selection?.kind === "window" && <WindowActivityDetail id={workspace.selection.id} onClose={() => workspace.setSelection(null)} />}
    {workspace.selection?.kind === "session" && <WorkflowSessionDetail project={workspace.selection.project} id={workspace.selection.id} onClose={() => workspace.setSelection(null)} />}
  </div>;
}
