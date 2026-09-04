class AdminHttpError extends Error {
    constructor(status, message) {
        super(message);
        this.name = "AdminHttpError";
        this.status = status;
    }
}
function isAbortError(error) {
    return error instanceof DOMException
        ? error.name === "AbortError"
        : Boolean(error && typeof error === "object" && "name" in error && error.name === "AbortError");
}
class AdminRefreshController {
    constructor(dependencies) {
        this.generation = 0;
        this.requestId = 0;
        this.token = "";
        this.active = null;
        this.timer = null;
        this.dependencies = dependencies;
    }
    beginSession(token) {
        this.invalidateRequests();
        this.token = token;
        this.dependencies.clearError();
        return this.refresh();
    }
    lock(message = "") {
        this.invalidateRequests();
        this.token = "";
        this.stopAutoRefresh();
        this.dependencies.showLocked(message);
    }
    refresh() {
        return this.refreshInternal();
    }
    invalidateAndRefresh() {
        if (!this.token)
            return Promise.resolve();
        this.invalidateRequests();
        return this.refreshInternal();
    }
    refreshInternal() {
        if (!this.token)
            return Promise.resolve();
        if (this.active &&
            this.active.generation === this.generation &&
            this.active.token === this.token) {
            return this.active.promise;
        }
        const generation = this.generation;
        const token = this.token;
        const id = ++this.requestId;
        const controller = new AbortController();
        this.dependencies.clearError();
        const promise = this.dependencies
            .request(token, controller.signal)
            .then((data) => {
            if (!this.isCurrent(generation, token, id))
                return;
            this.dependencies.render(data);
            this.dependencies.showAuthenticated();
            this.dependencies.setStatus(`Updated ${new Date().toLocaleTimeString()}`);
        })
            .catch((error) => {
            if (!this.isCurrent(generation, token, id) || isAbortError(error))
                return;
            if (error instanceof AdminHttpError && (error.status === 401 || error.status === 403)) {
                if (this.dependencies.onUnauthorized)
                    this.dependencies.onUnauthorized();
                else
                    this.lock("Administrator authentication required.");
                return;
            }
            this.dependencies.showError("Dashboard refresh failed");
            this.dependencies.setStatus("Refresh failed; showing last successful data.");
        })
            .finally(() => {
            if (this.active?.id === id)
                this.active = null;
        });
        this.active = { generation, id, token, controller, promise };
        return promise;
    }
    startAutoRefresh(milliseconds) {
        this.stopAutoRefresh();
        if (!this.token)
            return;
        const schedule = this.dependencies.setInterval || setInterval;
        this.timer = schedule(() => {
            void this.refresh();
        }, milliseconds);
    }
    stopAutoRefresh() {
        if (this.timer === null)
            return;
        const cancel = this.dependencies.clearInterval || clearInterval;
        cancel(this.timer);
        this.timer = null;
    }
    dispose() {
        this.invalidateRequests();
        this.stopAutoRefresh();
    }
    invalidateRequests() {
        this.generation += 1;
        this.active?.controller.abort();
        this.active = null;
    }
    isCurrent(generation, token, id) {
        return (this.generation === generation &&
            this.token === token &&
            this.active?.id === id);
    }
}

