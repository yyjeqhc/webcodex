import {
  workflowSessionListOverviewFacts,
  workflowSessionOverviewPresentation,
  workflowSessionLivenessPresentation,
  updateWorkflowSessionFollowFromScroll,
  workflowSessionScrollTopAfterRender,
  jumpWorkflowSessionToLatest,
  shouldFollowWorkflowSessionLatest,
} from "./workflow_session_state.js";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  filterAndSortRuntimeProjects,
  preferredRuntimeProjectSelection,
  invalidateRuntimeCredential,
  beginRuntimeCredential,
  refreshRuntimeOverview,
  isCurrentRuntimeOverviewRequest,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  selectRuntimeRunnerFilter,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  selectRuntimeWorkflowSession,
  selectRuntimeSessionLocation,
  refreshRuntimeWorkflowSession,
  clearRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
  adoptRuntimeCollaborationObservation,
  setRuntimeCollaborationAvailable,
  setRuntimeCollaborationPhase,
  runtimeCollaborationNeedsRefreshRecovery,
  runtimeCollaborationObservationAction,
} from "./runtime_console_state.js";

const API_BASE = "/api/runtime-console/";
const REFRESH_MS = 8000;
const COLLABORATION_WAIT_SECS = 25;

let token = "";
let timer = 0;
let overviewAbort: AbortController | null = null;
let projectsAbort: AbortController | null = null;
let sessionsAbort: AbortController | null = null;
let detailAbort: AbortController | null = null;
let collaborationAbort: AbortController | null = null;
let projectRows: any[] = [];
let homeProjectRows: any[] = [];
let runnerRows: any[] = [];
let recentSessionRows: any[] = [];
let projectSearch = "";
let collaborationReplyTo = "";
let refreshInFlight = false;
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

function appendChip(parent: HTMLElement, text: string, extraClass = ""): HTMLElement {
  const chip = document.createElement("span");
  chip.className = "chip" + (extraClass ? " " + extraClass : "");
  chip.textContent = text;
  parent.appendChild(chip);
  return chip;
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
  overviewAbort = null;
  projectsAbort = null;
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
  homeProjectRows = [];
  runnerRows = [];
  recentSessionRows = [];
  projectRowsTruncated = false;
  projectSearch = "";
  collaborationReplyTo = "";
  clearSessionSurface();
  clearNode(el("runtime-project-list"));
  clearNode(el("runtime-recent-session-list"));
  clearNode(el("runtime-runner-list"));
  show("runtime-token-gate", true);
  show("runtime-console", false);
  show("runtime-topbar-controls", false);
  stopAuto();
  setText("runtime-token-error", message);
  setText("runtime-refresh-status", "");
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

async function fetchOverview(request: any): Promise<boolean> {
  abort(overviewAbort);
  const controller = new AbortController();
  overviewAbort = controller;
  const response = await api("overview", {}, controller.signal);
  if (overviewAbort === controller) overviewAbort = null;
  if (!response || !isCurrentRuntimeOverviewRequest(state, request)) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    homeProjectRows = [];
    runnerRows = [];
    recentSessionRows = [];
    show("runtime-overview-unavailable", true);
    show("runtime-runner-unavailable", true);
    show("runtime-recent-unavailable", true);
    setText("runtime-overview-access", "runtime:read unavailable");
    setText("runtime-runner-access", "runtime:read unavailable");
    setText("runtime-recent-status", "runtime:read unavailable");
    renderRunnerFleet([]);
    renderRecentSessions([], null);
    renderProjectSelectors(projectRows, projectRowsTruncated);
    return true;
  }
  if (!response.ok || !response.data) {
    setText("runtime-overview-access", "refresh unavailable");
    setText("runtime-runner-access", "refresh unavailable");
    setText("runtime-recent-status", "refresh unavailable");
    return false;
  }
  show("runtime-overview-unavailable", false);
  show("runtime-runner-unavailable", false);
  show("runtime-recent-unavailable", false);
  setText("runtime-overview-access", "runtime:read");
  setText("runtime-runner-access", "runtime:read");
  const data = response.data;
  homeProjectRows = Array.isArray(data.projects) ? data.projects : [];
  runnerRows = Array.isArray(data.runners) ? data.runners : [];
  recentSessionRows = Array.isArray(data.recent_sessions?.sessions) ? data.recent_sessions.sessions : [];
  setText("runtime-server-identity", [data.service, data.version].filter(Boolean).join(" · "));
  setText("runtime-server-build", data.build_git_commit ? "build " + data.build_git_commit + (data.build_git_dirty ? " · dirty" : "") : "build unavailable");
  setText("runtime-server-runners", countLabel(data.runner_count, "Runner"));
  setText("runtime-server-alignment", countLabel(data.runners_online, "online") + " · " + countLabel(data.runners_stale, "stale") + " · " + countLabel(data.runners_unavailable, "unavailable"));
  setText("runtime-server-projects", data.projects_available ? countLabel(data.visible_projects, "visible Project") + (data.projects_truncated ? " · partial" : "") : "project:read unavailable");
  setText("runtime-server-jobs", countLabel(data.active_jobs, "active Job") + (data.mixed_builds_present ? " · mixed builds" : ""));
  setText("runtime-server-attention", attentionLabel(data.workflow_sessions));
  setText("runtime-server-sessions", countLabel(data.workflow_sessions?.active, "active Session") + " · " + countLabel(data.workflow_sessions?.running, "running Session") + (data.workflow_sessions?.truncated ? " · bounded aggregate" : ""));
  const recentMeta = data.recent_sessions || {};
  setText(
    "runtime-recent-status",
    countLabel(recentMeta.returned, "Session") +
      (recentMeta.truncated ? " · top " + String(recentMeta.returned || 0) : "") +
      (recentMeta.scan_truncated ? " · partial scan" : "")
  );
  renderRecentSessions(recentSessionRows, recentMeta);
  renderRunnerFleet(runnerRows);
  renderProjectSelectors(projectRows, projectRowsTruncated || !!data.projects_truncated);
  return true;
}

