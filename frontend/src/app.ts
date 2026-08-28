// WebCodex host-local review console.
//
// The page lets the same-host human review, accept, reject, and cancel
// connector task results. It talks only to the host-local `/api/console/*`
// surface (never a model-facing capability), renders every project value as
// text (never innerHTML), and keeps the project credential in memory only — it
// is never persisted to browser storage, a URL, a DOM attribute, or the log.
//
// All review-identity and concurrency correctness lives in the pure
// `review_state` module: which task is selected, which immutable snapshot an
// action binds to, and which refreshes are in flight. This file owns only the
// DOM and the network.

import {
  initialState,
  selectTask,
  adoptReview,
  actionsEnabled,
  openConfirm,
  closeConfirm,
  actionRequest,
  beginRefresh,
  endRefresh,
  reset,
  createReviewController,
} from "./review_state";
import {
  initialWorkflowSessionState,
  selectWorkflowSession,
  refreshWorkflowSessionDetail,
  clearWorkflowSessionSelection,
  isCurrentWorkflowSessionDetailRequest,
  adoptWorkflowSessionDetail,
  updateWorkflowSessionFollowFromScroll,
  workflowSessionScrollTopAfterRender,
  jumpWorkflowSessionToLatest,
  shouldFollowWorkflowSessionLatest,
  workflowSessionListOverviewFacts,
  workflowSessionOverviewPresentation,
} from "./workflow_session_state";

const CONSOLE_BASE = "/api/console/";
const REFRESH_MS = 8000;
const WORK_QUEUE_HINT = "No tasks need attention.";

// Credential is held only in this in-memory variable for the lifetime of the
// page. A refresh intentionally requires re-entering it.
let token = "";
let autoEnabled = true;
let connectLoaded = false;
let showCompleted = false;
let timer = 0;
let reviewLoop: any = null;
let projectName = "";
let workflowSessionDetailAbort: AbortController | null = null;

// Connector Task review and Workflow Session observability remain separate
// state machines. Neither identity is inferred from the other.
const state = initialState();
const workflowSessionState = initialWorkflowSessionState();

function el(id: string) {
  return document.getElementById(id);
}

function setText(id: string, input: unknown) {
  const node = el(id);
  if (node) {
    node.textContent =
      input === null || input === undefined || input === "" ? "—" : String(input);
  }
}

function show(id: string, visible: boolean) {
  const node = el(id);
  if (node) {
    node.hidden = !visible;
  }
}

function clearNode(node: any) {
  while (node && node.firstChild) {
    node.removeChild(node.firstChild);
  }
}

function inputValue(id: string) {
  const node = el(id);
  return node ? (node as HTMLInputElement).value : "";
}

function inputChecked(id: string) {
  const node = el(id);
  return node ? (node as HTMLInputElement).checked : false;
}

function showGate(message: string) {
  show("token-gate", true);
  show("console", false);
  show("topbar-controls", false);
  stopAuto();
  setText("token-error", message);
  const input = el("token-input");
  if (input) {
    (input as HTMLInputElement).value = "";
    (input as HTMLInputElement).focus();
  }
}

function showConsole() {
  show("token-gate", false);
  show("console", true);
  show("topbar-controls", true);
}

function showError(message: string) {
  const banner = el("error-banner");
  if (banner) {
    banner.textContent = message;
    banner.hidden = false;
  }
}

function hideError() {
  const banner = el("error-banner");
  if (banner) {
    banner.textContent = "";
    banner.hidden = true;
  }
}

function lock(message: string) {
  token = "";
  reset(state);
  clearWorkflowSessionDetailSelection();
  if (reviewLoop) {
    reviewLoop.stop();
  }
  closeConfirmUi();
  showGate(message);
}

// Single host-local request helper. Always POSTs JSON with a Bearer header and
// never echoes the token anywhere.
async function api(path: string, body: any, signal: any = null): Promise<any> {
  if (!token) {
    lock("Credential required.");
    return null;
  }
  const headers = new Headers();
  headers.set("Authorization", "Bearer " + token);
  headers.set("Content-Type", "application/json");
  let response;
  try {
    response = await fetch(CONSOLE_BASE + path, {
      method: "POST",
      headers: headers,
      body: JSON.stringify(body || {}),
      signal: signal,
    });
  } catch {
    if (signal && signal.aborted) {
      return null;
    }
    showError("WebCodex is not reachable. Run webcodex runner start.");
    return null;
  }
  let data: any = null;
  try {
    data = await response.json();
  } catch {
    data = null;
  }
  return { status: response.status, ok: response.ok, data: data };
}

reviewLoop = createReviewController({
  fetchReview: (body: any, signal: any) => api("task/review", body, signal),
  abort: () => new AbortController(),
  schedule: (next: any, delay: number) => window.setTimeout(next, delay),
  cancelSchedule: (handle: number) => window.clearTimeout(handle),
  unauthorized: () => lock("Credential rejected. Re-enter it."),
  error: (data: any) => showError(errorMessage(data)),
  render: (review: any) => {
    const previousTask = state.snapshot ? state.snapshot.taskId : null;
    const previousResult = state.snapshot ? state.snapshot.resultId : null;
    if (!adoptReview(state, String(review.task_id), state.reviewSeq, review)) {
      return;
    }
    if (previousResult !== state.snapshot.resultId) {
      closeConfirmUi();
      hideActionButtons();
    }
    hideError();
    renderDetail(review, previousTask === String(review.task_id));
    renderSelection();
  },
});

function errorMessage(data: any) {
  if (data && data.error && data.error.message) {
    return String(data.error.message);
  }
  return "Request failed.";
}

async function fetchReadiness() {
  if (!beginRefresh(state, "readiness")) {
    return;
  }
  try {
    const res = await api("readiness", {});
    if (!res) {
      return;
    }
    if (res.status === 401) {
      lock("Credential rejected. Re-enter it.");
      return;
    }
    if (res.data) {
      renderReadiness(res.data);
      hideError();
    } else if (!res.ok) {
      showError("Readiness check failed.");
    }
    setText("last-updated", "Updated " + new Date().toLocaleTimeString());
  } finally {
    endRefresh(state, "readiness");
  }
}

function renderReadiness(readiness: any) {
  projectName = readiness.project || "";
  setText("project", readiness.project || "Not configured");
  setText("connection", readiness.connection);
  setText("agent", readiness.agent);
  setText("capabilities", readiness.capabilities);
  setText("coding", readiness.ready ? "Ready" : "Needs action");
  setText("next-action", readiness.next_action || "No action needed");
}

