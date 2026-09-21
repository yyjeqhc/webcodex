import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
} from "../dist/runtime_collaboration_state.js";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  filterAndSortRuntimeProjects,
  runtimeProjectIdentityText,
  preferredRuntimeProjectSelection,
  runtimeWorkflowSessionSummaryRevision,
  runtimeWorkflowSessionSummaryChanged,
  beginRuntimeCredential,
  refreshRuntimeOverview,
  isCurrentRuntimeOverviewRequest,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  refreshRuntimeRunner,
  isCurrentRuntimeRunnerRequest,
  selectRuntimeRunnerFilter,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  refreshRuntimeProjectWindows,
  isCurrentRuntimeProjectWindowsRequest,
  selectRuntimeWorkflowSession,
  selectRuntimeSessionLocation,
  refreshRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  resolveRunnerDisclosure,
  runtimeWindowShortKey,
  runtimeWindowActivityLabel,
} from "../dist/runtime_console_state.js";

test("window activity presentation is hashed-id safe and WebPi-specific", () => {
  assert.equal(runtimeWindowShortKey("0123456789abcdef0123456789abcdef"), "01234567…cdef");
  assert.equal(runtimeWindowShortKey("short"), "short");
  assert.equal(runtimeWindowActivityLabel(null, 10_000), "No WebPi activity");
  assert.equal(runtimeWindowActivityLabel(9_500, 10_000), "just now");
  assert.equal(runtimeWindowActivityLabel(5_000, 10_000), "5s ago");
  assert.equal(runtimeWindowActivityLabel(0, 10_000), "No WebPi activity");
});

test("Workflow Session summary revision changes only for detail-relevant list state", () => {
  const base = {
    session_id: "wc_sess_a",
    title: "Work",
    lifecycle: "active",
    mode: "normal",
    updated_at: 100,
    running_call: false,
    running_jobs: 0,
    running_jobs_complete: true,
    current_activity: null,
    last_activity: { kind: "Edited", summary: "file" },
    overview: { attention: { open_todos: 0 } },
  };
  assert.equal(runtimeWorkflowSessionSummaryRevision(null), "");
  assert.equal(runtimeWorkflowSessionSummaryChanged(base, { ...base }), false);
  assert.equal(runtimeWorkflowSessionSummaryChanged(base, { ...base, updated_at: 101 }), true);
  assert.equal(runtimeWorkflowSessionSummaryChanged(base, { ...base, running_jobs: 1 }), true);
  assert.equal(
    runtimeWorkflowSessionSummaryChanged(base, {
      ...base,
      overview: { attention: { open_todos: 1 } },
    }),
    true
  );
});

test("runtime credential and project generations fence stale project responses", () => {
  const state = initialRuntimeConsoleState();
  const firstProjects = beginRuntimeCredential(state);
  const newerProjects = refreshRuntimeProjects(state);
  assert.equal(firstProjects.clientId, "");
  assert.equal(firstProjects.query, "");
  assert.equal(isCurrentRuntimeProjectsRequest(state, firstProjects), false);
  assert.equal(isCurrentRuntimeProjectsRequest(state, newerProjects), true);

  const listA = selectRuntimeProject(state, "device-a", "agent:a:project");
  assert.equal(state.selectedDevice, "device-a");
  assert.equal(state.selectedProject, "agent:a:project");
  const projectsDuringA = refreshRuntimeProjects(state, "  webcodex  ");
  assert.equal(projectsDuringA.clientId, "device-a");
  assert.equal(projectsDuringA.query, "webcodex");
  const fleetProjectsDuringA = refreshRuntimeProjects(state, "", "");
  assert.equal(fleetProjectsDuringA.clientId, "");
  const refreshedA = refreshRuntimeSessionList(state);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), true);

  const windowReqA = refreshRuntimeProjectWindows(state);
  assert.equal(windowReqA.project, "agent:a:project");
  assert.equal(isCurrentRuntimeProjectWindowsRequest(state, windowReqA), true);
  const refreshedWindowA = refreshRuntimeProjectWindows(state);
  assert.equal(isCurrentRuntimeProjectWindowsRequest(state, windowReqA), false);
  assert.equal(isCurrentRuntimeProjectWindowsRequest(state, refreshedWindowA), true);

  const listB = selectRuntimeProject(state, "device-b", "agent:b:project");
  assert.equal(state.selectedDevice, "device-b");
  assert.equal(state.selectedProject, "agent:b:project");
  assert.equal(isCurrentRuntimeProjectsRequest(state, projectsDuringA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listB), true);
  assert.equal(isCurrentRuntimeProjectWindowsRequest(state, refreshedWindowA), false);
  const windowReqB = refreshRuntimeProjectWindows(state);
  assert.equal(windowReqB.project, "agent:b:project");
  assert.equal(isCurrentRuntimeProjectWindowsRequest(state, windowReqB), true);
  assert.equal(state.workflow.selectedSessionId, "");
  assert.equal(state.workflow.snapshot, null);
});

test("server and Runner requests are fenced across credential and Runner changes", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  const overviewA = refreshRuntimeOverview(state);
  const listA = selectRuntimeProject(state, "runner-a", "agent:runner-a:p");
  const runnerA = refreshRuntimeRunner(state);
  assert.equal(isCurrentRuntimeOverviewRequest(state, overviewA), true);
  assert.equal(isCurrentRuntimeRunnerRequest(state, runnerA), true);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listA), true);

  selectRuntimeProject(state, "runner-b", "agent:runner-b:p");
  const runnerB = refreshRuntimeRunner(state);
  assert.equal(isCurrentRuntimeRunnerRequest(state, runnerA), false);
  assert.equal(isCurrentRuntimeRunnerRequest(state, runnerB), true);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listA), false);

  beginRuntimeCredential(state);
  assert.equal(isCurrentRuntimeOverviewRequest(state, overviewA), false);
  assert.equal(isCurrentRuntimeRunnerRequest(state, runnerB), false);
});