function projectLabel(project: any): string {
  const name = project && project.name ? String(project.name) : "";
  const id = project && project.id ? String(project.id) : "";
  const identity = name && name !== id ? name + " — " + id : id;
  const status = project && project.connected ? String(project.agent_status || "online") : "offline";
  return identity + " · " + status;
}

async function fetchProjects(request: any, unlocking = false): Promise<boolean> {
  abort(projectsAbort);
  const controller = new AbortController();
  projectsAbort = controller;
  const response = await api("projects", { limit: 100 }, controller.signal);
  if (projectsAbort === controller) projectsAbort = null;
  if (!response || !isCurrentRuntimeProjectsRequest(state, request)) return false;
  if (response.status === 401 || response.status === 403) {
    lock("Credential does not have Runtime Console project access.");
    return false;
  }
  if (!response.ok || !response.data) {
    if (unlocking) lock("Runtime Console is unavailable.");
    else showError("Could not refresh projects.");
    return false;
  }
  projectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
  projectRowsTruncated = !!response.data.truncated;
  unlockUi();
  showError("");

  const currentDevice = String(state.selectedDevice || "");
  const currentProject = String(state.selectedProject || "");
  const selection = preferredRuntimeProjectSelection(projectRows, currentDevice, currentProject);
  if (!selection.project) {
    if (currentProject || selection.device !== currentDevice) {
      abortProjectWork();
      selectRuntimeRunnerFilter(state, selection.device || "");
      collaborationReplyTo = "";
      clearSessionSurface();
    }
    renderProjectSelectors(projectRows, projectRowsTruncated);
    setText("runtime-selected-project", "No project selected");
    return true;
  }
  if (selection.device !== currentDevice || selection.project !== currentProject) {
    switchProject(selection.device, selection.project);
  } else {
    renderProjectSelectors(projectRows, projectRowsTruncated);
    const listRequest = refreshRuntimeSessionList(state);
    if (listRequest) void fetchSessions(listRequest);
  }
  return true;
}

