// DOM-free review identity, action binding, and selected-task polling state.

function initialState() {
  return {
    selectedTaskId: "",
    reviewSeq: 0,
    snapshot: null,
    pending: null,
    inFlight: { readiness: false, tasks: false, tick: false },
  };
}

function reviewSnapshot(review) {
  const result = review && review.result ? review.result : null;
  const cursor = review && typeof review.event_cursor === "number" ? review.event_cursor : 0;
  return {
    taskId: review && review.task_id ? String(review.task_id) : "",
    resultId: result && result.result_id ? String(result.result_id) : null,
    eventCursor: cursor,
    review: review,
  };
}

function selectTask(state, taskId) {
  state.selectedTaskId = taskId;
  state.reviewSeq = state.reviewSeq + 1;
  state.snapshot = null;
  state.pending = null;
  return state.reviewSeq;
}

function adoptReview(state, taskId, seq, review) {
  if (seq !== state.reviewSeq || taskId !== state.selectedTaskId) {
    return false;
  }
  const next = reviewSnapshot(review);
  if (state.snapshot && state.snapshot.resultId !== next.resultId) {
    state.pending = null;
  }
  state.snapshot = next;
  return true;
}

function actionsEnabled(state) {
  return !!state.snapshot && state.snapshot.taskId === state.selectedTaskId;
}

function openConfirm(state, action) {
  if (!actionsEnabled(state)) {
    state.pending = null;
    return null;
  }
  state.pending = { action: action, snapshot: state.snapshot };
  return state.pending;
}

function closeConfirm(state) {
  state.pending = null;
}

function actionRequest(pending) {
  if (!pending || !pending.snapshot || !pending.action || !pending.snapshot.taskId) {
    return null;
  }
  const snapshot = pending.snapshot;
  if (pending.action === "accept") {
    if (!snapshot.resultId) {
      return null;
    }
    return {
      path: "result/accept",
      body: { task_id: snapshot.taskId, result_id: snapshot.resultId },
    };
  }
  if (pending.action === "reject") {
    const body = { task_id: snapshot.taskId };
    if (snapshot.resultId) {
      body.result_id = snapshot.resultId;
    }
    return { path: "result/reject", body: body };
  }
  return { path: "task/cancel", body: { task_id: snapshot.taskId } };
}

function beginRefresh(state, channel) {
  if (state.inFlight[channel]) {
    return false;
  }
  state.inFlight[channel] = true;
  return true;
}

function endRefresh(state, channel) {
  state.inFlight[channel] = false;
}

function reset(state) {
  state.selectedTaskId = "";
  state.reviewSeq = state.reviewSeq + 1;
  state.snapshot = null;
  state.pending = null;
  state.inFlight = { readiness: false, tasks: false, tick: false };
}

