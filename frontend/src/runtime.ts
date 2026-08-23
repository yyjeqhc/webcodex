import {
  workflowSessionListOverviewFacts,
  workflowSessionOverviewPresentation,
  workflowSessionIdleAttentionLabel,
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
  refreshRuntimeOverview,
  isCurrentRuntimeOverviewRequest,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  refreshRuntimeRunner,
  isCurrentRuntimeRunnerRequest,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  selectRuntimeWorkflowSession,
  refreshRuntimeWorkflowSession,
  clearRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
  adoptRuntimeCollaborationObservation,
  setRuntimeCollaborationAvailable,
  runtimeCollaborationObservationAction,
} from "./runtime_console_state.js";

const API_BASE = "/api/runtime-console/";
const REFRESH_MS = 8000;
const COLLABORATION_WAIT_SECS = 25;

let token = "";
let timer = 0;
let overviewAbort: AbortController | null = null;
let projectsAbort: AbortController | null = null;
let runnerAbort: AbortController | null = null;
let sessionsAbort: AbortController | null = null;
let detailAbort: AbortController | null = null;
let collaborationAbort: AbortController | null = null;
let projectRows: any[] = [];
let projectRowsTruncated = false;
let sessionRows: any[] = [];
const state = initialRuntimeConsoleState();

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function setText(id: string, value: unknown): void {
  const node = el(id);
  if (node) node.textContent = value === null || value === undefined || value === "" ? "—" : String(value);
}

function show(id: string, visible: boolean): void {
  const node = el(id);
  if (node) node.hidden = !visible;
}