function effectiveProjects(projects: any[]): any[] {
  const aggregates = new Map<string, any>();
  for (const row of homeProjectRows) {
    if (row && typeof row.id === "string") aggregates.set(row.id, row);
  }
  return (Array.isArray(projects) ? projects : []).map((project) => {
    const aggregate = aggregates.get(String(project?.id || ""));
    return aggregate ? { ...project, sessions: aggregate.sessions } : project;
  });
}

function renderProjectSelectors(projects: any[], truncated: boolean): void {
  const deviceSelect = el("runtime-device-select") as HTMLSelectElement | null;
  const projectList = el("runtime-project-list");
  if (!deviceSelect || !projectList) return;
  const devices = runtimeDeviceIds(projects);
  clearNode(deviceSelect);
  const all = document.createElement("option");
  all.value = "";
  all.textContent = "All Runners";
  deviceSelect.appendChild(all);
  for (const clientId of devices) {
    const option = document.createElement("option");
    option.value = clientId;
    option.textContent = clientId;
    deviceSelect.appendChild(option);
  }
  deviceSelect.value = String(state.selectedDevice || "");
  const effective = effectiveProjects(projects);
  const rows = filterAndSortRuntimeProjects(
    effective,
    String(state.selectedDevice || ""),
    projectSearch,
  );
  clearNode(projectList);
  show("runtime-projects-empty", rows.length === 0);
  for (const project of rows) {
    const row = document.createElement("div");
    row.className = "project-row" + (project.id === state.selectedProject ? " selected" : "");
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", project.id === state.selectedProject ? "true" : "false");
    row.tabIndex = 0;
    const main = document.createElement("div"); main.className = "project-row-main";
    const title = document.createElement("div"); title.className = "project-row-title"; title.textContent = project.name || project.id;
    const id = document.createElement("div"); id.className = "project-row-id"; id.textContent = String(project.id || "");
    const runner = document.createElement("div"); runner.className = "project-row-runner muted small"; runner.textContent = "Runner " + String(project.client_id || "unknown") + " · " + (project.connected ? String(project.agent_status || "online") : "offline");
    main.appendChild(title); main.appendChild(id); main.appendChild(runner);
    const facts = document.createElement("div"); facts.className = "project-row-facts";
    if (!project.connected) appendChip(facts, "OFFLINE", "tone-fail");
    else if (project.agent_status && project.agent_status !== "online") appendChip(facts, String(project.agent_status).toUpperCase(), "tone-warn");
    if (project.sessions) {
      if (project.sessions.running_sessions) appendChip(facts, countLabel(project.sessions.running_sessions, "RUNNING"), "tone-runtime");
      const attention = attentionLabel(project.sessions.attention);
      if (!attention.startsWith("No retained")) appendChip(facts, attention, "tone-warn");
      const retained = document.createElement("span"); retained.className = "muted small"; retained.textContent = countLabel(project.sessions.retained_sessions, "retained Session"); facts.appendChild(retained);
      if (typeof project.sessions.latest_updated_at === "number") {
        const updated = document.createElement("span"); updated.className = "muted small"; updated.textContent = "updated " + updatedLabel(project.sessions.latest_updated_at); facts.appendChild(updated);
      }
    }
    row.appendChild(main); row.appendChild(facts);
    const select = (): void => switchProject(String(project.client_id || ""), String(project.id || ""));
    row.addEventListener("click", select);
    row.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(); } });
    projectList.appendChild(row);
  }
  const filteredProjects = runtimeProjectsForDevice(effective, String(state.selectedDevice || ""));
  setText(
    "runtime-device-status",
    devices.length
      ? countLabel(devices.length, "authorized Runner") + (state.selectedDevice ? " · filtered" : " · All Runners") + (truncated ? " · bounded project list" : "")
      : "No authorized Runners"
  );
  setText(
    "runtime-project-status",
    countLabel(filteredProjects.length, "visible Project") + (state.selectedDevice ? " on " + state.selectedDevice : " across fleet") + (truncated ? " · bounded list" : "")
  );
}

function switchProject(device: string, project: string): void {
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  const request = selectRuntimeProject(state, device, project);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  setText("runtime-selected-project", project || "No project selected");
  if (request) void fetchSessions(request);
}

