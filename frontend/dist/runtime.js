// DOM-free selection, response fencing, timeline-follow state, and narrow
// human-facing overview formatting for Workflow Sessions.
const FOLLOW_BOTTOM_THRESHOLD_PX = 24;
function overviewCount(value) {
    return typeof value === "number" && Number.isFinite(value) && value > 0
        ? Math.floor(value)
        : 0;
}
function countLabel(count, singular, plural = singular + "s") {
    return count + " " + (count === 1 ? singular : plural);
}
function validationOverviewFact(validation) {
    const state = String((validation && validation.state) || "unavailable");
    const retained = !!(validation && validation.history_truncated);
    const unresolved = overviewCount(validation && validation.unresolved_failure_count);
    if (state === "failed") {
        return {
            text: unresolved
                ? (retained ? "Retained: " : "") + countLabel(unresolved, "unresolved validation failure")
                : retained
                    ? "Latest retained validation failed"
                    : "Latest validation failed",
            tone: "fail",
        };
    }
    if (state === "passed") {
        return {
            text: retained ? "Latest retained validation passed" : "Latest validation passed",
            tone: "pass",
        };
    }
    if (state === "not_run") {
        return { text: "Validation not run", tone: "muted" };
    }
    return {
        text: retained
            ? "Retained terminal validation evidence unavailable"
            : "Terminal validation evidence unavailable",
        tone: "muted",
    };
}
function attentionOverviewParts(attention) {
    const parts = [];
    for (const [key, singular] of [
        ["open_risks", "risk"],
        ["open_todos", "todo"],
        ["open_questions", "question"],
        ["open_guidance", "guidance"],
    ]) {
        const count = overviewCount(attention && attention[key]);
        if (count) {
            parts.push(countLabel(count, singular));
        }
    }
    return parts;
}
function workOverviewParts(work, limit = 5) {
    const parts = [];
    for (const [key, singular, plural] of [
        ["edits", "edit", "edits"],
        ["validations", "validation", "validations"],
        ["exploration", "exploration", "exploration"],
        ["reviews", "review", "reviews"],
        ["runs", "run", "runs"],
    ]) {
        const count = overviewCount(work && work[key]);
        if (count) {
            parts.push(countLabel(count, singular, plural));
        }
        if (parts.length >= limit) {
            break;
        }
    }
    return parts;
}
function workflowSessionListOverviewFacts(overview) {
    if (!overview || typeof overview !== "object") {
        return [];
    }
    const facts = [validationOverviewFact(overview.validation)];
    const attention = attentionOverviewParts(overview.attention);
    if (attention.length) {
        facts.push({
            text: "Retained: " + attention.slice(0, 2).join(" · "),
            tone: overviewCount(overview.attention && overview.attention.open_risks) ? "fail" : "warn",
        });
    }
    const work = workOverviewParts(overview.work, 2);
    if (work.length) {
        facts.push({
            text: (overview.work && overview.work.history_truncated ? "Recent " : "") + work.join(" · "),
            tone: "runtime",
        });
    }
    return facts.slice(0, 3);
}
function workflowSessionOverviewPresentation(overview) {
    const value = overview && typeof overview === "object" ? overview : {};
    const work = workOverviewParts(value.work);
    const workText = work.length
        ? (value.work && value.work.history_truncated ? "Recent observed work: " : "Observed work: ") +
            work.join(" · ")
        : value.work && value.work.history_truncated
            ? "No work observations in retained events."
            : "No tool activity observed.";
    const validationFact = validationOverviewFact(value.validation);
    const validationParts = [validationFact.text];
    if (value.validation && value.validation.latest_kind) {
        validationParts.push("latest " + String(value.validation.latest_kind));
    }
    const testsRun = overviewCount(value.validation && value.validation.tests_run_count);
    if (testsRun || (value.validation && value.validation.tests_run_count === 0)) {
        validationParts.push(countLabel(testsRun, "test"));
    }
    const unresolved = overviewCount(value.validation && value.validation.unresolved_failure_count);
    if (unresolved && !validationFact.text.includes("unresolved validation failure")) {
        validationParts.push(countLabel(unresolved, "unresolved failure"));
    }
    const attention = attentionOverviewParts(value.attention);
    const attentionText = attention.length
        ? "Retained open messages: " + attention.join(" · ")
        : "No retained open guidance, questions, risks, or todos.";
    const progress = value.reported_progress && typeof value.reported_progress === "object"
        ? value.reported_progress
        : null;
    return {
        workText,
        validationText: validationParts.join(" · "),
        validationTone: validationFact.tone,
        validationAt: value.validation && typeof value.validation.latest_at === "number"
            ? value.validation.latest_at
            : null,
        attentionText,
        attentionTone: overviewCount(value.attention && value.attention.open_risks)
            ? "fail"
            : attention.length
                ? "warn"
                : "muted",
        progressText: progress && progress.text ? String(progress.text) : "No retained model-reported progress.",
        progressAt: progress && typeof progress.reported_at === "number" ? progress.reported_at : null,
    };
}
function workflowSessionIdleAttentionLabel(runningCall, overview) {
    if (runningCall)
        return "running call";
    const attention = overview && typeof overview === "object" ? overview.attention : null;
    const pending = ["open_guidance", "open_questions", "open_risks", "open_todos"]
        .some((key) => overviewCount(attention && attention[key]) > 0);
    return pending ? "idle · pending attention" : "no running call";
}
function initialWorkflowSessionState() {
    return {
        selectedSessionId: "",
        detailGeneration: 0,
        snapshot: null,
        followLatest: true,
    };
}
function selectWorkflowSession(state, sessionId) {
    state.selectedSessionId = sessionId;
    state.detailGeneration += 1;
    state.snapshot = null;
    state.followLatest = true;
    return workflowSessionDetailRequest(state);
}
function refreshWorkflowSessionDetail(state) {
    if (!state.selectedSessionId) {
        return null;
    }
    state.detailGeneration += 1;
    return workflowSessionDetailRequest(state);
}
function clearWorkflowSessionSelection(state) {
    state.selectedSessionId = "";
    state.detailGeneration += 1;
    state.snapshot = null;
    state.followLatest = true;
}
function workflowSessionDetailRequest(state) {
    if (!state.selectedSessionId) {
        return null;
    }
    return {
        sessionId: state.selectedSessionId,
        generation: state.detailGeneration,
    };
}
function isCurrentWorkflowSessionDetailRequest(state, request) {
    return !!request &&
        request.sessionId === state.selectedSessionId &&
        request.generation === state.detailGeneration;
}
function adoptWorkflowSessionDetail(state, request, detail) {
    if (!isCurrentWorkflowSessionDetailRequest(state, request)) {
        return false;
    }
    state.snapshot = detail;
    return true;
}
function updateWorkflowSessionFollowFromScroll(state, scrollTop, clientHeight, scrollHeight) {
    const distanceFromBottom = Math.max(0, scrollHeight - scrollTop - clientHeight);
    state.followLatest = distanceFromBottom <= FOLLOW_BOTTOM_THRESHOLD_PX;
    return state.followLatest;
}
function workflowSessionScrollTopAfterRender(state, previousScrollTop, clientHeight, scrollHeight) {
    if (shouldFollowWorkflowSessionLatest(state)) {
        return Math.max(0, scrollHeight - clientHeight);
    }
    return Math.min(Math.max(0, previousScrollTop), Math.max(0, scrollHeight - clientHeight));
}
function jumpWorkflowSessionToLatest(state) {
    state.followLatest = true;
}
function shouldFollowWorkflowSessionLatest(state) {
    return state.followLatest !== false;
}

