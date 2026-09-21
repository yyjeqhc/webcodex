import { createProductProjectRow, productActivity, productTime, productTitle } from "./runtime_product_view.js";
import { runtimeIcon } from "./runtime_icons.js";
import { translate, localizedWorkflowText, type RuntimeLanguage } from "./runtime_i18n.js";
import { workflowSessionOverviewPresentation } from "./workflow_session_state.js";
import { activityDescription, formatLivenessPresentation, formatUpdatedTime } from "./runtime_activity.js";
import { pendingAttentionCount } from "./runtime_overview.js";
import { renderProjectWindowCards, formatWindowEmptyState } from "./runtime_window.js";

// These projections use only authorized Workflow Session evidence. Window
// observations never contribute to Session status, completion or authority.
export function workspaceSessionGroups(sessions: any[]): { attention: any[]; working: any[]; completed: any[]; recent: any[] } {
  const rows = [...sessions].sort((a, b) => Number(b.updated_at || 0) - Number(a.updated_at || 0));
  const attention = rows.filter(row => pendingAttentionCount(row.overview?.attention) > 0 || row.overview?.validation?.state === "failed");
  const working = rows.filter(row => row.running_call === true || Number(row.running_jobs) > 0);
  const completed = rows.filter(row => row.lifecycle === "closed");
  return { attention, working, completed, recent: rows };
}

export function workspaceSessionEvidence(detail: any): { files: string[]; jobs: any[]; completed: any[] } {
  const activity = Array.isArray(detail?.activity) ? detail.activity : [];
  // Paths from exploration are not changed files. This is retained edit evidence,
  // not a claim about the current Git working tree.
  const files = [...new Set<string>(activity.filter((row: any) => row.kind === "Edited")
    .flatMap((row: any) => Array.isArray(row.paths) ? row.paths.map(String) : []))];
  const jobs = activity.filter((row: any) => row.job_id);
  const completed = activity.filter((row: any) => typeof row.finished_at === "number" && row.state === "succeeded" && !row.job_handoff);
  return { files, jobs, completed: completed.slice(-5).reverse() };
}

function workspaceNode<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.textContent = text;
  node.className = className;
  return node;
}

function workspaceButton(text: string, action: string, callback: () => void): HTMLButtonElement {
  const button = workspaceNode("button", text, "workspace-action");
  button.type = "button";
  button.dataset.action = action;
  workspaceControlKeys.set(button, action);
  button.addEventListener("click", callback);
  return button;
}

export interface WorkspaceHomeOptions {
  language: RuntimeLanguage;
  projects: any[];
  project: any | null;
  sessions: any[];
  sessionsStatus: string;
  sessionsAvailable: boolean;
  windows: any[];
  windowAvailability: "idle" | "loading" | "available" | "stale" | "unavailable";
  windowScope: "global" | "principal";
  windowStatus: string;
  onProject: (runner: string, project: string) => void;
  onSession: (session: any) => void;
  onWindow: (key: string) => void;
  onWindows: () => void;
  onSearch: () => void;
  overview?: any;
  onAddProject?: () => void;
  git?: (project: string, node: HTMLElement) => void;
}