async function fetchWorkflowSessions() {
  const res = await api("workflow-sessions", { limit: 20 });
  if (!res) {
    return;
  }
  if (res.status === 401) {
    lock("Credential rejected. Re-enter it.");
    return;
  }
  if (!res.ok || !res.data) {
    return;
  }
  const sessions = Array.isArray(res.data.sessions) ? res.data.sessions : [];
  renderWorkflowSessionList(sessions, res.data);
  const request = refreshWorkflowSessionDetail(workflowSessionState);
  if (request) {
    await fetchWorkflowSessionDetail(request);
  }
}

function renderWorkflowSessionList(sessions: any[], payload: any) {
  const node = el("workflow-session-list");
  if (!node) {
    return;
  }
  clearNode(node);
  show("workflow-sessions-empty", sessions.length === 0);
  const total = typeof payload.total === "number" ? payload.total : sessions.length;
  setText(
    "workflow-sessions-count",
    total ? "(" + sessions.length + (payload.truncated ? " of " + total : "") + ")" : ""
  );
  const selectedWorkflowSessionId = String(workflowSessionState.selectedSessionId || "");
  if (
    selectedWorkflowSessionId &&
    !sessions.some((session) => String(session.session_id || "") === selectedWorkflowSessionId)
  ) {
    clearWorkflowSessionDetailSelection();
  }
  for (const session of sessions) {
    const id = String(session.session_id || "");
    if (!id) {
      continue;
    }
    const item = document.createElement("li");
    item.className = "task" + (id === selectedWorkflowSessionId ? " task-selected" : "");
    const title = document.createElement("div");
    title.className = "task-goal";
    title.textContent = session.title ? String(session.title) : id;
    const meta = document.createElement("div");
    meta.className = "task-meta muted small";
    appendChip(meta, String(session.lifecycle || "unknown"));
    appendChip(meta, "mode " + String(session.mode || "unknown"));
    if (session.running_call) {
      appendChip(meta, "running");
    }
    appendChip(meta, updatedLabel(session.updated_at));
    item.appendChild(title);
    item.appendChild(meta);
    const summaryFacts = workflowSessionListOverviewFacts(session.overview);
    if (summaryFacts.length) {
      const summary = document.createElement("div");
      summary.className = "workflow-session-summary";
      for (const fact of summaryFacts) {
        const chip = document.createElement("span");
        chip.className = "chip workflow-session-summary-fact workflow-session-summary-" + fact.tone;
        chip.textContent = fact.text;
        summary.appendChild(chip);
      }
      item.appendChild(summary);
    }
    appendWorkflowSessionActivityPreview(item, "Now", session.current_activity);
    appendWorkflowSessionActivityPreview(item, "Last", session.last_activity);
    item.addEventListener("click", () => {
      const request = selectWorkflowSessionDetail(id);
      renderWorkflowSessionList(sessions, payload);
      void fetchWorkflowSessionDetail(request);
    });
    node.appendChild(item);
  }
}

function workflowActivityKindLabel(activity: any): string {
  const kind = String((activity && activity.kind) || "Activity");
  if (activity && activity.job_handoff) {
    if (kind === "Tested") {
      return "Test";
    }
    if (kind === "Ran") {
      return "Command";
    }
  }
  if (kind === "Explored" && activity && typeof activity.group_count === "number") {
    return "Explored ×" + activity.group_count;
  }
  return kind;
}

function workflowActivityFacts(activity: any, includeTiming: boolean): string[] {
  const facts: string[] = [];
  if (activity && typeof activity.group_count === "number") {
    if (Array.isArray(activity.group_kinds) && activity.group_kinds.length) {
      facts.push(activity.group_kinds.map((kind: any) => String(kind)).join(" / "));
    }
    if (Array.isArray(activity.group_tools) && activity.group_tools.length) {
      facts.push(activity.group_tools.map((tool: any) => String(tool)).join(", "));
    }
  } else if (activity && activity.tool) {
    facts.push(String(activity.tool));
  }
  if (activity && activity.kind === "Progress") {
    facts.push("informational");
  } else if (activity && activity.job_handoff) {
    facts.push("handed off");
    if (activity.execution_state) {
      facts.push("execution " + String(activity.execution_state));
    }
  } else if (activity && activity.state) {
    facts.push(String(activity.state));
  }
  if (includeTiming && activity && typeof activity.duration_ms === "number") {
    facts.push(durationLabel(activity.duration_ms));
  }
  if (activity && typeof activity.exit_code === "number") {
    facts.push("exit " + activity.exit_code);
  }
  if (activity && activity.job_id) {
    facts.push("job " + String(activity.job_id));
  }
  if (includeTiming && activity && typeof activity.started_at === "number") {
    facts.push(new Date(activity.started_at * 1000).toLocaleTimeString());
  }
  return facts;
}

function workflowActivityDescription(activity: any): string {
  if (!activity) {
    return "";
  }
  const parts = [workflowActivityKindLabel(activity), ...workflowActivityFacts(activity, false)];
  if (activity.summary && !activity.job_handoff) {
    parts.push(String(activity.summary));
  }
  return parts.join(" · ");
}

function appendWorkflowSessionActivityPreview(parent: HTMLElement, label: string, activity: any) {
  if (!activity) {
    return;
  }
  const row = document.createElement("div");
  row.className = "workflow-session-activity-preview muted small";
  const prefix = document.createElement("span");
  prefix.className = "workflow-session-activity-label";
  prefix.textContent = label;
  const text = document.createElement("span");
  text.textContent = workflowActivityDescription(activity);
  row.appendChild(prefix);
  row.appendChild(text);
  parent.appendChild(row);
}

function setWorkflowSessionOverviewTone(id: string, tone: string) {
  const node = el(id);
  if (!node) {
    return;
  }
  for (const name of ["pass", "warn", "fail", "muted"]) {
    node.classList.toggle("workflow-session-overview-" + name, tone === name);
  }
}