function compareText(left, right) {
    return left < right ? -1 : left > right ? 1 : 0;
}
function emptyCollaborationState() {
    return { generation: 0, sessionId: "", messages: [], observationToken: "", available: true };
}
function messageCreatedAt(message) {
    return typeof message?.created_at === "number" ? message.created_at : 0;
}
function mergeRuntimeCollaborationMessages(current, updates) {
    const byId = new Map();
    for (const message of Array.isArray(current) ? current : []) {
        const id = typeof message?.message_id === "string" ? message.message_id : "";
        if (id)
            byId.set(id, message);
    }
    for (const message of Array.isArray(updates) ? updates : []) {
        const id = typeof message?.message_id === "string" ? message.message_id : "";
        if (id)
            byId.set(id, message);
    }
    return Array.from(byId.values()).sort((left, right) => messageCreatedAt(left) - messageCreatedAt(right) ||
        compareText(String(left?.message_id || ""), String(right?.message_id || "")));
}
function runtimeCollaborationObservationAction(payload) {
    if (payload?.history_lost)
        return "reload";
    if (payload?.has_more)
        return "drain";
    return "wait";
}
function runtimeDeviceIds(projects) {
    const devices = new Set();
    for (const project of Array.isArray(projects) ? projects : []) {
        const clientId = typeof project?.client_id === "string" ? project.client_id : "";
        if (clientId)
            devices.add(clientId);
    }
    return Array.from(devices).sort(compareText);
}
function runtimeProjectsForDevice(projects, clientId) {
    return (Array.isArray(projects) ? projects : [])
        .filter((project) => project && project.client_id === clientId && typeof project.id === "string" && project.id)
        .slice()
        .sort((left, right) => {
        const leftName = typeof left.name === "string" && left.name ? left.name : left.id;
        const rightName = typeof right.name === "string" && right.name ? right.name : right.id;
        return compareText(leftName, rightName) || compareText(left.id, right.id);
    });
}
function preferredRuntimeProjectSelection(projects, selectedDevice, selectedProject) {
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
function initialRuntimeConsoleState() {
    return {
        credentialGeneration: 0,
        overviewGeneration: 0,
        projectsGeneration: 0,
        runnerGeneration: 0,
        selectedDevice: "",
        selectedProject: "",
        projectGeneration: 0,
        sessionListGeneration: 0,
        workflow: initialWorkflowSessionState(),
        collaboration: emptyCollaborationState(),
    };
}
function invalidateRuntimeCredential(state) {
    state.credentialGeneration += 1;
    state.overviewGeneration += 1;
    state.projectsGeneration += 1;
    state.runnerGeneration += 1;
    state.selectedDevice = "";
    state.selectedProject = "";
    state.projectGeneration += 1;
    state.sessionListGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
    state.collaboration.generation += 1;
    state.collaboration.sessionId = "";
    state.collaboration.messages = [];
    state.collaboration.observationToken = "";
    state.collaboration.available = true;
}
function beginRuntimeCredential(state) {
    invalidateRuntimeCredential(state);
    return refreshRuntimeProjects(state);
}
function refreshRuntimeOverview(state) {
    state.overviewGeneration += 1;
    return { credentialGeneration: state.credentialGeneration, generation: state.overviewGeneration };
}
function isCurrentRuntimeOverviewRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration && request.generation === state.overviewGeneration;
}
function refreshRuntimeProjects(state) {
    state.projectsGeneration += 1;
    return {
        credentialGeneration: state.credentialGeneration,
        projectGeneration: state.projectGeneration,
        generation: state.projectsGeneration,
    };
}
function isCurrentRuntimeProjectsRequest(state, request) {
    return !!request &&
        request.credentialGeneration === state.credentialGeneration &&
        request.projectGeneration === state.projectGeneration &&
        request.generation === state.projectsGeneration;
}
function refreshRuntimeRunner(state) {
    if (!state.selectedDevice)
        return null;
    state.runnerGeneration += 1;
    return { credentialGeneration: state.credentialGeneration, device: state.selectedDevice, generation: state.runnerGeneration };
}
function isCurrentRuntimeRunnerRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration &&
        request.device === state.selectedDevice && request.generation === state.runnerGeneration;
}
function selectRuntimeProject(state, device, project) {
    if (state.selectedDevice !== device)
        state.runnerGeneration += 1;
    state.selectedDevice = device;
    state.selectedProject = project;
    state.projectGeneration += 1;
    state.sessionListGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
    state.collaboration.generation += 1;
    state.collaboration.sessionId = "";
    state.collaboration.messages = [];
    state.collaboration.observationToken = "";
    state.collaboration.available = true;
    return refreshRuntimeSessionList(state);
}
function refreshRuntimeSessionList(state) {
    if (!state.selectedProject)
        return null;
    state.sessionListGeneration += 1;
    return {
        credentialGeneration: state.credentialGeneration,
        project: state.selectedProject,
        projectGeneration: state.projectGeneration,
        generation: state.sessionListGeneration,
    };
}
function isCurrentRuntimeSessionListRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
        request.generation === state.sessionListGeneration;
}
function wrapWorkflowRequest(state, request) {
    if (!request || !state.selectedProject)
        return null;
    return {
        credentialGeneration: state.credentialGeneration,
        project: state.selectedProject,
        projectGeneration: state.projectGeneration,
        sessionId: request.sessionId,
        generation: request.generation,
    };
}
function selectRuntimeWorkflowSession(state, sessionId) {
    state.collaboration.generation += 1;
    state.collaboration.sessionId = sessionId;
    state.collaboration.messages = [];
    state.collaboration.observationToken = "";
    state.collaboration.available = true;
    return wrapWorkflowRequest(state, selectWorkflowSession(state.workflow, sessionId));
}
function refreshRuntimeWorkflowSession(state) {
    return wrapWorkflowRequest(state, refreshWorkflowSessionDetail(state.workflow));
}
function clearRuntimeWorkflowSession(state) {
    clearWorkflowSessionSelection(state.workflow);
    state.collaboration.generation += 1;
    state.collaboration.sessionId = "";
    state.collaboration.messages = [];
    state.collaboration.observationToken = "";
}
function runtimeCollaborationRequest(state) {
    if (!state.selectedProject || !state.collaboration.sessionId)
        return null;
    return {
        credentialGeneration: state.credentialGeneration,
        project: state.selectedProject,
        projectGeneration: state.projectGeneration,
        sessionId: state.collaboration.sessionId,
        generation: state.collaboration.generation,
    };
}
function isCurrentRuntimeCollaborationRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
        request.sessionId === state.collaboration.sessionId && request.generation === state.collaboration.generation;
}
function adoptRuntimeCollaborationList(state, request, messages) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.messages = mergeRuntimeCollaborationMessages([], messages);
    return true;
}
function adoptRuntimeCollaborationObservation(state, request, payload) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.messages = mergeRuntimeCollaborationMessages(state.collaboration.messages, Array.isArray(payload?.messages) ? payload.messages : []);
    if (typeof payload?.observation_token === "string")
        state.collaboration.observationToken = payload.observation_token;
    return true;
}
function setRuntimeCollaborationAvailable(state, request, available) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.available = available;
    return true;
}
function isCurrentRuntimeWorkflowSessionRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
        isCurrentWorkflowSessionDetailRequest(state.workflow, { sessionId: request.sessionId, generation: request.generation });
}
function adoptRuntimeWorkflowSessionDetail(state, request, detail) {
    if (!isCurrentRuntimeWorkflowSessionRequest(state, request))
        return false;
    return adoptWorkflowSessionDetail(state.workflow, { sessionId: request.sessionId, generation: request.generation }, detail);
}

