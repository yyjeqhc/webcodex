// DOM-free selection and response fencing for Workflow Session Console detail.
export function initialWorkflowSessionState() {
    return {
        selectedSessionId: "",
        detailGeneration: 0,
        snapshot: null,
    };
}
export function selectWorkflowSession(state, sessionId) {
    state.selectedSessionId = sessionId;
    state.detailGeneration += 1;
    state.snapshot = null;
    return workflowSessionDetailRequest(state);
}
export function refreshWorkflowSessionDetail(state) {
    if (!state.selectedSessionId) {
        return null;
    }
    state.detailGeneration += 1;
    return workflowSessionDetailRequest(state);
}
export function clearWorkflowSessionSelection(state) {
    state.selectedSessionId = "";
    state.detailGeneration += 1;
    state.snapshot = null;
}
export function workflowSessionDetailRequest(state) {
    if (!state.selectedSessionId) {
        return null;
    }
    return {
        sessionId: state.selectedSessionId,
        generation: state.detailGeneration,
    };
}
export function isCurrentWorkflowSessionDetailRequest(state, request) {
    return !!request &&
        request.sessionId === state.selectedSessionId &&
        request.generation === state.detailGeneration;
}
export function adoptWorkflowSessionDetail(state, request, detail) {
    if (!isCurrentWorkflowSessionDetailRequest(state, request)) {
        return false;
    }
    state.snapshot = detail;
    return true;
}