function createReviewController(options) {
  let taskId = "";
  let cursor = null;
  let signature = "";
  let active = null;
  let scheduled = null;
  let failures = 0;
  let generation = 0;
  let visible = true;
  let authorized = true;
  let running = false;

  function invalidate() {
    generation += 1;
    if (scheduled !== null) {
      options.cancelSchedule(scheduled);
    }
    scheduled = null;
    if (active) {
      active.abort();
    }
    active = null;
    running = false;
  }

  function schedule(version, delay) {
    scheduled = options.schedule(() => {
      if (version !== generation) {
        return;
      }
      scheduled = null;
      void request(false);
    }, delay);
  }

  function identity(review) {
    const result = review && review.result ? review.result.result_id : null;
    const execution = review && review.recent_execution ? review.recent_execution : {};
    return JSON.stringify([
      review && review.task_id,
      result,
      review && review.status,
      review && review.run_status,
      execution.execution_status,
      execution.stdout_cursor,
      execution.stderr_cursor,
      execution.assertion_status,
    ]);
  }

  async function request(full) {
    if (!taskId || !visible || !authorized || running) {
      return;
    }
    running = true;
    const selected = taskId;
    const version = ++generation;
    const requestAbort = options.abort();
    active = requestAbort;
    const body = {
      task_id: selected,
      include_diff: true,
      include_output_tail: true,
      after_cursor: full ? null : cursor,
      wait_ms: full ? 0 : 15000,
    };
    let delay = 0;
    try {
      let response = null;
      try {
        response = await options.fetchReview(body, requestAbort.signal);
      } catch {}
      if (version !== generation || selected !== taskId) {
        return;
      }
      if (response && response.status === 401) {
        authorized = false;
        taskId = "";
        invalidate();
        options.unauthorized();
        return;
      }
      const valid =
        response &&
        response.ok &&
        response.data &&
        String(response.data.task_id) === selected;
      if (valid) {
        failures = 0;
        const nextSignature = identity(response.data);
        const nextCursor =
          typeof response.data.event_cursor === "number" ? response.data.event_cursor : cursor;
        if (full || nextCursor !== cursor || nextSignature !== signature) {
          cursor = nextCursor;
          signature = nextSignature;
          options.render(response.data);
        }
      } else {
        failures += 1;
        delay = Math.min(1000 * 2 ** (failures - 1), 15000);
        if (response && !response.ok) {
          options.error(response.data);
        }
      }
    } finally {
      if (version === generation) {
        active = null;
        running = false;
        if (taskId && visible && authorized) {
          schedule(version, delay);
        }
      }
    }
  }

  return {
    select(nextTaskId) {
      invalidate();
      taskId = nextTaskId;
      cursor = null;
      signature = "";
      failures = 0;
      authorized = true;
      void request(true);
    },
    restart() {
      invalidate();
      cursor = null;
      signature = "";
      failures = 0;
      void request(true);
    },
    hide() {
      visible = false;
      invalidate();
    },
    show() {
      if (!visible) {
        visible = true;
        void request(cursor === null);
      }
    },
    stop() {
      invalidate();
      taskId = "";
      cursor = null;
      signature = "";
      failures = 0;
    },
    running() {
      return running;
    },
  };
}

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

const CONSOLE_BASE = "/api/console/";
const REFRESH_MS = 8000;
const WORK_QUEUE_HINT = "No tasks need attention.";

// Credential is held only in this in-memory variable for the lifetime of the
// page. A refresh intentionally requires re-entering it.
let token = "";
let autoEnabled = true;
let showCompleted = false;
let timer = 0;
let reviewLoop = null;
let projectName = "";

// The single review-identity/concurrency state object (pure logic in the
// review_state module operates on it).
const state = initialState();

function el(id) {
  return document.getElementById(id);
}

function setText(id, input) {
  const node = el(id);
  if (node) {
    node.textContent =
      input === null || input === undefined || input === "" ? "—" : String(input);
  }
}

function show(id, visible) {
  const node = el(id);
  if (node) {
    node.hidden = !visible;
  }
}

function clearNode(node) {
  while (node && node.firstChild) {
    node.removeChild(node.firstChild);
  }
}

function inputValue(id) {
  const node = el(id);
  return node ? (node ).value : "";
}

function inputChecked(id) {
  const node = el(id);
  return node ? (node ).checked : false;
}

function showGate(message) {
  show("token-gate", true);
  show("console", false);
  show("topbar-controls", false);
  stopAuto();
  setText("token-error", message);
  const input = el("token-input");
  if (input) {
    (input ).value = "";
    (input ).focus();
  }
}

function showConsole() {
  show("token-gate", false);
  show("console", true);
  show("topbar-controls", true);
}

function showError(message) {
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

function lock(message) {
  token = "";
  reset(state);
  if (reviewLoop) {
    reviewLoop.stop();
  }
  closeConfirmUi();
  showGate(message);
}

// Single host-local request helper. Always POSTs JSON with a Bearer header and
// never echoes the token anywhere.
async function api(path, body, signal = null) {
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
    showError("WebCodex is not reachable. Run webcodex agent start.");
    return null;
  }
  let data = null;
  try {
    data = await response.json();
  } catch {
    data = null;
  }
  return { status: response.status, ok: response.ok, data: data };
}

reviewLoop = createReviewController({
  fetchReview: (body, signal) => api("task/review", body, signal),
  abort: () => new AbortController(),
  schedule: (next, delay) => window.setTimeout(next, delay),
  cancelSchedule: (handle) => window.clearTimeout(handle),
  unauthorized: () => lock("Credential rejected. Re-enter it."),
  error: (data) => showError(errorMessage(data)),
  render: (review) => {
    const previousResult = state.snapshot ? state.snapshot.resultId : null;
    if (!adoptReview(state, String(review.task_id), state.reviewSeq, review)) {
      return;
    }
    if (previousResult !== state.snapshot.resultId) {
      closeConfirmUi();
      hideActionButtons();
    }
    hideError();
    renderDetail(review);
    renderSelection();
  },
});

