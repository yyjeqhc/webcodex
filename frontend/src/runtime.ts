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
  runtimeProjectIdentityText,
  preferredRuntimeProjectSelection,
  runtimeCommunicationTranscriptAfterSeq,
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
  runtimeCollaborationMessageCanMutate,
  setRuntimeCollaborationReplyTarget,
  setRuntimeCollaborationEditTarget,
  clearRuntimeCollaborationEditTarget,
  runtimeCollaborationEditTarget,
  markRuntimeCollaborationMutationUncertain,
  runtimeCollaborationMutationRecovery,
  completeRuntimeCollaborationMutationRecovery,
  takeRuntimeCollaborationMutationNotice,
} from "./runtime_console_state.js";

const API_BASE = "/api/runtime-console/";
const REFRESH_MS = 8000;
const COLLABORATION_WAIT_SECS = 25;
const PROJECT_SEARCH_DEBOUNCE_MS = 200;

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
let projectSearchTimer = 0;
let collaborationReplyTo = "";
let refreshInFlight = false;
let projectRowsTotal = 0;
let projectRowsTruncated = false;
let knownProjectDevices: string[] = [];
let selectedProjectSnapshot: any | null = null;
let sessionRows: any[] = [];
const state = initialRuntimeConsoleState();

let communicationAgents: any[] = [];
let communicationConversations: any[] = [];
let communicationDetail: any | null = null;
let communicationInbox: any[] = [];
let selectedCommunicationAgentId = "";
let selectedCommunicationConversationId = "";
let communicationReadAvailable: boolean | null = null;
let communicationManageAvailable: boolean | null = null;
let communicationRefreshInFlight = false;
let communicationGeneration = 0;
const communicationEndpoints = new Map<string, string>();
const pendingEndpointAttach = new Map<string, { key: string; attachmentId: string }>();
let pendingAgentCreate: { fingerprint: string; key: string } | null = null;
let pendingConversationCreate: { fingerprint: string; key: string } | null = null;
let pendingConversationMessage: { fingerprint: string; key: string } | null = null;
const pageAttachmentId = "runtime-console-" + operationKey("page");

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

function stopProjectSearchTimer(): void {
  if (projectSearchTimer) window.clearTimeout(projectSearchTimer);
  projectSearchTimer = 0;
}

function abortAll(): void {
  abort(overviewAbort);
  abort(projectsAbort);
  overviewAbort = null;
  projectsAbort = null;
  stopProjectSearchTimer();
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
  setText("runtime-session-workspace", "");
  clearNode(el("runtime-collaboration-board"));
}

function clearSessionSurface(): void {
  sessionRows = [];
  clearNode(el("runtime-session-list"));
  show("runtime-sessions-empty", false);
  clearRuntimeWorkflowSession(state);
  abortCollaboration();
  hideDetail();
  resetCollaborationComposerUi();
}