class AdminMutationError extends Error {
    constructor(status, code, activeJobs) {
        super(code);
        this.status = status;
        this.code = code;
        this.activeJobs = activeJobs;
        this.name = "AdminMutationError";
    }
}
function aborted(error) {
    return Boolean(error && typeof error === "object" && "name" in error && error.name === "AbortError");
}
class AdminMutationController {
    constructor(deps) {
        this.deps = deps;
        this.generation = 0;
        this.token = "";
        this.contexts = new Map();
    }
    beginSession(token) { this.invalidate(); this.token = token; }
    lock() { this.invalidate(); this.token = ""; }
    dispose() { this.invalidate(); this.token = ""; }
    start(kind, target, body) {
        const existing = this.contexts.get(target);
        if (!this.token || existing)
            return null;
        const context = {
            kind, target, body: { ...body }, key: this.deps.keyFactory(), generation: this.generation,
            token: this.token, controller: new AbortController(), pending: false,
        };
        this.contexts.set(target, context);
        return context;
    }
    retry(target) {
        const context = this.contexts.get(target);
        return context ? this.submit(context) : Promise.resolve();
    }
    cancel(target) {
        const context = this.contexts.get(target);
        if (!context?.pending)
            this.contexts.delete(target);
    }
    has(target) { return this.contexts.has(target); }
    isPending(target) { return this.contexts.get(target)?.pending === true; }
    async submit(context) {
        if (!this.current(context) || context.pending)
            return;
        context.pending = true;
        this.deps.pending(context.target, true);
        try {
            await this.deps.request(context.kind, context.token, { ...context.body, idempotency_key: context.key }, context.controller.signal);
            if (!this.current(context))
                return;
            this.deps.outcome(`${context.kind[0].toUpperCase()}${context.kind.slice(1)} completed.`);
            this.contexts.delete(context.target);
            await this.deps.refresh();
        }
        catch (error) {
            if (!this.current(context) || aborted(error))
                return;
            const classified = error instanceof AdminMutationError
                ? error
                : new AdminMutationError(0, "network_error");
            if (classified.code === "unauthorized") {
                this.deps.lock("Administrator authentication required.");
                return;
            }
            this.deps.error(classified.code, context);
            if (classified.code === "revision_conflict" || classified.code === "active_jobs_conflict") {
                await this.deps.refresh();
            }
            if (classified.code === "revision_conflict")
                this.contexts.delete(context.target);
        }
        finally {
            if (this.generation === context.generation && this.token === context.token) {
                context.pending = false;
                this.deps.pending(context.target, false);
            }
        }
    }
    current(context) {
        return this.generation === context.generation && this.token === context.token && this.contexts.get(context.target) === context;
    }
    invalidate() {
        this.generation += 1;
        for (const context of this.contexts.values())
            context.controller.abort();
        this.contexts.clear();
    }
}

class AdminMutationDialogCoordinator {
    constructor(mutation, adapter) {
        this.mutation = mutation;
        this.adapter = adapter;
        this.target = "";
        this.context = null;
        this.bodyFingerprint = "";
        this.cleaning = false;
    }
    open(target, context = null, body) {
        this.cleanup(false);
        this.target = target;
        this.context = context;
        this.bodyFingerprint = body ? JSON.stringify(body) : "";
    }
    async submit(kind, body) {
        const fingerprint = JSON.stringify(body);
        if (this.context && fingerprint === this.bodyFingerprint && this.mutation.has(this.target)) {
            await this.mutation.retry(this.target);
            return;
        }
        if (this.context || this.mutation.has(this.target))
            this.mutation.cancel(this.target);
        this.context = this.mutation.start(kind, this.target, body);
        this.bodyFingerprint = fingerprint;
        if (this.context)
            await this.mutation.submit(this.context);
    }
    setPendingContext(context, body) {
        this.context = context;
        this.bodyFingerprint = JSON.stringify(body);
    }
    cancel() { this.cleanup(true); }
    handleCancel(event) {
        event.preventDefault();
        if (this.target && this.mutation.isPending(this.target))
            return;
        this.cleanup(true);
    }
    handleClose() { this.cleanup(true); }
    closeForSessionEnd() { this.cleanup(true); }
    currentTarget() { return this.target; }
    cleanup(closeDialog) {
        if (this.cleaning || (!this.target && !this.context && !this.bodyFingerprint))
            return;
        this.cleaning = true;
        const target = this.target;
        this.target = "";
        this.context = null;
        this.bodyFingerprint = "";
        if (target)
            this.mutation.cancel(target);
        this.adapter.clearSensitive();
        if (closeDialog && this.adapter.isOpen())
            this.adapter.close();
        this.adapter.restoreFocus();
        this.cleaning = false;
    }
}