test("runtime home defaults to All Runners with no automatic Project selection", () => {
  const projects = [
    { id: "opaque-b", client_id: "device-b", name: "Beta" },
    { id: "agent:not-device-a:project", client_id: "device-a", name: "Zulu" },
    { id: "opaque-a2", client_id: "device-a", name: "Alpha" },
    { id: "opaque-a1", client_id: "device-a", name: "Alpha" },
  ];
  assert.deepEqual(runtimeDeviceIds(projects), ["device-a", "device-b"]);
  assert.deepEqual(runtimeProjectsForDevice(projects, "device-a").map((project) => project.id), ["opaque-a1", "opaque-a2", "agent:not-device-a:project"]);
  assert.deepEqual(runtimeProjectsForDevice(projects, "").map((project) => project.id), ["opaque-a1", "opaque-a2", "opaque-b", "agent:not-device-a:project"]);
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-b", "agent:not-device-a:project"), { device: "device-a", project: "agent:not-device-a:project" });
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-a", "missing-project"), { device: "device-a", project: "" });
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "", ""), { device: "", project: "" });
});

test("runtime refresh preserves an authorized selected device and project", () => {
  const projects = [
    { id: "project-2", client_id: "device-z", name: "Second" },
    { id: "project-1", client_id: "device-z", name: "First" },
    { id: "project-3", client_id: "device-a", name: "Other" },
  ];
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-z", "project-2"), { device: "device-z", project: "project-2" });
});

test("Project list supports All Runners, Runner filter/search, and running attention recent ranking", () => {
  const projects = [
    { id: "agent:r:idle", client_id: "runner", name: "Idle", path: "/root/git/idle", sessions: { running_sessions: 0, attention: {}, latest_updated_at: 100 } },
    { id: "agent:r:recent", client_id: "runner", name: "Recent", path: "/root/git/webcodex-worktrees/recent", sessions: { running_sessions: 0, attention: {}, latest_updated_at: 400 } },
    { id: "agent:r:attention", client_id: "runner", name: "Needs review", sessions: { running_sessions: 0, attention: { open_guidance: 1 }, latest_updated_at: 50 } },
    { id: "agent:r:working", client_id: "runner", name: "Working", sessions: { running_sessions: 1, attention: {}, latest_updated_at: 10 } },
    { id: "agent:other:x", client_id: "other", name: "External" },
  ];
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "runner", "").map((project) => project.id),
    ["agent:r:working", "agent:r:attention", "agent:r:recent", "agent:r:idle"]
  );
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "runner", "REVIEW").map((project) => project.id),
    ["agent:r:attention"]
  );
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "runner", "agent:r:recent").map((project) => project.id),
    ["agent:r:recent"]
  );
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "runner", "webcodex-worktrees").map((project) => project.id),
    ["agent:r:recent"]
  );
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "", "").map((project) => project.id),
    ["agent:r:working", "agent:r:attention", "agent:r:recent", "agent:r:idle", "agent:other:x"]
  );
  assert.deepEqual(
    filterAndSortRuntimeProjects(projects, "", "OTHER").map((project) => project.id),
    ["agent:other:x"]
  );
});

test("Runtime Project identity preserves Linux macOS and Windows workspace paths exactly", () => {
  assert.equal(
    runtimeProjectIdentityText({ id: "agent:special:webcodex", client_id: "special", path: "/root/git/webcodex" }),
    "Runner: special · Project: agent:special:webcodex · Workspace: /root/git/webcodex"
  );
  assert.equal(
    runtimeProjectIdentityText({ id: "agent:mini:webcodex", client_id: "mini", path: "/Users/demo/git/webcodex" }),
    "Runner: mini · Project: agent:mini:webcodex · Workspace: /Users/demo/git/webcodex"
  );
  assert.equal(
    runtimeProjectIdentityText({ id: "agent:msi:webcodex", client_id: "msi", path: "E:\\git\\webcodex" }),
    "Runner: msi · Project: agent:msi:webcodex · Workspace: E:\\git\\webcodex"
  );
});

test("runtime workflow detail identity includes project plus session id", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "device-a", "agent:a:project");
  const detailA = selectRuntimeWorkflowSession(state, "wc_sess_same");
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailA), true);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailA, { title: "A" }), true);
  assert.equal(state.workflow.snapshot.title, "A");

  selectRuntimeProject(state, "device-b", "agent:b:project");
  assert.equal(state.workflow.snapshot, null);
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailA), false);
  const detailB = selectRuntimeWorkflowSession(state, "wc_sess_same");
  assert.equal(detailB.project, "agent:b:project");
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailB), true);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailA, { title: "late A" }), false);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailB, { title: "B" }), true);
  assert.equal(state.workflow.snapshot.title, "B");
});

test("Recent Session navigation atomically establishes Runner Project and Session and fences old collaboration", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner-a", "agent:runner-a:project-a");
  selectRuntimeWorkflowSession(state, "wc_sess_a");
  const oldDetail = refreshRuntimeWorkflowSession(state);
  const oldCollaboration = runtimeCollaborationRequest(state);

  const location = selectRuntimeSessionLocation(
    state,
    "runner-b",
    "agent:runner-b:project-b",
    "wc_sess_b"
  );
  assert.equal(state.selectedDevice, "runner-b");
  assert.equal(state.selectedProject, "agent:runner-b:project-b");
  assert.equal(state.workflow.selectedSessionId, "wc_sess_b");
  assert.equal(isCurrentRuntimeSessionListRequest(state, location.sessionListRequest), true);
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, location.detailRequest), true);
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, oldDetail), false);
  assert.equal(isCurrentRuntimeCollaborationRequest(state, oldCollaboration), false);
  const currentCollaboration = runtimeCollaborationRequest(state);
  assert.equal(currentCollaboration.project, "agent:runner-b:project-b");
  assert.equal(currentCollaboration.sessionId, "wc_sess_b");
});