function applyRunnerFilter(device: string): void {
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  selectRuntimeRunnerFilter(state, device);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  setText("runtime-selected-project", "No project selected");
}

function runnerAttentionCount(runner: any): number {
  const attention = runner?.sessions?.attention;
  return ["open_guidance", "open_questions", "open_risks", "open_todos"]
    .reduce((total, key) => total + (typeof attention?.[key] === "number" ? Math.max(0, attention[key]) : 0), 0);
}

function renderRunnerFleet(runners: any[]): void {
  const node = el("runtime-runner-list");
  if (!node) return;
  clearNode(node);
  show("runtime-runners-empty", runners.length === 0 && !!el("runtime-runner-unavailable")?.hidden);
  for (const runner of runners) {
    const clientId = String(runner?.client_id || "");
    if (!clientId) continue;
    const row = document.createElement("div");
    row.className = "fleet-row" + (clientId === state.selectedDevice ? " selected" : "");
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", clientId === state.selectedDevice ? "true" : "false");
    row.tabIndex = 0;
    const main = document.createElement("div"); main.className = "fleet-row-main";
    const title = document.createElement("div"); title.className = "fleet-row-title"; title.textContent = clientId;
    const meta = document.createElement("div"); meta.className = "muted small fleet-row-meta";
    const metaParts = [
      runner.connected ? String(runner.status || "online") : "offline",
      runner.version ? "v" + String(runner.version) : "version unavailable",
      runner.transport ? String(runner.transport) : "transport unavailable",
      runner.source_alignment ? "source " + String(runner.source_alignment) : "source alignment unavailable",
      typeof runner.last_seen_age_secs === "number" ? "seen " + String(runner.last_seen_age_secs) + "s ago" : "last seen unavailable",
    ];
    if (runner.build_git_commit) metaParts.push("build " + String(runner.build_git_commit));
    meta.textContent = metaParts.join(" · ");
    main.appendChild(title); main.appendChild(meta);

    const signals = document.createElement("div"); signals.className = "fleet-row-signals";
    const working = Math.max(Number(runner.jobs_running || 0), Number(runner.sessions?.running_sessions || 0));
    const attention = runnerAttentionCount(runner);
    if (working > 0) appendChip(signals, "RUNNING", "tone-runtime");
    if (attention > 0) appendChip(signals, "ATTENTION " + attention, "tone-warn");
    if (!runner.connected) appendChip(signals, "OFFLINE", "tone-fail");
    else if (String(runner.status || "") === "stale") appendChip(signals, "STALE", "tone-warn");
    if (runner.source_alignment === "different") appendChip(signals, "SOURCE DIFFERENT", "tone-fail");
    if (runner.version_matches_server === false) appendChip(signals, "BUILD DIFFERENT", "tone-warn");
    if (runner.build_git_dirty === true) appendChip(signals, "DIRTY", "tone-warn");

    const facts = document.createElement("div"); facts.className = "muted small fleet-row-facts";
    facts.textContent = [
      countLabel(runner.active_jobs, "active Job"),
      countLabel(runner.jobs_running, "running Job"),
      countLabel(runner.jobs_queued, "queued Job"),
      typeof runner.job_concurrency_limit === "number" ? "limit " + runner.job_concurrency_limit : "limit unavailable",
      countLabel(runner.visible_project_count, "visible Project"),
      countLabel(runner.sessions?.active_sessions, "active Session"),
    ].join(" · ") + (runner.projects_truncated ? " · project scan partial" : "");
    row.appendChild(main); row.appendChild(signals); row.appendChild(facts);
    const select = (): void => applyRunnerFilter(clientId);
    row.addEventListener("click", select);
    row.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(); } });
    node.appendChild(row);
  }
  setText("runtime-runner-count", countLabel(runners.length, "Runner"));
}