function lock(message = ""): void {
  detachCommunicationEndpointsBestEffort();
  token = "";
  abortAll();
  invalidateRuntimeCredential(state);
  projectRows = [];
  homeProjectRows = [];
  runnerRows = [];
  recentSessionRows = [];
  projectRowsTotal = 0;
  projectRowsTruncated = false;
  knownProjectDevices = [];
  selectedProjectSnapshot = null;
  projectSearch = "";
  collaborationReplyTo = "";
  clearSessionSurface();
  resetCommunicationSurface();
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
  const search = el("runtime-project-search") as HTMLInputElement | null;
  if (search) search.value = "";
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
  renderProjectSelectors(projectRows, projectRowsTruncated);
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
  const priorSelectedProject = selectedProjectRow();
  abort(projectsAbort);
  const controller = new AbortController();
  projectsAbort = controller;
  const payload: any = { limit: 100 };
  const clientId = String(request?.clientId || "");
  const query = String(request?.query || "").trim();
  if (clientId) payload.client_id = clientId;
  if (query) payload.query = query;
  const response = await api("projects", payload, controller.signal);
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
  const reportedTotal = typeof response.data.total === "number" && Number.isFinite(response.data.total)
    ? Math.max(0, Math.floor(response.data.total))
    : projectRows.length;
  projectRowsTotal = Math.max(projectRows.length, reportedTotal);
  projectRowsTruncated = !!response.data.truncated;
  const known = new Set(knownProjectDevices);
  for (const device of runtimeDeviceIds(projectRows)) known.add(device);
  knownProjectDevices = Array.from(known).sort((left, right) => left.localeCompare(right));
  if (priorSelectedProject && String(priorSelectedProject.id || "") === String(state.selectedProject || "")) {
    selectedProjectSnapshot = priorSelectedProject;
  }
  const refreshedSelected = effectiveProjects(projectRows).find(
    (project) => String(project?.id || "") === String(state.selectedProject || "")
  );
  if (refreshedSelected) selectedProjectSnapshot = refreshedSelected;
  unlockUi();
  showError("");

  const currentDevice = String(state.selectedDevice || "");
  const currentProject = String(state.selectedProject || "");
  if (query) {
    renderProjectSelectors(projectRows, projectRowsTruncated);
    renderSelectedProjectIdentity();
    return true;
  }
  const selection = preferredRuntimeProjectSelection(projectRows, currentDevice, currentProject);
  if (!selection.project) {
    if (currentProject && projectRowsTruncated) {
      renderProjectSelectors(projectRows, projectRowsTruncated);
      renderSelectedProjectIdentity();
      return true;
    }
    if (currentProject || selection.device !== currentDevice) {
      abortProjectWork();
      selectRuntimeRunnerFilter(state, selection.device || "");
      selectedProjectSnapshot = null;
      collaborationReplyTo = "";
      clearSessionSurface();
    }
    renderProjectSelectors(projectRows, projectRowsTruncated);
    renderSelectedProjectIdentity();
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

function projectSelectorDevices(projects: any[]): string[] {
  const devices = new Set(knownProjectDevices);
  for (const device of runtimeDeviceIds(projects)) devices.add(device);
  for (const runner of runnerRows) {
    const clientId = typeof runner?.client_id === "string" ? runner.client_id : "";
    if (clientId) devices.add(clientId);
  }
  const selectedDevice = String(state.selectedDevice || "");
  if (selectedDevice) devices.add(selectedDevice);
  return Array.from(devices).sort((left, right) => left.localeCompare(right));
}

function selectedProjectRow(): any | null {
  const selected = String(state.selectedProject || "");
  if (!selected) return null;
  const current = effectiveProjects(projectRows).find((project) => String(project?.id || "") === selected);
  if (current) return current;
  return selectedProjectSnapshot && String(selectedProjectSnapshot.id || "") === selected
    ? selectedProjectSnapshot
    : null;
}

function renderSelectedProjectIdentity(): void {
  setText("runtime-selected-project", runtimeProjectIdentityText(selectedProjectRow()));
}

function renderSessionWorkspaceIdentity(): void {
  setText("runtime-session-workspace", runtimeProjectIdentityText(selectedProjectRow()));
}

function revealWorkflowSessionDetail(): void {
  el("runtime-workflow-sessions-panel")?.scrollIntoView({ block: "start", inline: "nearest" });
}

function renderProjectSelectors(projects: any[], truncated: boolean): void {
  const deviceSelect = el("runtime-device-select") as HTMLSelectElement | null;
  const projectList = el("runtime-project-list");
  if (!deviceSelect || !projectList) return;
  const devices = projectSelectorDevices(projects);
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
    "",
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
    if (project.path) {
      const path = document.createElement("div"); path.className = "project-row-path workspace-path muted small"; path.textContent = String(project.path); main.appendChild(path);
    }
    const facts = document.createElement("div"); facts.className = "project-row-facts";
    if (!project.connected) appendChip(facts, "OFFLINE", "tone-fail");
    else if (project.agent_status && project.agent_status !== "online") appendChip(facts, String(project.agent_status).toUpperCase(), "tone-warn");
    if (project.sessions) {
      if (project.sessions.running_sessions) appendChip(facts, countLabel(project.sessions.running_sessions, "RUNNING"), "tone-runtime");
      const attention = attentionLabel(project.sessions.attention);
      if (!attention.startsWith("No retained")) appendChip(facts, attention, "tone-warn");
      if (project.sessions.sessions_truncated) appendChip(facts, "SESSION SCAN PARTIAL", "tone-warn");
      const retained = document.createElement("span"); retained.className = "muted small";
      retained.textContent = project.sessions.sessions_truncated
        ? String(project.sessions.returned_sessions || 0) + " / " + String(project.sessions.retained_sessions || 0) + " Sessions projected · partial"
        : countLabel(project.sessions.retained_sessions, "retained Session");
      facts.appendChild(retained);
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
  const returnedProjects = runtimeProjectsForDevice(effective, String(state.selectedDevice || "")).length;
  const totalProjects = Math.max(returnedProjects, projectRowsTotal);
  const scope = state.selectedDevice ? " on " + state.selectedDevice : " across fleet";
  const queryActive = !!projectSearch.trim();
  setText(
    "runtime-device-status",
    devices.length
      ? countLabel(devices.length, "authorized Runner") + (state.selectedDevice ? " · filtered" : " · All Runners")
      : "No authorized Runners"
  );
  setText(
    "runtime-project-status",
    truncated
      ? String(returnedProjects) + " of " + String(totalProjects) + (queryActive ? " matching Projects shown" : " visible Projects shown") + scope + " · bounded"
      : countLabel(totalProjects, queryActive ? "matching Project" : "visible Project") + scope
  );
  renderSelectedProjectIdentity();
}

function switchProject(device: string, project: string): void {
  const snapshot = effectiveProjects(projectRows).find((row) => String(row?.id || "") === project);
  selectedProjectSnapshot = snapshot || null;
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  const request = selectRuntimeProject(state, device, project);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  if (request) void fetchSessions(request);
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch));
}

function applyRunnerFilter(device: string): void {
  stopProjectSearchTimer();
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  selectedProjectSnapshot = null;
  selectRuntimeRunnerFilter(state, device);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch));
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
    const projectFact = runner.projects_scan_partial
      ? String(runner.projects_scanned || 0) + " Projects scanned"
      : countLabel(runner.projects_scanned, "visible Project");
    const factParts = [
      countLabel(runner.active_jobs, "active Job"),
      countLabel(runner.jobs_running, "running Job"),
      countLabel(runner.jobs_queued, "queued Job"),
      typeof runner.job_concurrency_limit === "number" ? "limit " + runner.job_concurrency_limit : "limit unavailable",
      projectFact,
      countLabel(runner.sessions?.active_sessions, "active Session"),
    ];
    if (runner.projects_scan_partial) factParts.push("fleet scan partial");
    if (runner.sessions?.sessions_truncated) factParts.push("Session scan partial");
    facts.textContent = factParts.join(" · ");
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
  const knownProject = effectiveProjects(projectRows).find((row) => String(row?.id || "") === projectId)
    || homeProjectRows.find((row) => String(row?.id || "") === projectId);
  selectedProjectSnapshot = knownProject || {
    id: projectId,
    client_id: clientId,
    name: typeof session?.project_name === "string" ? session.project_name : undefined,
  };
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  setHumanJoinSendEnabled(false);
  const location = selectRuntimeSessionLocation(state, clientId, projectId, sessionId);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  revealWorkflowSessionDetail();
  if (location.sessionListRequest) void fetchSessions(location.sessionListRequest);
  if (location.detailRequest) void fetchSessionDetail(location.detailRequest);
  const collaborationRequest = runtimeCollaborationRequest(state);
  if (collaborationRequest) void startCollaboration(collaborationRequest);
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch));
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

