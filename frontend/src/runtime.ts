import {
  workflowSessionListOverviewFacts,
  workflowSessionOverviewPresentation,
  updateWorkflowSessionFollowFromScroll,
  workflowSessionScrollTopAfterRender,
  jumpWorkflowSessionToLatest,
  shouldFollowWorkflowSessionLatest,
} from "./workflow_session_state.js";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  preferredRuntimeProjectSelection,
  invalidateRuntimeCredential,
  beginRuntimeCredential,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  selectRuntimeWorkflowSession,
  refreshRuntimeWorkflowSession,
  clearRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
} from "./runtime_console_state.js";

const API_BASE = "/api/runtime-console/";
const REFRESH_MS = 8000;

let token = "";
let timer = 0;
let projectsAbort: AbortController | null = null;
let sessionsAbort: AbortController | null = null;
let detailAbort: AbortController | null = null;
let projectRows: any[] = [];
let projectRowsTruncated = false;
let sessionRows: any[] = [];
const state = initialRuntimeConsoleState();

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function setText(id: string, value: unknown): void {
  const node = el(id);
  if (node) {
    node.textContent = value === null || value === undefined || value === "" ? "—" : String(value);
  }
}

function show(id: string, visible: boolean): void {
  const node = el(id);
  if (node) {
    node.hidden = !visible;
  }
}

function clearNode(node: any): void {
  while (node && node.firstChild) {
    node.removeChild(node.firstChild);
  }
}

function appendChip(parent: HTMLElement, text: string, extraClass = ""): void {
  const chip = document.createElement("span");
  chip.className = "chip" + (extraClass ? " " + extraClass : "");
  chip.textContent = text;
  parent.appendChild(chip);
}

function abort(controller: AbortController | null): void {
  if (controller) controller.abort();
}

function abortProjectWork(): void {
  abort(sessionsAbort);
  abort(detailAbort);
  sessionsAbort = null;
  detailAbort = null;
}

function abortAll(): void {
  abort(projectsAbort);
  projectsAbort = null;
  abortProjectWork();
}