function errorMessage(data) {
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

function renderReadiness(readiness) {
  projectName = readiness.project || "";
  setText("project", readiness.project || "Not configured");
  setText("connection", readiness.connection);
  setText("agent", readiness.agent);
  setText("capabilities", readiness.capabilities);
  setText("coding", readiness.ready ? "Ready" : "Needs action");
  setText("next-action", readiness.next_action || "No action needed");
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

function renderTaskList(tasks) {
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

function appendChip(parent, text) {
  if (!text) {
    return;
  }
  const span = document.createElement("span");
  span.textContent = text;
  parent.appendChild(span);
}

// Server-supplied time, rendered  fact — never inferred from list position.
function updatedLabel(updatedAt) {
  if (typeof updatedAt !== "number" || updatedAt <= 0) {
    return "updated not available";
  }
  return "updated " + new Date(updatedAt * 1000).toLocaleTimeString();
}

// Select a task: invalidate the previous snapshot, hide stale action buttons,
// show a loading detail, then load the new review under a fresh sequence.
function selectTaskUi(taskId) {
  selectTask(state, taskId);
  renderSelection();
  showDetailLoading();
  reviewLoop.select(taskId);
}

function renderSelection() {
  const list = el("task-list");
  if (!list) {
    return;
  }
  for (const child of Array.from(list.children)) {
    const item = child ;
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

function renderDetail(d) {
  show("detail-empty", false);
  show("detail", true);
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
  renderActions(d);
  setText("detail-next", "Next: " + (d.next_action || "not available"));
}

function timeLabel(value) {
  return typeof value === "number" && value > 0
    ? new Date(value * 1000).toLocaleString()
    : "not available";
}

function checkList(validation, execution) {
  if (validation && Array.isArray(validation.checks)) {
    return validation.checks;
  }
  if (execution && Array.isArray(execution.checks)) {
    return execution.checks;
  }
  return [];
}

function renderChecks(validation, execution) {
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

function renderFiles(d) {
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

function renderDiff(d) {
  const diff = d.changes && d.changes.diff_preview ? d.changes.diff_preview : null;
  const pre = el("detail-diff");
  const hasText = diff && typeof diff.text === "string" && diff.text.length > 0;
  show("detail-diff-section", true);
  if (pre) {
    // textContent, never innerHTML: project output is never trusted .
    pre.textContent = hasText ? diff.text : "not available";
  }
  show("detail-diff-trunc", !!(diff && diff.truncated));
}

function renderOutput(execution) {
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

// Buttons are offered only when the current snapshot is live (actionsEnabled)
// AND the durable state permits the action.
function renderActions(d) {
  const enabled = actionsEnabled(state);
  show("accept-btn", enabled && !!d.can_accept);
  show("reject-btn", enabled && !!d.can_reject);
  show("cancel-btn", enabled && !!d.can_cancel);
}

// Open a confirmation bound to the CURRENT snapshot. If the selection changed
// and no fresh snapshot exists, the action is denied (no modal).
function openConfirmUi(action) {
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
  // Only cancel carries an optional reason; reject has no durable reason field.
  show("confirm-reason-row", action === "cancel");
  const reason = el("confirm-reason");
  if (reason) {
    (reason ).value = "";
  }
  show("confirm-overlay", true);
}

function addLine(parent, label, value) {
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
  // A cancel may carry an optional human reason; identity still comes only from
  // the bound snapshot, never the live selection.
  if (req && req.path === "task/cancel") {
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
  } finally {
    endRefresh(state, "tick");
  }
}

function onTokenSubmit(event) {
  event.preventDefault();
  const value = inputValue("token-input").trim();
  if (!value) {
    setText("token-error", "Credential cannot be empty.");
    return;
  }
  token = value;
  const input = el("token-input");
  if (input) {
    (input ).value = "";
  }
  showConsole();
  void tick();
  startAuto();
}

function init() {
  el("token-form")?.addEventListener("submit", onTokenSubmit);
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
