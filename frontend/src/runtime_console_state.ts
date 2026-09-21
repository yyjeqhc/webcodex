import {
  initialWorkflowSessionState,
  selectWorkflowSession,
  refreshWorkflowSessionDetail,
  clearWorkflowSessionSelection,
  isCurrentWorkflowSessionDetailRequest,
  adoptWorkflowSessionDetail,
} from "./workflow_session_state.js";
import {
  emptyCollaborationState,
  resetCollaborationState,
} from "./runtime_collaboration_state.js";
import type { RuntimeLanguage } from "./runtime_i18n.js";

export type RuntimeWindowAvailability = "idle" | "loading" | "available" | "stale" | "unavailable";

export function runtimeWindowAvailabilityAfterHttpResponse(
  status: number,
  ok: boolean,
  hasData: boolean,
): RuntimeWindowAvailability {
  if (status === 403) return "unavailable";
  return ok && hasData ? "available" : "stale";
}

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

export function runtimeWorkflowSessionSummaryRevision(session: any): string {
  if (!session) return "";
  return JSON.stringify([
    String(session.session_id || ""),
    String(session.title || ""),
    String(session.lifecycle || ""),
    String(session.mode || ""),
    typeof session.updated_at === "number" ? session.updated_at : null,
    !!session.running_call,
    typeof session.running_jobs === "number" ? session.running_jobs : null,
    session.running_jobs_complete === true,
    session.current_activity ?? null,
    session.last_activity ?? null,
    session.overview ?? null,
  ]);
}

export function runtimeWorkflowSessionSummaryChanged(previous: any, next: any): boolean {
  return runtimeWorkflowSessionSummaryRevision(previous) !== runtimeWorkflowSessionSummaryRevision(next);
}

export function runtimeDeviceIds(projects: any[]): string[] {
  const devices = new Set<string>();
  for (const project of Array.isArray(projects) ? projects : []) {
    const clientId = typeof project?.client_id === "string" ? project.client_id : "";
    if (clientId) devices.add(clientId);
  }
  return Array.from(devices).sort(compareText);
}

export function runtimeProjectsForDevice(projects: any[], clientId: string): any[] {
  return (Array.isArray(projects) ? projects : [])
    .filter((project) => project && (!clientId || project.client_id === clientId) && typeof project.id === "string" && project.id)
    .slice()
    .sort((left, right) => {
      const leftName = typeof left.name === "string" && left.name ? left.name : left.id;
      const rightName = typeof right.name === "string" && right.name ? right.name : right.id;
      return compareText(leftName, rightName) || compareText(left.id, right.id);
    });
}

function projectAttentionCount(project: any): number {
  const attention = project?.sessions?.attention;
  return ["open_guidance", "open_questions", "open_risks", "open_todos"]
    .reduce((total, key) => total + (typeof attention?.[key] === "number" ? Math.max(0, attention[key]) : 0), 0);
}

export function filterAndSortRuntimeProjects(projects: any[], clientId: string, query: string): any[] {
  const needle = String(query || "").trim().toLocaleLowerCase();
  return runtimeProjectsForDevice(projects, clientId)
    .filter((project) => {
      if (!needle) return true;
      return [project?.name, project?.id, project?.client_id, project?.path]
        .filter((value) => typeof value === "string")
        .some((value) => String(value).toLocaleLowerCase().includes(needle));
    })
    .sort((left, right) => {
      const leftRunning = typeof left?.sessions?.running_sessions === "number" ? left.sessions.running_sessions : 0;
      const rightRunning = typeof right?.sessions?.running_sessions === "number" ? right.sessions.running_sessions : 0;
      if (!!rightRunning !== !!leftRunning) return rightRunning ? 1 : -1;
      const leftAttention = projectAttentionCount(left);
      const rightAttention = projectAttentionCount(right);
      if (!!rightAttention !== !!leftAttention) return rightAttention ? 1 : -1;
      const leftUpdated = typeof left?.sessions?.latest_updated_at === "number" ? left.sessions.latest_updated_at : 0;
      const rightUpdated = typeof right?.sessions?.latest_updated_at === "number" ? right.sessions.latest_updated_at : 0;
      if (leftUpdated !== rightUpdated) return rightUpdated - leftUpdated;
      const leftName = typeof left?.name === "string" && left.name ? left.name : left.id;
      const rightName = typeof right?.name === "string" && right.name ? right.name : right.id;
      return compareText(String(leftName || ""), String(rightName || "")) || compareText(String(left?.id || ""), String(right?.id || ""));
    });
}

