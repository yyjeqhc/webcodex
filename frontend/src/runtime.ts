import { productNode, productName, productTime, productActivity } from "./runtime_product_view.js";
import { ProductWorkspace, type ProductServices } from "./runtime_product.js";
import { ProductExtensions } from "./runtime_extensions.js";
import { renderWorkspaceOverview, renderWorkspaceHome, renderWorkspaceEvidence, renderWorkspaceSessionList, installWorkspaceCommands } from "./runtime_workspace.js";
import {
  workflowSessionOverviewPresentation,
  workflowSessionLivenessPresentation,
  updateWorkflowSessionFollowFromScroll,
  workflowSessionScrollTopAfterRender,
  jumpWorkflowSessionToLatest,
  shouldFollowWorkflowSessionLatest,
} from "./workflow_session_state.js";
import {
  RuntimeCommunicationRefreshCoordinator,
  runtimeCommunicationTranscriptAfterSeq,
} from "./runtime_communication_state.js";
import {
  resolveRuntimeContextState,
  reduceRuntimeContextUserIntent,
  resolveRuntimeContextFocusTransition,
} from "./runtime_context_state.js";
import {
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
  adoptRuntimeCollaborationObservation,
  setRuntimeCollaborationAvailable,
  setRuntimeCollaborationPhase,
  runtimeCollaborationNeedsRefreshRecovery,
  runtimeCollaborationObservationAction,
  runtimeCollaborationMessageCanMutate,
  runtimeCollaborationMessageSides,
  setRuntimeCollaborationReplyTarget,
  setRuntimeCollaborationEditTarget,
  clearRuntimeCollaborationEditTarget,
  runtimeCollaborationEditTarget,
  markRuntimeCollaborationMutationUncertain,
  runtimeCollaborationMutationRecovery,
  completeRuntimeCollaborationMutationRecovery,
  takeRuntimeCollaborationMutationNotice,
} from "./runtime_collaboration_state.js";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  filterAndSortRuntimeProjects,
  runtimeProjectIdentityText,
  preferredRuntimeProjectSelection,
  runtimeWorkflowSessionSummaryChanged,
  invalidateRuntimeCredential,
  beginRuntimeCredential,
  refreshRuntimeOverview,
  isCurrentRuntimeOverviewRequest,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  selectRuntimeRunnerFilter,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  refreshRuntimeProjectWindows,
  isCurrentRuntimeProjectWindowsRequest,
  selectRuntimeWorkflowSession,
  selectRuntimeSessionLocation,
  refreshRuntimeWorkflowSession,
  clearRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  resolveRunnerDisclosure,
  runtimeWindowShortKey,
  runtimeWindowActivityLabel,
  runtimeWindowAvailabilityAfterHttpResponse,
} from "./runtime_console_state.js";
import {
  formatWorkspaceBreadcrumb,
  formatSelectedProjectIdentity,
  formatSessionWorkspaceIdentity,
  formatDeviceStatusText,
  formatProjectStatusText,
  formatRunnerCountText,
  formatRecentSessionStatusText,
  formatProjectWindowStatusText,
  renderProjectSelectorTree,
  renderRunnerFleetRows,
  renderRecentSessionRows,
} from "./runtime_navigation.js";
import {
  collaborationPhaseLabel,
  syncCollaborationComposerLayout,
  formatComposerOptionSummary,
  runtimeSearchMatches,
  filterCollaborationCards,
  renderLatestAgentMessage,
  renderCollaborationMessageCards,
} from "./runtime_collaboration.js";

import {
  type RuntimeLanguage,
  LANGUAGE_STORAGE_KEY,
  RUNTIME_ZH_TEXT,
  ZH_COUNT_LABELS,
  languagePreference,
  loadLanguagePreference,
  translate,
  translateStaticNodeValue,
  localizedCountLabel,
  localizedWorkflowText,
} from "./runtime_i18n.js";
import {
  appendLinkifiedText,
  messageLineStartsBlock,
  appendMessageParagraph,
  appendMessageCode,
  appendRichMessage,
} from "./runtime_rich_text.js";
import {
  type RuntimeApiResponse,
  RUNTIME_API_BASE,
  RuntimeApiClient,
  abortController,
  writeClipboardText,
} from "./runtime_api.js";
import {
  runtimeProjectClientId,
  renderWindowActivityRows,
  createWindowCard,
  renderWindowActiveRequests,
  renderWindowLinkedSessions,
  renderSessionWindowCorrelationLinks,
  formatWindowDetailFields,
  renderWindowCards,
  renderProjectWindowCards,
  formatWindowEmptyState,
  formatWindowListStatusText,
} from "./runtime_window.js";
import {
  parseAgentIds,
  deliveryAgentLabel,
  renderAgentRows,
  renderConversationRows,
  renderConversationMessages,
  renderInboxDeliveryCards,
} from "./runtime_communication.js";
import {
  formatUpdatedTime,
  formatSessionDateTime,
  formatLivenessPresentation,
  appendActivityPreview,
  renderTimelineEvents,
} from "./runtime_activity.js";
import {
  RUNTIME_CREDENTIAL_SESSION_KEY,
  APPEARANCE_STORAGE_KEY,
  WORKSPACE_VIEW_STORAGE_KEY,
  DRAFT_STORAGE_PREFIX,
  DEVICE_DISCLOSURE_STORAGE_PREFIX,
  APPEARANCE_MEDIA_QUERY,
  type AppearancePreference,
  type RuntimeWorkspaceView,
  appearancePreference,
  loadAppearancePreference,
  persistAppearancePreference,
  resolvedAppearance,
  workspaceViewPreference,
  loadWorkspaceViewPreference,
  persistWorkspaceViewPreference,
  loadRememberedRuntimeCredential,
  persistRuntimeCredentialForTab,
  clearRememberedRuntimeCredential,
  loadDraft,
  saveDraft,
  clearDraft,
  clearRuntimeDrafts,
  storedDeviceDisclosure,
  persistDeviceDisclosure,
} from "./runtime_storage.js";
import {
  runnerAttentionCount,
  formatProjectIdentity,
  extractProjectSelectorDevices,
  formatProjectLabel,
  mergeEffectiveProjects,
  formatRuntimeOverviewMetrics,
} from "./runtime_overview.js";
import {
  runtimeIcon,
  createMessageAction,
  type RuntimeIconName,
} from "./runtime_icons.js";
import {
  operationKey,
  idempotencyKeyFor,
  formatCommunicationAvailability,
  formatAgentCardRevision,
  formatAgentWakeStatus,
  formatAgentEndpointStatus,
  formatConversationSeq,
  validateAgentCreateInputs,
  validateAgentUpdateInputs,
  validateConversationCreateInputs,
} from "./runtime_operations.js";

const API_BASE = RUNTIME_API_BASE;
const apiClient = new RuntimeApiClient(API_BASE);
const REFRESH_MS = 30000;
const WINDOW_REFRESH_MS = 3000;
const COLLABORATION_WAIT_SECS = 25;
const PROJECT_SEARCH_DEBOUNCE_MS = 200;
const MOBILE_NAVIGATION_MEDIA = "(max-width: 900px)";
const WIDE_CONTEXT_MEDIA = "(min-width: 1600px)";

let contextUserIntent: boolean | null = null;

type StaticTextSource = { node: Text; source: string };
type StaticAttributeSource = { node: Element; name: string; source: string };

// Verified localization mapping: "Close session context": "关闭会话上下文"

const appearanceMedia = window.matchMedia(APPEARANCE_MEDIA_QUERY);
let runtimeLanguage: RuntimeLanguage = languagePreference(document.documentElement.dataset.language);
const staticTextSources: StaticTextSource[] = [];
const staticAttributeSources: StaticAttributeSource[] = [];

let token = "";
let rememberCredentialForTab = true;
let timer = 0;
let overviewAbort: AbortController | null = null;
let projectsAbort: AbortController | null = null;
let sessionsAbort: AbortController | null = null;
let detailAbort: AbortController | null = null;
let collaborationAbort: AbortController | null = null;
let windowsAbort: AbortController | null = null;
let windowDetailAbort: AbortController | null = null;
let windowTimer = 0;
let windowRows: any[] = [];
let windowAvailability: "idle" | "loading" | "available" | "stale" | "unavailable" = "idle";
let windowVisibilityScope: "global" | "principal" = "principal";
let selectedWindowKey = "";
let selectedWindowDetail: any | null = null;
const PROJECT_WINDOW_LIMIT = 10;
let projectWindowsAbort: AbortController | null = null;
let projectWindowRows: any[] = [];
let projectWindowAvailability: "idle" | "loading" | "available" | "stale" | "unavailable" = "idle";
let projectWindowTruncated = false;
let projectWindowTotal = 0;
let projectWindowProjectId = "";
let renderedProjectWindowSignature = "";
let projectRows: any[] = [];
let homeProjectRows: any[] = [];
let runnerRows: any[] = [];
let recentSessionRows: any[] = [];
let runtimeOverviewSnapshot: any | null = null;
let recentSessionMetaSnapshot: any | null = null;
let projectSearch = "";
let projectDeviceFilter = "";
let projectSearchTimer = 0;
let collaborationReplyTo = "";
let renderedCollaborationMessageIds = new Set<string>();
let locallyAuthoredCollaborationMessageIds = new Set<string>();
let workspaceView: RuntimeWorkspaceView = "home";
let collaborationFollowLatest = true;
let collaborationPendingMessages = 0;
let refreshInFlight = false;
let projectRowsTotal = 0;
let projectRowsTruncated = false;
let knownProjectDevices: string[] = [];
let selectedProjectSnapshot: any | null = null;
let sessionRows: any[] = [];
let sessionAvailability = "idle";
let sessionListMetaSnapshot = { total: 0, truncated: false };
const state = initialRuntimeConsoleState();
let renderedProjectSelectorsSignature = "";
let renderedRunnerFleetSignature = "";
let renderedRecentSessionsSignature = "";
let renderedSessionListSignature = "";
let renderedCollaborationSignature = "";
let renderedCommunicationSurfaceSignature = "";

type RuntimeCommunicationEndpoint = {
  endpoint_id: string;
  agent_id: string;
  wake_capable: boolean;
  controller_generation: number;
  lifecycle: "attached" | "detached" | "expired" | string;
  attached_at_unix_ms: number;
  last_seen_at_unix_ms: number;
  lease_expires_at_unix_ms: number;
  expired_at_unix_ms: number | null;
  detached_at_unix_ms: number | null;
};

let communicationAgents: any[] = [];
let communicationConversations: any[] = [];
let communicationDetail: any | null = null;
let communicationInbox: any[] = [];
let selectedCommunicationAgentId = "";
let selectedCommunicationConversationId = "";
let communicationReadAvailable: boolean | null = null;
let communicationManageAvailable: boolean | null = null;
let communicationGeneration = 0;
const communicationRefreshCoordinator = new RuntimeCommunicationRefreshCoordinator(performCommunicationRefresh);
const communicationEndpoints = new Map<string, RuntimeCommunicationEndpoint>();
const pendingEndpointAttach = new Map<string, { key: string; attachmentId: string }>();
let pendingAgentCreate: { fingerprint: string; key: string } | null = null;
let pendingConversationCreate: { fingerprint: string; key: string } | null = null;
let pendingConversationMessage: { fingerprint: string; key: string } | null = null;
const pageAttachmentId = "runtime-console-" + operationKey("page");