function clearNode(node: any): void {
  while (node && node.firstChild) node.removeChild(node.firstChild);
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

function abortCollaboration(): void {
  abort(collaborationAbort);
  collaborationAbort = null;
}

function abortProjectWork(): void {
  abort(sessionsAbort);
  abort(detailAbort);
  abortCollaboration();
  sessionsAbort = null;
  detailAbort = null;
}

function abortAll(): void {
  abort(overviewAbort);
  abort(projectsAbort);
  abort(runnerAbort);
  overviewAbort = null;
  projectsAbort = null;
  runnerAbort = null;
  abortProjectWork();
}

async function api(path: string, payload: any, signal?: AbortSignal): Promise<any> {
  try {
    const response = await fetch(API_BASE + path, {
      method: "POST",
      headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      signal,
    });
    let data: any = null;
    try { data = await response.json(); } catch { data = null; }
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
  clearNode(el("runtime-collaboration-board"));
}

function clearSessionSurface(): void {
  sessionRows = [];
  clearNode(el("runtime-session-list"));
  show("runtime-sessions-empty", false);
  clearRuntimeWorkflowSession(state);
  abortCollaboration();
  hideDetail();
}

function lock(message = ""): void {
  token = "";
  abortAll();
  invalidateRuntimeCredential(state);
  projectRows = [];
  projectRowsTruncated = false;
  clearSessionSurface();
  clearNode(el("runtime-runner-projects"));
  show("runtime-token-gate", true);
  show("runtime-console", false);
  show("runtime-topbar-controls", false);
  stopAuto();
  setText("runtime-token-error", message);
  const input = el("runtime-token-input") as HTMLInputElement | null;
  if (input) { input.value = ""; input.focus(); }
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

function countLabel(value: any, singular: string, plural = singular + "s"): string {
  const count = typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0;
  return count + " " + (count === 1 ? singular : plural);
}

function attentionLabel(attention: any): string {
  const parts: string[] = [];
  for (const [key, singular] of [["open_risks", "risk"], ["open_todos", "todo"], ["open_questions", "question"], ["open_guidance", "guidance"]] as const) {
    const count = typeof attention?.[key] === "number" ? attention[key] : 0;
    if (count) parts.push(countLabel(count, singular));
  }
  return parts.length ? parts.join(" · ") : "No retained pending attention";
}

async function fetchOverview(request: any): Promise<void> {
  abort(overviewAbort);
  const controller = new AbortController();
  overviewAbort = controller;
  const response = await api("overview", {}, controller.signal);
  if (overviewAbort === controller) overviewAbort = null;
  if (!response || !isCurrentRuntimeOverviewRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 403) {
    show("runtime-overview-unavailable", true);
    setText("runtime-overview-access", "runtime:read unavailable");
    return;
  }
  if (!response.ok || !response.data) {
    setText("runtime-overview-access", "refresh unavailable");
    return;
  }
  show("runtime-overview-unavailable", false);
  setText("runtime-overview-access", "runtime:read");
  const data = response.data;
  setText("runtime-server-identity", [data.service, data.version].filter(Boolean).join(" · "));
  setText("runtime-server-build", data.build_git_commit ? "build " + data.build_git_commit + (data.build_git_dirty ? " · dirty" : "") : "build unavailable");
  setText("runtime-server-runners", countLabel(data.runner_count, "Runner"));
  setText("runtime-server-alignment", countLabel(data.runners_online, "online") + " · " + countLabel(data.runners_stale, "stale") + " · " + countLabel(data.runners_unavailable, "unavailable"));
  setText("runtime-server-projects", data.projects_available ? countLabel(data.visible_projects, "visible Project") + (data.projects_truncated ? " +" : "") : "project:read unavailable");
  setText("runtime-server-jobs", countLabel(data.active_jobs, "active Job") + (data.mixed_builds_present ? " · mixed builds" : ""));
  setText("runtime-server-attention", attentionLabel(data.workflow_sessions));
  setText("runtime-server-sessions", countLabel(data.workflow_sessions?.active, "active Session") + " · " + countLabel(data.workflow_sessions?.running, "running call") + (data.workflow_sessions?.truncated ? " · bounded aggregate" : ""));
}

function projectLabel(project: any): string {
  const name = project && project.name ? String(project.name) : "";
  const id = project && project.id ? String(project.id) : "";
  const identity = name && name !== id ? name + " — " + id : id;
  const status = project && project.connected ? String(project.agent_status || "online") : "offline";
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
    lock("Credential does not have Runtime Console project access.");
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
  void fetchOverview(refreshRuntimeOverview(state));

  const currentDevice = String(state.selectedDevice || "");
  const currentProject = String(state.selectedProject || "");
  const selection = preferredRuntimeProjectSelection(projectRows, currentDevice, currentProject);
  if (!selection.project) {
    if (currentDevice || currentProject) {
      abortProjectWork();
      selectRuntimeProject(state, selection.device || "", "");
    }
    renderProjectSelectors(projectRows, projectRowsTruncated);
    clearSessionSurface();
    setText("runtime-selected-project", "No project selected");
    const runnerRequest = refreshRuntimeRunner(state);
    if (runnerRequest) void fetchRunner(runnerRequest);
    return;
  }
  if (selection.device !== currentDevice || selection.project !== currentProject) {
    switchProject(selection.device, selection.project);
  } else {
    renderProjectSelectors(projectRows, projectRowsTruncated);
    const runnerRequest = refreshRuntimeRunner(state);
    if (runnerRequest) void fetchRunner(runnerRequest);
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
  setText("runtime-device-status", devices.length ? countLabel(devices.length, "authorized Runner") + (truncated ? " · bounded project list" : "") : "No authorized Runners");
  setText("runtime-project-status", state.selectedDevice ? countLabel(deviceProjects.length, "authorized Project") + " on this Runner" + (truncated ? " · bounded list" : "") : "No authorized Projects");
}

function switchProject(device: string, project: string): void {
  abortProjectWork();
  if (state.selectedDevice !== device) { abort(runnerAbort); runnerAbort = null; }
  clearSessionSurface();
  const request = selectRuntimeProject(state, device, project);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  setText("runtime-selected-project", project || "No project selected");
  const runnerRequest = refreshRuntimeRunner(state);
  if (runnerRequest) void fetchRunner(runnerRequest);
  if (request) void fetchSessions(request);
}

async function fetchRunner(request: any): Promise<void> {
  abort(runnerAbort);
  const controller = new AbortController();
  runnerAbort = controller;
  const response = await api("runner", { client_id: request.device, project_limit: 24 }, controller.signal);
  if (runnerAbort === controller) runnerAbort = null;
  if (!response || !isCurrentRuntimeRunnerRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 403) {
    show("runtime-runner-unavailable", true);
    setText("runtime-runner-access", "runtime:read unavailable");
    clearNode(el("runtime-runner-projects"));
    return;
  }
  if (!response.ok || !response.data) {
    show("runtime-runner-unavailable", true);
    setText("runtime-runner-access", "Runner view unavailable");
    return;
  }
  show("runtime-runner-unavailable", false);
  setText("runtime-runner-access", response.data.projects_truncated ? "bounded Project aggregate" : "runtime:read");
  renderRunner(response.data);
}

function renderRunner(data: any): void {
  setText("runtime-runner-id", data.client_id);
  setText("runtime-runner-health", (data.connected ? "connected" : "disconnected") + " · " + String(data.status || "unknown"));
  setText("runtime-runner-version", data.version ? "v" + data.version : "version unavailable");
  setText("runtime-runner-build", data.build_git_commit ? String(data.build_git_commit) + (data.build_git_dirty ? " · dirty" : "") : "build unavailable");
  setText("runtime-runner-jobs", countLabel(data.active_jobs, "active Job"));
  setText("runtime-runner-concurrency", countLabel(data.jobs_running, "running") + " · " + countLabel(data.jobs_queued, "queued") + (typeof data.job_concurrency_limit === "number" ? " · limit " + data.job_concurrency_limit : ""));
  setText("runtime-runner-alignment", data.source_alignment || "unknown");
  setText("runtime-runner-project-count", data.projects_available ? countLabel(data.visible_project_count, "visible Project") : "project:read unavailable");
  const node = el("runtime-runner-projects");
  clearNode(node);
  if (!node || !Array.isArray(data.projects)) return;
  for (const project of data.projects) {
    const card = document.createElement("div");
    card.className = "runner-project-card" + (project.id === state.selectedProject ? " selected" : "");
    const title = document.createElement("div");
    title.className = "runner-project-title";
    title.textContent = project.name && project.name !== project.id ? String(project.name) + " — " + String(project.id) : String(project.id || "");
    const meta = document.createElement("div");
    meta.className = "muted small";
    meta.textContent = (project.connected ? String(project.agent_status || "online") : "offline") + " · " + countLabel(project.sessions?.retained_sessions, "retained Session") + (project.sessions?.sessions_truncated ? " · bounded" : "");
    const facts = document.createElement("div");
    facts.className = "summary-facts";
    appendChip(facts, countLabel(project.sessions?.running_sessions, "running"), "tone-runtime");
    const attention = attentionLabel(project.sessions?.attention);
    if (!attention.startsWith("No retained")) appendChip(facts, attention, "tone-warn");
    if (typeof project.sessions?.latest_updated_at === "number") appendChip(facts, "updated " + updatedLabel(project.sessions.latest_updated_at));
    card.appendChild(title); card.appendChild(meta); card.appendChild(facts);
    card.addEventListener("click", () => switchProject(String(data.client_id || state.selectedDevice), String(project.id || "")));
    node.appendChild(card);
  }
}

async function fetchSessions(request: any): Promise<void> {
  abort(sessionsAbort);
  const controller = new AbortController();
  sessionsAbort = controller;
  const response = await api("workflow-sessions", { project: request.project, limit: 50 }, controller.signal);
  if (sessionsAbort === controller) sessionsAbort = null;
  if (!response || !isCurrentRuntimeSessionListRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 403 || response.status === 404) { showError("Selected project is no longer available."); return; }
  if (!response.ok || !response.data) { showError("Could not refresh Workflow Sessions."); return; }
  sessionRows = Array.isArray(response.data.sessions) ? response.data.sessions : [];
  renderSessionList(sessionRows, response.data);
  showError("");
  const selected = String(state.workflow.selectedSessionId || "");
  if (selected && sessionRows.some((row) => String(row.session_id || "") === selected)) {
    const detailRequest = refreshRuntimeWorkflowSession(state);
    if (detailRequest) void fetchSessionDetail(detailRequest);
  } else if (selected) {
    abortCollaboration();
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
  if (kind === "Explored" && activity && typeof activity.group_count === "number") return "Explored ×" + activity.group_count;
  return kind;
}

function activityFacts(activity: any, includeTiming: boolean): string[] {
  const facts: string[] = [];
  if (activity && typeof activity.group_count === "number") {
    if (Array.isArray(activity.group_kinds) && activity.group_kinds.length) facts.push(activity.group_kinds.map(String).join(" / "));
    if (Array.isArray(activity.group_tools) && activity.group_tools.length) facts.push(activity.group_tools.map(String).join(", "));
  } else if (activity && activity.tool) facts.push(String(activity.tool));
  if (activity && activity.kind === "Progress") facts.push("informational");
  else if (activity && activity.job_handoff) {
    facts.push("handed off");
    if (activity.execution_state) facts.push("execution " + String(activity.execution_state));
  } else if (activity && activity.state) facts.push(String(activity.state));
  if (activity && activity.job_id) facts.push("job " + String(activity.job_id));
  if (includeTiming && activity && typeof activity.started_at === "number") facts.push(new Date(activity.started_at * 1000).toLocaleTimeString());
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
  const row = document.createElement("div"); row.className = "activity-preview muted small";
  const prefix = document.createElement("span"); prefix.className = "activity-preview-label"; prefix.textContent = label;
  const text = document.createElement("span"); text.textContent = activityDescription(activity);
  row.appendChild(prefix); row.appendChild(text); parent.appendChild(row);
}

function renderSessionList(sessions: any[], payload: any): void {
  const node = el("runtime-session-list");
  if (!node) return;
  clearNode(node); show("runtime-sessions-empty", sessions.length === 0);
  const total = typeof payload.total === "number" ? payload.total : sessions.length;
  setText("runtime-sessions-count", total ? sessions.length + (payload.truncated ? " of " + total : "") : "0");
  const selected = String(state.workflow.selectedSessionId || "");
  for (const session of sessions) {
    const id = String(session && session.session_id || "");
    if (!id) continue;
    const item = document.createElement("li"); item.className = "session-card" + (id === selected ? " selected" : "");
    const title = document.createElement("div"); title.className = "session-title"; title.textContent = session.title ? String(session.title) : id;
    const meta = document.createElement("div"); meta.className = "chips";
    appendChip(meta, String(session.lifecycle || "unknown"));
    appendChip(meta, workflowSessionIdleAttentionLabel(!!session.running_call, session.overview));
    appendChip(meta, updatedLabel(session.updated_at));
    item.appendChild(title); item.appendChild(meta);
    const facts = workflowSessionListOverviewFacts(session.overview);
    if (facts.length) {
      const summary = document.createElement("div"); summary.className = "summary-facts";
      for (const fact of facts) appendChip(summary, fact.text, "tone-" + fact.tone);
      item.appendChild(summary);
    }
    appendPreview(item, "Now", session.current_activity); appendPreview(item, "Last", session.last_activity);
    item.addEventListener("click", () => selectSession(id)); node.appendChild(item);
  }
}

function selectSession(sessionId: string): void {
  abort(detailAbort); detailAbort = null; abortCollaboration(); hideDetail();
  const request = selectRuntimeWorkflowSession(state, sessionId);
  renderSessionList(sessionRows, { total: sessionRows.length, truncated: false });
  if (request) void fetchSessionDetail(request);
  const collaborationRequest = runtimeCollaborationRequest(state);
  if (collaborationRequest) void startCollaboration(collaborationRequest);
}

async function fetchSessionDetail(request: any): Promise<void> {
  abort(detailAbort);
  const controller = new AbortController(); detailAbort = controller;
  const response = await api("workflow-session", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (detailAbort === controller) detailAbort = null;
  if (!response || !isCurrentRuntimeWorkflowSessionRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 404) { abortCollaboration(); clearRuntimeWorkflowSession(state); hideDetail(); return; }
  if (!response.ok || !response.data) { showError("Could not refresh Workflow Session detail."); return; }
  if (!adoptRuntimeWorkflowSessionDetail(state, request, response.data)) return;
  renderDetail(response.data);
}

function setTone(id: string, tone: string): void {
  const node = el(id); if (!node) return;
  for (const name of ["pass", "warn", "fail", "muted"]) node.classList.toggle("tone-card-" + name, tone === name);
}

function renderOverview(overview: any): void {
  const view = workflowSessionOverviewPresentation(overview);
  setText("runtime-overview-work", view.workText);
  setText("runtime-overview-validation", view.validationText + (typeof view.validationAt === "number" ? " · " + updatedLabel(view.validationAt) : ""));
  setTone("runtime-overview-validation-card", view.validationTone);
  setText("runtime-overview-attention", view.attentionText); setTone("runtime-overview-attention-card", view.attentionTone);
  setText("runtime-overview-progress", view.progressText + (typeof view.progressAt === "number" ? " · reported " + updatedLabel(view.progressAt) : ""));
}

function syncFollowUi(): void {
  show("runtime-jump-latest", !!state.workflow.selectedSessionId && !shouldFollowWorkflowSessionLatest(state.workflow));
}

function renderDetail(detail: any): void {
  show("runtime-session-detail-empty", false); show("runtime-session-detail", true);
  setText("runtime-session-title", detail.title); setText("runtime-session-lifecycle", detail.lifecycle);
  setText("runtime-session-mode", "mode " + String(detail.mode || "unknown"));
  setText("runtime-session-running", workflowSessionIdleAttentionLabel(!!detail.running_call, detail.overview));
  setText("runtime-session-updated", "Updated " + updatedLabel(detail.updated_at)); renderOverview(detail.overview);
  renderCollaboration();
  const activities = Array.isArray(detail.activity) ? detail.activity : [];
  const node = el("runtime-timeline"); const previousScrollTop = node ? node.scrollTop : 0;
  clearNode(node); show("runtime-timeline-empty", activities.length === 0);
  if (!node) return syncFollowUi();
  for (const activity of activities) {
    const item = document.createElement("li"); item.className = "timeline-event";
    if (activity && activity.kind === "Progress") item.classList.add("reported-progress");
    if (activity && ["failed", "timed_out"].includes(String(activity.state || ""))) item.classList.add("failed");
    const head = document.createElement("div"); head.className = "timeline-head";
    const kind = document.createElement("span"); kind.className = "timeline-kind"; kind.textContent = activityKindLabel(activity);
    const meta = document.createElement("span"); meta.className = "muted small"; meta.textContent = activityFacts(activity, true).join(" · ");
    head.appendChild(kind); head.appendChild(meta); item.appendChild(head);
    if (activity && activity.summary) { const body = document.createElement("div"); body.className = "timeline-body small"; body.textContent = String(activity.summary); item.appendChild(body); }
    if (activity && Array.isArray(activity.paths) && activity.paths.length) { const paths = document.createElement("div"); paths.className = "muted small"; paths.textContent = activity.paths.map(String).join(" · "); item.appendChild(paths); }
    node.appendChild(item);
  }
  node.scrollTop = workflowSessionScrollTopAfterRender(state.workflow, previousScrollTop, node.clientHeight, node.scrollHeight); syncFollowUi();
}

function renderCollaboration(statusText?: string): void {
  const available = state.collaboration.available !== false;
  show("runtime-collaboration-unavailable", !available);
  const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
  show("runtime-collaboration-empty", available && messages.length === 0);
  setText("runtime-collaboration-status", statusText || (available ? countLabel(messages.length, "retained message") : "runtime:read unavailable"));
  const node = el("runtime-collaboration-board"); clearNode(node);
  if (!node || !available) return;
  const byId = new Map<string, any>();
  const children = new Map<string, any[]>();
  for (const message of messages) {
    const id = String(message?.message_id || ""); if (id) byId.set(id, message);
  }
  for (const message of messages) {
    const parent = typeof message?.reply_to === "string" ? message.reply_to : "";
    if (parent && byId.has(parent)) {
      const list = children.get(parent) || []; list.push(message); children.set(parent, list);
    }
  }
  const visited = new Set<string>();
  const appendMessage = (message: any, depth: number, parentUnavailable: boolean): void => {
    const id = String(message?.message_id || ""); if (!id || visited.has(id)) return; visited.add(id);
    const card = document.createElement("article");
    card.className = "message-card " + String(message?.kind || "note") + (String(message?.status || "") === "resolved" ? " resolved" : "") + (parentUnavailable ? " retained-reply" : "");
    if (depth > 0) card.classList.add("message-thread");
    const head = document.createElement("div"); head.className = "message-head";
    const kind = document.createElement("span"); kind.className = "message-kind"; kind.textContent = String(message?.kind || "message") + " · " + String(message?.status || "unknown") + " · " + String(message?.priority || "normal");
    const time = document.createElement("span"); time.className = "muted small"; time.textContent = updatedLabel(message?.created_at);
    head.appendChild(kind); head.appendChild(time); card.appendChild(head);
    const meta = document.createElement("div"); meta.className = "message-meta";
    const metaParts = [id]; if (message?.author_session_id) metaParts.push("author " + String(message.author_session_id));
    meta.textContent = metaParts.join(" · "); card.appendChild(meta);
    if (parentUnavailable) { const unavailable = document.createElement("div"); unavailable.className = "message-links"; unavailable.textContent = "retained reply · parent unavailable"; card.appendChild(unavailable); }
    else if (message?.reply_to) { const reply = document.createElement("div"); reply.className = "message-links"; reply.textContent = "reply to " + String(message.reply_to); card.appendChild(reply); }
    const body = document.createElement("div"); body.className = "message-body"; body.textContent = String(message?.message || ""); card.appendChild(body);
    if (message?.resolved_at || message?.resolution || message?.resolved_by_message_id) {
      const resolution = document.createElement("div"); resolution.className = "message-resolution";
      const parts: string[] = []; if (message.resolved_at) parts.push("resolved " + updatedLabel(message.resolved_at)); if (message.resolution) parts.push(String(message.resolution)); if (message.resolved_by_message_id) parts.push("by " + String(message.resolved_by_message_id));
      resolution.textContent = parts.join(" · "); card.appendChild(resolution);
    }
    node.appendChild(card);
    for (const child of children.get(id) || []) appendMessage(child, depth + 1, false);
  };
  for (const message of messages) {
    const parent = typeof message?.reply_to === "string" ? message.reply_to : "";
    if (!parent || !byId.has(parent)) appendMessage(message, 0, !!parent);
  }
  for (const message of messages) appendMessage(message, 0, false);
}

async function loadRetainedCollaboration(request: any, controller: AbortController): Promise<string | null> {
  const response = await api("workflow-session-messages", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!response || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (response.status === 401) { lock("Credential rejected."); return null; }
  if (response.status === 403) {
    setRuntimeCollaborationAvailable(state, request, false); renderCollaboration(); return null;
  }
  if (response.status === 404) { setRuntimeCollaborationAvailable(state, request, false); renderCollaboration("Session collaboration unavailable"); return null; }
  if (!response.ok || !response.data) { renderCollaboration("Collaboration refresh failed"); return null; }
  setRuntimeCollaborationAvailable(state, request, true);
  if (!adoptRuntimeCollaborationList(state, request, Array.isArray(response.data.messages) ? response.data.messages : [])) return null;
  renderCollaboration("Establishing live baseline…");
  const baseline = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!baseline || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (baseline.status === 401) { lock("Credential rejected."); return null; }
  if (baseline.status === 403) { setRuntimeCollaborationAvailable(state, request, false); renderCollaboration(); return null; }
  if (!baseline.ok || !baseline.data || typeof baseline.data.observation_token !== "string") { renderCollaboration("Live observation unavailable"); return null; }
  adoptRuntimeCollaborationObservation(state, request, baseline.data);
  renderCollaboration("Live · bounded long-poll");
  return baseline.data.observation_token;
}

async function startCollaboration(request: any): Promise<void> {
  abortCollaboration();
  const controller = new AbortController(); collaborationAbort = controller;
  let observationToken = await loadRetainedCollaboration(request, controller);
  while (observationToken && collaborationAbort === controller && isCurrentRuntimeCollaborationRequest(state, request)) {
    const response = await api("workflow-session-observe", {
      project: request.project,
      session_id: request.sessionId,
      after_observation_token: observationToken,
      wait_secs: COLLABORATION_WAIT_SECS,
      limit: 100,
    }, controller.signal);
    if (!response || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) break;
    if (response.status === 401) { lock("Credential rejected."); break; }
    if (response.status === 403) { setRuntimeCollaborationAvailable(state, request, false); renderCollaboration(); break; }
    if (!response.ok || !response.data) { renderCollaboration("Live refresh paused after request failure"); break; }
    const action = runtimeCollaborationObservationAction(response.data);
    if (action === "reload") {
      renderCollaboration("Retention changed · reloading retained board…");
      observationToken = await loadRetainedCollaboration(request, controller);
      continue;
    }
    if (!adoptRuntimeCollaborationObservation(state, request, response.data)) break;
    observationToken = String(response.data.observation_token || observationToken);
    renderCollaboration(action === "drain" ? "Live · draining retained changes…" : "Live · bounded long-poll");
    if (action === "drain") {
      const drain = await api("workflow-session-observe", {
        project: request.project,
        session_id: request.sessionId,
        after_observation_token: observationToken,
        limit: 100,
      }, controller.signal);
      if (!drain || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) break;
      if (!drain.ok || !drain.data) { renderCollaboration("Delta drain failed"); break; }
      if (runtimeCollaborationObservationAction(drain.data) === "reload") {
        observationToken = await loadRetainedCollaboration(request, controller);
        continue;
      }
      adoptRuntimeCollaborationObservation(state, request, drain.data);
      observationToken = String(drain.data.observation_token || observationToken);
      renderCollaboration(drain.data.has_more ? "Live · draining retained changes…" : "Live · bounded long-poll");
      while (drain.data.has_more) {
        const more = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, after_observation_token: observationToken, limit: 100 }, controller.signal);
        if (!more || !more.ok || !more.data || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) return;
        if (more.data.history_lost) { observationToken = await loadRetainedCollaboration(request, controller); break; }
        adoptRuntimeCollaborationObservation(state, request, more.data);
        observationToken = String(more.data.observation_token || observationToken);
        drain.data = more.data;
        renderCollaboration(more.data.has_more ? "Live · draining retained changes…" : "Live · bounded long-poll");
      }
    }
  }
  if (collaborationAbort === controller) collaborationAbort = null;
}

function jumpLatest(): void {
  jumpWorkflowSessionToLatest(state.workflow);
  const node = el("runtime-timeline"); if (node) node.scrollTop = node.scrollHeight; syncFollowUi();
}

async function refreshAll(): Promise<void> {
  if (!token) return;
  void fetchOverview(refreshRuntimeOverview(state));
  await fetchProjects(refreshRuntimeProjects(state));
}

function startAuto(): void {
  stopAuto();
  timer = window.setInterval(() => {
    const request = refreshRuntimeSessionList(state); if (request) void fetchSessions(request);
  }, REFRESH_MS);
}
function stopAuto(): void { if (timer) window.clearInterval(timer); timer = 0; }

el("runtime-token-form")?.addEventListener("submit", (event) => {
  event.preventDefault();
  const input = el("runtime-token-input") as HTMLInputElement | null;
  const nextToken = input ? input.value.trim() : ""; if (input) input.value = "";
  if (!nextToken) { setText("runtime-token-error", "Enter a runtime Bearer credential."); return; }
  token = nextToken; const request = beginRuntimeCredential(state); void fetchProjects(request, true);
});

el("runtime-device-select")?.addEventListener("change", () => {
  const select = el("runtime-device-select") as HTMLSelectElement | null; if (!select) return;
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
  const node = el("runtime-timeline"); if (!node) return;
  updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight); syncFollowUi();
});
window.addEventListener("pagehide", () => { token = ""; abortAll(); stopAuto(); });

lock();