function renderRecentSessions(sessions: any[], meta: any): void {
  const node = el("runtime-recent-session-list");
  if (!node) return;
  clearNode(node);
  show("runtime-recent-empty", sessions.length === 0 && !!el("runtime-recent-unavailable")?.hidden);
  for (const session of sessions) {
    const sessionId = String(session?.session_id || "");
    const projectId = String(session?.project_id || "");
    const clientId = String(session?.client_id || "");
    if (!sessionId || !projectId || !clientId) continue;
    const selected = projectId === state.selectedProject && sessionId === state.workflow.selectedSessionId;
    const row = document.createElement("div");
    row.className = "recent-session-row" + (selected ? " selected" : "");
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", selected ? "true" : "false");
    row.tabIndex = 0;
    const main = document.createElement("div"); main.className = "recent-session-main";
    const title = document.createElement("div"); title.className = "session-title"; title.textContent = session.title ? String(session.title) : sessionId;
    const location = document.createElement("div"); location.className = "muted small recent-session-location";
    location.textContent = clientId + " · " + String(session.project_name || projectId) + (session.project_name && session.project_name !== projectId ? " · " + projectId : "");
    main.appendChild(title); main.appendChild(location);
    const signals = document.createElement("div"); signals.className = "recent-session-signals";
    const liveness = workflowSessionLivenessPresentation(session);
    if (liveness.state === "working") appendChip(signals, "RUNNING", "tone-runtime");
    const attention = attentionLabel(session.overview?.attention);
    if (!attention.startsWith("No retained")) appendChip(signals, attention, "tone-warn");
    const lifecycle = document.createElement("span"); lifecycle.className = "muted small"; lifecycle.textContent = [session.lifecycle, liveness.label, "updated " + updatedLabel(session.updated_at)].filter(Boolean).join(" · "); lifecycle.title = liveness.tooltip; signals.appendChild(lifecycle);
    row.appendChild(main); row.appendChild(signals);
    appendPreview(row, "Now", session.current_activity);
    appendPreview(row, "Last", session.last_activity);
    const select = (): void => selectRecentSession(session);
    row.addEventListener("click", select);
    row.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(); } });
    node.appendChild(row);
  }
  if (meta) {
    setText(
      "runtime-recent-status",
      countLabel(meta.returned, "Session") + (meta.truncated ? " · top " + String(meta.returned || 0) : "") + (meta.scan_truncated ? " · partial scan" : "")
    );
  }
}

function selectRecentSession(session: any): void {
  const clientId = String(session?.client_id || "");
  const projectId = String(session?.project_id || "");
  const sessionId = String(session?.session_id || "");
  if (!clientId || !projectId || !sessionId) return;
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  setHumanJoinSendEnabled(false);
  const location = selectRuntimeSessionLocation(state, clientId, projectId, sessionId);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  setText("runtime-selected-project", projectId);
  if (location.sessionListRequest) void fetchSessions(location.sessionListRequest);
  if (location.detailRequest) void fetchSessionDetail(location.detailRequest);
  const collaborationRequest = runtimeCollaborationRequest(state);
  if (collaborationRequest) void startCollaboration(collaborationRequest);
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
    item.setAttribute("role", "option"); item.setAttribute("aria-selected", id === selected ? "true" : "false"); item.tabIndex = 0;
    const title = document.createElement("div"); title.className = "session-title"; title.textContent = session.title ? String(session.title) : id;
    const meta = document.createElement("div"); meta.className = "chips";
    appendChip(meta, String(session.lifecycle || "unknown"));
    const liveness = workflowSessionLivenessPresentation(session);
    const livenessChip = appendChip(meta, liveness.label, liveness.state === "working" ? "tone-runtime" : liveness.state === "attention" ? "tone-warn" : "");
    livenessChip.title = liveness.tooltip;
    appendChip(meta, updatedLabel(session.updated_at));
    item.appendChild(title); item.appendChild(meta);
    const facts = workflowSessionListOverviewFacts(session.overview);
    if (facts.length) {
      const summary = document.createElement("div"); summary.className = "summary-facts";
      for (const fact of facts) appendChip(summary, fact.text, "tone-" + fact.tone);
      item.appendChild(summary);
    }
    appendPreview(item, "Now", session.current_activity); appendPreview(item, "Last", session.last_activity);
    const select = (): void => selectSession(id);
    item.addEventListener("click", select);
    item.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(); } });
    node.appendChild(item);
  }
}