export function renderWorkspaceHome(node: HTMLElement | null, options: WorkspaceHomeOptions): void {
  if (!node) return;
  // Preserve focused controls on polling when the evidence has not changed.
  const signature = JSON.stringify([options.language, options.projects, options.project, options.sessions,
    options.sessionsAvailable, options.sessionsStatus, options.windows, options.windowAvailability, options.windowStatus, options.overview]);
  if (workspaceHomeSignatures.get(node) === signature) return;
  workspaceHomeSignatures.set(node, signature);
  const focusedKey = node.contains(document.activeElement) ? workspaceControlKeys.get(document.activeElement as HTMLElement) : null;
  node.replaceChildren();
  const tr = (text: string): string => translate(text, options.language);
  const heading = workspaceNode("header", "", "product-page-heading");
  const title = workspaceNode("div"); title.appendChild(workspaceNode("p", tr("Workspace"), "eyebrow"));
  title.appendChild(workspaceNode("h2", tr(options.overview ? "WebPi Ready" : "Workspace"))); heading.appendChild(title);
  heading.appendChild(workspaceButton(tr("Add Project"), "workspace-add-project", options.onAddProject || options.onSearch)); node.appendChild(heading);
  const status = workspaceNode("dl", "", "product-status-strip"); status.setAttribute("aria-label", tr("Workspace status"));
  const runners = Array.isArray(options.overview?.runners) ? options.overview.runners : [];
  for (const [label, value] of [["Server", tr(options.overview ? "Running" : "Not checked")], ["Runner", options.overview ? String(runners.filter((runner: any) => runner.connected).length) + " " + tr("online") : tr("Not checked")], ["Projects", String(options.projects.length)]]) {
    const item = workspaceNode("div"); item.appendChild(workspaceNode("dt", tr(label))); item.appendChild(workspaceNode("dd", value)); status.appendChild(item);
  }
  node.appendChild(status);
  const projects = workspaceNode("section", "", "product-section");
  const projectHeading = workspaceNode("header", "", "product-section-heading"); projectHeading.appendChild(workspaceNode("h3", tr("Recent Projects"))); projectHeading.appendChild(workspaceButton(tr("All Projects"), "workspace-find-project", options.onSearch)); projects.appendChild(projectHeading);
  const recentProjects = [...options.projects].sort((a, b) => Number(b.sessions?.latest_updated_at || 0) - Number(a.sessions?.latest_updated_at || 0));
  for (const project of recentProjects.slice(0, 4)) projects.appendChild(createProductProjectRow(project, { language: options.language, selected: options.project?.id, onOpen: options.onProject, git: options.git }));
  if (!recentProjects.length) projects.appendChild(workspaceNode("p", tr(options.overview ? "No projects yet" : "Loading projects…"), "muted"));
  node.appendChild(projects);
  const activity = workspaceNode("section", "", "product-section");
  const activityHeading = workspaceNode("header", "", "product-section-heading"); activityHeading.appendChild(workspaceNode("h3", tr("Recent Activity"))); activityHeading.appendChild(workspaceButton(tr("Windows"), "workspace-open-windows", options.onWindows)); activity.appendChild(activityHeading);
  if (!options.sessionsAvailable) activity.appendChild(workspaceNode("p", options.sessionsStatus || tr("Activity unavailable. Refresh to try again."), "muted"));
  const sessions = workspaceSessionGroups(options.sessions).recent.slice(0, 5);
  for (const session of sessions) {
    const button = workspaceButton("", "workspace-open-session", () => options.onSession(session)); workspaceControlKeys.set(button, "session:" + String(session.session_id)); button.className = "product-activity-row";
    button.appendChild(workspaceNode("span", tr("Workflow Sessions"), "product-badge")); button.appendChild(workspaceNode("strong", productTitle(session.title))); button.appendChild(workspaceNode("span", productTime(Number(session.updated_at) * 1000, options.language), "muted small")); activity.appendChild(button);
  }
  for (const window of options.windows.slice(0, 2)) {
    const button = workspaceButton("", "workspace-open-window", () => options.onWindow(String(window.client_window_key))); workspaceControlKeys.set(button, "window:" + String(window.client_window_key)); button.className = "product-activity-row";
    button.appendChild(workspaceNode("span", tr("Windows"), "product-badge")); button.appendChild(workspaceNode("strong", String(window.client_window_key).slice(-12))); button.appendChild(workspaceNode("span", productTime(window.last_meaningful_activity_at_ms || window.last_seen_at_ms, options.language), "muted small")); activity.appendChild(button);
  }
  if (options.sessionsAvailable && !sessions.length && !options.windows.length) activity.appendChild(workspaceNode("p", tr("No activity observed yet"), "muted"));
  node.appendChild(activity);
  if (focusedKey) Array.from(node.querySelectorAll<HTMLElement>("button")).find(button => workspaceControlKeys.get(button) === focusedKey)?.focus();
}

const workspaceControlKeys = new WeakMap<HTMLElement, string>();
const workspaceHomeSignatures = new WeakMap<HTMLElement, string>();