function record(value) { return value && typeof value === "object" && !Array.isArray(value) ? value : {}; }
function list(value) { return Array.isArray(value) ? value : []; }
function display(value) { if (value == null || value === "")
    return "—"; if (typeof value === "object") {
    try {
        return JSON.stringify(value);
    }
    catch {
        return "—";
    }
} return String(value); }
function capabilityLabels(value) { if (Array.isArray(value))
    return value.filter((item) => typeof item === "string"); if (value && typeof value === "object")
    return Object.entries(value).filter(([, enabled]) => enabled === true).map(([name]) => name).sort(); return []; }
function statusFor(data, section) { return record(record(data.section_status)[section]); }
function sectionOk(data, section) { return statusFor(data, section).status !== "error"; }
function sectionError(doc, section, data) { const node = doc.getElementById(`${section}-error`); if (!node)
    return; const status = statusFor(data, section); const failed = status.status === "error"; node.hidden = !failed; node.textContent = failed ? display(status.error || `${section} unavailable`) : ""; }
function clear(node) { while (node?.firstChild)
    node.removeChild(node.firstChild); }
function cell(doc, row, value, code = false) { const td = doc.createElement("td"); const child = doc.createElement(code ? "code" : "span"); child.textContent = display(value); td.appendChild(child); row.appendChild(td); }
function card(doc, label, value) { const box = doc.createElement("article"); box.className = "card"; const name = doc.createElement("span"); name.textContent = label; const content = doc.createElement("strong"); content.textContent = display(value); box.append(name, content); return box; }
function setVisible(doc, id, visible) { const node = doc.getElementById(id); if (node)
    node.hidden = !visible; }
function actionCell(doc, row, project) { const td = doc.createElement("td"); const group = doc.createElement("div"); group.className = "project-actions"; const actions = record(project.actions); for (const kind of ["enable", "disable", "unregister"]) {
    const button = doc.createElement("button");
    button.type = "button";
    button.textContent = kind[0].toUpperCase() + kind.slice(1);
    button.disabled = actions[kind] !== true;
    button.setAttribute("data-project-action", kind);
    button.setAttribute("data-project-id", String(project.id || ""));
    button.addEventListener("click", () => doc.dispatchEvent(new CustomEvent("admin-project-action", { detail: { kind, project: { ...project } } })));
    group.appendChild(button);
} td.appendChild(group); row.appendChild(td); }
function renderAdminDashboard(doc, raw) {
    const data = record(raw);
    sectionError(doc, "overview", data);
    if (sectionOk(data, "overview")) {
        const overview = doc.getElementById("overview");
        clear(overview);
        const value = record(data.overview);
        const cards = [["Server", `${display(value.version)} · ${display(value.build_commit)}`], ["Authority", value.authority_mode], ["Agents", `${display(value.agents_online || 0)} / ${display(value.agents_total || 0)} online`], ["Projects", `${display(value.projects_online || 0)} / ${display(value.projects_total || 0)} online`], ["Active jobs", value.active_jobs || 0], ["Compatibility", value.version_compatibility || "unknown"]];
        for (const [label, content] of cards)
            overview?.appendChild(card(doc, label, content));
        const diagnostics = doc.getElementById("diagnostics");
        clear(diagnostics);
        for (const [key, content] of Object.entries(record(data.diagnostics))) {
            const dt = doc.createElement("dt");
            dt.textContent = key.replace(/_/g, " ");
            const dd = doc.createElement("dd");
            dd.textContent = display(content);
            diagnostics?.append(dt, dd);
        }
    }
    sectionError(doc, "devices", data);
    if (sectionOk(data, "devices")) {
        const devices = doc.getElementById("devices");
        clear(devices);
        const rows = list(data.devices);
        for (const item of rows) {
            const device = record(item);
            const row = doc.createElement("tr");
            const values = [[device.display_name], [device.client_id, true], [device.status], [device.transport], [device.hostname], [device.last_seen], [capabilityLabels(device.capabilities).join(", ")], [device.project_count], [device.active_jobs], [device.compatibility]];
            for (const [value, code] of values)
                cell(doc, row, value, Boolean(code));
            devices?.appendChild(row);
        }
        setVisible(doc, "devices-empty", rows.length === 0);
    }
    sectionError(doc, "projects", data);
    if (sectionOk(data, "projects")) {
        const projects = doc.getElementById("projects");
        clear(projects);
        const rows = list(data.projects);
        for (const item of rows) {
            const project = record(item);
            const row = doc.createElement("tr");
            const values = [[project.id, true], [project.name], [project.client_id], [project.path], [project.lifecycle_status || project.readiness], [project.active_jobs], [project.git_available], [project.allow_patch], [project.shell_profile_status], [project.compatibility], [project.console_hint]];
            for (const [value, code] of values)
                cell(doc, row, value, Boolean(code));
            actionCell(doc, row, project);
            projects?.appendChild(row);
        }
        setVisible(doc, "projects-empty", rows.length === 0);
    }
    sectionError(doc, "activity", data);
    if (sectionOk(data, "activity")) {
        const activity = doc.getElementById("activity");
        clear(activity);
        const rows = list(data.activity);
        for (const item of rows) {
            const entry = record(item);
            const li = doc.createElement("li");
            li.textContent = [entry.created_at, entry.kind, entry.project_id, entry.status].filter(Boolean).map(String).join(" · ");
            activity?.appendChild(li);
        }
        setVisible(doc, "activity-empty", rows.length === 0);
    }
}