function selectSession(sessionId: string): void {
  abort(detailAbort); detailAbort = null; abortCollaboration(); hideDetail();
  setHumanJoinSendEnabled(false);
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
  const liveness = workflowSessionLivenessPresentation(detail);
  setText("runtime-session-running", liveness.label);
  const livenessNode = el("runtime-session-running"); if (livenessNode) livenessNode.title = liveness.tooltip;
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

function collaborationPhaseLabel(): string {
  switch (state.collaboration.phase) {
    case "live": return "Live";
    case "reconnecting": return "Reconnecting";
    case "paused": return "Paused";
    default: return "Idle";
  }
}

function setCollaborationReplyTarget(messageId: string): void {
  collaborationReplyTo = messageId;
  const reply = el("runtime-message-reply");
  if (reply) reply.hidden = !messageId;
  setText("runtime-message-reply-text", messageId ? "Reply to " + messageId : "");
}

function renderCollaboration(statusText?: string): void {
  const available = state.collaboration.available !== false;
  show("runtime-collaboration-unavailable", !available);
  show("runtime-collaboration-form", available);
  const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
  show("runtime-collaboration-empty", available && messages.length === 0);
  const status = available
    ? "Collaboration: " + collaborationPhaseLabel() + " · " + countLabel(messages.length, "retained message") + (statusText ? " · " + statusText : "")
    : "runtime:read unavailable";
  setText("runtime-collaboration-status", status);
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
    const kind = document.createElement("span"); kind.className = "message-kind"; kind.textContent = String(message?.kind || "message") + " · " + String(message?.priority || "normal") + " · " + String(message?.status || "unknown");
    const time = document.createElement("span"); time.className = "muted small"; time.textContent = updatedLabel(message?.created_at);
    head.appendChild(kind); head.appendChild(time); card.appendChild(head);
    const meta = document.createElement("div"); meta.className = "message-meta";
    const metaParts = [id]; if (message?.author_session_id) metaParts.push("author " + String(message.author_session_id));
    meta.textContent = metaParts.join(" · "); card.appendChild(meta);
    if (parentUnavailable) { const unavailable = document.createElement("div"); unavailable.className = "message-links"; unavailable.textContent = "retained reply · parent unavailable"; card.appendChild(unavailable); }
    else if (message?.reply_to) { const reply = document.createElement("div"); reply.className = "message-links"; reply.textContent = "reply to " + String(message.reply_to); card.appendChild(reply); }
    const body = document.createElement("div"); body.className = "message-body"; body.textContent = String(message?.message || ""); card.appendChild(body);
    if (message?.requires_ack) {
      const ack = document.createElement("div"); ack.className = "message-ack";
      ack.textContent = typeof message?.first_ack_observed_at === "number"
        ? "ACK required · First ACK observed " + updatedLabel(message.first_ack_observed_at)
        : "ACK required";
      card.appendChild(ack);
    }
    if (message?.resolved_at || message?.resolution || message?.resolved_by_message_id) {
      const resolution = document.createElement("div"); resolution.className = "message-resolution";
      const parts: string[] = []; if (message.resolved_at) parts.push("resolved " + updatedLabel(message.resolved_at)); if (message.resolution) parts.push(String(message.resolution)); if (message.resolved_by_message_id) parts.push("by " + String(message.resolved_by_message_id));
      resolution.textContent = parts.join(" · "); card.appendChild(resolution);
    }
    const actions = document.createElement("div"); actions.className = "message-actions";
    const replyButton = document.createElement("button"); replyButton.type = "button"; replyButton.className = "text-button"; replyButton.textContent = "Reply";
    replyButton.addEventListener("click", () => setCollaborationReplyTarget(id));
    actions.appendChild(replyButton); card.appendChild(actions);
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
  // Establish the cursor before the retained snapshot. A mutation between these
  // two reads is then present in the snapshot, the subsequent delta, or both;
  // merge-by-id makes the overlap harmless. Listing first and baselining second
  // would permanently skip a mutation that lands in that gap.
  setRuntimeCollaborationPhase(state, request, "reconnecting");
  renderCollaboration("establishing retained baseline");
  const baseline = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!baseline || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (baseline.status === 401) { lock("Credential rejected."); return null; }
  if (baseline.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); return null; }
  if (baseline.status === 404) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("Session unavailable"); return null; }
  if (!baseline.ok || !baseline.data || typeof baseline.data.observation_token !== "string") { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("observation unavailable"); return null; }

  const response = await api("workflow-session-messages", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!response || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (response.status === 401) { lock("Credential rejected."); return null; }
  if (response.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); return null; }
  if (response.status === 404) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("Session unavailable"); return null; }
  if (!response.ok || !response.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("retained snapshot failed"); return null; }
  setRuntimeCollaborationAvailable(state, request, true);
  if (!adoptRuntimeCollaborationList(state, request, Array.isArray(response.data.messages) ? response.data.messages : [])) return null;
  adoptRuntimeCollaborationObservation(state, request, baseline.data);
  setRuntimeCollaborationPhase(state, request, "live");
  setHumanJoinSendEnabled(true);
  renderCollaboration("bounded long-poll");
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
    if (response.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); break; }
    if (!response.ok || !response.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("request failed"); break; }
    const action = runtimeCollaborationObservationAction(response.data);
    if (action === "reload") {
      renderCollaboration("retention changed · reloading");
      observationToken = await loadRetainedCollaboration(request, controller);
      continue;
    }
    if (!adoptRuntimeCollaborationObservation(state, request, response.data)) break;
    observationToken = String(response.data.observation_token || observationToken);
    setRuntimeCollaborationPhase(state, request, "live");
    renderCollaboration(action === "drain" ? "draining retained changes" : "bounded long-poll");
    if (action === "drain") {
      let draining = true;
      while (draining && observationToken && collaborationAbort === controller && isCurrentRuntimeCollaborationRequest(state, request)) {
        const drain = await api("workflow-session-observe", {
          project: request.project,
          session_id: request.sessionId,
          after_observation_token: observationToken,
          limit: 100,
        }, controller.signal);
        if (!drain || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) break;
        if (!drain.ok || !drain.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("delta drain failed"); observationToken = null; break; }
        if (runtimeCollaborationObservationAction(drain.data) === "reload") {
          observationToken = await loadRetainedCollaboration(request, controller);
          draining = false;
          continue;
        }
        adoptRuntimeCollaborationObservation(state, request, drain.data);
        observationToken = String(drain.data.observation_token || observationToken);
        draining = !!drain.data.has_more;
        setRuntimeCollaborationPhase(state, request, "live");
        renderCollaboration(draining ? "draining retained changes" : "bounded long-poll");
      }
    }
  }
  if (collaborationAbort === controller) collaborationAbort = null;
}

