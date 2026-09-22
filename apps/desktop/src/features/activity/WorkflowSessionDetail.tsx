import { useEffect, useState } from "react";
import type { GitSummary, SessionActivity, WorkflowSession } from "../../models/workspace";
import { useProduct, type ProductKey } from "../../i18n/product";
import { useLocale } from "../../i18n/locale";
import { useShellText } from "../../i18n/runtime-shell";
import { sessionTitle, workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { observationTime } from "../workspace/WorkspaceStatus";

export function activityTitle(activity: SessionActivity | undefined, p: (key: ProductKey) => string): string {
  if (!activity) return p("noActivity");
  if (activity.summary) return activity.summary;
  const labels: Record<string, ProductKey> = { Exploring: "exploration", Explored: "exploration", Edited: "editing", Reviewing: "review", Validating: "validation", Running: "jobs", exploration: "exploration", edit: "editing", review: "review", validation: "validation", run: "jobs" };
  return p(labels[activity.kind] || "recentActivity");
}
export function sessionLifecycle(lifecycle: string, p: (key: ProductKey) => string): string {
  const labels: Record<string, ProductKey> = { active: "active", finalized: "completed", closed: "completed", completed: "completed", archived: "archived", paused: "paused" };
  return labels[lifecycle] ? p(labels[lifecycle]) : p("unknown");
}
export function SessionAttention({ session }: { session: WorkflowSession }) {
  const p = useProduct(); const attention = session.overview.attention;
  return <span className="session-attention"><span>{p("jobs")} {session.running_jobs}{!session.running_jobs_complete && "+"}</span>
    {attention.open_todos > 0 && <span>{p("todos")} {attention.open_todos}</span>}
    {attention.open_questions > 0 && <span>{p("questions")} {attention.open_questions}</span>}
    {attention.open_risks > 0 && <span className="risk-count">{p("risks")} {attention.open_risks}</span>}
  </span>;
}
export function WorkflowSessionDetail({ project, id, onClose }: { project: string; id: string; onClose: () => void }) {
  const p = useProduct(); const s = useShellText(); const { locale } = useLocale();
  const [session, setSession] = useState<WorkflowSession | null>(null);
  const [git, setGit] = useState<GitSummary | null>(null);
  const [failed, setFailed] = useState(false);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setSession(null); setGit(null); setFailed(false);
    void workspaceQuery<WorkflowSession>({ kind: "session", project, session_id: id }).then(next => { if (!cancelled) setSession(next); }).catch(() => { if (!cancelled) setFailed(true); });
    void workspaceQuery<GitSummary>({ kind: "project_git", project }).then(next => { if (!cancelled) setGit(next); }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [project, id, revision]);
  const activity = [...(session?.activity || [])].sort((a, b) => (b.started_at || 0) - (a.started_at || 0));
  return <WorkspaceDialog title={session ? sessionTitle(session.title) : p("sessions")} onClose={onClose}>
    {failed && <p role="alert">{p("loadError")} <button className="text-button" onClick={() => setRevision(value => value + 1)}>{p("refresh")}</button></p>}
    {!session && !failed && <p role="status">{p("loading")}</p>}
    {session && <>
      <div className="session-detail-meta"><span className="workspace-badge">{sessionLifecycle(session.lifecycle, p)}</span><span>{observationTime(session.updated_at * 1000, locale)}</span></div>
      <SessionAttention session={session} />
      <dl className="runtime-facts"><div><dt>{s("Validation")}</dt><dd>{session.overview.validation?.state ?? s("No validation evidence")}</dd></div><div><dt>{s("Requests in progress")}</dt><dd>{session.running_call ? 1 : 0}</dd></div></dl>
      <button type="button" className="secondary-button" onClick={() => setRevision(value => value + 1)}>{p("refresh")}</button>
      <section className="workspace-section"><h3>{p("task")}</h3><p>{session.overview.reported_progress?.text || activityTitle(activity[0], p)}</p></section>
      <section className="workspace-section"><h3>{p("recentActivity")}</h3><div className="workspace-timeline">{activity.slice(0, 30).map((entry, index) => <article key={`${entry.started_at}-${index}`}>
        <div><strong>{activityTitle(entry, p)}</strong>{entry.paths && entry.paths.length > 0 && <span>{entry.paths.join(" · ")}</span>}</div>
        <span className="activity-outcome">{entry.state === "running" ? p("inProgress") : entry.state === "failed" ? p("failed") : entry.state === "completed" || entry.state === "succeeded" ? p("completed") : p("observed")}</span>
        <time>{observationTime(entry.finished_at ? entry.finished_at * 1000 : entry.started_at ? entry.started_at * 1000 : null, locale)}</time>
      </article>)}</div>{session.activity_truncated && <p className="workspace-notice">{p("partial")}</p>}</section>
      <section className="workspace-section"><h3>{p("files")}</h3><p className="field-help">{p("currentGit")}</p>
        {git ? <><p>{git.branch || p(git.non_git_project ? "notGit" : "unknown")}</p>{git.files?.length ? <ul className="changed-files">{git.files.map(file => <li key={file.path}><span>{file.path}</span><small>{file.status}</small></li>)}</ul> : <p>{p(git.clean ? "clean" : git.non_git_project ? "notGit" : "unknown")}</p>}{git.files_truncated && <p>{p("partial")}</p>}</> : <p>{p("unknown")}</p>}
      </section>
      <details className="workspace-technical"><summary>{p("details")}</summary><code>{session.session_id}</code></details>
    </>}
  </WorkspaceDialog>;
}
