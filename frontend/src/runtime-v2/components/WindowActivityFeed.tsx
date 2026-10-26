import { Select, TextInput } from "@mantine/core";
import { memo, useCallback, useMemo, useRef, useState } from "react";
import { useFlowingSelection } from "../../ui/useFlowingSelection.js";
import type { RuntimeV2Client } from "../api/client.js";
import { TraceCallDetails } from "./TraceCallDetails.js";
import { CopyIdentity } from "./ui/CopyIdentity.js";
import { displayProjectPath } from "../../ui/projectPresentation.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, clockTime, durationText, shortId } from "../model/format.js";
import type { ProjectRow, WindowDetail, WindowJob, WindowLinkedSession } from "../model/types.js";
import { windowActivityIdentity } from "../model/windowActivity.js";
import { focusedWindowCallKeys, windowSessionCatalog } from "../model/windowSessions.js";

const CALL_PAGE_SIZE = 80;
type Call = {
  key: string; traceRef?: string; tool: string; project?: string; startedAt: number; duration: number;
  status: string; expectation?: string; sessions: string[]; asyncJobId?: string; observedJobIds: string[]; running: boolean;
};
type Props = {
  client?: RuntimeV2Client;
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  selectedSessionId?: string;
  onSelectSession?: (sessionId: string) => void;
  onOpenSessionRecord?: (project: string, sessionId: string) => void;
};
const failureStatuses = new Set(["error", "failed", "failure", "cancelled", "canceled", "timeout", "timed_out"]);
const mismatches = new Set(["expectation_mismatch", "unexpected_success", "unexpected_failure"]);
const isFailedCall = (call: Call) => failureStatuses.has(call.status.toLowerCase()) || mismatches.has(call.expectation || "");

export function WindowActivityFeed(props: Props) {
  // Paging, filters and expansion belong to this exact Window. Row unmounts caused
  // by filtering/paging do not discard its expansion choices.
  return <WindowActivityContent key={props.detail.client_window_key} {...props} />;
}