function jumpLatest(): void {
  jumpWorkflowSessionToLatest(state.workflow);
  const node = el("runtime-timeline"); if (node) node.scrollTop = node.scrollHeight; syncFollowUi();
}

function setHumanJoinSendEnabled(enabled: boolean): void {
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  if (send) send.disabled = !enabled;
}

function syncAckComposer(): void {
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const guidance = kind?.value === "guidance";
  show("runtime-message-ack-label", guidance);
  if (!checkbox) return;
  checkbox.disabled = !guidance || priority?.value !== "high";
  if (checkbox.disabled) checkbox.checked = false;
  checkbox.title = guidance && priority?.value !== "high" ? "ACK requirement is available for High priority guidance." : "";
}

async function postHumanCollaborationMessage(event: Event): Promise<void> {
  event.preventDefault();
  const request = runtimeCollaborationRequest(state);
  if (!request || state.collaboration.available === false) return;
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  const message = body?.value.trim() || "";
  if (!message) { setText("runtime-message-send-status", "Enter a message."); return; }
  if (send) send.disabled = true;
  setText("runtime-message-send-status", "Sending…");
  const response = await api("workflow-session-post-message", {
    project: request.project,
    session_id: request.sessionId,
    kind: kind?.value || "note",
    priority: priority?.value || "normal",
    message,
    reply_to: collaborationReplyTo || null,
    requires_ack: !!checkbox?.checked,
  });
  if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
  if (response?.status === 0) {
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    setText("runtime-message-send-status", "Send outcome unknown. Refresh and review retained messages before retrying.");
    renderCollaboration("send outcome unknown · refresh before retry");
    return;
  }
  if (send) send.disabled = false;
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (!response?.ok || !response.data) { setText("runtime-message-send-status", "Send failed."); return; }
  adoptRuntimeCollaborationObservation(state, request, { messages: [response.data] });
  if (body) body.value = "";
  setCollaborationReplyTarget("");
  setText("runtime-message-send-status", "Sent.");
  renderCollaboration();
}