const ADMIN_BASE = "/api/admin/";
const REFRESH_MS = 10000;
const byId = (id) => document.getElementById(id);
const text = (id, value) => { const node = byId(id); if (node)
    node.textContent = value == null || value === "" ? "—" : String(value); };
const visible = (id, yes) => { const node = byId(id); if (node)
    node.hidden = !yes; };
let adminToken = "";
let dialogTrigger = null;
let mutation;
let dialogFlow;
async function parseResponse(response) { try {
    return await response.json();
}
catch {
    return null;
} }
async function requestDashboard(token, signal) {
    const response = await fetch(ADMIN_BASE + "dashboard", { method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: "{}", signal });
    const data = await parseResponse(response);
    if (!response.ok)
        throw new AdminHttpError(response.status, "dashboard_failed");
    return data;
}
function classify(status, data) {
    if (status === 401 || status === 403)
        return new AdminMutationError(status, "unauthorized");
    const code = data?.error?.code;
    const allowed = new Set(["invalid_request", "revision_conflict", "active_jobs_conflict", "idempotency_conflict", "unsupported_runner_version", "agent_unavailable", "operation_indeterminate", "operation_failed"]);
    return new AdminMutationError(status, allowed.has(code) ? code : "operation_failed", typeof data?.active_jobs === "number" ? data.active_jobs : undefined);
}
async function requestMutation(kind, token, body, signal) {
    const response = await fetch(`${ADMIN_BASE}projects/${kind}`, { method: "POST", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" }, body: JSON.stringify(body), signal });
    const data = await parseResponse(response);
    if (!response.ok)
        throw classify(response.status, data);
    return data;
}
function unifiedLock(message = "Locked.") {
    dialogFlow?.closeForSessionEnd();
    mutation?.lock();
    refresh.lock(message);
    adminToken = "";
}
const refresh = new AdminRefreshController({
    request: requestDashboard, render: (data) => renderAdminDashboard(document, data),
    showAuthenticated: () => { visible("gate", false); visible("dashboard", true); visible("controls", true); },
    showLocked: (message) => { visible("gate", true); visible("dashboard", false); visible("controls", false); text("gate-error", message); const input = byId("token"); if (input)
        input.value = ""; },
    setStatus: (message) => text("status", message), showError: (message) => { text("error", message); visible("error", true); }, clearError: () => visible("error", false),
    onUnauthorized: () => unifiedLock("Administrator authentication required."),
});
const errorMessage = {
    invalid_request: "The request is invalid. Review the fields and try again.", revision_conflict: "Project state changed. The dashboard was refreshed; confirm again using the latest revision.", active_jobs_conflict: "The project has active jobs. No jobs were stopped; refresh and retry after they finish.", idempotency_conflict: "This retry no longer matches its original operation. Start a new operation.", unsupported_runner_version: "The Agent does not support project lifecycle operations.", agent_unavailable: "The Agent is unavailable. Current dashboard data is preserved; retry this same operation later.", operation_indeterminate: "The Agent may have completed the operation. Refresh state first, then retry this same operation context rather than creating a new mutation.", operation_failed: "The operation failed safely. Internal details were not displayed.", network_error: "Network failure. Current data is preserved; retry this same operation.", unauthorized: "Administrator authentication required."
};
mutation = new AdminMutationController({
    request: requestMutation, keyFactory: () => crypto.randomUUID(), refresh: () => refresh.invalidateAndRefresh(),
    outcome: (message) => { text("status", message); dialogFlow.cancel(); },
    error: (code) => { text("dialog-error", errorMessage[code]); visible("dialog-error", true); },
    pending: (_target, value) => { const submit = byId("dialog-submit"); const cancel = byId("dialog-cancel"); if (submit) {
        submit.disabled = value;
        submit.setAttribute("aria-busy", String(value));
        submit.textContent = value ? "Working…" : "Continue";
    } if (cancel)
        cancel.disabled = value; },
    lock: (message) => unifiedLock(message),
});
dialogFlow = new AdminMutationDialogCoordinator(mutation, {
    close: () => byId("project-dialog")?.close(),
    isOpen: () => byId("project-dialog")?.open === true,
    clearSensitive: () => { const path = document.querySelector('input[name="path"]'); if (path)
        path.value = ""; },
    restoreFocus: () => { const trigger = dialogTrigger; dialogTrigger = null; trigger?.focus(); },
});
function configureAutoRefresh() { const auto = byId("auto"); if (auto?.checked)
    refresh.startAutoRefresh(REFRESH_MS);
else
    refresh.stopAutoRefresh(); }
function field(name, label, type = "text", checked = false) { const wrap = document.createElement("label"); wrap.textContent = label; const input = document.createElement("input"); input.name = name; input.type = type; input.autocomplete = "off"; if (type === "checkbox")
    input.checked = checked; input.required = !["description", "template"].includes(name) && type !== "checkbox"; wrap.appendChild(input); return wrap; }
function paragraph(value) { const p = document.createElement("p"); p.textContent = value; return p; }
function openCreate(kind, trigger) {
    const target = `${kind}:${crypto.randomUUID()}`;
    dialogFlow.open(target);
    dialogTrigger = trigger;
    text("dialog-title", kind === "register" ? "Register existing project" : "Create project");
    text("dialog-description", kind === "create" ? "Create may create a directory and Git repository. An existing empty directory is used only when explicitly adopted; non-empty directories are never overwritten, and no directory deletion is offered." : "Register an existing directory. The path remains only in this form and page memory.");
    const fields = byId("dialog-fields");
    fields.replaceChildren(field("client_id", "Client ID"), field("project_id", "Project ID"), field("name", "Name"), field("description", "Description"), field("path", "Path"), field("allow_patch", "Allow patch", "checkbox", true));
    if (kind === "create")
        fields.append(field("git_init", "Initialize Git repository", "checkbox"), field("template", "Template"), field("adopt_existing_empty", "Adopt existing empty directory", "checkbox"));
    visible("dialog-error", false);
    byId("project-form").dataset.kind = kind;
    byId("project-dialog").showModal();
    fields.querySelector("input")?.focus();
}
function openAction(kind, project, trigger) {
    const target = String(project.id || "");
    const body = { project: target, expected_revision: String(project.revision || ""), confirm: true };
    const context = mutation.start(kind, target, body);
    dialogFlow.open(target, context, body);
    dialogTrigger = trigger;
    text("dialog-title", `${kind[0].toUpperCase()}${kind.slice(1)} project`);
    const fields = byId("dialog-fields");
    fields.replaceChildren(paragraph(`Project: ${target}`), paragraph(`Revision: ${String(project.revision || "")}`), paragraph(`Active jobs: ${String(project.active_jobs ?? 0)}`));
    if (kind === "disable")
        fields.append(paragraph("Already-started jobs will not be stopped. Project configuration and source directory are retained."));
    if (kind === "enable")
        fields.append(paragraph("This only re-enables a registered project. It does not create a missing directory; the Agent revalidates path policy."));
    if (kind === "unregister") {
        fields.append(paragraph("Only the Agent registry record is removed. The source directory and .git are not deleted. Active jobs cause rejection."));
        fields.append(field("confirm_project", "Type the full runtime project ID to confirm"));
    }
    byId("project-form").dataset.kind = kind;
    visible("dialog-error", false);
    byId("project-dialog").showModal();
    fields.querySelector("input")?.focus();
}
byId("token-form")?.addEventListener("submit", async (event) => { event.preventDefault(); const input = byId("token"); const token = input?.value.trim() || ""; if (input)
    input.value = ""; if (!token)
    return; dialogFlow.closeForSessionEnd(); adminToken = token; mutation.beginSession(token); await refresh.beginSession(token); configureAutoRefresh(); });
byId("refresh")?.addEventListener("click", () => void refresh.refresh());
byId("lock")?.addEventListener("click", () => unifiedLock());
byId("auto")?.addEventListener("change", configureAutoRefresh);
byId("register-project")?.addEventListener("click", (e) => openCreate("register", e.currentTarget));
byId("create-project")?.addEventListener("click", (e) => openCreate("create", e.currentTarget));
document.addEventListener("admin-project-action", (event) => { const detail = event.detail; openAction(detail.kind, detail.project, document.querySelector(`[data-project-action="${detail.kind}"][data-project-id="${CSS.escape(String(detail.project.id))}"]`)); });
byId("dialog-cancel")?.addEventListener("click", () => dialogFlow.cancel());
byId("project-dialog")?.addEventListener("cancel", (event) => dialogFlow.handleCancel(event));
byId("project-dialog")?.addEventListener("close", () => dialogFlow.handleClose());
byId("project-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const form = event.currentTarget;
    const kind = form.dataset.kind;
    const data = new FormData(form);
    const target = dialogFlow.currentTarget();
    let body = { project: target, expected_revision: "", confirm: true };
    if (kind === "register" || kind === "create") {
        body = { client_id: String(data.get("client_id") || "").trim(), project_id: String(data.get("project_id") || "").trim(), name: String(data.get("name") || "").trim(), description: String(data.get("description") || "").trim() || null, path: String(data.get("path") || "").trim(), allow_patch: data.get("allow_patch") === "on" };
        if (kind === "create")
            Object.assign(body, { git_init: data.get("git_init") === "on", template: String(data.get("template") || "").trim() || null, adopt_existing_empty: data.get("adopt_existing_empty") === "on" });
    }
    else {
        const revisionText = byId("dialog-fields")?.children[1]?.textContent || "";
        body = { project: target, expected_revision: revisionText.replace(/^Revision:\s*/, ""), confirm: true };
    }
    if (kind === "unregister" && String(data.get("confirm_project") || "") !== target) {
        text("dialog-error", "Type the full runtime project ID to confirm.");
        visible("dialog-error", true);
        return;
    }
    void dialogFlow.submit(kind, body);
});
window.addEventListener("pagehide", () => { dialogFlow.closeForSessionEnd(); mutation.dispose(); refresh.dispose(); adminToken = ""; });
refresh.lock();
