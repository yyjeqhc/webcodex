import { initialWorkflowSessionState, selectWorkflowSession, refreshWorkflowSessionDetail, clearWorkflowSessionSelection, isCurrentWorkflowSessionDetailRequest, adoptWorkflowSessionDetail, } from "./workflow_session_state.js";
export function initialRuntimeConsoleState() {
    return {
        credentialGeneration: 0,
        projectsGeneration: 0,
        selectedProject: "",
        projectGeneration: 0,
        sessionListGeneration: 0,
        workflow: initialWorkflowSessionState(),
    };
}
export function invalidateRuntimeCredential(state) {
    state.credentialGeneration += 1;
    state.projectsGeneration += 1;
    state.selectedProject = "";
    state.projectGeneration += 1;
    state.sessionListGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
}
export function beginRuntimeCredential(state) {
    invalidateRuntimeCredential(state);
    return refreshRuntimeProjects(state);
}
export function refreshRuntimeProjects(state) {
    state.projectsGeneration += 1;
    return {
        credentialGeneration: state.credentialGeneration,
        projectGeneration: state.projectGeneration,
        generation: state.projectsGeneration,
    };
}
export function isCurrentRuntimeProjectsRequest(state, request) {
    return !!request &&
        request.credentialGeneration === state.credentialGeneration &&
        request.projectGeneration === state.projectGeneration &&
        request.generation === state.projectsGeneration;
}
export function selectRuntimeProject(state, project) {
    state.selectedProject = project;
    state.projectGeneration += 1;
    state.sessionListGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
    return refreshRuntimeSessionList(state);
}
export function refreshRuntimeSessionList(state) {
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
export function isCurrentRuntimeSessionListRequest(state, request) {
    return !!request &&
        request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject &&
        request.projectGeneration === state.projectGeneration &&
        request.generation === state.sessionListGeneration;
}
function wrapWorkflowRequest(state, request) {
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
export function selectRuntimeWorkflowSession(state, sessionId) {
    return wrapWorkflowRequest(state, selectWorkflowSession(state.workflow, sessionId));
}
export function refreshRuntimeWorkflowSession(state) {
    return wrapWorkflowRequest(state, refreshWorkflowSessionDetail(state.workflow));
}
export function clearRuntimeWorkflowSession(state) {
    clearWorkflowSessionSelection(state.workflow);
}
export function isCurrentRuntimeWorkflowSessionRequest(state, request) {
    return !!request &&
        request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject &&
        request.projectGeneration === state.projectGeneration &&
        isCurrentWorkflowSessionDetailRequest(state.workflow, {
            sessionId: request.sessionId,
            generation: request.generation,
        });
}
export function adoptRuntimeWorkflowSessionDetail(state, request, detail) {
    if (!isCurrentRuntimeWorkflowSessionRequest(state, request)) {
        return false;
    }
    return adoptWorkflowSessionDetail(state.workflow, { sessionId: request.sessionId, generation: request.generation }, detail);
}