function setRefreshBusy(active: boolean): void {
  refreshInFlight = active;
  const button = el("runtime-refresh") as HTMLButtonElement | null;
  if (button) {
    button.disabled = active;
    button.textContent = active ? "Refreshing…" : "Refresh";
  }
}

async function refreshAll(): Promise<void> {
  if (!token || refreshInFlight) return;
  setRefreshBusy(true);
  setText("runtime-refresh-status", "Refreshing…");
  const recoverCollaboration = runtimeCollaborationNeedsRefreshRecovery(state);
  const overviewRequest = refreshRuntimeOverview(state);
  const projectsRequest = refreshRuntimeProjects(state);
  try {
    const [overviewOk, projectsOk] = await Promise.all([
      fetchOverview(overviewRequest),
      fetchProjects(projectsRequest),
    ]);
    if (!token) return;
    if (overviewOk && projectsOk) {
      setText("runtime-refresh-status", "Refreshed " + new Date().toLocaleTimeString());
    } else {
      setText("runtime-refresh-status", "Refresh failed · showing previous data");
    }
    if (recoverCollaboration && runtimeCollaborationNeedsRefreshRecovery(state)) {
      const collaborationRequest = runtimeCollaborationRequest(state);
      if (collaborationRequest) void startCollaboration(collaborationRequest);
    }
  } finally {
    setRefreshBusy(false);
  }
}

function startAuto(): void {
  stopAuto();
  timer = window.setInterval(() => {
    if (!token) return;
    void fetchOverview(refreshRuntimeOverview(state));
    const request = refreshRuntimeSessionList(state); if (request) void fetchSessions(request);
  }, REFRESH_MS);
}
function stopAuto(): void { if (timer) window.clearInterval(timer); timer = 0; }

el("runtime-token-form")?.addEventListener("submit", (event) => {
  event.preventDefault();
  const input = el("runtime-token-input") as HTMLInputElement | null;
  const nextToken = input ? input.value.trim() : ""; if (input) input.value = "";
  if (!nextToken) { setText("runtime-token-error", "Enter a runtime Bearer credential."); return; }
  token = nextToken;
  const request = beginRuntimeCredential(state);
  void fetchOverview(refreshRuntimeOverview(state));
  void fetchProjects(request, true);
});

el("runtime-device-select")?.addEventListener("change", () => {
  const select = el("runtime-device-select") as HTMLSelectElement | null; if (!select) return;
  applyRunnerFilter(select.value);
});
el("runtime-project-search")?.addEventListener("input", () => {
  const input = el("runtime-project-search") as HTMLInputElement | null;
  projectSearch = input?.value || "";
  renderProjectSelectors(projectRows, projectRowsTruncated);
});
el("runtime-message-kind")?.addEventListener("change", syncAckComposer);
el("runtime-message-priority")?.addEventListener("change", syncAckComposer);
el("runtime-message-reply-clear")?.addEventListener("click", () => setCollaborationReplyTarget(""));
el("runtime-collaboration-form")?.addEventListener("submit", (event) => void postHumanCollaborationMessage(event));
el("runtime-refresh")?.addEventListener("click", () => void refreshAll());
el("runtime-lock")?.addEventListener("click", () => lock());
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
  const node = el("runtime-timeline"); if (!node) return;
  updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight); syncFollowUi();
});
syncAckComposer();
window.addEventListener("pagehide", () => { token = ""; abortAll(); stopAuto(); });

lock();