test("removed Project clears stale detail while preserving a still-valid Runner filter", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner", "agent:runner:gone");
  const detail = selectRuntimeWorkflowSession(state, "wc_sess_gone");
  const remaining = [{ id: "agent:runner:remaining", client_id: "runner", name: "Remaining" }];
  const selection = preferredRuntimeProjectSelection(remaining, state.selectedDevice, state.selectedProject);
  assert.deepEqual(selection, { device: "runner", project: "" });
  selectRuntimeRunnerFilter(state, selection.device);
  assert.equal(state.selectedDevice, "runner");
  assert.equal(state.selectedProject, "");
  assert.equal(state.workflow.selectedSessionId, "");
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detail), false);
});

test("session switch invalidates old collaboration responses", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner", "agent:runner:project");
  selectRuntimeWorkflowSession(state, "wc_sess_a");
  const requestA = runtimeCollaborationRequest(state);
  assert.equal(isCurrentRuntimeCollaborationRequest(state, requestA), true);
  selectRuntimeWorkflowSession(state, "wc_sess_b");
  const requestB = runtimeCollaborationRequest(state);
  assert.equal(isCurrentRuntimeCollaborationRequest(state, requestA), false);
  assert.equal(isCurrentRuntimeCollaborationRequest(state, requestB), true);
});