const productProjectApi = new RuntimeApiClient("/api/projects/");
const productServices: ProductServices = {
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


function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function setText(id: string, value: unknown): void {
  const node = el(id);
  if (node) node.textContent = value === null || value === undefined ? "—" : String(value);
}

function show(id: string, visible: boolean): void {
  const node = el(id);
  if (node) node.hidden = !visible;
}

function tr(source: string): string {
  return translate(source, runtimeLanguage);
}

function translatedStaticNodeValue(source: string): string {
  return translateStaticNodeValue(source, runtimeLanguage);
}

function captureStaticUiSources(): void {
  if (staticTextSources.length || staticAttributeSources.length) return;
  const root = document.body;
  if (!root) return;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let node = walker.nextNode();
  while (node) {
    const textNode = node as Text;
    const source = textNode.nodeValue || "";
    if (source.trim()) staticTextSources.push({ node: textNode, source });
    node = walker.nextNode();
  }
  for (const element of Array.from(root.querySelectorAll<Element>("*"))) {
    for (const name of ["placeholder", "title", "aria-label"]) {
      const source = element.getAttribute(name);
      if (source) staticAttributeSources.push({ node: element, name, source });
    }
  }
}

function renderLanguageSensitiveUi(): void {
  renderRuntimeOverviewMetrics(runtimeOverviewSnapshot);
  if (projectRows.length || knownProjectDevices.length || runnerRows.length) {
    renderProjectSelectors(projectRows, projectRowsTruncated);
  } else {
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
  if (snapshot) renderDetail(snapshot, false);
  else if (!state.workflow?.selectedSessionId) hideDetail();
  if (!snapshot) renderCollaboration(undefined, false);
  renderCommunicationSurface();
  syncCollaborationComposer();
  renderWorkspaceHeading();
  renderHome();
  setRuntimeConnectionState(token ? "connected" : "disconnected");
}

function applyLanguage(language: RuntimeLanguage, persist = true, rerender = true): void {
  runtimeLanguage = languagePreference(language);
  document.documentElement.lang = runtimeLanguage;
  document.documentElement.dataset.language = runtimeLanguage;
  document.title = tr("WebPi — Workspace");
  for (const source of staticTextSources) source.node.nodeValue = translatedStaticNodeValue(source.source);
  for (const source of staticAttributeSources) source.node.setAttribute(source.name, tr(source.source));
  const nextLanguageLabel = runtimeLanguage === "zh-CN" ? "EN" : "中";
  const nextLanguageTitle = runtimeLanguage === "zh-CN" ? "切换到英文" : "Switch to Chinese";
  document.querySelectorAll<HTMLElement>("[data-language-toggle-label]").forEach((label) => {
    label.textContent = nextLanguageLabel;
  });
  document.querySelectorAll<HTMLElement>("[data-language-toggle]").forEach((button) => {
    button.title = nextLanguageTitle;
    button.setAttribute("aria-label", nextLanguageTitle);
  });
  applyAppearance(parseAppearancePreference(document.documentElement.dataset.theme), false);
  if (persist) {
    try { window.localStorage.setItem(LANGUAGE_STORAGE_KEY, runtimeLanguage); }
    catch { /* Language remains active when storage is unavailable. */ }
  }
  if (rerender) renderLanguageSensitiveUi();
}

function parseAppearancePreference(value: unknown): AppearancePreference {
  return appearancePreference(value);
}

function readStoredAppearance(): AppearancePreference {
  return loadAppearancePreference();
}

function computeResolvedAppearance(preference: AppearancePreference): "light" | "dark" {
  return resolvedAppearance(preference, appearanceMedia.matches);
}

function applyAppearance(preference: AppearancePreference, persist = true): void {
  const resolved = computeResolvedAppearance(preference);
  document.documentElement.dataset.theme = preference;
  document.documentElement.dataset.resolvedTheme = resolved;
  document.querySelector('meta[name="theme-color"]')?.setAttribute(
    "content",
    resolved === "light" ? "#f4f4f1" : "#090a0d"
  );
  document.querySelectorAll<HTMLButtonElement>("[data-theme-option]").forEach((button) => {
    button.setAttribute("aria-pressed", button.dataset.themeOption === preference ? "true" : "false");
  });
  const label = preference === "system" ? tr("System appearance") : preference === "light" ? tr("Light appearance") : tr("Dark appearance");
  document.querySelectorAll<HTMLElement>(".theme-trigger").forEach((trigger) => {
    trigger.title = label;
    trigger.setAttribute("aria-label", runtimeLanguage === "zh-CN" ? label + "。" + tr("Choose appearance") : label + ". " + tr("Choose appearance"));
  });
  if (persist) persistAppearancePreference(preference);
}

function parseWorkspaceViewPreference(value: unknown): RuntimeWorkspaceView {
  return workspaceViewPreference(value);
}

function readStoredWorkspaceView(): RuntimeWorkspaceView {
  return loadWorkspaceViewPreference();
}

function renderHome(): void {
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
  if (workspaceView === "projects") productWorkspace.renderProjects();
  if (workspaceView === "activity") productWorkspace.renderActivity();
  if (workspaceView === "extensions") productExtensions.open();
}

function renderWorkspaceHeading(): void {
  if (workspaceView === "operations") {
    setText("runtime-breadcrumb-runner", tr("Runtime workspace"));
    setText("runtime-breadcrumb-project", tr("Local control plane"));
    setText("runtime-session-title", tr("Runtime & Agents"));
    return;
  }
  if (workspaceView === "windows") {
    setText("runtime-breadcrumb-runner", tr("Runtime workspace"));
    setText("runtime-breadcrumb-project", tr("Window Activity"));
    setText(
      "runtime-session-title",
      selectedWindowKey ? "Window " + runtimeWindowShortKey(selectedWindowKey) : tr("Window activity"),
    );
    return;
  }
  renderWorkspaceBreadcrumb();
  const productHeading = { home: "Home", projects: "Projects", activity: "Activity", extensions: "Extensions" }[workspaceView as "home" | "projects" | "activity" | "extensions"];
  if (productHeading) { setText("runtime-session-title", tr(productHeading)); return; }
  const snapshot = state.workflow?.snapshot;
  setText("runtime-session-title", snapshot?.title ? String(snapshot.title) : tr("Select a Session"));
}

function applyWorkspaceView(view: RuntimeWorkspaceView, persist = true): void {
  workspaceView = parseWorkspaceViewPreference(view);
  const operations = workspaceView === "operations";
  const windows = workspaceView === "windows";
  const sessions = workspaceView === "sessions";
  const shell = el("runtime-console");
  if (shell) shell.dataset.workspaceView = workspaceView;
  document.body.classList.toggle("runtime-operations-view", operations);
  document.body.classList.toggle("runtime-windows-view", windows);
  show("runtime-navigation-sessions", sessions);
  for (const view of ["projects", "activity", "extensions"]) show(`runtime-${view}-stage`, workspaceView === view);
  show("runtime-home-stage", workspaceView === "home");
  renderHome();
  show("runtime-navigation-operations", operations);
  show("runtime-navigation-windows", windows);
  show("runtime-conversation-stage", sessions);
  show("runtime-operations-stage", operations);
  show("runtime-windows-stage", windows);
  document.querySelectorAll<HTMLButtonElement>("[data-runtime-view]").forEach((button) => {
    const selected = button.dataset.runtimeView === workspaceView;
    button.classList.toggle("selected", selected);
    if (selected) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  if (operations && token) void refreshCommunication(true);
  if (windows && token) {
    void refreshWindows(true);
    startWindowAuto();
  } else {
    stopWindowAuto();
  }
  renderWorkspaceHeading();
  syncResponsiveNavigation();
  setMobileNavigationOpen(false, false);
  if (persist) persistWorkspaceViewPreference(workspaceView);
}

function revealOperationsSection(targetId: string): void {
  applyWorkspaceView("operations");
  const buttons = document.querySelectorAll<HTMLButtonElement>("[data-operations-target]");
  if (!Array.from(buttons).some((button) => button.dataset.operationsTarget === targetId)) return;
  buttons.forEach((button) => {
    const selected = button.dataset.operationsTarget === targetId;
    button.classList.toggle("selected", selected);
    if (selected) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
    show(String(button.dataset.operationsTarget), selected);
  });
  const scroll = document.querySelector(".operations-scroll");
  if (scroll) scroll.scrollTop = 0;
  el(targetId)?.focus({ preventScroll: true });
}

type RuntimeConnectionState = "connected" | "connecting" | "stale" | "disconnected";

function setRuntimeConnectionState(connection: RuntimeConnectionState): void {
  const label = connection === "connected"
    ? tr("Connected")
    : connection === "connecting"
      ? tr("Reconnecting")
      : connection === "stale"
        ? tr("STALE")
        : tr("offline");
  document.querySelectorAll<HTMLElement>(".sidebar-runtime-status, .sidebar-connection, .operations-live").forEach((node) => {
    node.dataset.connection = connection;
  });
  const connectionLabel = document.querySelector<HTMLElement>(".sidebar-connection > span");
  if (connectionLabel) connectionLabel.textContent = label;
  setText("runtime-navigation-health", label);
}

function closeAppearanceMenus(restoreFocus = false, except: HTMLDetailsElement | null = null): boolean {
  let closed = false;
  document.querySelectorAll<HTMLDetailsElement>("details.theme-menu[open]").forEach((menu) => {
    if (menu === except) return;
    menu.open = false;
    closed = true;
    if (restoreFocus) (menu.querySelector("summary") as HTMLElement | null)?.focus();
  });
  return closed;
}

function closeTopbarMore(restoreFocus = false): boolean {
  const menu = el("runtime-topbar-more") as HTMLDetailsElement | null;
  if (!menu?.open) return false;
  menu.open = false;
  if (restoreFocus) (menu.querySelector(":scope > summary") as HTMLElement | null)?.focus();
  return true;
}

function closeComposerOptions(restoreFocus = false): boolean {
  const options = el("runtime-message-options") as HTMLDetailsElement | null;
  if (!options?.open) return false;
  options.open = false;
  if (restoreFocus) (options.querySelector("summary") as HTMLElement | null)?.focus();
  return true;
}

function mobileNavigationViewport(): boolean {
  return window.matchMedia(MOBILE_NAVIGATION_MEDIA).matches;
}

function isContextDocked(): boolean {
  return !!el("runtime-console")?.classList.contains("context-docked");
}

function syncContextUi(restoreFocus = false): void {
  const shell = el("runtime-console");
  const inspector = document.querySelector(".runtime-inspector") as HTMLDetailsElement | null;
  const trigger = inspector?.querySelector(".context-trigger") as HTMLElement | null;
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
  } else if (restoreFocus && !resolved.visible && trigger) {
    trigger.focus();
  }
}

function closeRuntimeInspector(restoreFocus = false, forceDocked = false): boolean {
  if (isContextDocked() && !forceDocked) return false;
  const inspector = document.querySelector(".runtime-inspector") as HTMLDetailsElement | null;
  if (!inspector?.open && !isContextDocked()) return false;
  contextUserIntent = reduceRuntimeContextUserIntent(contextUserIntent, { type: "explicit_close" });
  syncContextUi(restoreFocus);
  return true;
}

function setMobileNavigationOpen(open: boolean, restoreFocus = false, focusTarget = "runtime-mobile-nav-close"): void {
  const shell = el("runtime-console");
  const sidebar = el("runtime-sidebar");
  const toggle = el("runtime-mobile-nav-toggle") as HTMLButtonElement | null;
  const mobile = mobileNavigationViewport();
  const nextOpen = mobile && open;
  shell?.classList.toggle("mobile-nav-open", nextOpen);
  toggle?.setAttribute("aria-expanded", nextOpen ? "true" : "false");
  if (sidebar) {
    if (mobile) sidebar.setAttribute("aria-hidden", nextOpen ? "false" : "true");
    else sidebar.removeAttribute("aria-hidden");
  }
  if (nextOpen) {
    closeAppearanceMenus(false);
    closeTopbarMore(false);
    closeRuntimeInspector(false);
    window.setTimeout(() => {
      if (mobileNavigationViewport() && shell?.classList.contains("mobile-nav-open")) el(focusTarget)?.focus();
    }, 260);
  } else if (restoreFocus && mobile) {
    window.setTimeout(() => toggle?.focus(), 0);
  }
}

function focusProjectNavigation(): void {
  applyWorkspaceView("sessions");
  if (mobileNavigationViewport()) setMobileNavigationOpen(true, false, "runtime-project-search");
  else el("runtime-project-search")?.focus();
}

function syncResponsiveNavigation(): void {
  const shell = el("runtime-console");
  const sidebar = el("runtime-sidebar");
  const toggle = el("runtime-mobile-nav-toggle") as HTMLButtonElement | null;
  syncContextUi();
  if (!mobileNavigationViewport()) {
    shell?.classList.remove("mobile-nav-open");
    sidebar?.removeAttribute("aria-hidden");
    toggle?.setAttribute("aria-expanded", "false");
    return;
  }
  const open = !!shell?.classList.contains("mobile-nav-open");
  sidebar?.setAttribute("aria-hidden", open ? "false" : "true");
  if (!open && sidebar?.contains(document.activeElement)) toggle?.focus();
}

function visibleFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(
    'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [href], [tabindex]:not([tabindex="-1"])'
  )).filter((node) => node.offsetParent !== null);
}

function clearNode(node: any): void {
  while (node && node.firstChild) node.removeChild(node.firstChild);
}

function renderFingerprint(value: unknown): string {
  try { return JSON.stringify(value) || ""; }
  catch { return ""; }
}

function syncNewMessageIndicator(): void {
  const visible = collaborationPendingMessages > 0 && !collaborationFollowLatest;
  show("runtime-new-messages", visible);
  if (!visible) return;
  const label = runtimeLanguage === "zh-CN"
    ? String(collaborationPendingMessages) + " 条新消息"
    : String(collaborationPendingMessages) + " " + (collaborationPendingMessages === 1 ? "new message" : "new messages");
  setText("runtime-new-messages-label", label);
}

function chatIsNearLatest(): boolean {
  const scroll = el("runtime-chat-scroll");
  if (!scroll) return true;
  return scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight <= 96;
}

function updateCollaborationFollowFromScroll(): void {
  collaborationFollowLatest = chatIsNearLatest();
  if (collaborationFollowLatest) collaborationPendingMessages = 0;
  syncNewMessageIndicator();
}

function announceNewCollaborationMessages(count: number): void {
  if (count <= 0) return;
  const label = runtimeLanguage === "zh-CN"
    ? String(count) + " 条新消息"
    : String(count) + " " + (count === 1 ? "new message" : "new messages");
  setText("runtime-message-announcer", label);
}

function readStoredCredential(): string {
  return loadRememberedRuntimeCredential();
}

function writeTabCredential(): void {
  persistRuntimeCredentialForTab(token, rememberCredentialForTab);
}

function eraseStoredCredential(): void {
  clearRememberedRuntimeCredential();
}

function saveCurrentDraft(): void {
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (!body) return;
  saveDraft(state.selectedProject, state.workflow?.selectedSessionId, body.value);
}

function restoreCurrentDraft(): void {
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (!body) return;
  body.value = loadDraft(state.selectedProject, state.workflow?.selectedSessionId);
  syncCollaborationComposerLayout();
}

function clearCurrentDraft(): void {
  clearDraft(state.selectedProject, state.workflow?.selectedSessionId);
}

function eraseAllDrafts(): void {
  clearRuntimeDrafts();
}

function rememberLocalCollaborationMessage(messageId: unknown): void {
  const id = typeof messageId === "string" ? messageId : "";
  if (!/^wc_msg_[A-Za-z0-9_]+$/.test(id)) return;
  locallyAuthoredCollaborationMessageIds.add(id);
}

function readDeviceDisclosure(clientId: string): boolean | null {
  return storedDeviceDisclosure(clientId);
}

function writeDeviceDisclosure(clientId: string, open: boolean): void {
  persistDeviceDisclosure(clientId, open);
}

function revealRunner(clientId: string): void {
  if (!clientId) return;
  writeDeviceDisclosure(clientId, true);
  const group = document.querySelector(`.device-group[data-runner-id="${CSS.escape(clientId)}"]`) as HTMLDetailsElement | null;
  if (group) group.open = true;
}

function appendChip(parent: HTMLElement, text: string, extraClass = ""): HTMLElement {
  const chip = document.createElement("span");
  chip.className = "chip" + (extraClass ? " " + extraClass : "");
  chip.textContent = text;
  parent.appendChild(chip);
  return chip;
}

function abort(controller: AbortController | null): void {
  abortController(controller);
}

function abortCollaboration(): void {
  abort(collaborationAbort);
  collaborationAbort = null;
}

function abortProjectWork(): void {
  abort(sessionsAbort);
  abort(detailAbort);
  abortCollaboration();
  abort(projectWindowsAbort);
  sessionsAbort = null;
  detailAbort = null;
  projectWindowsAbort = null;
}

function stopProjectSearchTimer(): void {
  if (projectSearchTimer) window.clearTimeout(projectSearchTimer);
  projectSearchTimer = 0;
}

function abortAll(): void {
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

async function api(path: string, payload: any, signal?: AbortSignal): Promise<any> {
  apiClient.setToken(token);
  return apiClient.post(path, payload, signal);
}

async function copyRuntimeValue(value: string, statusId?: string): Promise<void> {
  if (!value) return;
  const ok = await writeClipboardText(value);
  if (statusId) setText(statusId, ok ? tr("Copied") : tr("Unable to copy"));
}

function openWindowLinkedSession(session: any): void {
  const project = String(session?.project || "");
  const sessionId = String(session?.workflow_session_id || "");
  const clientId = runtimeProjectClientId(project);
  if (!project || !sessionId || !clientId) return;
  applyWorkspaceView("sessions");
  selectRecentSession({ client_id: clientId, project_id: project, session_id: sessionId });
}

function renderWindowActivities(node: HTMLElement | null, activities: any[], compact = false): void {
  renderWindowActivityRows(node, activities, {
    compact,
    language: runtimeLanguage,
    onCopyTrace: (traceId) => void copyRuntimeValue(traceId),
  });
}

function renderWindowList(): void {
  const node = el("runtime-window-list");
  setText("runtime-window-list-count", String(windowRows.length));
  const emptyCopy = formatWindowEmptyState(windowAvailability, windowVisibilityScope, false, runtimeLanguage);
  setText("runtime-window-list-empty", emptyCopy);
  show("runtime-window-list-empty", windowRows.length === 0);
  setText(
    "runtime-window-list-status",
    formatWindowListStatusText(windowAvailability, windowRows.length, windowVisibilityScope, runtimeLanguage),
  );
  renderWindowCards(node, windowRows.map(row => ({ ...row, last_project_name: productName(homeProjectRows.find(project => project.id === row.last_project) || { id: row.last_project }) })), selectedWindowKey, (key) => void selectWindow(key), Date.now(), runtimeLanguage);
}

function renderWindowDetail(detail: any | null): void {
  selectedWindowDetail = detail;
  const present = !!detail;
  show("runtime-window-detail-empty", !present);
  show("runtime-window-detail", present);
  if (!detail) return;
  const fields = formatWindowDetailFields(detail, selectedWindowKey, Date.now(), runtimeLanguage);
  if (!fields) return;
  setText("runtime-window-title", fields.title);
  setText("runtime-window-key", fields.key);
  setText("runtime-window-source", fields.source);
  setText("runtime-window-active-count", fields.activeCount);
  setText("runtime-window-last-call", fields.lastCall);
  setText("runtime-window-last-meaningful", fields.lastMeaningful);
  setText("runtime-window-active-status", fields.activeStatus);
  setText("runtime-window-linked-status", fields.linkedStatus);
  setText("runtime-window-activity-status", fields.activityStatus);
  renderWindowActiveRequests(
    el("runtime-window-active-requests"),
    Array.isArray(detail.active_requests) ? detail.active_requests : [],
    {
      language: runtimeLanguage,
      onCopyTrace: (traceId) => void copyRuntimeValue(traceId),
    },
  );
  renderWindowLinkedSessions(
    el("runtime-window-linked-sessions"),
    Array.isArray(detail.linked_sessions) ? detail.linked_sessions : [],
    (session) => openWindowLinkedSession(session),
    runtimeLanguage,
  );
  renderWindowActivities(el("runtime-window-activity"), Array.isArray(detail.activity) ? detail.activity : []);
  const observed = [...(detail.active_requests || []).map((row: any) => ({ ...row, at: row.started_at_ms })), ...(detail.activity || []).filter((row: any) => row.meaningful).map((row: any) => ({ ...row, at: row.ended_at_ms }))].sort((a: any, b: any) => Number(b.at) - Number(a.at));
  const project = observed.find((row: any) => row.project)?.project;
  setText("runtime-window-project", productName(homeProjectRows.find(row => row.id === project) || { id: project }));
  setText("runtime-window-product-state", tr(Number(detail.active_count) > 0 ? "In progress" : "Observed"));
  const timeline = el("runtime-window-product-activity"); timeline?.replaceChildren();
  for (const entry of observed.slice(0, 20)) {
    const row = productNode("article"); const text = productNode("div");
    text.appendChild(productNode("strong", productActivity(entry, runtimeLanguage)));
    text.appendChild(productNode("span", productName(homeProjectRows.find(project => project.id === entry.project) || { id: entry.project }), "muted small"));
    row.append(text, productNode("time", productTime(entry.at, runtimeLanguage))); timeline?.appendChild(row);
  }
  if (!observed.length) timeline?.appendChild(productNode("p", tr("No activity observed yet"), "muted"));

  renderWorkspaceHeading();
}

async function refreshWindowDetail(): Promise<void> {
  if (!token || !selectedWindowKey) return renderWindowDetail(null);
  abort(windowDetailAbort);
  const controller = new AbortController();
  windowDetailAbort = controller;
  const key = selectedWindowKey;
  const response = await api("window", { client_window_key: key, activity_limit: 100, session_limit: 50 }, controller.signal);
  if (windowDetailAbort === controller) windowDetailAbort = null;
  if (!response || key !== selectedWindowKey) return;
  if (response.status === 401) return lock("Credential rejected.");
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

async function selectWindow(key: string): Promise<void> {
  if (!/^[0-9a-fA-F]{64}$/.test(key)) return;
  if (key !== selectedWindowKey) renderWindowDetail(null);
  selectedWindowKey = key;
  renderWindowList();
  renderWorkspaceHeading();
  setMobileNavigationOpen(false, true);
  await refreshWindowDetail();
}

async function refreshWindows(refreshSelected = true): Promise<void> {
  if (!token || workspaceView !== "windows") return;
  abort(windowsAbort);
  const controller = new AbortController();
  windowsAbort = controller;
  if (windowAvailability === "idle") {
    windowAvailability = "loading";
    renderWindowList();
  }
  const response = await api("windows", { limit: 100 }, controller.signal);
  // A superseded or navigation-cancelled request must not overwrite the newer Window state.
  if (windowsAbort !== controller) return;
  windowsAbort = null;
  // RuntimeApiClient returns null only for AbortError. Cancellation is not a refresh failure.
  if (!response) return;
  if (response.status === 401) return lock("Credential rejected.");
  const nextAvailability = runtimeWindowAvailabilityAfterHttpResponse(
    response.status,
    response.ok,
    !!response.data,
  );
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
  if (!selectedWindowKey && windowRows.length) selectedWindowKey = String(windowRows[0]?.client_window_key || "");
  renderWindowList();
  if (refreshSelected && selectedWindowKey) {
    await refreshWindowDetail();
  } else if (!selectedWindowKey) {
    renderWindowDetail(null);
  }
}

function openWindowInspector(key: string): void {
  selectedWindowKey = key;
  applyWorkspaceView("windows");
  renderWindowList();
  void refreshWindowDetail();
}

function renderSessionWindowCorrelation(detail: any): void {
  const available = detail?.window_activity_available === true;
  show("runtime-linked-windows-unavailable", !available);
  const linkedNode = el("runtime-linked-windows");
  clearNode(linkedNode);
  const links = available && Array.isArray(detail?.linked_windows) ? detail.linked_windows : [];
  setText("runtime-linked-windows-status", available ? runtimeCountLabel(links.length, "Window") : tr("runtime:read unavailable"));
  if (available) {
    renderSessionWindowCorrelationLinks(
      linkedNode,
      links,
      (key) => openWindowInspector(key),
      Date.now(),
      runtimeLanguage,
    );
  }
  const gaps = available && Array.isArray(detail?.window_activity_after_last_session_record)
    ? detail.window_activity_after_last_session_record
    : [];
  show("runtime-recorder-gap-panel", gaps.length > 0);
  renderWindowActivities(el("runtime-recorder-gap-activity"), gaps, true);
}

function projectWindowActiveCount(): number {
  if (projectWindowAvailability !== "available") return 0;
  return projectWindowRows.reduce((sum, w) => sum + Math.max(0, Number(w?.active_count || 0)), 0);
}

function clearProjectWindows(): void {
  projectWindowRows = [];
  projectWindowAvailability = "idle";
  projectWindowTruncated = false;
  projectWindowTotal = 0;
  projectWindowProjectId = "";
  renderedProjectWindowSignature = "";
  renderProjectWindows();
}

function renderProjectWindows(): void {
  renderHome();
  const list = el("runtime-project-windows-list");
  if (!list) return;

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
  setText(
    "runtime-project-windows-status",
    projectWindowAvailability === "stale"
      ? tr(count > 0 ? "Refresh failed · showing previous data" : "refresh unavailable")
      : formatProjectWindowStatusText(count, projectWindowTotal, projectWindowTruncated, runtimeLanguage)
  );
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
  if (signature === renderedProjectWindowSignature) return;
  renderedProjectWindowSignature = signature;

  renderProjectWindowCards(
    list,
    projectWindowRows,
    (key) => openWindowInspector(key),
    Date.now(),
    runtimeLanguage,
  );
}

async function fetchProjectWindows(request: any): Promise<boolean> {
  abort(projectWindowsAbort);
  const controller = new AbortController();
  projectWindowsAbort = controller;
  const response = await api("windows", { project: request.project, limit: PROJECT_WINDOW_LIMIT }, controller.signal);
  if (projectWindowsAbort === controller) projectWindowsAbort = null;
  if (!isCurrentRuntimeProjectWindowsRequest(state, request)) return false;
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

function hideDetail(): void {
  const messageSearch = el("runtime-message-search") as HTMLInputElement | null;
  if (messageSearch) messageSearch.value = "";
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
  renderedCollaborationMessageIds = new Set<string>();
  renderedCollaborationSignature = "";
  collaborationFollowLatest = true;
  collaborationPendingMessages = 0;
  syncNewMessageIndicator();
  syncResponsiveNavigation();
}

function clearSessionSurface(): void {
  saveCurrentDraft();
  sessionRows = []; sessionAvailability = "idle";
  sessionListMetaSnapshot = { total: 0, truncated: false };
  const sessionSearch = el("runtime-session-search") as HTMLInputElement | null;
  if (sessionSearch) sessionSearch.value = "";
  renderedSessionListSignature = "";
  renderedCollaborationSignature = "";
  clearNode(el("runtime-session-list"));
  show("runtime-sessions-empty", false);
  clearRuntimeWorkflowSession(state);
  locallyAuthoredCollaborationMessageIds = new Set<string>();
  abortCollaboration();
  hideDetail();
  resetCollaborationComposerUi();
  clearProjectWindows();
}

function lock(message = "", clearRemembered = true): void {
  productWorkspace.reset(); productExtensions.reset(); productProjectApi.clearToken();
  (el("runtime-command-dialog") as HTMLDialogElement | null)?.close(); el("runtime-command-results")?.replaceChildren();
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
  const messageSearch = el("runtime-message-search") as HTMLInputElement | null;
  if (messageSearch) messageSearch.value = "";
  setText("runtime-message-search-status", "");
  document.body.classList.remove("runtime-has-session");
  show("runtime-token-gate", true);
  show("runtime-console", false);
  show("runtime-topbar-controls", false);
  stopAuto();
  setRuntimeConnectionState("disconnected");
  setText("runtime-token-error", message ? tr(message) : "");
  setText("runtime-refresh-status", "");
  const input = el("runtime-token-input") as HTMLInputElement | null;
  if (input) { input.value = ""; input.focus(); }
  const search = el("runtime-project-search") as HTMLInputElement | null;
  if (search) search.value = "";
}

function unlockUi(): void {
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

function showError(message: string): void {
  setText("runtime-error", message ? tr(message) : "");
  show("runtime-error", !!message);
}

function runtimeCountLabel(value: any, singular: string, plural = singular + "s"): string {
  return localizedCountLabel(value, singular, plural, runtimeLanguage);
}

function renderRuntimeOverviewMetrics(data: any): void {
  const metrics = formatRuntimeOverviewMetrics(data, runtimeLanguage);
  if (!metrics) return;
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

async function fetchOverview(request: any): Promise<boolean> {
  abort(overviewAbort);
  const controller = new AbortController();
  overviewAbort = controller;
  const response = await api("overview", {}, controller.signal);
  if (overviewAbort === controller) overviewAbort = null;
  if (!response || !isCurrentRuntimeOverviewRequest(state, request)) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
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

function projectLabel(project: any): string {
  return formatProjectLabel(project);
}

async function fetchProjects(request: any, unlocking = false): Promise<boolean> {
  const priorSelectedProject = selectedProjectRow();
  abort(projectsAbort);
  const controller = new AbortController();
  projectsAbort = controller;
  const payload: any = { limit: 100 };
  const clientId = String(request?.clientId || "");
  const query = String(request?.query || "").trim();
  if (clientId) payload.client_id = clientId;
  if (query) payload.query = query;
  const response = await api("projects", payload, controller.signal);
  if (projectsAbort === controller) projectsAbort = null;
  if (!response || !isCurrentRuntimeProjectsRequest(state, request)) return false;
  if (response.status === 401 || response.status === 403) {
    lock("Credential does not have Runtime Console project access.");
    return false;
  }
  if (!response.ok || !response.data) {
    if (unlocking) lock("Runtime Console is unavailable.");
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
  for (const device of runtimeDeviceIds(projectRows)) known.add(device);
  knownProjectDevices = Array.from(known).sort((left, right) => left.localeCompare(right));
  if (priorSelectedProject && String(priorSelectedProject.id || "") === String(state.selectedProject || "")) {
    selectedProjectSnapshot = priorSelectedProject;
  }
  const refreshedSelected = effectiveProjects(projectRows).find(
    (project) => String(project?.id || "") === String(state.selectedProject || "")
  );
  if (refreshedSelected) selectedProjectSnapshot = refreshedSelected;
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
  } else {
    renderProjectSelectors(projectRows, projectRowsTruncated);
    const listRequest = refreshRuntimeSessionList(state);
    if (listRequest) void fetchSessions(listRequest);
    const windowRequest = refreshRuntimeProjectWindows(state);
    if (windowRequest) void fetchProjectWindows(windowRequest);
  }
  return true;
}

function effectiveProjects(projects: any[]): any[] {
  return mergeEffectiveProjects(projects, homeProjectRows);
}

function projectSelectorDevices(projects: any[]): string[] {
  return extractProjectSelectorDevices(
    projects,
    knownProjectDevices,
    runnerRows,
    String(state.selectedDevice || "")
  );
}

function selectedProjectRow(): any | null {
  const selected = String(state.selectedProject || "");
  if (!selected) return null;
  const current = effectiveProjects(projectRows).find((project) => String(project?.id || "") === selected);
  if (current) return current;
  return selectedProjectSnapshot && String(selectedProjectSnapshot.id || "") === selected
    ? selectedProjectSnapshot
    : null;
}

function renderWorkspaceBreadcrumb(): void {
  const project = selectedProjectRow();
  const { runnerText, projectText } = formatWorkspaceBreadcrumb(project, runtimeLanguage);
  setText("runtime-breadcrumb-runner", runnerText);
  setText("runtime-breadcrumb-project", projectText);
}

function renderSelectedProjectIdentity(): void {
  const project = selectedProjectRow();
  renderWorkspaceBreadcrumb();
  setText("runtime-selected-project", formatSelectedProjectIdentity(project, runtimeLanguage));
}

function renderSessionWorkspaceIdentity(): void {
  const project = selectedProjectRow();
  setText("runtime-session-workspace", formatSessionWorkspaceIdentity(project, runtimeLanguage));
}

function revealWorkflowSessionDetail(): void {
  const panel = el("runtime-workflow-sessions-panel");
  const workspace = panel?.closest("details.workspace-group") as HTMLDetailsElement | null;
  if (workspace) workspace.open = true;
  panel?.scrollIntoView({ block: "start", inline: "nearest" });
}

function renderProjectSelectors(projects: any[], truncated: boolean): void {
  renderHome();
  const deviceSelect = el("runtime-device-select") as HTMLSelectElement | null;
  const projectList = el("runtime-project-list");
  if (!deviceSelect || !projectList) return;
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
  if (signature === renderedProjectSelectorsSignature) return;
  renderedProjectSelectorsSignature = signature;
  const windowPanel = el("runtime-project-window-activity-panel");
  const sessionsPanel = el("runtime-workflow-sessions-panel");
  windowPanel?.remove();
  sessionsPanel?.remove();
  const devices = projectSelectorDevices(projects);
  renderProjectSelectorTree(
    deviceSelect,
    projectList,
    sessionsPanel,
    {
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
    },
  );
  const returnedProjects = runtimeProjectsForDevice(effective, projectDeviceFilter).length;
  const totalProjects = Math.max(returnedProjects, projectRowsTotal);
  show("runtime-projects-empty", returnedProjects === 0);
  setText("runtime-device-status", formatDeviceStatusText(devices.length, projectDeviceFilter, runtimeLanguage));
  setText("runtime-project-status", formatProjectStatusText(returnedProjects, totalProjects, truncated, projectDeviceFilter, projectSearch, runtimeLanguage));
  renderSelectedProjectIdentity();
}

function switchProject(device: string, project: string): void {
  const snapshot = effectiveProjects(projectRows).find((row) => String(row?.id || "") === project);
  selectedProjectSnapshot = snapshot || null;
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  if (device) revealRunner(device);
  const request = selectRuntimeProject(state, device, project);
  applyWorkspaceView("home");
  const windowRequest = refreshRuntimeProjectWindows(state);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  if (request) void fetchSessions(request);
  if (windowRequest) void fetchProjectWindows(windowRequest);
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
}

function applyRunnerFilter(device: string): void {
  stopProjectSearchTimer();
  abortProjectWork();
  collaborationReplyTo = "";
  clearSessionSurface();
  selectedProjectSnapshot = null;
  projectDeviceFilter = device;
  if (device) revealRunner(device);
  selectRuntimeRunnerFilter(state, device);
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
}

function renderRunnerFleet(runners: any[]): void {
  const node = el("runtime-runner-list");
  if (!node) return;
  const signature = renderFingerprint([runtimeLanguage, state.selectedDevice, runners]);
  if (signature === renderedRunnerFleetSignature) return;
  renderedRunnerFleetSignature = signature;
  show("runtime-runners-empty", runners.length === 0 && !!el("runtime-runner-unavailable")?.hidden);
  renderRunnerFleetRows(node, runners, {
    selectedDevice: state.selectedDevice,
    language: runtimeLanguage,
    onSelectRunner: (clientId) => applyRunnerFilter(clientId),
  });
  setText("runtime-runner-count", formatRunnerCountText(runners.length, runtimeLanguage));
}

function renderRecentSessions(sessions: any[], meta: any): void {
  renderHome();
  const node = el("runtime-recent-session-list");
  if (!node) return;
  const signature = renderFingerprint([
    runtimeLanguage,
    state.selectedProject,
    state.workflow.selectedSessionId,
    sessions,
    meta,
  ]);
  if (signature === renderedRecentSessionsSignature) return;
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

function selectRecentSession(session: any): void {
  applyWorkspaceView("sessions");
  const clientId = String(session?.client_id || "");
  const projectId = String(session?.project_id || "");
  const sessionId = String(session?.session_id || "");
  if (!clientId || !projectId || !sessionId) return;
  if (projectDeviceFilter && projectDeviceFilter !== clientId) projectDeviceFilter = "";
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
  if (clientId) revealRunner(clientId);
  const location = selectRuntimeSessionLocation(state, clientId, projectId, sessionId);
  restoreCurrentDraft();
  renderProjectSelectors(projectRows, projectRowsTruncated);
  renderRunnerFleet(runnerRows);
  renderRecentSessions(recentSessionRows, null);
  renderSelectedProjectIdentity();
  revealWorkflowSessionDetail();
  if (location.sessionListRequest) void fetchSessions(location.sessionListRequest);
  const windowRequest = refreshRuntimeProjectWindows(state);
  if (windowRequest) void fetchProjectWindows(windowRequest);
  if (location.detailRequest) void fetchSessionDetail(location.detailRequest);
  const collaborationRequest = runtimeCollaborationRequest(state);
  if (collaborationRequest) void startCollaboration(collaborationRequest);
  if (token) void fetchProjects(refreshRuntimeProjects(state, projectSearch, projectDeviceFilter));
  setMobileNavigationOpen(false, true);
}

async function fetchSessions(request: any): Promise<void> {
  abort(sessionsAbort);
  const controller = new AbortController();
  sessionsAbort = controller;
  const response = await api("workflow-sessions", { project: request.project, limit: 50 }, controller.signal);
  if (sessionsAbort === controller) sessionsAbort = null;
  if (!response || !isCurrentRuntimeSessionListRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 403 || response.status === 404) { sessionAvailability = "stale"; renderHome(); showError("Selected project is no longer available."); return; }
  if (!response.ok || !response.data) { sessionAvailability = "stale"; renderHome(); showError("Could not refresh Workflow Sessions."); return; }
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
      if (detailRequest) void fetchSessionDetail(detailRequest);
    }
  } else if (selected) {
    abortCollaboration();
    clearRuntimeWorkflowSession(state);
    hideDetail();
  }
}

function updatedLabel(timestamp: any): string {
  return formatUpdatedTime(timestamp, runtimeLanguage);
}

function dateTimeLabel(timestamp: any): string {
  return formatSessionDateTime(timestamp, runtimeLanguage);
}

function localizedLivenessPresentation(session: any): any {
  return formatLivenessPresentation(session, runtimeLanguage);
}

function localWorkflowText(value: unknown): string {
  return localizedWorkflowText(value, runtimeLanguage);
}

function renderSessionList(sessions: any[], payload: any): void {
  const node = el("runtime-session-list");
  if (!node) return;
  const total = typeof payload.total === "number" ? payload.total : sessions.length;
  const selected = String(state.workflow.selectedSessionId || "");
  const query = (el("runtime-session-search") as HTMLInputElement | null)?.value || "";
  const visible = sessions.filter((session) => runtimeSearchMatches(query, [session.title, session.session_id, session.lifecycle]));
  const signature = renderFingerprint([runtimeLanguage, selected, total, !!payload.truncated, sessions, query]);
  if (signature === renderedSessionListSignature) return;
  renderedSessionListSignature = signature;
  clearNode(node); show("runtime-sessions-empty", visible.length === 0);
  setText("runtime-session-search-status", query ? visible.length + " / " + sessions.length : "");
  setText("runtime-sessions-count", total ? sessions.length + (payload.truncated ? " of " + total : "") : "0");
  renderWorkspaceSessionList(node, visible, selected, runtimeLanguage, selectSession);
  renderHome();
}

function selectSession(sessionId: string): void {
  applyWorkspaceView("sessions");
  saveCurrentDraft();
  abort(detailAbort); detailAbort = null; abortCollaboration(); hideDetail();
  setHumanJoinSendEnabled(false);
  const request = selectRuntimeWorkflowSession(state, sessionId);
  locallyAuthoredCollaborationMessageIds = new Set<string>();
  resetCollaborationComposerUi();
  restoreCurrentDraft();
  renderSessionList(sessionRows, sessionListMetaSnapshot);
  revealWorkflowSessionDetail();
  if (request) void fetchSessionDetail(request);
  const collaborationRequest = runtimeCollaborationRequest(state);
  if (collaborationRequest) void startCollaboration(collaborationRequest);
  setMobileNavigationOpen(false, true);
}

async function fetchSessionDetail(request: any): Promise<void> {
  abort(detailAbort);
  const controller = new AbortController(); detailAbort = controller;
  const response = await api("workflow-session", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (detailAbort === controller) detailAbort = null;
  if (!response || !isCurrentRuntimeWorkflowSessionRequest(state, request)) return;
  if (response.status === 401) return lock("Credential rejected.");
  if (response.status === 404) { abortCollaboration(); clearRuntimeWorkflowSession(state); hideDetail(); resetCollaborationComposerUi(); return; }
  if (!response.ok || !response.data) { showError("Could not refresh Workflow Session detail."); return; }
  if (!adoptRuntimeWorkflowSessionDetail(state, request, response.data)) return;
  renderDetail(response.data);
}

function setTone(id: string, tone: string): void {
  const node = el(id); if (!node) return;
  for (const name of ["pass", "warn", "fail", "muted"]) node.classList.toggle("tone-card-" + name, tone === name);
}

function syncFollowUi(): void {
  show("runtime-jump-latest", !!state.workflow.selectedSessionId && !shouldFollowWorkflowSessionLatest(state.workflow));
}

function renderDetail(detail: any, consumeCollaborationNotice = true): void {
  document.body.classList.add("runtime-has-session");
  show("runtime-session-detail-empty", false); show("runtime-session-detail", true); show("runtime-session-context", true);
  renderWorkspaceHeading();
  setText("runtime-session-lifecycle", tr(String(detail.lifecycle || "unknown")));
  setText("runtime-session-mode", (runtimeLanguage === "zh-CN" ? "模式 " : "mode ") + tr(String(detail.mode || "unknown")));
  setText("runtime-session-context-lifecycle", tr(String(detail.lifecycle || "unknown")));
  setText("runtime-session-context-mode", tr(String(detail.mode || "unknown")));
  const liveness = localizedLivenessPresentation(detail);
  setText("runtime-session-running", liveness.label);
  const livenessNode = el("runtime-session-running"); if (livenessNode) livenessNode.title = liveness.tooltip;
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
  if (!node) return syncFollowUi();
  node.scrollTop = workflowSessionScrollTopAfterRender(state.workflow, previousScrollTop, node.clientHeight, node.scrollHeight);
  syncFollowUi();
}

function syncCollaborationComposer(): void {
  const edit = runtimeCollaborationEditTarget(state);
  const unavailable = state.collaboration.available === false;
  const replyTargetId = String(state.collaboration.replyTargetId || "");
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  if (unavailable) closeComposerOptions(false);
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
    if (edit) kind.value = String(edit.kind || "note");
  }
  if (priority) {
    priority.disabled = unavailable || !!edit;
    if (edit) priority.value = String(edit.priority || "normal");
  }
  if (checkbox && edit) checkbox.checked = !!edit.requires_ack;
  if (send) {
    const actionLabel = edit ? tr("Replace message") : tr("Send message");
    send.title = actionLabel;
    send.setAttribute("aria-label", actionLabel);
    send.classList.toggle("replace-mode", !!edit);
  }
  syncAckComposer();
  syncCollaborationComposerLayout(body, el("runtime-collaboration-form"), send);
  if (unavailable && checkbox) checkbox.disabled = true;
}

function scrollCollaborationToLatest(smooth: boolean): void {
  const scroll = el("runtime-chat-scroll");
  if (!scroll) return;
  collaborationFollowLatest = true;
  collaborationPendingMessages = 0;
  syncNewMessageIndicator();
  const behavior: ScrollBehavior = smooth && !window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "smooth" : "auto";
  window.requestAnimationFrame(() => {
    scroll.scrollTo({ top: scroll.scrollHeight, behavior });
  });
}

function syncComposerOptionSummary(): void {
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const options = el("runtime-message-options");
  const summary = formatComposerOptionSummary(
    String(kind?.value || ""),
    String(priority?.value || ""),
    !!checkbox?.checked,
    runtimeLanguage,
  );
  setText("runtime-message-options-label", summary.label);
  options?.classList.toggle("has-selection", summary.hasSelection);
}

function setCollaborationReplyTarget(messageId: string): void {
  collaborationReplyTo = messageId;
  const wasEditing = !!runtimeCollaborationEditTarget(state);
  setRuntimeCollaborationReplyTarget(state, messageId);
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (wasEditing && body) body.value = "";
  syncCollaborationComposer();
  if (messageId) {
    setText("runtime-message-send-status", runtimeLanguage === "zh-CN"
      ? "已选择回复目标。下一条消息将回复 " + messageId + "。"
      : "Reply target selected. Your next message will reply to " + messageId + ".");
    body?.focus();
  } else {
    setText("runtime-message-send-status", tr("Reply target cleared."));
  }
}

function beginCollaborationEdit(message: any): void {
  saveCurrentDraft();
  if (!setRuntimeCollaborationEditTarget(state, String(message?.message_id || ""))) return;
  collaborationReplyTo = "";
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (body) {
    body.value = String(message?.message || "");
    body.focus();
  }
  setText("runtime-message-send-status", "");
  syncCollaborationComposer();
}

function cancelCollaborationEdit(): void {
  clearRuntimeCollaborationEditTarget(state);
  restoreCurrentDraft();
  setText("runtime-message-send-status", tr("Edit cancelled."));
  syncCollaborationComposer();
}

function resetCollaborationComposerUi(): void {
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  if (body) body.value = "";
  closeComposerOptions(false);
  syncCollaborationComposer();
}

function filterCollaborationMessages(): void {
  const query = (el("runtime-message-search") as HTMLInputElement | null)?.value || "";
  const cards = Array.from(
    document.querySelectorAll<HTMLElement>("#runtime-collaboration-board .message-card"),
  );
  const separators = Array.from(
    document.querySelectorAll<HTMLElement>("#runtime-collaboration-board .message-date-separator"),
  );
  const result = filterCollaborationCards(cards, separators, state.collaboration.messages, query);
  setText(
    "runtime-message-search-status",
    query.trim() ? result.matches + " / " + result.total : "",
  );
}

function renderCollaboration(statusText?: string, consumeMutationNotice = true): void {
  const mutationNotice = consumeMutationNotice ? takeRuntimeCollaborationMutationNotice(state) : "";
  if (mutationNotice) {
    const editStillActive = !!runtimeCollaborationEditTarget(state);
    if (
      mutationNotice.includes("changed while editing")
      || mutationNotice.includes("Replacement confirmed")
      || mutationNotice.includes("Withdraw confirmed")
      || (mutationNotice.includes("Outcome not observed") && !editStillActive)
    ) {
      const body = el("runtime-message-body") as HTMLTextAreaElement | null;
      if (body) body.value = "";
    }
    const localizedMutationNotice = tr(mutationNotice);
    setText("runtime-message-send-status", localizedMutationNotice);
    statusText = [statusText ? tr(statusText) : "", localizedMutationNotice].filter(Boolean).join(" · ");
  }
  const available = state.collaboration.available !== false;
  if (!available) {
    const body = el("runtime-message-body") as HTMLTextAreaElement | null;
    if (body) body.value = "";
  }
  show("runtime-collaboration-unavailable", !available);
  show("runtime-collaboration-form", true);
  el("runtime-collaboration-form")?.classList.toggle("is-unavailable", !available);
  const messages = available && Array.isArray(state.collaboration.messages) ? state.collaboration.messages : [];
  const scroll = el("runtime-chat-scroll");
  const previousScrollTop = scroll?.scrollTop || 0;
  const shouldFollowNewMessages = collaborationFollowLatest || chatIsNearLatest();
  const previouslyRenderedMessageIds = renderedCollaborationMessageIds;
  const nextRenderedMessageIds = new Set<string>(messages.map((message: any) => String(message?.message_id || "")).filter(Boolean));
  const newMessageIds = Array.from(nextRenderedMessageIds).filter((id) => !previouslyRenderedMessageIds.has(id));
  const hasNewMessages = newMessageIds.length > 0;
  const firstRetainedRender = previouslyRenderedMessageIds.size === 0 && nextRenderedMessageIds.size > 0;
  show("runtime-collaboration-board", messages.length > 0);
  show("runtime-collaboration-empty", messages.length === 0);
  setText("runtime-collaboration-empty-title", available ? tr("Start this Session conversation") : tr("Conversation access unavailable"));
  setText(
    "runtime-collaboration-empty-copy",
    available
      ? tr("Messages posted here are retained on the Session collaboration board.")
      : tr("This credential can inspect the Project and Session, but retained messages require runtime:read.")
  );
  const localizedStatusText = statusText ? tr(statusText) : "";
  const status = available
    ? (runtimeLanguage === "zh-CN" ? "协作：" : "Collaboration: ") + collaborationPhaseLabel(state.collaboration.phase, runtimeLanguage) + " · " + runtimeCountLabel(messages.length, "retained message") + (localizedStatusText ? " · " + localizedStatusText : "")
    : (runtimeLanguage === "zh-CN" ? "runtime:read 不可用" : "runtime:read unavailable");
  setText("runtime-collaboration-status", status);
  const node = el("runtime-collaboration-board");
  syncCollaborationComposer();
  if (!available) setHumanJoinSendEnabled(false);
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
  if (hasNewMessages && !firstRetainedRender) announceNewCollaborationMessages(newMessageIds.length);
  if (firstRetainedRender || (hasNewMessages && shouldFollowNewMessages)) {
    scrollCollaborationToLatest(!firstRetainedRender);
  } else {
    window.requestAnimationFrame(() => {
      if (scroll) scroll.scrollTop = previousScrollTop;
    });
    if (hasNewMessages) {
      collaborationFollowLatest = false;
      collaborationPendingMessages += newMessageIds.length;
      syncNewMessageIndicator();
    }
  }
}

async function confirmCollaborationMutationDurability(
  request: any,
  mutation: any,
  controller: AbortController
): Promise<boolean> {
  const replacing = mutation?.kind === "replace";
  const payload: any = {
    project: request.project,
    session_id: request.sessionId,
    message_id: String(mutation?.messageId || ""),
  };
  if (replacing) payload.message = String(mutation?.message || "");
  setText("runtime-message-send-status", tr(replacing
    ? "Confirming replacement durability…"
    : "Confirming withdrawal durability…"));
  const response = await api(
    replacing ? "workflow-session-replace-message" : "workflow-session-withdraw-message",
    payload,
    controller.signal
  );
  if (!response || !isCurrentRuntimeCollaborationRequest(state, request)) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
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
  } else {
    adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
  }
  completeRuntimeCollaborationMutationRecovery(
    state,
    request,
    replacing
      ? "Replacement durably confirmed after exact replay."
      : "Withdraw durably confirmed after exact replay."
  );
  return true;
}

async function loadRetainedCollaboration(request: any, controller: AbortController): Promise<string | null> {
  // Establish the cursor before the retained snapshot. A mutation between these
  // two reads is then present in the snapshot, the subsequent delta, or both;
  // merge-by-id makes the overlap harmless. Listing first and baselining second
  // would permanently skip a mutation that lands in that gap.
  setRuntimeCollaborationPhase(state, request, "reconnecting");
  renderCollaboration("establishing retained baseline");
  const baseline = await api("workflow-session-observe", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!baseline || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (baseline.status === 401) { lock("Credential rejected."); return null; }
  if (baseline.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); return null; }
  if (baseline.status === 404) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("Session unavailable"); return null; }
  if (!baseline.ok || !baseline.data || typeof baseline.data.observation_token !== "string") { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("observation unavailable"); return null; }

  const response = await api("workflow-session-messages", { project: request.project, session_id: request.sessionId, limit: 100 }, controller.signal);
  if (!response || !isCurrentRuntimeCollaborationRequest(state, request)) return null;
  if (response.status === 401) { lock("Credential rejected."); return null; }
  if (response.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); return null; }
  if (response.status === 404) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("Session unavailable"); return null; }
  if (!response.ok || !response.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("retained snapshot failed"); return null; }
  setRuntimeCollaborationAvailable(state, request, true);
  if (!adoptRuntimeCollaborationList(state, request, Array.isArray(response.data.messages) ? response.data.messages : [])) return null;
  adoptRuntimeCollaborationObservation(state, request, baseline.data);
  const mutationRecovery = runtimeCollaborationMutationRecovery(state, request);
  if (mutationRecovery && !(await confirmCollaborationMutationDurability(request, mutationRecovery, controller))) return null;
  setRuntimeCollaborationPhase(state, request, "live");
  setHumanJoinSendEnabled(true);
  renderCollaboration("bounded long-poll");
  return baseline.data.observation_token;
}

async function startCollaboration(request: any): Promise<void> {
  abortCollaboration();
  const controller = new AbortController(); collaborationAbort = controller;
  let observationToken = await loadRetainedCollaboration(request, controller);
  while (observationToken && collaborationAbort === controller && isCurrentRuntimeCollaborationRequest(state, request)) {
    const response = await api("workflow-session-observe", {
      project: request.project,
      session_id: request.sessionId,
      after_observation_token: observationToken,
      wait_secs: COLLABORATION_WAIT_SECS,
      limit: 100,
    }, controller.signal);
    if (!response || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) break;
    if (response.status === 401) { lock("Credential rejected."); break; }
    if (response.status === 403) { setRuntimeCollaborationAvailable(state, request, false); setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration(); break; }
    if (!response.ok || !response.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("request failed"); break; }
    const action = runtimeCollaborationObservationAction(response.data);
    if (action === "reload") {
      renderCollaboration("retention changed · reloading");
      observationToken = await loadRetainedCollaboration(request, controller);
      continue;
    }
    if (!adoptRuntimeCollaborationObservation(state, request, response.data)) break;
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
        if (!drain || collaborationAbort !== controller || !isCurrentRuntimeCollaborationRequest(state, request)) break;
        if (!drain.ok || !drain.data) { setRuntimeCollaborationPhase(state, request, "paused"); renderCollaboration("delta drain failed"); observationToken = null; break; }
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
  if (collaborationAbort === controller) collaborationAbort = null;
}

function jumpLatest(): void {
  jumpWorkflowSessionToLatest(state.workflow);
  const node = el("runtime-timeline"); if (node) node.scrollTop = node.scrollHeight; syncFollowUi();
}

function sessionCollaborationAuthorityFailure(response: any): string | null {
  if (response?.status !== 403) return null;
  return "Session collaboration access required. This credential can still read the Session; add session:collaborate to send, edit, or withdraw messages.";
}

function setHumanJoinSendEnabled(enabled: boolean): void {
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  if (send) send.disabled = !enabled;
}

function syncAckComposer(): void {
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const edit = runtimeCollaborationEditTarget(state);
  const guidance = edit ? edit.kind === "guidance" : kind?.value === "guidance";
  show("runtime-message-ack-label", guidance);
  if (!checkbox) { syncComposerOptionSummary(); return; }
  if (edit) {
    checkbox.disabled = true;
    checkbox.checked = !!edit.requires_ack;
    checkbox.title = "Inherited from the original retained message.";
    syncComposerOptionSummary();
    return;
  }
  checkbox.disabled = !guidance || priority?.value !== "high";
  if (checkbox.disabled) checkbox.checked = false;
  checkbox.title = guidance && priority?.value !== "high" ? "ACK requirement is available for High priority guidance." : "";
  syncComposerOptionSummary();
}

async function withdrawHumanCollaborationMessage(messageId: string): Promise<void> {
  const request = runtimeCollaborationRequest(state);
  if (!request || state.collaboration.available === false) return;
  setText("runtime-message-send-status", tr("Withdrawing retained message…"));
  const response = await api("workflow-session-withdraw-message", {
    project: request.project,
    session_id: request.sessionId,
    message_id: messageId,
  });
  if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
  if (response?.status === 0 || response?.status === 503) {
    markRuntimeCollaborationMutationUncertain(state, request, { kind: "withdraw", messageId });
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    renderCollaboration("withdraw outcome unknown · refresh before retry");
    return;
  }
  if (response?.status === 401) { lock("Credential rejected."); return; }
  const authorityFailure = sessionCollaborationAuthorityFailure(response);
  if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
  if (response?.status === 409) {
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    setText("runtime-message-send-status", tr("Message changed before Delete. Refresh retained messages before retrying."));
    renderCollaboration("message changed · refresh retained state");
    return;
  }
  if (!response?.ok || !response.data?.message) { setText("runtime-message-send-status", tr("Delete failed.")); return; }
  if (String(state.collaboration.editTargetId || "") === messageId) {
    clearRuntimeCollaborationEditTarget(state);
    restoreCurrentDraft();
  }
  adoptRuntimeCollaborationObservation(state, request, { messages: [response.data.message] });
  setText("runtime-message-send-status", tr("Retained message withdrawn."));
  renderCollaboration();
}

async function postHumanCollaborationMessage(event: Event): Promise<void> {
  event.preventDefault();
  const request = runtimeCollaborationRequest(state);
  if (!request || state.collaboration.available === false) return;
  const kind = el("runtime-message-kind") as HTMLSelectElement | null;
  const priority = el("runtime-message-priority") as HTMLSelectElement | null;
  const body = el("runtime-message-body") as HTMLTextAreaElement | null;
  const checkbox = el("runtime-message-requires-ack") as HTMLInputElement | null;
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  const message = body?.value.trim() || "";
  if (!message) { setText("runtime-message-send-status", tr("Enter a message.")); return; }
  closeComposerOptions(false);
  const editTarget = runtimeCollaborationEditTarget(state);
  if (editTarget) {
    if (send) send.disabled = true;
    setText("runtime-message-send-status", tr("Replacing retained message…"));
    const response = await api("workflow-session-replace-message", {
      project: request.project,
      session_id: request.sessionId,
      message_id: editTarget.message_id,
      message,
    });
    if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
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
    if (send) send.disabled = false;
    if (response?.status === 401) { lock("Credential rejected."); return; }
    const authorityFailure = sessionCollaborationAuthorityFailure(response);
    if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
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
  if (send) send.disabled = true;
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
  if (!isCurrentRuntimeCollaborationRequest(state, request)) return;
  if (response?.status === 0) {
    abortCollaboration();
    setRuntimeCollaborationPhase(state, request, "paused");
    setText("runtime-message-send-status", tr("Send outcome unknown. Refresh and review retained messages before retrying."));
    renderCollaboration("send outcome unknown · refresh before retry");
    return;
  }
  if (send) send.disabled = false;
  if (response?.status === 401) { lock("Credential rejected."); return; }
  const authorityFailure = sessionCollaborationAuthorityFailure(response);
  if (authorityFailure) { setText("runtime-message-send-status", authorityFailure); return; }
  if (!response?.ok || !response.data) { setText("runtime-message-send-status", tr("Send failed.")); return; }
  rememberLocalCollaborationMessage(response.data?.message_id);
  adoptRuntimeCollaborationObservation(state, request, { messages: [response.data] });
  if (body) body.value = "";
  clearCurrentDraft();
  setCollaborationReplyTarget("");
  setText("runtime-message-send-status", tr("Sent."));
  renderCollaboration();
}

function communicationAgent(agentId: string): any | null {
  return communicationAgents.find((agent) => String(agent?.agent_id || "") === agentId) || null;
}

function selectedCommunicationAgent(): any | null {
  return communicationAgent(selectedCommunicationAgentId);
}

function selectedCommunicationConversation(): any | null {
  return communicationConversations.find(
    (conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId
  ) || null;
}

function communicationEndpoint(agentId = selectedCommunicationAgentId): RuntimeCommunicationEndpoint | null {
  return communicationEndpoints.get(agentId) || null;
}

function communicationEndpointId(agentId = selectedCommunicationAgentId): string {
  return communicationEndpoint(agentId)?.endpoint_id || "";
}

function resetCommunicationSurface(): void {
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
  const agentUpdateForm = el("runtime-agent-update-form") as HTMLFormElement | null;
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

function detachCommunicationEndpointsBestEffort(): void {
  if (!token || communicationEndpoints.size === 0) return;
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

function renderCommunicationAvailability(): void {
  const available = communicationReadAvailable !== false;
  show("runtime-communication-unavailable", !available);
  show("runtime-communication-surface", available);
  setText(
    "runtime-communication-status",
    formatCommunicationAvailability(communicationReadAvailable, communicationManageAvailable, runtimeLanguage)
  );
}

function renderCommunicationAgents(): void {
  setText("runtime-communication-count", runtimeCountLabel(communicationAgents.length, "Agent"));
  const list = el("runtime-agent-list");
  show("runtime-agent-empty", communicationReadAvailable === true && communicationAgents.length === 0);
  renderAgentRows(list, communicationAgents, selectedCommunicationAgentId, {
    language: runtimeLanguage,
    onSelect: (agentId) => {
      selectedCommunicationAgentId = agentId;
      communicationInbox = [];
      const participants = el("runtime-conversation-agent-ids") as HTMLInputElement | null;
      if (participants && !participants.value.trim()) participants.value = agentId;
      renderCommunicationAgents();
      renderCommunicationAgentCard();
      renderCommunicationInbox();
      if (communicationEndpointId(agentId)) void fetchCommunicationInbox(communicationGeneration);
    },
  });
}

function renderCommunicationAgentCard(): void {
  const agent = selectedCommunicationAgent();
  show("runtime-agent-card", !!agent);
  if (!agent) return;
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
  const updateForm = el("runtime-agent-update-form") as HTMLFormElement | null;
  const revision = String(agent.profile_revision || 0);
  if (updateForm && (
    updateForm.dataset.agentId !== agentId
    || updateForm.dataset.profileRevision !== revision
  )) {
    const handle = el("runtime-agent-update-handle") as HTMLInputElement | null;
    const displayName = el("runtime-agent-update-display-name") as HTMLInputElement | null;
    const description = el("runtime-agent-update-description") as HTMLTextAreaElement | null;
    const labelsInput = el("runtime-agent-update-labels") as HTMLInputElement | null;
    if (handle) handle.value = String(agent.handle || "");
    if (displayName) displayName.value = String(agent.display_name || "");
    if (description) description.value = String(agent.description || "");
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

function renderCommunicationConversations(): void {
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

function renderCommunicationConversation(): void {
  const detail = communicationDetail;
  const summary = detail?.conversation || selectedCommunicationConversation();
  const available = !!summary && String(summary?.conversation_id || "") === selectedCommunicationConversationId;
  show("runtime-conversation-detail", available);
  show("runtime-conversation-detail-empty", !available);
  const transcript = el("runtime-conversation-transcript");
  clearNode(transcript);
  clearNode(el("runtime-conversation-participants"));
  if (!available || !detail) return;
  setText("runtime-conversation-name", String(summary.title || tr("Untitled Conversation")));
  setText("runtime-conversation-id", String(summary.conversation_id || ""));
  setText(
    "runtime-conversation-seq",
    formatConversationSeq(summary, detail, runtimeLanguage)
  );
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

function renderCommunicationInbox(): void {
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
  setText(
    "runtime-inbox-status",
    runtimeCountLabel(totalQueued, "queued delivery")
      + (communicationInbox.length < totalQueued ? (runtimeLanguage === "zh-CN" ? " · 当前显示 " : " · showing ") + String(communicationInbox.length) : "")
      + (runtimeLanguage === "zh-CN" ? " · 读取不会消费投递或唤醒模型" : " · reading does not consume or wake a model")
  );
  renderInboxDeliveryCards(list, communicationInbox, communicationAgents, {
    language: runtimeLanguage,
    onConsume: (deliveryId) => void consumeCommunicationDeliveries([deliveryId]),
  });
}

function renderCommunicationSurface(): void {
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
  if (signature === renderedCommunicationSurfaceSignature) return;
  renderedCommunicationSurfaceSignature = signature;
  renderCommunicationAgents();
  renderCommunicationAgentCard();
  renderCommunicationConversations();
  renderCommunicationConversation();
  renderCommunicationInbox();
}

async function fetchCommunicationAgents(generation: number, render = true): Promise<boolean> {
  const response = await api("communication/agents", { offset: 0, limit: 100 });
  if (generation !== communicationGeneration || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    communicationAgents = [];
    communicationConversations = [];
    communicationDetail = null;
    communicationInbox = [];
    if (render) renderCommunicationSurface();
    return true;
  }
  if (!response.ok || !response.data) return false;
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

async function fetchCommunicationConversations(generation: number, render = true): Promise<boolean> {
  const response = await api("communication/conversations", { offset: 0, limit: 100 });
  if (generation !== communicationGeneration || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    if (render) renderCommunicationSurface();
    return true;
  }
  if (!response.ok || !response.data) return false;
  communicationReadAvailable = true;
  communicationConversations = Array.isArray(response.data.conversations) ? response.data.conversations : [];
  if (!communicationConversations.some((conversation) => String(conversation?.conversation_id || "") === selectedCommunicationConversationId)) {
    selectedCommunicationConversationId = String(communicationConversations[0]?.conversation_id || "");
    communicationDetail = null;
  }
  if (render) renderCommunicationConversations();
  return true;
}

async function fetchCommunicationConversation(generation: number, render = true): Promise<boolean> {
  const conversationId = selectedCommunicationConversationId;
  if (!conversationId) {
    communicationDetail = null;
    if (render) renderCommunicationConversation();
    return true;
  }
  const afterSeq = runtimeCommunicationTranscriptAfterSeq(selectedCommunicationConversation()?.last_seq, 100);
  const response = await api("communication/conversation", {
    conversation_id: conversationId,
    after_seq: afterSeq,
    limit: 100,
  });
  if (generation !== communicationGeneration || conversationId !== selectedCommunicationConversationId || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    if (render) renderCommunicationSurface();
    return true;
  }
  if (response.status === 404) {
    communicationDetail = null;
    selectedCommunicationConversationId = "";
    if (render) renderCommunicationConversation();
    return false;
  }
  if (!response.ok || !response.data) return false;
  communicationDetail = response.data;
  if (render) renderCommunicationConversation();
  return true;
}

async function fetchCommunicationInbox(generation: number, render = true): Promise<boolean> {
  const agentId = selectedCommunicationAgentId;
  const endpoint = communicationEndpoint(agentId);
  const endpointId = endpoint?.endpoint_id || "";
  if (!agentId || !endpoint) {
    communicationInbox = [];
    if (render) renderCommunicationInbox();
    return true;
  }
  const response = await api("communication/inbox", {
    agent_id: agentId,
    endpoint_id: endpointId,
    expected_controller_generation: endpoint.controller_generation,
    after_delivery_order: 0,
    limit: 100,
  });
  if (generation !== communicationGeneration || agentId !== selectedCommunicationAgentId || endpointId !== communicationEndpointId(agentId) || !response) return false;
  if (response.status === 401) { lock("Credential rejected."); return false; }
  if (response.status === 403) {
    communicationReadAvailable = false;
    if (render) renderCommunicationSurface();
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
  if (!response.ok || !response.data) return false;
  communicationInbox = Array.isArray(response.data.deliveries) ? response.data.deliveries : [];
  if (render) renderCommunicationInbox();
  return true;
}

async function renewCommunicationEndpoints(generation: number): Promise<boolean> {
  if (communicationEndpoints.size === 0 || communicationManageAvailable === false) return true;
  for (const [agentId, endpoint] of Array.from(communicationEndpoints.entries())) {
    const response = await api("communication/endpoint/renew", {
      endpoint_id: endpoint.endpoint_id,
      expected_controller_generation: endpoint.controller_generation,
    });
    if (generation !== communicationGeneration) return false;
    if (!response) return false;
    if (response.status === 401) { lock("Credential rejected."); return false; }
    if (response.status === 403) {
      communicationManageAvailable = false;
      renderCommunicationAvailability();
      return true;
    }
    if (response.status === 400 || response.status === 404) {
      communicationEndpoints.delete(agentId);
      if (agentId === selectedCommunicationAgentId) communicationInbox = [];
      continue;
    }
    if (!response.ok || !response.data?.endpoint?.endpoint_id) return false;
    communicationManageAvailable = true;
    communicationEndpoints.set(agentId, response.data.endpoint as RuntimeCommunicationEndpoint);
  }
  return true;
}

async function performCommunicationRefresh(includeData: boolean): Promise<boolean> {
  if (!token) return true;
  const generation = ++communicationGeneration;
  if (includeData && communicationReadAvailable !== false) {
    setText("runtime-communication-status", tr("Refreshing durable communication…"));
  }
  const endpointsOk = await renewCommunicationEndpoints(generation);
  if (generation !== communicationGeneration) return false;
  if (!includeData || communicationReadAvailable === false) return endpointsOk;
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

function refreshCommunication(includeData = true): Promise<boolean> {
  if (!token) return Promise.resolve(true);
  return communicationRefreshCoordinator.refresh(includeData);
}

async function createCommunicationAgent(event: Event): Promise<void> {
  event.preventDefault();
  const handle = (el("runtime-agent-handle") as HTMLInputElement | null)?.value || "";
  const displayName = (el("runtime-agent-display-name") as HTMLInputElement | null)?.value || "";
  const description = (el("runtime-agent-description") as HTMLTextAreaElement | null)?.value || "";
  const labelsRaw = (el("runtime-agent-labels") as HTMLInputElement | null)?.value || "";
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
  if (response?.status === 401) { lock("Credential rejected."); return; }
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
    const input = el(id) as HTMLInputElement | HTMLTextAreaElement | null;
    if (input) input.value = "";
  }
  setText("runtime-agent-create-status", tr(response.data.replayed ? "Existing idempotent Agent replayed." : "Agent created."));
  await refreshCommunication();
}

async function updateCommunicationAgent(event: Event): Promise<void> {
  event.preventDefault();
  const agent = selectedCommunicationAgent();
  if (!agent) return;
  const handle = (el("runtime-agent-update-handle") as HTMLInputElement | null)?.value || "";
  const displayName = (el("runtime-agent-update-display-name") as HTMLInputElement | null)?.value || "";
  const description = (el("runtime-agent-update-description") as HTMLTextAreaElement | null)?.value || "";
  const labelsRaw = (el("runtime-agent-update-labels") as HTMLInputElement | null)?.value || "";
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
  if (response?.status === 401) { lock("Credential rejected."); return; }
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

async function attachCommunicationEndpoint(): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  if (!agentId) return;
  for (const [otherAgentId, otherEndpoint] of Array.from(communicationEndpoints.entries())) {
    if (otherAgentId === agentId) continue;
    setText("runtime-agent-endpoint-status", tr("Releasing this window’s previous Agent Endpoint…"));
    const detached = await api("communication/endpoint/detach", {
      endpoint_id: otherEndpoint.endpoint_id,
    });
    if (detached?.status === 401) { lock("Credential rejected."); return; }
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
  if (response?.status === 401) { lock("Credential rejected."); return; }
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
  communicationEndpoints.set(agentId, response.data.endpoint as RuntimeCommunicationEndpoint);
  pendingEndpointAttach.delete(agentId);
  renderCommunicationAgentCard();
  await fetchCommunicationInbox(communicationGeneration);
}

async function detachCommunicationEndpoint(): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  const endpointId = communicationEndpointId(agentId);
  if (!agentId || !endpointId) return;
  setText("runtime-agent-endpoint-status", tr("Detaching browser Endpoint…"));
  const response = await api("communication/endpoint/detach", { endpoint_id: endpointId });
  if (response?.status === 401) { lock("Credential rejected."); return; }
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

async function createCommunicationConversation(event: Event): Promise<void> {
  event.preventDefault();
  const title = (el("runtime-conversation-title") as HTMLInputElement | null)?.value || "";
  const idsInput = (el("runtime-conversation-agent-ids") as HTMLInputElement | null)?.value || "";
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
  if (response?.status === 401) { lock("Credential rejected."); return; }
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
  const titleInput = el("runtime-conversation-title") as HTMLInputElement | null;
  const agentsInput = el("runtime-conversation-agent-ids") as HTMLInputElement | null;
  if (titleInput) titleInput.value = "";
  if (agentsInput) agentsInput.value = selectedCommunicationAgentId;
  setText("runtime-conversation-create-status", tr(response.data.replayed ? "Existing idempotent Conversation replayed." : "Conversation created."));
  await refreshCommunication();
}

async function postCommunicationMessage(event: Event): Promise<void> {
  event.preventDefault();
  const conversationId = selectedCommunicationConversationId;
  const bodyNode = el("runtime-conversation-body") as HTMLTextAreaElement | null;
  const recipientsNode = el("runtime-conversation-recipients") as HTMLInputElement | null;
  const body = bodyNode?.value.trim() || "";
  const recipientsText = recipientsNode?.value.trim() || "";
  const sendAsAgent = (el("runtime-conversation-send-as-agent") as HTMLInputElement | null)?.checked === true;
  if (!conversationId || !body) { setText("runtime-conversation-send-status", tr("Select a Conversation and enter a message.")); return; }
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
  if (response?.status === 401) { lock("Credential rejected."); return; }
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
  if (bodyNode) bodyNode.value = "";
  setText("runtime-conversation-send-status", tr(response.data.replayed ? "Existing Message replayed without duplicate delivery." : "Durable Message sent."));
  await refreshCommunication();
}

async function consumeCommunicationDeliveries(deliveryIds: string[]): Promise<void> {
  const agentId = selectedCommunicationAgentId;
  const endpoint = communicationEndpoint(agentId);
  const endpointId = endpoint?.endpoint_id || "";
  const ids = deliveryIds.filter(Boolean);
  if (!agentId || !endpoint || ids.length === 0) return;
  setText("runtime-inbox-status", tr("Consuming recipient state…"));
  const response = await api("communication/inbox/consume", {
    agent_id: agentId,
    endpoint_id: endpointId,
    expected_controller_generation: endpoint.controller_generation,
    delivery_ids: ids,
  });
  if (response?.status === 401) { lock("Credential rejected."); return; }
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

function setRefreshBusy(active: boolean): void {
  refreshInFlight = active;
  const button = el("runtime-refresh") as HTMLButtonElement | null;
  if (button) {
    button.disabled = active;
    button.classList.toggle("is-busy", active);
    button.title = active ? tr("Refreshing runtime") : tr("Refresh runtime");
    button.setAttribute("aria-label", button.title);
  }
}

async function refreshAll(): Promise<void> {
  productWorkspace.invalidateGit();
  if (workspaceView === "extensions") void productExtensions.refresh();
  if (!token || refreshInFlight) return;
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
    if (!token) return;
    if (overviewOk && projectsOk && communicationOk) {
      setText("runtime-refresh-status", tr("Refreshed") + " " + new Date().toLocaleTimeString());
    } else {
      setText("runtime-refresh-status", tr("Refresh failed · showing previous data"));
    }
    if (recoverCollaboration && runtimeCollaborationNeedsRefreshRecovery(state)) {
      const collaborationRequest = runtimeCollaborationRequest(state);
      if (collaborationRequest) void startCollaboration(collaborationRequest);
    }
  } finally {
    setRefreshBusy(false);
  }
}

function refreshAutoSurfaces(): void {
  if (!token) return;
  if (document.hidden) {
    void refreshCommunication(false);
    return;
  }
  void fetchOverview(refreshRuntimeOverview(state));
  const request = refreshRuntimeSessionList(state);
  if (request) void fetchSessions(request);
  const windowRequest = refreshRuntimeProjectWindows(state);
  if (windowRequest) void fetchProjectWindows(windowRequest);
  void refreshCommunication(workspaceView === "operations");
}

function startAuto(): void {
  stopAuto();
  timer = window.setInterval(refreshAutoSurfaces, REFRESH_MS);
}
function stopAuto(): void { if (timer) window.clearInterval(timer); timer = 0; }

function startWindowAuto(): void {
  stopWindowAuto();
  if (!token || workspaceView !== "windows" || document.hidden) return;
  windowTimer = window.setInterval(() => void refreshWindows(true), WINDOW_REFRESH_MS);
}

function stopWindowAuto(): void {
  if (windowTimer) window.clearInterval(windowTimer);
  windowTimer = 0;
}

function connectRuntimeCredential(nextToken: string, rememberForTab: boolean): void {
  productWorkspace.reset(); productExtensions.reset(); productProjectApi.clearToken();
  rememberCredentialForTab = rememberForTab;
  token = nextToken;
  setRuntimeConnectionState("connecting");
  const remember = el("runtime-token-remember") as HTMLInputElement | null;
  if (remember) remember.checked = rememberForTab;
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
  const input = el("runtime-token-input") as HTMLInputElement | null;
  const remember = el("runtime-token-remember") as HTMLInputElement | null;
  const nextToken = input ? input.value.trim() : ""; if (input) input.value = "";
  if (!nextToken) { setText("runtime-token-error", tr("Enter your access key.")); return; }
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
  void consumeCommunicationDeliveries(
    communicationInbox.map((item) => String(item?.delivery_id || "")).filter(Boolean)
  );
});

el("runtime-device-select")?.addEventListener("change", () => {
  const select = el("runtime-device-select") as HTMLSelectElement | null; if (!select) return;
  applyRunnerFilter(select.value);
});
el("runtime-session-search")?.addEventListener("input", () => renderSessionList(sessionRows, sessionListMetaSnapshot));
el("runtime-message-search")?.addEventListener("input", filterCollaborationMessages);
el("runtime-project-search")?.addEventListener("input", () => {
  const input = el("runtime-project-search") as HTMLInputElement | null;
  projectSearch = input?.value || "";
  stopProjectSearchTimer();
  if (!token) return;
  setText("runtime-project-status", tr("Searching…"));
  projectSearchTimer = window.setTimeout(() => {
    projectSearchTimer = 0;
    if (!token) return;
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
  if (!(event instanceof KeyboardEvent) || event.key !== "Enter" || event.shiftKey || event.isComposing || event.keyCode === 229) return;
  const body = event.currentTarget as HTMLTextAreaElement | null;
  const send = el("runtime-message-send") as HTMLButtonElement | null;
  const form = el("runtime-collaboration-form") as HTMLFormElement | null;
  event.preventDefault();
  if (!body?.value.trim() || send?.disabled || !form) return;
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
  const inspector = document.querySelector(".runtime-inspector") as HTMLDetailsElement | null;
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
document.querySelectorAll<HTMLButtonElement>("[data-runtime-view]").forEach((button) => {
  button.addEventListener("click", () => applyWorkspaceView(parseWorkspaceViewPreference(button.dataset.runtimeView)));
});
document.querySelectorAll<HTMLButtonElement>("[data-context-target]").forEach((button) => {
  button.addEventListener("click", () => {
    document.querySelectorAll<HTMLButtonElement>("[data-context-target]").forEach((item) => {
      const selected = item === button;
      item.setAttribute("aria-pressed", String(selected));
      show(String(item.dataset.contextTarget), selected);
    });
  });
});
document.querySelectorAll<HTMLButtonElement>("[data-operations-target]").forEach((button) => {
  button.addEventListener("click", () => revealOperationsSection(String(button.dataset.operationsTarget || "runtime-operations-overview")));
});
document.querySelectorAll<HTMLButtonElement>("[data-language-toggle]").forEach((button) => {
  button.addEventListener("click", () => {
    applyLanguage(runtimeLanguage === "zh-CN" ? "en" : "zh-CN");
    closeAppearanceMenus(false);
    closeTopbarMore(false);
  });
});
document.querySelectorAll<HTMLButtonElement>("[data-theme-option]").forEach((button) => {
  button.addEventListener("click", () => {
    applyAppearance(parseAppearancePreference(button.dataset.themeOption));
    const menu = button.closest("details.theme-menu") as HTMLDetailsElement | null;
    if (menu) menu.open = false;
    closeTopbarMore(false);
  });
});
el("runtime-topbar-more")?.addEventListener("toggle", (event) => {
  const menu = event.currentTarget as HTMLDetailsElement | null;
  if (!menu?.open) return;
  closeComposerOptions(false);
  closeRuntimeInspector(false);
  setMobileNavigationOpen(false, false);
});
document.querySelectorAll<HTMLDetailsElement>("details.theme-menu").forEach((menu) => {
  menu.addEventListener("toggle", () => {
    if (!menu.open) return;
    closeAppearanceMenus(false, menu);
    closeRuntimeInspector(false);
    setMobileNavigationOpen(false, false);
  });
});
el("runtime-message-options")?.addEventListener("toggle", (event) => {
  const options = event.currentTarget as HTMLDetailsElement | null;
  if (!options?.open) return;
  closeAppearanceMenus(false);
  setMobileNavigationOpen(false, false);
});
document.addEventListener("pointerdown", (event) => {
  const target = event.target;
  if (!(target instanceof Node)) return;
  document.querySelectorAll<HTMLDetailsElement>("details.theme-menu[open]").forEach((menu) => {
    if (!menu.contains(target)) menu.open = false;
  });
  const options = el("runtime-message-options") as HTMLDetailsElement | null;
  if (options?.open && !options.contains(target)) options.open = false;
  const topbarMore = el("runtime-topbar-more") as HTMLDetailsElement | null;
  if (topbarMore?.open && !topbarMore.contains(target)) topbarMore.open = false;
});
el("runtime-jump-latest")?.addEventListener("click", jumpLatest);
el("runtime-timeline")?.addEventListener("scroll", () => {
  const node = el("runtime-timeline"); if (!node) return;
  updateWorkflowSessionFollowFromScroll(state.workflow, node.scrollTop, node.clientHeight, node.scrollHeight); syncFollowUi();
});
document.querySelector(".runtime-inspector")?.addEventListener("toggle", (event) => {
  const inspector = event.currentTarget as HTMLDetailsElement | null;
  if (!inspector) return;
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
  if (document.querySelector("#runtime-command-dialog[open]")) return;
  if (!event.isComposing && !event.altKey && !event.shiftKey && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k" && shell && !shell.hidden) {
    event.preventDefault();
    focusProjectNavigation();
    return;
  }
  const inspector = document.querySelector(".runtime-inspector") as HTMLDetailsElement | null;
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
  if (event.key !== "Tab" || !shell?.classList.contains("mobile-nav-open")) return;
  const sidebar = el("runtime-sidebar");
  if (!sidebar) return;
  const focusable = visibleFocusableElements(sidebar);
  if (!focusable.length) return;
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
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
  if (!token) return;
  refreshAutoSurfaces();
  if (workspaceView === "windows") {
    void refreshWindows(true);
    startWindowAuto();
  }
});
const syncSystemAppearance = () => {
  if (parseAppearancePreference(document.documentElement.dataset.theme) === "system") applyAppearance("system", false);
};
if (typeof appearanceMedia.addEventListener === "function") appearanceMedia.addEventListener("change", syncSystemAppearance);
else appearanceMedia.addListener(syncSystemAppearance);
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
