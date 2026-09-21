import { translate, localizedCountLabel, } from "./runtime_i18n.js";
import { runtimeProjectIdentityText, filterAndSortRuntimeProjects, resolveRunnerDisclosure, } from "./runtime_console_state.js";
import { formatProjectIdentity, pendingAttentionCount, attentionLabel, runnerAttentionCount, } from "./runtime_overview.js";
import { formatUpdatedTime, formatLivenessPresentation, appendActivityPreview, } from "./runtime_activity.js";
import { runtimeIcon } from "./runtime_icons.js";
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
export function formatWorkspaceBreadcrumb(project, language) {
    const runnerText = project?.client_id
        ? String(project.client_id)
        : translate("Fleet", language);
    const projectText = project
        ? String(project.name || project.id || translate("Projects", language))
        : translate("Projects", language);
    return { runnerText, projectText };
}
export function formatSelectedProjectIdentity(project, language) {
    if (language !== "zh-CN") {
        return runtimeProjectIdentityText(project);
    }
    return formatProjectIdentity(project, language);
}
export function formatSessionWorkspaceIdentity(project, language) {
    return formatProjectIdentity(project, language);
}
export function formatDeviceStatusText(devicesCount, filter, language) {
    if (!devicesCount) {
        return language === "zh-CN" ? "没有已授权运行器" : "No authorized Runners";
    }
    const base = localizedCountLabel(devicesCount, "authorized Runner", "authorized Runners", language);
    if (filter) {
        return base + (language === "zh-CN" ? " · 已筛选" : " · filtered");
    }
    return base + " · " + translate("All Runners", language);
}
export function formatProjectStatusText(returnedProjects, totalProjects, truncated, filter, query, language) {
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
export function formatRunnerCountText(count, language) {
    return localizedCountLabel(count, "Runner", "Runners", language);
}
export function formatRecentSessionStatusText(meta, language) {
    if (!meta)
        return "";
    return localizedCountLabel(meta.returned, "Session", "Sessions", language)
        + (meta.truncated ? (language === "zh-CN" ? " · 前 " : " · top ") + String(meta.returned || 0) : "")
        + (meta.scan_truncated ? (language === "zh-CN" ? " · 扫描不完整" : " · partial scan") : "");
}
export function formatProjectWindowStatusText(returned, total, truncated, language) {
    if (!truncated)
        return "";
    return language === "zh-CN"
        ? String(returned) + " / " + String(total) + " 个窗口 · 有界"
        : String(returned) + " of " + String(total) + " Windows · bounded";
}
export function renderProjectSelectorTree(deviceSelect, projectList, sessionsPanel, options) {
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
export function renderRunnerFleetRows(node, runners, options) {
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
export function renderRecentSessionRows(node, sessions, options) {
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