function WindowActivityContent({
  client, detail, projects, language, selectedSessionId = "", onSelectSession, onOpenSessionRecord,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [order, setOrder] = useState<"newest" | "oldest">("newest");
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<"all" | "failed" | "running">("all");
  const [page, setPage] = useState({ criteria: "", count: CALL_PAGE_SIZE });
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const changeExpanded = useCallback((key: string, open: boolean) => setExpanded(current => {
    const next = new Set(current);
    if (open) next.add(key); else next.delete(key);
    return next;
  }), []);
  const orderRef = useRef<HTMLFieldSetElement | null>(null);
  useFlowingSelection(orderRef, order, { variant: "filled", itemSelector: "label", selectedSelector: "label:has(input:checked)" });

  const projectIndex = useMemo(() => new Map(projects.map(project => [project.id, project])), [projects]);
  const orderedSessions = useMemo(() => windowSessionCatalog(detail), [detail.activity, detail.linked_sessions]);
  const sessionOrder = useMemo(() => new Map(orderedSessions.map((session, index) => [session.workflow_session_id, index])), [orderedSessions]);
  const sessionMeta = useMemo(() => new Map(orderedSessions.map(session => [session.workflow_session_id, session])), [orderedSessions]);
  const jobsById = useMemo(() => new Map((detail.jobs || []).map(job => [job.job_id, job])), [detail.jobs]);

  // Completed projections stay stable while live elapsed times or Window metadata
  // change. A changed activity field is still observed through the activity array.
  const completedCalls = useMemo<Call[]>(() => {
    const occurrences = new Map<string, number>();
    return detail.activity.map(row => {
      const identity = windowActivityIdentity(row);
      const occurrence = occurrences.get(identity) || 0;
      occurrences.set(identity, occurrence + 1);
      return {
        key: identity + "#" + occurrence, traceRef: row.server_trace_id, tool: row.tool_name || row.method,
        project: row.project, startedAt: row.started_at_ms, duration: row.duration_ms, status: row.status,
        expectation: row.failure_expectation_result,
        sessions: [...new Set(row.workflow_sessions.map(link => link.workflow_session_id))].sort((left, right) =>
          (sessionOrder.get(left) ?? Number.MAX_SAFE_INTEGER) - (sessionOrder.get(right) ?? Number.MAX_SAFE_INTEGER)),
        asyncJobId: row.async_job_id, observedJobIds: row.observed_job_ids || [], running: false,
      };
    });
  }, [detail.activity, sessionOrder]);
  const completedTraces = useMemo(() => new Set(completedCalls.map(call => call.traceRef).filter(Boolean)), [completedCalls]);
  const activeCalls = useMemo<Call[]>(() => detail.active_requests
    .filter(row => !completedTraces.has(row.server_trace_id))
    .map(row => ({
      key: "trace:" + row.server_trace_id + "#0", traceRef: row.server_trace_id,
      tool: row.tool_name || row.method, project: row.project, startedAt: row.started_at_ms,
      duration: row.elapsed_ms, status: "running", sessions: [], observedJobIds: [], running: true,
    })), [detail.active_requests, completedTraces]);
  const calls = useMemo(() => [...completedCalls, ...activeCalls].sort((left, right) =>
    left.startedAt - right.startedAt || left.key.localeCompare(right.key)), [completedCalls, activeCalls]);
  const matchingCalls = useMemo(() => {
    // Session segments must be resolved on the complete chronological history
    // before text/status filtering or any presentation bound.
    const focusedKeys = selectedSessionId ? focusedWindowCallKeys(calls, selectedSessionId) : null;
    const needle = query.trim().toLowerCase();
    return calls.filter(call => {
      if (focusedKeys && !focusedKeys.has(call.key)) return false;
      if (!needle) return true;
      const project = call.project ? projectIndex.get(call.project) : undefined;
      const text = [call.tool, call.status, call.expectation, call.project, project?.name, project?.path,
        call.traceRef, call.asyncJobId, ...call.observedJobIds, ...call.sessions,
        ...call.sessions.map(id => sessionMeta.get(id)?.title)].filter(Boolean).join(" ").toLowerCase();
      return text.includes(needle);
    });
  }, [calls, selectedSessionId, query, projectIndex, sessionMeta]);
  const failedCount = useMemo(() => matchingCalls.filter(isFailedCall).length, [matchingCalls]);
  const runningCount = useMemo(() => matchingCalls.filter(call => call.running).length, [matchingCalls]);
  const filteredCalls = useMemo(() => {
    const rows = statusFilter === "all" ? matchingCalls : matchingCalls.filter(
      statusFilter === "failed" ? isFailedCall : call => call.running);
    return order === "newest" ? rows.slice().reverse() : rows;
  }, [matchingCalls, statusFilter, order]);
  const criteria = JSON.stringify([selectedSessionId, query, statusFilter, order]);
  const visibleCount = page.criteria === criteria ? page.count : CALL_PAGE_SIZE;
  const visibleCalls = useMemo(() => filteredCalls.slice(0, visibleCount), [filteredCalls, visibleCount]);
  const selectedProject = sessionMeta.get(selectedSessionId)?.project;
  const selectedTone = selectedSessionId ? (sessionOrder.get(selectedSessionId) ?? 0) % 8 : 0;
  const sessionOptions = useMemo(() => [
    { value: "", label: translate("All calls", language) },
    ...(selectedSessionId && !sessionMeta.has(selectedSessionId)
      ? [{ value: selectedSessionId, label: translate("Session", language) + " · " + shortId(selectedSessionId, 14, 6) }] : []),
    ...orderedSessions.map((session, index) => ({
      value: session.workflow_session_id,
      label: (session.title || translate("Work Session", language) + " " + (index + 1)) + " · " + shortId(session.workflow_session_id, 14, 6),
    })),
  ], [orderedSessions, sessionMeta, selectedSessionId, language]);

  return <section className="window-detail-section window-workflow-section" aria-label={t("Window activity")}>
    <div className="window-activity-toolbar" role="group" aria-label={t("Tool calls")}>
      {(orderedSessions.length > 0 || selectedSessionId) && <div className="window-session-focus">
        <span>{t("Session")}</span>
        <div className="window-session-focus-select" data-session-tone={selectedTone} data-active={Boolean(selectedSessionId)}>
          <span className="window-session-color-dot" />
          <Select aria-label={t("Session filter")} value={selectedSessionId} allowDeselect={false} size="xs"
            onChange={value => onSelectSession?.(value ?? "")} data={sessionOptions} />
        </div>
        <small>{filteredCalls.length}/{calls.length}</small>
      </div>}
      <TextInput className="window-call-search" aria-label={t("Search calls")} placeholder={t("Search calls")}
        value={query} onChange={event => setQuery(event.currentTarget.value)} size="xs" />
      <fieldset ref={orderRef} className="window-activity-order">
        <legend>{t("Activity order")}</legend>
        {(["newest", "oldest"] as const).map(value => <label key={value}>
          <input type="radio" name="window-activity-order" value={value} checked={order === value} onChange={() => setOrder(value)} />
          <span>{t(value === "newest" ? "Newest first" : "Oldest first")}</span>
        </label>)}
      </fieldset>
      <div className="window-call-filters" role="group" aria-label={t("Call status")}>
        <button type="button" className="quiet-pill" aria-pressed={statusFilter === "all"} onClick={() => setStatusFilter("all")}>{t("All calls")} ({matchingCalls.length})</button>
        <button type="button" className="quiet-pill" aria-pressed={statusFilter === "failed"} onClick={() => setStatusFilter("failed")}>{t("Failed calls")} ({failedCount})</button>
        <button type="button" className="quiet-pill" aria-pressed={statusFilter === "running"} onClick={() => setStatusFilter("running")}>{t("Running calls")} ({runningCount})</button>
      </div>
      {selectedProject && onOpenSessionRecord && <button type="button" className="text-button" onClick={() => onOpenSessionRecord(selectedProject, selectedSessionId)}>{t("View Session record")}</button>}
      {detail.sessions_truncated && <p className="inventory-note">{t("Some linked Sessions are not available in this view.")}</p>}
      {selectedSessionId && <CopyIdentity key={selectedSessionId} value={selectedSessionId} label={t("Session")} language={language} />}
    </div>
    <div className="window-activity-results">
      <p className="inventory-note" data-testid="window-call-count" role="status">
        {t("Calls shown")}: {visibleCalls.length}/{filteredCalls.length} · {t("Loaded calls")}: {calls.length}
      </p>
      {selectedSessionId && !matchingCalls.length && !query && <p className="inventory-note">{t("This Session is linked to the Window but has no retained calls.")}</p>}
      {detail.active_count > detail.active_requests.length && <div className="inventory-note">{t("Some running calls are not shown.")} {detail.active_requests.length}/{detail.active_count}</div>}
      {detail.activity_truncated && <div className="inventory-note">{t("Earlier calls are not available in this view. Only retained activity is shown.")}</div>}
      <div className="window-workflow-list">
        {visibleCalls.map(call => <WindowCallCard key={call.key} call={call}
          projectPath={call.project ? projectIndex.get(call.project)?.path : undefined}
          sessionMeta={sessionMeta} sessionOrder={sessionOrder} asyncJob={call.asyncJobId ? jobsById.get(call.asyncJobId) : undefined}
          activeSessionId={selectedSessionId} client={client} language={language} onSelectSession={onSelectSession}
          expanded={expanded.has(call.key)} onExpandedChange={changeExpanded} />)}
        {!visibleCalls.length && <div className="empty-inline">{t(query || statusFilter !== "all"
          ? "No retained calls match these filters." : selectedSessionId ? "No calls in this Session" : "No tool calls yet")}</div>}
      </div>
      {visibleCalls.length < filteredCalls.length && <button type="button" className="text-button window-call-more"
        onClick={() => setPage({ criteria, count: visibleCount + CALL_PAGE_SIZE })}>{t("Show more calls")} · {filteredCalls.length - visibleCalls.length}</button>}
    </div>
  </section>;
}

const WindowCallCard = memo(function WindowCallCard({
  call, projectPath, sessionMeta, sessionOrder, asyncJob, activeSessionId, client, language, onSelectSession, expanded, onExpandedChange,
}: {
  call: Call; projectPath?: string; sessionMeta: ReadonlyMap<string, WindowLinkedSession>; sessionOrder: ReadonlyMap<string, number>;
  asyncJob?: WindowJob; activeSessionId: string; client?: RuntimeV2Client; language: RuntimeLanguage;
  onSelectSession?: (sessionId: string) => void; expanded: boolean; onExpandedChange: (key: string, open: boolean) => void;
}) {
  const t = (value: string) => translate(value, language);
  const path = projectPath ? displayProjectPath(projectPath) : undefined;
  const success = ["ok", "success", "succeeded"].includes(call.status);
  const expected = ["matched_expected_failure", "matched_expected_result"].includes(call.expectation || "");
  const mismatch = mismatches.has(call.expectation || "");
  const expectationLabel = expected ? "Expected result" : mismatch ? "Expectation not met" : "";
  const status = call.running ? "Running" : success ? "Succeeded" : failureStatuses.has(call.status.toLowerCase()) ? "Failed" : call.status;
  const primarySession = call.sessions.find(sessionId => sessionMeta.has(sessionId));
  const selectable = Boolean(primarySession && onSelectSession);
  const asyncJobLabel = asyncJob ? asyncJob.active ? t("Background running")
    : asyncJob.status === "completed" ? t("Completed") : t(asyncJob.status) : t("Background job");
  const asyncJobDuration = asyncJob?.active && asyncJob.elapsed_secs !== undefined ? durationText(asyncJob.elapsed_secs * 1000)
    : asyncJob?.duration_ms !== undefined ? durationText(asyncJob.duration_ms) : "";
  return <article className={"window-call-card" + (call.running ? " running" : "") + (selectable ? " session-linked" : "")}
    data-testid="window-workflow-step" onClick={() => primarySession && onSelectSession?.(primarySession)}>
    <header>
      <strong>{call.tool}</strong>
      <span className={"status-pill " + (call.running ? "" : expected || (success && !mismatch) ? "good" : "warn")}>
        {t(status)}{!call.running && expectationLabel ? " · " + t(expectationLabel) : ""}
      </span>
    </header>
    {(path || call.sessions.length > 0 || call.asyncJobId || call.observedJobIds.length > 0) && <div className="window-call-context">
      {path && <span className="window-project-tag" data-testid="window-project-tag" title={path}>{path}</span>}
      {call.sessions.map(sessionId => {
        const linked = sessionMeta.get(sessionId);
        const selected = activeSessionId === sessionId;
        const tagSelectable = Boolean(linked && onSelectSession);
        return <button className={"window-session-tag" + (selected ? " selected" : "")}
          data-session-tone={(sessionOrder.get(sessionId) ?? 0) % 8} data-testid="window-session-tag" key={sessionId} type="button"
          disabled={!tagSelectable} title={(linked?.title ? linked.title + " · " : "") + sessionId}
          onClick={event => { event.stopPropagation(); if (tagSelectable) onSelectSession?.(selected ? "" : sessionId); }}>
          <span className="window-session-color-dot" />{shortId(sessionId, 14, 6)}
        </button>;
      })}
      {call.asyncJobId && <span className={"window-job-tag" + (asyncJob?.active ? " active" : "")} title={call.asyncJobId}>
        <span className="window-job-dot" />{asyncJobLabel} · {shortId(call.asyncJobId, 14, 6)}
        {asyncJobDuration && <small> · {asyncJobDuration}</small>}
      </span>}
      {call.observedJobIds.map(jobId => <span className="window-job-tag observe" title={jobId} key={jobId}>
        <span className="window-job-dot" />{t("Observing")} · {shortId(jobId, 14, 6)}
      </span>)}
    </div>}
    <div className="window-call-timing">
      {call.running ? <span className="window-call-live-time">{t("Running")} · <strong>{durationText(call.duration)}</strong></span>
        : <><span className="window-call-clock">
          <time dateTime={new Date(call.startedAt).toISOString()} title={absoluteTime(call.startedAt)}>{clockTime(call.startedAt)}</time>
        </span><span className="window-call-duration"><small>{t("Duration")}</small><strong>{durationText(call.duration)}</strong></span></>}
    </div>
    {client && call.traceRef && <TraceCallDetails client={client} traceRef={call.traceRef} language={language}
      expanded={expanded} onExpandedChange={open => onExpandedChange(call.key, open)} />}
  </article>;
});