function renderWorkflowSessionOverview(overview: any) {
  const view = workflowSessionOverviewPresentation(overview);
  setText("workflow-session-overview-work", view.workText);
  setText(
    "workflow-session-overview-validation",
    view.validationText +
      (typeof view.validationAt === "number"
        ? " · " + new Date(view.validationAt * 1000).toLocaleTimeString()
        : "")
  );
  setWorkflowSessionOverviewTone("workflow-session-overview-validation-card", view.validationTone);
  setText("workflow-session-overview-attention", view.attentionText);
  setWorkflowSessionOverviewTone("workflow-session-overview-attention-card", view.attentionTone);
  setText(
    "workflow-session-overview-progress",
    view.progressText +
      (typeof view.progressAt === "number"
        ? " · reported " + new Date(view.progressAt * 1000).toLocaleTimeString()
        : "")
  );
}

function syncWorkflowSessionFollowUi() {
  const selected = !!workflowSessionState.selectedSessionId;
  show(
    "workflow-session-jump-latest",
    selected && !shouldFollowWorkflowSessionLatest(workflowSessionState)
  );
}

function scrollWorkflowSessionTimelineToLatest() {
  const node = el("workflow-session-timeline");
  if (node) {
    node.scrollTop = node.scrollHeight;
  }
  syncWorkflowSessionFollowUi();
}

function hideWorkflowSessionDetail() {
  show("workflow-session-detail", false);
  show("workflow-session-detail-empty", true);
  show("workflow-session-jump-latest", false);
}

function abortWorkflowSessionDetailRequest() {
  if (workflowSessionDetailAbort) {
    workflowSessionDetailAbort.abort();
    workflowSessionDetailAbort = null;
  }
}

function clearWorkflowSessionDetailSelection() {
  abortWorkflowSessionDetailRequest();
  clearWorkflowSessionSelection(workflowSessionState);
  hideWorkflowSessionDetail();
}

function selectWorkflowSessionDetail(sessionId: string) {
  abortWorkflowSessionDetailRequest();
  const request = selectWorkflowSession(workflowSessionState, sessionId);
  // Never present the previous Session detail under the newly selected row.
  hideWorkflowSessionDetail();
  return request;
}

async function fetchWorkflowSessionDetail(request: any) {
  if (!request) {
    return;
  }
  abortWorkflowSessionDetailRequest();
  const controller = new AbortController();
  workflowSessionDetailAbort = controller;
  const res = await api(
    "workflow-session",
    { session_id: request.sessionId, limit: 100 },
    controller.signal
  );
  if (workflowSessionDetailAbort === controller) {
    workflowSessionDetailAbort = null;
  }
  if (!res || !isCurrentWorkflowSessionDetailRequest(workflowSessionState, request)) {
    return;
  }
  if (res.status === 401) {
    lock("Credential rejected. Re-enter it.");
    return;
  }
  if (res.status === 404) {
    clearWorkflowSessionDetailSelection();
    return;
  }
  if (!res.ok || !res.data) {
    return;
  }
  if (!adoptWorkflowSessionDetail(workflowSessionState, request, res.data)) {
    return;
  }
  renderWorkflowSessionDetail(res.data);
}

function renderWorkflowSessionDetail(detail: any) {
  show("workflow-session-detail-empty", false);
  show("workflow-session-detail", true);
  setText("workflow-session-title", detail.title);
  setText("workflow-session-lifecycle", detail.lifecycle);
  setText("workflow-session-mode", "mode " + String(detail.mode || "unknown"));
  setText("workflow-session-running", detail.running_call ? "running call" : "no running call");
  setText("workflow-session-updated", updatedLabel(detail.updated_at));
  renderWorkflowSessionOverview(detail.overview);
  const activities = Array.isArray(detail.activity) ? detail.activity : [];
  const node = el("workflow-session-timeline");
  const previousScrollTop = node ? node.scrollTop : 0;
  clearNode(node);
  show("workflow-session-timeline-empty", activities.length === 0);
  if (!node) {
    syncWorkflowSessionFollowUi();
    return;
  }
  for (const activity of activities) {
    const item = document.createElement("li");
    item.className = "timeline-event";
    if (activity && activity.kind === "Progress") {
      item.classList.add("workflow-session-progress");
    } else if (activity && (activity.state === "failed" || activity.state === "timed_out")) {
      item.classList.add("workflow-session-failed");
    } else if (
      activity &&
      ["outcome_unknown", "cancelled", "not_started"].includes(String(activity.state || ""))
    ) {
      item.classList.add("workflow-session-uncertain");
    } else if (activity && activity.job_handoff) {
      item.classList.add("workflow-session-job");
    } else if (activity && ["queued", "running"].includes(String(activity.state || ""))) {
      item.classList.add("workflow-session-running");
    } else if (activity && activity.kind === "Explored") {
      item.classList.add("workflow-session-exploration");
    }
    const head = document.createElement("div");
    head.className = "timeline-head";
    const kind = document.createElement("span");
    kind.className = "timeline-kind";
    kind.textContent = workflowActivityKindLabel(activity);
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = workflowActivityFacts(activity, true).join(" · ");
    head.appendChild(kind);
    head.appendChild(meta);
    item.appendChild(head);
    const bodyParts = [];
    if (activity && activity.summary) {
      bodyParts.push(String(activity.summary));
    }
    if (activity && Array.isArray(activity.paths) && activity.paths.length) {
      bodyParts.push(activity.paths.map((path: any) => String(path)).join(", "));
    }
    if (bodyParts.length) {
      const body = document.createElement("div");
      body.className = "timeline-payload muted small";
      body.textContent = bodyParts.join(" — ");
      item.appendChild(body);
    }
    node.appendChild(item);
  }
  node.scrollTop = workflowSessionScrollTopAfterRender(
    workflowSessionState,
    previousScrollTop,
    node.clientHeight,
    node.scrollHeight
  );
  syncWorkflowSessionFollowUi();
}

function durationLabel(durationMs: number): string {
  if (durationMs < 1000) {
    return durationMs + " ms";
  }
  return (durationMs / 1000).toFixed(durationMs < 10_000 ? 1 : 0) + " s";
}

async function fetchTasks() {
  if (!beginRefresh(state, "tasks")) {
    return;
  }
  try {
    const res = await api("tasks", { include_completed: showCompleted });
    if (res && res.status === 401) {
      lock("Credential rejected. Re-enter it.");
      return;
    }
    if (!res || !res.ok || !res.data) {
      return;
    }
    const tasks = Array.isArray(res.data.tasks) ? res.data.tasks : [];
    renderTaskList(tasks);
  } finally {
    endRefresh(state, "tasks");
  }
}

