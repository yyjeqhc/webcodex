// DOM-free selection, response fencing, and timeline-follow state for Workflow Session detail.

export {};

const FOLLOW_BOTTOM_THRESHOLD_PX = 24;

export function initialWorkflowSessionState(): any {
  return {
    selectedSessionId: "",
    detailGeneration: 0,
    snapshot: null,
    followLatest: true,
  };
}

export function selectWorkflowSession(state: any, sessionId: string): any {
  state.selectedSessionId = sessionId;
  state.detailGeneration += 1;
  state.snapshot = null;
  state.followLatest = true;
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
  state.followLatest = true;
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

export function updateWorkflowSessionFollowFromScroll(
  state: any,
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number
): boolean {
  const distanceFromBottom = Math.max(0, scrollHeight - scrollTop - clientHeight);
  state.followLatest = distanceFromBottom <= FOLLOW_BOTTOM_THRESHOLD_PX;
  return state.followLatest;
}

export function workflowSessionScrollTopAfterRender(
  state: any,
  previousScrollTop: number,
  clientHeight: number,
  scrollHeight: number
): number {
  if (shouldFollowWorkflowSessionLatest(state)) {
    return Math.max(0, scrollHeight - clientHeight);
  }
  return Math.min(Math.max(0, previousScrollTop), Math.max(0, scrollHeight - clientHeight));
}

export function jumpWorkflowSessionToLatest(state: any): void {
  state.followLatest = true;
}

export function shouldFollowWorkflowSessionLatest(state: any): boolean {
  return state.followLatest !== false;
}
