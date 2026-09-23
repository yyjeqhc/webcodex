import { useEffect, useState } from "react";
import type { WindowDetail } from "../../models/workspace";
import { useLocale } from "../../i18n/locale";
import { useProduct } from "../../i18n/product";
import { useShellText } from "../../i18n/runtime-shell";
import { projectName, useWorkspace, workspaceQuery } from "../workspace/WorkspaceContext";
import { WorkspaceDialog } from "../workspace/WorkspaceDialog";
import { observationTime } from "../workspace/WorkspaceStatus";
import { ContinuationFacts } from "./ContinuationFacts";
import { continuationFromWindow, executionLabel, recentMeaningfulCalls } from "./window-evidence";

export function WindowActivityDetail({ id, onClose }: { id: string; onClose: () => void }) {
  const p = useProduct(); const s = useShellText(); const { locale } = useLocale(); const workspace = useWorkspace();
  const [observed, setObserved] = useState<{ detail: WindowDetail; at: number } | null>(null);
  const [failed, setFailed] = useState(false); const [revision, setRevision] = useState(0);
  useEffect(() => {
    let cancelled = false; let timer: number | undefined;
    const load = async () => {
      try { const detail = await workspaceQuery<WindowDetail>({ kind: "window", client_window_key: id });
        if (!cancelled) { setObserved({ detail, at: Date.now() }); setFailed(false); }
      } catch { if (!cancelled) { setFailed(true); setObserved(null); } }
      finally { if (!cancelled) timer = window.setTimeout(() => void load(), 8000); }
    };
    void load(); return () => { cancelled = true; if (timer) window.clearTimeout(timer); };
  }, [id, revision]);
  const detail = observed?.detail;
  const name = (project?: string) => projectName(workspace.projects.find(row => row.id === project) || { id: project || "—" });
  const calls = detail ? recentMeaningfulCalls(detail) : [];
  return <WorkspaceDialog title={`${s("Request details")} · ${id.slice(-12)}`} onClose={onClose}>
    <div className="shell-actions"><button type="button" className="secondary-button" onClick={() => setRevision(value => value + 1)}>{p("refresh")}</button></div>
    {failed && <p role="alert">{p("loadError")}</p>}
    {!detail && !failed && <p role="status">{p("loading")}</p>}
    {detail && <>
      <div className="session-detail-meta"><span className="workspace-badge">{detail.active_count ? `${s("Requests in progress")}: ${detail.active_count}` : s("Observed calls")}</span><span>{observationTime(detail.last_meaningful_activity_at_ms || detail.last_seen_at_ms, locale)}</span></div>
      {!!detail.active_requests?.length && <section className="shell-subsection"><h3>{s("Requests in progress")}</h3>{detail.active_requests.map(request => <article className="active-request-row" key={request.server_trace_id}>
        <strong>{request.tool_name ?? s("Unknown")}</strong><span>{name(request.project)}</span><time>{Math.max(0, Math.floor(request.elapsed_ms / 1000))} s</time>
        <details><summary>{s("Details")}</summary><code>{request.server_trace_id}</code></details>
      </article>)}</section>}
      <ContinuationFacts value={continuationFromWindow(detail, observed!.at)} observedAt={observed!.at} />
      <section className="shell-subsection"><h3>{s("Associated work")}</h3>
        {detail.linked_sessions.map(session => <button type="button" className="workspace-activity-link" key={`${session.project}:${session.workflow_session_id}`} disabled={!session.project || !session.workflow_session_id} onClick={() => {
          if (session.project && session.workflow_session_id) workspace.setSelection({ kind: "session", project: session.project, id: session.workflow_session_id });
        }}><strong>{session.title || session.workflow_session_id?.slice(-12)}</strong><span>{name(session.project)}</span><span aria-hidden="true">→</span></button>)}
        {!detail.linked_sessions.length && <p>{p("noSessions")}</p>}{detail.sessions_truncated && <p className="field-help">{s("History is partial")}</p>}
      </section>
      <section className="shell-subsection"><h3>{s("Observed calls")}</h3><ol className="call-history">{calls.slice(0, 30).map((row, index) => <li key={`${row.started_at_ms}-${index}`}>
        <div><strong>{row.tool_name ?? s("Unknown")}</strong><span>{name(row.project)}</span></div><span>{s(executionLabel(row.status))}</span><time>{observationTime(row.ended_at_ms || row.started_at_ms, locale)}</time>
        <details><summary>{s("Details")}</summary>{row.server_trace_id && <code>{row.server_trace_id}</code>}<dl className="runtime-facts"><div><dt>{s("Response")}</dt><dd>{s(row.response_handed_at_ms == null ? "Response handoff not confirmed" : row.response_streaming ? "Response stream started" : "Returned to HTTP framework")}</dd></div>
          <div><dt>{s("Service time")}</dt><dd>{row.service_ms == null ? "—" : `${row.service_ms} ms`}</dd></div><div><dt>{s("Gap from preceding response")}</dt><dd>{row.next_call_gap_ms == null ? s("Not observed") : `${row.next_call_gap_ms} ms`}</dd></div></dl></details>
      </li>)}</ol>{!calls.length && <p>{s("Waiting for the first meaningful call")}</p>}{detail.activity_truncated && <p className="field-help">{s("History is partial")}</p>}</section>
      <details className="workspace-technical"><summary>{s("Details")}</summary><code>{id}</code></details>
    </>}
  </WorkspaceDialog>;
}