function renderTaskList(tasks: any) {
  const list = el("task-list");
  if (!list) {
    return;
  }
  clearNode(list);
  show("queue-empty", tasks.length === 0);
  const empty = el("queue-empty");
  if (empty) {
    empty.textContent = WORK_QUEUE_HINT;
  }
  for (const task of tasks) {
    const id = String(task.task_id);
    const item = document.createElement("li");
    item.className = "task" + (id === state.selectedTaskId ? " task-selected" : "");
    item.setAttribute("data-task-id", id);
    const goal = document.createElement("div");
    goal.className = "task-goal";
    goal.textContent = task.goal || id;
    const unread = typeof task.unread_guidance === "number" ? task.unread_guidance : 0;
    if (unread > 0) {
      // Flag a task whose guidance the model has not yet read, so a reviewer
      // scanning the queue sees which conversations still have an unread
      // course correction outstanding.
      const badge = document.createElement("span");
      badge.className = "guidance-badge";
      badge.textContent = unread + " unread guidance";
      badge.title = "Guidance the model has not yet seen on a capability response";
      goal.appendChild(badge);
      item.classList.add("task-guidance-pending");
    }
    const meta = document.createElement("div");
    meta.className = "task-meta muted small";
    const status = document.createElement("span");
    status.className = "chip chip-" + String(task.task_status);
    status.textContent = String(task.task_status);
    meta.appendChild(status);
    appendChip(meta, task.next_action ? String(task.next_action) : "not available");
    appendChip(meta, "exec " + (task.execution_status || "not available"));
    appendChip(meta, "checks " + (task.validation_status || "not available"));
    appendChip(meta, updatedLabel(task.updated_at));
    item.appendChild(goal);
    item.appendChild(meta);
    item.addEventListener("click", () => {
      selectTaskUi(id);
    });
    list.appendChild(item);
  }
}

function appendChip(parent: any, text: string) {
  if (!text) {
    return;
  }
  const span = document.createElement("span");
  span.textContent = text;
  parent.appendChild(span);
}

// Server-supplied time, rendered as a fact — never inferred from list position.
function updatedLabel(updatedAt: any): string {
  if (typeof updatedAt !== "number" || updatedAt <= 0) {
    return "updated not available";
  }
  return "updated " + new Date(updatedAt * 1000).toLocaleTimeString();
}

// Select a task: invalidate the previous snapshot, hide stale action buttons,
// show a loading detail, then load the new review under a fresh sequence.
function selectTaskUi(taskId: string) {
  selectTask(state, taskId);
  renderSelection();
  showDetailLoading();
  reviewLoop.select(taskId);
  // Focus the guidance box so a reviewer can type a course correction
  // immediately after selecting a task, without an extra click. The detail
  // panel is now visible, so the input is in the document.
  const guide = el("guide-input");
  if (guide) {
    (guide as HTMLInputElement).focus();
  }
}

function renderSelection() {
  const list = el("task-list");
  if (!list) {
    return;
  }
  for (const child of Array.from(list.children)) {
    const item = child as HTMLElement;
    const selected = item.getAttribute("data-task-id") === state.selectedTaskId;
    item.classList.toggle("task-selected", selected);
  }
}

function showDetailLoading() {
  show("detail-empty", false);
  show("detail", true);
  setText("detail-goal", "Loading…");
  setText("detail-task-status", "—");
  setText("detail-run-status", "");
  show("detail-exec-status", false);
  show("detail-validation", false);
  hideActionButtons();
  setText("detail-next", "");
}

function hideActionButtons() {
  show("accept-btn", false);
  show("reject-btn", false);
  show("cancel-btn", false);
}

function renderDetail(d: any, preserveScroll: boolean) {
  show("detail-empty", false);
  show("detail", true);
  // The review long-poll re-renders the same task every few seconds. Preserve
  // independently scrollable panes only for that same-task refresh; selecting
  // a different task must start its detail at the top instead of inheriting
  // the previous task's diff/output/timeline position.
  const scrollIds = ["detail", "detail-diff", "detail-output", "detail-timeline"];
  const scrollTop: Record<string, number> = {};
  if (preserveScroll) {
    for (const id of scrollIds) {
      const node = el(id);
      if (node) {
        scrollTop[id] = (node as HTMLElement).scrollTop;
      }
    }
  }
  setText("detail-goal", d.goal);
  setText("detail-task-status", d.status);
  setText("detail-run-status", "run: " + (d.run_status || "not available"));

  const execution = d.recent_execution || null;
  setText(
    "detail-exec-status",
    "exec: " + (execution && execution.execution_status ? execution.execution_status : "not available")
  );
  show("detail-exec-status", true);

  const validation = d.result && d.result.validation ? d.result.validation : null;
  const validationStatus = validation && validation.status ? validation.status : null;
  const assertion = execution && execution.assertion_status ? execution.assertion_status : null;
  setText("detail-validation", "checks: " + (validationStatus || assertion || "not available"));
  show("detail-validation", true);

  const parts = [];
  parts.push("mode " + d.mode);
  parts.push("cursor " + d.event_cursor);
  setText("detail-meta", parts.join(" · "));
  setText("detail-created", timeLabel(d.created_at));
  setText("detail-updated", timeLabel(d.updated_at));
  const recipe = (validation && validation.recipe) || (execution && execution.recipe);
  setText(
    "detail-recipe",
    recipe
      ? [recipe.id, recipe.version, recipe.root].filter((value) => !!value).join(" · ")
      : "not available"
  );
  const evidence =
    (validation && validation.assertion_evidence) ||
    (execution && execution.assertion_evidence);
  setText("detail-evidence", evidence ? JSON.stringify(evidence) : "not available");

  renderChecks(validation, execution);
  renderFiles(d);
  renderDiff(d);
  renderOutput(execution);
  renderTimeline(d);
  renderActions(d);
  setText("detail-next", "Next: " + (d.next_action || "not available"));
  for (const id of scrollIds) {
    const node = el(id);
    if (node) {
      (node as HTMLElement).scrollTop =
        preserveScroll && scrollTop[id] !== undefined ? scrollTop[id] : 0;
    }
  }
}

function timeLabel(value: any): string {
  return typeof value === "number" && value > 0
    ? new Date(value * 1000).toLocaleString()
    : "not available";
}

function checkList(validation: any, execution: any) {
  if (validation && Array.isArray(validation.checks)) {
    return validation.checks;
  }
  if (execution && Array.isArray(execution.checks)) {
    return execution.checks;
  }
  return [];
}

