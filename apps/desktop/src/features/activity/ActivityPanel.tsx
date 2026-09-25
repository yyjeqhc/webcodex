import { useEffect, useMemo, useState } from "react";
import { MessageSquare, TerminalSquare, ArrowRight } from "lucide-react";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import type { ActivityEntry } from "../../models/topology";
import type { WindowDetail, WindowSummary } from "../../models/workspace";
import { displayProjectPath, projectName, sessionTitle, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { observationTime } from "../workspace/WorkspaceStatus";
import { WindowActivityDetail } from "./WindowActivityDetail";
import { WorkflowSessionDetail, sessionLifecycle, SessionAttention } from "./WorkflowSessionDetail";
import { SystemActivity } from "./SystemActivity";
import { WorkspaceEmptyState } from "../../components/WorkspaceEmptyState";
import { ProjectPicker } from "../../../../../frontend/src/ui/ProjectPicker";
import { executionLabel, recentMeaningfulCalls } from "./window-evidence";

type Tab = "windows" | "sessions" | "system";
const TABS: Tab[] = ["windows", "sessions", "system"];
const PAGE_SIZE = 8;

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
  const [tab, setTab] = useState<Tab>("windows"); const [project, setProject] = useState(""); const [page, setPage] = useState(0);
  const windows = useMemo(() => workspace.windows.filter(row => !project || row.last_project === project)
    .slice().sort((a, b) => (b.active_count > 0 ? 1 : 0) - (a.active_count > 0 ? 1 : 0) || (b.last_meaningful_activity_at_ms ?? b.last_seen_at_ms) - (a.last_meaningful_activity_at_ms ?? a.last_seen_at_ms)), [workspace.windows, project]);
  const sessions = workspace.sessions.filter(row => !project || row.project_id === project);
  const visible = windows.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const previews = useWindowPreviews(visible, tab === "windows" && !workspace.state.current_operation, workspace.revision);
  useEffect(() => { setPage(0); }, [project, tab]);
  useEffect(() => { if (project && !workspace.projects.some(row => row.id === project)) setProject(""); }, [workspace.projects, project]);
  useEffect(() => { if (page * PAGE_SIZE >= windows.length && page > 0) setPage(0); }, [windows.length, page]);
  const projectLabel = (id?: string) => projectName(workspace.projects.find(row => row.id === id) || { id: id || "—" });
  const labels: Record<Tab, string> = { windows: s("ChatGPT calls"), sessions: s("Workflow Sessions"), system: s("System events") };
  return <div className="page-section workspace-page" data-webcodex-page="activity">
    <header className="page-heading-row"><h1>{p("activity")}</h1><button type="button" className="secondary-button" onClick={workspace.refresh}>{p("refresh")}</button></header>
    <p className="field-help activity-explainer">{s("Calls show observed MCP execution and WebCodex handler return. Sessions group durable work; system events describe Desktop-owned services.")}</p>
    <div className="workspace-tabs" role="tablist" aria-label={p("activity")}>{TABS.map(value => <button key={value} type="button" role="tab" id={`activity-tab-${value}`} aria-controls={`activity-view-${value}`} aria-selected={tab === value} tabIndex={tab === value ? 0 : -1} onClick={() => setTab(value)} onKeyDown={event => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault(); const next = TABS[(TABS.indexOf(value) + (event.key === "ArrowRight" ? 1 : TABS.length - 1)) % TABS.length]; setTab(next); document.getElementById(`activity-tab-${next}`)?.focus();
    }}>{labels[value]}</button>)}</div>
    {tab !== "system" && <div className="activity-project-filter"><span className="filter-label">{s("Project")}</span><ProjectPicker label={s("Project")} allLabel={p("allProjects")} emptyLabel={p("noMatches")} searchLabel={p("search")} value={project} onChange={setProject} options={workspace.projects.filter(row => row.id).map(row => ({ value: row.id, label: projectName(row), detail: displayProjectPath(row.path) }))} /></div>}
    {workspace.loading && <p role="status" className="workspace-notice">{p("loading")}</p>}
    <section role="tabpanel" id={`activity-view-${tab}`} aria-labelledby={`activity-tab-${tab}`}>
    {tab === "windows" && <>
      <div className="activity-overview"><span>{s("Calls in progress")}: <strong>{windows.reduce((total, row) => total + row.active_count, 0)}</strong></span><span>{s("Recent work")}: <strong>{windows.length}</strong></span></div>
      {workspace.windowsError && <p role="alert">{p("loadError")}</p>}
      {visible.map(row => {
        const detail = previews[row.client_window_key]; const latest = detail ? recentMeaningfulCalls(detail)[0] : undefined;
        const linked = detail?.linked_sessions.slice().sort((a, b) => (b.last_linked_at_ms ?? 0) - (a.last_linked_at_ms ?? 0)).find(link => link.title);
        const active = detail?.active_requests?.[0]; const tool = active?.tool_name ?? latest?.tool_name;
        const title = linked?.title ? sessionTitle(linked.title) : tool ?? `${s("ChatGPT calls")} · ${row.client_window_key.slice(-12)}`;
        return <button type="button" className="activity-work-row" key={row.client_window_key} aria-label={`${s("Open call details")}: ${title} · ${row.client_window_key.slice(-12)}`} onClick={() => workspace.setSelection({ kind: "window", id: row.client_window_key })}>
          <span className="activity-work-icon" aria-hidden="true">{row.active_count ? <TerminalSquare size={18} /> : <MessageSquare size={18} />}</span>
          <span className="activity-work-copy"><strong>{title}</strong><span>{projectLabel(latest?.project ?? row.last_project)}{tool ? ` · ${tool}` : ""}</span>
            <span className="activity-evidence"><span>{row.active_count ? `${s("Requests in progress")}: ${row.active_count}` : latest ? s(executionLabel(latest.status)) : s("Not observed")}</span>
              {latest && !row.active_count && <span>{s(latest.response_handed_at_ms == null ? "Response handoff not confirmed" : latest.response_streaming ? "Response stream started" : "Returned to HTTP framework")}</span>}
              <span>{row.linked_session_count} {s("Workflow Sessions")}</span></span></span>
          <span className="activity-work-trailing"><time>{observationTime(row.last_meaningful_activity_at_ms || row.last_seen_at_ms, locale)}</time><ArrowRight size={16} aria-hidden="true" /></span>
        </button>;
      })}
      {windows.length > PAGE_SIZE && <nav className="shell-actions" aria-label={s("ChatGPT calls")}><button type="button" className="secondary-button" disabled={page === 0} onClick={() => setPage(value => value - 1)}>{s("Previous")}</button><span>{page + 1} / {Math.ceil(windows.length / PAGE_SIZE)}</span><button type="button" className="secondary-button" disabled={(page + 1) * PAGE_SIZE >= windows.length} onClick={() => setPage(value => value + 1)}>{s("Next")}</button></nav>}
      {!windows.length && !workspace.loading && <WorkspaceEmptyState kind="activity" message={p("noWindows")} action={<button type="button" className="secondary-button" onClick={workspace.refresh}>{p("refresh")}</button>} />}
    </>}
    {tab === "sessions" && <>
      {workspace.error && <p role="alert">{p("loadError")}</p>}
      {sessions.map(session => <button type="button" className="workspace-session-row" key={`${session.project_id}:${session.session_id}`} onClick={() => session.project_id && workspace.setSelection({ kind: "session", project: session.project_id, id: session.session_id })}>
        <div className="session-row-heading"><strong>{sessionTitle(session.title)}</strong><span className={`workspace-badge ${session.running_call || session.running_jobs ? "working" : ""}`}>{session.running_call || session.running_jobs ? p("inProgress") : sessionLifecycle(session.lifecycle, p)}</span></div>
        <span className="session-row-project">{projectLabel(session.project_id)} · {observationTime(session.updated_at * 1000, locale)}</span>
        {session.overview.reported_progress?.text && <span className="session-row-task">{session.overview.reported_progress!.text}</span>}
        <SessionAttention session={session} />
        <div className="session-attention"><span>{s("Active Jobs")}: {session.running_jobs ?? 0}</span><span>{s("Validation")}: {session.overview.validation?.state ?? s("No validation evidence")}</span></div>
      </button>)}
      {!sessions.length && !workspace.loading && <WorkspaceEmptyState kind="activity" message={p("noSessions")} action={<button type="button" className="secondary-button" onClick={workspace.refresh}>{p("refresh")}</button>} />}
      {workspace.runner?.recent_sessions?.truncated && <p className="field-help">{s("History is partial")}</p>}
    </>}
    {tab === "system" && <SystemActivity activity={activity} />}
    </section>
    {workspace.selection?.kind === "window" && <WindowActivityDetail id={workspace.selection.id} onClose={() => workspace.setSelection(null)} />}
    {workspace.selection?.kind === "session" && <WorkflowSessionDetail project={workspace.selection.project} id={workspace.selection.id} onClose={() => workspace.setSelection(null)} />}
  </div>;
}
