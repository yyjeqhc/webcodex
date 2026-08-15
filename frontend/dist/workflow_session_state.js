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
export function workflowSessionListOverviewFacts(overview) {
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
export function workflowSessionOverviewPresentation(overview) {
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