function renderChecks(validation: any, execution: any) {
  const checks = checkList(validation, execution);
  const node = el("detail-checks");
  clearNode(node);
  show("detail-checks-section", true);
  if (!node) {
    return;
  }
  if (!checks.length) {
    const item = document.createElement("li");
    item.textContent = "not available";
    node.appendChild(item);
  }
  for (const check of checks) {
    const item = document.createElement("li");
    const status = check && check.status ? String(check.status) : "unknown";
    item.className = "check check-" + status;
    const name = document.createElement("span");
    name.textContent = check && check.name ? String(check.name) : "check";
    const state = document.createElement("span");
    state.className = "muted small";
    state.textContent = status;
    item.appendChild(name);
    item.appendChild(state);
    node.appendChild(item);
  }
}

function renderFiles(d: any) {
  const changes = d.changes || {};
  const result = d.result || {};
  const source = Array.isArray(changes.changed_paths)
    ? changes.changed_paths
    : Array.isArray(result.changed_paths)
    ? result.changed_paths
    : null;
  const files = source || [];
  const node = el("detail-files");
  clearNode(node);
  show("detail-files-section", true);
  setText("detail-files-count", source ? "(" + files.length + ")" : "");
  if (!node) {
    return;
  }
  if (!files.length) {
    const item = document.createElement("li");
    item.textContent = source ? "none" : "not available";
    node.appendChild(item);
  }
  for (const path of files) {
    const item = document.createElement("li");
    item.textContent = String(path);
    node.appendChild(item);
  }
}

function renderDiff(d: any) {
  const diff = d.changes && d.changes.diff_preview ? d.changes.diff_preview : null;
  const pre = el("detail-diff");
  const hasText = diff && typeof diff.text === "string" && diff.text.length > 0;
  show("detail-diff-section", true);
  if (pre) {
    // textContent, never innerHTML: project output is never trusted as markup.
    pre.textContent = hasText ? diff.text : "not available";
  }
  show("detail-diff-trunc", !!(diff && diff.truncated));
}

function renderOutput(execution: any) {
  const tail = execution && execution.output_tail ? execution.output_tail : null;
  const pre = el("detail-output");
  if (!tail) {
    show("detail-output-section", true);
    if (pre) {
      pre.textContent = "not available";
    }
    return;
  }
  const stdout = tail.stdout ? String(tail.stdout) : "";
  const stderr = tail.stderr ? String(tail.stderr) : "";
  const combined = stderr ? stdout + "\n" + stderr : stdout;
  show("detail-output-section", true);
  if (pre) {
    pre.textContent = combined || "not available";
  }
}

// Newest-first durable event log for the selected task. Facts only, rendered
// as text — the payload is never trusted as markup.
function renderTimeline(d: any) {
  const events = Array.isArray(d.recent_events) ? d.recent_events : null;
  const node = el("detail-timeline");
  clearNode(node);
  show("detail-timeline-section", true);
  setText("detail-timeline-count", events ? "(" + events.length + ")" : "");
  if (!node) {
    return;
  }
  if (!events || !events.length) {
    const item = document.createElement("li");
    item.textContent = events ? "none" : "not available";
    node.appendChild(item);
    return;
  }
  // Watermark the model has claimed up to. Guidance with a sequence above this
  // is still pending — unread — and is rendered distinctly so the reviewer can
  // tell whether their guidance has reached the model yet.
  const guidanceSeen =
    typeof d.guidance_seen_seq === "number" ? d.guidance_seen_seq : null;
  for (const event of [...events].reverse()) {
    const item = document.createElement("li");
    item.className = "timeline-event";
    const head = document.createElement("div");
    head.className = "timeline-head";
    const kind = document.createElement("span");
    kind.className = "timeline-kind";
    kind.textContent = event && event.kind ? String(event.kind) : "event";
    const meta = document.createElement("span");
    meta.className = "muted small";
    const seq = event && typeof event.sequence === "number" ? "#" + event.sequence : "";
    meta.textContent = [seq, eventTime(event)].filter((value) => !!value).join(" · ");
    head.appendChild(kind);
    head.appendChild(meta);
    item.appendChild(head);
    const guidance =
      event && event.kind === "human_guidance" && event.payload
        ? String(event.payload.message || "")
        : "";
    if (guidance) {
      item.classList.add("timeline-guidance");
      // Only label read/unread when the host projection supplied a valid
      // watermark. A missing auxiliary read-state must not be misrepresented
      // as proof that every historical guidance event is unread.
      if (guidanceSeen !== null && typeof event.sequence === "number") {
        const unread = event.sequence > guidanceSeen;
        item.classList.add(unread ? "timeline-guidance-unread" : "timeline-guidance-read");
        meta.textContent = [seq, unread ? "unread" : "read", eventTime(event)]
          .filter((value) => !!value)
          .join(" · ");
      }
    }
    const summary = guidance || eventSummary(event ? event.payload : null);
    if (summary) {
      const body = document.createElement("div");
      body.className = "timeline-payload muted small";
      body.textContent = summary;
      item.appendChild(body);
    }
    node.appendChild(item);
  }
}

function eventTime(event: any): string {
  return event && typeof event.created_at === "number" && event.created_at > 0
    ? new Date(event.created_at * 1000).toLocaleTimeString()
    : "";
}

// Compact scalar-first payload summary; long payloads degrade to truncated
// JSON text rather than being hidden.
function eventSummary(payload: any): string {
  if (!payload || typeof payload !== "object") {
    return "";
  }
  const parts = [];
  for (const key of ["ok", "dry_run", "exit_code", "change_count", "status", "reason"]) {
    if (payload[key] !== undefined && payload[key] !== null) {
      parts.push(key + "=" + String(payload[key]));
    }
  }
  if (Array.isArray(payload.changed_paths) && payload.changed_paths.length) {
    const paths = payload.changed_paths.slice(0, 3).map((path: any) => String(path));
    const extra = payload.changed_paths.length - paths.length;
    parts.push("paths: " + paths.join(", ") + (extra > 0 ? " +" + extra : ""));
  }
  if (!parts.length) {
    try {
      const text = JSON.stringify(payload);
      return text.length > 140 ? text.slice(0, 140) + "…" : text;
    } catch {
      return "";
    }
  }
  return parts.join(" · ");
}

