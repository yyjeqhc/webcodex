import { useState } from "react";
import type { RuntimeV2Client } from "../api/client.js";
import { TraceCallDetails } from "./TraceCallDetails.js";
import { CopyIdentity } from "./ui/CopyIdentity.js";
import { displayProjectPath } from "../../ui/projectPresentation.js";
import type { RuntimeLanguage } from "../../runtime_i18n.js";
import { translate } from "../../runtime_i18n.js";
import { absoluteTime, clockTime, durationText, shortId } from "../model/format.js";
import type { ProjectRow, WindowDetail } from "../model/types.js";
import {
  focusedWindowCallKeys,
  windowSessionCatalog,
} from "../model/windowSessions.js";

type Props = {
  client?: RuntimeV2Client;
  detail: WindowDetail;
  projects: ProjectRow[];
  language: RuntimeLanguage;
  selectedSessionId?: string;
  onSelectSession?: (sessionId: string) => void;
  onOpenSessionRecord?: (project: string, sessionId: string) => void;
};

export function WindowActivityFeed({
  client,
  detail,
  projects,
  language,
  selectedSessionId = "",
  onSelectSession,
  onOpenSessionRecord,
}: Props) {
  const t = (value: string) => translate(value, language);
  const [order, setOrder] = useState<"newest" | "oldest">("newest");
  const orderedSessions = windowSessionCatalog(detail);
  const sessionOrder = new Map(orderedSessions.map((session, index) => [session.workflow_session_id, index]));
  const sessionMeta = new Map(orderedSessions.map((session) => [session.workflow_session_id, session]));
  const jobsById = new Map((detail.jobs || []).map((job) => [job.job_id, job]));
  const filterSessions = orderedSessions;
  // Keep each invocation, including repeated observation calls. The trace only
  // reconciles a completed call with the same call in the live snapshot.
  const completedTraces = new Set(detail.activity.map((row) => row.server_trace_id).filter(Boolean));
  const calls = [
    ...detail.activity.map((row, index) => ({
      key: row.server_trace_id || `completed-${row.started_at_ms}-${index}`,
      traceRef: row.server_trace_id,
      tool: row.tool_name || row.method,
      project: row.project,
      startedAt: row.started_at_ms,
      duration: row.duration_ms,
      status: row.status,
      expectation: row.failure_expectation_result,
      sessions: row.workflow_sessions
        .map((link) => link.workflow_session_id)
        .filter((sessionId, index, values) => values.indexOf(sessionId) === index)
        .sort((left, right) => (sessionOrder.get(left) ?? Number.MAX_SAFE_INTEGER) - (sessionOrder.get(right) ?? Number.MAX_SAFE_INTEGER)),
      asyncJobId: row.async_job_id,
      observedJobIds: row.observed_job_ids || [],
      running: false,
    })),
    ...detail.active_requests.filter((row) => !completedTraces.has(row.server_trace_id)).map((row) => ({
      key: row.server_trace_id,
      traceRef: row.server_trace_id,
      tool: row.tool_name || row.method,
      project: row.project,
      startedAt: row.started_at_ms,
      duration: row.elapsed_ms,
      status: "running",
      expectation: undefined,
      sessions: [] as string[],
      running: true,
      asyncJobId: undefined as string | undefined,
      observedJobIds: [] as string[],
    })),
  ].sort((a, b) => a.startedAt - b.startedAt);
  const activeSessionId = selectedSessionId;
  const focusedKeys = focusedWindowCallKeys(calls, activeSessionId);
  // Session segments must be resolved chronologically; ordering is presentation
  // only and must never change the membership of untagged continuation calls.
  const filteredCalls = activeSessionId ? calls.filter((call) => focusedKeys.has(call.key)) : calls;
  const visibleCalls = order === "newest" ? filteredCalls.slice().reverse() : filteredCalls;
  const selectedProject = sessionMeta.get(activeSessionId)?.project;
  const selectedTone = activeSessionId ? (sessionOrder.get(activeSessionId) ?? 0) % 8 : 0;

  return (
    <section className="window-detail-section window-workflow-section" aria-label={t("Window activity")}>
      <div className="window-activity-toolbar" role="group" aria-label={t("Tool calls")}>
        {(filterSessions.length > 0 || activeSessionId) && (
          <div className="window-session-focus">
            <span>{t("Session")}</span>
            <div className="window-session-focus-select" data-session-tone={selectedTone} data-active={Boolean(activeSessionId)}>
              <span className="window-session-color-dot" />
              <select
                aria-label={t("Session filter")}
                value={activeSessionId}
                onChange={(event) => onSelectSession?.(event.currentTarget.value)}
              >
                <option value="">{t("All calls")}</option>
                {activeSessionId && !filterSessions.some(session => session.workflow_session_id === activeSessionId) && (
                  <option value={activeSessionId}>{t("Session")} · {shortId(activeSessionId, 14, 6)}</option>
                )}
                {filterSessions.map((session, index) => (
                  <option key={session.workflow_session_id} value={session.workflow_session_id}>
                    {(session.title || t("Work Session") + " " + (index + 1)) + " · " + shortId(session.workflow_session_id, 14, 6)}
                  </option>
                ))}
              </select>
            </div>
            <small>{visibleCalls.length}/{calls.length}</small>
          </div>
        )}
        <fieldset className="window-activity-order">
          <legend>{t("Activity order")}</legend>
          {(["newest", "oldest"] as const).map(value => (
            <label key={value}>
              <input type="radio" name="window-activity-order" value={value} checked={order === value}
                onChange={() => setOrder(value)} />
              <span>{t(value === "newest" ? "Newest first" : "Oldest first")}</span>
            </label>
          ))}
        </fieldset>
        {selectedProject && onOpenSessionRecord && <button type="button" className="text-button" onClick={() => onOpenSessionRecord(selectedProject, activeSessionId)}>{t("View Session record")}</button>}
        {detail.sessions_truncated && <p className="inventory-note">{t("Some linked Sessions are not available in this view.")}</p>}
        {activeSessionId && <CopyIdentity key={activeSessionId} value={activeSessionId} label={t("Session")} language={language} />}
      </div>
      <div className="window-activity-results">
        {activeSessionId && !visibleCalls.length && <p className="inventory-note">{t("This Session is linked to the Window but has no retained calls.")}</p>}
        {detail.active_count > detail.active_requests.length && <div className="inventory-note">{t("Some running calls are not shown.")} {detail.active_requests.length}/{detail.active_count}</div>}
        {detail.activity_truncated && <div className="inventory-note">{t("Earlier calls are not available in this view. Only retained activity is shown.")}</div>}
        <div className="window-workflow-list">
          {visibleCalls.map((call) => {
            const project = projects.find((row) => row.id === call.project);
            const path = project?.path ? displayProjectPath(project.path) : undefined;
            const success = ["ok", "success", "succeeded"].includes(call.status);
            const failed = ["error", "failed", "failure"].includes(call.status);
            const expected = ["matched_expected_failure", "matched_expected_result"].includes(call.expectation || "");
            const mismatch = ["expectation_mismatch", "unexpected_success", "unexpected_failure"].includes(call.expectation || "");
            const expectationLabel = expected ? "Expected result" : mismatch ? "Expectation not met" : "";
            const status = call.running ? "Running" : success ? "Succeeded" : failed ? "Failed" : call.status;
            const primarySession = call.sessions.find((sessionId) => sessionMeta.has(sessionId));
            const selectable = Boolean(primarySession && onSelectSession);
            const asyncJob = call.asyncJobId ? jobsById.get(call.asyncJobId) : undefined;
            const asyncJobLabel = asyncJob
              ? asyncJob.active
                ? t("Background running")
                : asyncJob.status === "completed"
                  ? t("Completed")
                  : t(asyncJob.status)
              : t("Background job");
            const asyncJobDuration = asyncJob?.active && asyncJob.elapsed_secs !== undefined
              ? durationText(asyncJob.elapsed_secs * 1000)
              : asyncJob?.duration_ms !== undefined
                ? durationText(asyncJob.duration_ms)
                : "";
            return (
              <article
                className={"window-call-card" + (call.running ? " running" : "") + (selectable ? " session-linked" : "")}
                data-testid="window-workflow-step"
                key={call.key}
                onClick={() => primarySession && onSelectSession?.(primarySession)}
              >
                <header>
                  <strong>{call.tool}</strong>
                  <span className={"status-pill " + (call.running ? "" : expected || (success && !mismatch) ? "good" : "warn")}>{t(status)}{!call.running && expectationLabel ? " · " + t(expectationLabel) : ""}</span>
                </header>
                {(path || call.sessions.length > 0 || call.asyncJobId || call.observedJobIds.length > 0) && (
                  <div className="window-call-context">
                    {path && <span className="window-project-tag" data-testid="window-project-tag" title={path}>{path}</span>}
                    {call.sessions.map((sessionId) => {
                      const linked = sessionMeta.get(sessionId);
                      const tone = (sessionOrder.get(sessionId) ?? 0) % 8;
                      const tagSelectable = Boolean(linked && onSelectSession);
                      const selected = activeSessionId === sessionId;
                      return (
                        <button
                          className={"window-session-tag" + (selected ? " selected" : "")}
                          data-session-tone={tone}
                          data-testid="window-session-tag"
                          key={sessionId}
                          type="button"
                          disabled={!tagSelectable}
                          title={(linked?.title ? linked.title + " · " : "") + sessionId}
                          onClick={(event) => {
                            event.stopPropagation();
                            if (tagSelectable) onSelectSession?.(selected ? "" : sessionId);
                          }}
                        >
                          <span className="window-session-color-dot" />
                          {shortId(sessionId, 14, 6)}
                        </button>
                      );
                    })}
                    {call.asyncJobId && (
                      <span className={"window-job-tag" + (asyncJob?.active ? " active" : "")} title={call.asyncJobId}>
                        <span className="window-job-dot" />
                        {asyncJobLabel} · {shortId(call.asyncJobId, 14, 6)}
                        {asyncJobDuration && <small> · {asyncJobDuration}</small>}
                      </span>
                    )}
                    {call.observedJobIds.map((jobId) => (
                      <span className="window-job-tag observe" title={jobId} key={jobId}>
                        <span className="window-job-dot" />
                        {t("Observing")} · {shortId(jobId, 14, 6)}
                      </span>
                    ))}
                  </div>
                )}
                <div className="window-call-timing">
                  {call.running ? (
                    <span className="window-call-live-time">{t("Running")} · <strong>{durationText(call.duration)}</strong></span>
                  ) : (
                    <>
                      <span className="window-call-clock">
                        <time dateTime={new Date(call.startedAt).toISOString()} title={absoluteTime(call.startedAt)}>{clockTime(call.startedAt)}</time>
                      </span>
                      <span className="window-call-duration">
                        <small>{t("Duration")}</small>
                        <strong>{durationText(call.duration)}</strong>
                      </span>
                    </>
                  )}
                </div>
                {client && call.traceRef && <TraceCallDetails client={client} traceRef={call.traceRef} language={language} />}
              </article>
            );
          })}
          {!visibleCalls.length && <div className="empty-inline">{t(activeSessionId ? "No calls in this Session" : "No tool calls yet")}</div>}
        </div>
      </div>
    </section>
  );
}