const API_BASE = "/api/runtime-console/";
const REFRESH_MS = 8000;
const COLLABORATION_WAIT_SECS = 25;
let token = "";
let timer = 0;
let overviewAbort = null;
let projectsAbort = null;
let runnerAbort = null;
let sessionsAbort = null;
let detailAbort = null;
let collaborationAbort = null;
let projectRows = [];
let projectRowsTruncated = false;
let sessionRows = [];
const state = initialRuntimeConsoleState();
function el(id) {
    return document.getElementById(id);
}
function setText(id, value) {
    const node = el(id);
    if (node)
        node.textContent = value === null || value === undefined || value === "" ? "—" : String(value);
}
function show(id, visible) {
    const node = el(id);
    if (node)
        node.hidden = !visible;
}
function clearNode(node) {
    while (node && node.firstChild)
        node.removeChild(node.firstChild);
}
function appendChip(parent, text, extraClass = "") {
    const chip = document.createElement("span");
    chip.className = "chip" + (extraClass ? " " + extraClass : "");
    chip.textContent = text;
    parent.appendChild(chip);
}
function abort(controller) {
    if (controller)
        controller.abort();
}
function abortCollaboration() {
    abort(collaborationAbort);
    collaborationAbort = null;
}
function abortProjectWork() {
    abort(sessionsAbort);
    abort(detailAbort);
    abortCollaboration();
    sessionsAbort = null;
    detailAbort = null;
}
function abortAll() {
    abort(overviewAbort);
    abort(projectsAbort);
    abort(runnerAbort);
    overviewAbort = null;
    projectsAbort = null;
    runnerAbort = null;
    abortProjectWork();
}
async function api(path, payload, signal) {
    try {
        const response = await fetch(API_BASE + path, {
            method: "POST",
            headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" },
            body: JSON.stringify(payload),
            signal,
        });
        let data = null;
        try {
            data = await response.json();
        }
        catch {
            data = null;
        }
        return { ok: response.ok, status: response.status, data };
    }
    catch (error) {
        if (error instanceof DOMException && error.name === "AbortError")
            return null;
        return { ok: false, status: 0, data: null };
    }
}
function hideDetail() {
    show("runtime-session-detail", false);
    show("runtime-session-detail-empty", true);
    show("runtime-jump-latest", false);
    clearNode(el("runtime-collaboration-board"));
}
function clearSessionSurface() {
    sessionRows = [];
    clearNode(el("runtime-session-list"));
    show("runtime-sessions-empty", false);
    clearRuntimeWorkflowSession(state);
    abortCollaboration();
    hideDetail();
}
function lock(message = "") {
    token = "";
    abortAll();
    invalidateRuntimeCredential(state);
    projectRows = [];
    projectRowsTruncated = false;
    clearSessionSurface();
    clearNode(el("runtime-runner-projects"));
    show("runtime-token-gate", true);
    show("runtime-console", false);
    show("runtime-topbar-controls", false);
    stopAuto();
    setText("runtime-token-error", message);
    const input = el("runtime-token-input");
    if (input) {
        input.value = "";
        input.focus();
    }
}
function unlockUi() {
    show("runtime-token-gate", false);
    show("runtime-console", true);
    show("runtime-topbar-controls", true);
    setText("runtime-token-error", "");
    startAuto();
}
function showError(message) {
    setText("runtime-error", message);
    show("runtime-error", !!message);
}
function countLabel(value, singular, plural = singular + "s") {
    const count = typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0;
    return count + " " + (count === 1 ? singular : plural);
}
function attentionLabel(attention) {
    const parts = [];
    for (const [key, singular] of [["open_risks", "risk"], ["open_todos", "todo"], ["open_questions", "question"], ["open_guidance", "guidance"]]) {
        const count = typeof attention?.[key] === "number" ? attention[key] : 0;
        if (count)
            parts.push(countLabel(count, singular));
    }
    return parts.length ? parts.join(" · ") : "No retained pending attention";
}
async function fetchOverview(request) {
    abort(overviewAbort);
    const controller = new AbortController();
    overviewAbort = controller;
    const response = await api("overview", {}, controller.signal);
    if (overviewAbort === controller)
        overviewAbort = null;
    if (!response || !isCurrentRuntimeOverviewRequest(state, request))
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    if (response.status === 403) {
        show("runtime-overview-unavailable", true);
        setText("runtime-overview-access", "runtime:read unavailable");
        return;
    }
    if (!response.ok || !response.data) {
        setText("runtime-overview-access", "refresh unavailable");
        return;
    }
    show("runtime-overview-unavailable", false);
    setText("runtime-overview-access", "runtime:read");
    const data = response.data;
    setText("runtime-server-identity", [data.service, data.version].filter(Boolean).join(" · "));
    setText("runtime-server-build", data.build_git_commit ? "build " + data.build_git_commit + (data.build_git_dirty ? " · dirty" : "") : "build unavailable");
    setText("runtime-server-runners", countLabel(data.runner_count, "Runner"));
    setText("runtime-server-alignment", countLabel(data.runners_online, "online") + " · " + countLabel(data.runners_stale, "stale") + " · " + countLabel(data.runners_unavailable, "unavailable"));
    setText("runtime-server-projects", data.projects_available ? countLabel(data.visible_projects, "visible Project") + (data.projects_truncated ? " +" : "") : "project:read unavailable");
    setText("runtime-server-jobs", countLabel(data.active_jobs, "active Job") + (data.mixed_builds_present ? " · mixed builds" : ""));
    setText("runtime-server-attention", attentionLabel(data.workflow_sessions));
    setText("runtime-server-sessions", countLabel(data.workflow_sessions?.active, "active Session") + " · " + countLabel(data.workflow_sessions?.running, "running call") + (data.workflow_sessions?.truncated ? " · bounded aggregate" : ""));
}
function projectLabel(project) {
    const name = project && project.name ? String(project.name) : "";
    const id = project && project.id ? String(project.id) : "";
    const identity = name && name !== id ? name + " — " + id : id;
    const status = project && project.connected ? String(project.agent_status || "online") : "offline";
    return identity + " · " + status;
}
async function fetchProjects(request, unlocking = false) {
    abort(projectsAbort);
    const controller = new AbortController();
    projectsAbort = controller;
    const response = await api("projects", { limit: 100 }, controller.signal);
    if (projectsAbort === controller)
        projectsAbort = null;
    if (!response || !isCurrentRuntimeProjectsRequest(state, request))
        return;
    if (response.status === 401 || response.status === 403) {
        lock("Credential does not have Runtime Console project access.");
        return;
    }
    if (!response.ok || !response.data) {
        if (unlocking)
            lock("Runtime Console is unavailable.");
        else
            showError("Could not refresh projects.");
        return;
    }
    projectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
    projectRowsTruncated = !!response.data.truncated;
    unlockUi();
    showError("");
    void fetchOverview(refreshRuntimeOverview(state));
    const currentDevice = String(state.selectedDevice || "");
    const currentProject = String(state.selectedProject || "");
    const selection = preferredRuntimeProjectSelection(projectRows, currentDevice, currentProject);
    if (!selection.project) {
        if (currentDevice || currentProject) {
            abortProjectWork();
            selectRuntimeProject(state, selection.device || "", "");
        }
        renderProjectSelectors(projectRows, projectRowsTruncated);
        clearSessionSurface();
        setText("runtime-selected-project", "No project selected");
        const runnerRequest = refreshRuntimeRunner(state);
        if (runnerRequest)
            void fetchRunner(runnerRequest);
        return;
    }
    if (selection.device !== currentDevice || selection.project !== currentProject) {
        switchProject(selection.device, selection.project);
    }
    else {
        renderProjectSelectors(projectRows, projectRowsTruncated);
        const runnerRequest = refreshRuntimeRunner(state);
        if (runnerRequest)
            void fetchRunner(runnerRequest);
        const listRequest = refreshRuntimeSessionList(state);
        if (listRequest)
            void fetchSessions(listRequest);
    }
}
function renderProjectSelectors(projects, truncated) {
    const deviceSelect = el("runtime-device-select");
    const projectSelect = el("runtime-project-select");
    if (!deviceSelect || !projectSelect)
        return;
    const devices = runtimeDeviceIds(projects);
    clearNode(deviceSelect);
    for (const clientId of devices) {
        const option = document.createElement("option");
        option.value = clientId;
        option.textContent = clientId;
        deviceSelect.appendChild(option);
    }
    if (state.selectedDevice)
        deviceSelect.value = state.selectedDevice;
    const deviceProjects = runtimeProjectsForDevice(projects, String(state.selectedDevice || ""));
    clearNode(projectSelect);
    for (const project of deviceProjects) {
        const option = document.createElement("option");
        option.value = project.id;
        option.textContent = projectLabel(project);
        projectSelect.appendChild(option);
    }
    if (state.selectedProject)
        projectSelect.value = state.selectedProject;
    setText("runtime-device-status", devices.length ? countLabel(devices.length, "authorized Runner") + (truncated ? " · bounded project list" : "") : "No authorized Runners");
    setText("runtime-project-status", state.selectedDevice ? countLabel(deviceProjects.length, "authorized Project") + " on this Runner" + (truncated ? " · bounded list" : "") : "No authorized Projects");
}
function switchProject(device, project) {
    abortProjectWork();
    if (state.selectedDevice !== device) {
        abort(runnerAbort);
        runnerAbort = null;
    }
    clearSessionSurface();
    const request = selectRuntimeProject(state, device, project);
    renderProjectSelectors(projectRows, projectRowsTruncated);
    setText("runtime-selected-project", project || "No project selected");
    const runnerRequest = refreshRuntimeRunner(state);
    if (runnerRequest)
        void fetchRunner(runnerRequest);
    if (request)
        void fetchSessions(request);
}
async function fetchRunner(request) {
    abort(runnerAbort);
    const controller = new AbortController();
    runnerAbort = controller;
    const response = await api("runner", { client_id: request.device, project_limit: 24 }, controller.signal);
    if (runnerAbort === controller)
        runnerAbort = null;
    if (!response || !isCurrentRuntimeRunnerRequest(state, request))
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    if (response.status === 403) {
        show("runtime-runner-unavailable", true);
        setText("runtime-runner-access", "runtime:read unavailable");
        clearNode(el("runtime-runner-projects"));
        return;
    }
    if (!response.ok || !response.data) {
        show("runtime-runner-unavailable", true);
        setText("runtime-runner-access", "Runner view unavailable");
        return;
    }
    show("runtime-runner-unavailable", false);
    setText("runtime-runner-access", response.data.projects_truncated ? "bounded Project aggregate" : "runtime:read");
    renderRunner(response.data);
}
function renderRunner(data) {
    setText("runtime-runner-id", data.client_id);
    setText("runtime-runner-health", (data.connected ? "connected" : "disconnected") + " · " + String(data.status || "unknown"));
    setText("runtime-runner-version", data.version ? "v" + data.version : "version unavailable");
    setText("runtime-runner-build", data.build_git_commit ? String(data.build_git_commit) + (data.build_git_dirty ? " · dirty" : "") : "build unavailable");
    setText("runtime-runner-jobs", countLabel(data.active_jobs, "active Job"));
    setText("runtime-runner-concurrency", countLabel(data.jobs_running, "running") + " · " + countLabel(data.jobs_queued, "queued") + (typeof data.job_concurrency_limit === "number" ? " · limit " + data.job_concurrency_limit : ""));
    setText("runtime-runner-alignment", data.source_alignment || "unknown");
    setText("runtime-runner-project-count", data.projects_available ? countLabel(data.visible_project_count, "visible Project") : "project:read unavailable");
    const node = el("runtime-runner-projects");
    clearNode(node);
    if (!node || !Array.isArray(data.projects))
        return;
    for (const project of data.projects) {
        const card = document.createElement("div");
        card.className = "runner-project-card" + (project.id === state.selectedProject ? " selected" : "");
        const title = document.createElement("div");
        title.className = "runner-project-title";
        title.textContent = project.name && project.name !== project.id ? String(project.name) + " — " + String(project.id) : String(project.id || "");
        const meta = document.createElement("div");
        meta.className = "muted small";
        meta.textContent = (project.connected ? String(project.agent_status || "online") : "offline") + " · " + countLabel(project.sessions?.retained_sessions, "retained Session") + (project.sessions?.sessions_truncated ? " · bounded" : "");
        const facts = document.createElement("div");
        facts.className = "summary-facts";
        appendChip(facts, countLabel(project.sessions?.running_sessions, "running"), "tone-runtime");
        const attention = attentionLabel(project.sessions?.attention);
        if (!attention.startsWith("No retained"))
            appendChip(facts, attention, "tone-warn");
        if (typeof project.sessions?.latest_updated_at === "number")
            appendChip(facts, "updated " + updatedLabel(project.sessions.latest_updated_at));
        card.appendChild(title);
        card.appendChild(meta);
        card.appendChild(facts);
        card.addEventListener("click", () => switchProject(String(data.client_id || state.selectedDevice), String(project.id || "")));
        node.appendChild(card);
    }
}
async function fetchSessions(request) {
    abort(sessionsAbort);
    const controller = new AbortController();
    sessionsAbort = controller;
    const response = await api("workflow-sessions", { project: request.project, limit: 50 }, controller.signal);
    if (sessionsAbort === controller)
        sessionsAbort = null;
    if (!response || !isCurrentRuntimeSessionListRequest(state, request))
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    if (response.status === 403 || response.status === 404) {
        showError("Selected project is no longer available.");
        return;
    }
    if (!response.ok || !response.data) {
        showError("Could not refresh Workflow Sessions.");
        return;
    }
    sessionRows = Array.isArray(response.data.sessions) ? response.data.sessions : [];
    renderSessionList(sessionRows, response.data);
    showError("");
    const selected = String(state.workflow.selectedSessionId || "");
    if (selected && sessionRows.some((row) => String(row.session_id || "") === selected)) {
        const detailRequest = refreshRuntimeWorkflowSession(state);
        if (detailRequest)
            void fetchSessionDetail(detailRequest);
    }
    else if (selected) {
        abortCollaboration();
        clearRuntimeWorkflowSession(state);
        hideDetail();
    }
}
function updatedLabel(timestamp) {
    if (typeof timestamp !== "number")
        return "time unavailable";
    return new Date(timestamp * 1000).toLocaleTimeString();
}
function activityKindLabel(activity) {
    const kind = String(activity && activity.kind || "Activity");
    if (activity && activity.job_handoff) {
        if (kind === "Tested")
            return "Test";
        if (kind === "Ran")
            return "Command";
    }
    if (kind === "Explored" && activity && typeof activity.group_count === "number")
        return "Explored ×" + activity.group_count;
    return kind;
}
function activityFacts(activity, includeTiming) {
    const facts = [];
    if (activity && typeof activity.group_count === "number") {
        if (Array.isArray(activity.group_kinds) && activity.group_kinds.length)
            facts.push(activity.group_kinds.map(String).join(" / "));
        if (Array.isArray(activity.group_tools) && activity.group_tools.length)
            facts.push(activity.group_tools.map(String).join(", "));
    }
    else if (activity && activity.tool)
        facts.push(String(activity.tool));
    if (activity && activity.kind === "Progress")
        facts.push("informational");
    else if (activity && activity.job_handoff) {
        facts.push("handed off");
        if (activity.execution_state)
            facts.push("execution " + String(activity.execution_state));
    }
    else if (activity && activity.state)
        facts.push(String(activity.state));
    if (activity && activity.job_id)
        facts.push("job " + String(activity.job_id));
    if (includeTiming && activity && typeof activity.started_at === "number")
        facts.push(new Date(activity.started_at * 1000).toLocaleTimeString());
    return facts;
}
function activityDescription(activity) {
    if (!activity)
        return "";
    const parts = [activityKindLabel(activity), ...activityFacts(activity, false)];
    if (activity.summary && !activity.job_handoff)
        parts.push(String(activity.summary));
    return parts.join(" · ");
}
function appendPreview(parent, label, activity) {
    if (!activity)
        return;
    const row = document.createElement("div");
    row.className = "activity-preview muted small";
    const prefix = document.createElement("span");
    prefix.className = "activity-preview-label";
    prefix.textContent = label;
    const text = document.createElement("span");
    text.textContent = activityDescription(activity);
    row.appendChild(prefix);
    row.appendChild(text);
    parent.appendChild(row);
}
function renderSessionList(sessions, payload) {
    const node = el("runtime-session-list");
    if (!node)
        return;
    clearNode(node);
    show("runtime-sessions-empty", sessions.length === 0);
    const total = typeof payload.total === "number" ? payload.total : sessions.length;
    setText("runtime-sessions-count", total ? sessions.length + (payload.truncated ? " of " + total : "") : "0");
    const selected = String(state.workflow.selectedSessionId || "");
    for (const session of sessions) {
        const id = String(session && session.session_id || "");
        if (!id)
            continue;
        const item = document.createElement("li");
        item.className = "session-card" + (id === selected ? " selected" : "");
        const title = document.createElement("div");
        title.className = "session-title";
        title.textContent = session.title ? String(session.title) : id;
        const meta = document.createElement("div");
        meta.className = "chips";
        appendChip(meta, String(session.lifecycle || "unknown"));
        appendChip(meta, workflowSessionIdleAttentionLabel(!!session.running_call, session.overview));
        appendChip(meta, updatedLabel(session.updated_at));
        item.appendChild(title);
        item.appendChild(meta);
        const facts = workflowSessionListOverviewFacts(session.overview);
        if (facts.length) {
            const summary = document.createElement("div");
            summary.className = "summary-facts";
            for (const fact of facts)
                appendChip(summary, fact.text, "tone-" + fact.tone);
            item.appendChild(summary);
        }
        appendPreview(item, "Now", session.current_activity);
        appendPreview(item, "Last", session.last_activity);
        item.addEventListener("click", () => selectSession(id));
        node.appendChild(item);
    }
}
function selectSession(sessionId) {
    abort(detailAbort);
    detailAbort = null;
    abortCollaboration();
    hideDetail();
    const request = selectRuntimeWorkflowSession(state, sessionId);
    renderSessionList(sessionRows, { total: sessionRows.length, truncated: false });
    if (request)
        void fetchSessionDetail(request);
    const collaborationRequest = runtimeCollaborationRequest(state);
    if (collaborationRequest)
        void startCollaboration(collaborationRequest);
}
async function fetchSessionDetail(request) {
    abort(detailAbort);
    const controller = new AbortController();
    detailAbort = controller;
    const response = await api("workflow-session", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
    if (detailAbort === controller)
        detailAbort = null;
    if (!response || !isCurrentRuntimeWorkflowSessionRequest(state, request))
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    if (response.status === 404) {
        abortCollaboration();
        clearRuntimeWorkflowSession(state);
        hideDetail();
        return;
    }
    if (!response.ok || !response.data) {
        showError("Could not refresh Workflow Session detail.");
        return;
    }
    if (!adoptRuntimeWorkflowSessionDetail(state, request, response.data))
        return;
    renderDetail(response.data);
}
function setTone(id, tone) {
    const node = el(id);
    if (!node)
        return;
    for (const name of ["pass", "warn", "fail", "muted"])
        node.classList.toggle("tone-card-" + name, tone === name);
}
function renderOverview(overview) {
    const view = workflowSessionOverviewPresentation(overview);
    setText("runtime-overview-work", view.workText);
    setText("runtime-overview-validation", view.validationText + (typeof view.validationAt === "number" ? " · " + updatedLabel(view.validationAt) : ""));
    setTone("runtime-overview-validation-card", view.validationTone);
    setText("runtime-overview-attention", view.attentionText);
    setTone("runtime-overview-attention-card", view.attentionTone);
    setText("runtime-overview-progress", view.progressText + (typeof view.progressAt === "number" ? " · reported " + updatedLabel(view.progressAt) : ""));
}
function syncFollowUi() {
    show("runtime-jump-latest", !!state.workflow.selectedSessionId && !shouldFollowWorkflowSessionLatest(state.workflow));
}
function renderDetail(detail) {
    show("runtime-session-detail-empty", false);
    show("runtime-session-detail", true);
    setText("runtime-session-title", detail.title);
    setText("runtime-session-lifecycle", detail.lifecycle);
    setText("runtime-session-mode", "mode " + String(detail.mode || "unknown"));
    setText("runtime-session-running", workflowSessionIdleAttentionLabel(!!detail.running_call, detail.overview));
    setText("runtime-session-updated", "Updated " + updatedLabel(detail.updated_at));
    renderOverview(detail.overview);
    renderCollaboration();
    const activities = Array.isArray(detail.activity) ? detail.activity : [];
    const node = el("runtime-timeline");
    const previousScrollTop = node ? node.scrollTop : 0;
    clearNode(node);
    show("runtime-timeline-empty", activities.length === 0);
    if (!node)
        return syncFollowUi();
    for (const activity of activities) {
        const item = document.createElement("li");
        item.className = "timeline-event";
        if (activity && activity.kind === "Progress")
            item.classList.add("reported-progress");
        if (activity && ["failed", "timed_out"].includes(String(activity.state || "")))
            item.classList.add("failed");
        const head = document.createElement("div");
        head.className = "timeline-head";
        const kind = document.createElement("span");
        kind.className = "timeline-kind";
        kind.textContent = activityKindLabel(activity);
        const meta = document.createElement("span");
        meta.className = "muted small";
        meta.textContent = activityFacts(activity, true).join(" · ");
        head.appendChild(kind);
        head.appendChild(meta);
        item.appendChild(head);
        if (activity && activity.summary) {
            const body = document.createElement("div");
            body.className = "timeline-body small";
            body.textContent = String(activity.summary);
            item.appendChild(body);
        }
        if (activity && Array.isArray(activity.paths) && activity.paths.length) {
            const paths = document.createElement("div");
            paths.className = "muted small";
            paths.textContent = activity.paths.map(String).join(" · ");
            item.appendChild(paths);
        }
        node.appendChild(item);
    }
    node.scrollTop = workflowSessionScrollTopAfterRender(state.workflow, previousScrollTop, node.clientHeight, node.scrollHeight);
    syncFollowUi();
}
function renderCollaboration(statusText) {
    const available = state.collaboration.available !== false;
    show("runtime-collaboration-unavailable", !available);
    const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
    show("runtime-collaboration-empty", available && messages.length === 0);
    setText("runtime-collaboration-status", statusText || (available ? countLabel(messages.length, "retained message") : "runtime:read unavailable"));
    const node = el("runtime-collaboration-board");
    clearNode(node);
    if (!node || !available)
        return;
    const byId = new Map();
    const children = new Map();
    for (const message of messages) {
        const id = String(message?.message_id || "");
        if (id)
            byId.set(id, message);
    }
    for (const message of messages) {
        const parent = typeof message?.reply_to === "string" ? message.reply_to : "";
        if (parent && byId.has(parent)) {
            const list = children.get(parent) || [];
            list.push(message);
            children.set(parent, list);
        }
    }
    const visited = new Set();
    const appendMessage = (message, depth, parentUnavailable) => {
        const id = String(message?.message_id || "");
        if (!id || visited.has(id))
            return;
        visited.add(id);
        const card = document.createElement("article");
        card.className = "message-card " + String(message?.kind || "note") + (String(message?.status || "") === "resolved" ? " resolved" : "") + (parentUnavailable ? " retained-reply" : "");
        if (depth > 0)
            card.classList.add("message-thread");
        const head = document.createElement("div");
        head.className = "message-head";
        const kind = document.createElement("span");
        kind.className = "message-kind";
        kind.textContent = String(message?.kind || "message") + " · " + String(message?.status || "unknown") + " · " + String(message?.priority || "normal");
        const time = document.createElement("span");
        time.className = "muted small";
        time.textContent = updatedLabel(message?.created_at);
        head.appendChild(kind);
        head.appendChild(time);
        card.appendChild(head);
        const meta = document.createElement("div");
        meta.className = "message-meta";
        const metaParts = [id];
        if (message?.author_session_id)
            metaParts.push("author " + String(message.author_session_id));
        meta.textContent = metaParts.join(" · ");
        card.appendChild(meta);
        if (parentUnavailable) {
            const unavailable = document.createElement("div");
            unavailable.className = "message-links";
            unavailable.textContent = "retained reply · parent unavailable";
            card.appendChild(unavailable);
        }
        else if (message?.reply_to) {
            const reply = document.createElement("div");
            reply.className = "message-links";
            reply.textContent = "reply to " + String(message.reply_to);
            card.appendChild(reply);
        }
        const body = document.createElement("div");
        body.className = "message-body";
        body.textContent = String(message?.message || "");
        card.appendChild(body);
        if (message?.resolved_at || message?.resolution || message?.resolved_by_message_id) {
            const resolution = document.createElement("div");
            resolution.className = "message-resolution";
            const parts = [];
            if (message.resolved_at)
                parts.push("resolved " + updatedLabel(message.resolved_at));
            if (message.resolution)
                parts.push(String(message.resolution));
            if (message.resolved_by_message_id)
                parts.push("by " + String(message.resolved_by_message_id));
            resolution.textContent = parts.join(" · ");
            card.appendChild(resolution);
        }
        node.appendChild(card);
        for (const child of children.get(id) || [])
            appendMessage(child, depth + 1, false);
    };
    for (const message of messages) {
        const parent = typeof message?.reply_to === "string" ? message.reply_to : "";
        if (!parent || !byId.has(parent))
            appendMessage(message, 0, !!parent);
    }
    for (const message of messages)
        appendMessage(message, 0, false);
}
async function loadRetainedCollaboration(request, controller) {
    const response = await api("workflow-session-messages", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
    if (!response || !isCurrentRuntimeCollaborationRequest(state, request))
        return null;
    if (response.status === 401) {
        lock("Credential rejected.");
        return null;
    }
    if (response.status === 403) {
        setRuntimeCollaborationAvailable(state, request, false);
        renderCollaboration();
        return null;
    }
    if (response.status === 404) {
        setRuntimeCollaborationAvailable(state, request, false);
        renderCollaboration("Session collaboration unavailable");
        return null;
    }
    if (!response.ok || !response.data) {
        renderCollaboration("Collaboration refresh failed");
        return null;
    }
    setRuntimeCollaborationAvailable(state, request, true);
    if (!adoptRuntimeCollaborationList(state, request, Array.isArray(response.data.messages) ? response.data.messages : []))
        return null;
    renderCollaboration("Establishing live baseline…");
    const baseline = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
    if (!baseline || !isCurrentRuntimeCollaborationRequest(state, request))
        return null;
    if (baseline.status === 401) {
        lock("Credential rejected.");
        return null;
    }
    if (baseline.status === 403) {
        setRuntimeCollaborationAvailable(state, request, false);
        renderCollaboration();
        return null;
    }
    if (!baseline.ok || !baseline.data || typeof baseline.data.observation_token !== "string") {
        renderCollaboration("Live observation unavailable");
        return null;
    }
    adoptRuntimeCollaborationObservation(state, request, baseline.data);
    renderCollaboration("Live · bounded long-poll");
    return baseline.data.observation_token;
}
async function startCollaboration(request) {
    abortCollaboration();
    const controller = new AbortController();
    collaborationAbort = controller;
    let observationToken = await loadRetainedCollaboration(request, controller);
    while (observationToken && collaborationAbort === controller && isCurrentRuntimeCollaborationRequest(state, request)) {
        const response = await api("workflow-session-observe", {
            project: request.project,
            session_id: request.sessionId,
            after_observation_token: observationToken,
            wait_secs: COLLABORATION_WAIT_SECS,
            limit: 100,
        }, controller.signal);
        if (!response || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request))
            break;
        if (response.status === 401) {
            lock("Credential rejected.");
            break;
        }
        if (response.status === 403) {
            setRuntimeCollaborationAvailable(state, request, false);
            renderCollaboration();
            break;
        }
        if (!response.ok || !response.data) {
            renderCollaboration("Live refresh paused after request failure");
            break;
        }
        const action = runtimeCollaborationObservationAction(response.data);
        if (action === "reload") {
            renderCollaboration("Retention changed · reloading retained board…");
            observationToken = await loadRetainedCollaboration(request, controller);
            continue;
        }
        if (!adoptRuntimeCollaborationObservation(state, request, response.data))
            break;
        observationToken = String(response.data.observation_token || observationToken);
        renderCollaboration(action === "drain" ? "Live · draining retained changes…" : "Live · bounded long-poll");
        if (action === "drain") {
            const drain = await api("workflow-session-observe", {
                project: request.project,
                session_id: request.sessionId,
                after_observation_token: observationToken,
                limit: 100,
            }, controller.signal);
            if (!drain || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request))
                break;
            if (!drain.ok || !drain.data) {
                renderCollaboration("Delta drain failed");
                break;
            }
            if (runtimeCollaborationObservationAction(drain.data) === "reload") {
                observationToken = await loadRetainedCollaboration(request, controller);
                continue;
            }
            adoptRuntimeCollaborationObservation(state, request, drain.data);
            observationToken = String(drain.data.observation_token || observationToken);
            renderCollaboration(drain.data.has_more ? "Live · draining retained changes…" : "Live · bounded long-poll");
            while (drain.data.has_more) {
                const more = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, after_observation_token: observationToken, limit: 100 }, controller.signal);
                if (!more || !more.ok || !more.data || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request))
                    return;
                if (more.data.history_lost) {
                    observationToken = await loadRetainedCollaboration(request, controller);
                    break;
                }
                adoptRuntimeCollaborationObservation(state, request, more.data);
                observationToken = String(more.data.observation_token || observationToken);
                drain.data = more.data;
                renderCollaboration(more.data.has_more ? "Live · draining retained changes…" : "Live · bounded long-poll");
            }
        }
    }
    if (collaborationAbort === controller)
        collaborationAbort = null;
}
function jumpLatest() {
    jumpWorkflowSessionToLatest(state.workflow);
    const node = el("runtime-timeline");
    if (node)
        node.scrollTop = node.scrollHeight;
    syncFollowUi();
}
async function refreshAll() {
    if (!token)
        return;
    void fetchOverview(refreshRuntimeOverview(state));
    await fetchProjects(refreshRuntimeProjects(state));
}
function startAuto() {
    stopAuto();
    timer = window.setInterval(() => {
        const request = refreshRuntimeSessionList(state);
        if (request)
            void fetchSessions(request);
    }, REFRESH_MS);
}
function stopAuto() { if (timer)
    window.clearInterval(timer); timer = 0; }
el("runtime-token-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const input = el("runtime-token-input");
    const nextToken = input ? input.value.trim() : "";
    if (input)
        input.value = "";
    if (!nextToken) {
        setText("runtime-token-error", "Enter a runtime Bearer credential.");
        return;
    }
    token = nextToken;
    const request = beginRuntimeCredential(state);
    void fetchProjects(request, true);
});
el("runtime-device-select")?.addEventListener("change", () => {
    const select = el("runtime-device-select");
    if (!select)
        return;
    const projects = runtimeProjectsForDevice(projectRows, select.value);
    switchProject(select.value, projects.length ? String(projects[0].id) : "");
});
el("runtime-project-select")?.addEventListener("change", () => {
    const select = el("runtime-project-select");
    if (select)
        switchProject(String(state.selectedDevice || ""), select.value);
});
el("runtime-refresh")?.addEventListener("click", () => void refreshAll());
el("runtime-lock")?.addEventListener("click", () => lock());
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
    const node = el("runtime-timeline");
    if (!node)
        return;
    updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight);
    syncFollowUi();
});
window.addEventListener("pagehide", () => { token = ""; abortAll(); stopAuto(); });
lock();