test("runtime collaboration rendering uses textContent and explicitly reloads on history loss", async () => {
  const source = await readFile(new URL("../src/runtime.ts", import.meta.url), "utf8");
  const navigationSource = await readFile(new URL("../src/runtime_navigation.ts", import.meta.url), "utf8");
  const collaborationSource = await readFile(new URL("../src/runtime_collaboration.ts", import.meta.url), "utf8");
  const html = await readFile(new URL("../src/runtime.html", import.meta.url), "utf8");
  const css = await readFile(new URL("../src/runtime.css", import.meta.url), "utf8");
  assert.equal(html.includes("runtime-project-" + "select"), false);
  assert.match(html, /runtime-project-list/);
  assert.match(html, /runtime-project-search/);
  assert.match(html, /runtime-token-remember/);
  assert.match(html, /data-theme-option="system"/);
  assert.match(html, /data-theme-option="light"/);
  assert.match(html, /data-theme-option="dark"/);
  assert.match(html, /runtime-mobile-nav-toggle/);
  assert.match(html, /runtime-mobile-nav-close/);
  assert.match(html, /runtime-mobile-nav-backdrop/);
  assert.match(html, /runtime-inspector-backdrop/);
  assert.match(html, /data-runtime-view="sessions"/);
  assert.match(html, /data-runtime-view="operations"/);
  assert.match(html, /runtime-operations-stage/);
  assert.match(html, /runtime-operations-overview/);
  assert.match(html, /runtime-operations-runners/);
  assert.match(html, /runtime-operations-agents/);
  assert.match(html, /runtime-session-workspace/);
  assert.match(html, /runtime-workflow-sessions-panel/);
  assert.match(html, /runtime-session-id/);
  assert.match(html, /runtime-session-created/);
  assert.match(html, /runtime-session-updated/);
  assert.match(html, /runtime-session-context-lifecycle/);
  assert.match(html, /runtime-session-context-mode/);
  assert.match(css, /\.session-identity/);
  assert.match(html, /class="context-navigation"/);
  assert.match(html, /data-context-target="runtime-context-activity"/);
  assert.match(html, /workspace path/);
  assert.match(html, /class="recent-panel-title">Recent Sessions<\/span>/);
  assert.match(html, /id="runtime-inspector-close"[^>]*aria-label="Close session context"/);
  assert.match(html, /runtime-recent-session-list/);
  assert.match(html, /Runner Fleet/);
  assert.match(html, /runtime-runner-list/);
  assert.match(html, /All Runners/);
  assert.match(html, /runtime-collaboration-form/);
  assert.match(html, /runtime-collaboration-board[^>]*role="log"/);
  assert.match(html, /runtime-message-announcer[^>]*aria-live="polite"/);
  assert.match(html, /runtime-new-messages/);
  assert.match(html, /id="runtime-chat-scroll" class="chat-scroll"/);
  assert.match(html, /id="runtime-message-body" rows="1" maxlength="4000" enterkeyhint="send"/);
  assert.match(html, /runtime-message-options/);
  assert.match(html, /composer-options-popover/);
  assert.match(html, /runtime-collaboration-empty-title/);
  assert.match(html, /runtime-collaboration-empty-copy/);
  assert.match(html, /runtime-message-requires-ack/);
  assert.match(css, /-webkit-line-clamp:\s*4/);
  assert.match(css, /\.recent-session-row/);
  assert.match(css, /\.fleet-row/);
  assert.match(css, /\.device-group/);
  assert.match(css, /@media \(max-width: 900px\)/);
  assert.match(css, /@media \(min-width: 1600px\)/);
  assert.match(css, /--context-rail-width:\s*clamp\(320px,\s*26vw,\s*420px\)/);
  assert.match(css, /\.runtime-shell\.context-docked\s*\{[^}]*--content-width:\s*760px[^}]*grid-template-columns:\s*var\(--sidebar-width\)\s+minmax\(0,\s*1fr\)\s+var\(--context-rail-width\)/);
  assert.match(css, /translateX\(-102%\)/);
  assert.match(css, /env\(safe-area-inset-bottom\)/);
  assert.match(css, /env\(safe-area-inset-top\)/);
  assert.match(css, /@media \(pointer: coarse\)/);
  assert.match(css, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(css, /data-resolved-theme="light"/);
  assert.match(css, /backdrop-filter/);
  assert.match(css, /--ambient-three/);
  assert.match(css, /\.workspace-topbar\s*\{[^}]*z-index:\s*40/);
  assert.match(css, /\.runtime-shell\s*\{[^}]*gap:\s*0[^}]*padding:\s*0/);
  assert.match(css, /\.workspace-main\s*\{[^}]*background:\s*var\(--page-surface\)/);
  assert.match(css, /--layout-major:\s*61\.8%/);
  assert.match(css, /--layout-minor:\s*38\.2%/);
  assert.match(css, /--sidebar-width:\s*clamp\(280px,\s*21vw,\s*320px\)/);
  assert.match(css, /--content-width:\s*760px/);
  assert.match(css, /\.message-card\.message-incoming\s*\{[^}]*width:\s*fit-content[^}]*max-width:\s*min\(82%,\s*880px\)/);
  assert.match(css, /\.message-card\.message-neutral\s*\{[^}]*width:\s*fit-content[^}]*max-width:\s*min\(82%,\s*880px\)/);
  assert.match(css, /\.message-card\.message-outgoing\s*\{[^}]*max-width:\s*min\(68%,\s*680px\)[^}]*align-self:\s*flex-end/);
  assert.match(css, /--message-bubble-radius:\s*22px/);
  assert.doesNotMatch(css, /message-avatar/);
  assert.doesNotMatch(css, /--message-bubble-anchor-radius/);
  assert.match(css, /\.message-card\.message-incoming \.message-bubble\s*\{[^}]*border:\s*0[^}]*border-radius:\s*var\(--message-bubble-radius\)/);
  assert.match(css, /\.message-card\.message-outgoing \.message-bubble\s*\{[^}]*border:\s*0[^}]*border-radius:\s*var\(--message-bubble-radius\)/);
  assert.match(css, /\.project-row\.selected\s*\{[^}]*background:\s*var\(--sidebar-selected\)/);
  assert.match(css, /\.device-project-list\s*\{[^}]*border-left:\s*0/);
  assert.match(css, /\.project-row-state/);
  assert.match(css, /--message-incoming-bg:\s*rgba\(255,\s*255,\s*255,\s*\.055\)/);
  assert.match(css, /--message-outgoing-bg:\s*#285487/);
  assert.match(css, /data-resolved-theme="light"[\s\S]*--message-incoming-bg:\s*rgba\(25,\s*32,\s*45,\s*\.055\)/);
  assert.match(css, /data-resolved-theme="light"[\s\S]*--message-outgoing-bg:\s*#2866a8/);
  assert.match(css, /\.message-card\.message-incoming \.message-bubble\s*\{[^}]*background:\s*var\(--message-incoming-bg\)/);
  assert.match(css, /\.message-card\.message-outgoing \.message-bubble\s*\{[^}]*background:\s*var\(--message-outgoing-bg\)/);
  assert.match(css, /\.message-footer\s*\{[^}]*display:\s*flex/);
  assert.match(css, /\.collaboration-composer:focus-within/);
  assert.match(css, /\.composer-options-popover/);
  assert.match(css, /\.message-code-copy/);
  assert.match(css, /\.message-date-separator/);
  assert.match(css, /\.new-messages-button/);
  assert.match(css, /\.operations-stage/);
  assert.match(css, /\.topbar-more-popover/);
  assert.match(css, /@keyframes composer-enter/);
  assert.match(css, /\.message-card \+ \.message-card/);
  assert.match(css, /\.session-card::before/);
  assert.equal(source.includes("innerHTML"), false);
  assert.match(source, /APPEARANCE_STORAGE_KEY/);
  assert.match(source, /applyAppearance/);
  assert.doesNotMatch(source, /api\("runner"/);
  assert.match(source, /selectRuntimeSessionLocation/);
  assert.doesNotMatch(source, /project-row-path/);
  assert.match(navigationSource, /runtimeProjectIdentityText\(project\)/);
  assert.match(collaborationSource, /runtimeCollaborationMessageSides\(messages, options\.locallyAuthoredIds\)/);
  assert.match(collaborationSource, /provenance-unknown/);
  assert.match(source, /sessionListMetaSnapshot = \{\s*total: typeof response\.data\.total === "number" \? Math\.max\(sessionRows\.length, response\.data\.total\) : sessionRows\.length,\s*truncated: !!response\.data\.truncated,\s*\}/);
  assert.match(source, /runtime-session-search"\)\?\.addEventListener\("input", \(\) => renderSessionList\(sessionRows, sessionListMetaSnapshot\)\)/);
  assert.match(source, /rememberLocalCollaborationMessage/);
  assert.match(collaborationSource, /message-group-continuation/);
  assert.match(source, /syncCollaborationComposerLayout/);
  assert.match(source, /scrollCollaborationToLatest/);
  assert.match(source, /scroll\.scrollTo\(\{ top: scroll\.scrollHeight, behavior \}\)/);
  assert.match(source, /firstRetainedRender \|\| \(hasNewMessages && shouldFollowNewMessages\)/);
  assert.match(source, /collaborationFollowLatest \|\| chatIsNearLatest\(\)/);
  assert.match(source, /collaborationPendingMessages \+= newMessageIds\.length/);
  assert.match(collaborationSource, /appendRichMessage/);
  assert.match(source, /DRAFT_STORAGE_PREFIX/);
  assert.match(source, /WORKSPACE_VIEW_STORAGE_KEY/);
  assert.doesNotMatch(source, /window\.matchMedia\("\(pointer: fine\)"\)/);
  assert.match(source, /event\.shiftKey \|\| event\.isComposing \|\| event\.keyCode === 229/);
  assert.match(source, /form\.requestSubmit\(\)/);
  assert.match(collaborationSource, /message-entering/);
  assert.match(collaborationSource, /Acknowledgement required/);
  assert.doesNotMatch(source, /className = "message-links"/);
  assert.match(source, /renderSessionWorkspaceIdentity\(\)/);
  assert.match(source, /function revealWorkflowSessionDetail[\s\S]*scrollIntoView\(\{ block: "start", inline: "nearest" \}\)/);
  assert.match(source, /setText\("runtime-session-id", String\(detail\.session_id/);
  assert.match(source, /setText\("runtime-session-created", dateTimeLabel\(detail\.created_at\)\)/);
  assert.match(source, /setText\("runtime-session-updated", dateTimeLabel\(detail\.updated_at\)\)/);
  const recentStart = navigationSource.indexOf("function renderRecentSessionRows");
  const recentEnd = navigationSource.length;
  const recentRender = navigationSource.slice(recentStart, recentEnd);
  assert.match(recentRender, /formatLivenessPresentation\(session/);
  assert.match(recentRender, /attentionLabel\(session\.overview\?\.attention\)/);
  assert.match(recentRender, /updatedLabel\(session\.updated_at\)/);
  const recentSelectStart = source.indexOf("function selectRecentSession");
  const recentSelectEnd = source.indexOf("async function fetchSessions", recentSelectStart);
  assert.match(source.slice(recentSelectStart, recentSelectEnd), /revealWorkflowSessionDetail\(\)/);
  assert.doesNotMatch(recentRender, /workflowSessionListOverviewFacts|summary-facts|validation/);
  assert.doesNotMatch(recentRender, /\.sort\(/);
  assert.match(source, /applyRunnerFilter\(select\.value\)/);
  assert.match(source, /void fetchOverview\(refreshRuntimeOverview\(state\)\)/);
  assert.match(source, /const REFRESH_MS = 30000;/);
  assert.doesNotMatch(source, /every 8 seconds|每 8 秒|REFRESH_MS = 8000/);
  const autoRefreshStart = source.indexOf("function refreshAutoSurfaces");
  const autoRefreshEnd = source.indexOf("function connectRuntimeCredential", autoRefreshStart);
  const autoRefresh = source.slice(autoRefreshStart, autoRefreshEnd);
  assert.match(autoRefresh, /document\.hidden[\s\S]*refreshCommunication\(false\)/);
  assert.match(autoRefresh, /refreshCommunication\(workspaceView === "operations"\)/);
  assert.match(autoRefresh, /window\.setInterval\(refreshAutoSurfaces, REFRESH_MS\)/);
  assert.match(collaborationSource, /appendRichMessage\(bubble, message\?\.message\)/);
  assert.match(source, /action === "reload"[\s\S]*loadRetainedCollaboration/);
  assert.match(source, /action === "drain"/);
  assert.match(source, /abortCollaboration\(\)/);
  assert.match(source, /workflow-session-post-message/);
  assert.match(source, /workflow-session-withdraw-message/);
  assert.match(source, /workflow-session-replace-message/);
  const collaborationRenderStart = source.indexOf("function renderCollaboration");
  const collaborationRenderEnd = source.indexOf("async function confirmCollaborationMutationDurability", collaborationRenderStart);
  const collaborationRender = source.slice(collaborationRenderStart, collaborationRenderEnd);
  assert.doesNotMatch(collaborationRender, /messageSide === "outgoing" && runtimeCollaborationMessageCanMutate/);
  assert.match(source, /sessionCollaborationAuthorityFailure/);
  assert.match(source, /response\?\.status !== 403/);
  assert.match(source, /This credential can still read the Session; add session:collaborate to send, edit, or withdraw messages\./);
  assert.match(source, /show\("runtime-collaboration-form", true\)/);
  assert.match(source, /show\("runtime-collaboration-board", messages\.length > 0\)/);
  assert.match(source, /Conversation access requires runtime:read/);
  assert.match(source, /setHumanJoinSendEnabled\(false\)/);
  assert.match(source, /function setMobileNavigationOpen/);
  assert.match(source, /function syncResponsiveNavigation/);
  assert.match(source, /WIDE_CONTEXT_MEDIA/);
  assert.match(source, /classList\.toggle\("context-docked", resolved\.isDocked\)/);
  assert.match(source, /inspector\.open = resolved\.visible/);
  assert.match(source, /event\.key === "Escape"/);
  assert.match(source, /visibleFocusableElements\(sidebar\)/);
  assert.match(source, /Reply target selected\. Your next message will reply to /);
  assert.match(source, /body\?\.focus\(\)/);
  assert.match(source, /state\.collaboration\.phase === "live" && !state\.collaboration\.uncertainMutation/);
  assert.match(collaborationSource, /Withdraw this retained message; history is preserved\./);
  assert.match(collaborationSource, /Replace this retained message while preserving its history\./);
  assert.match(source, /kind\?\.value === "guidance"/);
  assert.match(source, /priority\?\.value !== "high"/);
  assert.match(collaborationSource, /First ACK observed/);
  assert.doesNotMatch(collaborationSource, /Delivered|Read by model|Currently acknowledged/);
  assert.match(source, /Refresh failed · showing previous data/);
  assert.match(source, /runtimeCollaborationNeedsRefreshRecovery/);
  assert.match(source, /signature === renderedCollaborationSignature/);
  const communicationRefreshStart = source.indexOf("async function performCommunicationRefresh");
  const communicationRefreshEnd = source.indexOf("async function createCommunicationAgent", communicationRefreshStart);
  const communicationRefresh = source.slice(communicationRefreshStart, communicationRefreshEnd);
  assert.match(communicationRefresh, /!includeData \|\| communicationReadAvailable === false/);
  assert.match(communicationRefresh, /fetchCommunicationAgents\(generation, false\)/);
  assert.match(communicationRefresh, /fetchCommunicationConversations\(generation, false\)/);
  assert.match(communicationRefresh, /fetchCommunicationConversation\(generation, false\)/);
  assert.match(communicationRefresh, /fetchCommunicationInbox\(generation, false\)/);
  assert.match(communicationRefresh, /communicationRefreshCoordinator\.refresh\(includeData\)/);
  assert.doesNotMatch(source, /communicationRefreshInFlight/);
  assert.match(source, /operations && token\) void refreshCommunication\(true\)/);
  assert.match(source, /visibilitychange/);
  const renderProjectsStart = source.indexOf("function renderProjectSelectors");
  const renderProjectsEnd = source.indexOf("function switchProject", renderProjectsStart);
  const renderProjects = source.slice(renderProjectsStart, renderProjectsEnd);
  assert.match(renderProjects, /signature === renderedProjectSelectorsSignature/);
  assert.match(renderProjects, /switchProject\(clientId, projectId\)/);

  const renderProjectsTreeStart = navigationSource.indexOf("function renderProjectSelectorTree");
  const renderProjectsTreeEnd = navigationSource.indexOf("function renderRunnerFleetRows", renderProjectsTreeStart);
  const renderProjectsTree = navigationSource.slice(renderProjectsTreeStart, renderProjectsTreeEnd);
  assert.match(renderProjectsTree, /document\.createElement\("summary"\)/);
  assert.match(renderProjectsTree, /workspace\.addEventListener\("toggle"/);
  assert.doesNotMatch(renderProjectsTree, /addEventListener\("keydown"/);
  assert.match(renderProjectsTree, /all\.textContent = tr\("All Runners"\)/);
  assert.match(renderProjectsTree, /project-row-signals/);
  assert.match(renderProjectsTree, /project-row-meta/);
  assert.match(renderProjectsTree, /scan partial/);
  assert.match(renderProjectsTree, /row\.title = \[projectName, projectId/);
  assert.match(renderProjectsTree, /projectsByDevice/);
  assert.match(renderProjectsTree, /options\.projectDeviceFilter/);
  assert.match(renderProjectsTree, /workspace\.appendChild\(sessionsPanel\)/);
  assert.match(renderProjectsTree, /deviceMeta\.textContent = tr\(status\) \+ " · " \+ countLabel\(deviceProjects\.length, "Project"\)/);
  assert.match(collaborationSource, /appendRichMessage\(bubble, message\?\.message\);\s*content\.appendChild\(bubble\)/);
  assert.match(collaborationSource, /footer\.appendChild\(actions\);\s*content\.appendChild\(footer\);\s*card\.appendChild\(content\)/);
  assert.doesNotMatch(collaborationSource, /message-avatar/);
  assert.match(collaborationSource, /createMessageAction\(tr\("Reply"\), "reply"/);
  assert.match(navigationSource, /projectIcon\.appendChild\(runtimeIcon\("folder"\)\)/);
  assert.match(await readFile(new URL("../src/runtime_workspace.ts", import.meta.url), "utf8"), /icon\.appendChild\(runtimeIcon\("message"\)\)/);
  const renderRunnersStart = navigationSource.indexOf("function renderRunnerFleetRows");
  const renderRunnersEnd = navigationSource.indexOf("function renderRecentSessionRows", renderRunnersStart);
  const renderRunners = navigationSource.slice(renderRunnersStart, renderRunnersEnd);
  assert.match(renderRunners, /projects_scan_partial/);
  assert.match(renderRunners, /Projects scanned/);
  assert.match(renderRunners, /fleet scan partial/);
  assert.match(renderRunners, /Session scan partial/);
  assert.doesNotMatch(renderRunners, /visible_project_count/);
  const fetchProjectsStart = source.indexOf("async function fetchProjects");
  const fetchProjectsEnd = source.indexOf("function effectiveProjects", fetchProjectsStart);
  assert.doesNotMatch(source.slice(fetchProjectsStart, fetchProjectsEnd), /fetchOverview\(/);
  const fetchProjects = source.slice(fetchProjectsStart, fetchProjectsEnd);
  assert.match(fetchProjects, /payload\.client_id = clientId/);
  const fetchSessionsStart = source.indexOf("async function fetchSessions");
  const fetchSessionsEnd = source.indexOf("function updatedLabel", fetchSessionsStart);
  const fetchSessions = source.slice(fetchSessionsStart, fetchSessionsEnd);
  assert.match(fetchSessions, /runtimeWorkflowSessionSummaryChanged\(previousSelected, nextSelected\)/);
  assert.match(fetchSessions, /!state\.workflow\.snapshot \|\| runtimeWorkflowSessionSummaryChanged/);
  assert.match(fetchProjects, /payload\.query = query/);
  assert.match(fetchProjects, /if \(query\) \{[\s\S]*renderProjectSelectors\(projectRows, projectRowsTruncated\);[\s\S]*return true;/);
  assert.match(fetchProjects, /currentProject && projectRowsTruncated/);
  assert.match(fetchProjects, /projectRowsTotal = Math\.max\(projectRows\.length, reportedTotal\)/);
  assert.match(renderProjects, /formatProjectStatusText/);
  assert.match(navigationSource, /matching Projects shown/);
  assert.match(navigationSource, /visible Projects shown/);
  const applyRunnerStart = source.indexOf("function applyRunnerFilter");
  const applyRunnerEnd = source.indexOf("function runnerAttentionCount", applyRunnerStart);
  const applyRunner = source.slice(applyRunnerStart, applyRunnerEnd);
  assert.match(applyRunner, /projectDeviceFilter = device/);
  assert.match(applyRunner, /fetchProjects\(refreshRuntimeProjects\(state, projectSearch, projectDeviceFilter\)\)/);
  const searchHandlerStart = source.indexOf('el("runtime-project-search")?.addEventListener("input"');
  const searchHandlerEnd = source.indexOf('el("runtime-message-kind")', searchHandlerStart);
  const searchHandler = source.slice(searchHandlerStart, searchHandlerEnd);
  assert.match(searchHandler, /window\.setTimeout/);
  assert.match(searchHandler, /PROJECT_SEARCH_DEBOUNCE_MS/);
  assert.match(searchHandler, /fetchProjects\(refreshRuntimeProjects\(state, projectSearch, projectDeviceFilter\)\)/);
  const selectStart = source.indexOf("function selectSession");
  const selectEnd = source.indexOf("async function fetchSessionDetail", selectStart);
  assert.match(source.slice(selectStart, selectEnd), /setHumanJoinSendEnabled\(false\)[\s\S]*startCollaboration/);
  assert.match(source.slice(selectStart, selectEnd), /revealWorkflowSessionDetail\(\)/);
  const postStart = source.indexOf("async function postHumanCollaborationMessage");
  const postEnd = source.indexOf("function setRefreshBusy", postStart);
  const post = source.slice(postStart, postEnd);
  assert.match(post, /if \(!isCurrentRuntimeCollaborationRequest\(state, request\)\) return;\s*if \(response\?\.status === 0\)[\s\S]*return;\s*\}[\s\S]*if \(send\) send\.disabled = false;/);
  assert.match(post, /Send outcome unknown\. Refresh and review retained messages before retrying\./);
  assert.match(post, /abortCollaboration\(\)[\s\S]*setRuntimeCollaborationPhase\(state, request, "paused"\)/);
  const refreshStart = source.indexOf("async function refreshAll");
  const refreshEnd = source.indexOf("function refreshAutoSurfaces", refreshStart);
  assert.equal((source.slice(refreshStart, refreshEnd).match(/fetchOverview\(/g) || []).length, 1);
  assert.match(source.slice(refreshStart, refreshEnd), /refreshCommunication\(\)/);
  const runnerFilterStart = source.indexOf('el("runtime-device-select")?.addEventListener("change"');
  const runnerFilterEnd = source.indexOf('el("runtime-project-search")', runnerFilterStart);
  const runnerFilter = source.slice(runnerFilterStart, runnerFilterEnd);
  assert.match(runnerFilter, /applyRunnerFilter\(select\.value\)/);
  assert.doesNotMatch(runnerFilter, /switchProject|fetchRunner/);
  const bootstrapStart = source.indexOf("async function loadRetainedCollaboration");
  const bootstrapEnd = source.indexOf("async function startCollaboration", bootstrapStart);
  const bootstrap = source.slice(bootstrapStart, bootstrapEnd);
  const baselineAt = bootstrap.indexOf('api("workflow-session-observe"');
  const retainedListAt = bootstrap.indexOf('api("workflow-session-messages"');
  assert.ok(baselineAt >= 0 && retainedListAt > baselineAt, "live baseline must precede the retained snapshot to avoid a lost-update gap");
  assert.match(bootstrap, /runtimeCollaborationMutationRecovery\(state, request\)/);
  assert.match(bootstrap, /confirmCollaborationMutationDurability\(request, mutationRecovery, controller\)/);
  assert.match(bootstrap, /confirmCollaborationMutationDurability[\s\S]*setRuntimeCollaborationPhase\(state, request, "live"\);[\s\S]*setHumanJoinSendEnabled\(true\)/);
});

test("runner disclosure honors user collapse over selected project and auto-reveals on navigation", () => {
  assert.equal(resolveRunnerDisclosure(null, true), true);
  assert.equal(resolveRunnerDisclosure(null, false), false);
  assert.equal(resolveRunnerDisclosure(false, true), false);
  assert.equal(resolveRunnerDisclosure(true, false), true);

  let storedRunner1 = true;
  assert.equal(resolveRunnerDisclosure(storedRunner1, false), true);

  storedRunner1 = false;
  assert.equal(resolveRunnerDisclosure(storedRunner1, true), false, "rerender must not override manual collapse");

  storedRunner1 = true;
  assert.equal(resolveRunnerDisclosure(storedRunner1, false), true, "explicit navigation auto-reveals target runner");
});

test("navigation and inspector source contracts maintain disclosure hierarchy and accessibility", async () => {
  const [html, css, source, navigationSource] = await Promise.all([
    readFile(new URL("../src/runtime.html", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime.css", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime_navigation.ts", import.meta.url), "utf8"),
  ]);

  // P3: Recent Sessions component semantics - clean class, no legacy sidebar-details overrides
  assert.match(html, /<details id="runtime-recent-panel" class="recent-panel" open>/);
  assert.match(html, /<span class="recent-panel-title">Recent Sessions<\/span>/);
  assert.doesNotMatch(html, /class="[^"]*sidebar-details/);
  assert.doesNotMatch(css, /\.sidebar-details/);
  assert.doesNotMatch(css, /summary::before\s*\{\s*content:\s*"Show more"/);
  assert.doesNotMatch(css, /summary\[open\]::before\s*\{\s*content:\s*"Recent Sessions"/);
  assert.match(css, /\.recent-panel\s*\{[^}]*border-top:/);

  // P1 & P2: Inspector triggers and close controls with deterministic user intent handling
  assert.match(html, /id="runtime-inspector-close"[^>]*aria-label="Close session context"/);
  assert.match(html, /id="runtime-inspector-backdrop"[^>]*aria-label="Close session context"/);
  assert.match(source, /"Close session context": "关闭会话上下文"/);
  assert.match(source, /el\("runtime-inspector-close"\)\?\.addEventListener\("click", \(\) => closeRuntimeInspector\(true, true\)\)/);
  assert.match(source, /document\.querySelector\("\.context-trigger"\)\?\.addEventListener\("click",/);
  assert.match(source, /reduceRuntimeContextUserIntent/);
  assert.match(source, /resolveRuntimeContextFocusTransition/);
  assert.doesNotMatch(source, /syncingContextDom/);
  assert.doesNotMatch(source, /contextUserIntent\s*=\s*inspector\.open/);
  assert.match(source, /function lock[\s\S]*closeRuntimeInspector\(false, true\);[\s\S]*contextUserIntent = null;/);

  assert.match(source, /function isContextDocked/);
  assert.match(source, /function syncContextUi/);
  assert.match(source, /function revealRunner/);
  assert.match(source, /function switchProject[\s\S]*if \(device\) revealRunner\(device\)/);
  assert.match(source, /function selectRecentSession[\s\S]*if \(clientId\) revealRunner\(clientId\)/);
  assert.match(navigationSource, /group\.open = resolveRunnerDisclosure\(storedDisclosure, defaultOpen\)/);
});

test("project-scoped window activity contracts maintain separation, fencing, and hierarchy", async () => {
  const [html, css, source, navigationSource] = await Promise.all([
    readFile(new URL("../src/runtime.html", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime.css", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/runtime_navigation.ts", import.meta.url), "utf8"),
  ]);

  // HTML hierarchy: project window panel mounted directly above sessions panel
  assert.match(html, /<section id="runtime-project-window-activity-panel"[^>]*class="[^"]*project-window-panel/);
  assert.match(html, /id="runtime-project-windows-count"/);
  assert.match(html, /id="runtime-project-windows-status"/);
  assert.match(html, /id="runtime-project-windows-list"/);
  assert.match(html, /id="runtime-project-windows-empty"/);
  assert.match(html, /id="runtime-project-windows-unavailable"/);
  const windowPanelIndex = html.indexOf('id="runtime-project-window-activity-panel"');
  const sessionsPanelIndex = html.indexOf('id="runtime-workflow-sessions-panel"');
  assert.ok(sessionsPanelIndex > 0 && windowPanelIndex > sessionsPanelIndex, "Workflow Sessions panel must precede Window activity panel in HTML");

  // CSS styling
  assert.match(css, /\.project-window-panel,\s*\.sessions-panel/);
  assert.match(css, /\.project-window-list\s*\{[^}]*gap:\s*4px/);

  // Source contract: fetchProjectWindows fences by project and limits to 10
  const fetchProjWindowsStart = source.indexOf("async function fetchProjectWindows");
  const fetchProjWindowsEnd = source.indexOf("function hideDetail", fetchProjWindowsStart);
  const fetchProjWindows = source.slice(fetchProjWindowsStart, fetchProjWindowsEnd);
  assert.match(fetchProjWindows, /api\("windows",\s*\{\s*project:\s*request\.project,\s*limit:\s*PROJECT_WINDOW_LIMIT\s*\}/);
  assert.match(fetchProjWindows, /isCurrentRuntimeProjectWindowsRequest\(state, request\)/);
  assert.match(fetchProjWindows, /response\.status === 403[\s\S]*projectWindowAvailability = "unavailable"/);
  assert.doesNotMatch(fetchProjWindows, /response\.status === 403[\s\S]*lock\(/);
  assert.match(fetchProjWindows, /!response[\s\S]*projectWindowAvailability = "stale"/);
  assert.match(fetchProjWindows, /!response\.ok \|\| !response\.data[\s\S]*projectWindowAvailability = "stale"/);
  assert.match(source, /function projectWindowActiveCount\(\)[\s\S]*projectWindowAvailability !== "available"[\s\S]*return 0/);

  // Global windows fetch contract remains decoupled (limit 100, no project scope)
  const refreshWindowsStart = source.indexOf("async function refreshWindows");
  const refreshWindowsEnd = source.indexOf("function openWindowInspector", refreshWindowsStart);
  const refreshWindows = source.slice(refreshWindowsStart, refreshWindowsEnd);
  assert.match(refreshWindows, /api\("windows",\s*\{\s*limit:\s*100\s*\}/);
  assert.doesNotMatch(refreshWindows, /project:/);

  // Navigation: windowPanel mounting before sessionsPanel, and WINDOW ACTIVE signal
  assert.match(navigationSource, /if \(options\.windowPanel\) \{\s*options\.windowPanel\.hidden = false;\s*workspace\.appendChild\(options\.windowPanel\);\s*windowsAttached = true;\s*\}/);
  assert.match(navigationSource, /workspace\.appendChild\(sessionsPanel\)/);
  assert.match(navigationSource, /WINDOW ACTIVE/);
  assert.match(navigationSource, /options\.selectedProjectWindowActiveCount \?\? 0\) > 0/);

  // Navigation to global inspector
  const openInspectorStart = source.indexOf("function openWindowInspector");
  const openInspectorEnd = source.indexOf("function projectWindowActiveCount", openInspectorStart);
  const openInspector = source.slice(openInspectorStart, openInspectorEnd);
  assert.match(openInspector, /applyWorkspaceView\("windows"\)/);
  assert.match(openInspector, /selectedWindowKey = key/);
});

