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
function hasPendingAttention(overview) {
    const attention = overview && typeof overview === "object" ? overview.attention : null;
    return ["open_guidance", "open_questions", "open_risks", "open_todos"]
        .some((key) => overviewCount(attention && attention[key]) > 0);
}
function idleAgeLabel(ageSeconds) {
    if (ageSeconds < 60)
        return "<1m";
    const minutes = Math.floor(ageSeconds / 60);
    if (minutes < 60)
        return minutes + "m";
    const hours = Math.floor(minutes / 60);
    if (hours < 24)
        return hours + "h";
    return Math.floor(hours / 24) + "d";
}
function workflowSessionLivenessPresentation(session, nowSeconds = Date.now() / 1000) {
    const runningCall = !!session?.running_call;
    const runningJobs = typeof session?.running_jobs === "number" ? Math.max(0, session.running_jobs) : 0;
    const tooltip = "WebCodex activity only; host/model state is unknown.";
    if (runningCall || runningJobs > 0) {
        return { state: "working", label: "working", tooltip };
    }
    const updatedAt = typeof session?.updated_at === "number" ? session.updated_at : 0;
    const ageSeconds = updatedAt > 0 ? Math.max(0, nowSeconds - updatedAt) : Number.POSITIVE_INFINITY;
    if (ageSeconds <= 120) {
        return { state: "recent", label: "recently active", tooltip };
    }
    if (hasPendingAttention(session?.overview)) {
        return { state: "attention", label: "idle · pending attention", tooltip };
    }
    return {
        state: "idle",
        label: Number.isFinite(ageSeconds) ? "idle · " + idleAgeLabel(ageSeconds) : "idle",
        tooltip,
    };
}
function workflowSessionIdleAttentionLabel(runningCall, overview) {
    return workflowSessionLivenessPresentation({ running_call: runningCall, overview, updated_at: 0 }, 0).label;
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
    return {
        generation: 0,
        sessionId: "",
        messages: [],
        observationToken: "",
        available: true,
        phase: "idle",
    };
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
function projectAttentionCount(project) {
    const attention = project?.sessions?.attention;
    return ["open_guidance", "open_questions", "open_risks", "open_todos"]
        .reduce((total, key) => total + (typeof attention?.[key] === "number" ? Math.max(0, attention[key]) : 0), 0);
}
function filterAndSortRuntimeProjects(projects, clientId, query) {
    const needle = String(query || "").trim().toLocaleLowerCase();
    return runtimeProjectsForDevice(projects, clientId)
        .filter((project) => {
        if (!needle)
            return true;
        return [project?.name, project?.id]
            .filter((value) => typeof value === "string")
            .some((value) => String(value).toLocaleLowerCase().includes(needle));
    })
        .sort((left, right) => {
        const leftRunning = typeof left?.sessions?.running_sessions === "number" ? left.sessions.running_sessions : 0;
        const rightRunning = typeof right?.sessions?.running_sessions === "number" ? right.sessions.running_sessions : 0;
        if (!!rightRunning !== !!leftRunning)
            return rightRunning ? 1 : -1;
        const leftAttention = projectAttentionCount(left);
        const rightAttention = projectAttentionCount(right);
        if (!!rightAttention !== !!leftAttention)
            return rightAttention ? 1 : -1;
        const leftUpdated = typeof left?.sessions?.latest_updated_at === "number" ? left.sessions.latest_updated_at : 0;
        const rightUpdated = typeof right?.sessions?.latest_updated_at === "number" ? right.sessions.latest_updated_at : 0;
        if (leftUpdated !== rightUpdated)
            return rightUpdated - leftUpdated;
        const leftName = typeof left?.name === "string" && left.name ? left.name : left.id;
        const rightName = typeof right?.name === "string" && right.name ? right.name : right.id;
        return compareText(String(leftName || ""), String(rightName || "")) || compareText(String(left?.id || ""), String(right?.id || ""));
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
    state.collaboration.phase = "idle";
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
    state.collaboration.phase = "idle";
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
    state.collaboration.phase = "idle";
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
function setRuntimeCollaborationPhase(state, request, phase) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.phase = phase;
    return true;
}
function runtimeCollaborationNeedsRefreshRecovery(state) {
    return state?.collaboration?.phase === "paused";
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
let runnerProjectRows = [];
let projectSearch = "";
let collaborationReplyTo = "";
let refreshInFlight = false;
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
    return chip;
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
    runnerProjectRows = [];
    projectRowsTruncated = false;
    projectSearch = "";
    collaborationReplyTo = "";
    clearSessionSurface();
    clearNode(el("runtime-project-list"));
    show("runtime-token-gate", true);
    show("runtime-console", false);
    show("runtime-topbar-controls", false);
    stopAuto();
    setText("runtime-token-error", message);
    setText("runtime-refresh-status", "");
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
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        show("runtime-overview-unavailable", true);
        setText("runtime-overview-access", "runtime:read unavailable");
        return true;
    }
    if (!response.ok || !response.data) {
        setText("runtime-overview-access", "refresh unavailable");
        return false;
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
    return true;
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
        return false;
    if (response.status === 401 || response.status === 403) {
        lock("Credential does not have Runtime Console project access.");
        return false;
    }
    if (!response.ok || !response.data) {
        if (unlocking)
            lock("Runtime Console is unavailable.");
        else
            showError("Could not refresh projects.");
        return false;
    }
    projectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
    projectRowsTruncated = !!response.data.truncated;
    unlockUi();
    showError("");
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
        return true;
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
    return true;
}
function effectiveProjects(projects) {
    const aggregates = new Map();
    for (const row of runnerProjectRows) {
        if (row && typeof row.id === "string")
            aggregates.set(row.id, row);
    }
    return (Array.isArray(projects) ? projects : []).map((project) => {
        const aggregate = aggregates.get(String(project?.id || ""));
        return aggregate ? { ...project, sessions: aggregate.sessions } : project;
    });
}
function renderProjectSelectors(projects, truncated) {
    const deviceSelect = el("runtime-device-select");
    const projectList = el("runtime-project-list");
    if (!deviceSelect || !projectList)
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
    const rows = filterAndSortRuntimeProjects(effectiveProjects(projects), String(state.selectedDevice || ""), projectSearch);
    clearNode(projectList);
    show("runtime-projects-empty", !!state.selectedDevice && rows.length === 0);
    for (const project of rows) {
        const row = document.createElement("div");
        row.className = "project-row" + (project.id === state.selectedProject ? " selected" : "");
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", project.id === state.selectedProject ? "true" : "false");
        row.tabIndex = 0;
        const main = document.createElement("div");
        main.className = "project-row-main";
        const title = document.createElement("div");
        title.className = "project-row-title";
        title.textContent = project.name || project.id;
        const id = document.createElement("div");
        id.className = "project-row-id";
        id.textContent = String(project.id || "");
        main.appendChild(title);
        main.appendChild(id);
        const facts = document.createElement("div");
        facts.className = "project-row-facts";
        appendChip(facts, project.connected ? String(project.agent_status || "online") : "offline");
        if (project.sessions) {
            appendChip(facts, countLabel(project.sessions.retained_sessions, "retained Session"));
            if (project.sessions.running_sessions)
                appendChip(facts, countLabel(project.sessions.running_sessions, "working"), "tone-runtime");
            const attention = attentionLabel(project.sessions.attention);
            if (!attention.startsWith("No retained"))
                appendChip(facts, attention, "tone-warn");
            if (typeof project.sessions.latest_updated_at === "number")
                appendChip(facts, "updated " + updatedLabel(project.sessions.latest_updated_at));
        }
        row.appendChild(main);
        row.appendChild(facts);
        const select = () => switchProject(String(state.selectedDevice || ""), String(project.id || ""));
        row.addEventListener("click", select);
        row.addEventListener("keydown", (event) => { if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            select();
        } });
        projectList.appendChild(row);
    }
    const deviceProjects = runtimeProjectsForDevice(projects, String(state.selectedDevice || ""));
    setText("runtime-device-status", devices.length ? countLabel(devices.length, "authorized Runner") + (truncated ? " · bounded project list" : "") : "No authorized Runners");
    setText("runtime-project-status", state.selectedDevice ? countLabel(deviceProjects.length, "authorized Project") + " on this Runner" + (truncated ? " · bounded list" : "") : "No authorized Projects");
}
function switchProject(device, project) {
    abortProjectWork();
    if (state.selectedDevice !== device) {
        abort(runnerAbort);
        runnerAbort = null;
        runnerProjectRows = [];
    }
    collaborationReplyTo = "";
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
        runnerProjectRows = [];
        renderProjectSelectors(projectRows, projectRowsTruncated);
        return;
    }
    if (!response.ok || !response.data) {
        show("runtime-runner-unavailable", true);
        setText("runtime-runner-access", "Runner view unavailable");
        return;
    }
    show("runtime-runner-unavailable", false);
    setText("runtime-runner-access", response.data.projects_truncated ? "bounded Project aggregate" : "runtime:read");
    runnerProjectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
    renderRunner(response.data);
    renderProjectSelectors(projectRows, projectRowsTruncated);
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
        const liveness = workflowSessionLivenessPresentation(session);
        const livenessChip = appendChip(meta, liveness.label, liveness.state === "working" ? "tone-runtime" : liveness.state === "attention" ? "tone-warn" : "");
        livenessChip.title = liveness.tooltip;
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
    setHumanJoinSendEnabled(false);
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
    const liveness = workflowSessionLivenessPresentation(detail);
    setText("runtime-session-running", liveness.label);
    const livenessNode = el("runtime-session-running");
    if (livenessNode)
        livenessNode.title = liveness.tooltip;
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
function collaborationPhaseLabel() {
    switch (state.collaboration.phase) {
        case "live": return "Live";
        case "reconnecting": return "Reconnecting";
        case "paused": return "Paused";
        default: return "Idle";
    }
}
function setCollaborationReplyTarget(messageId) {
    collaborationReplyTo = messageId;
    const reply = el("runtime-message-reply");
    if (reply)
        reply.hidden = !messageId;
    setText("runtime-message-reply-text", messageId ? "Reply to " + messageId : "");
}
function renderCollaboration(statusText) {
    const available = state.collaboration.available !== false;
    show("runtime-collaboration-unavailable", !available);
    show("runtime-collaboration-form", available);
    const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
    show("runtime-collaboration-empty", available && messages.length === 0);
    const status = available
        ? "Collaboration: " + collaborationPhaseLabel() + " · " + countLabel(messages.length, "retained message") + (statusText ? " · " + statusText : "")
        : "runtime:read unavailable";
    setText("runtime-collaboration-status", status);
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
        kind.textContent = String(message?.kind || "message") + " · " + String(message?.priority || "normal") + " · " + String(message?.status || "unknown");
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
        if (message?.requires_ack) {
            const ack = document.createElement("div");
            ack.className = "message-ack";
            ack.textContent = typeof message?.first_ack_observed_at === "number"
                ? "ACK required · First ACK observed " + updatedLabel(message.first_ack_observed_at)
                : "ACK required";
            card.appendChild(ack);
        }
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
        const actions = document.createElement("div");
        actions.className = "message-actions";
        const replyButton = document.createElement("button");
        replyButton.type = "button";
        replyButton.className = "text-button";
        replyButton.textContent = "Reply";
        replyButton.addEventListener("click", () => setCollaborationReplyTarget(id));
        actions.appendChild(replyButton);
        card.appendChild(actions);
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
    // Establish the cursor before the retained snapshot. A mutation between these
    // two reads is then present in the snapshot, the subsequent delta, or both;
    // merge-by-id makes the overlap harmless. Listing first and baselining second
    // would permanently skip a mutation that lands in that gap.
    setRuntimeCollaborationPhase(state, request, "reconnecting");
    renderCollaboration("establishing retained baseline");
    const baseline = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
    if (!baseline || !isCurrentRuntimeCollaborationRequest(state, request))
        return null;
    if (baseline.status === 401) {
        lock("Credential rejected.");
        return null;
    }
    if (baseline.status === 403) {
        setRuntimeCollaborationAvailable(state, request, false);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration();
        return null;
    }
    if (baseline.status === 404) {
        setRuntimeCollaborationAvailable(state, request, false);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("Session unavailable");
        return null;
    }
    if (!baseline.ok || !baseline.data || typeof baseline.data.observation_token !== "string") {
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("observation unavailable");
        return null;
    }
    const response = await api("workflow-session-messages", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
    if (!response || !isCurrentRuntimeCollaborationRequest(state, request))
        return null;
    if (response.status === 401) {
        lock("Credential rejected.");
        return null;
    }
    if (response.status === 403) {
        setRuntimeCollaborationAvailable(state, request, false);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration();
        return null;
    }
    if (response.status === 404) {
        setRuntimeCollaborationAvailable(state, request, false);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("Session unavailable");
        return null;
    }
    if (!response.ok || !response.data) {
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("retained snapshot failed");
        return null;
    }
    setRuntimeCollaborationAvailable(state, request, true);
    if (!adoptRuntimeCollaborationList(state, request, Array.isArray(response.data.messages) ? response.data.messages : []))
        return null;
    adoptRuntimeCollaborationObservation(state, request, baseline.data);
    setRuntimeCollaborationPhase(state, request, "live");
    setHumanJoinSendEnabled(true);
    renderCollaboration("bounded long-poll");
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
            setRuntimeCollaborationPhase(state, request, "paused");
            renderCollaboration();
            break;
        }
        if (!response.ok || !response.data) {
            setRuntimeCollaborationPhase(state, request, "paused");
            renderCollaboration("request failed");
            break;
        }
        const action = runtimeCollaborationObservationAction(response.data);
        if (action === "reload") {
            renderCollaboration("retention changed · reloading");
            observationToken = await loadRetainedCollaboration(request, controller);
            continue;
        }
        if (!adoptRuntimeCollaborationObservation(state, request, response.data))
            break;
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
                if (!drain || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request))
                    break;
                if (!drain.ok || !drain.data) {
                    setRuntimeCollaborationPhase(state, request, "paused");
                    renderCollaboration("delta drain failed");
                    observationToken = null;
                    break;
                }
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
function setHumanJoinSendEnabled(enabled) {
    const send = el("runtime-message-send");
    if (send)
        send.disabled = !enabled;
}
function syncAckComposer() {
    const kind = el("runtime-message-kind");
    const priority = el("runtime-message-priority");
    const checkbox = el("runtime-message-requires-ack");
    const guidance = kind?.value === "guidance";
    show("runtime-message-ack-label", guidance);
    if (!checkbox)
        return;
    checkbox.disabled = !guidance || priority?.value !== "high";
    if (checkbox.disabled)
        checkbox.checked = false;
    checkbox.title = guidance && priority?.value !== "high" ? "ACK requirement is available for High priority guidance." : "";
}
async function postHumanCollaborationMessage(event) {
    event.preventDefault();
    const request = runtimeCollaborationRequest(state);
    if (!request || state.collaboration.available === false)
        return;
    const kind = el("runtime-message-kind");
    const priority = el("runtime-message-priority");
    const body = el("runtime-message-body");
    const checkbox = el("runtime-message-requires-ack");
    const send = el("runtime-message-send");
    const message = body?.value.trim() || "";
    if (!message) {
        setText("runtime-message-send-status", "Enter a message.");
        return;
    }
    if (send)
        send.disabled = true;
    setText("runtime-message-send-status", "Sending…");
    const response = await api("workflow-session-post-message", {
        project: request.project,
        session_id: request.sessionId,
        kind: kind?.value || "note",
        priority: priority?.value || "normal",
        message,
        reply_to: collaborationReplyTo || null,
        requires_ack: !!checkbox?.checked,
    });
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return;
    if (response?.status === 0) {
        abortCollaboration();
        setRuntimeCollaborationPhase(state, request, "paused");
        setText("runtime-message-send-status", "Send outcome unknown. Refresh and review retained messages before retrying.");
        renderCollaboration("send outcome unknown · refresh before retry");
        return;
    }
    if (send)
        send.disabled = false;
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (!response?.ok || !response.data) {
        setText("runtime-message-send-status", "Send failed.");
        return;
    }
    adoptRuntimeCollaborationObservation(state, request, { messages: [response.data] });
    if (body)
        body.value = "";
    setCollaborationReplyTarget("");
    setText("runtime-message-send-status", "Sent.");
    renderCollaboration();
}
function setRefreshBusy(active) {
    refreshInFlight = active;
    const button = el("runtime-refresh");
    if (button) {
        button.disabled = active;
        button.textContent = active ? "Refreshing…" : "Refresh";
    }
}
async function refreshAll() {
    if (!token || refreshInFlight)
        return;
    setRefreshBusy(true);
    setText("runtime-refresh-status", "Refreshing…");
    const recoverCollaboration = runtimeCollaborationNeedsRefreshRecovery(state);
    const overviewRequest = refreshRuntimeOverview(state);
    const projectsRequest = refreshRuntimeProjects(state);
    try {
        const [overviewOk, projectsOk] = await Promise.all([
            fetchOverview(overviewRequest),
            fetchProjects(projectsRequest),
        ]);
        if (!token)
            return;
        if (overviewOk && projectsOk) {
            setText("runtime-refresh-status", "Refreshed " + new Date().toLocaleTimeString());
        }
        else {
            setText("runtime-refresh-status", "Refresh failed · showing previous data");
        }
        if (recoverCollaboration && runtimeCollaborationNeedsRefreshRecovery(state)) {
            const collaborationRequest = runtimeCollaborationRequest(state);
            if (collaborationRequest)
                void startCollaboration(collaborationRequest);
        }
    }
    finally {
        setRefreshBusy(false);
    }
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
    void fetchOverview(refreshRuntimeOverview(state));
    void fetchProjects(request, true);
});
el("runtime-device-select")?.addEventListener("change", () => {
    const select = el("runtime-device-select");
    if (!select)
        return;
    const projects = filterAndSortRuntimeProjects(effectiveProjects(projectRows), select.value, "");
    switchProject(select.value, projects.length ? String(projects[0].id) : "");
});
el("runtime-project-search")?.addEventListener("input", () => {
    const input = el("runtime-project-search");
    projectSearch = input?.value || "";
    renderProjectSelectors(projectRows, projectRowsTruncated);
});
el("runtime-message-kind")?.addEventListener("change", syncAckComposer);
el("runtime-message-priority")?.addEventListener("change", syncAckComposer);
el("runtime-message-reply-clear")?.addEventListener("click", () => setCollaborationReplyTarget(""));
el("runtime-collaboration-form")?.addEventListener("submit", (event) => void postHumanCollaborationMessage(event));
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
syncAckComposer();
window.addEventListener("pagehide", () => { token = ""; abortAll(); stopAuto(); });
lock();