function dateTimeLabel(timestamp: any): string {
  if (typeof timestamp !== "number") return "time unavailable";
  return new Date(timestamp * 1000).toLocaleString();
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
  resetCollaborationComposerUi();
  renderSessionList(sessionRows, { total: sessionRows.length, truncated: false });
  revealWorkflowSessionDetail();
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
  if (response.status === 404) { abortCollaboration(); clearRuntimeWorkflowSession(state); hideDetail(); resetCollaborationComposerUi(); return; }
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
  setText("runtime-session-id", String(detail.session_id || "session id unavailable"));
  setText("runtime-session-created", dateTimeLabel(detail.created_at));
  setText("runtime-session-updated", dateTimeLabel(detail.updated_at));
  renderSessionWorkspaceIdentity();
  renderOverview(detail.overview);
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

function syncCollaborationComposer(): void {
  const edit = runtimeCollaborationEditTarget(state);
  const replyTargetId = String(state.collaboration.replyTargetId || "");
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  show("runtime-message-reply", !!replyTargetId && !edit);
  setText("runtime-message-reply-text", replyTargetId ? "Reply to " + replyTargetId : "");
  show("runtime-message-edit", !!edit);
  setText("runtime-message-edit-text", edit ? "Editing " + String(edit.message_id) : "");
  if (kind) {
    kind.disabled = !!edit;
    if (edit) kind.value = String(edit.kind || "note");
  }
  if (priority) {
    priority.disabled = !!edit;
    if (edit) priority.value = String(edit.priority || "normal");
  }
  if (checkbox && edit) checkbox.checked = !!edit.requires_ack;
  if (send) send.textContent = edit ? "Replace" : "Send";
  syncAckComposer();
}

function setCollaborationReplyTarget(messageId: string): void {
  collaborationReplyTo = messageId;
  const wasEditing = !!runtimeCollaborationEditTarget(state);
  setRuntimeCollaborationReplyTarget(state, messageId);
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (wasEditing && body) body.value = "";
  syncCollaborationComposer();
  if (messageId) {
    setText("runtime-message-send-status", "Reply target selected. Your next message will reply to " + messageId + ".");
    body?.focus();
  } else {
    setText("runtime-message-send-status", "Reply target cleared.");
  }
}

function beginCollaborationEdit(message: any): void {
  if (!setRuntimeCollaborationEditTarget(state, String(message?.message_id || ""))) return;
  collaborationReplyTo = "";
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (body) {
    body.value = String(message?.message || "");
    body.focus();
  }
  setText("runtime-message-send-status", "");
  syncCollaborationComposer();
}

function cancelCollaborationEdit(): void {
  clearRuntimeCollaborationEditTarget(state);
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (body) body.value = "";
  setText("runtime-message-send-status", "Edit cancelled.");
  syncCollaborationComposer();
}

function resetCollaborationComposerUi(): void {
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (body) body.value = "";
  syncCollaborationComposer();
}

function renderCollaboration(statusText?: string): void {
  const mutationNotice = takeRuntimeCollaborationMutationNotice(state);
  if (mutationNotice) {
    const editStillActive = !!runtimeCollaborationEditTarget(state);
    if (
      mutationNotice.includes("changed while editing")
      || mutationNotice.includes("Replacement confirmed")
      || mutationNotice.includes("Withdraw confirmed")
      || (mutationNotice.includes("Outcome not observed") && !editStillActive)
    ) {
      const body = el("runtime-message-body") as HTMLTextAreaElement | null;
      if (body) body.value = "";
    }
    setText("runtime-message-send-status", mutationNotice);
    statusText = [statusText, mutationNotice].filter(Boolean).join(" · ");
  }
  const available = state.collaboration.available !== false;
  if (!available) {
    const body = el("runtime-message-body") as HTMLTextAreaElement | null;
    if (body) body.value = "";
  }
  show("runtime-collaboration-unavailable", !available);
  show("runtime-collaboration-form", available);
  const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
  show("runtime-collaboration-empty", available && messages.length === 0);
  const status = available
    ? "Collaboration: " + collaborationPhaseLabel() + " · " + countLabel(messages.length, "retained message") + (statusText ? " · " + statusText : "")
    : "runtime:read unavailable";
  setText("runtime-collaboration-status", status);
  const node = el("runtime-collaboration-board"); clearNode(node);
  syncCollaborationComposer();
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
    if (message?.superseded_by_message_id) {
      const link = document.createElement("div"); link.className = "message-links";
      const replacementId = String(message.superseded_by_message_id);
      link.textContent = byId.has(replacementId)
        ? "superseded by " + replacementId
        : "superseded by " + replacementId + " · replacement unavailable / retained link only";
      card.appendChild(link);
    }
    if (message?.supersedes_message_id) {
      const link = document.createElement("div"); link.className = "message-links";
      const originalId = String(message.supersedes_message_id);
      link.textContent = byId.has(originalId)
        ? "replaces " + originalId
        : "replaces " + originalId + " · retained link only";
      card.appendChild(link);
    }
    const body = document.createElement("div"); body.className = "message-body"; body.textContent = String(message?.message || ""); card.appendChild(body);
    if (message?.requires_ack) {
      const ack = document.createElement("div"); ack.className = "message-ack";
      ack.textContent = typeof message?.first_ack_observed_at === "number"
        ? "ACK required · First ACK observed " + updatedLabel(message.first_ack_observed_at)
        : "ACK required";
      card.appendChild(ack);
    }
    if (message?.resolved_at || message?.resolution || message?.resolved_by_message_id || message?.closure_kind) {
      const resolution = document.createElement("div"); resolution.className = "message-resolution";
      const parts: string[] = [];
      if (message?.closure_kind === "withdrawn") parts.push("withdrawn" + (message.resolved_at ? " " + updatedLabel(message.resolved_at) : ""));
      else if (message?.closure_kind === "superseded") parts.push("superseded" + (message.resolved_at ? " " + updatedLabel(message.resolved_at) : ""));
      else if (message.resolved_at) parts.push("resolved " + updatedLabel(message.resolved_at));
      if (message.resolution) parts.push(String(message.resolution));
      if (message.resolved_by_message_id) parts.push("by " + String(message.resolved_by_message_id));
      resolution.textContent = parts.join(" · "); card.appendChild(resolution);
    }
    const actions = document.createElement("div"); actions.className = "message-actions";
    const replyButton = document.createElement("button"); replyButton.type = "button"; replyButton.className = "text-button"; replyButton.textContent = "Reply";
    replyButton.addEventListener("click", () => setCollaborationReplyTarget(id));
    actions.appendChild(replyButton);
    if (runtimeCollaborationMessageCanMutate(message) && state.collaboration.phase === "live" && !state.collaboration.uncertainMutation) {
      const editButton = document.createElement("button"); editButton.type = "button"; editButton.className = "text-button"; editButton.textContent = "Edit"; editButton.title = "Replace this retained message while preserving its history.";
      editButton.addEventListener("click", () => beginCollaborationEdit(message));
      const deleteButton = document.createElement("button"); deleteButton.type = "button"; deleteButton.className = "text-button"; deleteButton.textContent = "Delete"; deleteButton.title = "Withdraw this retained message; history is preserved.";
      deleteButton.addEventListener("click", () => void withdrawHumanCollaborationMessage(id));
      actions.appendChild(editButton); actions.appendChild(deleteButton);
    }
    card.appendChild(actions);
    node.appendChild(card);
    for (const child of children.get(id) || []) appendMessage(child, depth + 1, false);
  };
  for (const message of messages) {
    const parent = typeof message?.reply_to === "string" ? message.reply_to : "";
    if (!parent || !byId.has(parent)) appendMessage(message, 0, !!parent);
  }
  for (const message of messages) appendMessage(message, 0, false);
}

async function confirmCollaborationMutationDurability(
  request: any,
  mutation: any,
  controller: AbortController
): Promise<boolean> {
  const replacing = mutation?.kind === "replace";
  const payload: any = {
    project: request.project,
    session_id: request.sessionId,
    message_id: String(mutation?.messageId || ""),
  };
  if (replacing) payload.message = String(mutation?.message || "");
  setText("runtime-message-send-status", replacing
    ? "Confirming replacement durability…"
    : "Confirming withdrawal durability…");
  const response = await api(
    replacing ? "workflow-session-replace-message" : "workflow-session-withdraw-message",
    payload,
    controller.signal
  );
  if (!response || !isCurrentRuntimeCollaborationRequest(state, request)) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 0 || response.status === 503) {
    markRuntimeCollaborationMutationUncertain(state, request, mutation);
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration("durability confirmation still uncertain · refresh before retry");
    return false;
  }
  if (response.status === 403) {
    setRuntimeCollaborationAvailable(state, request, false);
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration();
    return false;
  }
  if (response.status === 404 || response.status === 409) {
    markRuntimeCollaborationMutationUncertain(state, request, mutation);
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration("message changed during durability confirmation · refresh retained state");
    return false;
  }
  const valid = replacing
    ? response.ok && response.data?.original && response.data?.replacement
    : response.ok && response.data?.message;
  if (!valid) {
    markRuntimeCollaborationMutationUncertain(state, request, mutation);
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration("durability confirmation failed · refresh before retry");
    return false;
  }
  if (replacing) {
    adoptRuntimeCollaborationObservation(state, request, {
      messages: [response.data.original, response.data.replacement],
    });
  } else {
    adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
  }
  completeRuntimeCollaborationMutationRecovery(
    state,
    request,
    replacing
      ? "Replacement durably confirmed after exact replay."
      : "Withdraw durably confirmed after exact replay."
  );
  return true;
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
  const mutationRecovery = runtimeCollaborationMutationRecovery(state, request);
  if (mutationRecovery && !(await confirmCollaborationMutationDurability(request, mutationRecovery, controller))) return null;
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

