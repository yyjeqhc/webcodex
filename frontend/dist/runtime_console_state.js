import { initialWorkflowSessionState, selectWorkflowSession, refreshWorkflowSessionDetail, clearWorkflowSessionSelection, isCurrentWorkflowSessionDetailRequest, adoptWorkflowSessionDetail, } from "./workflow_session_state.js";
function compareText(left, right) {
    return left < right ? -1 : left > right ? 1 : 0;
}
export function runtimeDeviceIds(projects) {
    const devices = new Set();
    for (const project of Array.isArray(projects) ? projects : []) {
        const clientId = typeof project?.client_id === "string" ? project.client_id : "";
        if (clientId)
            devices.add(clientId);
    }
    return Array.from(devices).sort(compareText);
}
export function runtimeProjectsForDevice(projects, clientId) {
    return (Array.isArray(projects) ? projects : [])
        .filter((project) => project && project.client_id === clientId && typeof project.id === "string" && project.id)
        .slice()
        .sort((left, right) => {
        const leftName = typeof left.name === "string" && left.name ? left.name : left.id;
        const rightName = typeof right.name === "string" && right.name ? right.name : right.id;
        return compareText(leftName, rightName) || compareText(left.id, right.id);
    });
}
export function preferredRuntimeProjectSelection(projects, selectedDevice, selectedProject) {
    const rows = Array.isArray(projects) ? projects : [];
    if (selectedProject) {
        const retained = rows.find((project) => project && project.id === selectedProject && typeof project.client_id === "string" && project.client_id);
        if (retained)
            return { device: retained.client_id, project: retained.id };
    }
    const devices = runtimeDeviceIds(rows);
    const device = devices.includes(selectedDevice) ? selectedDevice : devices[0] || "";
    const project = runtimeProjectsForDevice(rows, device)[0];
    return { device, project: project ? project.id : "" };
}
export function initialRuntimeConsoleState() {
    return {
        credentialGeneration: 0,
        projectsGeneration: 0,
        selectedDevice: "",
        selectedProject: "",
        projectGeneration: 0,
        sessionListGeneration: 0,
        workflow: initialWorkflowSessionState(),
    };
}
export function invalidateRuntimeCredential(state) {
    state.credentialGeneration += 1;
    state.projectsGeneration += 1;
    state.selectedDevice = "";
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
export function selectRuntimeProject(state, device, project) {
    state.selectedDevice = device;
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