export function runtimeProjectIdentityText(project: any): string {
  if (!project || typeof project.id !== "string" || !project.id) return "No project selected";
  const runner = typeof project.client_id === "string" && project.client_id ? project.client_id : "unknown";
  const path = typeof project.path === "string" && project.path ? project.path : "unavailable";
  return "Runner: " + runner + " · Project: " + project.id + " · Workspace: " + path;
}

export function preferredRuntimeProjectSelection(
  projects: any[],
  selectedDevice: string,
  selectedProject: string
): any {
  const rows = Array.isArray(projects) ? projects : [];
  if (selectedProject) {
    const retained = rows.find(
      (project) => project && project.id === selectedProject && typeof project.client_id === "string" && project.client_id
    );
    if (retained) return { device: retained.client_id, project: retained.id };
  }
  const devices = runtimeDeviceIds(rows);
  const device = devices.includes(selectedDevice) ? selectedDevice : "";
  return { device, project: "" };
}

export function initialRuntimeConsoleState(): any {
  return {
    credentialGeneration: 0,
    overviewGeneration: 0,
    projectsGeneration: 0,
    runnerGeneration: 0,
    selectedDevice: "",
    selectedProject: "",
    projectGeneration: 0,
    sessionListGeneration: 0,
    projectWindowsGeneration: 0,
    workflow: initialWorkflowSessionState(),
    collaboration: emptyCollaborationState(),
  };
}

export function invalidateRuntimeCredential(state: any): void {
  state.credentialGeneration += 1;
  state.overviewGeneration += 1;
  state.projectsGeneration += 1;
  state.runnerGeneration += 1;
  state.selectedDevice = "";
  state.selectedProject = "";
  state.projectGeneration += 1;
  state.sessionListGeneration += 1;
  state.projectWindowsGeneration += 1;
  clearWorkflowSessionSelection(state.workflow);
  resetCollaborationState(state.collaboration);
}

export function beginRuntimeCredential(state: any): any {
  invalidateRuntimeCredential(state);
  return refreshRuntimeProjects(state);
}

export function refreshRuntimeOverview(state: any): any {
  state.overviewGeneration += 1;
  return { credentialGeneration: state.credentialGeneration, generation: state.overviewGeneration };
}

export function isCurrentRuntimeOverviewRequest(state: any, request: any): boolean {
  return !!request && request.credentialGeneration === state.credentialGeneration && request.generation === state.overviewGeneration;
}

export function refreshRuntimeProjects(state: any, query = "", clientId = state.selectedDevice): any {
  state.projectsGeneration += 1;
  return {
    credentialGeneration: state.credentialGeneration,
    projectGeneration: state.projectGeneration,
    generation: state.projectsGeneration,
    clientId: String(clientId || ""),
    query: String(query || "").trim(),
  };
}

export function isCurrentRuntimeProjectsRequest(state: any, request: any): boolean {
  return !!request &&
    request.credentialGeneration === state.credentialGeneration &&
    request.projectGeneration === state.projectGeneration &&
    request.generation === state.projectsGeneration;
}

export function refreshRuntimeRunner(state: any): any {
  if (!state.selectedDevice) return null;
  state.runnerGeneration += 1;
  return { credentialGeneration: state.credentialGeneration, device: state.selectedDevice, generation: state.runnerGeneration };
}

export function isCurrentRuntimeRunnerRequest(state: any, request: any): boolean {
  return !!request && request.credentialGeneration === state.credentialGeneration &&
    request.device === state.selectedDevice && request.generation === state.runnerGeneration;
}

export function selectRuntimeRunnerFilter(state: any, device: string): void {
  selectRuntimeProject(state, device, "");
}

export function selectRuntimeProject(state: any, device: string, project: string): any {
  if (state.selectedDevice !== device) state.runnerGeneration += 1;
  state.selectedDevice = device;
  state.selectedProject = project;
  state.projectGeneration += 1;
  state.sessionListGeneration += 1;
  state.projectWindowsGeneration += 1;
  clearWorkflowSessionSelection(state.workflow);
  resetCollaborationState(state.collaboration);
  return refreshRuntimeSessionList(state);
}

