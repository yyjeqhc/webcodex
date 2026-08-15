// DOM-free selection, response fencing, and timeline-follow state for Workflow Session detail.
const FOLLOW_BOTTOM_THRESHOLD_PX = 24;
export function initialWorkflowSessionState() {
    return {
        selectedSessionId: "",
        detailGeneration: 0,
        snapshot: null,
        followLatest: true,
    };
}
export function selectWorkflowSession(state, sessionId) {
    state.selectedSessionId = sessionId;
    state.detailGeneration += 1;
    state.snapshot = null;
    state.followLatest = true;
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
    state.followLatest = true;
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
export function updateWorkflowSessionFollowFromScroll(state, scrollTop, clientHeight, scrollHeight) {
    const distanceFromBottom = Math.max(0, scrollHeight - scrollTop - clientHeight);
    state.followLatest = distanceFromBottom <= FOLLOW_BOTTOM_THRESHOLD_PX;
    return state.followLatest;
}
export function workflowSessionScrollTopAfterRender(state, previousScrollTop, clientHeight, scrollHeight) {
    if (shouldFollowWorkflowSessionLatest(state)) {
        return Math.max(0, scrollHeight - clientHeight);
    }
    return Math.min(Math.max(0, previousScrollTop), Math.max(0, scrollHeight - clientHeight));
}
export function jumpWorkflowSessionToLatest(state) {
    state.followLatest = true;
}
export function shouldFollowWorkflowSessionLatest(state) {
    return state.followLatest !== false;
}