async function api(path: string, payload: any, signal?: AbortSignal): Promise<any> {
  try {
    const response = await fetch(API_BASE + path, {
      method: "POST",
      headers: {
        Authorization: "Bearer " + token,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
      signal,
    });
    let data: any = null;
    try {
      data = await response.json();
    } catch {
      data = null;
    }
    return { ok: response.ok, status: response.status, data };
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") return null;
    return { ok: false, status: 0, data: null };
  }
}

function hideDetail(): void {
  show("runtime-session-detail", false);
  show("runtime-session-detail-empty", true);
  show("runtime-jump-latest", false);
}

function clearSessionSurface(): void {
  sessionRows = [];
  clearNode(el("runtime-session-list"));
  show("runtime-sessions-empty", false);
  clearRuntimeWorkflowSession(state);
  hideDetail();
}

function lock(message = ""): void {
  token = "";
  abortAll();
  invalidateRuntimeCredential(state);
  projectRows = [];
  projectRowsTruncated = false;
  clearSessionSurface();
  show("runtime-token-gate", true);
  show("runtime-console", false);
  show("runtime-topbar-controls", false);
  stopAuto();
  setText("runtime-token-error", message);
  const input = el("runtime-token-input") as HTMLInputElement | null;
  if (input) {
    input.value = "";
    input.focus();
  }
}

function unlockUi(): void {
  show("runtime-token-gate", false);
  show("runtime-console", true);
  show("runtime-topbar-controls", true);
  setText("runtime-token-error", "");
  startAuto();
}

function showError(message: string): void {
  setText("runtime-error", message);
  show("runtime-error", !!message);
}

function projectLabel(project: any): string {
  const name = project && project.name ? String(project.name) : "";
  const id = project && project.id ? String(project.id) : "";
  const identity = name && name !== id ? name + " — " + id : id;
  const status = project && project.connected
    ? String(project.agent_status || "online")
    : "offline";
  return identity + " · " + status;
}

async function fetchProjects(request: any, unlocking = false): Promise<void> {
  abort(projectsAbort);
  const controller = new AbortController();
  projectsAbort = controller;
  const response = await api("projects", { limit: 100 }, controller.signal);
  if (projectsAbort === controller) projectsAbort = null;
  if (!response || !isCurrentRuntimeProjectsRequest(state, request)) return;
  if (response.status === 401 || response.status === 403) {
    lock("Credential does not have Runtime Console access.");
    return;
  }
  if (!response.ok || !response.data) {
    if (unlocking) lock("Runtime Console is unavailable.");
    else showError("Could not refresh projects.");
    return;
  }
  projectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
  projectRowsTruncated = !!response.data.truncated;
  unlockUi();
  showError("");

  const currentDevice = String(state.selectedDevice || "");
  const currentProject = String(state.selectedProject || "");
  const selection = preferredRuntimeProjectSelection(
    projectRows,
    currentDevice,
    currentProject
  );
  if (!selection.project) {
    if (currentDevice || currentProject) {
      abortProjectWork();
      selectRuntimeProject(state, "", "");
    }
    renderProjectSelectors(projectRows, projectRowsTruncated);
    clearSessionSurface();
    setText("runtime-selected-project", "No project selected");
    return;
  }
  if (selection.device !== currentDevice || selection.project !== currentProject) {
    switchProject(selection.device, selection.project);
  } else {
    renderProjectSelectors(projectRows, projectRowsTruncated);
    const listRequest = refreshRuntimeSessionList(state);
    if (listRequest) void fetchSessions(listRequest);
  }
}

function renderProjectSelectors(projects: any[], truncated: boolean): void {
  const deviceSelect = el("runtime-device-select") as HTMLSelectElement | null;
  const projectSelect = el("runtime-project-select") as HTMLSelectElement | null;
  if (!deviceSelect || !projectSelect) return;

  const devices = runtimeDeviceIds(projects);
  clearNode(deviceSelect);
  for (const clientId of devices) {
    const option = document.createElement("option");
    option.value = clientId;
    option.textContent = clientId;
    deviceSelect.appendChild(option);
  }
  if (state.selectedDevice) deviceSelect.value = state.selectedDevice;

  const deviceProjects = runtimeProjectsForDevice(projects, String(state.selectedDevice || ""));
  clearNode(projectSelect);
  for (const project of deviceProjects) {
    const option = document.createElement("option");
    option.value = project.id;
    option.textContent = projectLabel(project);
    projectSelect.appendChild(option);
  }
  if (state.selectedProject) projectSelect.value = state.selectedProject;

  setText(
    "runtime-device-status",
    devices.length
      ? devices.length + " device" + (devices.length === 1 ? "" : "s") + " shown" + (truncated ? " · bounded project list" : "")
      : "No authorized devices"
  );
  setText(
    "runtime-project-status",
    state.selectedDevice
      ? deviceProjects.length + " authorized project" + (deviceProjects.length === 1 ? "" : "s") + " on this device" + (truncated ? " · from bounded list" : "")
      : "No authorized projects"
  );
}

function switchProject(device: string, project: string): void {
  abortProjectWork();
  clearSessionSurface();
  const request = selectRuntimeProject(state, device, project);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  setText("runtime-selected-project", project || "No project selected");
  if (request) void fetchSessions(request);
}

async function fetchSessions(request: any): Promise<void> {
  abort(sessionsAbort);
  const controller = new AbortController();
  sessionsAbort = controller;
  const response = await api(
    "workflow-sessions",
    { project: request.project, limit: 50 },
    controller.signal
  );
  if (sessionsAbort === controller) sessionsAbort = null;
  if (!response || !isCurrentRuntimeSessionListRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 403 || response.status === 404) {
    showError("Selected project is no longer available.");
    return;
  }
  if (!response.ok || !response.data) {
    showError("Could not refresh Workflow Sessions.");
    return;
  }
  sessionRows = Array.isArray(response.data.sessions) ? response.data.sessions : [];
  renderSessionList(sessionRows, response.data);
  showError("");
  const selected = String(state.workflow.selectedSessionId || "");
  if (selected && sessionRows.some((row) => String(row.session_id || "") === selected)) {
    const detailRequest = refreshRuntimeWorkflowSession(state);
    if (detailRequest) void fetchSessionDetail(detailRequest);
  } else if (selected) {
    clearRuntimeWorkflowSession(state);
    hideDetail();
  }
}

function updatedLabel(timestamp: any): string {
  if (typeof timestamp !== "number") return "time unavailable";
  return new Date(timestamp * 1000).toLocaleTimeString();
}

function activityKindLabel(activity: any): string {
  const kind = String(activity && activity.kind || "Activity");
  if (activity && activity.job_handoff) {
    if (kind === "Tested") return "Test";
    if (kind === "Ran") return "Command";
  }
  if (kind === "Explored" && activity && typeof activity.group_count === "number") {
    return "Explored ×" + activity.group_count;
  }
  return kind;
}

function activityFacts(activity: any, includeTiming: boolean): string[] {
  const facts: string[] = [];
  if (activity && typeof activity.group_count === "number") {
    if (Array.isArray(activity.group_kinds) && activity.group_kinds.length) {
      facts.push(activity.group_kinds.map((value: any) => String(value)).join(" / "));
    }
    if (Array.isArray(activity.group_tools) && activity.group_tools.length) {
      facts.push(activity.group_tools.map((value: any) => String(value)).join(", "));
    }
  } else if (activity && activity.tool) {
    facts.push(String(activity.tool));
  }
  if (activity && activity.kind === "Progress") {
    facts.push("informational");
  } else if (activity && activity.job_handoff) {
    facts.push("handed off");
    if (activity.execution_state) facts.push("execution " + String(activity.execution_state));
  } else if (activity && activity.state) {
    facts.push(String(activity.state));
  }
  if (activity && activity.job_id) facts.push("job " + String(activity.job_id));
  if (includeTiming && activity && typeof activity.started_at === "number") {
    facts.push(new Date(activity.started_at * 1000).toLocaleTimeString());
  }
  return facts;
}

function activityDescription(activity: any): string {
  if (!activity) return "";
  const parts = [activityKindLabel(activity), ...activityFacts(activity, false)];
  if (activity.summary && !activity.job_handoff) parts.push(String(activity.summary));
  return parts.join(" · ");
}

function appendPreview(parent: HTMLElement, label: string, activity: any): void {
  if (!activity) return;
  const row = document.createElement("div");
  row.className = "activity-preview muted small";
  const prefix = document.createElement("span");
  prefix.className = "activity-preview-label";
  prefix.textContent = label;
  const text = document.createElement("span");
  text.textContent = activityDescription(activity);
  row.appendChild(prefix);
  row.appendChild(text);
  parent.appendChild(row);
}

function renderSessionList(sessions: any[], payload: any): void {
  const node = el("runtime-session-list");
  if (!node) return;
  clearNode(node);
  show("runtime-sessions-empty", sessions.length === 0);
  const total = typeof payload.total === "number" ? payload.total : sessions.length;
  setText("runtime-sessions-count", total ? sessions.length + (payload.truncated ? " of " + total : "") : "0");
  const selected = String(state.workflow.selectedSessionId || "");
  for (const session of sessions) {
    const id = String(session && session.session_id || "");
    if (!id) continue;
    const item = document.createElement("li");
    item.className = "session-card" + (id === selected ? " selected" : "");
    const title = document.createElement("div");
    title.className = "session-title";
    title.textContent = session.title ? String(session.title) : id;
    const meta = document.createElement("div");
    meta.className = "chips";
    appendChip(meta, String(session.lifecycle || "unknown"));
    if (session.running_call) appendChip(meta, "running");
    appendChip(meta, updatedLabel(session.updated_at));
    item.appendChild(title);
    item.appendChild(meta);
    const facts = workflowSessionListOverviewFacts(session.overview);
    if (facts.length) {
      const summary = document.createElement("div");
      summary.className = "summary-facts";
      for (const fact of facts) appendChip(summary, fact.text, "tone-" + fact.tone);
      item.appendChild(summary);
    }
    appendPreview(item, "Now", session.current_activity);
    appendPreview(item, "Last", session.last_activity);
    item.addEventListener("click", () => selectSession(id));
    node.appendChild(item);
  }
}

function selectSession(sessionId: string): void {
  abort(detailAbort);
  detailAbort = null;
  hideDetail();
  const request = selectRuntimeWorkflowSession(state, sessionId);
  renderSessionList(sessionRows, { total: sessionRows.length, truncated: false });
  if (request) void fetchSessionDetail(request);
}

async function fetchSessionDetail(request: any): Promise<void> {
  abort(detailAbort);
  const controller = new AbortController();
  detailAbort = controller;
  const response = await api(
    "workflow-session",
    { project: request.project, session_id: request.sessionId, limit: 100 },
    controller.signal
  );
  if (detailAbort === controller) detailAbort = null;
  if (!response || !isCurrentRuntimeWorkflowSessionRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 404) {
    clearRuntimeWorkflowSession(state);
    hideDetail();
    return;
  }
  if (!response.ok || !response.data) {
    showError("Could not refresh Workflow Session detail.");
    return;
  }
  if (!adoptRuntimeWorkflowSessionDetail(state, request, response.data)) return;
  renderDetail(response.data);
}

function setTone(id: string, tone: string): void {
  const node = el(id);
  if (!node) return;
  for (const name of ["pass", "warn", "fail", "muted"]) {
    node.classList.toggle("tone-card-" + name, tone === name);
  }
}

function renderOverview(overview: any): void {
  const view = workflowSessionOverviewPresentation(overview);
  setText("runtime-overview-work", view.workText);
  setText(
    "runtime-overview-validation",
    view.validationText + (typeof view.validationAt === "number" ? " · " + updatedLabel(view.validationAt) : "")
  );
  setTone("runtime-overview-validation-card", view.validationTone);
  setText("runtime-overview-attention", view.attentionText);
  setTone("runtime-overview-attention-card", view.attentionTone);
  setText(
    "runtime-overview-progress",
    view.progressText + (typeof view.progressAt === "number" ? " · reported " + updatedLabel(view.progressAt) : "")
  );
}

function syncFollowUi(): void {
  show(
    "runtime-jump-latest",
    !!state.workflow.selectedSessionId && !shouldFollowWorkflowSessionLatest(state.workflow)
  );
}

function renderDetail(detail: any): void {
  show("runtime-session-detail-empty", false);
  show("runtime-session-detail", true);
  setText("runtime-session-title", detail.title);
  setText("runtime-session-lifecycle", detail.lifecycle);
  setText("runtime-session-mode", "mode " + String(detail.mode || "unknown"));
  setText("runtime-session-running", detail.running_call ? "running call" : "no running call");
  setText("runtime-session-updated", "Updated " + updatedLabel(detail.updated_at));
  renderOverview(detail.overview);

  const activities = Array.isArray(detail.activity) ? detail.activity : [];
  const node = el("runtime-timeline");
  const previousScrollTop = node ? node.scrollTop : 0;
  clearNode(node);
  show("runtime-timeline-empty", activities.length === 0);
  if (!node) return syncFollowUi();
  for (const activity of activities) {
    const item = document.createElement("li");
    item.className = "timeline-event";
    if (activity && activity.kind === "Progress") item.classList.add("reported-progress");
    if (activity && ["failed", "timed_out"].includes(String(activity.state || ""))) {
      item.classList.add("failed");
    }
    const head = document.createElement("div");
    head.className = "timeline-head";
    const kind = document.createElement("span");
    kind.className = "timeline-kind";
    kind.textContent = activityKindLabel(activity);
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = activityFacts(activity, true).join(" · ");
    head.appendChild(kind);
    head.appendChild(meta);
    item.appendChild(head);
    if (activity && activity.summary) {
      const body = document.createElement("div");
      body.className = "timeline-body small";
      body.textContent = String(activity.summary);
      item.appendChild(body);
    }
    if (activity && Array.isArray(activity.paths) && activity.paths.length) {
      const paths = document.createElement("div");
      paths.className = "muted small";
      paths.textContent = activity.paths.map((path: any) => String(path)).join(" · ");
      item.appendChild(paths);
    }
    node.appendChild(item);
  }
  node.scrollTop = workflowSessionScrollTopAfterRender(
    state.workflow,
    previousScrollTop,
    node.clientHeight,
    node.scrollHeight
  );
  syncFollowUi();
}

function jumpLatest(): void {
  jumpWorkflowSessionToLatest(state.workflow);
  const node = el("runtime-timeline");
  if (node) node.scrollTop = node.scrollHeight;
  syncFollowUi();
}

async function refreshAll(): Promise<void> {
  if (!token) return;
  await fetchProjects(refreshRuntimeProjects(state));
}

function startAuto(): void {
  stopAuto();
  timer = window.setInterval(() => {
    const request = refreshRuntimeSessionList(state);
    if (request) void fetchSessions(request);
  }, REFRESH_MS);
}

function stopAuto(): void {
  if (timer) window.clearInterval(timer);
  timer = 0;
}

el("runtime-token-form")?.addEventListener("submit", (event) => {
  event.preventDefault();
  const input = el("runtime-token-input") as HTMLInputElement | null;
  const nextToken = input ? input.value.trim() : "";
  if (input) input.value = "";
  if (!nextToken) {
    setText("runtime-token-error", "Enter a runtime Bearer credential.");
    return;
  }
  token = nextToken;
  const request = beginRuntimeCredential(state);
  void fetchProjects(request, true);
});

el("runtime-device-select")?.addEventListener("change", () => {
  const select = el("runtime-device-select") as HTMLSelectElement | null;
  if (!select) return;
  const projects = runtimeProjectsForDevice(projectRows, select.value);
  switchProject(select.value, projects.length ? String(projects[0].id) : "");
});

el("runtime-project-select")?.addEventListener("change", () => {
  const select = el("runtime-project-select") as HTMLSelectElement | null;
  if (select) switchProject(String(state.selectedDevice || ""), select.value);
});

el("runtime-refresh")?.addEventListener("click", () => void refreshAll());
el("runtime-lock")?.addEventListener("click", () => lock());
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
  const node = el("runtime-timeline");
  if (!node) return;
  updateWorkflowSessionFollowFromScroll(
    state.workflow,
    node.scrollTop,
    node.clientHeight,
    node.scrollHeight
  );
  syncFollowUi();
});
window.addEventListener("pagehide", () => {
  token = "";
  abortAll();
  stopAuto();
});

lock();