export function refreshRuntimeSessionList(state: any): any {
  if (!state.selectedProject) return null;
  state.sessionListGeneration += 1;
  return {
    credentialGeneration: state.credentialGeneration,
    project: state.selectedProject,
    projectGeneration: state.projectGeneration,
    generation: state.sessionListGeneration,
  };
}

export function isCurrentRuntimeSessionListRequest(state: any, request: any): boolean {
  return !!request && request.credentialGeneration === state.credentialGeneration &&
    request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
    request.generation === state.sessionListGeneration;
}

export function refreshRuntimeProjectWindows(state: any): any {
  if (!state.selectedProject) return null;
  state.projectWindowsGeneration += 1;
  return {
    credentialGeneration: state.credentialGeneration,
    project: state.selectedProject,
    projectGeneration: state.projectGeneration,
    generation: state.projectWindowsGeneration,
  };
}

export function isCurrentRuntimeProjectWindowsRequest(state: any, request: any): boolean {
  return !!request && request.credentialGeneration === state.credentialGeneration &&
    request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
    request.generation === state.projectWindowsGeneration;
}

function wrapWorkflowRequest(state: any, request: any): any {
  if (!request || !state.selectedProject) return null;
  return {
    credentialGeneration: state.credentialGeneration,
    project: state.selectedProject,
    projectGeneration: state.projectGeneration,
    sessionId: request.sessionId,
    generation: request.generation,
  };
}

export function selectRuntimeWorkflowSession(state: any, sessionId: string): any {
  resetCollaborationState(state.collaboration);
  state.collaboration.sessionId = sessionId;
  return wrapWorkflowRequest(state, selectWorkflowSession(state.workflow, sessionId));
}

export function selectRuntimeSessionLocation(
  state: any,
  device: string,
  project: string,
  sessionId: string
): any {
  const sessionListRequest = selectRuntimeProject(state, device, project);
  const detailRequest = selectRuntimeWorkflowSession(state, sessionId);
  return { sessionListRequest, detailRequest };
}

export function refreshRuntimeWorkflowSession(state: any): any {
  return wrapWorkflowRequest(state, refreshWorkflowSessionDetail(state.workflow));
}

export function clearRuntimeWorkflowSession(state: any): void {
  clearWorkflowSessionSelection(state.workflow);
  resetCollaborationState(state.collaboration);
}

export function isCurrentRuntimeWorkflowSessionRequest(state: any, request: any): boolean {
  return !!request && request.credentialGeneration === state.credentialGeneration &&
    request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
    isCurrentWorkflowSessionDetailRequest(state.workflow, { sessionId: request.sessionId, generation: request.generation });
}

export function adoptRuntimeWorkflowSessionDetail(state: any, request: any, detail: any): boolean {
  if (!isCurrentRuntimeWorkflowSessionRequest(state, request)) return false;
  return adoptWorkflowSessionDetail(
    state.workflow,
    { sessionId: request.sessionId, generation: request.generation },
    detail
  );
}

export function resolveRunnerDisclosure(storedDisclosure: boolean | null, defaultOpen: boolean): boolean {
  return storedDisclosure === null ? defaultOpen : storedDisclosure;
}

export function runtimeWindowShortKey(value: unknown): string {
  const key = String(value || "");
  if (key.length <= 14) return key;
  return key.slice(0, 8) + "…" + key.slice(-4);
}

export function runtimeWindowActivityLabel(
  timestampMs: unknown,
  nowMs: number,
  language?: RuntimeLanguage,
): string {
  const value = Number(timestampMs);
  if (!Number.isFinite(value) || value <= 0) {
    return language === "zh-CN" ? "无 WebPi 活动" : "No WebPi activity";
  }
  const elapsed = Math.max(0, nowMs - value);
  if (elapsed < 1000) return language === "zh-CN" ? "刚刚" : "just now";
  if (elapsed < 60_000) return language === "zh-CN" ? Math.floor(elapsed / 1000) + " 秒前" : Math.floor(elapsed / 1000) + "s ago";
  if (elapsed < 3_600_000) return language === "zh-CN" ? Math.floor(elapsed / 60_000) + " 分钟前" : Math.floor(elapsed / 60_000) + "m ago";
  if (elapsed < 86_400_000) return language === "zh-CN" ? Math.floor(elapsed / 3_600_000) + " 小时前" : Math.floor(elapsed / 3_600_000) + "h ago";
  return language === "zh-CN" ? Math.floor(elapsed / 86_400_000) + " 天前" : Math.floor(elapsed / 86_400_000) + "d ago";
}
