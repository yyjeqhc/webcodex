// DOM-free selection and response fencing for Workflow Session Console detail.

export {};

export function initialWorkflowSessionState(): any {
  return {
    selectedSessionId: "",
    detailGeneration: 0,
    snapshot: null,
  };
}

export function selectWorkflowSession(state: any, sessionId: string): any {
  state.selectedSessionId = sessionId;
  state.detailGeneration += 1;
  state.snapshot = null;
  return workflowSessionDetailRequest(state);
}

export function refreshWorkflowSessionDetail(state: any): any {
  if (!state.selectedSessionId) {
    return null;
  }
  state.detailGeneration += 1;
  return workflowSessionDetailRequest(state);
}

export function clearWorkflowSessionSelection(state: any): void {
  state.selectedSessionId = "";
  state.detailGeneration += 1;
  state.snapshot = null;
}

export function workflowSessionDetailRequest(state: any): any {
  if (!state.selectedSessionId) {
    return null;
  }
  return {
    sessionId: state.selectedSessionId,
    generation: state.detailGeneration,
  };
}

export function isCurrentWorkflowSessionDetailRequest(state: any, request: any): boolean {
  return !!request &&
    request.sessionId === state.selectedSessionId &&
    request.generation === state.detailGeneration;
}

export function adoptWorkflowSessionDetail(state: any, request: any, detail: any): boolean {
  if (!isCurrentWorkflowSessionDetailRequest(state, request)) {
    return false;
  }
  state.snapshot = detail;
  return true;
}
