import {
  initialWorkflowSessionState,
  selectWorkflowSession,
  refreshWorkflowSessionDetail,
  clearWorkflowSessionSelection,
  isCurrentWorkflowSessionDetailRequest,
  adoptWorkflowSessionDetail,
} from "./workflow_session_state.js";

export {};

export function initialRuntimeConsoleState(): any {
  return {
    credentialGeneration: 0,
    projectsGeneration: 0,
    selectedProject: "",
    projectGeneration: 0,
    sessionListGeneration: 0,
    workflow: initialWorkflowSessionState(),
  };
}

export function invalidateRuntimeCredential(state: any): void {
  state.credentialGeneration += 1;
  state.projectsGeneration += 1;
  state.selectedProject = "";
  state.projectGeneration += 1;
  state.sessionListGeneration += 1;
  clearWorkflowSessionSelection(state.workflow);
}

export function beginRuntimeCredential(state: any): any {
  invalidateRuntimeCredential(state);
  return refreshRuntimeProjects(state);
}

export function refreshRuntimeProjects(state: any): any {
  state.projectsGeneration += 1;
  return {
    credentialGeneration: state.credentialGeneration,
    projectGeneration: state.projectGeneration,
    generation: state.projectsGeneration,
  };
}

export function isCurrentRuntimeProjectsRequest(state: any, request: any): boolean {
  return !!request &&
    request.credentialGeneration === state.credentialGeneration &&
    request.projectGeneration === state.projectGeneration &&
    request.generation === state.projectsGeneration;
}

export function selectRuntimeProject(state: any, project: string): any {
  state.selectedProject = project;
  state.projectGeneration += 1;
  state.sessionListGeneration += 1;
  clearWorkflowSessionSelection(state.workflow);
  return refreshRuntimeSessionList(state);
}

export function refreshRuntimeSessionList(state: any): any {
  if (!state.selectedProject) {
    return null;
  }
  state.sessionListGeneration += 1;
  return {
    credentialGeneration: state.credentialGeneration,
    project: state.selectedProject,
    projectGeneration: state.projectGeneration,
    generation: state.sessionListGeneration,
  };
}

export function isCurrentRuntimeSessionListRequest(state: any, request: any): boolean {
  return !!request &&
    request.credentialGeneration === state.credentialGeneration &&
    request.project === state.selectedProject &&
    request.projectGeneration === state.projectGeneration &&
    request.generation === state.sessionListGeneration;
}

function wrapWorkflowRequest(state: any, request: any): any {
  if (!request || !state.selectedProject) {
    return null;
  }
  return {
    credentialGeneration: state.credentialGeneration,
    project: state.selectedProject,
    projectGeneration: state.projectGeneration,
    sessionId: request.sessionId,
    generation: request.generation,
  };
}

export function selectRuntimeWorkflowSession(state: any, sessionId: string): any {
  return wrapWorkflowRequest(state, selectWorkflowSession(state.workflow, sessionId));
}

export function refreshRuntimeWorkflowSession(state: any): any {
  return wrapWorkflowRequest(state, refreshWorkflowSessionDetail(state.workflow));
}

export function clearRuntimeWorkflowSession(state: any): void {
  clearWorkflowSessionSelection(state.workflow);
}

export function isCurrentRuntimeWorkflowSessionRequest(state: any, request: any): boolean {
  return !!request &&
    request.credentialGeneration === state.credentialGeneration &&
    request.project === state.selectedProject &&
    request.projectGeneration === state.projectGeneration &&
    isCurrentWorkflowSessionDetailRequest(state.workflow, {
      sessionId: request.sessionId,
      generation: request.generation,
    });
}

export function adoptRuntimeWorkflowSessionDetail(state: any, request: any, detail: any): boolean {
  if (!isCurrentRuntimeWorkflowSessionRequest(state, request)) {
    return false;
  }
  return adoptWorkflowSessionDetail(
    state.workflow,
    { sessionId: request.sessionId, generation: request.generation },
    detail
  );
}