// Buttons are offered only when the current snapshot is live (actionsEnabled)
// AND the durable state permits the action.
function renderActions(d: any) {
  const enabled = actionsEnabled(state);
  show("accept-btn", enabled && !!d.can_accept);
  show("reject-btn", enabled && !!d.can_reject);
  show("cancel-btn", enabled && !!d.can_cancel);
}

// Open a confirmation bound to the CURRENT snapshot. If the selection changed
// and no fresh snapshot exists, the action is denied (no modal).
function openConfirmUi(action: string) {
  const pending = openConfirm(state, action);
  if (!pending) {
    return;
  }
  const snapshot = pending.snapshot;
  const review = snapshot.review;
  setText(
    "confirm-title",
    action === "accept" ? "Accept result" : action === "reject" ? "Reject result" : "Cancel task"
  );
  const body = el("confirm-body");
  clearNode(body);
  if (body) {
    addLine(body, "Project", projectName || "—");
    addLine(body, "Task", snapshot.taskId);
    if (snapshot.resultId) {
      addLine(body, "Result", snapshot.resultId);
    }
    const files =
      review.result && Array.isArray(review.result.changed_paths)
        ? review.result.changed_paths.length
        : 0;
    addLine(body, "Changed files", String(files));
    const validation =
      review.result && review.result.validation && review.result.validation.status
        ? String(review.result.validation.status)
        : review.recent_execution && review.recent_execution.assertion_status
        ? String(review.recent_execution.assertion_status)
        : "not_run";
    addLine(body, "Validation", validation);
    addLine(body, "Precondition", review.status + " / " + review.run_status);
    if (action === "accept") {
      addLine(body, "Effect", "The server re-verifies the checkout and result, then applies the patch.");
    } else if (action === "reject") {
      addLine(body, "Effect", "The result is discarded. The patch is not applied.");
    } else {
      addLine(body, "Effect", "The active execution is stopped.");
    }
  }
  // A stable-result rejection can carry guidance for the model's next call.
  // An interrupted task without a result has no such delivery channel, so its
  // reject dialog must not claim that a reason can be sent.
  const reasonAvailable =
    action === "cancel" ||
    (action === "reject" && !!pending.snapshot && !!pending.snapshot.resultId);
  show("confirm-reason-row", reasonAvailable);
  const reason = el("confirm-reason");
  if (reason) {
    (reason as HTMLInputElement).value = "";
    (reason as HTMLInputElement).placeholder =
      action === "reject"
        ? "Optional reason (delivered to the model as guidance)"
        : "Optional reason";
  }
  show("confirm-overlay", true);
}

function addLine(parent: any, label: string, value: string) {
  const row = document.createElement("div");
  row.className = "confirm-line";
  const key = document.createElement("span");
  key.className = "muted small";
  key.textContent = label;
  const val = document.createElement("span");
  val.textContent = value;
  row.appendChild(key);
  row.appendChild(val);
  parent.appendChild(row);
}

function closeConfirmUi() {
  show("confirm-overlay", false);
}

async function performAction() {
  const pending = state.pending;
  const req = actionRequest(pending);
  // Cancel and reject may carry an optional human reason; identity still comes
  // only from the bound snapshot, never the live selection.
  if (
    req &&
    (req.path === "task/cancel" ||
      (req.path === "result/reject" && !!pending.snapshot.resultId))
  ) {
    const reason = inputValue("confirm-reason").trim();
    if (reason) {
      req.body.reason = reason;
    }
  }
  closeConfirm(state);
  closeConfirmUi();
  if (!req) {
    return;
  }
  const taskId = req.body.task_id;
  const res = await api(req.path, req.body);
  if (!res) {
    return;
  }
  if (res.status === 401) {
    lock("Credential rejected. Re-enter it.");
    return;
  }
  if (!res.ok) {
    showError(errorMessage(res.data));
    if (res.data && res.data.error && res.data.error.code === "result_changed") {
      reviewLoop.restart();
    }
    return;
  }
  hideError();
  setText("detail-next", "Done: " + pending.action + ".");
  await fetchTasks();
  if (state.selectedTaskId === taskId) {
    reviewLoop.restart();
  }
}

// Course correction: the message becomes a durable human_guidance event and
// is delivered inside the model's next capability response for this task.
async function sendGuidance() {
  if (!state.selectedTaskId) {
    return;
  }
  const message = inputValue("guide-input").trim();
  if (!message) {
    return;
  }
  const res = await api("task/guide", { task_id: state.selectedTaskId, message: message });
  if (!res) {
    return;
  }
  if (res.status === 401) {
    lock("Credential rejected. Re-enter it.");
    return;
  }
  if (!res.ok) {
    showError(errorMessage(res.data));
    return;
  }
  hideError();
  const input = el("guide-input");
  if (input) {
    (input as HTMLInputElement).value = "";
  }
  setText("detail-next", "Guidance recorded — delivered with the model's next response.");
  reviewLoop.restart();
}

function stopAuto() {
  if (timer) {
    window.clearTimeout(timer);
    timer = 0;
  }
}

// Self-scheduling refresh chain: the next tick is scheduled only after the
// current one settles, so setInterval-style overlap is impossible.
function startAuto() {
  stopAuto();
  if (autoEnabled) {
    scheduleNext();
  }
}

function scheduleNext() {
  timer = window.setTimeout(() => {
    void tick().then(() => {
      if (autoEnabled && token) {
        scheduleNext();
      }
    });
  }, REFRESH_MS);
}

async function tick() {
  // Single-flight: a manual Refresh during an auto tick (or vice versa) is
  // skipped rather than overlapping.
  if (!beginRefresh(state, "tick")) {
    return;
  }
  try {
    await fetchReadiness();
    await fetchTasks();
    await fetchWorkflowSessions();
    await fetchApprovals();
    await fetchActivity();
    await fetchDevices();
    await fetchConnect();
  } finally {
    endRefresh(state, "tick");
  }
}

// The connect targets are static per process: fetch them once per unlock.
async function fetchConnect() {
  if (connectLoaded) {
    return;
  }
  const res = await api("connect", {});
  if (!res || !res.ok || !res.data) {
    return;
  }
  connectLoaded = true;
  renderConnect(res.data);
}

