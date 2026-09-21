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
    const tooltip = "WebPi activity only; host/model state is unknown.";
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

function compareCollaborationText(left, right) {
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
        replyTargetId: "",
        editTargetId: "",
        uncertainMutation: null,
        mutationNotice: "",
    };
}
function resetCollaborationState(collaboration) {
    if (!collaboration)
        return;
    collaboration.generation += 1;
    collaboration.sessionId = "";
    collaboration.messages = [];
    collaboration.observationToken = "";
    collaboration.available = true;
    collaboration.phase = "idle";
    collaboration.replyTargetId = "";
    collaboration.editTargetId = "";
    collaboration.uncertainMutation = null;
    collaboration.mutationNotice = "";
}
function messageCreatedAt(message) {
    return typeof message?.created_at === "number" ? message.created_at : 0;
}
const RUNTIME_COLLABORATION_MUTABLE_KINDS = new Set(["note", "guidance", "question", "todo"]);
function runtimeCollaborationMessageCanMutate(message) {
    return !!message && message.status === "open" && RUNTIME_COLLABORATION_MUTABLE_KINDS.has(String(message.kind || ""));
}
function runtimeCollaborationMessageSides(messages, locallyAuthoredMessageIds = new Set()) {
    const sides = new Map();
    for (const message of Array.isArray(messages) ? messages : []) {
        const id = typeof message?.message_id === "string" ? message.message_id : "";
        if (!id)
            continue;
        const side = message?.author_session_id
            ? "incoming"
            : locallyAuthoredMessageIds.has(id)
                ? "outgoing"
                : "neutral";
        sides.set(id, side);
    }
    return sides;
}
function collaborationMessageById(state, messageId) {
    return (Array.isArray(state?.collaboration?.messages) ? state.collaboration.messages : [])
        .find((message) => String(message?.message_id || "") === messageId) || null;
}
function reconcileRuntimeCollaborationMutationState(state, authoritativeRefresh = false) {
    const collaboration = state.collaboration;
    const uncertain = collaboration.uncertainMutation;
    if (uncertain) {
        const original = collaborationMessageById(state, String(uncertain.messageId || ""));
        const confirmedWithdraw = uncertain.kind === "withdraw" && original?.closure_kind === "withdrawn";
        let confirmedReplace = false;
        if (uncertain.kind === "replace") {
            const replacementId = original?.closure_kind === "superseded"
                ? String(original?.superseded_by_message_id || "")
                : "";
            const linkedReplacement = replacementId ? collaborationMessageById(state, replacementId) : null;
            const retainedReplacement = linkedReplacement || (Array.isArray(collaboration.messages)
                ? collaboration.messages.find((message) => message?.supersedes_message_id === uncertain.messageId && message?.message === uncertain.message)
                : null);
            confirmedReplace = !!retainedReplacement
                && retainedReplacement?.supersedes_message_id === uncertain.messageId
                && retainedReplacement?.message === uncertain.message;
        }
        if (confirmedWithdraw || confirmedReplace) {
            collaboration.mutationNotice = confirmedWithdraw
                ? "Withdraw observed after refresh; exact replay required to confirm durability."
                : "Replacement observed after refresh; exact replay required to confirm durability.";
        }
        else if (authoritativeRefresh) {
            collaboration.mutationNotice = "Outcome not observed in retained messages; exact replay required before live observation resumes.";
        }
    }
    if (collaboration.editTargetId) {
        const target = collaborationMessageById(state, collaboration.editTargetId);
        if (!runtimeCollaborationMessageCanMutate(target)) {
            collaboration.editTargetId = "";
            if (!collaboration.mutationNotice) {
                collaboration.mutationNotice = "Message changed while editing; current retained state was refreshed.";
            }
        }
    }
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
        compareCollaborationText(String(left?.message_id || ""), String(right?.message_id || "")));
}
function runtimeCollaborationObservationAction(payload) {
    if (payload?.history_lost)
        return "reload";
    if (payload?.has_more)
        return "drain";
    return "wait";
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
function setRuntimeCollaborationReplyTarget(state, messageId) {
    state.collaboration.replyTargetId = String(messageId || "");
    if (state.collaboration.replyTargetId)
        state.collaboration.editTargetId = "";
}
function setRuntimeCollaborationEditTarget(state, messageId) {
    const id = String(messageId || "");
    const message = collaborationMessageById(state, id);
    if (!id || !runtimeCollaborationMessageCanMutate(message))
        return false;
    state.collaboration.editTargetId = id;
    state.collaboration.replyTargetId = "";
    state.collaboration.mutationNotice = "";
    return true;
}
function clearRuntimeCollaborationEditTarget(state) {
    state.collaboration.editTargetId = "";
}
function runtimeCollaborationEditTarget(state) {
    const id = String(state?.collaboration?.editTargetId || "");
    return id ? collaborationMessageById(state, id) : null;
}
function markRuntimeCollaborationMutationUncertain(state, request, mutation) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.uncertainMutation = {
        kind: mutation?.kind === "replace" ? "replace" : "withdraw",
        messageId: String(mutation?.messageId || ""),
        ...(mutation?.kind === "replace" ? { message: String(mutation?.message || "") } : {}),
    };
    state.collaboration.mutationNotice = "Outcome unknown; refresh retained messages before retrying.";
    return true;
}
function runtimeCollaborationMutationRecovery(state, request) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return null;
    const mutation = state?.collaboration?.uncertainMutation;
    const messageId = String(mutation?.messageId || "");
    if (!mutation || !messageId)
        return null;
    return {
        kind: mutation.kind === "replace" ? "replace" : "withdraw",
        messageId,
        ...(mutation.kind === "replace" ? { message: String(mutation.message || "") } : {}),
    };
}
function completeRuntimeCollaborationMutationRecovery(state, request, notice) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.uncertainMutation = null;
    state.collaboration.mutationNotice = String(notice || "");
    return true;
}
function takeRuntimeCollaborationMutationNotice(state) {
    const notice = String(state?.collaboration?.mutationNotice || "");
    state.collaboration.mutationNotice = "";
    return notice;
}
function adoptRuntimeCollaborationList(state, request, messages) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.messages = mergeRuntimeCollaborationMessages([], messages);
    reconcileRuntimeCollaborationMutationState(state, true);
    return true;
}
function adoptRuntimeCollaborationObservation(state, request, payload) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.messages = mergeRuntimeCollaborationMessages(state.collaboration.messages, Array.isArray(payload?.messages) ? payload.messages : []);
    if (typeof payload?.observation_token === "string")
        state.collaboration.observationToken = payload.observation_token;
    reconcileRuntimeCollaborationMutationState(state, false);
    return true;
}
function setRuntimeCollaborationAvailable(state, request, available) {
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    state.collaboration.available = available;
    if (!available) {
        state.collaboration.editTargetId = "";
        state.collaboration.replyTargetId = "";
    }
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

class RuntimeCommunicationRefreshCoordinator {
    constructor(runRefresh) {
        this.runRefresh = runRefresh;
        this.generation = 0;
        this.inFlight = null;
    }
    refresh(includeData = true) {
        const generation = this.generation;
        const current = this.inFlight;
        if (current && current.generation === generation) {
            if (!includeData || current.includeData)
                return current.promise;
            return current.promise.then(() => this.generation === generation ? this.refresh(true) : false, () => this.generation === generation ? this.refresh(true) : false);
        }
        const promise = Promise.resolve().then(() => this.runRefresh(includeData));
        const started = { includeData, generation, promise };
        this.inFlight = started;
        const clear = () => {
            if (this.inFlight === started)
                this.inFlight = null;
        };
        void promise.then(clear, clear);
        return promise;
    }
    reset() {
        this.generation += 1;
        this.inFlight = null;
    }
}
function runtimeCommunicationTranscriptAfterSeq(lastSeq, limit = 100) {
    const normalizedLastSeq = typeof lastSeq === "number" && Number.isSafeInteger(lastSeq)
        ? Math.max(0, lastSeq)
        : 0;
    const normalizedLimit = Number.isSafeInteger(limit) && limit > 0 ? limit : 100;
    return Math.max(0, normalizedLastSeq - normalizedLimit);
}

function resolveRuntimeContextPresentationMode(isWideViewport, isMobileViewport) {
    if (isMobileViewport)
        return "sheet";
    if (isWideViewport)
        return "docked";
    return "popover";
}
function resolveRuntimeContextState(options) {
    const presentationMode = resolveRuntimeContextPresentationMode(options.isWideViewport, options.isMobileViewport);
    const sessionAvailable = options.hasSelectedSession && options.workspaceView === "sessions";
    if (!sessionAvailable) {
        return {
            visible: false,
            presentationMode,
            isDocked: false,
        };
    }
    const visible = options.userIntent !== null
        ? options.userIntent
        : options.isWideViewport;
    const isDocked = visible && presentationMode === "docked";
    return {
        visible,
        presentationMode,
        isDocked,
    };
}
function reduceRuntimeContextUserIntent(_previousIntent, action) {
    switch (action.type) {
        case "toggle_trigger":
            return !action.currentVisible;
        case "explicit_open":
            return true;
        case "explicit_close":
            return false;
    }
}
function resolveRuntimeContextFocusTransition(options) {
    if (!options.wasDocked && options.nextDocked && options.isTriggerFocused) {
        return "inspector_close";
    }
    return "none";
}

function runtimeWindowAvailabilityAfterHttpResponse(status, ok, hasData) {
    if (status === 403)
        return "unavailable";
    return ok && hasData ? "available" : "stale";
}
function compareText(left, right) {
    return left < right ? -1 : left > right ? 1 : 0;
}
function runtimeWorkflowSessionSummaryRevision(session) {
    if (!session)
        return "";
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
function runtimeWorkflowSessionSummaryChanged(previous, next) {
    return runtimeWorkflowSessionSummaryRevision(previous) !== runtimeWorkflowSessionSummaryRevision(next);
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
        .filter((project) => project && (!clientId || project.client_id === clientId) && typeof project.id === "string" && project.id)
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
        return [project?.name, project?.id, project?.client_id, project?.path]
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
function runtimeProjectIdentityText(project) {
    if (!project || typeof project.id !== "string" || !project.id)
        return "No project selected";
    const runner = typeof project.client_id === "string" && project.client_id ? project.client_id : "unknown";
    const path = typeof project.path === "string" && project.path ? project.path : "unavailable";
    return "Runner: " + runner + " · Project: " + project.id + " · Workspace: " + path;
}
function preferredRuntimeProjectSelection(projects, selectedDevice, selectedProject) {
    const rows = Array.isArray(projects) ? projects : [];
    if (selectedProject) {
        const retained = rows.find((project) => project && project.id === selectedProject && typeof project.client_id === "string" && project.client_id);
        if (retained)
            return { device: retained.client_id, project: retained.id };
    }
    const devices = runtimeDeviceIds(rows);
    const device = devices.includes(selectedDevice) ? selectedDevice : "";
    return { device, project: "" };
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
        projectWindowsGeneration: 0,
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
    state.projectWindowsGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
    resetCollaborationState(state.collaboration);
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
function refreshRuntimeProjects(state, query = "", clientId = state.selectedDevice) {
    state.projectsGeneration += 1;
    return {
        credentialGeneration: state.credentialGeneration,
        projectGeneration: state.projectGeneration,
        generation: state.projectsGeneration,
        clientId: String(clientId || ""),
        query: String(query || "").trim(),
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
function selectRuntimeRunnerFilter(state, device) {
    selectRuntimeProject(state, device, "");
}
function selectRuntimeProject(state, device, project) {
    if (state.selectedDevice !== device)
        state.runnerGeneration += 1;
    state.selectedDevice = device;
    state.selectedProject = project;
    state.projectGeneration += 1;
    state.sessionListGeneration += 1;
    state.projectWindowsGeneration += 1;
    clearWorkflowSessionSelection(state.workflow);
    resetCollaborationState(state.collaboration);
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
function refreshRuntimeProjectWindows(state) {
    if (!state.selectedProject)
        return null;
    state.projectWindowsGeneration += 1;
    return {
        credentialGeneration: state.credentialGeneration,
        project: state.selectedProject,
        projectGeneration: state.projectGeneration,
        generation: state.projectWindowsGeneration,
    };
}
function isCurrentRuntimeProjectWindowsRequest(state, request) {
    return !!request && request.credentialGeneration === state.credentialGeneration &&
        request.project === state.selectedProject && request.projectGeneration === state.projectGeneration &&
        request.generation === state.projectWindowsGeneration;
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
    resetCollaborationState(state.collaboration);
    state.collaboration.sessionId = sessionId;
    return wrapWorkflowRequest(state, selectWorkflowSession(state.workflow, sessionId));
}
function selectRuntimeSessionLocation(state, device, project, sessionId) {
    const sessionListRequest = selectRuntimeProject(state, device, project);
    const detailRequest = selectRuntimeWorkflowSession(state, sessionId);
    return { sessionListRequest, detailRequest };
}
function refreshRuntimeWorkflowSession(state) {
    return wrapWorkflowRequest(state, refreshWorkflowSessionDetail(state.workflow));
}
function clearRuntimeWorkflowSession(state) {
    clearWorkflowSessionSelection(state.workflow);
    resetCollaborationState(state.collaboration);
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
function resolveRunnerDisclosure(storedDisclosure, defaultOpen) {
    return storedDisclosure === null ? defaultOpen : storedDisclosure;
}
function runtimeWindowShortKey(value) {
    const key = String(value || "");
    if (key.length <= 14)
        return key;
    return key.slice(0, 8) + "…" + key.slice(-4);
}
function runtimeWindowActivityLabel(timestampMs, nowMs, language) {
    const value = Number(timestampMs);
    if (!Number.isFinite(value) || value <= 0) {
        return language === "zh-CN" ? "无 WebPi 活动" : "No WebPi activity";
    }
    const elapsed = Math.max(0, nowMs - value);
    if (elapsed < 1000)
        return language === "zh-CN" ? "刚刚" : "just now";
    if (elapsed < 60000)
        return language === "zh-CN" ? Math.floor(elapsed / 1000) + " 秒前" : Math.floor(elapsed / 1000) + "s ago";
    if (elapsed < 3600000)
        return language === "zh-CN" ? Math.floor(elapsed / 60000) + " 分钟前" : Math.floor(elapsed / 60000) + "m ago";
    if (elapsed < 86400000)
        return language === "zh-CN" ? Math.floor(elapsed / 3600000) + " 小时前" : Math.floor(elapsed / 3600000) + "h ago";
    return language === "zh-CN" ? Math.floor(elapsed / 86400000) + " 天前" : Math.floor(elapsed / 86400000) + "d ago";
}

const LANGUAGE_STORAGE_KEY = "webpi.runtime.language.v1";
const RUNTIME_ZH_TEXT = {
    "Updates automatically": "自动更新",
    "Open a Window to see its project and Workflow Sessions.": "打开窗口，查看关联项目与工作会话。",
    "Window Details": "窗口详情",
    "Tool diagnostics": "工具诊断",
    "Observed": "已观察到",
    "In progress": "进行中",
    "Last activity": "最近活动",
    "WebPi — Workspace": "WebPi — 工作区",
    "Your projects and work, in one place.": "项目与工作，尽在此处。",
    "Workspace": "工作区",
    "WebPi Ready": "WebPi 已就绪",
    "Home": "首页",
    "Access key": "访问密钥",
    "Use your access key to open this workspace.": "输入访问密钥，打开工作区。",
    "Remember for this tab": "在此标签页保持登录",
    "Advanced": "高级",
    "The key stays in this tab and is cleared when you lock the workspace or close the tab.": "密钥仅保留在此标签页，锁定工作区或关闭标签页时清除。",
    "Add Project": "添加项目",
    "Open Project": "打开项目",
    "All Projects": "全部项目",
    "Search projects": "搜索项目",
    "Recent Projects": "最近项目",
    "Recent Activity": "最近活动",
    "Current": "当前项目",
    "Git branch": "Git 分支",
    "active sessions": "个活跃会话",
    "Running": "运行中",
    "Not checked": "尚未检查",
    "No activity observed yet": "尚未观察到活动",
    "No projects yet": "尚无项目",
    "No matching projects": "没有匹配的项目",
    "No Git repository": "非 Git 项目",
    "Loading projects…": "正在加载项目…",
    "Projects unavailable. Refresh to try again.": "暂时无法加载项目，请刷新重试。",
    "Activity unavailable. Refresh to try again.": "暂时无法加载活动，请刷新重试。",
    "Reading project files": "查看项目文件",
    "Editing files": "编辑文件",
    "Reviewing changes": "检查更改",
    "Running checks": "运行检查",
    "Running tasks": "运行任务",
    "Workspace activity": "工作区活动",
    "Project folder": "项目文件夹",
    "Absolute folder path on the selected Runner": "所选 Runner 上的绝对文件夹路径",
    "Adding project…": "正在添加项目…",
    "The result could not be confirmed. Refresh Projects before trying again.": "无法确认操作结果，请刷新项目列表后再重试。",
    "Project could not be added. Check the folder and Runner access.": "未能添加项目，请检查文件夹及 Runner 访问权限。",
    "Could not refresh. Check the connection and try again.": "暂时无法刷新，请检查连接后重试。",
    "Extensions": "扩展",
    "Instructions": "指令",
    "Global instructions": "全局指令",
    "Available": "可用",
    "Unavailable": "暂不可用",
    "Registered": "已登记",
    "Nothing installed yet": "尚未安装",
    "No instructions configured": "尚未配置指令",
    "Add a project to manage its extensions.": "添加项目后即可管理扩展。",
    "Showing recent results": "显示最近的结果",
    "Reload": "重新加载",
    "Reload could not be confirmed. Refresh before trying again.": "无法确认重新加载的结果，请刷新后再重试。",
    "tools": "个工具",
    "Windows": "窗口",
    "Open": "打开",
    "Close": "关闭",
    "Workspace status": "工作区状态",
    "Enter your access key.": "请输入访问密钥。",
    "Your access key is no longer valid. Connect again.": "访问密钥已失效，请重新连接。",
    "Open a project, then choose a Workflow Session.": "打开项目，然后选择工作会话。",
    "About Session messages": "关于会话消息",
    "Session messages are not available with this access key.": "当前访问密钥无法查看会话消息。",
    "Session activity and associated Windows are available in Details.": "会话活动和关联窗口可在详情中查看。",
    "Session refresh unavailable. Refresh to try again.": "无法刷新会话。请点击刷新重试。",
    "Loading work Sessions…": "正在加载工作会话…",
    "Project overview": "项目概览",
    "Work Sessions": "工作会话",
    "Diagnostics & Agents": "诊断与 Agent",
    "Choose where to work": "选择工作项目",
    "Review observed work, then choose a Session to continue.": "查看已观测的工作，再选择会话继续。",
    "Find a project, review recent work, or inspect client activity.": "查找项目、查看近期工作，或检查客户端活动。",
    "Runner disconnected. Open diagnostics to check the connection.": "运行器已断开。请打开诊断检查连接。",
    "Needs attention": "待处理",
    "No attention requests in loaded Sessions.": "已加载的会话中没有待处理请求。",
    "Working now": "正在工作",
    "No active work observed in loaded Sessions.": "已加载的会话中未观测到正在执行的工作。",
    "Recently closed": "最近关闭",
    "No closed Sessions in this retained view.": "此留存视图中没有已关闭的会话。",
    "Recent work Sessions": "近期工作会话",
    "Choose a project to load its work Sessions.": "选择项目以加载其工作会话。",
    "Untitled Session": "未命名会话",
    "More Sessions are available in the sidebar.": "可在侧边栏查看其他会话。",
    "Loaded evidence only; counts may be bounded.": "仅显示已加载的证据；计数可能受留存范围限制。",
    "Client calls, separate from work Sessions. Observation does not mean the host is online.": "客户端调用，与工作会话分别展示。观测到活动不代表主机在线。",
    "Browse Window activity": "浏览窗口活动",
    "Observed work": "已观测的工作",
    "Running Jobs": "运行中的作业",
    "Files in retained edits": "留存编辑涉及的文件",
    "No file edits in loaded activity.": "已加载的活动中没有文件编辑。",
    "Recent Job evidence": "近期作业证据",
    "No Jobs in loaded activity.": "已加载的活动中没有作业。",
    "Recently completed": "最近完成",
    "No successful completions in loaded activity.": "已加载的活动中没有成功完成的记录。",
    "Work summary": "工作摘要",
    "Retained evidence, not a live Git diff. Open Context for activity and linked Windows.": "留存证据，并非实时 Git 差异。打开上下文查看活动和关联窗口。",
    "Work Sessions in this Project": "此项目中的工作会话",
    "Commands · ⇧⌘ / Ctrl K": "命令 · ⇧⌘ / Ctrl K",
    "Commands": "命令",
    "Find a destination": "查找目标",
    "Destinations": "导航目标",
    "Close commands": "关闭命令菜单",
    "No matching destinations.": "没有匹配的目标。",
    "partial scan": "扫描不完整",
    "Your workspace": "你的工作空间",
    "Pick up where work happens": "从这里继续工作",
    "Find a project": "查找项目",
    "Find a project…": "查找项目…",
    "Runtime overview": "运行概览",
    "WebPi — Runtime Console": "WebPi — 运行控制台",
    "WebPi Runtime Console": "WebPi 运行控制台",
    "A local workspace for Projects, Sessions, and collaboration": "用于管理项目、会话与协作的本地工作空间",
    "Appearance": "外观",
    "Choose appearance": "选择外观",
    "Color mode": "颜色模式",
    "System": "跟随系统",
    "Light": "浅色",
    "Dark": "深色",
    "Local runtime": "本地运行时",
    "Connect to your workspace": "连接到你的工作空间",
    "Enter an existing runtime Bearer credential. Project and Workflow Session views require their existing scopes; durable Agent Chat separately requires communication:read and communication:manage.": "输入已有的运行时 Bearer 凭证。项目和工作流会话视图需要相应权限；持久 Agent 对话还需要 communication:read 和 communication:manage。",
    "Runtime Bearer credential": "运行时 Bearer 凭证",
    "Connect": "连接",
    "Keep me signed in for this tab (survives refresh, clears on Lock or tab close)": "在此标签页保持登录（刷新后仍有效，锁定或关闭标签页时清除）",
    "Project and Session navigation": "项目与会话导航",
    "Local": "本地",
    "Close project navigation": "关闭项目导航",
    "Projects & Sessions": "项目与会话",
    "Workspace views": "工作空间视图",
    "Connected": "已连接",
    "Runtime & Agents": "运行时与 Agent",
    "Runtime workspace": "运行时工作区",
    "Inspect infrastructure and manage durable Agents without mixing administration into the current Session.": "检查基础设施并管理持久 Agent，同时避免将管理操作混入当前会话。",
    "Overview": "概览",
    "Details": "详情",
    "Session context views": "会话上下文视图",
    "Server health and fleet capacity": "服务器健康状态与设备群容量",
    "Runner fleet": "运行器设备群",
    "Devices, builds and current load": "设备、构建与当前负载",
    "Agents, inboxes and conversations": "Agent、收件箱与对话",
    "Local control plane": "本地控制平面",
    "Server health, Runner capacity, durable Agent identity, inboxes, and conversations.": "查看服务器健康状态、运行器容量、持久 Agent 标识、收件箱与对话。",
    "Live updates": "实时更新",
    "Infrastructure": "基础设施",
    "Devices": "设备",
    "Durable communication": "持久通信",
    "Agents & Conversations": "Agent 与对话",
    "Session conversation": "会话对话",
    "New messages": "新消息",
    "new message": "条新消息",
    "new messages": "条新消息",
    "Show full message": "展开完整消息",
    "Collapse message": "收起消息",
    "Copy code": "复制代码",
    "Code copied": "代码已复制",
    "Unable to copy code": "无法复制代码",
    "connected": "已连接",
    "Projects": "项目",
    "Runner": "运行器",
    "Latest Agent message": "最近的 Agent 留言",
    "Search loaded Sessions": "搜索已加载会话",
    "Title, id, or lifecycle": "标题、ID 或生命周期",
    "Search retained messages": "搜索已保留消息",
    "Message, resolution, or id": "消息内容、处理说明或 ID",
    "This board shows retained Session messages. ACK is not a reply or completion. Host chat replies appear here only when explicitly posted to this Session.": "这里展示会话中保留的协作消息。ACK 不代表回复或完成；宿主聊天中的回复只有明确发布到此会话后才会显示。",
    "All Runners": "全部运行器",
    "Filter by Project name, id, Runner, or workspace path": "按项目名称、ID、运行器或工作空间路径筛选",
    "No project selected": "尚未选择项目",
    "No Projects match this filter.": "没有符合当前筛选条件的项目。",
    "Window activity": "窗口活动",
    "Project Window activity": "项目窗口活动",
    "No Window activity recorded for this project.": "此项目没有记录到窗口活动。",
    "Window activity requires runtime:read. Project-scoped Session access remains available.": "查看窗口活动需要 runtime:read 权限；仍可访问项目范围内的会话。",
    "Sessions": "会话",
    "Workflow Sessions": "工作会话",
    "No retained Workflow Sessions for this project.": "此项目没有保留的工作流会话。",
    "Working & Recently Updated Sessions": "正在工作与最近更新的会话",
    "Working and recently updated Workflow Sessions": "正在工作与最近更新的工作流会话",
    "No recent Workflow Sessions are visible.": "当前没有可见的最近工作流会话。",
    "Fleet-wide recent Sessions require runtime:read. Project-scoped Session access remains available.": "查看整个设备群的最近会话需要 runtime:read；仍可访问项目范围内的会话。",
    "Local Runtime": "本地运行时",
    "Credential stays in this tab only": "凭证仅保留在此标签页",
    "Open project navigation": "打开项目导航",
    "Current location": "当前位置",
    "Fleet": "设备群",
    "Select a Session": "选择一个会话",
    "Refresh runtime": "刷新运行时",
    "Lock": "锁定",
    "Choose a project and Workflow Session from the sidebar to inspect its context and continue the collaboration.": "从侧边栏选择项目和工作流会话，以查看上下文并继续协作。",
    "Conversation": "对话",
    "Collaboration messages require runtime:read. Existing project/session observability remains available.": "协作消息需要 runtime:read；现有的项目和会话观察能力仍可使用。",
    "Start this Session conversation": "开始此会话的对话",
    "Messages posted here are retained on the Session collaboration board.": "此处发送的消息会保留在会话协作板中。",
    "Retained board only; this is not a permanent or complete chat history. ACK observed is server-side evidence of an explicit echo, not a delivery or read receipt.": "这里只展示保留的协作板内容，并非永久或完整的聊天记录。已观察到 ACK 仅表示服务端收到明确回显，不代表送达或已读。",
    "Clear reply": "清除回复",
    "Cancel Edit": "取消编辑",
    "Message this Session…": "给此会话发送消息…",
    "Options": "选项",
    "Message options": "消息选项",
    "Applied to this message": "应用于本条消息",
    "Kind": "类型",
    "Note": "备注",
    "Guidance": "指导",
    "Question": "问题",
    "Todo": "待办",
    "Priority": "优先级",
    "Low": "低",
    "Normal": "普通",
    "High": "高",
    "Require acknowledgement": "需要确认",
    "Send message": "发送消息",
    "More actions": "更多操作",
    "Language": "语言",
    "Refresh": "刷新",
    "Runtime details": "运行时详情",
    "Session context": "会话上下文",
    "Close runtime details": "关闭运行时详情",
    "Close session context": "关闭会话上下文",
    "Context": "上下文",
    "Live": "实时",
    "Session": "会话",
    "Selected context": "已选上下文",
    "Workflow Session identity": "工作流会话标识",
    "Session ID": "会话 ID",
    "Lifecycle": "生命周期",
    "Mode": "模式",
    "Created": "创建时间",
    "Updated": "更新时间",
    "Workflow Session overview": "工作流会话概览",
    "Details & activity": "详情与活动",
    "IDs, validation, timeline": "标识、验证与时间线",
    "Work": "工作",
    "Validation": "验证",
    "Attention": "待处理",
    "Reported progress": "已报告进度",
    "Model-reported; informational only.": "由模型报告，仅供参考。",
    "Activity": "活动",
    "Jump to latest": "跳到最新",
    "No bounded activity is available.": "没有可用的有界活动记录。",
    "Server overview": "服务器概览",
    "Server": "服务器",
    "Runners": "运行器",
    "Collaboration attention": "协作待处理项",
    "Runtime-wide overview is unavailable to this credential; project-scoped Console access remains available.": "此凭证无法查看运行时全局概览；仍可使用项目范围的控制台访问。",
    "Runner Fleet": "运行器设备群",
    "No caller-visible Runners.": "没有调用方可见的运行器。",
    "Runtime-wide Runner facts require runtime:read.": "运行时全局运行器信息需要 runtime:read。",
    "Durable Agent Chat": "持久 Agent 对话",
    "Durable Conversation transcript, recipient-specific Inbox, and coalesced Wake Intent state. While Runtime & Agents is visible, the Console refreshes communication every 30 seconds; attached Endpoint leases are renewed every 30 seconds even outside that view. Polling, renewal, refresh, or unload cleanup does not invoke or wake a model.": "展示持久对话记录、收件人专属收件箱与合并后的唤醒意图状态。显示“运行时与 Agent”时，控制台每 30 秒刷新通信数据；即使离开该视图，已附加端点的租约仍每 30 秒续期。轮询、续期、刷新或卸载清理都不会调用或唤醒模型。",
    "Choose “Continue as this Agent” to bind this browser window to one durable Agent. The Console can poll and renew that bounded Endpoint lease, but it has no production model-resume adapter: pending Wake Intents remain durable until an explicit Host/model activation.": "选择“以此 Agent 继续”可将当前浏览器窗口绑定到一个持久 Agent。控制台可以轮询并续期该有界端点租约，但没有生产模型恢复适配器；待处理的唤醒意图会一直持久保留，直到主机或模型被明确激活。",
    "Durable Agent Chat requires communication:read. Project and Workflow Session access remain independent.": "持久 Agent 对话需要 communication:read；项目与工作流会话访问彼此独立。",
    "Agents": "Agent",
    "Handle": "标识名",
    "Display name": "显示名称",
    "Description": "描述",
    "What this Agent mainly does": "此 Agent 的主要职责",
    "Specialty labels": "专长标签",
    "Create Agent": "创建 Agent",
    "Durable Agents": "持久 Agent",
    "No durable Agents are owned by this communication principal.": "此通信主体尚未拥有持久 Agent。",
    "Agent Card": "Agent 卡片",
    "Update Agent Card": "更新 Agent 卡片",
    "No browser Endpoint attached.": "尚未附加浏览器端点。",
    "Attach this browser": "附加此浏览器",
    "Continue as this Agent": "以此 Agent 继续",
    "Detach": "分离",
    "Selected Agent Inbox": "所选 Agent 收件箱",
    "Consume visible": "消费可见项",
    "Select and attach an Agent to inspect recipient-specific queued deliveries.": "选择并附加一个 Agent，以查看收件人专属的排队投递。",
    "Conversations": "对话",
    "Title": "标题",
    "Agent IDs": "Agent ID",
    "Select an Agent or enter comma-separated wc_dagent_* ids": "选择 Agent，或输入以逗号分隔的 wc_dagent_* ID",
    "Create Conversation": "创建对话",
    "Durable Conversations": "持久对话",
    "No Conversations are visible to this Human principal.": "此人工主体目前没有可见对话。",
    "No messages yet.": "暂无消息。",
    "Inbox recipients": "收件箱接收方",
    "Blank = all Agent participants; empty delivery can be sent with [] through the API": "留空表示所有 Agent 参与者；可通过 API 使用 [] 发送不投递到收件箱的消息",
    "Send a Human-authored durable message…": "发送一条由人工撰写的持久消息…",
    "Send as the selected Agent through its exact attached Endpoint": "通过精确附加的端点，以所选 Agent 身份发送",
    "Send a durable message…": "发送一条持久消息…",
    "Send durable message": "发送持久消息",
    "Select or create a Conversation.": "选择或创建一个对话。",
    "Show more": "展开更多",
    "Recent Sessions": "最近会话",
    "Switch to Chinese": "切换到中文",
    "System appearance": "跟随系统外观",
    "Light appearance": "浅色外观",
    "Dark appearance": "深色外观",
    "No retained pending attention": "没有保留的待处理项",
    "No visible Projects": "没有可见项目",
    "Conversation access unavailable": "对话访问不可用",
    "This credential can inspect the Project and Session, but retained messages require runtime:read.": "此凭证可以查看项目和会话，但查看保留消息需要 runtime:read。",
    "Conversation access requires runtime:read": "对话访问需要 runtime:read",
    "Replace message": "替换消息",
    "Reply": "回复",
    "Replying to": "回复",
    "Original message unavailable": "原消息不可用",
    "You": "你",
    "Agent": "Agent",
    "Retained message": "保留消息",
    "Author provenance unavailable": "作者来源不可用",
    "Edit": "编辑",
    "Delete": "删除",
    "Consume": "消费",
    "Untitled Conversation": "未命名对话",
    "No description.": "暂无描述。",
    "time unavailable": "时间不可用",
    "working": "工作中",
    "recently active": "最近活跃",
    "idle · pending attention": "空闲 · 有待处理项",
    "idle": "空闲",
    "WebPi activity only; host/model state is unknown.": "仅反映 WebPi 活动；主机与模型状态未知。",
    "Now": "当前",
    "Last": "上次",
    "Reconnecting": "正在重连",
    "Paused": "已暂停",
    "Idle": "空闲",
    "OFFLINE": "离线",
    "online": "在线",
    "offline": "离线",
    "stale": "状态过期",
    "unknown": "未知",
    "note": "备注",
    "guidance": "指导",
    "question": "问题",
    "todo": "待办",
    "low": "低",
    "normal": "普通",
    "high": "高",
    "open": "开放",
    "resolved": "已解决",
    "Acknowledgement required": "需要确认",
    "Acknowledged": "已确认",
    "Withdrawn": "已撤回",
    "Replaced": "已替换",
    "Resolved": "已解决",
    "active": "活跃",
    "completed": "已完成",
    "none": "无",
    "attached": "已附加",
    "detached": "已分离",
    "expired": "已过期",
    "queued": "排队中",
    "consumed": "已消费",
    "passed": "已通过",
    "failed": "失败",
    "runtime:read unavailable": "runtime:read 不可用",
    "refresh unavailable": "刷新不可用",
    "project:read unavailable": "project:read 不可用",
    "build unavailable": "构建信息不可用",
    "Credential rejected.": "凭证已被拒绝。",
    "Credential does not have Runtime Console project access.": "此凭证没有运行控制台的项目访问权限。",
    "Runtime Console is unavailable.": "运行控制台当前不可用。",
    "Could not refresh projects.": "无法刷新项目。",
    "Selected project is no longer available.": "所选项目已不可用。",
    "Could not refresh Workflow Sessions.": "无法刷新工作流会话。",
    "Could not refresh Workflow Session detail.": "无法刷新工作流会话详情。",
    "Enter a runtime Bearer credential.": "请输入运行时 Bearer 凭证。",
    "Searching…": "正在搜索…",
    "Refreshing…": "刷新中…",
    "Refreshed": "已刷新",
    "Refresh failed · showing previous data": "刷新失败 · 正在显示之前的数据",
    "Refreshing runtime": "正在刷新运行时",
    "Restoring this tab…": "正在恢复此标签页…",
    "Reply target cleared.": "已清除回复目标。",
    "Edit cancelled.": "已取消编辑。",
    "Enter a message.": "请输入消息。",
    "Sending…": "正在发送…",
    "Sent.": "已发送。",
    "Send failed.": "发送失败。",
    "Delete failed.": "删除失败。",
    "Replace failed.": "替换失败。",
    "Withdrawing retained message…": "正在撤回保留消息…",
    "Message changed before Delete. Refresh retained messages before retrying.": "删除前消息已发生变化。请刷新保留消息后再重试。",
    "Retained message withdrawn.": "保留消息已撤回。",
    "Replacing retained message…": "正在替换保留消息…",
    "Message changed before Replace. Refresh retained messages before retrying.": "替换前消息已发生变化。请刷新保留消息后再重试。",
    "Send outcome unknown. Refresh and review retained messages before retrying.": "发送结果未知。请先刷新并检查保留消息，再决定是否重试。",
    "Refreshing durable communication…": "正在刷新持久通信…",
    "Handle and display name are required.": "标识名和显示名称不能为空。",
    "Creating durable Agent…": "正在创建持久 Agent…",
    "Updating Agent Card…": "正在更新 Agent 卡片…",
    "Outcome uncertain. Refresh the Card before deciding whether to retry.": "操作结果不确定。请刷新 Agent 卡片后再决定是否重试。",
    "Agent Card update failed; refresh before retrying a stale revision.": "Agent 卡片更新失败；请刷新后再重试，避免使用过期版本。",
    "Agent Card updated.": "Agent 卡片已更新。",
    "Releasing this window’s previous Agent Endpoint…": "正在释放此窗口之前的 Agent 端点…",
    "Previous Endpoint detach is uncertain. Refresh before switching this window to another Agent.": "之前端点的分离结果不确定。请刷新后再将此窗口切换到其他 Agent。",
    "Could not release the previous Agent Endpoint.": "无法释放之前的 Agent 端点。",
    "The exact Attach replay was already replaced. Choose “Continue as this Agent” again to create a fresh Endpoint generation.": "这次精确附加重放已被替代。请再次选择“以此 Agent 继续”，创建新的端点代数。",
    "communication:manage required.": "需要 communication:manage 权限。",
    "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key, or refresh before deciding.": "操作结果不确定。请保持输入不变并重试以复用同一幂等键，或先刷新再决定。",
    "Attaching browser Endpoint…": "正在附加浏览器端点…",
    "Outcome uncertain. Retry Attach to replay the same idempotency key; do not create a new attachment.": "附加结果不确定。请重试附加以复用同一幂等键，不要创建新的附加记录。",
    "Detaching browser Endpoint…": "正在分离浏览器端点…",
    "Detach outcome uncertain. Refresh before retry; the durable Agent and Inbox are unaffected.": "分离结果不确定。请刷新后再重试；持久 Agent 和收件箱不受影响。",
    "At least one Agent id is required.": "至少需要一个 Agent ID。",
    "Creating durable Conversation…": "正在创建持久对话…",
    "Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key.": "操作结果不确定。请保持输入不变并重试以复用同一幂等键。",
    "Select a Conversation and enter a message.": "请选择一个对话并输入消息。",
    "Select an Agent and choose “Continue as this Agent” before sending as it.": "请先选择一个 Agent 并点击“以此 Agent 继续”，然后再以其身份发送。",
    "Appending Message and Agent deliveries atomically…": "正在以原子方式写入消息和 Agent 投递…",
    "Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key, or refresh the transcript first.": "操作结果不确定。请保持消息不变，仅在复用同一幂等键时重试，或先刷新对话记录。",
    "Consuming recipient state…": "正在消费接收方状态…",
    "communication:manage required to consume deliveries.": "消费投递需要 communication:manage 权限。",
    "Consume outcome uncertain. Refresh before retry; desired-state replay is safe.": "消费结果不确定。请刷新后再重试；目标状态重放是安全的。",
    "Delivery consume failed.": "消费投递失败。",
    "Existing idempotent Agent replayed.": "已重放现有的幂等 Agent。",
    "Agent created.": "Agent 已创建。",
    "Existing idempotent Conversation replayed.": "已重放现有的幂等对话。",
    "Conversation created.": "对话已创建。",
    "Existing Message replayed without duplicate delivery.": "已重放现有消息，未产生重复投递。",
    "Durable Message sent.": "持久消息已发送。",
    "Confirming replacement durability…": "正在确认替换操作的持久性…",
    "Confirming withdrawal durability…": "正在确认撤回操作的持久性…",
    "Replacement already retained.": "替换消息已保留。",
    "Message replaced.": "消息已替换。",
    "Withdraw observed after refresh; exact replay required to confirm durability.": "刷新后已观察到撤回结果；仍需精确重放以确认持久性。",
    "Replacement observed after refresh; exact replay required to confirm durability.": "刷新后已观察到替换结果；仍需精确重放以确认持久性。",
    "Outcome not observed in retained messages; exact replay required before live observation resumes.": "保留消息中未观察到操作结果；恢复实时观察前需要精确重放。",
    "Message changed while editing; current retained state was refreshed.": "编辑期间消息已变化；当前保留状态已刷新。",
    "Outcome unknown; refresh retained messages before retrying.": "操作结果未知；请刷新保留消息后再重试。",
    "Replacement durably confirmed after exact replay.": "精确重放后已确认替换操作持久保存。",
    "Withdraw durably confirmed after exact replay.": "精确重放后已确认撤回操作持久保存。",
    "durability confirmation still uncertain · refresh before retry": "持久性确认仍不确定 · 请刷新后再重试",
    "message changed during durability confirmation · refresh retained state": "持久性确认期间消息已变化 · 请刷新保留状态",
    "durability confirmation failed · refresh before retry": "持久性确认失败 · 请刷新后再重试",
    "establishing retained baseline": "正在建立保留消息基线",
    "Session unavailable": "会话不可用",
    "observation unavailable": "观察接口不可用",
    "retained snapshot failed": "保留消息快照获取失败",
    "bounded long-poll": "有界长轮询",
    "request failed": "请求失败",
    "retention changed · reloading": "保留窗口已变化 · 正在重新加载",
    "delta drain failed": "增量排空失败",
    "withdraw outcome unknown · refresh before retry": "撤回结果未知 · 请刷新后再重试",
    "message changed · refresh retained state": "消息已变化 · 请刷新保留状态",
    "replace outcome unknown · refresh before retry": "替换结果未知 · 请刷新后再重试",
    "send outcome unknown · refresh before retry": "发送结果未知 · 请刷新后再重试",
    "RUNNING": "运行中",
    "ATTENTION": "待处理",
    "STALE": "状态过期",
    "SOURCE DIFFERENT": "源码不一致",
    "BUILD DIFFERENT": "构建不一致",
    "DIRTY": "有未提交更改",
    "SESSION SCAN PARTIAL": "会话扫描不完整",
    "WINDOW ACTIVE": "窗口活跃",
    "Active host window request": "活跃的主机窗口请求",
    "Open Window inspector": "打开窗口检查器",
    "runtime:read required": "需要 runtime:read 权限",
    "Window Activity": "窗口活动",
    "Host Window Activity": "主机窗口活动",
    "WebPi host windows": "WebPi 主机窗口",
    "WebPi Windows": "WebPi 窗口活动",
    "WebPi windows": "WebPi 窗口活动",
    "WebPi Window activity": "WebPi 窗口活动",
    "WebPi window activity": "WebPi 窗口活动",
    "Client Windows": "客户端窗口",
    "Window activity has not been loaded yet.": "尚未加载窗口活动。",
    "No Window activity is visible.": "当前没有可见的窗口活动。",
    "No Window activity is visible to this credential.": "当前凭证范围内没有可见的窗口活动。",
    "No Window activity is visible for this Project to this credential.": "当前凭证范围内，此项目没有可见的窗口活动。",
    "Window activity requires runtime:read.": "查看窗口活动需要 runtime:read 权限。",
    "Window activity could not be refreshed.": "窗口活动无法刷新。",
    "Window activity could not be refreshed; showing previous data.": "窗口活动无法刷新；正在显示之前的数据。",
    "refresh failed, showing previous data": "刷新失败，正在显示之前的数据",
    "No Window activity has been observed for this Project.": "此项目尚未观察到窗口活动。",
    "No Window activity observed for this project.": "此项目尚未观察到窗口活动。",
    "No WebPi activity is available.": "没有可用的 WebPi 活动。",
    "meaningful": "有效工作",
    "recorder gap": "记录断层",
    "streaming timing unavailable": "流式传输耗时不可用",
    "service unavailable": "服务耗时不可用",
    "next gap unavailable": "下次间隔不可用",
    "overlap from previous": "与前次调用重叠",
    "Active request": "活跃请求",
    "No active request": "无活跃请求",
    "No active requests": "无活跃请求",
    "No WebPi request is currently active.": "当前没有活跃的 WebPi 请求。",
    "No authorized Workflow Session links.": "没有已授权的工作流会话关联。",
    "No linked Window evidence.": "没有关联的窗口证据。",
    "No completed tools/call activity": "没有已完成的 tools/call 活动",
    "No meaningful WebPi work recorded": "未记录到有效 WebPi 工作",
    "Last WebPi call": "最后 WebPi 调用",
    "Last WebPi call ": "最后 WebPi 调用 ",
    "Last WebPi activity": "最后 WebPi 活动",
    "Last WebPi activity ": "最后 WebPi 活动 ",
    "Last meaningful work": "最后有效工作",
    "Last meaningful work ": "最后有效工作 ",
    "Open Window Activity inspector": "打开窗口活动检查器",
    "Copy trace id": "复制 Trace ID",
    "Copy hashed key": "复制哈希键",
    "Could not refresh Window activity.": "无法刷新窗口活动。",
    "Hashed identity": "哈希标识",
    "Select a Window": "选择一个窗口",
    "Window axis": "窗口维度",
    "Choose a hashed Window identity from the sidebar to inspect active requests, linked Workflow Sessions, and bounded recent activity.": "从侧边栏选择哈希窗口标识，以检查活跃请求、关联的工作流会话及有界近期活动。",
    "Shows WebPi calls and correlations only. It cannot observe model reasoning or determine whether the ChatGPT frontend is frozen.": "仅反映 WebPi 调用与关联关系。它无法观察模型推理，也无法判断 ChatGPT 前端是否卡顿。",
    "3s activity refresh": "3秒活动刷新",
    "3s window refresh": "3秒窗口刷新",
    "Host Window liveness and correlation evidence. Window identity never grants execution or Session authority.": "主机窗口活跃度与关联证据。窗口标识绝不授予执行或会话权限。",
    "Recording was not continued for ": "记录未继续于会话 ",
    "service ": "服务耗时 ",
    "next gap ": "下次间隔 ",
    "cycle ": "周期 ",
    "process exit ": "进程退出码 ",
    "exit ": "退出码 ",
    "bounded": "有界",
    "Inspect": "检查",
    "Active calls": "活跃调用",
    "Linked sessions": "关联会话",
    "Timeline": "时间线",
    "RECORDER GAP": "记录断层",
    "Loading Window activity…": "正在加载窗口活动…",
};
const ZH_COUNT_LABELS = {
    "Runner": "台运行器",
    "authorized Runner": "台已授权运行器",
    "Project": "个项目",
    "visible Project": "个可见项目",
    "matching Project": "个匹配项目",
    "Window": "个窗口",
    "Session": "个会话",
    "linked Session": "个关联会话",
    "retained Session": "个保留会话",
    "active Session": "个活跃会话",
    "running Session": "个运行中会话",
    "Agent": "个 Agent",
    "active Endpoint": "个活跃端点",
    "active Job": "个活跃任务",
    "running Job": "个运行中任务",
    "queued Job": "个排队任务",
    "retained message": "条保留消息",
    "message": "条消息",
    "participant": "位参与者",
    "queued delivery": "条排队投递",
    "queued": "条排队项",
    "unresolved Wake": "个未解决唤醒",
    "risk": "个风险",
    "todo": "个待办",
    "question": "个问题",
    "guidance": "条指导",
    "online": "在线",
    "stale": "台状态过期",
    "unavailable": "台不可用",
    "RUNNING": "个运行中",
    "event": "个事件",
    "events": "个事件",
    "window": "个窗口",
    "active call": "个活跃调用",
    "active calls": "个活跃调用",
    "active request": "个活跃请求",
    "active requests": "个活跃请求",
};
function languagePreference(value) {
    return value === "zh-CN" ? "zh-CN" : "en";
}
function loadLanguagePreference() {
    try {
        const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
        if (stored === "en" || stored === "zh-CN")
            return stored;
    }
    catch { /* Fall through to the browser language. */ }
    return navigator.language && navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}
function translate(source, language = "en") {
    return language === "zh-CN" ? (RUNTIME_ZH_TEXT[source] || source) : source;
}
function translateStaticNodeValue(source, language = "en") {
    const match = /^(\s*)([\s\S]*?)(\s*)$/.exec(source);
    if (!match)
        return source;
    return match[1] + translate(match[2], language) + match[3];
}
function localizedCountLabel(value, singular, pluralOrLanguage, language = "en") {
    const isLang = pluralOrLanguage === "en" || pluralOrLanguage === "zh-CN";
    const plural = isLang || !pluralOrLanguage ? singular + "s" : pluralOrLanguage;
    const lang = isLang ? pluralOrLanguage : (language || "en");
    const count = typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0;
    if (lang === "zh-CN")
        return count + " " + (ZH_COUNT_LABELS[singular] || RUNTIME_ZH_TEXT[singular] || singular);
    return count + " " + (count === 1 ? singular : plural);
}
function localizedWorkflowText(value, language = "en") {
    const source = String(value || "");
    if (language !== "zh-CN" || !source)
        return source;
    const exact = {
        "Latest retained validation passed": "最近保留的验证已通过",
        "Latest validation passed": "最近验证已通过",
        "Latest retained validation failed": "最近保留的验证失败",
        "Latest validation failed": "最近验证失败",
        "Validation not run": "尚未运行验证",
        "Retained terminal validation evidence unavailable": "保留的最终验证证据不可用",
        "Terminal validation evidence unavailable": "最终验证证据不可用",
        "No work observations in retained events.": "保留事件中没有工作观察记录。",
        "No tool activity observed.": "尚未观察到工具活动。",
        "No retained open guidance, questions, risks, or todos.": "没有保留的开放指导、问题、风险或待办。",
        "No retained model-reported progress.": "没有保留的模型报告进度。",
    };
    let text = exact[source] || source;
    const prefixes = [
        ["Recent observed work: ", "最近观察到的工作："],
        ["Observed work: ", "已观察工作："],
        ["Retained open messages: ", "保留的开放消息："],
        ["Retained: ", "保留："],
        ["Recent ", "最近 "],
        ["latest ", "最近 "],
    ];
    for (const [english, chinese] of prefixes) {
        if (text.startsWith(english)) {
            text = chinese + text.slice(english.length);
            break;
        }
    }
    const nounMap = {
        edit: "次编辑", edits: "次编辑", validation: "次验证", validations: "次验证",
        exploration: "次探索", review: "次审查", reviews: "次审查", run: "次运行", runs: "次运行",
        risk: "个风险", risks: "个风险", todo: "个待办", todos: "个待办",
        question: "个问题", questions: "个问题", guidance: "条指导",
        test: "次测试", tests: "次测试", "unresolved failure": "个未解决失败",
        "unresolved failures": "个未解决失败", "unresolved validation failure": "个未解决的验证失败",
        "unresolved validation failures": "个未解决的验证失败",
    };
    return text.replace(/(\d+) (unresolved validation failures?|unresolved failures?|edits?|validations?|exploration|reviews?|runs?|risks?|todos?|questions?|guidance|tests?)/g, (_match, count, noun) => {
        return String(count) + " " + (nounMap[String(noun)] || noun);
    });
}

function resolveTr(source, ctx) {
    if (ctx?.tr)
        return ctx.tr(source);
    if (typeof tr === "function")
        return tr(source);
    return source;
}
function resolveIcon(name, ctx) {
    if (ctx?.runtimeIcon)
        return ctx.runtimeIcon(name);
    if (typeof runtimeIcon === "function")
        return runtimeIcon(name);
    return document.createElementNS("http://www.w3.org/2000/svg", "svg");
}
function resolveSetText(id, value, ctx) {
    if (ctx?.setText) {
        ctx.setText(id, value);
        return;
    }
    if (typeof setText === "function") {
        setText(id, value);
        return;
    }
    const node = document.getElementById(id);
    if (node)
        node.textContent = value == null ? "—" : String(value);
}
function appendLinkifiedText(parent, text) {
    const pattern = /https?:\/\/[^\s<>{}\[\]]+/g;
    let cursor = 0;
    for (const match of text.matchAll(pattern)) {
        const index = match.index || 0;
        if (index > cursor)
            parent.appendChild(document.createTextNode(text.slice(cursor, index)));
        let href = match[0];
        let trailing = "";
        while (/[.,;:!?)]$/.test(href)) {
            trailing = href.slice(-1) + trailing;
            href = href.slice(0, -1);
        }
        const link = document.createElement("a");
        link.href = href;
        link.target = "_blank";
        link.rel = "noopener noreferrer";
        link.textContent = href;
        parent.appendChild(link);
        if (trailing)
            parent.appendChild(document.createTextNode(trailing));
        cursor = index + match[0].length;
    }
    if (cursor < text.length)
        parent.appendChild(document.createTextNode(text.slice(cursor)));
}
function messageLineStartsBlock(line) {
    return /^```/.test(line)
        || /^#{1,3}\s+/.test(line)
        || /^>\s?/.test(line)
        || /^\s*[-*+]\s+/.test(line)
        || /^\s*\d+[.)]\s+/.test(line);
}
function appendMessageParagraph(parent, lines) {
    if (!lines.length)
        return;
    const paragraph = document.createElement("p");
    paragraph.className = "message-paragraph";
    lines.forEach((line, index) => {
        if (index)
            paragraph.appendChild(document.createElement("br"));
        appendLinkifiedText(paragraph, line);
    });
    parent.appendChild(paragraph);
}
function appendMessageCode(parent, language, codeText, ctx) {
    const block = document.createElement("section");
    block.className = "message-code";
    const header = document.createElement("header");
    const label = document.createElement("span");
    label.textContent = language || "code";
    const copy = document.createElement("button");
    copy.type = "button";
    copy.className = "message-code-copy";
    copy.appendChild(resolveIcon("copy", ctx));
    const copyLabel = document.createElement("span");
    copyLabel.textContent = resolveTr("Copy code", ctx);
    copy.appendChild(copyLabel);
    copy.title = resolveTr("Copy code", ctx);
    copy.setAttribute("aria-label", resolveTr("Copy code", ctx));
    copy.addEventListener("click", async () => {
        try {
            await navigator.clipboard.writeText(codeText);
            copyLabel.textContent = resolveTr("Code copied", ctx);
            resolveSetText("runtime-message-announcer", resolveTr("Code copied", ctx), ctx);
            window.setTimeout(() => { copyLabel.textContent = resolveTr("Copy code", ctx); }, 1400);
        }
        catch {
            copyLabel.textContent = resolveTr("Unable to copy code", ctx);
            resolveSetText("runtime-message-announcer", resolveTr("Unable to copy code", ctx), ctx);
            window.setTimeout(() => { copyLabel.textContent = resolveTr("Copy code", ctx); }, 1800);
        }
    });
    header.appendChild(label);
    header.appendChild(copy);
    const pre = document.createElement("pre");
    const code = document.createElement("code");
    if (language)
        code.dataset.language = language;
    code.textContent = codeText;
    pre.appendChild(code);
    block.appendChild(header);
    block.appendChild(pre);
    parent.appendChild(block);
}
function appendRichMessage(bubble, sourceValue, ctx) {
    const source = String(sourceValue || "").replace(/\r\n?/g, "\n");
    const lines = source.split("\n");
    const body = document.createElement("div");
    body.className = "message-body";
    let index = 0;
    while (index < lines.length) {
        const line = lines[index];
        if (!line.trim()) {
            index += 1;
            continue;
        }
        const fence = /^```\s*([^\s`]*)/.exec(line);
        if (fence) {
            const codeLines = [];
            index += 1;
            while (index < lines.length && !/^```\s*$/.test(lines[index])) {
                codeLines.push(lines[index]);
                index += 1;
            }
            if (index < lines.length)
                index += 1;
            appendMessageCode(body, fence[1] || "", codeLines.join("\n"), ctx);
            continue;
        }
        const heading = /^(#{1,3})\s+(.+)$/.exec(line);
        if (heading) {
            const title = document.createElement(heading[1].length === 1 ? "h3" : heading[1].length === 2 ? "h4" : "h5");
            title.className = "message-heading";
            appendLinkifiedText(title, heading[2]);
            body.appendChild(title);
            index += 1;
            continue;
        }
        if (/^>\s?/.test(line)) {
            const quote = document.createElement("blockquote");
            const quoteLines = [];
            while (index < lines.length && /^>\s?/.test(lines[index])) {
                quoteLines.push(lines[index].replace(/^>\s?/, ""));
                index += 1;
            }
            appendMessageParagraph(quote, quoteLines);
            body.appendChild(quote);
            continue;
        }
        const unordered = /^\s*[-*+]\s+/.test(line);
        const ordered = /^\s*\d+[.)]\s+/.test(line);
        if (unordered || ordered) {
            const list = document.createElement(ordered ? "ol" : "ul");
            const pattern = ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-*+]\s+/;
            while (index < lines.length && pattern.test(lines[index])) {
                const item = document.createElement("li");
                appendLinkifiedText(item, lines[index].replace(pattern, ""));
                list.appendChild(item);
                index += 1;
            }
            body.appendChild(list);
            continue;
        }
        const paragraphLines = [];
        while (index < lines.length && lines[index].trim() && !messageLineStartsBlock(lines[index])) {
            paragraphLines.push(lines[index]);
            index += 1;
        }
        if (!paragraphLines.length) {
            paragraphLines.push(line);
            index += 1;
        }
        appendMessageParagraph(body, paragraphLines);
    }
    bubble.appendChild(body);
    if (source.length <= 2200 && lines.length <= 36)
        return;
    body.classList.add("is-collapsed");
    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "message-expand";
    toggle.textContent = resolveTr("Show full message", ctx);
    toggle.setAttribute("aria-expanded", "false");
    toggle.addEventListener("click", () => {
        const expanded = body.classList.toggle("is-expanded");
        body.classList.toggle("is-collapsed", !expanded);
        toggle.textContent = resolveTr(expanded ? "Collapse message" : "Show full message", ctx);
        toggle.setAttribute("aria-expanded", expanded ? "true" : "false");
    });
    bubble.appendChild(toggle);
}

const RUNTIME_API_BASE = "/api/runtime-console/";
function isAbortError(error) {
    return error instanceof DOMException
        ? error.name === "AbortError"
        : Boolean(error && typeof error === "object" && "name" in error && error.name === "AbortError");
}
function abortController(controller) {
    if (controller)
        controller.abort();
}
async function writeClipboardText(value, clipboard) {
    if (!value)
        return false;
    try {
        const cb = clipboard || (typeof navigator !== "undefined" ? navigator.clipboard : null);
        if (!cb || typeof cb.writeText !== "function")
            return false;
        await cb.writeText(value);
        return true;
    }
    catch {
        return false;
    }
}
class RuntimeApiClient {
    constructor(apiBase = RUNTIME_API_BASE) {
        this.apiBase = apiBase;
        this.token = "";
    }
    setToken(token) {
        this.token = token;
    }
    getToken() {
        return this.token;
    }
    clearToken() {
        this.token = "";
    }
    async post(path, payload, signal) {
        try {
            const response = await fetch(this.apiBase + path, {
                method: "POST",
                headers: {
                    Authorization: "Bearer " + this.token,
                    "Content-Type": "application/json",
                },
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
            if (isAbortError(error))
                return null;
            return { ok: false, status: 0, data: null };
        }
    }
}

function windowDateTimeLabel(timestampMs, language) {
    const value = Number(timestampMs);
    if (!Number.isFinite(value) || value <= 0)
        return translate("time unavailable", language);
    return new Date(value).toLocaleString(language === "zh-CN" ? "zh-CN" : "en");
}
function windowAgeLabel(timestampMs, now = Date.now(), language) {
    return runtimeWindowActivityLabel(timestampMs, now, language);
}
function runtimeProjectClientId(project) {
    const value = String(project || "");
    const parts = value.split(":");
    return parts.length >= 3 && parts[0] === "agent" ? parts[1] : "";
}
function appendChipElement(parent, text, extraClass = "") {
    const chip = document.createElement("span");
    chip.className = "chip" + (extraClass ? " " + extraClass : "");
    chip.textContent = text;
    parent.appendChild(chip);
    return chip;
}
function renderWindowActivityRows(node, activities, options = {}) {
    if (!node)
        return;
    while (node.firstChild)
        node.removeChild(node.firstChild);
    const compact = options.compact ?? false;
    const language = options.language;
    for (const activity of activities) {
        const item = document.createElement("article");
        item.className = "window-activity-item" + (compact ? " compact" : "")
            + (activity?.recorder_gap_session_id ? " recorder-gap" : "");
        const head = document.createElement("div");
        head.className = "window-activity-head";
        const title = document.createElement("strong");
        title.textContent = String(activity?.tool_name || activity?.method || "WebPi call");
        const time = document.createElement("span");
        time.className = "muted small";
        time.textContent = windowDateTimeLabel(activity?.started_at_ms, language);
        head.appendChild(title);
        head.appendChild(time);
        item.appendChild(head);
        const facts = document.createElement("div");
        facts.className = "chips window-activity-facts";
        appendChipElement(facts, translate(String(activity?.status || "unknown"), language));
        if (activity?.project)
            appendChipElement(facts, String(activity.project));
        if (activity?.activity_presentation) {
            appendChipElement(facts, String(activity.activity_presentation), "tone-runtime");
        }
        if (activity?.activity_kind)
            appendChipElement(facts, String(activity.activity_kind));
        if (activity?.meaningful)
            appendChipElement(facts, translate("meaningful", language), "tone-runtime");
        if (activity?.recorder_gap_session_id)
            appendChipElement(facts, translate("recorder gap", language), "tone-warn");
        if (activity?.response_streaming === true) {
            appendChipElement(facts, translate("streaming timing unavailable", language), "tone-warn");
        }
        else if (typeof activity?.service_ms === "number") {
            appendChipElement(facts, (language === "zh-CN" ? "服务耗时 " : "service ") + String(activity.service_ms) + " ms");
        }
        else if (activity?.meaningful) {
            appendChipElement(facts, translate("service unavailable", language));
        }
        if (activity?.meaningful) {
            if (typeof activity?.next_call_gap_ms === "number") {
                appendChipElement(facts, (language === "zh-CN" ? "下次间隔 " : "next gap ") + String(activity.next_call_gap_ms) + " ms");
            }
            else {
                appendChipElement(facts, translate("next gap unavailable", language));
            }
            if (typeof activity?.cycle_ms === "number") {
                appendChipElement(facts, (language === "zh-CN" ? "周期 " : "cycle ") + String(activity.cycle_ms) + " ms");
            }
        }
        if (activity?.window_transition_kind === "overlap") {
            appendChipElement(facts, translate("overlap from previous", language), "tone-warn");
        }
        item.appendChild(facts);
        const links = Array.isArray(activity?.workflow_sessions) ? activity.workflow_sessions : [];
        if (links.length) {
            const relation = document.createElement("div");
            relation.className = "muted small";
            relation.textContent = links
                .map((link) => String(link.workflow_session_id || "") + " · " + String(link.relation || "linked"))
                .join(" · ");
            item.appendChild(relation);
        }
        if (activity?.recorder_gap_session_id) {
            const gap = document.createElement("div");
            gap.className = "window-gap-note small";
            gap.textContent = (language === "zh-CN" ? "记录未继续于会话 " : "Recording was not continued for ") + String(activity.recorder_gap_session_id) + ".";
            item.appendChild(gap);
        }
        if (activity?.server_trace_id) {
            const trace = document.createElement("button");
            trace.type = "button";
            trace.className = "window-trace-copy";
            trace.textContent = "trace " + String(activity.server_trace_id);
            trace.title = translate("Copy trace id", language);
            if (options.onCopyTrace) {
                trace.addEventListener("click", () => options.onCopyTrace(String(activity.server_trace_id)));
            }
            item.appendChild(trace);
        }
        node.appendChild(item);
    }
}
function createWindowCard(row, selectedWindowKey, onSelect, now = Date.now(), language) {
    const key = String(row?.client_window_key || "");
    if (!key)
        return null;
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.action = "open-window";
    button.className = "runtime-window-card" + (key === selectedWindowKey ? " selected" : "");
    if (key === selectedWindowKey)
        button.setAttribute("aria-current", "true");
    const head = document.createElement("div");
    head.className = "runtime-window-card-head";
    const title = document.createElement("strong");
    title.textContent = "Window " + runtimeWindowShortKey(key);
    const active = document.createElement("span");
    active.className = "chip" + (Number(row?.active_count || 0) > 0 ? " tone-runtime" : "");
    active.textContent = Number(row?.active_count || 0) > 0
        ? (language === "zh-CN" ? String(row.active_count) + " 个活跃" : String(row.active_count) + " active")
        : String(row?.source || "window");
    head.appendChild(title);
    head.appendChild(active);
    button.appendChild(head);
    if (row.last_project_name && row.last_project_name !== "—") {
        const project = document.createElement("p");
        project.className = "window-project-label";
        project.textContent = String(row.last_project_name);
        button.appendChild(project);
    }
    const call = document.createElement("span");
    call.className = "muted small";
    call.textContent = row?.last_tool_call_at_ms
        ? (language === "zh-CN" ? "最后调用 " : "Last WebPi call ") + windowAgeLabel(row.last_tool_call_at_ms, now, language)
        : (language === "zh-CN" ? "最后活动 " : "Last WebPi activity ") + windowAgeLabel(row?.last_seen_at_ms, now, language);
    button.appendChild(call);
    const meaningful = document.createElement("span");
    meaningful.className = "muted small";
    meaningful.textContent = row?.last_meaningful_activity_at_ms
        ? (language === "zh-CN" ? "最后有效工作 " : "Last meaningful work ") + windowAgeLabel(row.last_meaningful_activity_at_ms, now, language)
        : (language === "zh-CN" ? "未记录到有效 WebPi 工作" : "No meaningful WebPi work recorded");
    button.appendChild(meaningful);
    const links = document.createElement("span");
    links.className = "muted small";
    links.textContent = localizedCountLabel(Number(row?.linked_session_count || 0), "linked Session", "linked Sessions", language)
        + (Number(row?.recorder_gap_count || 0) ? " · " + String(row.recorder_gap_count) + (language === "zh-CN" ? " 个记录断层" : " recorder gap") : "");
    button.appendChild(links);
    button.addEventListener("click", () => onSelect(key));
    return button;
}
function renderWindowActiveRequests(activeNode, activeRequests, options = {}) {
    if (!activeNode)
        return;
    while (activeNode.firstChild)
        activeNode.removeChild(activeNode.firstChild);
    const now = options.now ?? Date.now();
    const language = options.language;
    if (!activeRequests.length) {
        const empty = document.createElement("p");
        empty.className = "muted small";
        empty.textContent = translate("No WebPi request is currently active.", language);
        activeNode.appendChild(empty);
        return;
    }
    for (const request of activeRequests) {
        const item = document.createElement("article");
        item.className = "window-request-item";
        const title = document.createElement("strong");
        title.textContent = String(request?.tool_name || request?.method || "WebPi request");
        item.appendChild(title);
        const meta = document.createElement("div");
        meta.className = "muted small";
        const facts = [
            request?.project,
            request?.started_at_ms ? (language === "zh-CN" ? "开始于 " : "started ") + windowAgeLabel(request.started_at_ms, now, language) : null,
            typeof request?.elapsed_ms === "number" ? String(request.elapsed_ms) + (language === "zh-CN" ? " 毫秒已耗时" : " ms elapsed") : null,
        ].filter(Boolean).map(String);
        meta.textContent = facts.join(" · ");
        item.appendChild(meta);
        if (request?.server_trace_id) {
            const trace = document.createElement("button");
            trace.type = "button";
            trace.className = "window-trace-copy";
            trace.textContent = "trace " + String(request.server_trace_id);
            trace.title = translate("Copy trace id", language);
            if (options.onCopyTrace) {
                trace.addEventListener("click", () => options.onCopyTrace(String(request.server_trace_id)));
            }
            item.appendChild(trace);
        }
        activeNode.appendChild(item);
    }
}
function renderWindowLinkedSessions(sessionsNode, linkedSessions, onOpenSession, language) {
    if (!sessionsNode)
        return;
    while (sessionsNode.firstChild)
        sessionsNode.removeChild(sessionsNode.firstChild);
    for (const session of linkedSessions) {
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.action = "open-linked-session";
        button.className = "window-session-card";
        const title = document.createElement("strong");
        title.textContent = String(session?.title || session?.workflow_session_id || translate("Workflow Session", language));
        const meta = document.createElement("span");
        meta.className = "muted small";
        meta.textContent = [
            session?.workflow_session_id,
            session?.lifecycle,
            session?.project,
            ...(Array.isArray(session?.relations) ? session.relations : []),
        ].filter(Boolean).map(String).join(" · ");
        button.appendChild(title);
        button.appendChild(meta);
        button.addEventListener("click", () => onOpenSession(session));
        sessionsNode.appendChild(button);
    }
    if (!sessionsNode.childElementCount) {
        const empty = document.createElement("p");
        empty.className = "muted small";
        empty.textContent = translate("No authorized Workflow Session links.", language);
        sessionsNode.appendChild(empty);
    }
}
function renderSessionWindowCorrelationLinks(linkedNode, links, onSelectWindow, now = Date.now(), language) {
    if (!linkedNode)
        return;
    while (linkedNode.firstChild)
        linkedNode.removeChild(linkedNode.firstChild);
    for (const link of links) {
        const key = String(link?.client_window_key || "");
        if (!key)
            continue;
        const button = document.createElement("button");
        button.type = "button";
        button.className = "window-session-card" + (Number(link?.recorder_gap_count || 0) ? " recorder-gap" : "");
        const title = document.createElement("strong");
        title.textContent = "Window " + runtimeWindowShortKey(key);
        const meta = document.createElement("span");
        meta.className = "muted small";
        meta.textContent = [
            link?.source,
            link?.last_seen_at_ms ? (language === "zh-CN" ? "最后活动 " : "last WebPi activity ") + windowAgeLabel(link.last_seen_at_ms, now, language) : null,
            Number(link?.recorder_gap_count || 0) ? String(link.recorder_gap_count) + (language === "zh-CN" ? " 个记录断层" : " recorder gap") : null,
        ].filter(Boolean).map(String).join(" · ");
        button.appendChild(title);
        button.appendChild(meta);
        button.addEventListener("click", () => onSelectWindow(key));
        linkedNode.appendChild(button);
    }
    if (!links.length) {
        const empty = document.createElement("p");
        empty.className = "muted small";
        empty.textContent = translate("No linked Window evidence.", language);
        linkedNode.appendChild(empty);
    }
}
function formatWindowDetailFields(detail, fallbackKey = "", now = Date.now(), language) {
    if (!detail)
        return null;
    const key = String(detail.client_window_key || fallbackKey || "");
    return {
        title: "Window " + runtimeWindowShortKey(key),
        key: key || "—",
        source: String(detail.source || "—"),
        activeCount: String(Number(detail.active_count || 0)),
        lastCall: detail.last_tool_call_at_ms
            ? windowAgeLabel(detail.last_tool_call_at_ms, now, language)
            : translate("No completed tools/call activity", language),
        lastMeaningful: detail.last_meaningful_activity_at_ms
            ? windowAgeLabel(detail.last_meaningful_activity_at_ms, now, language)
            : translate("No meaningful WebPi work recorded", language),
        activeStatus: translate(Number(detail.active_count || 0) ? "Active request" : "No active request", language),
        linkedStatus: localizedCountLabel(Number(detail.sessions_returned || 0), "Session", "Sessions", language) +
            (detail.sessions_truncated ? " · " + translate("bounded", language) : ""),
        activityStatus: localizedCountLabel(Number(detail.activity_returned || 0), "event", "events", language) +
            (detail.activity_truncated ? " · " + translate("bounded", language) : ""),
    };
}
function formatWindowEmptyState(availability, visibilityScope = "principal", isProjectScoped = false, language) {
    if (availability === "unavailable") {
        return translate("Window activity requires runtime:read.", language);
    }
    if (availability === "stale") {
        return translate("Window activity could not be refreshed.", language);
    }
    if (availability === "available") {
        if (isProjectScoped) {
            if (visibilityScope === "principal") {
                return translate("No Window activity is visible for this Project to this credential.", language);
            }
            return translate("No Window activity has been observed for this Project.", language);
        }
        if (visibilityScope === "principal") {
            return translate("No Window activity is visible to this credential.", language);
        }
        return translate("No Window activity is visible.", language);
    }
    return translate("Loading Window activity…", language);
}
function formatWindowListStatusText(availability, count, visibilityScope = "principal", language) {
    if (availability === "unavailable") {
        return translate("runtime:read required", language);
    }
    if (availability === "stale") {
        if (count > 0) {
            const countPart = localizedCountLabel(count, "Window", "Windows", language);
            const stalePart = translate("refresh failed, showing previous data", language);
            return countPart + " · " + stalePart;
        }
        return translate("Window activity could not be refreshed.", language);
    }
    if (count > 0) {
        return localizedCountLabel(count, "Window", "Windows", language);
    }
    return formatWindowEmptyState(availability, visibilityScope, false, language);
}
function renderWindowCards(node, windowRows, selectedWindowKey, onSelect, now = Date.now(), language) {
    if (!node)
        return;
    while (node.firstChild)
        node.removeChild(node.firstChild);
    for (const row of windowRows) {
        const card = createWindowCard(row, selectedWindowKey, onSelect, now, language);
        if (card)
            node.appendChild(card);
    }
}
function renderProjectWindowCards(node, windowRows, onSelect, now = Date.now(), language) {
    if (!node)
        return;
    while (node.firstChild)
        node.removeChild(node.firstChild);
    for (const row of windowRows) {
        const card = createWindowCard(row, "", onSelect, now, language);
        if (card) {
            card.title = translate("Open Window inspector", language);
            node.appendChild(card);
        }
    }
}

function communicationTimeLabel(value, language) {
    if (typeof value !== "number" || !Number.isFinite(value))
        return translate("time unavailable", language);
    return new Date(value).toLocaleString(language === "zh-CN" ? "zh-CN" : "en");
}
function parseAgentIds(value) {
    const ids = value
        .split(/[\s,]+/)
        .map((item) => item.trim())
        .filter(Boolean);
    return Array.from(new Set(ids));
}
function deliveryAgentLabel(agentId, agents = []) {
    const agent = agents.find((a) => String(a?.agent_id || "") === agentId);
    return agent ? String(agent.display_name || agent.handle || agentId) : agentId;
}
function appendCommunicationChip(parent, text, extraClass = "") {
    const chip = document.createElement("span");
    chip.className = "chip" + (extraClass ? " " + extraClass : "");
    chip.textContent = text;
    parent.appendChild(chip);
    return chip;
}
function createAgentRow(agent, selectedAgentId, options) {
    const agentId = String(agent?.agent_id || "");
    if (!agentId)
        return null;
    const language = options.language;
    const row = document.createElement("button");
    row.type = "button";
    row.className = "communication-row" + (agentId === selectedAgentId ? " selected" : "");
    if (agentId === selectedAgentId)
        row.setAttribute("aria-current", "true");
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(agent?.display_name || agent?.handle || "Agent") + " · @" + String(agent?.handle || "agent");
    const unread = document.createElement("span");
    unread.className = "chip" + (Number(agent?.queued_delivery_count || 0) > 0 ? " tone-warn" : "");
    unread.textContent = localizedCountLabel(agent?.queued_delivery_count, "queued delivery", "queued deliveries", language);
    head.appendChild(title);
    head.appendChild(unread);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = agentId
        + (language === "zh-CN" ? " · 配置版本 r" : " · profile r") + String(agent?.profile_revision || 0)
        + (language === "zh-CN" ? " · 控制器 g" : " · controller g") + String(agent?.current_controller_generation || 0)
        + " · " + localizedCountLabel(agent?.active_endpoint_count, "active Endpoint", "active Endpoints", language)
        + " · " + localizedCountLabel(agent?.unresolved_wake_count, "unresolved Wake", "unresolved Wakes", language);
    row.appendChild(meta);
    row.addEventListener("click", () => options.onSelect(agentId));
    return row;
}
function renderAgentRows(list, agents, selectedAgentId, options) {
    if (!list)
        return;
    while (list.firstChild)
        list.removeChild(list.firstChild);
    for (const agent of agents) {
        const row = createAgentRow(agent, selectedAgentId, options);
        if (row)
            list.appendChild(row);
    }
}
function createConversationRow(conversation, selectedConversationId, options) {
    const conversationId = String(conversation?.conversation_id || "");
    if (!conversationId)
        return null;
    const language = options.language;
    const row = document.createElement("button");
    row.type = "button";
    row.className = "communication-row" + (conversationId === selectedConversationId ? " selected" : "");
    if (conversationId === selectedConversationId)
        row.setAttribute("aria-current", "true");
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(conversation?.title || translate("Untitled Conversation", language));
    const count = document.createElement("span");
    count.className = "chip";
    count.textContent = localizedCountLabel(conversation?.message_count, "message", "messages", language);
    head.appendChild(title);
    head.appendChild(count);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = conversationId
        + " · " + localizedCountLabel(conversation?.participant_count, "participant", "participants", language)
        + (language === "zh-CN" ? " · 序号 " : " · seq ") + String(conversation?.last_seq || 0);
    row.appendChild(meta);
    row.addEventListener("click", () => options.onSelect(conversationId));
    return row;
}
function renderConversationRows(list, conversations, selectedConversationId, options) {
    if (!list)
        return;
    while (list.firstChild)
        list.removeChild(list.firstChild);
    for (const conversation of conversations) {
        const row = createConversationRow(conversation, selectedConversationId, options);
        if (row)
            list.appendChild(row);
    }
}
function createConversationMessageCard(message, agents, options = {}) {
    const language = options.language;
    const author = message?.author || {};
    const agentAuthored = String(author.participant_kind || "") === "agent";
    const card = document.createElement("article");
    card.className = "conversation-message" + (agentAuthored ? " agent-authored" : "");
    const head = document.createElement("div");
    head.className = "conversation-message-head";
    const name = document.createElement("span");
    name.className = "conversation-message-author";
    name.textContent = agentAuthored
        ? "Agent · " + String(author.display_name || author.handle || (author.agent_id ? deliveryAgentLabel(String(author.agent_id), agents) : "") || author.agent_id || translate("unknown", language))
        : (language === "zh-CN" ? "人工 · " : "Human · ") + String(author.principal_kind || (language === "zh-CN" ? "凭证主体" : "credential principal"));
    const seq = document.createElement("span");
    seq.className = "muted small";
    seq.textContent = "#" + String(message?.seq || 0) + " · " + communicationTimeLabel(message?.created_at_unix_ms, language);
    head.appendChild(name);
    head.appendChild(seq);
    card.appendChild(head);
    const meta = document.createElement("div");
    meta.className = "conversation-message-meta";
    const metaParts = [String(message?.message_id || "")];
    if (author.agent_id)
        metaParts.push(String(author.agent_id));
    if (message?.reply_to)
        metaParts.push((language === "zh-CN" ? "回复 " : "reply to ") + String(message.reply_to));
    meta.textContent = metaParts.join(" · ");
    card.appendChild(meta);
    const body = document.createElement("div");
    body.className = "conversation-message-body";
    body.textContent = String(message?.body || "");
    card.appendChild(body);
    const deliveries = Array.isArray(message?.deliveries) ? message.deliveries : [];
    const delivery = document.createElement("div");
    delivery.className = "conversation-message-deliveries";
    delivery.textContent = deliveries.length
        ? (language === "zh-CN" ? "Agent 收件箱：" : "Agent Inbox: ") + deliveries.map((item) => deliveryAgentLabel(String(item?.recipient_agent_id || ""), agents) + " " + translate(String(item?.state || "unknown"), language)).join(" · ")
        : (language === "zh-CN" ? "没有 Agent 收件箱投递 · 仅保留记录 / 人工房间" : "No Agent Inbox delivery · transcript / Human room only");
    card.appendChild(delivery);
    return card;
}
function renderConversationMessages(transcript, messages, agents, options = {}) {
    if (!transcript)
        return;
    while (transcript.firstChild)
        transcript.removeChild(transcript.firstChild);
    for (const message of messages) {
        const card = createConversationMessageCard(message, agents, options);
        transcript.appendChild(card);
    }
    transcript.scrollTop = transcript.scrollHeight;
}
function createInboxDeliveryCard(item, agents, options) {
    const language = options.language;
    const row = document.createElement("article");
    row.className = "communication-row inbox-delivery";
    const head = document.createElement("div");
    head.className = "communication-row-head";
    const title = document.createElement("span");
    title.className = "communication-row-title";
    title.textContent = String(item?.conversation_title || translate("Untitled Conversation", language)) + " · #" + String(item?.message?.seq || 0);
    const consume = document.createElement("button");
    consume.type = "button";
    consume.className = "text-button";
    consume.textContent = translate("Consume", language);
    consume.addEventListener("click", () => options.onConsume(String(item?.delivery_id || "")));
    head.appendChild(title);
    head.appendChild(consume);
    row.appendChild(head);
    const meta = document.createElement("span");
    meta.className = "communication-row-meta";
    meta.textContent = String(item?.delivery_id || "")
        + (language === "zh-CN" ? " · 来自 " : " · from ")
        + (item?.message?.author?.participant_kind === "agent"
            ? deliveryAgentLabel(String(item.message.author.agent_id || ""), agents)
            : (language === "zh-CN" ? "人工" : "Human"));
    row.appendChild(meta);
    const body = document.createElement("div");
    body.className = "inbox-message-preview";
    body.textContent = String(item?.message?.body || "");
    row.appendChild(body);
    return row;
}
function renderInboxDeliveryCards(container, inbox, agents, options) {
    if (!container)
        return;
    while (container.firstChild)
        container.removeChild(container.firstChild);
    for (const item of inbox) {
        const row = createInboxDeliveryCard(item, agents, options);
        container.appendChild(row);
    }
}

function formatUpdatedTime(timestamp, language) {
    if (typeof timestamp !== "number")
        return translate("time unavailable", language);
    return new Date(timestamp * 1000).toLocaleTimeString(language === "zh-CN" ? "zh-CN" : "en");
}
function formatSessionDateTime(timestamp, language) {
    if (typeof timestamp !== "number")
        return translate("time unavailable", language);
    return new Date(timestamp * 1000).toLocaleString(language === "zh-CN" ? "zh-CN" : "en");
}
function formatLivenessPresentation(session, language) {
    const presentation = workflowSessionLivenessPresentation(session);
    if (language !== "zh-CN")
        return presentation;
    let label = translate(String(presentation.label || "idle"), language);
    if (presentation.state === "idle" && String(presentation.label || "").startsWith("idle · ")) {
        label = translate("idle", language) + " · " + String(presentation.label).slice("idle · ".length);
    }
    return { ...presentation, label, tooltip: translate(String(presentation.tooltip || ""), language) };
}
function activityKindLabel(activity, language) {
    const kind = String(activity && activity.kind || "Activity");
    if (activity && activity.job_handoff) {
        if (kind === "Tested")
            return language === "zh-CN" ? "测试" : "Test";
        if (kind === "Ran")
            return language === "zh-CN" ? "命令" : "Command";
    }
    if (kind === "Explored" && activity && typeof activity.group_count === "number") {
        return (language === "zh-CN" ? "探索 ×" : "Explored ×") + activity.group_count;
    }
    if (language !== "zh-CN")
        return kind;
    const labels = {
        Activity: "活动",
        Progress: "进度",
        Explored: "探索",
        Edited: "编辑",
        Tested: "测试",
        Ran: "运行",
        Reviewed: "审查",
    };
    return labels[kind] || kind;
}
function durationLabel(durationMs) {
    if (durationMs < 1000)
        return durationMs + " ms";
    return (durationMs / 1000).toFixed(durationMs < 10000 ? 1 : 0) + " s";
}
function activityFacts(activity, includeTiming, language) {
    const facts = [];
    if (activity && typeof activity.group_count === "number") {
        if (Array.isArray(activity.group_kinds) && activity.group_kinds.length) {
            facts.push(activity.group_kinds.map(String).join(" / "));
        }
        if (Array.isArray(activity.group_tools) && activity.group_tools.length) {
            facts.push(activity.group_tools.map(String).join(", "));
        }
    }
    else if (activity && activity.tool) {
        facts.push(String(activity.tool));
    }
    if (activity && activity.kind === "Progress") {
        facts.push(language === "zh-CN" ? "仅供参考" : "informational");
    }
    else if (activity && activity.job_handoff) {
        facts.push(language === "zh-CN" ? "已移交" : "handed off");
        if (activity.execution_state) {
            facts.push((language === "zh-CN" ? "执行 " : "execution ") + translate(String(activity.execution_state), language));
        }
    }
    else if (activity && activity.state) {
        facts.push(translate(String(activity.state), language));
    }
    if (includeTiming && activity && typeof activity.duration_ms === "number") {
        facts.push(durationLabel(activity.duration_ms));
    }
    if (activity && typeof activity.exit_code === "number") {
        // Label it explicitly as process exit code so that `state = failed` + `exit_code = 0`
        // is clearly understood as a process exit code and never confused with action success.
        facts.push((language === "zh-CN" ? "进程退出 " : "process exit ") + activity.exit_code);
    }
    if (activity && activity.job_id) {
        facts.push("job " + String(activity.job_id));
    }
    if (includeTiming && activity && typeof activity.started_at === "number") {
        facts.push(new Date(activity.started_at * 1000).toLocaleTimeString(language === "zh-CN" ? "zh-CN" : "en"));
    }
    return facts;
}
function activityDescription(activity, language) {
    if (!activity)
        return "";
    const parts = [activityKindLabel(activity, language), ...activityFacts(activity, false, language)];
    if (activity.summary && !activity.job_handoff)
        parts.push(String(activity.summary));
    return parts.join(" · ");
}
function appendActivityPreview(parent, label, activity, language) {
    if (!activity)
        return;
    const row = document.createElement("div");
    row.className = "activity-preview muted small";
    const prefix = document.createElement("span");
    prefix.className = "activity-preview-label";
    prefix.textContent = label;
    const text = document.createElement("span");
    text.textContent = activityDescription(activity, language);
    row.appendChild(prefix);
    row.appendChild(text);
    parent.appendChild(row);
}
function createTimelineEvent(activity, language) {
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
    kind.textContent = activityKindLabel(activity, language);
    const meta = document.createElement("span");
    meta.className = "muted small";
    meta.textContent = activityFacts(activity, true, language).join(" · ");
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
    return item;
}
function renderTimelineEvents(container, activities, language) {
    if (!container)
        return;
    while (container.firstChild)
        container.removeChild(container.firstChild);
    for (const activity of activities) {
        const item = createTimelineEvent(activity, language);
        container.appendChild(item);
    }
}

const RUNTIME_CREDENTIAL_SESSION_KEY = "webpi.runtime.credential.v1";
const APPEARANCE_STORAGE_KEY = "webpi.runtime.appearance.v1";
const WORKSPACE_VIEW_STORAGE_KEY = "webpi.runtime.workspace-view.v1";
const DRAFT_STORAGE_PREFIX = "webpi.runtime.draft.v1.";
const DEVICE_DISCLOSURE_STORAGE_PREFIX = "webpi.runtime.runner-open.v1.";
const APPEARANCE_MEDIA_QUERY = "(prefers-color-scheme: light)";
function appearancePreference(value) {
    return value === "light" || value === "dark" || value === "system" ? value : "system";
}
function loadAppearancePreference() {
    try {
        return appearancePreference(window.localStorage.getItem(APPEARANCE_STORAGE_KEY));
    }
    catch {
        return "system";
    }
}
function persistAppearancePreference(preference) {
    try {
        window.localStorage.setItem(APPEARANCE_STORAGE_KEY, preference);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
function resolvedAppearance(preference, prefersLight) {
    if (preference !== "system")
        return preference;
    return prefersLight ? "light" : "dark";
}
function workspaceViewPreference(value) {
    return value === "sessions" || value === "operations" || value === "windows" || value === "projects" || value === "activity" || value === "extensions" ? value : "home";
}
function loadWorkspaceViewPreference() {
    try {
        return workspaceViewPreference(window.localStorage.getItem(WORKSPACE_VIEW_STORAGE_KEY));
    }
    catch {
        return "home";
    }
}
function persistWorkspaceViewPreference(view) {
    try {
        window.localStorage.setItem(WORKSPACE_VIEW_STORAGE_KEY, view);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
function loadRememberedRuntimeCredential() {
    try {
        return window.sessionStorage.getItem(RUNTIME_CREDENTIAL_SESSION_KEY)?.trim() || "";
    }
    catch {
        return "";
    }
}
function persistRuntimeCredentialForTab(token, remember) {
    try {
        if (remember && token) {
            window.sessionStorage.setItem(RUNTIME_CREDENTIAL_SESSION_KEY, token);
        }
        else {
            window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
        }
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
function clearRememberedRuntimeCredential() {
    try {
        window.sessionStorage.removeItem(RUNTIME_CREDENTIAL_SESSION_KEY);
    }
    catch {
        /* Storage can be unavailable in hardened browser contexts. */
    }
}
function currentDraftStorageKey(project, sessionId) {
    const projectId = String(project || "");
    const workflowSessionId = String(sessionId || "");
    return projectId && workflowSessionId
        ? DRAFT_STORAGE_PREFIX + encodeURIComponent(projectId) + "." + encodeURIComponent(workflowSessionId)
        : "";
}
function loadDraft(project, sessionId) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return "";
    try {
        return window.sessionStorage.getItem(key) || "";
    }
    catch {
        return "";
    }
}
function saveDraft(project, sessionId, text) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return;
    try {
        if (text)
            window.sessionStorage.setItem(key, text);
        else
            window.sessionStorage.removeItem(key);
    }
    catch {
        /* Draft remains in active input when storage is unavailable. */
    }
}
function clearDraft(project, sessionId) {
    const key = currentDraftStorageKey(project, sessionId);
    if (!key)
        return;
    try {
        window.sessionStorage.removeItem(key);
    }
    catch {
        /* No-op in hardened browser contexts. */
    }
}
function clearRuntimeDrafts() {
    try {
        const keys = [];
        for (let index = 0; index < window.sessionStorage.length; index += 1) {
            const key = window.sessionStorage.key(index);
            if (key?.startsWith(DRAFT_STORAGE_PREFIX))
                keys.push(key);
        }
        for (const key of keys)
            window.sessionStorage.removeItem(key);
    }
    catch {
        /* No-op in hardened browser contexts. */
    }
}
function deviceDisclosureStorageKey(clientId) {
    return DEVICE_DISCLOSURE_STORAGE_PREFIX + encodeURIComponent(clientId);
}
function storedDeviceDisclosure(clientId) {
    try {
        const value = window.localStorage.getItem(deviceDisclosureStorageKey(clientId));
        return value === "open" ? true : value === "closed" ? false : null;
    }
    catch {
        return null;
    }
}
function persistDeviceDisclosure(clientId, open) {
    try {
        window.localStorage.setItem(deviceDisclosureStorageKey(clientId), open ? "open" : "closed");
    }
    catch {
        /* Disclosure remains active for current render. */
    }
}

function pendingAttentionCount(attention) {
    return ["open_risks", "open_todos", "open_questions", "open_guidance"].reduce((total, key) => total + (typeof attention?.[key] === "number" ? Math.max(0, Math.floor(attention[key])) : 0), 0);
}
function runnerAttentionCount(runner) {
    return pendingAttentionCount(runner?.sessions?.attention);
}
function attentionLabel(attention, language) {
    const parts = [];
    for (const [key, singular] of [
        ["open_risks", "risk"],
        ["open_todos", "todo"],
        ["open_questions", "question"],
        ["open_guidance", "guidance"],
    ]) {
        const count = typeof attention?.[key] === "number" ? attention[key] : 0;
        if (count)
            parts.push(localizedCountLabel(count, singular, singular + "s", language));
    }
    return parts.length ? parts.join(" · ") : translate("No retained pending attention", language);
}
function formatProjectIdentity(project, language) {
    if (language !== "zh-CN")
        return runtimeProjectIdentityText(project);
    if (!project || typeof project.id !== "string" || !project.id) {
        return translate("No project selected", language);
    }
    const runner = typeof project.client_id === "string" && project.client_id
        ? project.client_id
        : translate("unknown", language);
    const path = typeof project.path === "string" && project.path ? project.path : "不可用";
    return "运行器：" + runner + " · 项目：" + project.id + " · 工作空间：" + path;
}
function extractProjectSelectorDevices(projects, knownDevices = [], runnerRows = [], selectedDevice = "") {
    const devices = new Set(knownDevices);
    for (const device of runtimeDeviceIds(projects))
        devices.add(device);
    for (const runner of runnerRows) {
        const clientId = typeof runner?.client_id === "string" ? runner.client_id : "";
        if (clientId)
            devices.add(clientId);
    }
    if (selectedDevice)
        devices.add(selectedDevice);
    return Array.from(devices).sort((left, right) => left.localeCompare(right));
}
function formatProjectLabel(project) {
    const name = project && project.name ? String(project.name) : "";
    const id = project && project.id ? String(project.id) : "";
    const identity = name && name !== id ? name + " — " + id : id;
    const status = project && project.connected ? String(project.agent_status || "online") : "offline";
    return identity + " · " + status;
}
function mergeEffectiveProjects(projects, homeProjectRows = []) {
    const aggregates = new Map();
    for (const row of homeProjectRows) {
        if (row && typeof row.id === "string")
            aggregates.set(row.id, row);
    }
    return (Array.isArray(projects) ? projects : []).map((project) => {
        const aggregate = aggregates.get(String(project?.id || ""));
        return aggregate ? { ...project, sessions: aggregate.sessions } : project;
    });
}
function formatRuntimeOverviewMetrics(data, language) {
    if (!data)
        return null;
    const buildGitCommit = data.build_git_commit;
    const buildText = buildGitCommit
        ? (language === "zh-CN" ? "构建 " : "build ") +
            buildGitCommit +
            (data.build_git_dirty ? (language === "zh-CN" ? " · 有未提交更改" : " · dirty") : "")
        : translate("build unavailable", language);
    const projectsText = data.projects_available
        ? localizedCountLabel(data.visible_projects, "visible Project", "visible Projects", language) +
            (data.projects_truncated ? (language === "zh-CN" ? " · 不完整" : " · partial") : "")
        : translate("project:read unavailable", language);
    const jobsText = localizedCountLabel(data.active_jobs, "active Job", "active Jobs", language) +
        (data.mixed_builds_present ? (language === "zh-CN" ? " · 存在混合构建" : " · mixed builds") : "");
    const sessionsText = localizedCountLabel(data.workflow_sessions?.active, "active Session", "active Sessions", language) +
        " · " +
        localizedCountLabel(data.workflow_sessions?.running, "running Session", "running Sessions", language) +
        (data.workflow_sessions?.truncated
            ? language === "zh-CN"
                ? " · 有界汇总"
                : " · bounded aggregate"
            : "");
    const recentMeta = data.recent_sessions || {};
    const recentStatusText = localizedCountLabel(recentMeta.returned, "Session", "Sessions", language) +
        (recentMeta.truncated
            ? (language === "zh-CN" ? " · 前 " : " · top ") + String(recentMeta.returned || 0)
            : "") +
        (recentMeta.scan_truncated
            ? language === "zh-CN"
                ? " · 扫描不完整"
                : " · partial scan"
            : "");
    return {
        identity: [data.service, data.version].filter(Boolean).join(" · "),
        build: buildText,
        runners: localizedCountLabel(data.runner_count, "Runner", "Runners", language),
        alignment: localizedCountLabel(data.runners_online, "online", "online", language) +
            " · " +
            localizedCountLabel(data.runners_stale, "stale", "stale", language) +
            " · " +
            localizedCountLabel(data.runners_unavailable, "unavailable", "unavailable", language),
        projects: projectsText,
        jobs: jobsText,
        attention: attentionLabel(data.workflow_sessions, language),
        sessions: sessionsText,
        recentStatus: recentStatusText,
    };
}

function operationKey(prefix) {
    const random = typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
        ? crypto.randomUUID()
        : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2);
    return prefix + "-" + random;
}
function idempotencyKeyFor(pending, fingerprint, prefix) {
    return pending && pending.fingerprint === fingerprint
        ? pending
        : { fingerprint, key: operationKey(prefix) };
}
function formatCommunicationAvailability(readAvailable, manageAvailable, language) {
    const available = readAvailable !== false;
    if (readAvailable === null) {
        return language === "zh-CN" ? "正在检查 communication:read…" : "communication:read checking…";
    }
    if (!available) {
        return language === "zh-CN" ? "communication:read 不可用" : "communication:read unavailable";
    }
    return ("communication:read" +
        (manageAvailable === false
            ? language === "zh-CN"
                ? " · 只读"
                : " · read only"
            : language === "zh-CN"
                ? " · 当前视图每 30 秒刷新 · 端点租约每 30 秒续期"
                : " · 30s refresh while visible · 30s endpoint lease renewal"));
}
function formatAgentCardRevision(agent, language) {
    return ((language === "zh-CN" ? "配置版本 " : "Profile revision ") +
        String(agent?.profile_revision || 0) +
        (language === "zh-CN" ? " · 控制器代数 " : " · controller generation ") +
        String(agent?.current_controller_generation || 0) +
        (language === "zh-CN" ? " · 更新于 " : " · updated ") +
        communicationTimeLabel(agent?.updated_at_unix_ms, language));
}
function formatAgentWakeStatus(agent, language) {
    const unresolvedWakeCount = Number(agent?.unresolved_wake_count || 0);
    const latestWakeState = String(agent?.latest_wake_state || "none");
    return (localizedCountLabel(unresolvedWakeCount, "unresolved Wake", "unresolved Wakes", language) +
        (language === "zh-CN" ? " · 最近状态 " : " · latest ") +
        translate(latestWakeState, language) +
        (language === "zh-CN"
            ? " · 收件箱投递与唤醒消费彼此独立"
            : " · Inbox Delivery and Wake consumption remain independent"));
}
function formatAgentEndpointStatus(endpoint, language) {
    if (!endpoint) {
        return language === "zh-CN"
            ? "此窗口尚未作为该 Agent。Agent 卡片、对话、收件箱投递和唤醒意图仍会持久保留。"
            : "This window is not acting as the Agent. Agent Card, Conversations, Inbox deliveries, and Wake Intents remain durable.";
    }
    return ((language === "zh-CN" ? "浏览器端点 " : "Browser Endpoint ") +
        endpoint.endpoint_id +
        " · " +
        translate(endpoint.lifecycle, language) +
        (language === "zh-CN" ? " · 代数 " : " · generation ") +
        String(endpoint.controller_generation) +
        (language === "zh-CN" ? " · 租约至 " : " · lease ") +
        communicationTimeLabel(endpoint.lease_expires_at_unix_ms, language) +
        (language === "zh-CN"
            ? " · 运行控制台适配器：仅轮询（运行时可唤醒："
            : " · Runtime Console adapter: polling only (runtime wake capable: ") +
        String(endpoint.wake_capable) +
        ")");
}
function formatConversationSeq(summary, detail, language) {
    return ((language === "zh-CN" ? "序号 " : "seq ") +
        String(summary?.last_seq || 0) +
        " · " +
        localizedCountLabel(summary?.message_count, "message", "messages", language) +
        (Number(detail?.after_seq || 0) > 0 || detail?.truncated
            ? language === "zh-CN"
                ? " · 最近有界页面"
                : " · recent bounded page"
            : ""));
}
function validateAgentCreateInputs(handle, displayName, description, labelsRaw) {
    const cleanHandle = handle.trim();
    const cleanName = displayName.trim();
    const cleanDescription = description.trim();
    const labels = parseAgentIds(labelsRaw);
    if (!cleanHandle || !cleanName) {
        return { valid: false, error: "Handle and display name are required." };
    }
    const fingerprint = JSON.stringify({
        handle: cleanHandle,
        displayName: cleanName,
        description: cleanDescription,
        labels,
    });
    return {
        valid: true,
        data: {
            handle: cleanHandle,
            displayName: cleanName,
            description: cleanDescription,
            labels,
            fingerprint,
        },
    };
}
function validateAgentUpdateInputs(handle, displayName, description, labelsRaw) {
    const cleanHandle = handle.trim();
    const cleanName = displayName.trim();
    const cleanDescription = description.trim();
    const specialtyLabels = parseAgentIds(labelsRaw);
    if (!cleanHandle || !cleanName) {
        return { valid: false, error: "Handle and display name are required." };
    }
    return {
        valid: true,
        data: {
            handle: cleanHandle,
            displayName: cleanName,
            description: cleanDescription,
            specialtyLabels,
        },
    };
}
function validateConversationCreateInputs(title, agentIdsRaw, defaultAgentId = "") {
    const cleanTitle = title.trim();
    const agentIds = parseAgentIds(agentIdsRaw || defaultAgentId);
    if (agentIds.length === 0) {
        return { valid: false, error: "At least one Agent id is required." };
    }
    const fingerprint = JSON.stringify({ title: cleanTitle, agentIds: [...agentIds].sort() });
    return {
        valid: true,
        data: {
            title: cleanTitle,
            agentIds,
            fingerprint,
        },
    };
}

const RUNTIME_ICON_PATHS = {
    folder: ["M3 6h7l2 2h9v10H3V6Z"],
    monitor: ["M4 5h16v12H4V5Z", "M8 21h8", "M12 17v4"],
    message: ["M5 5h14v12H9l-4 3V5Z", "M9 9h6", "M9 13h4"],
    reply: ["m10 8-5 4 5 4", "M5 12h7a6 6 0 0 1 6 6"],
    edit: ["m4 16-.5 4.5L8 20l10.5-10.5-4-4L4 16Z", "m12.5 7.5 4 4"],
    trash: ["M4 7h16", "M9 7V4h6v3", "m7 7 1 13h8l1-13", "M10 11v5", "M14 11v5"],
    copy: ["M8 8h11v11H8V8Z", "M5 16H4V4h12v1"],
};
function runtimeIcon(name, className = "") {
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    if (className)
        svg.setAttribute("class", className);
    const paths = RUNTIME_ICON_PATHS[name] || [];
    for (const pathData of paths) {
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        path.setAttribute("d", pathData);
        svg.appendChild(path);
    }
    return svg;
}
function createMessageAction(label, iconName, action, danger = false) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "message-action" + (danger ? " danger" : "");
    button.title = label;
    button.setAttribute("aria-label", label);
    button.appendChild(runtimeIcon(iconName));
    button.addEventListener("click", action);
    return button;
}

function clearNavigationNode(node) {
    while (node.firstChild)
        node.removeChild(node.firstChild);
}
function appendNavigationChip(parent, text, extraClass = "") {
    const chip = document.createElement("span");
    chip.className = "chip" + (extraClass ? " " + extraClass : "");
    chip.textContent = text;
    parent.appendChild(chip);
    return chip;
}
function formatWorkspaceBreadcrumb(project, language) {
    const runnerText = project?.client_id
        ? String(project.client_id)
        : translate("Fleet", language);
    const projectText = project
        ? String(project.name || project.id || translate("Projects", language))
        : translate("Projects", language);
    return { runnerText, projectText };
}
function formatSelectedProjectIdentity(project, language) {
    if (language !== "zh-CN") {
        return runtimeProjectIdentityText(project);
    }
    return formatProjectIdentity(project, language);
}
function formatSessionWorkspaceIdentity(project, language) {
    return formatProjectIdentity(project, language);
}
function formatDeviceStatusText(devicesCount, filter, language) {
    if (!devicesCount) {
        return language === "zh-CN" ? "没有已授权运行器" : "No authorized Runners";
    }
    const base = localizedCountLabel(devicesCount, "authorized Runner", "authorized Runners", language);
    if (filter) {
        return base + (language === "zh-CN" ? " · 已筛选" : " · filtered");
    }
    return base + " · " + translate("All Runners", language);
}
function formatProjectStatusText(returnedProjects, totalProjects, truncated, filter, query, language) {
    const scope = language === "zh-CN"
        ? (filter ? " · 位于 " + filter : " · 跨全部设备")
        : (filter ? " on " + filter : " across fleet");
    const queryActive = !!String(query || "").trim();
    if (truncated) {
        return language === "zh-CN"
            ? "已显示 " + String(returnedProjects) + " / " + String(totalProjects) + (queryActive ? " 个匹配项目" : " 个可见项目") + scope + " · 有界"
            : String(returnedProjects) + " of " + String(totalProjects) + (queryActive ? " matching Projects shown" : " visible Projects shown") + scope + " · bounded";
    }
    const singular = queryActive ? "matching Project" : "visible Project";
    return localizedCountLabel(totalProjects, singular, singular + "s", language) + scope;
}
function formatRunnerCountText(count, language) {
    return localizedCountLabel(count, "Runner", "Runners", language);
}
function formatRecentSessionStatusText(meta, language) {
    if (!meta)
        return "";
    return localizedCountLabel(meta.returned, "Session", "Sessions", language)
        + (meta.truncated ? (language === "zh-CN" ? " · 前 " : " · top ") + String(meta.returned || 0) : "")
        + (meta.scan_truncated ? (language === "zh-CN" ? " · 扫描不完整" : " · partial scan") : "");
}
function formatProjectWindowStatusText(returned, total, truncated, language) {
    if (!truncated)
        return "";
    return language === "zh-CN"
        ? String(returned) + " / " + String(total) + " 个窗口 · 有界"
        : String(returned) + " of " + String(total) + " Windows · bounded";
}
function renderProjectSelectorTree(deviceSelect, projectList, sessionsPanel, options) {
    const tr = (text) => translate(text, options.language);
    const countLabel = (count, singular) => localizedCountLabel(count, singular, options.language);
    const updatedLabel = (timestamp) => formatUpdatedTime(timestamp, options.language);
    clearNavigationNode(deviceSelect);
    const all = document.createElement("option");
    all.value = "";
    all.textContent = tr("All Runners");
    deviceSelect.appendChild(all);
    for (const clientId of options.devices) {
        const option = document.createElement("option");
        option.value = clientId;
        option.textContent = clientId;
        deviceSelect.appendChild(option);
    }
    deviceSelect.value = options.projectDeviceFilter;
    const rows = filterAndSortRuntimeProjects(options.effectiveProjects, options.projectDeviceFilter, "");
    clearNavigationNode(projectList);
    const projectsByDevice = new Map();
    for (const project of rows) {
        const clientId = String(project?.client_id || "unknown");
        const deviceProjects = projectsByDevice.get(clientId) || [];
        deviceProjects.push(project);
        projectsByDevice.set(clientId, deviceProjects);
    }
    const visibleDevices = options.projectDeviceFilter ? [options.projectDeviceFilter] : options.devices;
    let sessionsAttached = false;
    let windowsAttached = false;
    for (const clientId of visibleDevices) {
        const deviceProjects = projectsByDevice.get(clientId) || [];
        const runner = options.runnerRows.find((candidate) => String(candidate?.client_id || "") === clientId);
        const connected = runner ? runner.connected !== false : deviceProjects.some((project) => project?.connected);
        const group = document.createElement("details");
        group.className = "device-group" + (connected ? " online" : " offline");
        group.dataset.runnerId = clientId;
        group.setAttribute("aria-label", options.language === "zh-CN" ? "设备 " + clientId : "Device " + clientId);
        const storedDisclosure = options.storedDeviceDisclosure(clientId);
        const defaultOpen = options.projectDeviceFilter ? true : String(options.selectedDevice || "") === clientId || clientId === visibleDevices[0];
        group.open = resolveRunnerDisclosure(storedDisclosure, defaultOpen);
        const deviceHead = document.createElement("summary");
        deviceHead.className = "device-group-head";
        const deviceIcon = document.createElement("span");
        deviceIcon.className = "device-group-icon";
        deviceIcon.setAttribute("aria-hidden", "true");
        deviceIcon.appendChild(runtimeIcon("monitor"));
        const deviceIdentity = document.createElement("div");
        deviceIdentity.className = "device-group-identity";
        const deviceName = document.createElement("strong");
        deviceName.textContent = clientId;
        const deviceMeta = document.createElement("span");
        deviceMeta.className = "muted small";
        const status = runner ? String(runner.status || (connected ? "online" : "offline")) : (connected ? "online" : "offline");
        deviceMeta.textContent = tr(status) + " · " + countLabel(deviceProjects.length, "Project");
        const deviceDot = document.createElement("span");
        deviceDot.className = "device-group-dot";
        deviceDot.title = tr(status);
        deviceIdentity.appendChild(deviceName);
        deviceIdentity.appendChild(deviceMeta);
        deviceHead.appendChild(deviceIcon);
        deviceHead.appendChild(deviceIdentity);
        deviceHead.appendChild(deviceDot);
        group.appendChild(deviceHead);
        group.addEventListener("toggle", () => options.onPersistDeviceDisclosure(clientId, group.open));
        const deviceProjectList = document.createElement("div");
        deviceProjectList.className = "device-project-list";
        if (deviceProjects.length === 0) {
            const empty = document.createElement("p");
            empty.className = "device-project-empty muted small";
            empty.textContent = tr("No visible Projects");
            deviceProjectList.appendChild(empty);
        }
        for (const project of deviceProjects) {
            const workspace = document.createElement("details");
            workspace.className = "workspace-group";
            const disclosureKey = "webpi.runtime.workspace-disclosure.v1." + encodeURIComponent(JSON.stringify([clientId, project.id]));
            workspace.open = project.id === options.selectedProject;
            try {
                workspace.open = workspace.open && window.localStorage.getItem(disclosureKey) !== "closed";
            }
            catch { }
            workspace.addEventListener("toggle", () => {
                if (!workspace.isConnected || project.id !== options.selectedProject)
                    return;
                try {
                    window.localStorage.setItem(disclosureKey, workspace.open ? "open" : "closed");
                }
                catch { }
            });
            const row = document.createElement("summary");
            row.dataset.action = "select-project";
            row.className = "project-row" + (project.id === options.selectedProject ? " selected" : "");
            if (project.id === options.selectedProject)
                row.setAttribute("aria-current", "true");
            const projectName = String(project.name || project.id || "");
            const projectId = String(project.id || "");
            const projectPath = String(project.path || "");
            row.title = [projectName, projectId && projectId !== projectName ? projectId : "", projectPath].filter(Boolean).join(" · ");
            row.setAttribute("aria-label", row.title || projectName);
            const projectIcon = document.createElement("span");
            projectIcon.className = "project-row-icon";
            projectIcon.setAttribute("aria-hidden", "true");
            projectIcon.appendChild(runtimeIcon("folder"));
            const main = document.createElement("div");
            main.className = "project-row-main";
            const heading = document.createElement("div");
            heading.className = "project-row-heading";
            const title = document.createElement("div");
            title.className = "project-row-title";
            title.textContent = projectName;
            const signals = document.createElement("div");
            signals.className = "project-row-signals";
            const addSignal = (text, tone, signalTitle = text) => {
                const signal = document.createElement("span");
                signal.className = "project-row-state " + tone;
                signal.textContent = text;
                signal.title = signalTitle;
                signals.appendChild(signal);
            };
            const runningSessions = Math.max(0, Number(project.sessions?.running_sessions || 0));
            const projectAttention = pendingAttentionCount(project.sessions?.attention);
            if (!project.connected)
                addSignal(tr("OFFLINE"), "tone-fail");
            else if (project.agent_status && project.agent_status !== "online")
                addSignal(tr(String(project.agent_status).toUpperCase()), "tone-warn");
            if (project.id === options.selectedProject && (options.selectedProjectWindowActiveCount ?? 0) > 0) {
                addSignal(options.language === "zh-CN" ? "窗口活跃" : "WINDOW ACTIVE", "tone-runtime", options.language === "zh-CN" ? "活跃的主机窗口请求" : "Active host window request");
            }
            if (runningSessions > 0) {
                addSignal(options.language === "zh-CN" ? "运行中 " + runningSessions : runningSessions + " running", "tone-runtime", countLabel(runningSessions, "running Session"));
            }
            if (projectAttention > 0) {
                addSignal(options.language === "zh-CN" ? "待处理 " + projectAttention : projectAttention + " attention", "tone-warn", attentionLabel(project.sessions?.attention));
            }
            heading.appendChild(title);
            heading.appendChild(signals);
            main.appendChild(heading);
            const meta = document.createElement("div");
            meta.className = "project-row-meta muted small";
            const metaParts = [];
            if (project.sessions) {
                metaParts.push(project.sessions.sessions_truncated
                    ? options.language === "zh-CN"
                        ? String(project.sessions.returned_sessions || 0) + " / " + String(project.sessions.retained_sessions || 0) + " 个会话"
                        : String(project.sessions.returned_sessions || 0) + " / " + String(project.sessions.retained_sessions || 0) + " Sessions"
                    : countLabel(project.sessions.retained_sessions, "Session"));
                if (project.id === options.selectedProject && typeof options.selectedProjectWindowCount === "number") {
                    metaParts.push(countLabel(options.selectedProjectWindowCount, "Window"));
                }
                if (typeof project.sessions.latest_updated_at === "number") {
                    metaParts.push((options.language === "zh-CN" ? "更新于 " : "updated ") + updatedLabel(project.sessions.latest_updated_at));
                }
                if (project.sessions.sessions_truncated)
                    metaParts.push(options.language === "zh-CN" ? "扫描不完整" : "scan partial");
            }
            meta.textContent = metaParts.join(" · ");
            if (metaParts.length)
                main.appendChild(meta);
            row.appendChild(projectIcon);
            row.appendChild(main);
            const select = () => options.onSelectProject(String(project.client_id || ""), String(project.id || ""));
            row.addEventListener("click", (event) => {
                if (project.id === options.selectedProject)
                    return;
                event.preventDefault();
                try {
                    window.localStorage.setItem(disclosureKey, "open");
                }
                catch { }
                select();
            });
            workspace.appendChild(row);
            deviceProjectList.appendChild(workspace);
            if (project.id === options.selectedProject) {
                if (sessionsPanel) {
                    sessionsPanel.hidden = false;
                    workspace.appendChild(sessionsPanel);
                    sessionsAttached = true;
                }
                if (options.windowPanel) {
                    options.windowPanel.hidden = false;
                    workspace.appendChild(options.windowPanel);
                    windowsAttached = true;
                }
            }
        }
        group.appendChild(deviceProjectList);
        projectList.appendChild(group);
    }
    if (sessionsPanel && !sessionsAttached) {
        sessionsPanel.hidden = true;
        projectList.appendChild(sessionsPanel);
    }
    if (options.windowPanel && !windowsAttached) {
        options.windowPanel.hidden = true;
        projectList.appendChild(options.windowPanel);
    }
}
function renderRunnerFleetRows(node, runners, options) {
    const tr = (text) => translate(text, options.language);
    const countLabel = (count, singular) => localizedCountLabel(count, singular, options.language);
    clearNavigationNode(node);
    for (const runner of runners) {
        const clientId = String(runner?.client_id || "");
        if (!clientId)
            continue;
        const row = document.createElement("button");
        row.type = "button";
        row.className = "fleet-row" + (clientId === options.selectedDevice ? " selected" : "");
        if (clientId === options.selectedDevice)
            row.setAttribute("aria-current", "true");
        const main = document.createElement("div");
        main.className = "fleet-row-main";
        const title = document.createElement("div");
        title.className = "fleet-row-title";
        title.textContent = clientId;
        const meta = document.createElement("div");
        meta.className = "muted small fleet-row-meta";
        const metaParts = [
            tr(runner.connected ? String(runner.status || "online") : "offline"),
            runner.version ? "v" + String(runner.version) : (options.language === "zh-CN" ? "版本不可用" : "version unavailable"),
            runner.transport ? String(runner.transport) : (options.language === "zh-CN" ? "传输方式不可用" : "transport unavailable"),
            runner.source_alignment
                ? (options.language === "zh-CN" ? "源码 " : "source ") + tr(String(runner.source_alignment))
                : (options.language === "zh-CN" ? "源码对齐状态不可用" : "source alignment unavailable"),
            typeof runner.last_seen_age_secs === "number"
                ? (options.language === "zh-CN" ? String(runner.last_seen_age_secs) + " 秒前在线" : "seen " + String(runner.last_seen_age_secs) + "s ago")
                : (options.language === "zh-CN" ? "最后在线时间不可用" : "last seen unavailable"),
        ];
        if (runner.build_git_commit)
            metaParts.push((options.language === "zh-CN" ? "构建 " : "build ") + String(runner.build_git_commit));
        meta.textContent = metaParts.join(" · ");
        main.appendChild(title);
        main.appendChild(meta);
        const signals = document.createElement("div");
        signals.className = "fleet-row-signals";
        const working = Math.max(Number(runner.jobs_running || 0), Number(runner.sessions?.running_sessions || 0));
        const attention = runnerAttentionCount(runner);
        if (working > 0)
            appendNavigationChip(signals, tr("RUNNING"), "tone-runtime");
        if (attention > 0)
            appendNavigationChip(signals, tr("ATTENTION") + " " + attention, "tone-warn");
        if (!runner.connected)
            appendNavigationChip(signals, tr("OFFLINE"), "tone-fail");
        else if (String(runner.status || "") === "stale")
            appendNavigationChip(signals, tr("STALE"), "tone-warn");
        if (runner.source_alignment === "different")
            appendNavigationChip(signals, tr("SOURCE DIFFERENT"), "tone-fail");
        if (runner.version_matches_server === false)
            appendNavigationChip(signals, tr("BUILD DIFFERENT"), "tone-warn");
        if (runner.build_git_dirty === true)
            appendNavigationChip(signals, tr("DIRTY"), "tone-warn");
        const facts = document.createElement("div");
        facts.className = "muted small fleet-row-facts";
        const projectFact = runner.projects_scan_partial
            ? options.language === "zh-CN" ? String(runner.projects_scanned || 0) + " 个项目已扫描" : String(runner.projects_scanned || 0) + " Projects scanned"
            : countLabel(runner.projects_scanned, "visible Project");
        const factParts = [
            countLabel(runner.active_jobs, "active Job"),
            countLabel(runner.jobs_running, "running Job"),
            countLabel(runner.jobs_queued, "queued Job"),
            typeof runner.job_concurrency_limit === "number"
                ? (options.language === "zh-CN" ? "并发上限 " : "limit ") + runner.job_concurrency_limit
                : (options.language === "zh-CN" ? "并发上限不可用" : "limit unavailable"),
            projectFact,
            countLabel(runner.sessions?.active_sessions, "active Session"),
        ];
        if (runner.projects_scan_partial)
            factParts.push(options.language === "zh-CN" ? "设备群扫描不完整" : "fleet scan partial");
        if (runner.sessions?.sessions_truncated)
            factParts.push(options.language === "zh-CN" ? "会话扫描不完整" : "Session scan partial");
        facts.textContent = factParts.join(" · ");
        row.appendChild(main);
        row.appendChild(signals);
        row.appendChild(facts);
        const select = () => options.onSelectRunner(clientId);
        row.addEventListener("click", select);
        node.appendChild(row);
    }
}
function renderRecentSessionRows(node, sessions, options) {
    const tr = (text) => translate(text, options.language);
    const updatedLabel = (timestamp) => formatUpdatedTime(timestamp, options.language);
    clearNavigationNode(node);
    for (const session of sessions) {
        const sessionId = String(session?.session_id || "");
        const projectId = String(session?.project_id || "");
        const clientId = String(session?.client_id || "");
        if (!sessionId || !projectId || !clientId)
            continue;
        const selected = projectId === options.selectedProject && sessionId === options.selectedSessionId;
        const row = document.createElement("button");
        row.type = "button";
        row.className = "recent-session-row" + (selected ? " selected" : "");
        if (selected)
            row.setAttribute("aria-current", "true");
        const main = document.createElement("div");
        main.className = "recent-session-main";
        const title = document.createElement("div");
        title.className = "session-title";
        title.textContent = session.title ? String(session.title) : sessionId;
        const location = document.createElement("div");
        location.className = "muted small recent-session-location";
        location.textContent = clientId + " · " + String(session.project_name || projectId) + (session.project_name && session.project_name !== projectId ? " · " + projectId : "");
        main.appendChild(title);
        main.appendChild(location);
        const signals = document.createElement("div");
        signals.className = "recent-session-signals";
        const liveness = formatLivenessPresentation(session, options.language);
        if (liveness.state === "working")
            appendNavigationChip(signals, tr("RUNNING"), "tone-runtime");
        const attention = attentionLabel(session.overview?.attention);
        if (pendingAttentionCount(session.overview?.attention) > 0)
            appendNavigationChip(signals, attention, "tone-warn");
        const lifecycle = document.createElement("span");
        lifecycle.className = "muted small";
        lifecycle.textContent = [tr(String(session.lifecycle || "")), liveness.label, (options.language === "zh-CN" ? "更新于 " : "updated ") + updatedLabel(session.updated_at)].filter(Boolean).join(" · ");
        lifecycle.title = liveness.tooltip;
        signals.appendChild(lifecycle);
        row.appendChild(main);
        row.appendChild(signals);
        appendActivityPreview(row, tr("Now"), session.current_activity, options.language);
        appendActivityPreview(row, tr("Last"), session.last_activity, options.language);
        const select = () => options.onSelectSession(session);
        row.addEventListener("click", select);
        node.appendChild(row);
    }
}

function clearCollaborationNode(node) {
    while (node.firstChild)
        node.removeChild(node.firstChild);
}
function collaborationPhaseLabel(phase, language) {
    switch (phase) {
        case "live": return translate("Live", language);
        case "reconnecting": return translate("Reconnecting", language);
        case "paused": return translate("Paused", language);
        default: return translate("Idle", language);
    }
}
function syncCollaborationComposerLayout(body = typeof document !== "undefined" ? document.getElementById("runtime-message-body") : null, composer = typeof document !== "undefined" ? document.getElementById("runtime-collaboration-form") : null, send = typeof document !== "undefined" ? document.getElementById("runtime-message-send") : null) {
    const hasContent = !!body?.value.trim();
    composer?.classList.toggle("has-content", hasContent);
    send?.classList.toggle("is-ready", hasContent);
    if (!body)
        return;
    body.style.height = "0px";
    const nextHeight = Math.min(Math.max(body.scrollHeight, 44), 180);
    body.style.height = nextHeight + "px";
    body.style.overflowY = body.scrollHeight > 180 ? "auto" : "hidden";
}
function formatComposerOptionSummary(kind, priority, requiresAck, language) {
    const signals = [];
    if (kind && kind !== "note")
        signals.push(translate(kind, language));
    if (priority && priority !== "normal")
        signals.push(translate(priority, language));
    if (requiresAck)
        signals.push(language === "zh-CN" ? "需确认" : "ACK");
    return {
        label: signals.length ? signals.join(" · ") : translate("Options", language),
        hasSelection: signals.length > 0,
    };
}
function runtimeSearchMatches(query, values) {
    const text = values.filter((value) => typeof value === "string").join(" ").toLocaleLowerCase();
    return query.trim().toLocaleLowerCase().split(/\s+/).every((term) => text.includes(term));
}
function filterCollaborationCards(cards, separators, messages, query) {
    let matches = 0;
    for (const card of cards) {
        const message = messages.find((entry) => entry && entry.message_id === card.dataset.messageId);
        const visible = runtimeSearchMatches(query, [message?.message, message?.resolution, message?.message_id, message?.author_session_id]);
        card.hidden = !visible;
        if (visible)
            matches++;
    }
    for (const node of separators) {
        node.hidden = !!query.trim();
    }
    return { matches, total: cards.length };
}
function renderLatestAgentMessage(container, messages, locallyAuthoredMessageIds, language) {
    const updatedLabel = (timestamp) => formatUpdatedTime(timestamp, language);
    const sides = runtimeCollaborationMessageSides(messages, locallyAuthoredMessageIds);
    const latest = [...messages].reverse().find((message) => sides.get(String(message.message_id)) === "incoming" && !message.superseded_by_message_id && message.closure_kind !== "withdrawn");
    clearCollaborationNode(container);
    if (latest) {
        appendRichMessage(container, latest.message);
        const time = document.createElement("p");
        time.className = "muted small";
        time.textContent = updatedLabel(latest.created_at);
        container.appendChild(time);
    }
    else {
        container.textContent = language === "zh-CN"
            ? "当前保留范围内暂无 Agent 留言。ACK 不包含回复正文；下方可查看模型报告的进度。"
            : "No Agent message in the retained window. ACK contains no reply text; model-reported progress appears below.";
    }
}
function renderCollaborationMessageCards(node, messages, options) {
    const tr = (text) => translate(text, options.language);
    const updatedLabel = (timestamp) => formatUpdatedTime(timestamp, options.language);
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
    const messageSides = runtimeCollaborationMessageSides(messages, options.locallyAuthoredIds);
    const visited = new Set();
    let previousRenderedSide = "";
    let previousRenderedDay = "";
    const appendMessage = (message, depth, parentUnavailable) => {
        const id = String(message?.message_id || "");
        if (!id || visited.has(id))
            return;
        visited.add(id);
        const card = document.createElement("article");
        card.dataset.messageId = id;
        card.className = "message-card " + String(message?.kind || "note") + (String(message?.status || "") === "resolved" ? " resolved" : "") + (parentUnavailable ? " retained-reply" : "");
        const messageSide = messageSides.get(id) || "neutral";
        card.classList.add(messageSide === "incoming" ? "agent-authored" : messageSide === "outgoing" ? "human-authored" : "provenance-unknown");
        const createdAt = typeof message?.created_at === "number" ? message.created_at : 0;
        const createdDate = createdAt ? new Date(createdAt * 1000) : null;
        const dayKey = createdDate ? [createdDate.getFullYear(), createdDate.getMonth(), createdDate.getDate()].join("-") : "";
        if (dayKey && dayKey !== previousRenderedDay) {
            const separator = document.createElement("div");
            separator.className = "message-date-separator";
            const label = document.createElement("span");
            label.textContent = createdDate?.toLocaleDateString(options.language === "zh-CN" ? "zh-CN" : "en", { month: "short", day: "numeric", year: "numeric" }) || "";
            separator.appendChild(label);
            node.appendChild(separator);
            previousRenderedDay = dayKey;
            previousRenderedSide = "";
        }
        card.classList.add(messageSide === "incoming" ? "message-incoming" : messageSide === "outgoing" ? "message-outgoing" : "message-neutral");
        if (!options.previouslyRenderedMessageIds.has(id))
            card.classList.add("message-entering");
        if (previousRenderedSide === messageSide)
            card.classList.add("message-group-continuation");
        previousRenderedSide = messageSide;
        if (depth > 0)
            card.classList.add("message-thread");
        const content = document.createElement("div");
        content.className = "message-content";
        const author = document.createElement("div");
        author.className = "message-author";
        const authorName = document.createElement("span");
        authorName.className = "message-author-name";
        authorName.textContent = messageSide === "incoming" ? tr("Agent") : messageSide === "outgoing" ? tr("You") : tr("Retained message");
        if (message?.author_session_id)
            authorName.title = String(message.author_session_id);
        else if (messageSide === "neutral")
            authorName.title = tr("Author provenance unavailable");
        author.appendChild(authorName);
        content.appendChild(author);
        if (message?.reply_to) {
            const replyContext = document.createElement("div");
            replyContext.className = "message-reply-context";
            replyContext.appendChild(runtimeIcon("reply"));
            const replyText = document.createElement("span");
            const parent = byId.get(String(message.reply_to));
            const preview = parent?.message ? String(parent.message).replace(/\s+/g, " ").trim().slice(0, 120) : tr("Original message unavailable");
            replyText.textContent = tr("Replying to") + " · " + preview;
            replyContext.appendChild(replyText);
            content.appendChild(replyContext);
        }
        const footer = document.createElement("div");
        footer.className = "message-footer";
        const head = document.createElement("div");
        head.className = "message-head";
        const kindValue = String(message?.kind || "note");
        const priorityValue = String(message?.priority || "normal");
        const statusValue = String(message?.status || "open");
        const messageSignals = [];
        if (kindValue !== "note")
            messageSignals.push(tr(kindValue));
        if (priorityValue !== "normal")
            messageSignals.push(tr(priorityValue));
        if (statusValue && statusValue !== "open" && statusValue !== "resolved")
            messageSignals.push(tr(statusValue));
        if (messageSignals.length) {
            const kind = document.createElement("span");
            kind.className = "message-kind";
            kind.textContent = messageSignals.join(" · ");
            head.appendChild(kind);
        }
        const time = document.createElement("span");
        time.className = "muted small";
        time.textContent = updatedLabel(message?.created_at);
        head.appendChild(time);
        footer.appendChild(head);
        const meta = document.createElement("div");
        meta.className = "message-meta";
        const metaParts = [id];
        if (message?.author_session_id)
            metaParts.push((options.language === "zh-CN" ? "作者 " : "author ") + String(message.author_session_id));
        if (parentUnavailable)
            metaParts.push(options.language === "zh-CN" ? "保留的回复 · 上级消息不可用" : "retained reply · parent unavailable");
        else if (message?.reply_to)
            metaParts.push((options.language === "zh-CN" ? "回复 " : "reply to ") + String(message.reply_to));
        if (message?.superseded_by_message_id) {
            const replacementId = String(message.superseded_by_message_id);
            metaParts.push(byId.has(replacementId)
                ? "superseded by " + replacementId
                : "superseded by " + replacementId + " · replacement unavailable / retained link only");
        }
        if (message?.supersedes_message_id) {
            const originalId = String(message.supersedes_message_id);
            metaParts.push(byId.has(originalId)
                ? "replaces " + originalId
                : "replaces " + originalId + " · retained link only");
        }
        meta.textContent = metaParts.join(" · ");
        footer.appendChild(meta);
        footer.title = meta.textContent;
        const bubble = document.createElement("div");
        bubble.className = "message-bubble";
        appendRichMessage(bubble, message?.message);
        content.appendChild(bubble);
        if (message?.requires_ack) {
            const ack = document.createElement("div");
            ack.className = "message-ack";
            const acknowledged = typeof message?.first_ack_observed_at === "number";
            ack.classList.toggle("observed", acknowledged);
            ack.textContent = acknowledged
                ? (options.language === "zh-CN" ? "已观察到 ACK（不代表回复或完成）" : "ACK observed (not a reply or completion)") + " · " + updatedLabel(message.first_ack_observed_at)
                : tr("Acknowledgement required");
            ack.title = acknowledged
                ? "ACK required · First ACK observed " + updatedLabel(message.first_ack_observed_at)
                : "ACK required";
            footer.appendChild(ack);
        }
        if (message?.resolved_at || message?.resolution || message?.resolved_by_message_id || message?.closure_kind) {
            const resolution = document.createElement("div");
            resolution.className = "message-resolution";
            const parts = [];
            if (message?.closure_kind === "withdrawn")
                parts.push("withdrawn" + (message.resolved_at ? " " + updatedLabel(message.resolved_at) : ""));
            else if (message?.closure_kind === "superseded")
                parts.push("superseded" + (message.resolved_at ? " " + updatedLabel(message.resolved_at) : ""));
            else if (message.resolved_at)
                parts.push("resolved " + updatedLabel(message.resolved_at));
            if (message.resolution)
                parts.push(String(message.resolution));
            if (message.resolved_by_message_id)
                parts.push("by " + String(message.resolved_by_message_id));
            const resolutionLabel = message?.closure_kind === "withdrawn"
                ? tr("Withdrawn")
                : message?.closure_kind === "superseded"
                    ? tr("Replaced")
                    : tr("Resolved");
            resolution.textContent = resolutionLabel + (message.resolved_at ? " · " + updatedLabel(message.resolved_at) : "");
            resolution.title = parts.join(" · ");
            footer.appendChild(resolution);
            if (message.resolution) {
                const explanation = document.createElement("section");
                explanation.className = "message-resolution-body";
                const label = document.createElement("strong");
                label.textContent = options.language === "zh-CN" ? "处理说明" : "Resolution";
                explanation.appendChild(label);
                appendRichMessage(explanation, message.resolution);
                content.appendChild(explanation);
            }
        }
        const actions = document.createElement("div");
        actions.className = "message-actions";
        actions.appendChild(createMessageAction(tr("Reply"), "reply", () => options.onReply(id)));
        if (runtimeCollaborationMessageCanMutate(message) && options.canMutate) {
            const editLabel = options.language === "zh-CN" ? "替换这条保留消息，同时保留其历史记录。" : "Replace this retained message while preserving its history.";
            const deleteLabel = options.language === "zh-CN" ? "撤回这条保留消息；历史记录仍会保留。" : "Withdraw this retained message; history is preserved.";
            actions.appendChild(createMessageAction(editLabel, "edit", () => options.onEdit(message)));
            actions.appendChild(createMessageAction(deleteLabel, "trash", () => options.onWithdraw(id), true));
        }
        footer.appendChild(actions);
        content.appendChild(footer);
        card.appendChild(content);
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

function productNode(tag, text = "", className = "") {
    const node = document.createElement(tag);
    node.textContent = text;
    node.className = className;
    return node;
}
function productButton(label, action, className = "btn secondary") {
    const node = productNode("button", label, className);
    node.type = "button";
    node.setAttribute("aria-label", label);
    node.addEventListener("click", action);
    return node;
}
function productName(project) {
    return project.name || project.path?.split(/[\\/]/).filter(Boolean).pop() || project.id || "—";
}
function productTime(timestamp, language, now = Date.now()) {
    if (typeof timestamp !== "number" || !Number.isFinite(timestamp) || timestamp <= 0)
        return translate("No activity observed yet", language);
    const seconds = Math.min(0, Math.round((timestamp - now) / 1000));
    const formatter = new Intl.RelativeTimeFormat(language === "zh-CN" ? "zh-CN" : "en", { numeric: "auto" });
    if (seconds > -60)
        return formatter.format(seconds, "second");
    if (seconds > -3600)
        return formatter.format(Math.round(seconds / 60), "minute");
    if (seconds > -86400)
        return formatter.format(Math.round(seconds / 3600), "hour");
    return formatter.format(Math.round(seconds / 86400), "day");
}
function productTitle(value) {
    const text = String(value || "").trim().split(/\r?\n/).find(Boolean) || "Untitled Session";
    return text.length > 110 ? text.slice(0, 109) + "…" : text;
}
function productActivity(activity, language) {
    if (!activity)
        return translate("No activity observed yet", language);
    if (typeof activity.summary === "string" && activity.summary)
        return productTitle(activity.summary);
    const kind = String(activity.kind || activity.tool_name || activity.tool || "");
    const label = /Explor|read|search|inspect/i.test(kind) ? "Reading project files"
        : /Edit|write|patch/i.test(kind) ? "Editing files" : /Validat|test|check|build/i.test(kind) ? "Running checks"
            : /Review|changes|diff/i.test(kind) ? "Reviewing changes" : /Running|job|process/i.test(kind) ? "Running tasks" : "Workspace activity";
    return translate(label, language);
}
function createProductProjectRow(project, options) {
    const tr = (value) => translate(value, options.language);
    const selected = project.id === options.selected;
    const row = productNode("article", "", "product-project-row" + (selected ? " selected" : ""));
    row.setAttribute("aria-label", productName(project));
    row.appendChild(productNode("span", productName(project).slice(0, 2).toUpperCase(), "product-avatar"));
    const body = productNode("div", "", "product-project-main");
    const title = productNode("div", "", "product-project-title");
    title.appendChild(productNode("h3", productName(project)));
    if (selected)
        title.appendChild(productNode("span", tr("Current"), "product-badge"));
    body.appendChild(title);
    const path = productNode("span", project.path || "—", "product-path");
    path.title = project.path || "";
    body.appendChild(path);
    const meta = productNode("div", "", "product-project-meta");
    const branch = productNode("span", "—");
    branch.title = tr("Git branch");
    meta.appendChild(branch);
    if (project.connected && options.git)
        options.git(project.id, branch);
    meta.appendChild(productNode("span", project.sessions ? String(project.sessions.active_sessions ?? 0) + (project.sessions.sessions_truncated ? "+" : "") + " " + tr("active sessions") : tr("Not checked")));
    meta.appendChild(productNode("span", productTime(project.sessions?.latest_updated_at ? project.sessions.latest_updated_at * 1000 : null, options.language)));
    body.appendChild(meta);
    row.appendChild(body);
    const open = productButton(tr("Open"), () => options.onOpen(project.client_id, project.id));
    open.setAttribute("aria-label", tr("Open Project") + " " + productName(project));
    open.dataset.action = "open-project";
    row.appendChild(open);
    return row;
}
function productDialog(title, language) {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = productNode("dialog", "", "product-dialog");
    dialog.setAttribute("aria-label", title);
    const close = () => { dialog.close(); dialog.remove(); if (previous?.isConnected)
        previous.focus(); };
    const header = productNode("header");
    header.appendChild(productNode("h2", title));
    header.appendChild(productButton(translate("Close", language), close));
    dialog.appendChild(header);
    const body = productNode("div", "", "product-dialog-body");
    dialog.appendChild(body);
    dialog.addEventListener("cancel", event => { event.preventDefault(); close(); });
    document.body.appendChild(dialog);
    dialog.showModal();
    return { dialog, body, close };
}

// UI state only. Project registration, authorization and Git remain canonical API operations.
class ProductWorkspace {
    constructor(services) {
        this.services = services;
        this.generation = 0;
        this.requests = new Set();
        this.gitCache = new Map();
        this.gitPending = new Map();
        this.gitRunning = 0;
        this.gitQueue = [];
        this.dialogs = new Set();
        this.query = "";
        this.runner = "";
        this.projectLanguage = "";
        this.projectSignature = "";
        this.projectList = null;
    }
    reset() {
        this.generation++;
        for (const request of this.requests)
            request.abort();
        this.requests.clear();
        this.gitCache.clear();
        this.gitPending.clear();
        for (const dialog of this.dialogs)
            dialog.close();
        this.dialogs.clear();
        this.query = "";
        this.runner = "";
        this.projectLanguage = "";
        this.projectSignature = "";
        this.projectList = null;
        document.getElementById("runtime-projects-content")?.replaceChildren();
        document.getElementById("runtime-activity-content")?.replaceChildren();
    }
    invalidateGit() { this.gitCache.clear(); }
    async git(project) {
        const cached = this.gitCache.get(project);
        if (cached && Date.now() - cached.at < 30000)
            return cached.value;
        const pending = this.gitPending.get(project);
        if (pending)
            return pending;
        const generation = this.generation;
        const task = (async () => {
            if (this.gitRunning >= 3)
                await new Promise(resolve => this.gitQueue.push(resolve));
            this.gitRunning++;
            const request = new AbortController();
            this.requests.add(request);
            try {
                if (generation !== this.generation)
                    return null;
                const result = await this.services.post("project-git", { project }, request.signal);
                if (generation !== this.generation || request.signal.aborted)
                    return null;
                if (result?.status === 401) {
                    this.services.unauthorized();
                    return null;
                }
                const value = result?.ok ? result.data : null;
                this.gitCache.set(project, { value, at: Date.now() });
                return value;
            }
            finally {
                this.requests.delete(request);
                this.gitRunning--;
                this.gitQueue.shift()?.();
                if (generation === this.generation)
                    this.gitPending.delete(project);
            }
        })();
        this.gitPending.set(project, task);
        return task;
    }
    attachGit(project, target) {
        const generation = this.generation;
        const language = this.services.context().language;
        void this.git(project).then(value => {
            if (generation !== this.generation || !target.isConnected)
                return;
            target.textContent = value?.branch || translate(value?.non_git_project ? "No Git repository" : "Not checked", language);
            target.title = value?.branch || translate("Git branch", language);
        });
    }
    renderProjects() {
        const root = document.getElementById("runtime-projects-content");
        if (!root)
            return;
        const context = this.services.context();
        const tr = (text) => translate(text, context.language);
        if (!this.projectList || !root.contains(this.projectList) || this.projectLanguage !== context.language) {
            root.replaceChildren();
            this.projectLanguage = context.language;
            this.projectSignature = "";
            const heading = productNode("header", "", "product-page-heading");
            heading.appendChild(productNode("h2", tr("Projects")));
            const add = productButton(tr("Add Project"), () => this.addProject(), "btn primary");
            add.dataset.action = "add-project";
            heading.appendChild(add);
            root.appendChild(heading);
            const filters = productNode("div", "", "product-filters");
            const runnerLabel = productNode("label", "Runner");
            runnerLabel.htmlFor = "product-project-runner";
            const select = productNode("select");
            select.id = "product-project-runner";
            select.setAttribute("aria-label", "Runner");
            select.appendChild(new Option(tr("All Runners"), ""));
            for (const runner of context.runners)
                select.appendChild(new Option(runner.client_id, runner.client_id));
            select.value = this.runner;
            select.addEventListener("change", () => { this.runner = select.value; this.renderProjects(); });
            const searchLabel = productNode("label", tr("Search projects"));
            searchLabel.htmlFor = "product-project-query";
            const search = productNode("input");
            search.type = "search";
            search.id = "product-project-query";
            search.maxLength = 200;
            search.value = this.query;
            search.addEventListener("input", () => { this.query = search.value; this.renderProjects(); });
            filters.append(runnerLabel, select, searchLabel, search);
            root.appendChild(filters);
            this.projectList = productNode("div", "", "product-project-list");
            root.appendChild(this.projectList);
        }
        const signature = JSON.stringify([context.projects, context.selectedProject, context.available, this.query, this.runner]);
        if (signature === this.projectSignature)
            return;
        this.projectSignature = signature;
        const activeName = this.projectList.contains(document.activeElement) ? document.activeElement?.getAttribute("aria-label") : null;
        this.projectList.replaceChildren();
        const rows = context.projects.filter(project => (!this.runner || this.runner === project.client_id) && `${productName(project)} ${project.path || ""}`.toLocaleLowerCase().includes(this.query.trim().toLocaleLowerCase()))
            .sort((a, b) => (b.sessions?.latest_updated_at || 0) - (a.sessions?.latest_updated_at || 0));
        let lastRunner = "";
        for (const project of rows) {
            if (context.runners.length > 1 && project.client_id !== lastRunner) {
                this.projectList.appendChild(productNode("p", "Runner · " + project.client_id, "product-runner-label"));
                lastRunner = project.client_id;
            }
            this.projectList.appendChild(createProductProjectRow(project, { language: context.language, selected: context.selectedProject, onOpen: this.services.onProject, git: (project, target) => this.attachGit(project, target) }));
        }
        if (!rows.length)
            this.projectList.appendChild(productNode("p", tr(!context.available ? "Projects unavailable. Refresh to try again." : this.query || this.runner ? "No matching projects" : "No projects yet"), "product-empty"));
        if (activeName)
            Array.from(this.projectList.querySelectorAll("button")).find(value => value.getAttribute("aria-label") === activeName)?.focus();
    }
    addProject() {
        const context = this.services.context();
        const tr = (text) => translate(text, context.language);
        const generation = this.generation;
        const popup = productDialog(tr("Add Project"), context.language);
        this.dialogs.add(popup);
        const form = productNode("form");
        const request = new AbortController();
        const runnerLabel = productNode("label", "Runner");
        runnerLabel.htmlFor = "product-add-runner";
        const runner = productNode("select");
        runner.id = "product-add-runner";
        runner.required = true;
        for (const row of context.runners)
            runner.appendChild(new Option(row.client_id, row.client_id));
        if (this.runner)
            runner.value = this.runner;
        const pathLabel = productNode("label", tr("Project folder"));
        pathLabel.htmlFor = "product-add-path";
        const path = productNode("input");
        path.id = "product-add-path";
        path.required = true;
        path.maxLength = 4096;
        path.autocomplete = "off";
        path.spellcheck = false;
        path.title = tr("Project folder");
        path.placeholder = tr("Absolute folder path on the selected Runner");
        const message = productNode("p", "", "product-form-message");
        message.setAttribute("role", "status");
        const submit = productNode("button", tr("Add Project"), "btn primary");
        submit.type = "submit";
        submit.disabled = !context.runners.length;
        form.append(runnerLabel, runner, pathLabel, path, message, submit);
        popup.body.appendChild(form);
        let pending = false;
        form.addEventListener("submit", async (event) => {
            event.preventDefault();
            if (pending || !path.value.trim() || !runner.value || generation !== this.generation)
                return;
            pending = true;
            submit.disabled = true;
            runner.disabled = true;
            path.disabled = true;
            this.requests.add(request);
            message.textContent = tr("Adding project…");
            try {
                const result = await this.services.registerProject({ client_id: runner.value, path: path.value.trim() }, request.signal);
                if (generation !== this.generation || request.signal.aborted || !popup.dialog.isConnected)
                    return;
                if (result?.status === 401) {
                    this.services.unauthorized();
                    return;
                }
                if (result?.ok && result.data?.success === true) {
                    popup.close();
                    this.dialogs.delete(popup);
                    this.services.refresh();
                    return;
                }
                message.textContent = tr(result?.status === 0 || result === null ? "The result could not be confirmed. Refresh Projects before trying again." : "Project could not be added. Check the folder and Runner access.");
                message.setAttribute("role", "alert");
                // No automatic write replay, including after transport failure.
            }
            finally {
                this.requests.delete(request);
                pending = false;
                if (generation === this.generation && popup.dialog.isConnected) {
                    submit.disabled = false;
                    runner.disabled = false;
                    path.disabled = false;
                }
            }
        });
        path.focus();
    }
    renderActivity() {
        const root = document.getElementById("runtime-activity-content");
        if (!root)
            return;
        const context = this.services.context();
        const tr = (text) => translate(text, context.language);
        root.replaceChildren();
        const heading = productNode("header", "", "product-page-heading");
        heading.appendChild(productNode("h2", tr("Activity")));
        root.appendChild(heading);
        const rows = context.sessions.map(session => ({ at: Number(session.updated_at) * 1000, kind: "session", value: session })).concat(context.windows.map(window => ({ at: window.last_meaningful_activity_at_ms || window.last_seen_at_ms, kind: "window", value: window })))
            .sort((a, b) => b.at - a.at).slice(0, 40);
        for (const row of rows) {
            const entry = productButton("", () => row.kind === "session" ? this.services.onSession(row.value) : this.services.onWindow(row.value.client_window_key), "product-activity-entry");
            const body = productNode("div");
            body.appendChild(productNode("strong", row.kind === "session" ? productTitle(row.value.title) : tr("Windows") + " · " + String(row.value.client_window_key).slice(-12)));
            body.appendChild(productNode("span", row.kind === "session" ? productActivity(row.value.current_activity || row.value.last_activity, context.language) : productName(context.projects.find(project => project.id === row.value.last_project) || {}), "muted"));
            entry.setAttribute("aria-label", body.textContent || tr("Activity"));
            entry.append(body, productNode("time", productTime(row.at, context.language)));
            root.appendChild(entry);
        }
        if (!rows.length)
            root.appendChild(productNode("p", tr(context.available ? "No activity observed yet" : "Activity unavailable. Refresh to try again."), "product-empty"));
    }
}

const PRODUCT_EXTENSION_TABS = ["instructions", "skills", "plugins"];
class ProductExtensions {
    constructor(services) {
        this.services = services;
        this.project = "";
        this.tab = "instructions";
        this.generation = 0;
        this.request = null;
        this.catalog = null;
        this.failed = false;
        this.loading = false;
        this.language = "";
        this.body = null;
        this.dialogs = new Set();
    }
    reset() {
        this.generation++;
        this.request?.abort();
        this.request = null;
        for (const dialog of this.dialogs)
            dialog.close();
        this.dialogs.clear();
        this.project = "";
        this.catalog = null;
        this.failed = false;
        this.loading = false;
        this.body = null;
        this.language = "";
        document.getElementById("runtime-extensions-content")?.replaceChildren();
    }
    open() {
        const context = this.services.context();
        const project = context.projects.some(row => row.id === this.project) ? this.project : context.selectedProject || context.projects[0]?.id || "";
        if (this.project !== project || this.language !== context.language || !this.body?.isConnected) {
            this.request?.abort();
            this.generation++;
            this.project = project;
            this.catalog = null;
            this.language = context.language;
            this.renderShell();
            void this.refresh();
        }
    }
    async refresh() {
        this.request?.abort();
        const request = new AbortController();
        this.request = request;
        const generation = ++this.generation;
        const project = this.project;
        this.loading = Boolean(project);
        this.failed = false;
        this.catalog = null;
        this.render();
        if (!project)
            return;
        const result = await this.services.post("extensions", { project }, request.signal);
        if (generation !== this.generation || request.signal.aborted)
            return;
        this.loading = false;
        if (result?.status === 401) {
            this.services.unauthorized();
            return;
        }
        this.failed = !result?.ok;
        this.catalog = result?.ok ? result.data : null;
        this.render();
    }
    renderShell() {
        const root = document.getElementById("runtime-extensions-content");
        if (!root)
            return;
        const context = this.services.context();
        const tr = (text) => translate(text, context.language);
        root.replaceChildren();
        const heading = productNode("header", "", "product-page-heading");
        heading.appendChild(productNode("h2", tr("Extensions")));
        heading.appendChild(productButton(tr("Refresh"), () => { void this.refresh(); }));
        root.appendChild(heading);
        const filter = productNode("div", "", "product-filters");
        const label = productNode("label", tr("Project"));
        label.htmlFor = "product-extensions-project";
        const select = productNode("select");
        select.id = "product-extensions-project";
        for (const project of context.projects)
            select.appendChild(new Option(productName(project), project.id));
        select.value = this.project;
        select.addEventListener("change", () => { this.project = select.value; void this.refresh(); });
        filter.append(label, select);
        root.appendChild(filter);
        const tabs = productNode("div", "", "product-tabs");
        tabs.setAttribute("role", "tablist");
        tabs.setAttribute("aria-label", tr("Extensions"));
        const selectTab = (tab) => {
            this.tab = tab;
            for (const control of Array.from(tabs.querySelectorAll("button"))) {
                const selected = control.dataset.tab === tab;
                control.setAttribute("aria-selected", String(selected));
                control.tabIndex = selected ? 0 : -1;
            }
            this.render();
        };
        for (const tab of PRODUCT_EXTENSION_TABS) {
            const control = productButton(tr(tab === "instructions" ? "Instructions" : tab === "skills" ? "Skills" : "Plugins"), () => selectTab(tab), "product-tab");
            control.id = "product-tab-" + tab;
            control.dataset.tab = tab;
            control.setAttribute("role", "tab");
            control.setAttribute("aria-controls", "product-extensions-panel");
            control.setAttribute("aria-selected", String(this.tab === tab));
            control.tabIndex = this.tab === tab ? 0 : -1;
            control.addEventListener("keydown", event => { if (!['ArrowLeft', 'ArrowRight'].includes(event.key))
                return; event.preventDefault(); const next = PRODUCT_EXTENSION_TABS[(PRODUCT_EXTENSION_TABS.indexOf(tab) + (event.key === 'ArrowRight' ? 1 : 2)) % 3]; selectTab(next); document.getElementById("product-tab-" + next)?.focus(); });
            tabs.appendChild(control);
        }
        root.appendChild(tabs);
        this.body = productNode("section");
        this.body.id = "product-extensions-panel";
        this.body.setAttribute("role", "tabpanel");
        root.appendChild(this.body);
    }
    render() {
        if (!this.body)
            return;
        const context = this.services.context();
        const tr = (text) => translate(text, context.language);
        this.body.replaceChildren();
        this.body.setAttribute("aria-labelledby", "product-tab-" + this.tab);
        if (this.loading) {
            const status = productNode("p", tr("Refreshing…"), "muted");
            status.setAttribute("role", "status");
            this.body.appendChild(status);
            return;
        }
        if (this.failed) {
            const error = productNode("p", tr("Could not refresh. Check the connection and try again."), "error");
            error.setAttribute("role", "alert");
            this.body.appendChild(error);
            return;
        }
        if (!this.project) {
            this.body.appendChild(productNode("p", tr("Add a project to manage its extensions."), "product-empty"));
            return;
        }
        if (!this.catalog)
            return;
        if (this.tab === "instructions") {
            const files = Array.isArray(this.catalog.instructions?.files) ? this.catalog.instructions.files : [];
            for (const file of files) {
                const title = file.source_scope === "runner" ? tr("Global instructions") : "Project " + String(file.path).split(/[\\/]/).pop();
                const row = this.row(title, (file.source_scope === "runner" ? "Runner" : productName(context.projects.find(project => project.id === this.project) || {})) + " · " + tr("Available"));
                const details = productNode("details");
                details.append(productNode("summary", tr("Details")), productNode("code", String(file.path)));
                row.firstElementChild?.appendChild(details);
                const open = productButton(tr("Open"), () => { void this.openInstruction(file); });
                open.setAttribute("aria-label", tr("Open") + " " + title);
                row.appendChild(open);
                this.body.appendChild(row);
            }
            if (!files.length)
                this.body.appendChild(productNode("p", tr(this.catalog.instructions?.scan_complete ? "No instructions configured" : "Unavailable"), "product-empty"));
            if (this.catalog.instructions?.truncated)
                this.body.appendChild(productNode("p", tr("Showing recent results"), "muted small"));
        }
        else if (this.tab === "skills") {
            const result = this.catalog.skills;
            const skills = Array.isArray(result?.catalog?.skills) ? result.catalog.skills : [];
            for (const skill of skills) {
                const row = this.row(String(skill.name), tr("Available") + " · " + (skill.source_scope === "project" ? tr("Project") : "Runner"));
                if (skill.description)
                    row.firstElementChild?.appendChild(productNode("p", String(skill.description), "muted"));
                this.body.appendChild(row);
            }
            if (!skills.length)
                this.body.appendChild(productNode("p", tr(result?.available ? "Nothing installed yet" : "Unavailable"), "product-empty"));
            if (result?.catalog?.truncated)
                this.body.appendChild(productNode("p", tr("Showing recent results"), "muted small"));
        }
        else {
            const result = this.catalog.plugins;
            const plugins = result?.catalog?.plugins || result?.catalog?.providers || [];
            for (const plugin of Array.isArray(plugins) ? plugins : []) {
                const id = String(plugin.id || plugin.plugin || "");
                const status = plugin.status === "error" ? "Unavailable" : plugin.status === "ready" || plugin.status === "available" ? "Available" : "Registered";
                const row = this.row(String(plugin.name || id), tr(status) + " · " + (plugin.tool_count ?? plugin.tools?.length ?? "—") + " " + tr("tools"));
                if (id && this.catalog.can_reload_plugins)
                    row.appendChild(productButton(tr("Reload"), () => { void this.reloadPlugin(id, row); }));
                this.body.appendChild(row);
            }
            if (!plugins.length)
                this.body.appendChild(productNode("p", tr(result?.available ? "Nothing installed yet" : "Unavailable"), "product-empty"));
        }
    }
    row(title, subtitle) {
        const row = productNode("article", "", "product-extension-row");
        const body = productNode("div");
        body.append(productNode("h3", title), productNode("span", subtitle, "muted small"));
        row.appendChild(body);
        return row;
    }
    async openInstruction(file) {
        const project = this.project;
        const generation = this.generation;
        const language = this.services.context().language;
        const popup = productDialog(file.source_scope === "runner" ? translate("Global instructions", language) : String(file.path), language);
        this.dialogs.add(popup);
        popup.body.appendChild(productNode("p", translate("Refreshing…", language)));
        const result = await this.services.post("instruction", { project, source_scope: file.source_scope, path: file.path, fingerprint: file.fingerprint }, this.request?.signal);
        if (generation !== this.generation || !popup.dialog.isConnected) {
            popup.close();
            this.dialogs.delete(popup);
            return;
        }
        if (result?.status === 401) {
            this.services.unauthorized();
            return;
        }
        popup.body.replaceChildren();
        if (!result?.ok)
            popup.body.appendChild(productNode("p", translate("Could not refresh. Check the connection and try again.", language), "error"));
        else {
            popup.body.appendChild(productNode("pre", String(result.data?.content || ""), "product-instructions"));
            if (result.data?.truncated)
                popup.body.appendChild(productNode("p", translate("Showing recent results", language)));
        }
    }
    async reloadPlugin(plugin, row) {
        const project = this.project;
        const generation = this.generation;
        const language = this.services.context().language;
        const control = row.querySelector("button");
        if (!control || control.disabled)
            return;
        control.disabled = true;
        const result = await this.services.post("plugin-reload", { project, plugin }, this.request?.signal);
        if (generation !== this.generation || !row.isConnected)
            return;
        if (result?.status === 401) {
            this.services.unauthorized();
            return;
        }
        if (result?.ok) {
            await this.refresh();
            return;
        }
        const error = productNode("p", translate("Reload could not be confirmed. Refresh before trying again.", language), "error");
        error.setAttribute("role", "alert");
        row.appendChild(error);
        control.disabled = false;
    }
}

// These projections use only authorized Workflow Session evidence. Window
// observations never contribute to Session status, completion or authority.
function workspaceSessionGroups(sessions) {
    const rows = [...sessions].sort((a, b) => Number(b.updated_at || 0) - Number(a.updated_at || 0));
    const attention = rows.filter(row => pendingAttentionCount(row.overview?.attention) > 0 || row.overview?.validation?.state === "failed");
    const working = rows.filter(row => row.running_call === true || Number(row.running_jobs) > 0);
    const completed = rows.filter(row => row.lifecycle === "closed");
    return { attention, working, completed, recent: rows };
}
function workspaceSessionEvidence(detail) {
    const activity = Array.isArray(detail?.activity) ? detail.activity : [];
    // Paths from exploration are not changed files. This is retained edit evidence,
    // not a claim about the current Git working tree.
    const files = [...new Set(activity.filter((row) => row.kind === "Edited")
            .flatMap((row) => Array.isArray(row.paths) ? row.paths.map(String) : []))];
    const jobs = activity.filter((row) => row.job_id);
    const completed = activity.filter((row) => typeof row.finished_at === "number" && row.state === "succeeded" && !row.job_handoff);
    return { files, jobs, completed: completed.slice(-5).reverse() };
}
function workspaceNode(tag, text = "", className = "") {
    const node = document.createElement(tag);
    node.textContent = text;
    node.className = className;
    return node;
}
function workspaceButton(text, action, callback) {
    const button = workspaceNode("button", text, "workspace-action");
    button.type = "button";
    button.dataset.action = action;
    workspaceControlKeys.set(button, action);
    button.addEventListener("click", callback);
    return button;
}
function renderWorkspaceHome(node, options) {
    if (!node)
        return;
    // Preserve focused controls on polling when the evidence has not changed.
    const signature = JSON.stringify([options.language, options.projects, options.project, options.sessions,
        options.sessionsAvailable, options.sessionsStatus, options.windows, options.windowAvailability, options.windowStatus, options.overview]);
    if (workspaceHomeSignatures.get(node) === signature)
        return;
    workspaceHomeSignatures.set(node, signature);
    const focusedKey = node.contains(document.activeElement) ? workspaceControlKeys.get(document.activeElement) : null;
    node.replaceChildren();
    const tr = (text) => translate(text, options.language);
    const heading = workspaceNode("header", "", "product-page-heading");
    const title = workspaceNode("div");
    title.appendChild(workspaceNode("p", tr("Workspace"), "eyebrow"));
    title.appendChild(workspaceNode("h2", tr(options.overview ? "WebPi Ready" : "Workspace")));
    heading.appendChild(title);
    heading.appendChild(workspaceButton(tr("Add Project"), "workspace-add-project", options.onAddProject || options.onSearch));
    node.appendChild(heading);
    const status = workspaceNode("dl", "", "product-status-strip");
    status.setAttribute("aria-label", tr("Workspace status"));
    const runners = Array.isArray(options.overview?.runners) ? options.overview.runners : [];
    for (const [label, value] of [["Server", tr(options.overview ? "Running" : "Not checked")], ["Runner", options.overview ? String(runners.filter((runner) => runner.connected).length) + " " + tr("online") : tr("Not checked")], ["Projects", String(options.projects.length)]]) {
        const item = workspaceNode("div");
        item.appendChild(workspaceNode("dt", tr(label)));
        item.appendChild(workspaceNode("dd", value));
        status.appendChild(item);
    }
    node.appendChild(status);
    const projects = workspaceNode("section", "", "product-section");
    const projectHeading = workspaceNode("header", "", "product-section-heading");
    projectHeading.appendChild(workspaceNode("h3", tr("Recent Projects")));
    projectHeading.appendChild(workspaceButton(tr("All Projects"), "workspace-find-project", options.onSearch));
    projects.appendChild(projectHeading);
    const recentProjects = [...options.projects].sort((a, b) => Number(b.sessions?.latest_updated_at || 0) - Number(a.sessions?.latest_updated_at || 0));
    for (const project of recentProjects.slice(0, 4))
        projects.appendChild(createProductProjectRow(project, { language: options.language, selected: options.project?.id, onOpen: options.onProject, git: options.git }));
    if (!recentProjects.length)
        projects.appendChild(workspaceNode("p", tr(options.overview ? "No projects yet" : "Loading projects…"), "muted"));
    node.appendChild(projects);
    const activity = workspaceNode("section", "", "product-section");
    const activityHeading = workspaceNode("header", "", "product-section-heading");
    activityHeading.appendChild(workspaceNode("h3", tr("Recent Activity")));
    activityHeading.appendChild(workspaceButton(tr("Windows"), "workspace-open-windows", options.onWindows));
    activity.appendChild(activityHeading);
    if (!options.sessionsAvailable)
        activity.appendChild(workspaceNode("p", options.sessionsStatus || tr("Activity unavailable. Refresh to try again."), "muted"));
    const sessions = workspaceSessionGroups(options.sessions).recent.slice(0, 5);
    for (const session of sessions) {
        const button = workspaceButton("", "workspace-open-session", () => options.onSession(session));
        workspaceControlKeys.set(button, "session:" + String(session.session_id));
        button.className = "product-activity-row";
        button.appendChild(workspaceNode("span", tr("Workflow Sessions"), "product-badge"));
        button.appendChild(workspaceNode("strong", productTitle(session.title)));
        button.appendChild(workspaceNode("span", productTime(Number(session.updated_at) * 1000, options.language), "muted small"));
        activity.appendChild(button);
    }
    for (const window of options.windows.slice(0, 2)) {
        const button = workspaceButton("", "workspace-open-window", () => options.onWindow(String(window.client_window_key)));
        workspaceControlKeys.set(button, "window:" + String(window.client_window_key));
        button.className = "product-activity-row";
        button.appendChild(workspaceNode("span", tr("Windows"), "product-badge"));
        button.appendChild(workspaceNode("strong", String(window.client_window_key).slice(-12)));
        button.appendChild(workspaceNode("span", productTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms, options.language), "muted small"));
        activity.appendChild(button);
    }
    if (options.sessionsAvailable && !sessions.length && !options.windows.length)
        activity.appendChild(workspaceNode("p", tr("No activity observed yet"), "muted"));
    node.appendChild(activity);
    if (focusedKey)
        Array.from(node.querySelectorAll("button")).find(button => workspaceControlKeys.get(button) === focusedKey)?.focus();
}
const workspaceControlKeys = new WeakMap();
const workspaceHomeSignatures = new WeakMap();
function renderWorkspaceEvidence(node, detail, language) {
    if (!node)
        return;
    node.replaceChildren();
    const tr = (text) => translate(text, language);
    const evidence = workspaceSessionEvidence(detail);
    const overview = workflowSessionOverviewPresentation(detail?.overview);
    const facts = [
        ["Observed work", localizedWorkflowText(overview.workText, language)],
        ["Attention", localizedWorkflowText(overview.attentionText, language)],
        ["Validation", localizedWorkflowText(overview.validationText, language)],
        ["Running Jobs", String(detail.running_jobs ?? 0) + (detail.running_jobs_complete === false ? " · " + tr("partial scan") : "")],
    ];
    for (const [label, text] of facts) {
        const fact = workspaceNode("div", "", "workspace-evidence-fact");
        fact.appendChild(workspaceNode("dt", tr(label)));
        fact.appendChild(workspaceNode("dd", text));
        node.appendChild(fact);
    }
    const edits = workspaceNode("div", "", "workspace-evidence-fact");
    edits.appendChild(workspaceNode("dt", tr("Files in retained edits")));
    edits.appendChild(workspaceNode("dd", evidence.files.length ? evidence.files.join(" · ") : tr("No file edits in loaded activity.")));
    node.appendChild(edits);
    const jobs = workspaceNode("div", "", "workspace-evidence-fact");
    jobs.appendChild(workspaceNode("dt", tr("Recent Job evidence")));
    jobs.appendChild(workspaceNode("dd", evidence.jobs.length ? evidence.jobs.slice(-3).reverse().map(row => activityDescription(row, language)).join(" · ") : tr("No Jobs in loaded activity.")));
    node.appendChild(jobs);
    const completed = workspaceNode("div", "", "workspace-evidence-fact");
    completed.appendChild(workspaceNode("dt", tr("Recently completed")));
    completed.appendChild(workspaceNode("dd", evidence.completed.length ? evidence.completed.slice(0, 3).map(row => activityDescription(row, language)).join(" · ") : tr("No successful completions in loaded activity.")));
    node.appendChild(completed);
}
function workspaceListChip(parent, text, extraClass = "") {
    const chip = workspaceNode("span", text, "chip " + extraClass);
    parent.appendChild(chip);
    return chip;
}
function renderWorkspaceSessionList(node, visible, selected, language, onSelect) {
    const tr = (text) => translate(text, language);
    for (const session of visible) {
        const id = String(session && session.session_id || "");
        if (!id)
            continue;
        const wrapper = document.createElement("li");
        const item = document.createElement("button");
        item.type = "button";
        item.dataset.action = "select-work-session";
        item.className = "session-card" + (id === selected ? " selected" : "");
        if (id === selected)
            item.setAttribute("aria-current", "true");
        const icon = document.createElement("span");
        icon.className = "session-card-icon";
        icon.setAttribute("aria-hidden", "true");
        icon.appendChild(runtimeIcon("message"));
        const main = document.createElement("div");
        main.className = "session-card-main";
        const title = document.createElement("div");
        title.className = "session-title";
        title.textContent = session.title ? String(session.title) : id;
        const meta = document.createElement("div");
        meta.className = "chips session-meta";
        const lifecycle = String(session.lifecycle || "unknown");
        if (lifecycle !== "active")
            workspaceListChip(meta, tr(lifecycle));
        const liveness = formatLivenessPresentation(session, language);
        const livenessChip = workspaceListChip(meta, liveness.label, liveness.state === "working" ? "tone-runtime" : liveness.state === "attention" ? "tone-warn" : "");
        livenessChip.title = liveness.tooltip;
        workspaceListChip(meta, formatUpdatedTime(session.updated_at, language));
        main.appendChild(title);
        main.appendChild(meta);
        item.appendChild(icon);
        item.appendChild(main);
        const select = () => onSelect(id);
        item.addEventListener("click", select);
        wrapper.appendChild(item);
        node.appendChild(wrapper);
    }
}
function installWorkspaceCommands(options) {
    const dialog = document.getElementById("runtime-command-dialog");
    const input = document.getElementById("runtime-command-query");
    const results = document.getElementById("runtime-command-results");
    if (!dialog || !input || !results)
        return;
    let returnFocus = null;
    const render = () => {
        const language = options.language();
        const query = input.value.trim().toLocaleLowerCase();
        results.replaceChildren();
        const entries = [
            ...[["Home", "home"], ["Projects", "projects"], ["Workflow Sessions", "sessions"], ["Windows", "windows"], ["Activity", "activity"], ["Extensions", "extensions"], ["Advanced", "operations"]]
                .map(([label, view]) => ({ label: translate(label, language), action: "command-" + view, run: () => options.onView(view) })),
            ...options.projects().map(project => ({ label: String(project.name || project.id) + " · " + String(project.client_id || ""), action: "command-project", run: () => options.onProject(String(project.client_id || ""), String(project.id || "")) })),
        ];
        for (const entry of entries.filter(entry => entry.label.toLocaleLowerCase().includes(query))) {
            results.appendChild(workspaceButton(entry.label, entry.action, () => { dialog.close(); if (options.available())
                entry.run(); }));
        }
        if (!results.childElementCount)
            results.appendChild(workspaceNode("p", translate("No matching destinations.", language), "muted"));
    };
    const open = () => {
        if (!options.available() || dialog.open)
            return;
        returnFocus = document.activeElement;
        input.value = "";
        render();
        dialog.showModal();
        input.focus();
    };
    document.getElementById("runtime-open-commands")?.addEventListener("click", open);
    document.getElementById("runtime-command-close")?.addEventListener("click", () => dialog.close());
    dialog.addEventListener("close", () => returnFocus?.focus());
    input.addEventListener("input", render);
    input.addEventListener("keydown", event => {
        if (event.isComposing)
            return;
        if (event.key === "ArrowDown") {
            event.preventDefault();
            results.querySelector("button")?.focus();
        }
        if (event.key === "Enter") {
            event.preventDefault();
            results.querySelector("button")?.click();
        }
    });
    document.addEventListener("keydown", event => {
        if ((event.metaKey || event.ctrlKey) && event.shiftKey && !event.altKey && !event.isComposing && event.key.toLowerCase() === "k") {
            event.preventDefault();
            open();
        }
    });
}
function renderWorkspaceOverview(overview, language) {
    const view = workflowSessionOverviewPresentation(overview);
    for (const [id, text] of [
        ["work", view.workText], ["attention", view.attentionText],
        ["validation", view.validationText + (typeof view.validationAt === "number" ? " · " + formatUpdatedTime(view.validationAt, language) : "")],
        ["progress", view.progressText + (typeof view.progressAt === "number" ? " · " + formatUpdatedTime(view.progressAt, language) : "")],
    ]) {
        const node = document.getElementById("runtime-overview-" + id);
        if (node)
            node.textContent = localizedWorkflowText(text, language);
    }
    for (const [id, tone] of [["validation", view.validationTone], ["attention", view.attentionTone]]) {
        const node = document.getElementById("runtime-overview-" + id + "-card");
        for (const name of ["pass", "warn", "fail", "muted"])
            node?.classList.toggle("tone-card-" + name, tone === name);
    }
}

const API_BASE = RUNTIME_API_BASE;
const apiClient = new RuntimeApiClient(API_BASE);
const REFRESH_MS = 30000;
const WINDOW_REFRESH_MS = 3000;
const COLLABORATION_WAIT_SECS = 25;
const PROJECT_SEARCH_DEBOUNCE_MS = 200;
const MOBILE_NAVIGATION_MEDIA = "(max-width: 900px)";
const WIDE_CONTEXT_MEDIA = "(min-width: 1600px)";
let contextUserIntent = null;
// Verified localization mapping: "Close session context": "关闭会话上下文"
const appearanceMedia = window.matchMedia(APPEARANCE_MEDIA_QUERY);
let runtimeLanguage = languagePreference(document.documentElement.dataset.language);
const staticTextSources = [];
const staticAttributeSources = [];
let token = "";
let rememberCredentialForTab = true;
let timer = 0;
let overviewAbort = null;
let projectsAbort = null;
let sessionsAbort = null;
let detailAbort = null;
let collaborationAbort = null;
let windowsAbort = null;
let windowDetailAbort = null;
let windowTimer = 0;
let windowRows = [];
let windowAvailability = "idle";
let windowVisibilityScope = "principal";
let selectedWindowKey = "";
let selectedWindowDetail = null;
const PROJECT_WINDOW_LIMIT = 10;
let projectWindowsAbort = null;
let projectWindowRows = [];
let projectWindowAvailability = "idle";
let projectWindowTruncated = false;
let projectWindowTotal = 0;
let projectWindowProjectId = "";
let renderedProjectWindowSignature = "";
let projectRows = [];
let homeProjectRows = [];
let runnerRows = [];
let recentSessionRows = [];
let runtimeOverviewSnapshot = null;
let recentSessionMetaSnapshot = null;
let projectSearch = "";
let projectDeviceFilter = "";
let projectSearchTimer = 0;
let collaborationReplyTo = "";
let renderedCollaborationMessageIds = new Set();
let locallyAuthoredCollaborationMessageIds = new Set();
let workspaceView = "home";
let collaborationFollowLatest = true;
let collaborationPendingMessages = 0;
let refreshInFlight = false;
let projectRowsTotal = 0;
let projectRowsTruncated = false;
let knownProjectDevices = [];
let selectedProjectSnapshot = null;
let sessionRows = [];
let sessionAvailability = "idle";
let sessionListMetaSnapshot = { total: 0, truncated: false };
const state = initialRuntimeConsoleState();
let renderedProjectSelectorsSignature = "";
let renderedRunnerFleetSignature = "";
let renderedRecentSessionsSignature = "";
let renderedSessionListSignature = "";
let renderedCollaborationSignature = "";
let renderedCommunicationSurfaceSignature = "";
let communicationAgents = [];
let communicationConversations = [];
let communicationDetail = null;
let communicationInbox = [];
let selectedCommunicationAgentId = "";
let selectedCommunicationConversationId = "";
let communicationReadAvailable = null;
let communicationManageAvailable = null;
let communicationGeneration = 0;
const communicationRefreshCoordinator = new RuntimeCommunicationRefreshCoordinator(performCommunicationRefresh);
const communicationEndpoints = new Map();
const pendingEndpointAttach = new Map();
let pendingAgentCreate = null;
let pendingConversationCreate = null;
let pendingConversationMessage = null;
const pageAttachmentId = "runtime-console-" + operationKey("page");
const productProjectApi = new RuntimeApiClient("/api/projects/");
const productServices = {
    context: () => ({ language: runtimeLanguage, projects: homeProjectRows, runners: runnerRows,
        sessions: recentSessionRows, windows: windowRows.length ? windowRows : projectWindowRows,
        selectedProject: state.selectedProject || "", available: Boolean(runtimeOverviewSnapshot),
    }),
    post: (path, payload, signal) => api(path, payload, signal),
    registerProject: (payload, signal) => { productProjectApi.setToken(token); return productProjectApi.post("resolve-or-register", payload, signal); },
    onProject: (runner, project) => { switchProject(runner, project); applyWorkspaceView("sessions"); },
    onSession: session => selectRecentSession(session), onWindow: openWindowInspector,
    refresh: () => { void refreshAll(); }, unauthorized: () => lock(tr("Your access key is no longer valid. Connect again.")),
};
const productWorkspace = new ProductWorkspace(productServices);
const productExtensions = new ProductExtensions(productServices);
function el(id) {
    return document.getElementById(id);
}
function setText(id, value) {
    const node = el(id);
    if (node)
        node.textContent = value === null || value === undefined ? "—" : String(value);
}
function show(id, visible) {
    const node = el(id);
    if (node)
        node.hidden = !visible;
}
function tr(source) {
    return translate(source, runtimeLanguage);
}
function translatedStaticNodeValue(source) {
    return translateStaticNodeValue(source, runtimeLanguage);
}
function captureStaticUiSources() {
    if (staticTextSources.length || staticAttributeSources.length)
        return;
    const root = document.body;
    if (!root)
        return;
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let node = walker.nextNode();
    while (node) {
        const textNode = node;
        const source = textNode.nodeValue || "";
        if (source.trim())
            staticTextSources.push({ node: textNode, source });
        node = walker.nextNode();
    }
    for (const element of Array.from(root.querySelectorAll("*"))) {
        for (const name of ["placeholder", "title", "aria-label"]) {
            const source = element.getAttribute(name);
            if (source)
                staticAttributeSources.push({ node: element, name, source });
        }
    }
}
function renderLanguageSensitiveUi() {
    renderRuntimeOverviewMetrics(runtimeOverviewSnapshot);
    if (projectRows.length || knownProjectDevices.length || runnerRows.length) {
        renderProjectSelectors(projectRows, projectRowsTruncated);
    }
    else {
        renderSelectedProjectIdentity();
    }
    renderRunnerFleet(runnerRows);
    renderRecentSessions(recentSessionRows, recentSessionMetaSnapshot);
    if (state.selectedProject) {
        renderSessionList(sessionRows, sessionListMetaSnapshot);
        renderProjectWindows();
    }
    renderWindowList();
    renderWindowDetail(selectedWindowDetail);
    const snapshot = state.workflow?.snapshot;
    if (snapshot)
        renderDetail(snapshot, false);
    else if (!state.workflow?.selectedSessionId)
        hideDetail();
    if (!snapshot)
        renderCollaboration(undefined, false);
    renderCommunicationSurface();
    syncCollaborationComposer();
    renderWorkspaceHeading();
    renderHome();
    setRuntimeConnectionState(token ? "connected" : "disconnected");
}
function applyLanguage(language, persist = true, rerender = true) {
    runtimeLanguage = languagePreference(language);
    document.documentElement.lang = runtimeLanguage;
    document.documentElement.dataset.language = runtimeLanguage;
    document.title = tr("WebPi — Workspace");
    for (const source of staticTextSources)
        source.node.nodeValue = translatedStaticNodeValue(source.source);
    for (const source of staticAttributeSources)
        source.node.setAttribute(source.name, tr(source.source));
    const nextLanguageLabel = runtimeLanguage === "zh-CN" ? "EN" : "中";
    const nextLanguageTitle = runtimeLanguage === "zh-CN" ? "切换到英文" : "Switch to Chinese";
    document.querySelectorAll("[data-language-toggle-label]").forEach((label) => {
        label.textContent = nextLanguageLabel;
    });
    document.querySelectorAll("[data-language-toggle]").forEach((button) => {
        button.title = nextLanguageTitle;
        button.setAttribute("aria-label", nextLanguageTitle);
    });
    applyAppearance(parseAppearancePreference(document.documentElement.dataset.theme), false);
    if (persist) {
        try {
            window.localStorage.setItem(LANGUAGE_STORAGE_KEY, runtimeLanguage);
        }
        catch { /* Language remains active when storage is unavailable. */ }
    }
    if (rerender)
        renderLanguageSensitiveUi();
}
function parseAppearancePreference(value) {
    return appearancePreference(value);
}
function readStoredAppearance() {
    return loadAppearancePreference();
}
function computeResolvedAppearance(preference) {
    return resolvedAppearance(preference, appearanceMedia.matches);
}
function applyAppearance(preference, persist = true) {
    const resolved = computeResolvedAppearance(preference);
    document.documentElement.dataset.theme = preference;
    document.documentElement.dataset.resolvedTheme = resolved;
    document.querySelector('meta[name="theme-color"]')?.setAttribute("content", resolved === "light" ? "#f4f4f1" : "#090a0d");
    document.querySelectorAll("[data-theme-option]").forEach((button) => {
        button.setAttribute("aria-pressed", button.dataset.themeOption === preference ? "true" : "false");
    });
    const label = preference === "system" ? tr("System appearance") : preference === "light" ? tr("Light appearance") : tr("Dark appearance");
    document.querySelectorAll(".theme-trigger").forEach((trigger) => {
        trigger.title = label;
        trigger.setAttribute("aria-label", runtimeLanguage === "zh-CN" ? label + "。" + tr("Choose appearance") : label + ". " + tr("Choose appearance"));
    });
    if (persist)
        persistAppearancePreference(preference);
}
function parseWorkspaceViewPreference(value) {
    return workspaceViewPreference(value);
}
function readStoredWorkspaceView() {
    return loadWorkspaceViewPreference();
}
function renderHome() {
    renderWorkspaceHome(el("runtime-home-content"), {
        language: runtimeLanguage, projects: homeProjectRows, project: selectedProjectRow(),
        sessions: recentSessionRows, sessionsAvailable: Boolean(runtimeOverviewSnapshot),
        sessionsStatus: runtimeOverviewSnapshot ? "" : tr("Activity unavailable. Refresh to try again."),
        windows: projectWindowRows, windowAvailability: projectWindowAvailability, windowScope: windowVisibilityScope,
        windowStatus: "", overview: runtimeOverviewSnapshot,
        onProject: productServices.onProject, onSession: productServices.onSession,
        onWindow: openWindowInspector, onWindows: () => applyWorkspaceView("windows"),
        onSearch: () => applyWorkspaceView("projects"), onAddProject: () => productWorkspace.addProject(),
        git: (project, target) => productWorkspace.attachGit(project, target),
    });
    if (workspaceView === "projects")
        productWorkspace.renderProjects();
    if (workspaceView === "activity")
        productWorkspace.renderActivity();
    if (workspaceView === "extensions")
        productExtensions.open();
}
function renderWorkspaceHeading() {
    if (workspaceView === "operations") {
        setText("runtime-breadcrumb-runner", tr("Runtime workspace"));
        setText("runtime-breadcrumb-project", tr("Local control plane"));
        setText("runtime-session-title", tr("Runtime & Agents"));
        return;
    }
    if (workspaceView === "windows") {
        setText("runtime-breadcrumb-runner", tr("Runtime workspace"));
        setText("runtime-breadcrumb-project", tr("Window Activity"));
        setText("runtime-session-title", selectedWindowKey ? "Window " + runtimeWindowShortKey(selectedWindowKey) : tr("Window activity"));
        return;
    }
    renderWorkspaceBreadcrumb();
    const productHeading = { home: "Home", projects: "Projects", activity: "Activity", extensions: "Extensions" }[workspaceView];
    if (productHeading) {
        setText("runtime-session-title", tr(productHeading));
        return;
    }
    const snapshot = state.workflow?.snapshot;
    setText("runtime-session-title", snapshot?.title ? String(snapshot.title) : tr("Select a Session"));
}
function applyWorkspaceView(view, persist = true) {
    workspaceView = parseWorkspaceViewPreference(view);
    const operations = workspaceView === "operations";
    const windows = workspaceView === "windows";
    const sessions = workspaceView === "sessions";
    const shell = el("runtime-console");
    if (shell)
        shell.dataset.workspaceView = workspaceView;
    document.body.classList.toggle("runtime-operations-view", operations);
    document.body.classList.toggle("runtime-windows-view", windows);
    show("runtime-navigation-sessions", sessions);
    for (const view of ["projects", "activity", "extensions"])
        show(`runtime-${view}-stage`, workspaceView === view);
    show("runtime-home-stage", workspaceView === "home");
    renderHome();
    show("runtime-navigation-operations", operations);
    show("runtime-navigation-windows", windows);
    show("runtime-conversation-stage", sessions);
    show("runtime-operations-stage", operations);
    show("runtime-windows-stage", windows);
    document.querySelectorAll("[data-runtime-view]").forEach((button) => {
        const selected = button.dataset.runtimeView === workspaceView;
        button.classList.toggle("selected", selected);
        if (selected)
            button.setAttribute("aria-current", "page");
        else
            button.removeAttribute("aria-current");
    });
    if (operations && token)
        void refreshCommunication(true);
    if (windows && token) {
        void refreshWindows(true);
        startWindowAuto();
    }
    else {
        stopWindowAuto();
    }
    renderWorkspaceHeading();
    syncResponsiveNavigation();
    setMobileNavigationOpen(false, false);
    if (persist)
        persistWorkspaceViewPreference(workspaceView);
}
function revealOperationsSection(targetId) {
    applyWorkspaceView("operations");
    const buttons = document.querySelectorAll("[data-operations-target]");
    if (!Array.from(buttons).some((button) => button.dataset.operationsTarget === targetId))
        return;
    buttons.forEach((button) => {
        const selected = button.dataset.operationsTarget === targetId;
        button.classList.toggle("selected", selected);
        if (selected)
            button.setAttribute("aria-current", "page");
        else
            button.removeAttribute("aria-current");
        show(String(button.dataset.operationsTarget), selected);
    });
    const scroll = document.querySelector(".operations-scroll");
    if (scroll)
        scroll.scrollTop = 0;
    el(targetId)?.focus({ preventScroll: true });
}
function setRuntimeConnectionState(connection) {
    const label = connection === "connected"
        ? tr("Connected")
        : connection === "connecting"
            ? tr("Reconnecting")
            : connection === "stale"
                ? tr("STALE")
                : tr("offline");
    document.querySelectorAll(".sidebar-runtime-status, .sidebar-connection, .operations-live").forEach((node) => {
        node.dataset.connection = connection;
    });
    const connectionLabel = document.querySelector(".sidebar-connection > span");
    if (connectionLabel)
        connectionLabel.textContent = label;
    setText("runtime-navigation-health", label);
}
function closeAppearanceMenus(restoreFocus = false, except = null) {
    let closed = false;
    document.querySelectorAll("details.theme-menu[open]").forEach((menu) => {
        if (menu === except)
            return;
        menu.open = false;
        closed = true;
        if (restoreFocus)
            menu.querySelector("summary")?.focus();
    });
    return closed;
}
function closeTopbarMore(restoreFocus = false) {
    const menu = el("runtime-topbar-more");
    if (!menu?.open)
        return false;
    menu.open = false;
    if (restoreFocus)
        menu.querySelector(":scope > summary")?.focus();
    return true;
}
function closeComposerOptions(restoreFocus = false) {
    const options = el("runtime-message-options");
    if (!options?.open)
        return false;
    options.open = false;
    if (restoreFocus)
        options.querySelector("summary")?.focus();
    return true;
}
function mobileNavigationViewport() {
    return window.matchMedia(MOBILE_NAVIGATION_MEDIA).matches;
}
function isContextDocked() {
    return !!el("runtime-console")?.classList.contains("context-docked");
}
function syncContextUi(restoreFocus = false) {
    const shell = el("runtime-console");
    const inspector = document.querySelector(".runtime-inspector");
    const trigger = inspector?.querySelector(".context-trigger");
    const wasDocked = !!shell?.classList.contains("context-docked");
    const isTriggerFocused = document.activeElement === trigger;
    const resolved = resolveRuntimeContextState({
        userIntent: contextUserIntent,
        isWideViewport: window.matchMedia(WIDE_CONTEXT_MEDIA).matches,
        isMobileViewport: mobileNavigationViewport(),
        hasSelectedSession: !!state.workflow?.selectedSessionId,
        workspaceView,
    });
    shell?.classList.toggle("context-docked", resolved.isDocked);
    if (inspector && inspector.open !== resolved.visible) {
        inspector.open = resolved.visible;
    }
    const focusTarget = resolveRuntimeContextFocusTransition({
        wasDocked,
        nextDocked: resolved.isDocked,
        isTriggerFocused,
    });
    if (focusTarget === "inspector_close") {
        el("runtime-inspector-close")?.focus();
    }
    else if (restoreFocus && !resolved.visible && trigger) {
        trigger.focus();
    }
}
function closeRuntimeInspector(restoreFocus = false, forceDocked = false) {
    if (isContextDocked() && !forceDocked)
        return false;
    const inspector = document.querySelector(".runtime-inspector");
    if (!inspector?.open && !isContextDocked())
        return false;
    contextUserIntent = reduceRuntimeContextUserIntent(contextUserIntent, { type: "explicit_close" });
    syncContextUi(restoreFocus);
    return true;
}
function setMobileNavigationOpen(open, restoreFocus = false, focusTarget = "runtime-mobile-nav-close") {
    const shell = el("runtime-console");
    const sidebar = el("runtime-sidebar");
    const toggle = el("runtime-mobile-nav-toggle");
    const mobile = mobileNavigationViewport();
    const nextOpen = mobile && open;
    shell?.classList.toggle("mobile-nav-open", nextOpen);
    toggle?.setAttribute("aria-expanded", nextOpen ? "true" : "false");
    if (sidebar) {
        if (mobile)
            sidebar.setAttribute("aria-hidden", nextOpen ? "false" : "true");
        else
            sidebar.removeAttribute("aria-hidden");
    }
    if (nextOpen) {
        closeAppearanceMenus(false);
        closeTopbarMore(false);
        closeRuntimeInspector(false);
        window.setTimeout(() => {
            if (mobileNavigationViewport() && shell?.classList.contains("mobile-nav-open"))
                el(focusTarget)?.focus();
        }, 260);
    }
    else if (restoreFocus && mobile) {
        window.setTimeout(() => toggle?.focus(), 0);
    }
}
function focusProjectNavigation() {
    applyWorkspaceView("sessions");
    if (mobileNavigationViewport())
        setMobileNavigationOpen(true, false, "runtime-project-search");
    else
        el("runtime-project-search")?.focus();
}
function syncResponsiveNavigation() {
    const shell = el("runtime-console");
    const sidebar = el("runtime-sidebar");
    const toggle = el("runtime-mobile-nav-toggle");
    syncContextUi();
    if (!mobileNavigationViewport()) {
        shell?.classList.remove("mobile-nav-open");
        sidebar?.removeAttribute("aria-hidden");
        toggle?.setAttribute("aria-expanded", "false");
        return;
    }
    const open = !!shell?.classList.contains("mobile-nav-open");
    sidebar?.setAttribute("aria-hidden", open ? "false" : "true");
    if (!open && sidebar?.contains(document.activeElement))
        toggle?.focus();
}
function visibleFocusableElements(container) {
    return Array.from(container.querySelectorAll('button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [href], [tabindex]:not([tabindex="-1"])')).filter((node) => node.offsetParent !== null);
}
function clearNode(node) {
    while (node && node.firstChild)
        node.removeChild(node.firstChild);
}
function renderFingerprint(value) {
    try {
        return JSON.stringify(value) || "";
    }
    catch {
        return "";
    }
}
function syncNewMessageIndicator() {
    const visible = collaborationPendingMessages > 0 && !collaborationFollowLatest;
    show("runtime-new-messages", visible);
    if (!visible)
        return;
    const label = runtimeLanguage === "zh-CN"
        ? String(collaborationPendingMessages) + " 条新消息"
        : String(collaborationPendingMessages) + " " + (collaborationPendingMessages === 1 ? "new message" : "new messages");
    setText("runtime-new-messages-label", label);
}
function chatIsNearLatest() {
    const scroll = el("runtime-chat-scroll");
    if (!scroll)
        return true;
    return scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight <= 96;
}
function updateCollaborationFollowFromScroll() {
    collaborationFollowLatest = chatIsNearLatest();
    if (collaborationFollowLatest)
        collaborationPendingMessages = 0;
    syncNewMessageIndicator();
}
function announceNewCollaborationMessages(count) {
    if (count <= 0)
        return;
    const label = runtimeLanguage === "zh-CN"
        ? String(count) + " 条新消息"
        : String(count) + " " + (count === 1 ? "new message" : "new messages");
    setText("runtime-message-announcer", label);
}
function readStoredCredential() {
    return loadRememberedRuntimeCredential();
}
function writeTabCredential() {
    persistRuntimeCredentialForTab(token, rememberCredentialForTab);
}
function eraseStoredCredential() {
    clearRememberedRuntimeCredential();
}
function saveCurrentDraft() {
    const body = el("runtime-message-body");
    if (!body)
        return;
    saveDraft(state.selectedProject, state.workflow?.selectedSessionId, body.value);
}
function restoreCurrentDraft() {
    const body = el("runtime-message-body");
    if (!body)
        return;
    body.value = loadDraft(state.selectedProject, state.workflow?.selectedSessionId);
    syncCollaborationComposerLayout();
}
function clearCurrentDraft() {
    clearDraft(state.selectedProject, state.workflow?.selectedSessionId);
}
function eraseAllDrafts() {
    clearRuntimeDrafts();
}
function rememberLocalCollaborationMessage(messageId) {
    const id = typeof messageId === "string" ? messageId : "";
    if (!/^wc_msg_[A-Za-z0-9_]+$/.test(id))
        return;
    locallyAuthoredCollaborationMessageIds.add(id);
}
function readDeviceDisclosure(clientId) {
    return storedDeviceDisclosure(clientId);
}
function writeDeviceDisclosure(clientId, open) {
    persistDeviceDisclosure(clientId, open);
}
function revealRunner(clientId) {
    if (!clientId)
        return;
    writeDeviceDisclosure(clientId, true);
    const group = document.querySelector(`.device-group[data-runner-id="${CSS.escape(clientId)}"]`);
    if (group)
        group.open = true;
}
function appendChip(parent, text, extraClass = "") {
    const chip = document.createElement("span");
    chip.className = "chip" + (extraClass ? " " + extraClass : "");
    chip.textContent = text;
    parent.appendChild(chip);
    return chip;
}
function abort(controller) {
    abortController(controller);
}
function abortCollaboration() {
    abort(collaborationAbort);
    collaborationAbort = null;
}
function abortProjectWork() {
    abort(sessionsAbort);
    abort(detailAbort);
    abortCollaboration();
    abort(projectWindowsAbort);
    sessionsAbort = null;
    detailAbort = null;
    projectWindowsAbort = null;
}
function stopProjectSearchTimer() {
    if (projectSearchTimer)
        window.clearTimeout(projectSearchTimer);
    projectSearchTimer = 0;
}
function abortAll() {
    abort(overviewAbort);
    abort(projectsAbort);
    overviewAbort = null;
    projectsAbort = null;
    stopProjectSearchTimer();
    abortProjectWork();
    abort(windowsAbort);
    abort(windowDetailAbort);
    windowsAbort = null;
    windowDetailAbort = null;
}
async function api(path, payload, signal) {
    apiClient.setToken(token);
    return apiClient.post(path, payload, signal);
}
async function copyRuntimeValue(value, statusId) {
    if (!value)
        return;
    const ok = await writeClipboardText(value);
    if (statusId)
        setText(statusId, ok ? tr("Copied") : tr("Unable to copy"));
}
function openWindowLinkedSession(session) {
    const project = String(session?.project || "");
    const sessionId = String(session?.workflow_session_id || "");
    const clientId = runtimeProjectClientId(project);
    if (!project || !sessionId || !clientId)
        return;
    applyWorkspaceView("sessions");
    selectRecentSession({ client_id: clientId, project_id: project, session_id: sessionId });
}
function renderWindowActivities(node, activities, compact = false) {
    renderWindowActivityRows(node, activities, {
        compact,
        language: runtimeLanguage,
        onCopyTrace: (traceId) => void copyRuntimeValue(traceId),
    });
}
function renderWindowList() {
    const node = el("runtime-window-list");
    setText("runtime-window-list-count", String(windowRows.length));
    const emptyCopy = formatWindowEmptyState(windowAvailability, windowVisibilityScope, false, runtimeLanguage);
    setText("runtime-window-list-empty", emptyCopy);
    show("runtime-window-list-empty", windowRows.length === 0);
    setText("runtime-window-list-status", formatWindowListStatusText(windowAvailability, windowRows.length, windowVisibilityScope, runtimeLanguage));
    renderWindowCards(node, windowRows.map(row => ({ ...row, last_project_name: productName(homeProjectRows.find(project => project.id === row.last_project) || { id: row.last_project }) })), selectedWindowKey, (key) => void selectWindow(key), Date.now(), runtimeLanguage);
}
function renderWindowDetail(detail) {
    selectedWindowDetail = detail;
    const present = !!detail;
    show("runtime-window-detail-empty", !present);
    show("runtime-window-detail", present);
    if (!detail)
        return;
    const fields = formatWindowDetailFields(detail, selectedWindowKey, Date.now(), runtimeLanguage);
    if (!fields)
        return;
    setText("runtime-window-title", fields.title);
    setText("runtime-window-key", fields.key);
    setText("runtime-window-source", fields.source);
    setText("runtime-window-active-count", fields.activeCount);
    setText("runtime-window-last-call", fields.lastCall);
    setText("runtime-window-last-meaningful", fields.lastMeaningful);
    setText("runtime-window-active-status", fields.activeStatus);
    setText("runtime-window-linked-status", fields.linkedStatus);
    setText("runtime-window-activity-status", fields.activityStatus);
    renderWindowActiveRequests(el("runtime-window-active-requests"), Array.isArray(detail.active_requests) ? detail.active_requests : [], {
        language: runtimeLanguage,
        onCopyTrace: (traceId) => void copyRuntimeValue(traceId),
    });
    renderWindowLinkedSessions(el("runtime-window-linked-sessions"), Array.isArray(detail.linked_sessions) ? detail.linked_sessions : [], (session) => openWindowLinkedSession(session), runtimeLanguage);
    renderWindowActivities(el("runtime-window-activity"), Array.isArray(detail.activity) ? detail.activity : []);
    const observed = [...(detail.active_requests || []).map((row) => ({ ...row, at: row.started_at_ms })), ...(detail.activity || []).filter((row) => row.meaningful).map((row) => ({ ...row, at: row.ended_at_ms }))].sort((a, b) => Number(b.at) - Number(a.at));
    const project = observed.find((row) => row.project)?.project;
    setText("runtime-window-project", productName(homeProjectRows.find(row => row.id === project) || { id: project }));
    setText("runtime-window-product-state", tr(Number(detail.active_count) > 0 ? "In progress" : "Observed"));
    const timeline = el("runtime-window-product-activity");
    timeline?.replaceChildren();
    for (const entry of observed.slice(0, 20)) {
        const row = productNode("article");
        const text = productNode("div");
        text.appendChild(productNode("strong", productActivity(entry, runtimeLanguage)));
        text.appendChild(productNode("span", productName(homeProjectRows.find(project => project.id === entry.project) || { id: entry.project }), "muted small"));
        row.append(text, productNode("time", productTime(entry.at, runtimeLanguage)));
        timeline?.appendChild(row);
    }
    if (!observed.length)
        timeline?.appendChild(productNode("p", tr("No activity observed yet"), "muted"));
    renderWorkspaceHeading();
}
async function refreshWindowDetail() {
    if (!token || !selectedWindowKey)
        return renderWindowDetail(null);
    abort(windowDetailAbort);
    const controller = new AbortController();
    windowDetailAbort = controller;
    const key = selectedWindowKey;
    const response = await api("window", { client_window_key: key, activity_limit: 100, session_limit: 50 }, controller.signal);
    if (windowDetailAbort === controller)
        windowDetailAbort = null;
    if (!response || key !== selectedWindowKey)
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    if (response.status === 404) {
        selectedWindowKey = "";
        renderWindowList();
        renderWindowDetail(null);
        return;
    }
    if (!response.ok || !response.data) {
        setText("runtime-window-list-status", response.status === 403 ? "runtime:read required" : "Could not refresh Window detail.");
        return;
    }
    renderWindowDetail(response.data);
}
async function selectWindow(key) {
    if (!/^[0-9a-fA-F]{64}$/.test(key))
        return;
    if (key !== selectedWindowKey)
        renderWindowDetail(null);
    selectedWindowKey = key;
    renderWindowList();
    renderWorkspaceHeading();
    setMobileNavigationOpen(false, true);
    await refreshWindowDetail();
}
async function refreshWindows(refreshSelected = true) {
    if (!token || workspaceView !== "windows")
        return;
    abort(windowsAbort);
    const controller = new AbortController();
    windowsAbort = controller;
    if (windowAvailability === "idle") {
        windowAvailability = "loading";
        renderWindowList();
    }
    const response = await api("windows", { limit: 100 }, controller.signal);
    // A superseded or navigation-cancelled request must not overwrite the newer Window state.
    if (windowsAbort !== controller)
        return;
    windowsAbort = null;
    // RuntimeApiClient returns null only for AbortError. Cancellation is not a refresh failure.
    if (!response)
        return;
    if (response.status === 401)
        return lock("Credential rejected.");
    const nextAvailability = runtimeWindowAvailabilityAfterHttpResponse(response.status, response.ok, !!response.data);
    if (nextAvailability === "unavailable") {
        windowRows = [];
        selectedWindowKey = "";
        windowAvailability = nextAvailability;
        renderWindowList();
        renderWindowDetail(null);
        return;
    }
    if (nextAvailability === "stale") {
        windowAvailability = nextAvailability;
        renderWindowList();
        return;
    }
    windowAvailability = nextAvailability;
    if (response.data.visibility?.scope === "global" || response.data.visibility?.scope === "principal") {
        windowVisibilityScope = response.data.visibility.scope;
    }
    windowRows = Array.isArray(response.data.windows) ? response.data.windows : [];
    if (!selectedWindowKey && windowRows.length)
        selectedWindowKey = String(windowRows[0]?.client_window_key || "");
    renderWindowList();
    if (refreshSelected && selectedWindowKey) {
        await refreshWindowDetail();
    }
    else if (!selectedWindowKey) {
        renderWindowDetail(null);
    }
}
function openWindowInspector(key) {
    selectedWindowKey = key;
    applyWorkspaceView("windows");
    renderWindowList();
    void refreshWindowDetail();
}
function renderSessionWindowCorrelation(detail) {
    const available = detail?.window_activity_available === true;
    show("runtime-linked-windows-unavailable", !available);
    const linkedNode = el("runtime-linked-windows");
    clearNode(linkedNode);
    const links = available && Array.isArray(detail?.linked_windows) ? detail.linked_windows : [];
    setText("runtime-linked-windows-status", available ? runtimeCountLabel(links.length, "Window") : tr("runtime:read unavailable"));
    if (available) {
        renderSessionWindowCorrelationLinks(linkedNode, links, (key) => openWindowInspector(key), Date.now(), runtimeLanguage);
    }
    const gaps = available && Array.isArray(detail?.window_activity_after_last_session_record)
        ? detail.window_activity_after_last_session_record
        : [];
    show("runtime-recorder-gap-panel", gaps.length > 0);
    renderWindowActivities(el("runtime-recorder-gap-activity"), gaps, true);
}
function projectWindowActiveCount() {
    if (projectWindowAvailability !== "available")
        return 0;
    return projectWindowRows.reduce((sum, w) => sum + Math.max(0, Number(w?.active_count || 0)), 0);
}
function clearProjectWindows() {
    projectWindowRows = [];
    projectWindowAvailability = "idle";
    projectWindowTruncated = false;
    projectWindowTotal = 0;
    projectWindowProjectId = "";
    renderedProjectWindowSignature = "";
    renderProjectWindows();
}
function renderProjectWindows() {
    renderHome();
    const list = el("runtime-project-windows-list");
    if (!list)
        return;
    if (projectWindowAvailability === "unavailable") {
        clearNode(list);
        show("runtime-project-windows-empty", false);
        show("runtime-project-windows-unavailable", true);
        setText("runtime-project-windows-count", "—");
        setText("runtime-project-windows-status", tr("runtime:read required"));
        return;
    }
    show("runtime-project-windows-unavailable", false);
    const count = projectWindowRows.length;
    setText("runtime-project-windows-count", String(count));
    setText("runtime-project-windows-status", projectWindowAvailability === "stale"
        ? tr(count > 0 ? "Refresh failed · showing previous data" : "refresh unavailable")
        : formatProjectWindowStatusText(count, projectWindowTotal, projectWindowTruncated, runtimeLanguage));
    const projectEmptyCopy = formatWindowEmptyState(projectWindowAvailability, windowVisibilityScope, true, runtimeLanguage);
    setText("runtime-project-windows-empty", projectEmptyCopy);
    show("runtime-project-windows-empty", count === 0 && projectWindowAvailability === "available");
    const signature = renderFingerprint([
        runtimeLanguage,
        projectWindowProjectId,
        projectWindowRows,
        projectWindowTruncated,
        projectWindowTotal,
        projectWindowAvailability,
    ]);
    if (signature === renderedProjectWindowSignature)
        return;
    renderedProjectWindowSignature = signature;
    renderProjectWindowCards(list, projectWindowRows, (key) => openWindowInspector(key), Date.now(), runtimeLanguage);
}
async function fetchProjectWindows(request) {
    abort(projectWindowsAbort);
    const controller = new AbortController();
    projectWindowsAbort = controller;
    const response = await api("windows", { project: request.project, limit: PROJECT_WINDOW_LIMIT }, controller.signal);
    if (projectWindowsAbort === controller)
        projectWindowsAbort = null;
    if (!isCurrentRuntimeProjectWindowsRequest(state, request))
        return false;
    if (!response) {
        projectWindowAvailability = "stale";
        projectWindowProjectId = request.project;
        renderProjectWindows();
        renderProjectSelectors(projectRows, projectRowsTruncated);
        return false;
    }
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        projectWindowRows = [];
        projectWindowAvailability = "unavailable";
        projectWindowProjectId = request.project;
        projectWindowTruncated = false;
        projectWindowTotal = 0;
        renderProjectWindows();
        renderProjectSelectors(projectRows, projectRowsTruncated);
        return false;
    }
    if (!response.ok || !response.data) {
        projectWindowAvailability = "stale";
        projectWindowProjectId = request.project;
        renderProjectWindows();
        renderProjectSelectors(projectRows, projectRowsTruncated);
        return false;
    }
    projectWindowRows = Array.isArray(response.data.windows) ? response.data.windows : [];
    projectWindowAvailability = "available";
    if (response.data.visibility?.scope === "global" || response.data.visibility?.scope === "principal") {
        windowVisibilityScope = response.data.visibility.scope;
    }
    projectWindowProjectId = request.project;
    projectWindowTruncated = !!response.data.truncated;
    projectWindowTotal = typeof response.data.total === "number" ? response.data.total : projectWindowRows.length;
    renderProjectWindows();
    renderProjectSelectors(projectRows, projectRowsTruncated);
    return true;
}
function hideDetail() {
    const messageSearch = el("runtime-message-search");
    if (messageSearch)
        messageSearch.value = "";
    setText("runtime-message-search-status", "");
    document.body.classList.remove("runtime-has-session");
    renderWorkspaceHeading();
    show("runtime-session-detail", false);
    show("runtime-session-context", false);
    show("runtime-session-detail-empty", true);
    renderHome();
    show("runtime-jump-latest", false);
    setText("runtime-session-workspace", "");
    clearNode(el("runtime-collaboration-board"));
    renderedCollaborationMessageIds = new Set();
    renderedCollaborationSignature = "";
    collaborationFollowLatest = true;
    collaborationPendingMessages = 0;
    syncNewMessageIndicator();
    syncResponsiveNavigation();
}
function clearSessionSurface() {
    saveCurrentDraft();
    sessionRows = [];
    sessionAvailability = "idle";
    sessionListMetaSnapshot = { total: 0, truncated: false };
    const sessionSearch = el("runtime-session-search");
    if (sessionSearch)
        sessionSearch.value = "";
    renderedSessionListSignature = "";
    renderedCollaborationSignature = "";
    clearNode(el("runtime-session-list"));
    show("runtime-sessions-empty", false);
    clearRuntimeWorkflowSession(state);
    locallyAuthoredCollaborationMessageIds = new Set();
    abortCollaboration();
    hideDetail();
    resetCollaborationComposerUi();
    clearProjectWindows();
}
function lock(message = "", clearRemembered = true) {
    productWorkspace.reset();
    productExtensions.reset();
    productProjectApi.clearToken();
    el("runtime-command-dialog")?.close();
    el("runtime-command-results")?.replaceChildren();
    setMobileNavigationOpen(false, false);
    closeRuntimeInspector(false, true);
    contextUserIntent = null;
    detachCommunicationEndpointsBestEffort();
    token = "";
    if (clearRemembered) {
        eraseStoredCredential();
        eraseAllDrafts();
    }
    abortAll();
    invalidateRuntimeCredential(state);
    projectRows = [];
    homeProjectRows = [];
    runnerRows = [];
    recentSessionRows = [];
    runtimeOverviewSnapshot = null;
    recentSessionMetaSnapshot = null;
    projectRowsTotal = 0;
    projectRowsTruncated = false;
    knownProjectDevices = [];
    selectedProjectSnapshot = null;
    projectSearch = "";
    projectDeviceFilter = "";
    collaborationReplyTo = "";
    collaborationFollowLatest = true;
    collaborationPendingMessages = 0;
    clearSessionSurface();
    windowRows = [];
    windowAvailability = "idle";
    windowVisibilityScope = "principal";
    selectedWindowKey = "";
    selectedWindowDetail = null;
    stopWindowAuto();
    renderWindowList();
    renderWindowDetail(null);
    resetCommunicationSurface();
    const projectList = el("runtime-project-list");
    const windowPanel = el("runtime-project-window-activity-panel");
    const sessionsPanel = el("runtime-workflow-sessions-panel");
    renderedProjectSelectorsSignature = "";
    renderedRunnerFleetSignature = "";
    renderedRecentSessionsSignature = "";
    windowPanel?.remove();
    sessionsPanel?.remove();
    clearNode(projectList);
    if (projectList && sessionsPanel) {
        sessionsPanel.hidden = true;
        projectList.appendChild(sessionsPanel);
        if (windowPanel) {
            windowPanel.hidden = true;
            projectList.appendChild(windowPanel);
        }
    }
    clearNode(el("runtime-recent-session-list"));
    clearNode(el("runtime-runner-list"));
    document.body.classList.remove("runtime-connected");
    const messageSearch = el("runtime-message-search");
    if (messageSearch)
        messageSearch.value = "";
    setText("runtime-message-search-status", "");
    document.body.classList.remove("runtime-has-session");
    show("runtime-token-gate", true);
    show("runtime-console", false);
    show("runtime-topbar-controls", false);
    stopAuto();
    setRuntimeConnectionState("disconnected");
    setText("runtime-token-error", message ? tr(message) : "");
    setText("runtime-refresh-status", "");
    const input = el("runtime-token-input");
    if (input) {
        input.value = "";
        input.focus();
    }
    const search = el("runtime-project-search");
    if (search)
        search.value = "";
}
function unlockUi() {
    writeTabCredential();
    document.body.classList.add("runtime-connected");
    show("runtime-token-gate", false);
    show("runtime-console", true);
    show("runtime-topbar-controls", true);
    setText("runtime-token-error", "");
    setRuntimeConnectionState("connected");
    applyWorkspaceView(workspaceView, false);
    syncResponsiveNavigation();
    startAuto();
}
function showError(message) {
    setText("runtime-error", message ? tr(message) : "");
    show("runtime-error", !!message);
}
function runtimeCountLabel(value, singular, plural = singular + "s") {
    return localizedCountLabel(value, singular, plural, runtimeLanguage);
}
function renderRuntimeOverviewMetrics(data) {
    const metrics = formatRuntimeOverviewMetrics(data, runtimeLanguage);
    if (!metrics)
        return;
    setText("runtime-server-identity", metrics.identity);
    setText("runtime-server-build", metrics.build);
    setText("runtime-server-runners", metrics.runners);
    setText("runtime-server-alignment", metrics.alignment);
    setText("runtime-server-projects", metrics.projects);
    setText("runtime-server-jobs", metrics.jobs);
    setText("runtime-server-attention", metrics.attention);
    setText("runtime-server-sessions", metrics.sessions);
    setText("runtime-recent-status", metrics.recentStatus);
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
        homeProjectRows = [];
        runnerRows = [];
        recentSessionRows = [];
        runtimeOverviewSnapshot = null;
        recentSessionMetaSnapshot = null;
        show("runtime-overview-unavailable", true);
        show("runtime-runner-unavailable", true);
        show("runtime-recent-unavailable", true);
        setText("runtime-overview-access", tr("runtime:read unavailable"));
        setText("runtime-runner-access", tr("runtime:read unavailable"));
        setText("runtime-recent-status", tr("runtime:read unavailable"));
        renderRunnerFleet([]);
        renderRecentSessions([], null);
        renderProjectSelectors(projectRows, projectRowsTruncated);
        setRuntimeConnectionState("connected");
        return true;
    }
    if (!response.ok || !response.data) {
        setText("runtime-overview-access", tr("refresh unavailable"));
        setText("runtime-runner-access", tr("refresh unavailable"));
        setText("runtime-recent-status", tr("refresh unavailable"));
        setRuntimeConnectionState("stale");
        return false;
    }
    show("runtime-overview-unavailable", false);
    show("runtime-runner-unavailable", false);
    show("runtime-recent-unavailable", false);
    setText("runtime-overview-access", "runtime:read");
    setText("runtime-runner-access", "runtime:read");
    const data = response.data;
    runtimeOverviewSnapshot = data;
    homeProjectRows = Array.isArray(data.projects) ? data.projects : [];
    runnerRows = Array.isArray(data.runners) ? data.runners : [];
    recentSessionRows = Array.isArray(data.recent_sessions?.sessions) ? data.recent_sessions.sessions : [];
    const recentMeta = data.recent_sessions || {};
    recentSessionMetaSnapshot = recentMeta;
    renderRuntimeOverviewMetrics(data);
    renderRecentSessions(recentSessionRows, recentMeta);
    renderRunnerFleet(runnerRows);
    renderProjectSelectors(projectRows, projectRowsTruncated);
    setRuntimeConnectionState("connected");
    return true;
}
function projectLabel(project) {
    return formatProjectLabel(project);
}
async function fetchProjects(request, unlocking = false) {
    const priorSelectedProject = selectedProjectRow();
    abort(projectsAbort);
    const controller = new AbortController();
    projectsAbort = controller;
    const payload = { limit: 100 };
    const clientId = String(request?.clientId || "");
    const query = String(request?.query || "").trim();
    if (clientId)
        payload.client_id = clientId;
    if (query)
        payload.query = query;
    const response = await api("projects", payload, controller.signal);
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
        else {
            showError("Could not refresh projects.");
            setRuntimeConnectionState("stale");
        }
        return false;
    }
    projectRows = Array.isArray(response.data.projects) ? response.data.projects : [];
    const reportedTotal = typeof response.data.total === "number" && Number.isFinite(response.data.total)
        ? Math.max(0, Math.floor(response.data.total))
        : projectRows.length;
    projectRowsTotal = Math.max(projectRows.length, reportedTotal);
    projectRowsTruncated = !!response.data.truncated;
    const known = new Set(knownProjectDevices);
    for (const device of runtimeDeviceIds(projectRows))
        known.add(device);
    knownProjectDevices = Array.from(known).sort((left, right) => left.localeCompare(right));
    if (priorSelectedProject && String(priorSelectedProject.id || "") === String(state.selectedProject || "")) {
        selectedProjectSnapshot = priorSelectedProject;
    }
    const refreshedSelected = effectiveProjects(projectRows).find((project) => String(project?.id || "") === String(state.selectedProject || ""));
    if (refreshedSelected)
        selectedProjectSnapshot = refreshedSelected;
    unlockUi();
    setRuntimeConnectionState("connected");
    showError("");
    const currentDevice = String(state.selectedDevice || "");
    const currentProject = String(state.selectedProject || "");
    if (query) {
        renderProjectSelectors(projectRows, projectRowsTruncated);
        renderSelectedProjectIdentity();
        return true;
    }
    const selection = preferredRuntimeProjectSelection(projectRows, currentDevice, currentProject);
    if (!selection.project) {
        if (currentProject && projectRowsTruncated) {
            renderProjectSelectors(projectRows, projectRowsTruncated);
            renderSelectedProjectIdentity();
            return true;
        }
        if (currentProject || selection.device !== currentDevice) {
            abortProjectWork();
            selectRuntimeRunnerFilter(state, selection.device || "");
            selectedProjectSnapshot = null;
            collaborationReplyTo = "";
            clearSessionSurface();
        }
        renderProjectSelectors(projectRows, projectRowsTruncated);
        renderSelectedProjectIdentity();
        return true;
    }
    if (selection.device !== currentDevice || selection.project !== currentProject) {
        switchProject(selection.device, selection.project);
    }
    else {
        renderProjectSelectors(projectRows, projectRowsTruncated);
        const listRequest = refreshRuntimeSessionList(state);
        if (listRequest)
            void fetchSessions(listRequest);
        const windowRequest = refreshRuntimeProjectWindows(state);
        if (windowRequest)
            void fetchProjectWindows(windowRequest);
    }
    return true;
}
function effectiveProjects(projects) {
    return mergeEffectiveProjects(projects, homeProjectRows);
}
function projectSelectorDevices(projects) {
    return extractProjectSelectorDevices(projects, knownProjectDevices, runnerRows, String(state.selectedDevice || ""));
}
function selectedProjectRow() {
    const selected = String(state.selectedProject || "");
    if (!selected)
        return null;
    const current = effectiveProjects(projectRows).find((project) => String(project?.id || "") === selected);
    if (current)
        return current;
    return selectedProjectSnapshot && String(selectedProjectSnapshot.id || "") === selected
        ? selectedProjectSnapshot
        : null;
}
function renderWorkspaceBreadcrumb() {
    const project = selectedProjectRow();
    const { runnerText, projectText } = formatWorkspaceBreadcrumb(project, runtimeLanguage);
    setText("runtime-breadcrumb-runner", runnerText);
    setText("runtime-breadcrumb-project", projectText);
}
function renderSelectedProjectIdentity() {
    const project = selectedProjectRow();
    renderWorkspaceBreadcrumb();
    setText("runtime-selected-project", formatSelectedProjectIdentity(project, runtimeLanguage));
}
function renderSessionWorkspaceIdentity() {
    const project = selectedProjectRow();
    setText("runtime-session-workspace", formatSessionWorkspaceIdentity(project, runtimeLanguage));
}
function revealWorkflowSessionDetail() {
    const panel = el("runtime-workflow-sessions-panel");
    const workspace = panel?.closest("details.workspace-group");
    if (workspace)
        workspace.open = true;
    panel?.scrollIntoView({ block: "start", inline: "nearest" });
}
function renderProjectSelectors(projects, truncated) {
    renderHome();
    const deviceSelect = el("runtime-device-select");
    const projectList = el("runtime-project-list");
    if (!deviceSelect || !projectList)
        return;
    const effective = effectiveProjects(projects);
    const activeWindowCount = projectWindowActiveCount();
    const signature = renderFingerprint({
        language: runtimeLanguage,
        selectedDevice: state.selectedDevice,
        selectedProject: state.selectedProject,
        projectDeviceFilter,
        projectSearch,
        truncated,
        projectRowsTotal,
        knownProjectDevices,
        projects: effective,
        runners: runnerRows.map((runner) => [runner?.client_id, runner?.connected, runner?.status]),
        activeWindowCount,
        selectedProjectWindowCount: projectWindowRows.length,
    });
    if (signature === renderedProjectSelectorsSignature)
        return;
    renderedProjectSelectorsSignature = signature;
    const windowPanel = el("runtime-project-window-activity-panel");
    const sessionsPanel = el("runtime-workflow-sessions-panel");
    windowPanel?.remove();
    sessionsPanel?.remove();
    const devices = projectSelectorDevices(projects);
    renderProjectSelectorTree(deviceSelect, projectList, sessionsPanel, {
        effectiveProjects: effective,
        devices,
        runnerRows,
        selectedDevice: state.selectedDevice,
        selectedProject: state.selectedProject,
        projectDeviceFilter,
        language: runtimeLanguage,
        storedDeviceDisclosure: readDeviceDisclosure,
        onPersistDeviceDisclosure: writeDeviceDisclosure,
        onSelectProject: (clientId, projectId) => switchProject(clientId, projectId),
        windowPanel,
        selectedProjectWindowActiveCount: activeWindowCount,
        selectedProjectWindowCount: projectWindowRows.length,
    });
    const returnedProjects = runtimeProjectsForDevice(effective, projectDeviceFilter).length;
    const totalProjects = Math.max(returnedProjects, projectRowsTotal);
    show("runtime-projects-empty", returnedProjects === 0);
    setText("runtime-device-status", formatDeviceStatusText(devices.length, projectDeviceFilter, runtimeLanguage));
    setText("runtime-project-status", formatProjectStatusText(returnedProjects, totalProjects, truncated, projectDeviceFilter, projectSearch, runtimeLanguage));
    renderSelectedProjectIdentity();
}
function switchProject(device, project) {
    const snapshot = effectiveProjects(projectRows).find((row) => String(row?.id || "") === project);
    selectedProjectSnapshot = snapshot || null;
    abortProjectWork();
    collaborationReplyTo = "";
    clearSessionSurface();
    if (device)
        revealRunner(device);
    const request = selectRuntimeProject(state, device, project);
    applyWorkspaceView("home");
    const windowRequest = refreshRuntimeProjectWindows(state);
    renderProjectSelectors(projectRows, projectRowsTruncated);
    renderRunnerFleet(runnerRows);
    renderRecentSessions(recentSessionRows, null);
    renderSelectedProjectIdentity();
    if (request)
        void fetchSessions(request);
    if (windowRequest)
        void fetchProjectWindows(windowRequest);
    if (token)
        void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
}
function applyRunnerFilter(device) {
    stopProjectSearchTimer();
    abortProjectWork();
    collaborationReplyTo = "";
    clearSessionSurface();
    selectedProjectSnapshot = null;
    projectDeviceFilter = device;
    if (device)
        revealRunner(device);
    selectRuntimeRunnerFilter(state, device);
    renderProjectSelectors(projectRows, projectRowsTruncated);
    renderRunnerFleet(runnerRows);
    renderRecentSessions(recentSessionRows, null);
    renderSelectedProjectIdentity();
    if (token)
        void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
}
function renderRunnerFleet(runners) {
    const node = el("runtime-runner-list");
    if (!node)
        return;
    const signature = renderFingerprint([runtimeLanguage, state.selectedDevice, runners]);
    if (signature === renderedRunnerFleetSignature)
        return;
    renderedRunnerFleetSignature = signature;
    show("runtime-runners-empty", runners.length === 0 && !!el("runtime-runner-unavailable")?.hidden);
    renderRunnerFleetRows(node, runners, {
        selectedDevice: state.selectedDevice,
        language: runtimeLanguage,
        onSelectRunner: (clientId) => applyRunnerFilter(clientId),
    });
    setText("runtime-runner-count", formatRunnerCountText(runners.length, runtimeLanguage));
}
function renderRecentSessions(sessions, meta) {
    renderHome();
    const node = el("runtime-recent-session-list");
    if (!node)
        return;
    const signature = renderFingerprint([
        runtimeLanguage,
        state.selectedProject,
        state.workflow.selectedSessionId,
        sessions,
        meta,
    ]);
    if (signature === renderedRecentSessionsSignature)
        return;
    renderedRecentSessionsSignature = signature;
    show("runtime-recent-empty", sessions.length === 0 && !!el("runtime-recent-unavailable")?.hidden);
    renderRecentSessionRows(node, sessions, {
        selectedProject: state.selectedProject,
        selectedSessionId: state.workflow.selectedSessionId,
        language: runtimeLanguage,
        onSelectSession: (session) => selectRecentSession(session),
    });
    if (meta) {
        setText("runtime-recent-status", formatRecentSessionStatusText(meta, runtimeLanguage));
    }
}
function selectRecentSession(session) {
    applyWorkspaceView("sessions");
    const clientId = String(session?.client_id || "");
    const projectId = String(session?.project_id || "");
    const sessionId = String(session?.session_id || "");
    if (!clientId || !projectId || !sessionId)
        return;
    if (projectDeviceFilter && projectDeviceFilter !== clientId)
        projectDeviceFilter = "";
    const knownProject = effectiveProjects(projectRows).find((row) => String(row?.id || "") === projectId)
        || homeProjectRows.find((row) => String(row?.id || "") === projectId);
    selectedProjectSnapshot = knownProject || {
        id: projectId,
        client_id: clientId,
        name: typeof session?.project_name === "string" ? session.project_name : undefined,
    };
    abortProjectWork();
    collaborationReplyTo = "";
    clearSessionSurface();
    setHumanJoinSendEnabled(false);
    if (clientId)
        revealRunner(clientId);
    const location = selectRuntimeSessionLocation(state, clientId, projectId, sessionId);
    restoreCurrentDraft();
    renderProjectSelectors(projectRows, projectRowsTruncated);
    renderRunnerFleet(runnerRows);
    renderRecentSessions(recentSessionRows, null);
    renderSelectedProjectIdentity();
    revealWorkflowSessionDetail();
    if (location.sessionListRequest)
        void fetchSessions(location.sessionListRequest);
    const windowRequest = refreshRuntimeProjectWindows(state);
    if (windowRequest)
        void fetchProjectWindows(windowRequest);
    if (location.detailRequest)
        void fetchSessionDetail(location.detailRequest);
    const collaborationRequest = runtimeCollaborationRequest(state);
    if (collaborationRequest)
        void startCollaboration(collaborationRequest);
    if (token)
        void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
    setMobileNavigationOpen(false, true);
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
        sessionAvailability = "stale";
        renderHome();
        showError("Selected project is no longer available.");
        return;
    }
    if (!response.ok || !response.data) {
        sessionAvailability = "stale";
        renderHome();
        showError("Could not refresh Workflow Sessions.");
        return;
    }
    const selected = String(state.workflow.selectedSessionId || "");
    const previousSelected = selected
        ? sessionRows.find((row) => String(row?.session_id || "") === selected)
        : null;
    sessionAvailability = "available";
    sessionRows = Array.isArray(response.data.sessions) ? response.data.sessions : [];
    sessionListMetaSnapshot = {
        total: typeof response.data.total === "number" ? Math.max(sessionRows.length, response.data.total) : sessionRows.length,
        truncated: !!response.data.truncated,
    };
    renderSessionList(sessionRows, sessionListMetaSnapshot);
    showError("");
    const nextSelected = selected
        ? sessionRows.find((row) => String(row?.session_id || "") === selected)
        : null;
    if (selected && nextSelected) {
        if (!state.workflow.snapshot || runtimeWorkflowSessionSummaryChanged(previousSelected, nextSelected)) {
            const detailRequest = refreshRuntimeWorkflowSession(state);
            if (detailRequest)
                void fetchSessionDetail(detailRequest);
        }
    }
    else if (selected) {
        abortCollaboration();
        clearRuntimeWorkflowSession(state);
        hideDetail();
    }
}
function updatedLabel(timestamp) {
    return formatUpdatedTime(timestamp, runtimeLanguage);
}
function dateTimeLabel(timestamp) {
    return formatSessionDateTime(timestamp, runtimeLanguage);
}
function localizedLivenessPresentation(session) {
    return formatLivenessPresentation(session, runtimeLanguage);
}
function localWorkflowText(value) {
    return localizedWorkflowText(value, runtimeLanguage);
}
function renderSessionList(sessions, payload) {
    const node = el("runtime-session-list");
    if (!node)
        return;
    const total = typeof payload.total === "number" ? payload.total : sessions.length;
    const selected = String(state.workflow.selectedSessionId || "");
    const query = el("runtime-session-search")?.value || "";
    const visible = sessions.filter((session) => runtimeSearchMatches(query, [session.title, session.session_id, session.lifecycle]));
    const signature = renderFingerprint([runtimeLanguage, selected, total, !!payload.truncated, sessions, query]);
    if (signature === renderedSessionListSignature)
        return;
    renderedSessionListSignature = signature;
    clearNode(node);
    show("runtime-sessions-empty", visible.length === 0);
    setText("runtime-session-search-status", query ? visible.length + " / " + sessions.length : "");
    setText("runtime-sessions-count", total ? sessions.length + (payload.truncated ? " of " + total : "") : "0");
    renderWorkspaceSessionList(node, visible, selected, runtimeLanguage, selectSession);
    renderHome();
}
function selectSession(sessionId) {
    applyWorkspaceView("sessions");
    saveCurrentDraft();
    abort(detailAbort);
    detailAbort = null;
    abortCollaboration();
    hideDetail();
    setHumanJoinSendEnabled(false);
    const request = selectRuntimeWorkflowSession(state, sessionId);
    locallyAuthoredCollaborationMessageIds = new Set();
    resetCollaborationComposerUi();
    restoreCurrentDraft();
    renderSessionList(sessionRows, sessionListMetaSnapshot);
    revealWorkflowSessionDetail();
    if (request)
        void fetchSessionDetail(request);
    const collaborationRequest = runtimeCollaborationRequest(state);
    if (collaborationRequest)
        void startCollaboration(collaborationRequest);
    setMobileNavigationOpen(false, true);
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
        resetCollaborationComposerUi();
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
function syncFollowUi() {
    show("runtime-jump-latest", !!state.workflow.selectedSessionId && !shouldFollowWorkflowSessionLatest(state.workflow));
}
function renderDetail(detail, consumeCollaborationNotice = true) {
    document.body.classList.add("runtime-has-session");
    show("runtime-session-detail-empty", false);
    show("runtime-session-detail", true);
    show("runtime-session-context", true);
    renderWorkspaceHeading();
    setText("runtime-session-lifecycle", tr(String(detail.lifecycle || "unknown")));
    setText("runtime-session-mode", (runtimeLanguage === "zh-CN" ? "模式 " : "mode ") + tr(String(detail.mode || "unknown")));
    setText("runtime-session-context-lifecycle", tr(String(detail.lifecycle || "unknown")));
    setText("runtime-session-context-mode", tr(String(detail.mode || "unknown")));
    const liveness = localizedLivenessPresentation(detail);
    setText("runtime-session-running", liveness.label);
    const livenessNode = el("runtime-session-running");
    if (livenessNode)
        livenessNode.title = liveness.tooltip;
    setText("runtime-session-id", String(detail.session_id || (runtimeLanguage === "zh-CN" ? "会话 ID 不可用" : "session id unavailable")));
    setText("runtime-session-created", dateTimeLabel(detail.created_at));
    setText("runtime-session-updated", dateTimeLabel(detail.updated_at));
    renderSessionWorkspaceIdentity();
    renderSessionWindowCorrelation(detail);
    renderWorkspaceOverview(detail.overview, runtimeLanguage);
    renderWorkspaceEvidence(el("runtime-work-evidence"), detail, runtimeLanguage);
    renderWorkspaceEvidence(el("runtime-context-evidence"), detail, runtimeLanguage);
    renderCollaboration(undefined, consumeCollaborationNotice);
    syncResponsiveNavigation();
    const activities = Array.isArray(detail.activity) ? detail.activity : [];
    const node = el("runtime-timeline");
    const previousScrollTop = node ? node.scrollTop : 0;
    show("runtime-timeline-empty", activities.length === 0);
    renderTimelineEvents(node, activities, runtimeLanguage);
    if (!node)
        return syncFollowUi();
    node.scrollTop = workflowSessionScrollTopAfterRender(state.workflow, previousScrollTop, node.clientHeight, node.scrollHeight);
    syncFollowUi();
}
function syncCollaborationComposer() {
    const edit = runtimeCollaborationEditTarget(state);
    const unavailable = state.collaboration.available === false;
    const replyTargetId = String(state.collaboration.replyTargetId || "");
    const body = el("runtime-message-body");
    const kind = el("runtime-message-kind");
    const priority = el("runtime-message-priority");
    const checkbox = el("runtime-message-requires-ack");
    const send = el("runtime-message-send");
    if (unavailable)
        closeComposerOptions(false);
    show("runtime-message-reply", !!replyTargetId && !edit);
    setText("runtime-message-reply-text", replyTargetId ? (runtimeLanguage === "zh-CN" ? "回复 " : "Reply to ") + replyTargetId : "");
    show("runtime-message-edit", !!edit);
    setText("runtime-message-edit-text", edit ? (runtimeLanguage === "zh-CN" ? "正在编辑 " : "Editing ") + String(edit.message_id) : "");
    if (body) {
        body.disabled = unavailable;
        body.placeholder = unavailable ? tr("Conversation access requires runtime:read") : tr("Message this Session…");
    }
    if (kind) {
        kind.disabled = unavailable || !!edit;
        if (edit)
            kind.value = String(edit.kind || "note");
    }
    if (priority) {
        priority.disabled = unavailable || !!edit;
        if (edit)
            priority.value = String(edit.priority || "normal");
    }
    if (checkbox && edit)
        checkbox.checked = !!edit.requires_ack;
    if (send) {
        const actionLabel = edit ? tr("Replace message") : tr("Send message");
        send.title = actionLabel;
        send.setAttribute("aria-label", actionLabel);
        send.classList.toggle("replace-mode", !!edit);
    }
    syncAckComposer();
    syncCollaborationComposerLayout(body, el("runtime-collaboration-form"), send);
    if (unavailable && checkbox)
        checkbox.disabled = true;
}
function scrollCollaborationToLatest(smooth) {
    const scroll = el("runtime-chat-scroll");
    if (!scroll)
        return;
    collaborationFollowLatest = true;
    collaborationPendingMessages = 0;
    syncNewMessageIndicator();
    const behavior = smooth && !window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "smooth" : "auto";
    window.requestAnimationFrame(() => {
        scroll.scrollTo({ top: scroll.scrollHeight, behavior });
    });
}
function syncComposerOptionSummary() {
    const kind = el("runtime-message-kind");
    const priority = el("runtime-message-priority");
    const checkbox = el("runtime-message-requires-ack");
    const options = el("runtime-message-options");
    const summary = formatComposerOptionSummary(String(kind?.value || ""), String(priority?.value || ""), !!checkbox?.checked, runtimeLanguage);
    setText("runtime-message-options-label", summary.label);
    options?.classList.toggle("has-selection", summary.hasSelection);
}
function setCollaborationReplyTarget(messageId) {
    collaborationReplyTo = messageId;
    const wasEditing = !!runtimeCollaborationEditTarget(state);
    setRuntimeCollaborationReplyTarget(state, messageId);
    const body = el("runtime-message-body");
    if (wasEditing && body)
        body.value = "";
    syncCollaborationComposer();
    if (messageId) {
        setText("runtime-message-send-status", runtimeLanguage === "zh-CN"
            ? "已选择回复目标。下一条消息将回复 " + messageId + "。"
            : "Reply target selected. Your next message will reply to " + messageId + ".");
        body?.focus();
    }
    else {
        setText("runtime-message-send-status", tr("Reply target cleared."));
    }
}
function beginCollaborationEdit(message) {
    saveCurrentDraft();
    if (!setRuntimeCollaborationEditTarget(state, String(message?.message_id || "")))
        return;
    collaborationReplyTo = "";
    const body = el("runtime-message-body");
    if (body) {
        body.value = String(message?.message || "");
        body.focus();
    }
    setText("runtime-message-send-status", "");
    syncCollaborationComposer();
}
function cancelCollaborationEdit() {
    clearRuntimeCollaborationEditTarget(state);
    restoreCurrentDraft();
    setText("runtime-message-send-status", tr("Edit cancelled."));
    syncCollaborationComposer();
}
function resetCollaborationComposerUi() {
    const body = el("runtime-message-body");
    if (body)
        body.value = "";
    closeComposerOptions(false);
    syncCollaborationComposer();
}
function filterCollaborationMessages() {
    const query = el("runtime-message-search")?.value || "";
    const cards = Array.from(document.querySelectorAll("#runtime-collaboration-board .message-card"));
    const separators = Array.from(document.querySelectorAll("#runtime-collaboration-board .message-date-separator"));
    const result = filterCollaborationCards(cards, separators, state.collaboration.messages, query);
    setText("runtime-message-search-status", query.trim() ? result.matches + " / " + result.total : "");
}
function renderCollaboration(statusText, consumeMutationNotice = true) {
    const mutationNotice = consumeMutationNotice ? takeRuntimeCollaborationMutationNotice(state) : "";
    if (mutationNotice) {
        const editStillActive = !!runtimeCollaborationEditTarget(state);
        if (mutationNotice.includes("changed while editing")
            || mutationNotice.includes("Replacement confirmed")
            || mutationNotice.includes("Withdraw confirmed")
            || (mutationNotice.includes("Outcome not observed") && !editStillActive)) {
            const body = el("runtime-message-body");
            if (body)
                body.value = "";
        }
        const localizedMutationNotice = tr(mutationNotice);
        setText("runtime-message-send-status", localizedMutationNotice);
        statusText = [statusText ? tr(statusText) : "", localizedMutationNotice].filter(Boolean).join(" · ");
    }
    const available = state.collaboration.available !== false;
    if (!available) {
        const body = el("runtime-message-body");
        if (body)
            body.value = "";
    }
    show("runtime-collaboration-unavailable", !available);
    show("runtime-collaboration-form", true);
    el("runtime-collaboration-form")?.classList.toggle("is-unavailable", !available);
    const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
    const scroll = el("runtime-chat-scroll");
    const previousScrollTop = scroll?.scrollTop || 0;
    const shouldFollowNewMessages = collaborationFollowLatest || chatIsNearLatest();
    const previouslyRenderedMessageIds = renderedCollaborationMessageIds;
    const nextRenderedMessageIds = new Set(messages.map((message) => String(message?.message_id || "")).filter(Boolean));
    const newMessageIds = Array.from(nextRenderedMessageIds).filter((id) => !previouslyRenderedMessageIds.has(id));
    const hasNewMessages = newMessageIds.length > 0;
    const firstRetainedRender = previouslyRenderedMessageIds.size === 0 && nextRenderedMessageIds.size > 0;
    show("runtime-collaboration-board", messages.length > 0);
    show("runtime-collaboration-empty", messages.length === 0);
    setText("runtime-collaboration-empty-title", available ? tr("Start this Session conversation") : tr("Conversation access unavailable"));
    setText("runtime-collaboration-empty-copy", available
        ? tr("Messages posted here are retained on the Session collaboration board.")
        : tr("This credential can inspect the Project and Session, but retained messages require runtime:read."));
    const localizedStatusText = statusText ? tr(statusText) : "";
    const status = available
        ? (runtimeLanguage === "zh-CN" ? "协作：" : "Collaboration: ") + collaborationPhaseLabel(state.collaboration.phase, runtimeLanguage) + " · " + runtimeCountLabel(messages.length, "retained message") + (localizedStatusText ? " · " + localizedStatusText : "")
        : (runtimeLanguage === "zh-CN" ? "runtime:read 不可用" : "runtime:read unavailable");
    setText("runtime-collaboration-status", status);
    const node = el("runtime-collaboration-board");
    syncCollaborationComposer();
    if (!available)
        setHumanJoinSendEnabled(false);
    const signature = renderFingerprint({
        language: runtimeLanguage,
        sessionId: state.collaboration.sessionId,
        available,
        phase: state.collaboration.phase,
        uncertainMutation: state.collaboration.uncertainMutation,
        locallyAuthored: Array.from(locallyAuthoredCollaborationMessageIds).sort(),
        messages,
    });
    if (!node) {
        renderedCollaborationMessageIds = nextRenderedMessageIds;
        return;
    }
    if (signature === renderedCollaborationSignature) {
        renderedCollaborationMessageIds = nextRenderedMessageIds;
        return;
    }
    const latestAgent = el("runtime-latest-agent-message");
    if (latestAgent) {
        renderLatestAgentMessage(latestAgent, messages, locallyAuthoredCollaborationMessageIds, runtimeLanguage);
    }
    renderedCollaborationSignature = signature;
    clearNode(node);
    if (!available) {
        renderedCollaborationMessageIds = nextRenderedMessageIds;
        return;
    }
    renderCollaborationMessageCards(node, messages, {
        locallyAuthoredIds: locallyAuthoredCollaborationMessageIds,
        previouslyRenderedMessageIds,
        canMutate: state.collaboration.phase === "live" && !state.collaboration.uncertainMutation,
        language: runtimeLanguage,
        onReply: (id) => setCollaborationReplyTarget(id),
        onEdit: (message) => beginCollaborationEdit(message),
        onWithdraw: (id) => void withdrawHumanCollaborationMessage(id),
    });
    renderedCollaborationMessageIds = nextRenderedMessageIds;
    filterCollaborationMessages();
    if (hasNewMessages && !firstRetainedRender)
        announceNewCollaborationMessages(newMessageIds.length);
    if (firstRetainedRender || (hasNewMessages && shouldFollowNewMessages)) {
        scrollCollaborationToLatest(!firstRetainedRender);
    }
    else {
        window.requestAnimationFrame(() => {
            if (scroll)
                scroll.scrollTop = previousScrollTop;
        });
        if (hasNewMessages) {
            collaborationFollowLatest = false;
            collaborationPendingMessages += newMessageIds.length;
            syncNewMessageIndicator();
        }
    }
}
async function confirmCollaborationMutationDurability(request, mutation, controller) {
    const replacing = mutation?.kind === "replace";
    const payload = {
        project: request.project,
        session_id: request.sessionId,
        message_id: String(mutation?.messageId || ""),
    };
    if (replacing)
        payload.message = String(mutation?.message || "");
    setText("runtime-message-send-status", tr(replacing
        ? "Confirming replacement durability…"
        : "Confirming withdrawal durability…"));
    const response = await api(replacing ? "workflow-session-replace-message" : "workflow-session-withdraw-message", payload, controller.signal);
    if (!response || !isCurrentRuntimeCollaborationRequest(state, request))
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 0 || response.status === 503) {
        markRuntimeCollaborationMutationUncertain(state, request, mutation);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("durability confirmation still uncertain · refresh before retry");
        return false;
    }
    if (response.status === 403) {
        setRuntimeCollaborationAvailable(state, request, false);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration();
        return false;
    }
    if (response.status === 404 || response.status === 409) {
        markRuntimeCollaborationMutationUncertain(state, request, mutation);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("message changed during durability confirmation · refresh retained state");
        return false;
    }
    const valid = replacing
        ? response.ok && response.data?.original && response.data?.replacement
        : response.ok && response.data?.message;
    if (!valid) {
        markRuntimeCollaborationMutationUncertain(state, request, mutation);
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("durability confirmation failed · refresh before retry");
        return false;
    }
    if (replacing) {
        adoptRuntimeCollaborationObservation(state, request, {
            messages: [response.data.original, response.data.replacement],
        });
    }
    else {
        adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
    }
    completeRuntimeCollaborationMutationRecovery(state, request, replacing
        ? "Replacement durably confirmed after exact replay."
        : "Withdraw durably confirmed after exact replay.");
    return true;
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
    const mutationRecovery = runtimeCollaborationMutationRecovery(state, request);
    if (mutationRecovery && !(await confirmCollaborationMutationDurability(request, mutationRecovery, controller)))
        return null;
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
function sessionCollaborationAuthorityFailure(response) {
    if (response?.status !== 403)
        return null;
    return "Session collaboration access required. This credential can still read the Session; add session:collaborate to send, edit, or withdraw messages.";
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
    const edit = runtimeCollaborationEditTarget(state);
    const guidance = edit ? edit.kind === "guidance" : kind?.value === "guidance";
    show("runtime-message-ack-label", guidance);
    if (!checkbox) {
        syncComposerOptionSummary();
        return;
    }
    if (edit) {
        checkbox.disabled = true;
        checkbox.checked = !!edit.requires_ack;
        checkbox.title = "Inherited from the original retained message.";
        syncComposerOptionSummary();
        return;
    }
    checkbox.disabled = !guidance || priority?.value !== "high";
    if (checkbox.disabled)
        checkbox.checked = false;
    checkbox.title = guidance && priority?.value !== "high" ? "ACK requirement is available for High priority guidance." : "";
    syncComposerOptionSummary();
}
async function withdrawHumanCollaborationMessage(messageId) {
    const request = runtimeCollaborationRequest(state);
    if (!request || state.collaboration.available === false)
        return;
    setText("runtime-message-send-status", tr("Withdrawing retained message…"));
    const response = await api("workflow-session-withdraw-message", {
        project: request.project,
        session_id: request.sessionId,
        message_id: messageId,
    });
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return;
    if (response?.status === 0 || response?.status === 503) {
        markRuntimeCollaborationMutationUncertain(state, request, { kind: "withdraw", messageId });
        abortCollaboration();
        setRuntimeCollaborationPhase(state, request, "paused");
        renderCollaboration("withdraw outcome unknown · refresh before retry");
        return;
    }
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    const authorityFailure = sessionCollaborationAuthorityFailure(response);
    if (authorityFailure) {
        setText("runtime-message-send-status", authorityFailure);
        return;
    }
    if (response?.status === 409) {
        abortCollaboration();
        setRuntimeCollaborationPhase(state, request, "paused");
        setText("runtime-message-send-status", tr("Message changed before Delete. Refresh retained messages before retrying."));
        renderCollaboration("message changed · refresh retained state");
        return;
    }
    if (!response?.ok || !response.data?.message) {
        setText("runtime-message-send-status", tr("Delete failed."));
        return;
    }
    if (String(state.collaboration.editTargetId || "") === messageId) {
        clearRuntimeCollaborationEditTarget(state);
        restoreCurrentDraft();
    }
    adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
    setText("runtime-message-send-status", tr("Retained message withdrawn."));
    renderCollaboration();
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
        setText("runtime-message-send-status", tr("Enter a message."));
        return;
    }
    closeComposerOptions(false);
    const editTarget = runtimeCollaborationEditTarget(state);
    if (editTarget) {
        if (send)
            send.disabled = true;
        setText("runtime-message-send-status", tr("Replacing retained message…"));
        const response = await api("workflow-session-replace-message", {
            project: request.project,
            session_id: request.sessionId,
            message_id: editTarget.message_id,
            message,
        });
        if (!isCurrentRuntimeCollaborationRequest(state, request))
            return;
        if (response?.status === 0 || response?.status === 503) {
            markRuntimeCollaborationMutationUncertain(state, request, {
                kind: "replace",
                messageId: String(editTarget.message_id),
                message,
            });
            abortCollaboration();
            setRuntimeCollaborationPhase(state, request, "paused");
            renderCollaboration("replace outcome unknown · refresh before retry");
            return;
        }
        if (send)
            send.disabled = false;
        if (response?.status === 401) {
            lock("Credential rejected.");
            return;
        }
        const authorityFailure = sessionCollaborationAuthorityFailure(response);
        if (authorityFailure) {
            setText("runtime-message-send-status", authorityFailure);
            return;
        }
        if (response?.status === 409) {
            clearRuntimeCollaborationEditTarget(state);
            restoreCurrentDraft();
            abortCollaboration();
            setRuntimeCollaborationPhase(state, request, "paused");
            setText("runtime-message-send-status", tr("Message changed before Replace. Refresh retained messages before retrying."));
            renderCollaboration("message changed · refresh retained state");
            return;
        }
        if (!response?.ok || !response.data?.original || !response.data?.replacement) {
            setText("runtime-message-send-status", tr("Replace failed."));
            return;
        }
        clearRuntimeCollaborationEditTarget(state);
        rememberLocalCollaborationMessage(response.data.replacement?.message_id);
        adoptRuntimeCollaborationObservation(state, request, {
            messages: [response.data.original, response.data.replacement],
        });
        restoreCurrentDraft();
        setText("runtime-message-send-status", tr(response.data.replayed ? "Replacement already retained." : "Message replaced."));
        renderCollaboration();
        return;
    }
    if (send)
        send.disabled = true;
    setText("runtime-message-send-status", tr("Sending…"));
    const response = await api("workflow-session-post-message", {
        project: request.project,
        session_id: request.sessionId,
        kind: kind?.value || "note",
        priority: priority?.value || "normal",
        message,
        reply_to: state.collaboration.replyTargetId || null,
        requires_ack: !!checkbox?.checked,
    });
    if (!isCurrentRuntimeCollaborationRequest(state, request))
        return;
    if (response?.status === 0) {
        abortCollaboration();
        setRuntimeCollaborationPhase(state, request, "paused");
        setText("runtime-message-send-status", tr("Send outcome unknown. Refresh and review retained messages before retrying."));
        renderCollaboration("send outcome unknown · refresh before retry");
        return;
    }
    if (send)
        send.disabled = false;
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    const authorityFailure = sessionCollaborationAuthorityFailure(response);
    if (authorityFailure) {
        setText("runtime-message-send-status", authorityFailure);
        return;
    }
    if (!response?.ok || !response.data) {
        setText("runtime-message-send-status", tr("Send failed."));
        return;
    }
    rememberLocalCollaborationMessage(response.data?.message_id);
    adoptRuntimeCollaborationObservation(state, request, { messages: [response.data] });
    if (body)
        body.value = "";
    clearCurrentDraft();
    setCollaborationReplyTarget("");
    setText("runtime-message-send-status", tr("Sent."));
    renderCollaboration();
}
function communicationAgent(agentId) {
    return communicationAgents.find((agent) => String(agent?.agent_id || "") === agentId) || null;
}
function selectedCommunicationAgent() {
    return communicationAgent(selectedCommunicationAgentId);
}
function selectedCommunicationConversation() {
    return communicationConversations.find((conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId) || null;
}
function communicationEndpoint(agentId = selectedCommunicationAgentId) {
    return communicationEndpoints.get(agentId) || null;
}
function communicationEndpointId(agentId = selectedCommunicationAgentId) {
    return communicationEndpoint(agentId)?.endpoint_id || "";
}
function resetCommunicationSurface() {
    communicationGeneration += 1;
    renderedCommunicationSurfaceSignature = "";
    communicationAgents = [];
    communicationConversations = [];
    communicationDetail = null;
    communicationInbox = [];
    selectedCommunicationAgentId = "";
    selectedCommunicationConversationId = "";
    communicationReadAvailable = null;
    communicationManageAvailable = null;
    communicationRefreshCoordinator.reset();
    communicationEndpoints.clear();
    pendingEndpointAttach.clear();
    pendingAgentCreate = null;
    pendingConversationCreate = null;
    pendingConversationMessage = null;
    const agentUpdateForm = el("runtime-agent-update-form");
    if (agentUpdateForm) {
        delete agentUpdateForm.dataset.agentId;
        delete agentUpdateForm.dataset.profileRevision;
    }
    clearNode(el("runtime-agent-list"));
    clearNode(el("runtime-conversation-list"));
    clearNode(el("runtime-conversation-transcript"));
    clearNode(el("runtime-conversation-participants"));
    clearNode(el("runtime-inbox-list"));
    renderCommunicationSurface();
}
function detachCommunicationEndpointsBestEffort() {
    if (!token || communicationEndpoints.size === 0)
        return;
    const credential = token;
    for (const endpoint of communicationEndpoints.values()) {
        void fetch(API_BASE + "communication/endpoint/detach", {
            method: "POST",
            headers: { Authorization: "Bearer " + credential, "Content-Type": "application/json" },
            body: JSON.stringify({ endpoint_id: endpoint.endpoint_id }),
            keepalive: true,
        }).catch(() => undefined);
    }
    communicationEndpoints.clear();
}
function renderCommunicationAvailability() {
    const available = communicationReadAvailable !== false;
    show("runtime-communication-unavailable", !available);
    show("runtime-communication-surface", available);
    setText("runtime-communication-status", formatCommunicationAvailability(communicationReadAvailable, communicationManageAvailable, runtimeLanguage));
}
function renderCommunicationAgents() {
    setText("runtime-communication-count", runtimeCountLabel(communicationAgents.length, "Agent"));
    const list = el("runtime-agent-list");
    show("runtime-agent-empty", communicationReadAvailable === true && communicationAgents.length === 0);
    renderAgentRows(list, communicationAgents, selectedCommunicationAgentId, {
        language: runtimeLanguage,
        onSelect: (agentId) => {
            selectedCommunicationAgentId = agentId;
            communicationInbox = [];
            const participants = el("runtime-conversation-agent-ids");
            if (participants && !participants.value.trim())
                participants.value = agentId;
            renderCommunicationAgents();
            renderCommunicationAgentCard();
            renderCommunicationInbox();
            if (communicationEndpointId(agentId))
                void fetchCommunicationInbox(communicationGeneration);
        },
    });
}
function renderCommunicationAgentCard() {
    const agent = selectedCommunicationAgent();
    show("runtime-agent-card", !!agent);
    if (!agent)
        return;
    const agentId = String(agent.agent_id || "");
    setText("runtime-agent-card-name", String(agent.display_name || agent.handle || tr("Agent Card")) + " · @" + String(agent.handle || "agent"));
    setText("runtime-agent-card-id", agentId);
    setText("runtime-agent-card-description", String(agent.description || tr("No description.")));
    setText("runtime-agent-card-revision", formatAgentCardRevision(agent, runtimeLanguage));
    setText("runtime-agent-unread", runtimeCountLabel(agent.queued_delivery_count, "queued"));
    const labels = el("runtime-agent-card-labels");
    clearNode(labels);
    if (labels) {
        for (const label of Array.isArray(agent.specialty_labels) ? agent.specialty_labels : []) {
            appendChip(labels, String(label));
        }
    }
    setText("runtime-agent-wake-status", formatAgentWakeStatus(agent, runtimeLanguage));
    const updateForm = el("runtime-agent-update-form");
    const revision = String(agent.profile_revision || 0);
    if (updateForm && (updateForm.dataset.agentId !== agentId
        || updateForm.dataset.profileRevision !== revision)) {
        const handle = el("runtime-agent-update-handle");
        const displayName = el("runtime-agent-update-display-name");
        const description = el("runtime-agent-update-description");
        const labelsInput = el("runtime-agent-update-labels");
        if (handle)
            handle.value = String(agent.handle || "");
        if (displayName)
            displayName.value = String(agent.display_name || "");
        if (description)
            description.value = String(agent.description || "");
        if (labelsInput) {
            labelsInput.value = Array.isArray(agent.specialty_labels)
                ? agent.specialty_labels.join(", ")
                : "";
        }
        updateForm.dataset.agentId = agentId;
        updateForm.dataset.profileRevision = revision;
        setText("runtime-agent-update-status", "");
    }
    const endpoint = communicationEndpoint(agentId);
    setText("runtime-agent-endpoint-status", formatAgentEndpointStatus(endpoint, runtimeLanguage));
    show("runtime-agent-attach", !endpoint);
    show("runtime-agent-detach", !!endpoint);
}
function renderCommunicationConversations() {
    const list = el("runtime-conversation-list");
    show("runtime-conversation-empty", communicationReadAvailable === true && communicationConversations.length === 0);
    renderConversationRows(list, communicationConversations, selectedCommunicationConversationId, {
        language: runtimeLanguage,
        onSelect: (conversationId) => {
            selectedCommunicationConversationId = conversationId;
            communicationDetail = null;
            renderCommunicationConversations();
            renderCommunicationConversation();
            void fetchCommunicationConversation(communicationGeneration);
        },
    });
}
function renderCommunicationConversation() {
    const detail = communicationDetail;
    const summary = detail?.conversation || selectedCommunicationConversation();
    const available = !!summary && String(summary?.conversation_id || "") === selectedCommunicationConversationId;
    show("runtime-conversation-detail", available);
    show("runtime-conversation-detail-empty", !available);
    const transcript = el("runtime-conversation-transcript");
    clearNode(transcript);
    clearNode(el("runtime-conversation-participants"));
    if (!available || !detail)
        return;
    setText("runtime-conversation-name", String(summary.title || tr("Untitled Conversation")));
    setText("runtime-conversation-id", String(summary.conversation_id || ""));
    setText("runtime-conversation-seq", formatConversationSeq(summary, detail, runtimeLanguage));
    const participants = el("runtime-conversation-participants");
    if (participants) {
        for (const participant of Array.isArray(detail.participants) ? detail.participants : []) {
            const kind = String(participant?.participant_kind || "participant");
            const label = kind === "agent"
                ? "Agent · " + String(participant?.display_name || participant?.handle || participant?.agent_id || tr("unknown"))
                : (runtimeLanguage === "zh-CN" ? "人工 · " : "Human · ") + String(participant?.principal_kind || (runtimeLanguage === "zh-CN" ? "凭证主体" : "credential principal"));
            appendChip(participants, label, kind === "agent" ? "tone-pass" : "tone-runtime");
        }
    }
    const messages = Array.isArray(detail.messages) ? detail.messages : [];
    show("runtime-conversation-transcript-empty", messages.length === 0);
    renderConversationMessages(transcript, messages, communicationAgents, {
        language: runtimeLanguage,
    });
}
function renderCommunicationInbox() {
    const list = el("runtime-inbox-list");
    const agent = selectedCommunicationAgent();
    const endpointId = communicationEndpointId();
    show("runtime-inbox-consume-all", !!endpointId && communicationInbox.length > 0);
    if (!agent) {
        clearNode(list);
        setText("runtime-inbox-status", runtimeLanguage === "zh-CN" ? "选择一个 Agent 以查看收件人专属的排队投递。" : "Select an Agent to inspect recipient-specific queued deliveries.");
        return;
    }
    if (!endpointId) {
        clearNode(list);
        setText("runtime-inbox-status", runtimeLanguage === "zh-CN" ? "将此浏览器附加为端点。离线期间排队投递仍会持久保留。" : "Attach this browser as an Endpoint. Queued deliveries remain durable while offline.");
        return;
    }
    const totalQueued = Number(agent.queued_delivery_count || 0);
    setText("runtime-inbox-status", runtimeCountLabel(totalQueued, "queued delivery")
        + (communicationInbox.length < totalQueued ? (runtimeLanguage === "zh-CN" ? " · 当前显示 " : " · showing ") + String(communicationInbox.length) : "")
        + (runtimeLanguage === "zh-CN" ? " · 读取不会消费投递或唤醒模型" : " · reading does not consume or wake a model"));
    renderInboxDeliveryCards(list, communicationInbox, communicationAgents, {
        language: runtimeLanguage,
        onConsume: (deliveryId) => void consumeCommunicationDeliveries([deliveryId]),
    });
}
function renderCommunicationSurface() {
    renderCommunicationAvailability();
    const signature = renderFingerprint({
        language: runtimeLanguage,
        read: communicationReadAvailable,
        manage: communicationManageAvailable,
        selectedAgent: selectedCommunicationAgentId,
        selectedConversation: selectedCommunicationConversationId,
        endpoints: Array.from(communicationEndpoints.entries()).sort(([left], [right]) => left.localeCompare(right)),
        agents: communicationAgents,
        conversations: communicationConversations,
        detail: communicationDetail,
        inbox: communicationInbox,
    });
    if (signature === renderedCommunicationSurfaceSignature)
        return;
    renderedCommunicationSurfaceSignature = signature;
    renderCommunicationAgents();
    renderCommunicationAgentCard();
    renderCommunicationConversations();
    renderCommunicationConversation();
    renderCommunicationInbox();
}
async function fetchCommunicationAgents(generation, render = true) {
    const response = await api("communication/agents", { offset: 0, limit: 100 });
    if (generation !== communicationGeneration || !response)
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        communicationReadAvailable = false;
        communicationAgents = [];
        communicationConversations = [];
        communicationDetail = null;
        communicationInbox = [];
        if (render)
            renderCommunicationSurface();
        return true;
    }
    if (!response.ok || !response.data)
        return false;
    communicationReadAvailable = true;
    communicationAgents = Array.isArray(response.data.agents) ? response.data.agents : [];
    if (!communicationAgents.some((agent) => String(agent?.agent_id || "") === selectedCommunicationAgentId)) {
        selectedCommunicationAgentId = String(communicationAgents[0]?.agent_id || "");
        communicationInbox = [];
    }
    if (render) {
        renderCommunicationAgents();
        renderCommunicationAgentCard();
    }
    return true;
}
async function fetchCommunicationConversations(generation, render = true) {
    const response = await api("communication/conversations", { offset: 0, limit: 100 });
    if (generation !== communicationGeneration || !response)
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        communicationReadAvailable = false;
        if (render)
            renderCommunicationSurface();
        return true;
    }
    if (!response.ok || !response.data)
        return false;
    communicationReadAvailable = true;
    communicationConversations = Array.isArray(response.data.conversations) ? response.data.conversations : [];
    if (!communicationConversations.some((conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId)) {
        selectedCommunicationConversationId = String(communicationConversations[0]?.conversation_id || "");
        communicationDetail = null;
    }
    if (render)
        renderCommunicationConversations();
    return true;
}
async function fetchCommunicationConversation(generation, render = true) {
    const conversationId = selectedCommunicationConversationId;
    if (!conversationId) {
        communicationDetail = null;
        if (render)
            renderCommunicationConversation();
        return true;
    }
    const afterSeq = runtimeCommunicationTranscriptAfterSeq(selectedCommunicationConversation()?.last_seq, 100);
    const response = await api("communication/conversation", {
        conversation_id: conversationId,
        after_seq: afterSeq,
        limit: 100,
    });
    if (generation !== communicationGeneration || conversationId !== selectedCommunicationConversationId || !response)
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        communicationReadAvailable = false;
        if (render)
            renderCommunicationSurface();
        return true;
    }
    if (response.status === 404) {
        communicationDetail = null;
        selectedCommunicationConversationId = "";
        if (render)
            renderCommunicationConversation();
        return false;
    }
    if (!response.ok || !response.data)
        return false;
    communicationDetail = response.data;
    if (render)
        renderCommunicationConversation();
    return true;
}
async function fetchCommunicationInbox(generation, render = true) {
    const agentId = selectedCommunicationAgentId;
    const endpoint = communicationEndpoint(agentId);
    const endpointId = endpoint?.endpoint_id || "";
    if (!agentId || !endpoint) {
        communicationInbox = [];
        if (render)
            renderCommunicationInbox();
        return true;
    }
    const response = await api("communication/inbox", {
        agent_id: agentId,
        endpoint_id: endpointId,
        expected_controller_generation: endpoint.controller_generation,
        after_delivery_order: 0,
        limit: 100,
    });
    if (generation !== communicationGeneration || agentId !== selectedCommunicationAgentId || endpointId !== communicationEndpointId(agentId) || !response)
        return false;
    if (response.status === 401) {
        lock("Credential rejected.");
        return false;
    }
    if (response.status === 403) {
        communicationReadAvailable = false;
        if (render)
            renderCommunicationSurface();
        return true;
    }
    if (response.status === 404 || response.status === 400) {
        communicationEndpoints.delete(agentId);
        communicationInbox = [];
        if (render) {
            renderCommunicationAgentCard();
            renderCommunicationInbox();
        }
        return false;
    }
    if (!response.ok || !response.data)
        return false;
    communicationInbox = Array.isArray(response.data.deliveries) ? response.data.deliveries : [];
    if (render)
        renderCommunicationInbox();
    return true;
}
async function renewCommunicationEndpoints(generation) {
    if (communicationEndpoints.size === 0 || communicationManageAvailable === false)
        return true;
    for (const [agentId, endpoint] of Array.from(communicationEndpoints.entries())) {
        const response = await api("communication/endpoint/renew", {
            endpoint_id: endpoint.endpoint_id,
            expected_controller_generation: endpoint.controller_generation,
        });
        if (generation !== communicationGeneration)
            return false;
        if (!response)
            return false;
        if (response.status === 401) {
            lock("Credential rejected.");
            return false;
        }
        if (response.status === 403) {
            communicationManageAvailable = false;
            renderCommunicationAvailability();
            return true;
        }
        if (response.status === 400 || response.status === 404) {
            communicationEndpoints.delete(agentId);
            if (agentId === selectedCommunicationAgentId)
                communicationInbox = [];
            continue;
        }
        if (!response.ok || !response.data?.endpoint?.endpoint_id)
            return false;
        communicationManageAvailable = true;
        communicationEndpoints.set(agentId, response.data.endpoint);
    }
    return true;
}
async function performCommunicationRefresh(includeData) {
    if (!token)
        return true;
    const generation = ++communicationGeneration;
    if (includeData && communicationReadAvailable !== false) {
        setText("runtime-communication-status", tr("Refreshing durable communication…"));
    }
    const endpointsOk = await renewCommunicationEndpoints(generation);
    if (generation !== communicationGeneration)
        return false;
    if (!includeData || communicationReadAvailable === false)
        return endpointsOk;
    const agentsOk = await fetchCommunicationAgents(generation, false);
    if (generation !== communicationGeneration || !agentsOk || communicationReadAvailable !== true) {
        renderCommunicationSurface();
        return endpointsOk && agentsOk;
    }
    const conversationsOk = await fetchCommunicationConversations(generation, false);
    if (generation !== communicationGeneration || !conversationsOk || communicationReadAvailable !== true) {
        renderCommunicationSurface();
        return endpointsOk && agentsOk && conversationsOk;
    }
    const [conversationOk, inboxOk] = await Promise.all([
        fetchCommunicationConversation(generation, false),
        fetchCommunicationInbox(generation, false),
    ]);
    renderCommunicationSurface();
    return endpointsOk && agentsOk && conversationsOk && conversationOk && inboxOk;
}
function refreshCommunication(includeData = true) {
    if (!token)
        return Promise.resolve(true);
    return communicationRefreshCoordinator.refresh(includeData);
}
async function createCommunicationAgent(event) {
    event.preventDefault();
    const handle = el("runtime-agent-handle")?.value || "";
    const displayName = el("runtime-agent-display-name")?.value || "";
    const description = el("runtime-agent-description")?.value || "";
    const labelsRaw = el("runtime-agent-labels")?.value || "";
    const validation = validateAgentCreateInputs(handle, displayName, description, labelsRaw);
    if (!validation.valid) {
        setText("runtime-agent-create-status", tr(validation.error));
        return;
    }
    const { data } = validation;
    pendingAgentCreate = idempotencyKeyFor(pendingAgentCreate, data.fingerprint, "runtime-agent");
    setText("runtime-agent-create-status", tr("Creating durable Agent…"));
    const response = await api("communication/agent/create", {
        handle: data.handle,
        display_name: data.displayName,
        description: data.description,
        specialty_labels: data.labels,
        idempotency_key: pendingAgentCreate.key,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-agent-create-status", tr("communication:manage required."));
        renderCommunicationAvailability();
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-agent-create-status", tr("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key, or refresh before deciding."));
        return;
    }
    if (!response.ok || !response.data?.agent) {
        setText("runtime-agent-create-status", String(response.data?.message || "Agent creation failed."));
        return;
    }
    communicationManageAvailable = true;
    selectedCommunicationAgentId = String(response.data.agent.agent_id || "");
    pendingAgentCreate = null;
    for (const id of ["runtime-agent-handle", "runtime-agent-display-name", "runtime-agent-description", "runtime-agent-labels"]) {
        const input = el(id);
        if (input)
            input.value = "";
    }
    setText("runtime-agent-create-status", tr(response.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."));
    await refreshCommunication();
}
async function updateCommunicationAgent(event) {
    event.preventDefault();
    const agent = selectedCommunicationAgent();
    if (!agent)
        return;
    const handle = el("runtime-agent-update-handle")?.value || "";
    const displayName = el("runtime-agent-update-display-name")?.value || "";
    const description = el("runtime-agent-update-description")?.value || "";
    const labelsRaw = el("runtime-agent-update-labels")?.value || "";
    const validation = validateAgentUpdateInputs(handle, displayName, description, labelsRaw);
    if (!validation.valid) {
        setText("runtime-agent-update-status", tr(validation.error));
        return;
    }
    const { data } = validation;
    setText("runtime-agent-update-status", tr("Updating Agent Card…"));
    const response = await api("communication/agent/update", {
        agent_id: String(agent.agent_id || ""),
        expected_profile_revision: Number(agent.profile_revision || 0),
        handle: data.handle,
        display_name: data.displayName,
        description: data.description,
        specialty_labels: data.specialtyLabels,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-agent-update-status", tr("communication:manage required."));
        renderCommunicationAvailability();
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-agent-update-status", tr("Outcome uncertain. Refresh the Card before deciding whether to retry."));
        return;
    }
    if (!response.ok || !response.data?.agent_id) {
        setText("runtime-agent-update-status", response.data?.message ? String(response.data.message) : tr("Agent Card update failed; refresh before retrying a stale revision."));
        return;
    }
    communicationManageAvailable = true;
    setText("runtime-agent-update-status", tr("Agent Card updated."));
    await refreshCommunication();
}
async function attachCommunicationEndpoint() {
    const agentId = selectedCommunicationAgentId;
    if (!agentId)
        return;
    for (const [otherAgentId, otherEndpoint] of Array.from(communicationEndpoints.entries())) {
        if (otherAgentId === agentId)
            continue;
        setText("runtime-agent-endpoint-status", tr("Releasing this window’s previous Agent Endpoint…"));
        const detached = await api("communication/endpoint/detach", {
            endpoint_id: otherEndpoint.endpoint_id,
        });
        if (detached?.status === 401) {
            lock("Credential rejected.");
            return;
        }
        if (detached?.status === 403) {
            communicationManageAvailable = false;
            setText("runtime-agent-endpoint-status", tr("communication:manage required."));
            return;
        }
        if (!detached || detached.status === 0 || detached.status === 503) {
            setText("runtime-agent-endpoint-status", tr("Previous Endpoint detach is uncertain. Refresh before switching this window to another Agent."));
            return;
        }
        if (!detached.ok && detached.status !== 404) {
            setText("runtime-agent-endpoint-status", detached.data?.message ? String(detached.data.message) : tr("Could not release the previous Agent Endpoint."));
            return;
        }
        communicationEndpoints.delete(otherAgentId);
    }
    let pending = pendingEndpointAttach.get(agentId);
    if (!pending) {
        pending = { key: operationKey("runtime-endpoint"), attachmentId: pageAttachmentId + "-" + agentId.slice(-8) };
        pendingEndpointAttach.set(agentId, pending);
    }
    setText("runtime-agent-endpoint-status", tr("Attaching browser Endpoint…"));
    const response = await api("communication/endpoint/attach", {
        agent_id: agentId,
        host: "Runtime Console",
        client_attachment_id: pending.attachmentId,
        idempotency_key: pending.key,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-agent-endpoint-status", tr("communication:manage required."));
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-agent-endpoint-status", tr("Outcome uncertain. Retry Attach to replay the same idempotency key; do not create a new attachment."));
        return;
    }
    if (!response.ok || !response.data?.endpoint?.endpoint_id) {
        setText("runtime-agent-endpoint-status", String(response.data?.message || "Endpoint attach failed."));
        return;
    }
    if (String(response.data.endpoint.lifecycle || "") !== "attached") {
        pendingEndpointAttach.delete(agentId);
        setText("runtime-agent-endpoint-status", tr("The exact Attach replay was already replaced. Choose “Continue as this Agent” again to create a fresh Endpoint generation."));
        return;
    }
    communicationManageAvailable = true;
    communicationEndpoints.set(agentId, response.data.endpoint);
    pendingEndpointAttach.delete(agentId);
    renderCommunicationAgentCard();
    await fetchCommunicationInbox(communicationGeneration);
}
async function detachCommunicationEndpoint() {
    const agentId = selectedCommunicationAgentId;
    const endpointId = communicationEndpointId(agentId);
    if (!agentId || !endpointId)
        return;
    setText("runtime-agent-endpoint-status", tr("Detaching browser Endpoint…"));
    const response = await api("communication/endpoint/detach", { endpoint_id: endpointId });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-agent-endpoint-status", tr("communication:manage required."));
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-agent-endpoint-status", tr("Detach outcome uncertain. Refresh before retry; the durable Agent and Inbox are unaffected."));
        return;
    }
    if (!response.ok) {
        setText("runtime-agent-endpoint-status", String(response.data?.message || "Endpoint detach failed."));
        return;
    }
    communicationEndpoints.delete(agentId);
    communicationInbox = [];
    renderCommunicationAgentCard();
    renderCommunicationInbox();
    await refreshCommunication();
}
async function createCommunicationConversation(event) {
    event.preventDefault();
    const title = el("runtime-conversation-title")?.value || "";
    const idsInput = el("runtime-conversation-agent-ids")?.value || "";
    const validation = validateConversationCreateInputs(title, idsInput, selectedCommunicationAgentId);
    if (!validation.valid) {
        setText("runtime-conversation-create-status", tr(validation.error));
        return;
    }
    const { data } = validation;
    pendingConversationCreate = idempotencyKeyFor(pendingConversationCreate, data.fingerprint, "runtime-conversation");
    setText("runtime-conversation-create-status", tr("Creating durable Conversation…"));
    const response = await api("communication/conversation/create", {
        title: data.title || null,
        agent_ids: data.agentIds,
        idempotency_key: pendingConversationCreate.key,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-conversation-create-status", tr("communication:manage required."));
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-conversation-create-status", tr("Outcome uncertain. Keep inputs unchanged and retry to replay the same idempotency key."));
        return;
    }
    if (!response.ok || !response.data?.conversation?.conversation?.conversation_id) {
        setText("runtime-conversation-create-status", String(response.data?.message || "Conversation creation failed."));
        return;
    }
    communicationManageAvailable = true;
    selectedCommunicationConversationId = String(response.data.conversation.conversation.conversation_id);
    pendingConversationCreate = null;
    const titleInput = el("runtime-conversation-title");
    const agentsInput = el("runtime-conversation-agent-ids");
    if (titleInput)
        titleInput.value = "";
    if (agentsInput)
        agentsInput.value = selectedCommunicationAgentId;
    setText("runtime-conversation-create-status", tr(response.data.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."));
    await refreshCommunication();
}
async function postCommunicationMessage(event) {
    event.preventDefault();
    const conversationId = selectedCommunicationConversationId;
    const bodyNode = el("runtime-conversation-body");
    const recipientsNode = el("runtime-conversation-recipients");
    const body = bodyNode?.value.trim() || "";
    const recipientsText = recipientsNode?.value.trim() || "";
    const sendAsAgent = el("runtime-conversation-send-as-agent")?.checked === true;
    if (!conversationId || !body) {
        setText("runtime-conversation-send-status", tr("Select a Conversation and enter a message."));
        return;
    }
    const actingAgent = sendAsAgent ? selectedCommunicationAgent() : null;
    const endpoint = actingAgent ? communicationEndpoint(String(actingAgent.agent_id || "")) : null;
    if (sendAsAgent && (!actingAgent || !endpoint)) {
        setText("runtime-conversation-send-status", tr("Select an Agent and choose “Continue as this Agent” before sending as it."));
        return;
    }
    const recipientAgentIds = recipientsText ? parseAgentIds(recipientsText) : null;
    const fingerprint = JSON.stringify({
        conversationId,
        body,
        recipientAgentIds,
        authorAgentId: actingAgent?.agent_id || null,
        endpointId: endpoint?.endpoint_id || null,
        controllerGeneration: endpoint?.controller_generation || null,
    });
    pendingConversationMessage = idempotencyKeyFor(pendingConversationMessage, fingerprint, "runtime-message");
    setText("runtime-conversation-send-status", tr("Appending Message and Agent deliveries atomically…"));
    const response = await api("communication/message/post", {
        conversation_id: conversationId,
        body,
        author_agent_id: actingAgent?.agent_id || null,
        endpoint_id: endpoint?.endpoint_id || null,
        expected_controller_generation: endpoint?.controller_generation || null,
        recipient_agent_ids: recipientAgentIds,
        idempotency_key: pendingConversationMessage.key,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-conversation-send-status", tr("communication:manage required."));
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-conversation-send-status", tr("Outcome uncertain. Keep the message unchanged and retry only to replay the same idempotency key, or refresh the transcript first."));
        return;
    }
    if (!response.ok || !response.data?.message) {
        setText("runtime-conversation-send-status", String(response.data?.message || "Message append failed."));
        return;
    }
    communicationManageAvailable = true;
    pendingConversationMessage = null;
    if (bodyNode)
        bodyNode.value = "";
    setText("runtime-conversation-send-status", tr(response.data.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."));
    await refreshCommunication();
}
async function consumeCommunicationDeliveries(deliveryIds) {
    const agentId = selectedCommunicationAgentId;
    const endpoint = communicationEndpoint(agentId);
    const endpointId = endpoint?.endpoint_id || "";
    const ids = deliveryIds.filter(Boolean);
    if (!agentId || !endpoint || ids.length === 0)
        return;
    setText("runtime-inbox-status", tr("Consuming recipient state…"));
    const response = await api("communication/inbox/consume", {
        agent_id: agentId,
        endpoint_id: endpointId,
        expected_controller_generation: endpoint.controller_generation,
        delivery_ids: ids,
    });
    if (response?.status === 401) {
        lock("Credential rejected.");
        return;
    }
    if (response?.status === 403) {
        communicationManageAvailable = false;
        setText("runtime-inbox-status", tr("communication:manage required to consume deliveries."));
        return;
    }
    if (!response || response.status === 0 || response.status === 503) {
        setText("runtime-inbox-status", tr("Consume outcome uncertain. Refresh before retry; desired-state replay is safe."));
        return;
    }
    if (!response.ok) {
        setText("runtime-inbox-status", response.data?.message ? String(response.data.message) : tr("Delivery consume failed."));
        return;
    }
    communicationManageAvailable = true;
    await refreshCommunication();
}
function setRefreshBusy(active) {
    refreshInFlight = active;
    const button = el("runtime-refresh");
    if (button) {
        button.disabled = active;
        button.classList.toggle("is-busy", active);
        button.title = active ? tr("Refreshing runtime") : tr("Refresh runtime");
        button.setAttribute("aria-label", button.title);
    }
}
async function refreshAll() {
    productWorkspace.invalidateGit();
    if (workspaceView === "extensions")
        void productExtensions.refresh();
    if (!token || refreshInFlight)
        return;
    setRefreshBusy(true);
    setText("runtime-refresh-status", tr("Refreshing…"));
    const recoverCollaboration = runtimeCollaborationNeedsRefreshRecovery(state);
    const overviewRequest = refreshRuntimeOverview(state);
    const projectsRequest = refreshRuntimeProjects(state, projectSearch, projectDeviceFilter);
    const windowRequest = refreshRuntimeProjectWindows(state);
    try {
        const [overviewOk, projectsOk, communicationOk] = await Promise.all([
            fetchOverview(overviewRequest),
            fetchProjects(projectsRequest),
            refreshCommunication(),
            windowRequest ? fetchProjectWindows(windowRequest) : Promise.resolve(true),
        ]);
        if (!token)
            return;
        if (overviewOk && projectsOk && communicationOk) {
            setText("runtime-refresh-status", tr("Refreshed") + " " + new Date().toLocaleTimeString());
        }
        else {
            setText("runtime-refresh-status", tr("Refresh failed · showing previous data"));
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
function refreshAutoSurfaces() {
    if (!token)
        return;
    if (document.hidden) {
        void refreshCommunication(false);
        return;
    }
    void fetchOverview(refreshRuntimeOverview(state));
    const request = refreshRuntimeSessionList(state);
    if (request)
        void fetchSessions(request);
    const windowRequest = refreshRuntimeProjectWindows(state);
    if (windowRequest)
        void fetchProjectWindows(windowRequest);
    void refreshCommunication(workspaceView === "operations");
}
function startAuto() {
    stopAuto();
    timer = window.setInterval(refreshAutoSurfaces, REFRESH_MS);
}
function stopAuto() { if (timer)
    window.clearInterval(timer); timer = 0; }
function startWindowAuto() {
    stopWindowAuto();
    if (!token || workspaceView !== "windows" || document.hidden)
        return;
    windowTimer = window.setInterval(() => void refreshWindows(true), WINDOW_REFRESH_MS);
}
function stopWindowAuto() {
    if (windowTimer)
        window.clearInterval(windowTimer);
    windowTimer = 0;
}
function connectRuntimeCredential(nextToken, rememberForTab) {
    productWorkspace.reset();
    productExtensions.reset();
    productProjectApi.clearToken();
    rememberCredentialForTab = rememberForTab;
    token = nextToken;
    setRuntimeConnectionState("connecting");
    const remember = el("runtime-token-remember");
    if (remember)
        remember.checked = rememberForTab;
    const request = beginRuntimeCredential(state);
    void fetchOverview(refreshRuntimeOverview(state));
    void fetchProjects(request, true);
    void refreshCommunication(workspaceView === "operations");
    if (workspaceView === "windows") {
        void refreshWindows(true);
        startWindowAuto();
    }
}
el("runtime-token-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const input = el("runtime-token-input");
    const remember = el("runtime-token-remember");
    const nextToken = input ? input.value.trim() : "";
    if (input)
        input.value = "";
    if (!nextToken) {
        setText("runtime-token-error", tr("Enter your access key."));
        return;
    }
    connectRuntimeCredential(nextToken, remember?.checked !== false);
});
el("runtime-agent-create-form")?.addEventListener("submit", (event) => void createCommunicationAgent(event));
el("runtime-window-copy-key")?.addEventListener("click", () => {
    const key = String(selectedWindowDetail?.client_window_key || selectedWindowKey || "");
    void copyRuntimeValue(key, "runtime-window-list-status");
});
el("runtime-agent-update-form")?.addEventListener("submit", (event) => void updateCommunicationAgent(event));
el("runtime-agent-attach")?.addEventListener("click", () => void attachCommunicationEndpoint());
el("runtime-agent-detach")?.addEventListener("click", () => void detachCommunicationEndpoint());
el("runtime-conversation-create-form")?.addEventListener("submit", (event) => void createCommunicationConversation(event));
el("runtime-conversation-message-form")?.addEventListener("submit", (event) => void postCommunicationMessage(event));
el("runtime-inbox-consume-all")?.addEventListener("click", () => {
    void consumeCommunicationDeliveries(communicationInbox.map((item) => String(item?.delivery_id || "")).filter(Boolean));
});
el("runtime-device-select")?.addEventListener("change", () => {
    const select = el("runtime-device-select");
    if (!select)
        return;
    applyRunnerFilter(select.value);
});
el("runtime-session-search")?.addEventListener("input", () => renderSessionList(sessionRows, sessionListMetaSnapshot));
el("runtime-message-search")?.addEventListener("input", filterCollaborationMessages);
el("runtime-project-search")?.addEventListener("input", () => {
    const input = el("runtime-project-search");
    projectSearch = input?.value || "";
    stopProjectSearchTimer();
    if (!token)
        return;
    setText("runtime-project-status", tr("Searching…"));
    projectSearchTimer = window.setTimeout(() => {
        projectSearchTimer = 0;
        if (!token)
            return;
        void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
    }, PROJECT_SEARCH_DEBOUNCE_MS);
});
el("runtime-message-kind")?.addEventListener("change", syncAckComposer);
el("runtime-message-priority")?.addEventListener("change", syncAckComposer);
el("runtime-message-requires-ack")?.addEventListener("change", syncComposerOptionSummary);
el("runtime-message-body")?.addEventListener("input", () => {
    setText("runtime-message-send-status", "");
    saveCurrentDraft();
    syncCollaborationComposerLayout();
});
el("runtime-message-body")?.addEventListener("keydown", (event) => {
    if (!(event instanceof KeyboardEvent) || event.key !== "Enter" || event.shiftKey || event.isComposing || event.keyCode === 229)
        return;
    const body = event.currentTarget;
    const send = el("runtime-message-send");
    const form = el("runtime-collaboration-form");
    event.preventDefault();
    if (!body?.value.trim() || send?.disabled || !form)
        return;
    form.requestSubmit();
});
el("runtime-message-reply-clear")?.addEventListener("click", () => setCollaborationReplyTarget(""));
el("runtime-message-edit-clear")?.addEventListener("click", cancelCollaborationEdit);
el("runtime-collaboration-form")?.addEventListener("submit", (event) => void postHumanCollaborationMessage(event));
el("runtime-chat-scroll")?.addEventListener("scroll", updateCollaborationFollowFromScroll, { passive: true });
el("runtime-new-messages")?.addEventListener("click", () => scrollCollaborationToLatest(true));
el("runtime-refresh")?.addEventListener("click", () => {
    closeTopbarMore(false);
    void refreshAll();
});
el("runtime-lock")?.addEventListener("click", () => lock());
el("runtime-mobile-nav-toggle")?.addEventListener("click", () => setMobileNavigationOpen(true));
el("runtime-find-project")?.addEventListener("click", focusProjectNavigation);
el("runtime-welcome-overview")?.addEventListener("click", () => revealOperationsSection("runtime-operations-overview"));
el("runtime-welcome-agents")?.addEventListener("click", () => revealOperationsSection("runtime-operations-agents"));
el("runtime-mobile-nav-close")?.addEventListener("click", () => setMobileNavigationOpen(false, true));
el("runtime-mobile-nav-backdrop")?.addEventListener("click", () => setMobileNavigationOpen(false, true));
el("runtime-inspector-backdrop")?.addEventListener("click", () => closeRuntimeInspector(true));
el("runtime-inspector-close")?.addEventListener("click", () => closeRuntimeInspector(true, true));
document.querySelector(".context-trigger")?.addEventListener("click", (event) => {
    event.preventDefault();
    const inspector = document.querySelector(".runtime-inspector");
    const currentlyVisible = !!inspector?.open;
    contextUserIntent = reduceRuntimeContextUserIntent(contextUserIntent, {
        type: "toggle_trigger",
        currentVisible: currentlyVisible,
    });
    if (contextUserIntent && mobileNavigationViewport()) {
        setMobileNavigationOpen(false, false);
    }
    syncContextUi(false);
});
installWorkspaceCommands({ language: () => runtimeLanguage, available: () => !!token, projects: () => effectiveProjects(projectRows), onProject: switchProject, onView: applyWorkspaceView });
document.querySelectorAll("[data-runtime-view]").forEach((button) => {
    button.addEventListener("click", () => applyWorkspaceView(parseWorkspaceViewPreference(button.dataset.runtimeView)));
});
document.querySelectorAll("[data-context-target]").forEach((button) => {
    button.addEventListener("click", () => {
        document.querySelectorAll("[data-context-target]").forEach((item) => {
            const selected = item === button;
            item.setAttribute("aria-pressed", String(selected));
            show(String(item.dataset.contextTarget), selected);
        });
    });
});
document.querySelectorAll("[data-operations-target]").forEach((button) => {
    button.addEventListener("click", () => revealOperationsSection(String(button.dataset.operationsTarget || "runtime-operations-overview")));
});
document.querySelectorAll("[data-language-toggle]").forEach((button) => {
    button.addEventListener("click", () => {
        applyLanguage(runtimeLanguage === "zh-CN" ? "en" : "zh-CN");
        closeAppearanceMenus(false);
        closeTopbarMore(false);
    });
});
document.querySelectorAll("[data-theme-option]").forEach((button) => {
    button.addEventListener("click", () => {
        applyAppearance(parseAppearancePreference(button.dataset.themeOption));
        const menu = button.closest("details.theme-menu");
        if (menu)
            menu.open = false;
        closeTopbarMore(false);
    });
});
el("runtime-topbar-more")?.addEventListener("toggle", (event) => {
    const menu = event.currentTarget;
    if (!menu?.open)
        return;
    closeComposerOptions(false);
    closeRuntimeInspector(false);
    setMobileNavigationOpen(false, false);
});
document.querySelectorAll("details.theme-menu").forEach((menu) => {
    menu.addEventListener("toggle", () => {
        if (!menu.open)
            return;
        closeAppearanceMenus(false, menu);
        closeRuntimeInspector(false);
        setMobileNavigationOpen(false, false);
    });
});
el("runtime-message-options")?.addEventListener("toggle", (event) => {
    const options = event.currentTarget;
    if (!options?.open)
        return;
    closeAppearanceMenus(false);
    setMobileNavigationOpen(false, false);
});
document.addEventListener("pointerdown", (event) => {
    const target = event.target;
    if (!(target instanceof Node))
        return;
    document.querySelectorAll("details.theme-menu[open]").forEach((menu) => {
        if (!menu.contains(target))
            menu.open = false;
    });
    const options = el("runtime-message-options");
    if (options?.open && !options.contains(target))
        options.open = false;
    const topbarMore = el("runtime-topbar-more");
    if (topbarMore?.open && !topbarMore.contains(target))
        topbarMore.open = false;
});
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
    const node = el("runtime-timeline");
    if (!node)
        return;
    updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight);
    syncFollowUi();
});
document.querySelector(".runtime-inspector")?.addEventListener("toggle", (event) => {
    const inspector = event.currentTarget;
    if (!inspector)
        return;
    const resolved = resolveRuntimeContextState({
        userIntent: contextUserIntent,
        isWideViewport: window.matchMedia(WIDE_CONTEXT_MEDIA).matches,
        isMobileViewport: mobileNavigationViewport(),
        hasSelectedSession: !!state.workflow?.selectedSessionId,
        workspaceView,
    });
    if (inspector.open !== resolved.visible) {
        inspector.open = resolved.visible;
    }
});
document.addEventListener("keydown", (event) => {
    const shell = el("runtime-console");
    if (document.querySelector("#runtime-command-dialog[open]"))
        return;
    if (!event.isComposing && !event.altKey && !event.shiftKey && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k" && shell && !shell.hidden) {
        event.preventDefault();
        focusProjectNavigation();
        return;
    }
    const inspector = document.querySelector(".runtime-inspector");
    if (event.key === "Escape") {
        if (closeComposerOptions(true)) {
            event.preventDefault();
            return;
        }
        if (closeAppearanceMenus(true)) {
            event.preventDefault();
            return;
        }
        if (closeTopbarMore(true)) {
            event.preventDefault();
            return;
        }
        if (inspector?.open && !shell?.classList.contains("context-docked")) {
            event.preventDefault();
            closeRuntimeInspector(true);
            return;
        }
        if (shell?.classList.contains("context-docked") && inspector?.open && inspector.contains(document.activeElement)) {
            event.preventDefault();
            closeRuntimeInspector(true, true);
            return;
        }
        if (shell?.classList.contains("mobile-nav-open")) {
            event.preventDefault();
            setMobileNavigationOpen(false, true);
        }
        return;
    }
    if (event.key !== "Tab" || !shell?.classList.contains("mobile-nav-open"))
        return;
    const sidebar = el("runtime-sidebar");
    if (!sidebar)
        return;
    const focusable = visibleFocusableElements(sidebar);
    if (!focusable.length)
        return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
    }
    else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
    }
});
window.addEventListener("resize", syncResponsiveNavigation, { passive: true });
document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
        stopWindowAuto();
        return;
    }
    if (!token)
        return;
    refreshAutoSurfaces();
    if (workspaceView === "windows") {
        void refreshWindows(true);
        startWindowAuto();
    }
});
const syncSystemAppearance = () => {
    if (parseAppearancePreference(document.documentElement.dataset.theme) === "system")
        applyAppearance("system", false);
};
if (typeof appearanceMedia.addEventListener === "function")
    appearanceMedia.addEventListener("change", syncSystemAppearance);
else
    appearanceMedia.addListener(syncSystemAppearance);
captureStaticUiSources();
applyLanguage(loadLanguagePreference(), false, false);
applyAppearance(readStoredAppearance(), false);
applyWorkspaceView(readStoredWorkspaceView(), false);
syncAckComposer();
window.addEventListener("pagehide", () => {
    saveCurrentDraft();
    detachCommunicationEndpointsBestEffort();
    token = "";
    abortAll();
    resetCommunicationSurface();
    stopAuto();
    stopWindowAuto();
});
lock("", false);
const rememberedRuntimeCredential = readStoredCredential();
if (rememberedRuntimeCredential) {
    setText("runtime-token-error", tr("Restoring this tab…"));
    connectRuntimeCredential(rememberedRuntimeCredential, true);
}