function sessionCollaborationAuthorityFailure(response: any): string | null {
  if (response?.status !== 403) return null;
  return "Session collaboration access required. This credential can still read the Session; add session:collaborate to send, edit, or withdraw messages.";
}

function setHumanJoinSendEnabled(enabled: boolean): void {
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  if (send) send.disabled = !enabled;
}

function syncAckComposer(): void {
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const edit = runtimeCollaborationEditTarget(state);
  const guidance = edit ? edit.kind === "guidance" : kind?.value === "guidance";
  show("runtime-message-ack-label", guidance);
  if (!checkbox) return;
  if (edit) {
    checkbox.disabled = true;
    checkbox.checked = !!edit.requires_ack;
    checkbox.title = "Inherited from the original retained message.";
    return;
  }
  checkbox.disabled = !guidance || priority?.value !== "high";
  if (checkbox.disabled) checkbox.checked = false;
  checkbox.title = guidance && priority?.value !== "high" ? "ACK requirement is available for High priority guidance." : "";
}

async function withdrawHumanCollaborationMessage(messageId: string): Promise<void> {
  const request = runtimeCollaborationRequest(state);
  if (!request || state.collaboration.available === false) return;
  setText("runtime-message-send-status", "Withdrawing retained message…");
  const response = await api("workflow-session-withdraw-message", {
    project: request.project,
    session_id: request.sessionId,
    message_id: messageId,
  });
  if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
  if (response?.status === 0 || response?.status === 503) {
    markRuntimeCollaborationMutationUncertain(state, request, { kind: "withdraw", messageId });
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration("withdraw outcome unknown · refresh before retry");
    return;
  }
  if (response?.status === 401) { lock("Credential rejected."); return; }
  const authorityFailure = sessionCollaborationAuthorityFailure(response);
  if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
  if (response?.status === 409) {
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    setText("runtime-message-send-status", "Message changed before Delete. Refresh retained messages before retrying.");
    renderCollaboration("message changed · refresh retained state");
    return;
  }
  if (!response?.ok || !response.data?.message) { setText("runtime-message-send-status", "Delete failed."); return; }
  if (String(state.collaboration.editTargetId || "") === messageId) {
    clearRuntimeCollaborationEditTarget(state);
    const body = el("runtime-message-body") as HTMLTextAreaElement | null;
    if (body) body.value = "";
  }
  adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
  setText("runtime-message-send-status", "Retained message withdrawn.");
  renderCollaboration();
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
  const editTarget = runtimeCollaborationEditTarget(state);
  if (editTarget) {
    if (send) send.disabled = true;
    setText("runtime-message-send-status", "Replacing retained message…");
    const response = await api("workflow-session-replace-message", {
      project: request.project,
      session_id: request.sessionId,
      message_id: editTarget.message_id,
      message,
    });
    if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
    if (response?.status === 0 || response?.status === 503) {
      markRuntimeCollaborationMutationUncertain(state, request, {
        kind: "replace",
        messageId: String(editTarget.message_id),
        message,
      });
      abortCollaboration();
      setRuntimeCollaborationPhase(state, request, "paused");
      renderCollaboration("replace outcome unknown · refresh before retry");
      return;
    }
    if (send) send.disabled = false;
    if (response?.status === 401) { lock("Credential rejected."); return; }
    const authorityFailure = sessionCollaborationAuthorityFailure(response);
    if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
    if (response?.status === 409) {
      clearRuntimeCollaborationEditTarget(state);
      if (body) body.value = "";
      abortCollaboration();
      setRuntimeCollaborationPhase(state, request, "paused");
      setText("runtime-message-send-status", "Message changed before Replace. Refresh retained messages before retrying.");
      renderCollaboration("message changed · refresh retained state");
      return;
    }
    if (!response?.ok || !response.data?.original || !response.data?.replacement) {
      setText("runtime-message-send-status", "Replace failed.");
      return;
    }
    clearRuntimeCollaborationEditTarget(state);
    adoptRuntimeCollaborationObservation(state, request, {
      messages: [response.data.original, response.data.replacement],
    });
    if (body) body.value = "";
    setText("runtime-message-send-status", response.data.replayed ? "Replacement already retained." : "Message replaced.");
    renderCollaboration();
    return;
  }
  if (send) send.disabled = true;
  setText("runtime-message-send-status", "Sending…");
  const response = await api("workflow-session-post-message", {
    project: request.project,
    session_id: request.sessionId,
    kind: kind?.value || "note",
    priority: priority?.value || "normal",
    message,
    reply_to: state.collaboration.replyTargetId || null,
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
  const authorityFailure = sessionCollaborationAuthorityFailure(response);
  if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
  if (!response?.ok || !response.data) { setText("runtime-message-send-status", "Send failed."); return; }
  adoptRuntimeCollaborationObservation(state, request, { messages: [response.data] });
  if (body) body.value = "";
  setCollaborationReplyTarget("");
  setText("runtime-message-send-status", "Sent.");
  renderCollaboration();
}

function operationKey(prefix: string): string {
  const random = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
  return prefix + "-" + random;
}

function communicationTimeLabel(value: any): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "time unavailable";
  return new Date(value).toLocaleString();
}

function parseAgentIds(value: string): string[] {
  const ids = value
    .split(/[\s,]+/)
    .map((item) => item.trim())
    .filter(Boolean);
  return Array.from(new Set(ids));
}

function communicationAgent(agentId: string): any | null {
  return communicationAgents.find((agent) => String(agent?.agent_id || "") === agentId) || null;
}

function selectedCommunicationAgent(): any | null {
  return communicationAgent(selectedCommunicationAgentId);
}

function selectedCommunicationConversation(): any | null {
  return communicationConversations.find(
    (conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId
  ) || null;
}

function communicationEndpointId(agentId = selectedCommunicationAgentId): string {
  return communicationEndpoints.get(agentId) || "";
}

function idempotencyKeyFor(
  pending: { fingerprint: string; key: string } | null,
  fingerprint: string,
  prefix: string
): { fingerprint: string; key: string } {
  return pending && pending.fingerprint === fingerprint
    ? pending
    : { fingerprint, key: operationKey(prefix) };
}

function resetCommunicationSurface(): void {
  communicationGeneration += 1;
  communicationAgents = [];
  communicationConversations = [];
  communicationDetail = null;
  communicationInbox = [];
  selectedCommunicationAgentId = "";
  selectedCommunicationConversationId = "";
  communicationReadAvailable = null;
  communicationManageAvailable = null;
  communicationRefreshInFlight = false;
  communicationEndpoints.clear();
  pendingEndpointAttach.clear();
  pendingAgentCreate = null;
  pendingConversationCreate = null;
  pendingConversationMessage = null;
  clearNode(el("runtime-agent-list"));
  clearNode(el("runtime-conversation-list"));
  clearNode(el("runtime-conversation-transcript"));
  clearNode(el("runtime-conversation-participants"));
  clearNode(el("runtime-inbox-list"));
  renderCommunicationSurface();
}

function detachCommunicationEndpointsBestEffort(): void {
  if (!token || communicationEndpoints.size === 0) return;
  const credential = token;
  for (const endpointId of communicationEndpoints.values()) {
    void fetch(API_BASE + "communication/endpoint/detach", {
      method: "POST",
      headers: { Authorization: "Bearer " + credential, "Content-Type": "application/json" },
      body: JSON.stringify({ endpoint_id: endpointId }),
      keepalive: true,
    }).catch(() => undefined);
  }
  communicationEndpoints.clear();
}

function renderCommunicationAvailability(): void {
  const available = communicationReadAvailable !== false;
  show("runtime-communication-unavailable", !available);
  show("runtime-communication-surface", available);
  const access = communicationReadAvailable === null
    ? "communication:read checking…"
    : available
      ? "communication:read" + (communicationManageAvailable === false ? " · read only" : " · polling every 8s")
      : "communication:read unavailable";
  setText("runtime-communication-status", access);
}

function renderCommunicationAgents(): void {
  setText("runtime-communication-count", countLabel(communicationAgents.length, "Agent"));
  const list = el("runtime-agent-list");
  clearNode(list);
  show("runtime-agent-empty", communicationReadAvailable === true && communicationAgents.length === 0);
  if (!list) return;
  for (const agent of communicationAgents) {
    const agentId = String(agent?.agent_id || "");
    if (!agentId) continue;
    const row = document.createElement("button");
    row.type = "button";
    row.className = "communication-row" + (agentId === selectedCommunicationAgentId ? " selected" : "");
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", agentId === selectedCommunicationAgentId ? "true" : "false");
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(agent?.display_name || agent?.handle || "Agent") + " · @" + String(agent?.handle || "agent");
    const unread = document.createElement("span");
    unread.className = "chip" + (Number(agent?.queued_delivery_count || 0) > 0 ? " tone-warn" : "");
    unread.textContent = countLabel(agent?.queued_delivery_count, "queued delivery");
    head.appendChild(title);
    head.appendChild(unread);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = agentId + " · profile r" + String(agent?.profile_revision || 0) + " · " + countLabel(agent?.active_endpoint_count, "active Endpoint");
    row.appendChild(meta);
    row.addEventListener("click", () => {
      selectedCommunicationAgentId = agentId;
      communicationInbox = [];
      const participants = el("runtime-conversation-agent-ids") as HTMLInputElement | null;
      if (participants && !participants.value.trim()) participants.value = agentId;
      renderCommunicationAgents();
      renderCommunicationAgentCard();
      renderCommunicationInbox();
      if (communicationEndpointId(agentId)) void fetchCommunicationInbox(communicationGeneration);
    });
    list.appendChild(row);
  }
}

function renderCommunicationAgentCard(): void {
  const agent = selectedCommunicationAgent();
  show("runtime-agent-card", !!agent);
  if (!agent) return;
  const agentId = String(agent.agent_id || "");
  setText("runtime-agent-card-name", String(agent.display_name || agent.handle || "Agent Card") + " · @" + String(agent.handle || "agent"));
  setText("runtime-agent-card-id", agentId);
  setText("runtime-agent-card-description", String(agent.description || "No description."));
  setText("runtime-agent-card-revision", "Profile revision " + String(agent.profile_revision || 0) + " · updated " + communicationTimeLabel(agent.updated_at_unix_ms));
  setText("runtime-agent-unread", countLabel(agent.queued_delivery_count, "queued"));
  const labels = el("runtime-agent-card-labels");
  clearNode(labels);
  if (labels) {
    for (const label of Array.isArray(agent.specialty_labels) ? agent.specialty_labels : []) {
      appendChip(labels, String(label));
    }
  }
  const endpointId = communicationEndpointId(agentId);
  setText(
    "runtime-agent-endpoint-status",
    endpointId
      ? "Browser Endpoint " + endpointId + " · attachment only, no execution authority"
      : "No browser Endpoint attached. Agent identity remains durable."
  );
  show("runtime-agent-attach", !endpointId);
  show("runtime-agent-detach", !!endpointId);
}

function renderCommunicationConversations(): void {
  const list = el("runtime-conversation-list");
  clearNode(list);
  show("runtime-conversation-empty", communicationReadAvailable === true && communicationConversations.length === 0);
  if (!list) return;
  for (const conversation of communicationConversations) {
    const conversationId = String(conversation?.conversation_id || "");
    if (!conversationId) continue;
    const row = document.createElement("button");
    row.type = "button";
    row.className = "communication-row" + (conversationId === selectedCommunicationConversationId ? " selected" : "");
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", conversationId === selectedCommunicationConversationId ? "true" : "false");
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(conversation?.title || "Untitled Conversation");
    const count = document.createElement("span");
    count.className = "chip";
    count.textContent = countLabel(conversation?.message_count, "message");
    head.appendChild(title);
    head.appendChild(count);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = conversationId + " · " + countLabel(conversation?.participant_count, "participant") + " · seq " + String(conversation?.last_seq || 0);
    row.appendChild(meta);
    row.addEventListener("click", () => {
      selectedCommunicationConversationId = conversationId;
      communicationDetail = null;
      renderCommunicationConversations();
      renderCommunicationConversation();
      void fetchCommunicationConversation(communicationGeneration);
    });
    list.appendChild(row);
  }
}

function deliveryAgentLabel(agentId: string): string {
  const agent = communicationAgent(agentId);
  return agent ? String(agent.display_name || agent.handle || agentId) : agentId;
}

function renderCommunicationConversation(): void {
  const detail = communicationDetail;
  const summary = detail?.conversation || selectedCommunicationConversation();
  const available = !!summary && String(summary?.conversation_id || "") === selectedCommunicationConversationId;
  show("runtime-conversation-detail", available);
  show("runtime-conversation-detail-empty", !available);
  const transcript = el("runtime-conversation-transcript");
  clearNode(transcript);
  clearNode(el("runtime-conversation-participants"));
  if (!available || !detail) return;
  setText("runtime-conversation-name", String(summary.title || "Untitled Conversation"));
  setText("runtime-conversation-id", String(summary.conversation_id || ""));
  setText(
    "runtime-conversation-seq",
    "seq " + String(summary.last_seq || 0) + " · " + countLabel(summary.message_count, "message") + ((Number(detail?.after_seq || 0) > 0 || detail.truncated) ? " · recent bounded page" : "")
  );
  const participants = el("runtime-conversation-participants");
  if (participants) {
    for (const participant of Array.isArray(detail.participants) ? detail.participants : []) {
      const kind = String(participant?.participant_kind || "participant");
      const label = kind === "agent"
        ? "Agent · " + String(participant?.display_name || participant?.handle || participant?.agent_id || "unknown")
        : "Human · " + String(participant?.principal_kind || "credential principal");
      appendChip(participants, label, kind === "agent" ? "tone-pass" : "tone-runtime");
    }
  }
  const messages = Array.isArray(detail.messages) ? detail.messages : [];
  show("runtime-conversation-transcript-empty", messages.length === 0);
  if (!transcript) return;
  for (const message of messages) {
    const author = message?.author || {};
    const agentAuthored = String(author.participant_kind || "") === "agent";
    const card = document.createElement("article");
    card.className = "conversation-message" + (agentAuthored ? " agent-authored" : "");
    const head = document.createElement("div");
    head.className = "conversation-message-head";
    const name = document.createElement("span");
    name.className = "conversation-message-author";
    name.textContent = agentAuthored
      ? "Agent · " + String(author.display_name || author.handle || author.agent_id || "unknown")
      : "Human · " + String(author.principal_kind || "credential principal");
    const seq = document.createElement("span");
    seq.className = "muted small";
    seq.textContent = "#" + String(message?.seq || 0) + " · " + communicationTimeLabel(message?.created_at_unix_ms);
    head.appendChild(name);
    head.appendChild(seq);
    card.appendChild(head);
    const meta = document.createElement("div");
    meta.className = "conversation-message-meta";
    const metaParts = [String(message?.message_id || "")];
    if (author.agent_id) metaParts.push(String(author.agent_id));
    if (message?.reply_to) metaParts.push("reply to " + String(message.reply_to));
    meta.textContent = metaParts.join(" · ");
    card.appendChild(meta);
    const body = document.createElement("div");
    body.className = "conversation-message-body";
    body.textContent = String(message?.body || "");
    card.appendChild(body);
    const deliveries = Array.isArray(message?.deliveries) ? message.deliveries : [];
    const delivery = document.createElement("div");
    delivery.className = "conversation-message-deliveries";
    delivery.textContent = deliveries.length
      ? "Agent Inbox: " + deliveries.map((item: any) => deliveryAgentLabel(String(item?.recipient_agent_id || "")) + " " + String(item?.state || "unknown")).join(" · ")
      : "No Agent Inbox delivery · transcript / Human room only";
    card.appendChild(delivery);
    transcript.appendChild(card);
  }
  transcript.scrollTop = transcript.scrollHeight;
}

function renderCommunicationInbox(): void {
  const list = el("runtime-inbox-list");
  clearNode(list);
  const agent = selectedCommunicationAgent();
  const endpointId = communicationEndpointId();
  show("runtime-inbox-consume-all", !!endpointId && communicationInbox.length > 0);
  if (!agent) {
    setText("runtime-inbox-status", "Select an Agent to inspect recipient-specific queued deliveries.");
    return;
  }
  if (!endpointId) {
    setText("runtime-inbox-status", "Attach this browser as an Endpoint. Queued deliveries remain durable while offline.");
    return;
  }
  const totalQueued = Number(agent.queued_delivery_count || 0);
  setText(
    "runtime-inbox-status",
    countLabel(totalQueued, "queued delivery") + (communicationInbox.length < totalQueued ? " · showing " + String(communicationInbox.length) : "") + " · reading does not consume or wake a model"
  );
  if (!list) return;
  for (const item of communicationInbox) {
    const row = document.createElement("article");
    row.className = "communication-row inbox-delivery";
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(item?.conversation_title || "Untitled Conversation") + " · #" + String(item?.message?.seq || 0);
    const consume = document.createElement("button");
    consume.type = "button";
    consume.className = "text-button";
    consume.textContent = "Consume";
    consume.addEventListener("click", () => void consumeCommunicationDeliveries([String(item?.delivery_id || "")]));
    head.appendChild(title);
    head.appendChild(consume);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = String(item?.delivery_id || "") + " · from " + (item?.message?.author?.participant_kind === "agent" ? deliveryAgentLabel(String(item.message.author.agent_id || "")) : "Human");
    row.appendChild(meta);
    const body = document.createElement("div");
    body.className = "inbox-message-preview";
    body.textContent = String(item?.message?.body || "");
    row.appendChild(body);
    list.appendChild(row);
  }
}

function renderCommunicationSurface(): void {
  renderCommunicationAvailability();
  renderCommunicationAgents();
  renderCommunicationAgentCard();
  renderCommunicationConversations();
  renderCommunicationConversation();
  renderCommunicationInbox();
}

async function fetchCommunicationAgents(generation: number): Promise<boolean> {
  const response = await api("communication/agents", { offset: 0, limit: 100 });
  if (generation !== communicationGeneration || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    communicationAgents = [];
    communicationConversations = [];
    communicationDetail = null;
    communicationInbox = [];
    renderCommunicationSurface();
    return true;
  }
  if (!response.ok || !response.data) return false;
  communicationReadAvailable = true;
  communicationAgents = Array.isArray(response.data.agents) ? response.data.agents : [];
  if (!communicationAgents.some((agent) => String(agent?.agent_id || "") === selectedCommunicationAgentId)) {
    selectedCommunicationAgentId = String(communicationAgents[0]?.agent_id || "");
    communicationInbox = [];
  }
  renderCommunicationAgents();
  renderCommunicationAgentCard();
  return true;
}

async function fetchCommunicationConversations(generation: number): Promise<boolean> {
  const response = await api("communication/conversations", { offset: 0, limit: 100 });
  if (generation !== communicationGeneration || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    renderCommunicationSurface();
    return true;
  }
  if (!response.ok || !response.data) return false;
  communicationReadAvailable = true;
  communicationConversations = Array.isArray(response.data.conversations) ? response.data.conversations : [];
  if (!communicationConversations.some((conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId)) {
    selectedCommunicationConversationId = String(communicationConversations[0]?.conversation_id || "");
    communicationDetail = null;
  }
  renderCommunicationConversations();
  return true;
}

async function fetchCommunicationConversation(generation: number): Promise<boolean> {
  const conversationId = selectedCommunicationConversationId;
  if (!conversationId) {
    communicationDetail = null;
    renderCommunicationConversation();
    return true;
  }
  const afterSeq = runtimeCommunicationTranscriptAfterSeq(selectedCommunicationConversation()?.last_seq, 100);
  const response = await api("communication/conversation", {
    conversation_id: conversationId,
    after_seq: afterSeq,
    limit: 100,
  });
  if (generation !== communicationGeneration || conversationId !== selectedCommunicationConversationId || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    renderCommunicationSurface();
    return true;
  }
  if (response.status === 404) {
    communicationDetail = null;
    selectedCommunicationConversationId = "";
    renderCommunicationConversation();
    return false;
  }
  if (!response.ok || !response.data) return false;
  communicationDetail = response.data;
  renderCommunicationConversation();
  return true;
}

async function fetchCommunicationInbox(generation: number): Promise<boolean> {
  const agentId = selectedCommunicationAgentId;
  const endpointId = communicationEndpointId(agentId);
  if (!agentId || !endpointId) {
    communicationInbox = [];
    renderCommunicationInbox();
    return true;
  }
  const response = await api("communication/inbox", {
    agent_id: agentId,
    endpoint_id: endpointId,
    after_delivery_order: 0,
    limit: 100,
  });
  if (generation !== communicationGeneration || agentId !== selectedCommunicationAgentId || endpointId !== communicationEndpointId(agentId) || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    renderCommunicationSurface();
    return true;
  }
  if (response.status === 404 || response.status === 400) {
    communicationEndpoints.delete(agentId);
    communicationInbox = [];
    renderCommunicationAgentCard();
    renderCommunicationInbox();
    return false;
  }
  if (!response.ok || !response.data) return false;
  communicationInbox = Array.isArray(response.data.deliveries) ? response.data.deliveries : [];
  renderCommunicationInbox();
  return true;
}

async function refreshCommunication(): Promise<boolean> {
  if (!token || communicationRefreshInFlight) return true;
  communicationRefreshInFlight = true;
  const generation = ++communicationGeneration;
  setText("runtime-communication-status", "Refreshing durable communication…");
  try {
    const agentsOk = await fetchCommunicationAgents(generation);
    if (generation !== communicationGeneration || !agentsOk || communicationReadAvailable !== true) return agentsOk;
    const conversationsOk = await fetchCommunicationConversations(generation);
    if (generation !== communicationGeneration || !conversationsOk || communicationReadAvailable !== true) return agentsOk && conversationsOk;
    const [conversationOk, inboxOk] = await Promise.all([
      fetchCommunicationConversation(generation),
      fetchCommunicationInbox(generation),
    ]);
    renderCommunicationSurface();
    return agentsOk && conversationsOk && conversationOk && inboxOk;
  } finally {
    if (generation === communicationGeneration) communicationRefreshInFlight = false;
  }
}

async function createCommunicationAgent(event: Event): Promise<void> {
  event.preventDefault();
  const handle = (el("runtime-agent-handle") as HTMLInputElement | null)?.value.trim() || "";
  const displayName = (el("runtime-agent-display-name") as HTMLInputElement | null)?.value.trim() || "";
  const description = (el("runtime-agent-description") as HTMLTextAreaElement | null)?.value.trim() || "";
  const labels = parseAgentIds((el("runtime-agent-labels") as HTMLInputElement | null)?.value || "");
  if (!handle || !displayName) { setText("runtime-agent-create-status", "Handle and display name are required."); return; }
  const fingerprint = JSON.stringify({ handle, displayName, description, labels });
  pendingAgentCreate = idempotencyKeyFor(pendingAgentCreate, fingerprint, "runtime-agent");
  setText("runtime-agent-create-status", "Creating durable Agent…");
  const response = await api("communication/agent/create", {
    handle,
    display_name: displayName,
    description,
    specialty_labels: labels,
    idempotency_key: pendingAgentCreate.key,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-agent-create-status", "communication:manage required.");
    renderCommunicationAvailability();
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-agent-create-status", "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key, or refresh before deciding.");
    return;
  }
  if (!response.ok || !response.data?.agent) {
    setText("runtime-agent-create-status", String(response.data?.message || "Agent creation failed."));
    return;
  }
  communicationManageAvailable = true;
  selectedCommunicationAgentId = String(response.data.agent.agent_id || "");
  pendingAgentCreate = null;
  for (const id of ["runtime-agent-handle", "runtime-agent-display-name", "runtime-agent-description", "runtime-agent-labels"]) {
    const input = el(id) as HTMLInputElement | HTMLTextAreaElement | null;
    if (input) input.value = "";
  }
  setText("runtime-agent-create-status", response.data.replayed ? "Existing idempotent Agent replayed." : "Agent created.");
  await refreshCommunication();
}

async function attachCommunicationEndpoint(): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  if (!agentId) return;
  let pending = pendingEndpointAttach.get(agentId);
  if (!pending) {
    pending = { key: operationKey("runtime-endpoint"), attachmentId: pageAttachmentId + "-" + agentId.slice(-8) };
    pendingEndpointAttach.set(agentId, pending);
  }
  setText("runtime-agent-endpoint-status", "Attaching browser Endpoint…");
  const response = await api("communication/endpoint/attach", {
    agent_id: agentId,
    host: "Runtime Console",
    client_attachment_id: pending.attachmentId,
    wake_capable: false,
    controller_generation: "a1-bounded-polling-v1",
    idempotency_key: pending.key,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-agent-endpoint-status", "communication:manage required.");
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-agent-endpoint-status", "Outcome uncertain. Retry Attach to replay the same idempotency key; do not create a new attachment.");
    return;
  }
  if (!response.ok || !response.data?.endpoint?.endpoint_id) {
    setText("runtime-agent-endpoint-status", String(response.data?.message || "Endpoint attach failed."));
    return;
  }
  communicationManageAvailable = true;
  communicationEndpoints.set(agentId, String(response.data.endpoint.endpoint_id));
  pendingEndpointAttach.delete(agentId);
  renderCommunicationAgentCard();
  await fetchCommunicationInbox(communicationGeneration);
}

async function detachCommunicationEndpoint(): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  const endpointId = communicationEndpointId(agentId);
  if (!agentId || !endpointId) return;
  setText("runtime-agent-endpoint-status", "Detaching browser Endpoint…");
  const response = await api("communication/endpoint/detach", { endpoint_id: endpointId });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-agent-endpoint-status", "communication:manage required.");
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-agent-endpoint-status", "Detach outcome uncertain. Refresh before retry; the durable Agent and Inbox are unaffected.");
    return;
  }
  if (!response.ok) {
    setText("runtime-agent-endpoint-status", String(response.data?.message || "Endpoint detach failed."));
    return;
  }
  communicationEndpoints.delete(agentId);
  communicationInbox = [];
  renderCommunicationAgentCard();
  renderCommunicationInbox();
  await refreshCommunication();
}

async function createCommunicationConversation(event: Event): Promise<void> {
  event.preventDefault();
  const title = (el("runtime-conversation-title") as HTMLInputElement | null)?.value.trim() || "";
  const idsInput = (el("runtime-conversation-agent-ids") as HTMLInputElement | null)?.value || "";
  const agentIds = parseAgentIds(idsInput || selectedCommunicationAgentId);
  if (agentIds.length === 0) { setText("runtime-conversation-create-status", "At least one Agent id is required."); return; }
  const fingerprint = JSON.stringify({ title, agentIds: [...agentIds].sort() });
  pendingConversationCreate = idempotencyKeyFor(pendingConversationCreate, fingerprint, "runtime-conversation");
  setText("runtime-conversation-create-status", "Creating durable Conversation…");
  const response = await api("communication/conversation/create", {
    title: title || null,
    agent_ids: agentIds,
    idempotency_key: pendingConversationCreate.key,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-conversation-create-status", "communication:manage required.");
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-conversation-create-status", "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key.");
    return;
  }
  if (!response.ok || !response.data?.conversation?.conversation?.conversation_id) {
    setText("runtime-conversation-create-status", String(response.data?.message || "Conversation creation failed."));
    return;
  }
  communicationManageAvailable = true;
  selectedCommunicationConversationId = String(response.data.conversation.conversation.conversation_id);
  pendingConversationCreate = null;
  const titleInput = el("runtime-conversation-title") as HTMLInputElement | null;
  const agentsInput = el("runtime-conversation-agent-ids") as HTMLInputElement | null;
  if (titleInput) titleInput.value = "";
  if (agentsInput) agentsInput.value = selectedCommunicationAgentId;
  setText("runtime-conversation-create-status", response.data.replayed ? "Existing idempotent Conversation replayed." : "Conversation created.");
  await refreshCommunication();
}

async function postCommunicationMessage(event: Event): Promise<void> {
  event.preventDefault();
  const conversationId = selectedCommunicationConversationId;
  const bodyNode = el("runtime-conversation-body") as HTMLTextAreaElement | null;
  const recipientsNode = el("runtime-conversation-recipients") as HTMLInputElement | null;
  const body = bodyNode?.value.trim() || "";
  const recipientsText = recipientsNode?.value.trim() || "";
  if (!conversationId || !body) { setText("runtime-conversation-send-status", "Select a Conversation and enter a message."); return; }
  const recipientAgentIds = recipientsText ? parseAgentIds(recipientsText) : null;
  const fingerprint = JSON.stringify({ conversationId, body, recipientAgentIds });
  pendingConversationMessage = idempotencyKeyFor(pendingConversationMessage, fingerprint, "runtime-message");
  setText("runtime-conversation-send-status", "Appending Message and Agent deliveries atomically…");
  const response = await api("communication/message/post", {
    conversation_id: conversationId,
    body,
    recipient_agent_ids: recipientAgentIds,
    idempotency_key: pendingConversationMessage.key,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-conversation-send-status", "communication:manage required.");
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-conversation-send-status", "Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key, or refresh the transcript first.");
    return;
  }
  if (!response.ok || !response.data?.message) {
    setText("runtime-conversation-send-status", String(response.data?.message || "Message append failed."));
    return;
  }
  communicationManageAvailable = true;
  pendingConversationMessage = null;
  if (bodyNode) bodyNode.value = "";
  setText("runtime-conversation-send-status", response.data.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent.");
  await refreshCommunication();
}

async function consumeCommunicationDeliveries(deliveryIds: string[]): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  const endpointId = communicationEndpointId(agentId);
  const ids = deliveryIds.filter(Boolean);
  if (!agentId || !endpointId || ids.length === 0) return;
  setText("runtime-inbox-status", "Consuming recipient state…");
  const response = await api("communication/inbox/consume", {
    agent_id: agentId,
    endpoint_id: endpointId,
    delivery_ids: ids,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
  if (response?.status === 403) {
    communicationManageAvailable = false;
    setText("runtime-inbox-status", "communication:manage required to consume deliveries.");
    return;
  }
  if (!response || response.status === 0 || response.status === 503) {
    setText("runtime-inbox-status", "Consume outcome uncertain. Refresh before retry; desired-state replay is safe.");
    return;
  }
  if (!response.ok) {
    setText("runtime-inbox-status", String(response.data?.message || "Delivery consume failed."));
    return;
  }
  communicationManageAvailable = true;
  await refreshCommunication();
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
  const projectsRequest = refreshRuntimeProjects(state, projectSearch);
  try {
    const [overviewOk, projectsOk, communicationOk] = await Promise.all([
      fetchOverview(overviewRequest),
      fetchProjects(projectsRequest),
      refreshCommunication(),
    ]);
    if (!token) return;
    if (overviewOk && projectsOk && communicationOk) {
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
    void refreshCommunication();
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
  void refreshCommunication();
});

el("runtime-agent-create-form")?.addEventListener("submit", (event) => void createCommunicationAgent(event));
el("runtime-agent-attach")?.addEventListener("click", () => void attachCommunicationEndpoint());
el("runtime-agent-detach")?.addEventListener("click", () => void detachCommunicationEndpoint());
el("runtime-conversation-create-form")?.addEventListener("submit", (event) => void createCommunicationConversation(event));
el("runtime-conversation-message-form")?.addEventListener("submit", (event) => void postCommunicationMessage(event));
el("runtime-inbox-consume-all")?.addEventListener("click", () => {
  void consumeCommunicationDeliveries(
    communicationInbox.map((item) => String(item?.delivery_id || "")).filter(Boolean)
  );
});

el("runtime-device-select")?.addEventListener("change", () => {
  const select = el("runtime-device-select") as HTMLSelectElement | null; if (!select) return;
  applyRunnerFilter(select.value);
});
el("runtime-project-search")?.addEventListener("input", () => {
  const input = el("runtime-project-search") as HTMLInputElement | null;
  projectSearch = input?.value || "";
  stopProjectSearchTimer();
  if (!token) return;
  setText("runtime-project-status", "Searching…");
  projectSearchTimer = window.setTimeout(() => {
    projectSearchTimer = 0;
    if (!token) return;
    void fetchProjects(refreshRuntimeProjects(state, projectSearch));
  }, PROJECT_SEARCH_DEBOUNCE_MS);
});
el("runtime-message-kind")?.addEventListener("change", syncAckComposer);
el("runtime-message-priority")?.addEventListener("change", syncAckComposer);
el("runtime-message-reply-clear")?.addEventListener("click", () => setCollaborationReplyTarget(""));
el("runtime-message-edit-clear")?.addEventListener("click", cancelCollaborationEdit);
el("runtime-collaboration-form")?.addEventListener("submit", (event) => void postHumanCollaborationMessage(event));
el("runtime-refresh")?.addEventListener("click", () => void refreshAll());
el("runtime-lock")?.addEventListener("click", () => lock());
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
  const node = el("runtime-timeline"); if (!node) return;
  updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight); syncFollowUi();
});
syncAckComposer();
window.addEventListener("pagehide", () => {
  detachCommunicationEndpointsBestEffort();
  token = "";
  abortAll();
  resetCommunicationSurface();
  stopAuto();
});

lock();