// Copy-paste targets for hosted chat clients. URLs prefer the configured
// public URL and fall back to the address this page is already reached on;
// credentials never appear here.
function renderConnect(data: any) {
  const base =
    typeof data.public_url === "string" && data.public_url
      ? data.public_url
      : window.location.origin;
  setText("connect-mcp-url", base + data.mcp_path);
  setText("connect-schema-url", base + data.actions_schema_path);
  setText("connect-oauth", base + data.oauth_discovery_path);
  show("connect-public-warning", /^https?:\/\/(localhost|127\.)/.test(base));
  show("connect-panel", true);
}

async function fetchDevices() {
  const res = await api("devices", {});
  if (!res || !res.ok || !res.data) {
    return;
  }
  renderDevices(Array.isArray(res.data.agents) ? res.data.agents : []);
}

// Read-only device roster: every agent this credential can see, with its
// connection state, transport, capabilities, and provider health.
function renderDevices(agents: any[]) {
  show("devices-panel", agents.length > 0);
  setText("devices-count", agents.length ? "(" + agents.length + ")" : "");
  const node = el("devices-list");
  if (!node) {
    return;
  }
  clearNode(node);
  for (const agent of agents) {
    const item = document.createElement("li");
    const online = !!(agent && agent.connected);
    item.className = "timeline-event " + (online ? "device-online" : "device-offline");
    const head = document.createElement("div");
    head.className = "timeline-head";
    const name = document.createElement("span");
    name.className = "timeline-kind";
    name.textContent =
      String((agent && (agent.display_name || agent.client_id)) || "agent") +
      (online ? "" : " (offline)");
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = [
      agent && agent.transport ? String(agent.transport) : "",
      agent && agent.hostname ? String(agent.hostname) : "",
      lastSeenLabel(agent ? agent.last_seen_age_secs : null),
    ]
      .filter((value) => !!value)
      .join(" · ");
    head.appendChild(name);
    head.appendChild(meta);
    item.appendChild(head);
    const body = document.createElement("div");
    body.className = "timeline-payload muted small";
    body.textContent = deviceDetail(agent);
    item.appendChild(body);
    const clientId = agent && agent.client_id ? String(agent.client_id) : "";
    if (clientId) {
      item.classList.add("device-clickable");
      item.title = "Show this device's recent activity";
      item.addEventListener("click", () => {
        setActivityClientFilter(clientId);
      });
    }
    node.appendChild(item);
  }
}

function lastSeenLabel(ageSecs: any): string {
  if (typeof ageSecs !== "number" || ageSecs < 0) {
    return "";
  }
  if (ageSecs < 60) {
    return "seen just now";
  }
  if (ageSecs < 3600) {
    return "seen " + Math.floor(ageSecs / 60) + " min ago";
  }
  return "seen " + Math.floor(ageSecs / 3600) + " h ago";
}

function deviceDetail(agent: any): string {
  const parts = [];
  const caps = agent && agent.capabilities && typeof agent.capabilities === "object"
    ? Object.keys(agent.capabilities).filter((key) => !!agent.capabilities[key])
    : [];
  if (caps.length) {
    parts.push("caps: " + caps.join(", "));
  }
  if (agent && typeof agent.projects_count === "number") {
    parts.push("projects " + agent.projects_count);
  }
  if (agent && typeof agent.active_jobs === "number" && agent.active_jobs > 0) {
    parts.push("active jobs " + agent.active_jobs);
  }
  const providers = agent ? agent.tool_providers : null;
  if (!providers) {
    parts.push("providers: native");
  } else {
    try {
      const text = JSON.stringify(providers);
      parts.push("providers: " + (text.length > 120 ? text.slice(0, 120) + "…" : text));
    } catch {}
  }
  return parts.join(" · ");
}

async function fetchApprovals() {
  const res = await api("approvals", {});
  if (!res || !res.ok || !res.data) {
    return;
  }
  renderApprovals(Array.isArray(res.data.approvals) ? res.data.approvals : []);
}

// Pending one-time command approvals for the whole project. Each row shows
// the bounded command preview (informed consent) and decides with an
// optional reason; a denial reason is delivered to the model on its retry.
function renderApprovals(rows: any[]) {
  show("approvals-panel", rows.length > 0);
  setText("approvals-count", rows.length ? "(" + rows.length + ")" : "");
  const node = el("approvals-list");
  if (!node) {
    return;
  }
  clearNode(node);
  for (const row of rows) {
    const item = document.createElement("li");
    item.className = "timeline-event approval-row";
    const head = document.createElement("div");
    head.className = "timeline-head";
    const goal = document.createElement("span");
    goal.className = "timeline-kind";
    goal.textContent = row && row.goal ? String(row.goal) : String(row.task_id || "task");
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = expiresLabel(row ? row.expires_at : null);
    head.appendChild(goal);
    head.appendChild(meta);
    item.appendChild(head);
    const summary = document.createElement("div");
    summary.className = "timeline-payload";
    summary.textContent = row && row.action_summary ? String(row.action_summary) : "";
    item.appendChild(summary);
    const controls = document.createElement("div");
    controls.className = "field-row approval-controls";
    const reason = document.createElement("input");
    reason.type = "text";
    reason.maxLength = 500;
    reason.placeholder = "Optional reason (a denial reason is shown to the model)";
    const deny = document.createElement("button");
    deny.type = "button";
    deny.className = "btn";
    deny.textContent = "Deny";
    const approve = document.createElement("button");
    approve.type = "button";
    approve.className = "btn btn-primary";
    approve.textContent = "Approve";
    deny.addEventListener("click", () => {
      void decideApproval(row, false, reason.value);
    });
    approve.addEventListener("click", () => {
      void decideApproval(row, true, reason.value);
    });
    controls.appendChild(reason);
    controls.appendChild(deny);
    controls.appendChild(approve);
    item.appendChild(controls);
    node.appendChild(item);
  }
}

function expiresLabel(expiresAt: any): string {
  if (typeof expiresAt !== "number" || expiresAt <= 0) {
    return "";
  }
  const remaining = expiresAt - Math.floor(Date.now() / 1000);
  if (remaining <= 0) {
    return "expired";
  }
  const minutes = Math.floor(remaining / 60);
  return minutes >= 1 ? "expires in " + minutes + " min" : "expires in <1 min";
}