export function renderWorkspaceEvidence(node: HTMLElement | null, detail: any, language: RuntimeLanguage): void {
  if (!node) return;
  node.replaceChildren();
  const tr = (text: string): string => translate(text, language);
  const evidence = workspaceSessionEvidence(detail);
  const overview = workflowSessionOverviewPresentation(detail?.overview);
  const facts: [string, string][] = [
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

function workspaceListChip(parent: HTMLElement, text: string, extraClass = ""): HTMLElement {
  const chip = workspaceNode("span", text, "chip " + extraClass);
  parent.appendChild(chip);
  return chip;
}
export function renderWorkspaceSessionList(node: HTMLElement, visible: any[], selected: string, language: RuntimeLanguage, onSelect: (id: string) => void): void {
  const tr = (text: string): string => translate(text, language);
  for (const session of visible) {
    const id = String(session && session.session_id || "");
    if (!id) continue;
    const wrapper = document.createElement("li");
    const item = document.createElement("button");
    item.type = "button";
    item.dataset.action = "select-work-session";
    item.className = "session-card" + (id === selected ? " selected" : "");
    if (id === selected) item.setAttribute("aria-current", "true");
    const icon = document.createElement("span"); icon.className = "session-card-icon"; icon.setAttribute("aria-hidden", "true"); icon.appendChild(runtimeIcon("message"));
    const main = document.createElement("div"); main.className = "session-card-main";
    const title = document.createElement("div"); title.className = "session-title"; title.textContent = session.title ? String(session.title) : id;
    const meta = document.createElement("div"); meta.className = "chips session-meta";
    const lifecycle = String(session.lifecycle || "unknown");
    if (lifecycle !== "active") workspaceListChip(meta, tr(lifecycle));
    const liveness = formatLivenessPresentation(session, language);
    const livenessChip = workspaceListChip(meta, liveness.label, liveness.state === "working" ? "tone-runtime" : liveness.state === "attention" ? "tone-warn" : "");
    livenessChip.title = liveness.tooltip;
    workspaceListChip(meta, formatUpdatedTime(session.updated_at, language));
    main.appendChild(title); main.appendChild(meta);
    item.appendChild(icon); item.appendChild(main);
    const select = (): void => onSelect(id);
    item.addEventListener("click", select);
    wrapper.appendChild(item);
    node.appendChild(wrapper);
  }
}

export function installWorkspaceCommands(options: {
  language: () => RuntimeLanguage;
  available: () => boolean;
  projects: () => any[];
  onProject: (runner: string, project: string) => void;
  onView: (view: "home" | "projects" | "sessions" | "windows" | "activity" | "extensions" | "operations") => void;
}): void {
  const dialog = document.getElementById("runtime-command-dialog") as HTMLDialogElement | null;
  const input = document.getElementById("runtime-command-query") as HTMLInputElement | null;
  const results = document.getElementById("runtime-command-results");
  if (!dialog || !input || !results) return;
  let returnFocus: HTMLElement | null = null;
  const render = (): void => {
    const language = options.language();
    const query = input.value.trim().toLocaleLowerCase();
    results.replaceChildren();
    const entries: { label: string; action: string; run: () => void }[] = [
      ...([ ["Home", "home"], ["Projects", "projects"], ["Workflow Sessions", "sessions"], ["Windows", "windows"], ["Activity", "activity"], ["Extensions", "extensions"], ["Advanced", "operations"] ] as const)
        .map(([label, view]) => ({ label: translate(label, language), action: "command-" + view, run: () => options.onView(view) })),
      ...options.projects().map(project => ({ label: String(project.name || project.id) + " · " + String(project.client_id || ""), action: "command-project", run: () => options.onProject(String(project.client_id || ""), String(project.id || "")) })),
    ];
    for (const entry of entries.filter(entry => entry.label.toLocaleLowerCase().includes(query))) {
      results.appendChild(workspaceButton(entry.label, entry.action, () => { dialog.close(); if (options.available()) entry.run(); }));
    }
    if (!results.childElementCount) results.appendChild(workspaceNode("p", translate("No matching destinations.", language), "muted"));
  };
  const open = (): void => {
    if (!options.available() || dialog.open) return;
    returnFocus = document.activeElement as HTMLElement | null;
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
    if (event.isComposing) return;
    if (event.key === "ArrowDown") { event.preventDefault(); results.querySelector<HTMLButtonElement>("button")?.focus(); }
    if (event.key === "Enter") { event.preventDefault(); results.querySelector<HTMLButtonElement>("button")?.click(); }
  });
  document.addEventListener("keydown", event => {
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && !event.altKey && !event.isComposing && event.key.toLowerCase() === "k") {
      event.preventDefault(); open();
    }
  });
}

export function renderWorkspaceOverview(overview: any, language: RuntimeLanguage): void {
  const view = workflowSessionOverviewPresentation(overview);
  for (const [id, text] of [
    ["work", view.workText], ["attention", view.attentionText],
    ["validation", view.validationText + (typeof view.validationAt === "number" ? " · " + formatUpdatedTime(view.validationAt, language) : "")],
    ["progress", view.progressText + (typeof view.progressAt === "number" ? " · " + formatUpdatedTime(view.progressAt, language) : "")],
  ]) {
    const node = document.getElementById("runtime-overview-" + id);
    if (node) node.textContent = localizedWorkflowText(text, language);
  }
  for (const [id, tone] of [["validation", view.validationTone], ["attention", view.attentionTone]]) {
    const node = document.getElementById("runtime-overview-" + id + "-card");
    for (const name of ["pass", "warn", "fail", "muted"]) node?.classList.toggle("tone-card-" + name, tone === name);
  }
}