async function decideApproval(row: any, approve: boolean, reason: string) {
  if (!row || !row.task_id || !row.approval_id) {
    return;
  }
  const body: any = {
    task_id: String(row.task_id),
    approval_id: String(row.approval_id),
    approve: approve,
  };
  const trimmed = reason.trim();
  if (trimmed) {
    body.reason = trimmed;
  }
  const res = await api("approval/decide", body);
  if (!res) {
    return;
  }
  if (res.status === 401) {
    lock("Credential rejected. Re-enter it.");
    return;
  }
  if (!res.ok) {
    showError(errorMessage(res.data));
    return;
  }
  hideError();
  await fetchApprovals();
  if (state.selectedTaskId === String(row.task_id)) {
    reviewLoop.restart();
  }
}

let activityClientFilter = "";

// Devices rows toggle this filter; the ledger then shows one device's work.
function setActivityClientFilter(client: string) {
  activityClientFilter = activityClientFilter === client ? "" : client;
  const clear = el("activity-filter-clear");
  if (clear) {
    clear.hidden = !activityClientFilter;
    clear.textContent = activityClientFilter
      ? "device: " + activityClientFilter + " ✕"
      : "";
  }
  void fetchActivity();
}

async function fetchActivity() {
  const body: any = { limit: 50 };
  if (activityClientFilter) {
    body.client = activityClientFilter;
  }
  const res = await api("activity", body);
  if (!res || !res.ok || !res.data) {
    return;
  }
  renderActivity(Array.isArray(res.data.activity) ? res.data.activity : []);
}

// Workspace ledger: every mutating tool execution from any client surface,
// newest first. Facts render as text only.
function renderActivity(rows: any[]) {
  const node = el("activity-list");
  if (!node) {
    return;
  }
  clearNode(node);
  show("activity-empty", rows.length === 0);
  setText("activity-count", rows.length ? "(" + rows.length + ")" : "");
  for (const row of rows) {
    const item = document.createElement("li");
    item.className = "timeline-event" + (row && row.success ? "" : " activity-failed");
    const head = document.createElement("div");
    head.className = "timeline-head";
    const kind = document.createElement("span");
    kind.className = "timeline-kind";
    kind.textContent =
      (row && row.tool ? String(row.tool) : "tool") + (row && row.success ? "" : " (failed)");
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = [
      row && row.surface ? String(row.surface) : "",
      row && row.client ? "device " + String(row.client) : "",
      timeLabel(row ? row.created_at : null),
    ]
      .filter((value) => !!value && value !== "not available")
      .join(" · ");
    head.appendChild(kind);
    head.appendChild(meta);
    item.appendChild(head);
    const detailText = activityDetail(row);
    if (detailText) {
      const body = document.createElement("div");
      body.className = "timeline-payload muted small";
      body.textContent = detailText;
      item.appendChild(body);
    }
    node.appendChild(item);
  }
}

function activityDetail(row: any): string {
  if (!row) {
    return "";
  }
  const parts = [];
  if (typeof row.command_preview === "string" && row.command_preview) {
    parts.push(row.command_preview);
  } else if (Array.isArray(row.paths) && row.paths.length) {
    parts.push(row.paths.map((path: any) => String(path)).join(", "));
  }
  if (!row.success && typeof row.error_summary === "string" && row.error_summary) {
    parts.push(row.error_summary);
  }
  return parts.join(" — ");
}

function onTokenSubmit(event: SubmitEvent) {
  event.preventDefault();
  const value = inputValue("token-input").trim();
  if (!value) {
    setText("token-error", "Credential cannot be empty.");
    return;
  }
  token = value;
  const input = el("token-input");
  if (input) {
    (input as HTMLInputElement).value = "";
  }
  showConsole();
  void tick();
  startAuto();
}

function init() {
  el("token-form")?.addEventListener("submit", onTokenSubmit);
  for (const button of Array.from(document.querySelectorAll("[data-copy]"))) {
    button.addEventListener("click", () => {
      const id = (button as HTMLElement).getAttribute("data-copy") || "";
      const node = el(id);
      const text = node ? node.textContent || "" : "";
      if (text && text !== "—" && navigator.clipboard) {
        void navigator.clipboard.writeText(text);
        (button as HTMLElement).textContent = "Copied";
        window.setTimeout(() => {
          (button as HTMLElement).textContent = "Copy";
        }, 1200);
      }
    });
  }
  el("refresh-btn")?.addEventListener("click", () => {
    void tick();
  });
  el("lock-btn")?.addEventListener("click", () => {
    lock("");
  });
  el("auto-toggle")?.addEventListener("change", () => {
    autoEnabled = inputChecked("auto-toggle");
    if (autoEnabled) {
      startAuto();
    } else {
      stopAuto();
    }
  });
  el("show-completed")?.addEventListener("change", () => {
    showCompleted = inputChecked("show-completed");
    void fetchTasks();
  });
  el("activity-filter-clear")?.addEventListener("click", () => {
    setActivityClientFilter(activityClientFilter);
  });
  el("workflow-session-timeline")?.addEventListener("scroll", () => {
    const timeline = el("workflow-session-timeline");
    if (!timeline) {
      return;
    }
    updateWorkflowSessionFollowFromScroll(
      workflowSessionState,
      timeline.scrollTop,
      timeline.clientHeight,
      timeline.scrollHeight
    );
    syncWorkflowSessionFollowUi();
  });
  el("workflow-session-jump-latest")?.addEventListener("click", () => {
    jumpWorkflowSessionToLatest(workflowSessionState);
    scrollWorkflowSessionTimelineToLatest();
  });
  el("guide-btn")?.addEventListener("click", () => {
    void sendGuidance();
  });
  // Ctrl/Cmd+Enter sends guidance so a reviewer can fire off short course
  // corrections from the keyboard without tabbing to the button. Plain Enter
  // stays a newline-free submit guard for a single-line text input.
  el("guide-input")?.addEventListener("keydown", (event: KeyboardEvent) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void sendGuidance();
    }
  });
  el("accept-btn")?.addEventListener("click", () => {
    openConfirmUi("accept");
  });
  el("reject-btn")?.addEventListener("click", () => {
    openConfirmUi("reject");
  });
  el("cancel-btn")?.addEventListener("click", () => {
    openConfirmUi("cancel");
  });
  el("confirm-ok")?.addEventListener("click", () => {
    void performAction();
  });
  el("confirm-cancel")?.addEventListener("click", () => {
    closeConfirm(state);
    closeConfirmUi();
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      reviewLoop.hide();
      stopAuto();
    } else if (token) {
      reviewLoop.show();
      startAuto();
      void tick();
    }
  });
  showGate("");
}

init();
