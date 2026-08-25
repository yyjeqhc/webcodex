import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  filterAndSortRuntimeProjects,
  runtimeProjectIdentityText,
  preferredRuntimeProjectSelection,
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
  selectRuntimeWorkflowSession,
  selectRuntimeSessionLocation,
  refreshRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
  adoptRuntimeCollaborationObservation,
  setRuntimeCollaborationAvailable,
  setRuntimeCollaborationPhase,
  runtimeCollaborationNeedsRefreshRecovery,
  mergeRuntimeCollaborationMessages,
  runtimeCollaborationObservationAction,
} from "../dist/runtime_console_state.js";

test("runtime credential and project generations fence stale project responses", () => {
  const state = initialRuntimeConsoleState();
  const firstProjects = beginRuntimeCredential(state);
  const newerProjects = refreshRuntimeProjects(state);
  assert.equal(isCurrentRuntimeProjectsRequest(state, firstProjects), false);
  assert.equal(isCurrentRuntimeProjectsRequest(state, newerProjects), true);

  const listA = selectRuntimeProject(state, "device-a", "agent:a:project");
  assert.equal(state.selectedDevice, "device-a");
  assert.equal(state.selectedProject, "agent:a:project");
  const projectsDuringA = refreshRuntimeProjects(state);
  const refreshedA = refreshRuntimeSessionList(state);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), true);

  const listB = selectRuntimeProject(state, "device-b", "agent:b:project");
  assert.equal(state.selectedDevice, "device-b");
  assert.equal(state.selectedProject, "agent:b:project");
  assert.equal(isCurrentRuntimeProjectsRequest(state, projectsDuringA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listB), true);
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
  assert.equal(adoptRuntimeCollaborationList(state, requestA, [{ message_id: "wc_msg_old" }]), false);
});

test("collaboration delta replaces message state by id and completion renders todo resolution plus answer", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner", "agent:runner:project");
  selectRuntimeWorkflowSession(state, "wc_sess_a");
  const request = runtimeCollaborationRequest(state);
  adoptRuntimeCollaborationList(state, request, [
    { message_id: "wc_msg_todo", kind: "todo", status: "open", created_at: 1, message: "do work" },
  ]);
  assert.equal(adoptRuntimeCollaborationObservation(state, request, {
    observation_token: "opaque-1",
    messages: [
      { message_id: "wc_msg_todo", kind: "todo", status: "resolved", created_at: 1, message: "do work", resolved_by_message_id: "wc_msg_answer" },
      { message_id: "wc_msg_answer", kind: "answer", status: "open", created_at: 2, message: "done", reply_to: "wc_msg_todo", author_session_id: "wc_sess_worker" },
    ],
  }), true);
  assert.equal(state.collaboration.messages.length, 2);
  assert.equal(state.collaboration.messages[0].status, "resolved");
  assert.equal(state.collaboration.messages[1].reply_to, "wc_msg_todo");
  assert.equal(state.collaboration.observationToken, "opaque-1");
});

test("history loss reloads and has_more drains without duplicate message ids", () => {
  assert.equal(runtimeCollaborationObservationAction({ history_lost: true, has_more: true }), "reload");
  assert.equal(runtimeCollaborationObservationAction({ history_lost: false, has_more: true }), "drain");
  assert.equal(runtimeCollaborationObservationAction({ wait_outcome: "timeout" }), "wait");
  const merged = mergeRuntimeCollaborationMessages(
    [{ message_id: "a", created_at: 1, status: "open" }],
    [{ message_id: "a", created_at: 1, status: "resolved" }, { message_id: "b", created_at: 2 }]
  );
  assert.deepEqual(merged.map((message) => message.message_id), ["a", "b"]);
  assert.equal(merged[0].status, "resolved");
});

test("project-read-only degradation keeps project selection while collaboration is marked unavailable", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner", "agent:runner:project");
  selectRuntimeWorkflowSession(state, "wc_sess_a");
  const request = runtimeCollaborationRequest(state);
  assert.equal(setRuntimeCollaborationAvailable(state, request, false), true);
  assert.equal(state.selectedProject, "agent:runner:project");
  assert.equal(state.workflow.selectedSessionId, "wc_sess_a");
  assert.equal(state.collaboration.available, false);
});

test("manual Refresh recovery is required only after collaboration is paused", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "runner", "agent:runner:project");
  selectRuntimeWorkflowSession(state, "wc_sess_a");
  const requestA = runtimeCollaborationRequest(state);
  assert.equal(setRuntimeCollaborationPhase(state, requestA, "live"), true);
  assert.equal(runtimeCollaborationNeedsRefreshRecovery(state), false);
  assert.equal(setRuntimeCollaborationPhase(state, requestA, "paused"), true);
  assert.equal(runtimeCollaborationNeedsRefreshRecovery(state), true);
  selectRuntimeWorkflowSession(state, "wc_sess_b");
  assert.equal(state.collaboration.phase, "idle");
  assert.equal(setRuntimeCollaborationPhase(state, requestA, "paused"), false);
  assert.equal(runtimeCollaborationNeedsRefreshRecovery(state), false);
});

test("runtime collaboration rendering uses textContent and explicitly reloads on history loss", async () => {
  const source = await readFile(new URL("../src/runtime.ts", import.meta.url), "utf8");
  const html = await readFile(new URL("../src/runtime.html", import.meta.url), "utf8");
  const css = await readFile(new URL("../src/runtime.css", import.meta.url), "utf8");
  assert.equal(html.includes("runtime-project-" + "select"), false);
  assert.match(html, /runtime-project-list/);
  assert.match(html, /runtime-project-search/);
  assert.match(html, /runtime-session-workspace/);
  assert.match(html, /workspace path/);
  assert.match(html, /Working &amp; Recently Updated Sessions/);
  assert.match(html, /runtime-recent-session-list/);
  assert.match(html, /Runner Fleet/);
  assert.match(html, /runtime-runner-list/);
  assert.match(html, /All Runners/);
  assert.match(html, /runtime-collaboration-form/);
  assert.match(html, /runtime-message-requires-ack/);
  assert.match(css, /-webkit-line-clamp:\s*4/);
  assert.match(css, /\.recent-session-row/);
  assert.match(css, /\.fleet-row/);
  assert.equal(source.includes("innerHTML"), false);
  assert.doesNotMatch(source, /api\("runner"/);
  assert.match(source, /selectRuntimeSessionLocation/);
  assert.match(source, /path\.textContent = String\(project\.path\)/);
  assert.match(source, /runtimeProjectIdentityText\(selectedProjectRow\(\)\)/);
  assert.match(source, /renderSessionWorkspaceIdentity\(\)/);
  const recentStart = source.indexOf("function renderRecentSessions");
  const recentEnd = source.indexOf("function selectRecentSession", recentStart);
  const recentRender = source.slice(recentStart, recentEnd);
  assert.match(recentRender, /workflowSessionLivenessPresentation\(session\)/);
  assert.match(recentRender, /attentionLabel\(session\.overview\?\.attention\)/);
  assert.match(recentRender, /updatedLabel\(session\.updated_at\)/);
  assert.doesNotMatch(recentRender, /workflowSessionListOverviewFacts|summary-facts|validation/);
  assert.match(source, /applyRunnerFilter\(select\.value\)/);
  assert.match(source, /void fetchOverview\(refreshRuntimeOverview\(state\)\)/);
  assert.match(source, /body\.textContent = String\(message\?\.message \|\| ""\)/);
  assert.match(source, /action === "reload"[\s\S]*loadRetainedCollaboration/);
  assert.match(source, /action === "drain"/);
  assert.match(source, /abortCollaboration\(\)/);
  assert.match(source, /workflow-session-post-message/);
  assert.match(source, /kind\?\.value === "guidance"/);
  assert.match(source, /priority\?\.value !== "high"/);
  assert.match(source, /First ACK observed/);
  assert.doesNotMatch(source, /Delivered|Read by model|Currently acknowledged/);
  assert.match(source, /Refresh failed · showing previous data/);
  assert.match(source, /runtimeCollaborationNeedsRefreshRecovery/);
  assert.match(source, /event\.key === "Enter" \|\| event\.key === " "/);
  const renderProjectsStart = source.indexOf("function renderProjectSelectors");
  const renderProjectsEnd = source.indexOf("function switchProject", renderProjectsStart);
  const renderProjects = source.slice(renderProjectsStart, renderProjectsEnd);
  assert.match(renderProjects, /all\.textContent = "All Runners"/);
  assert.match(renderProjects, /switchProject\(String\(project\.client_id \|\| ""\), String\(project\.id \|\| ""\)\)/);
  const fetchProjectsStart = source.indexOf("async function fetchProjects");
  const fetchProjectsEnd = source.indexOf("function effectiveProjects", fetchProjectsStart);
  assert.doesNotMatch(source.slice(fetchProjectsStart, fetchProjectsEnd), /fetchOverview\(/);
  const selectStart = source.indexOf("function selectSession");
  const selectEnd = source.indexOf("async function fetchSessionDetail", selectStart);
  assert.match(source.slice(selectStart, selectEnd), /setHumanJoinSendEnabled\(false\)[\s\S]*startCollaboration/);
  const postStart = source.indexOf("async function postHumanCollaborationMessage");
  const postEnd = source.indexOf("function setRefreshBusy", postStart);
  const post = source.slice(postStart, postEnd);
  assert.match(post, /if \(!isCurrentRuntimeCollaborationRequest\(state, request\)\) return;\s*if \(response\?\.status === 0\)[\s\S]*return;\s*\}[\s\S]*if \(send\) send\.disabled = false;/);
  assert.match(post, /Send outcome unknown\. Refresh and review retained messages before retrying\./);
  assert.match(post, /abortCollaboration\(\)[\s\S]*setRuntimeCollaborationPhase\(state, request, "paused"\)/);
  const refreshStart = source.indexOf("async function refreshAll");
  const refreshEnd = source.indexOf("function startAuto", refreshStart);
  assert.equal((source.slice(refreshStart, refreshEnd).match(/fetchOverview\(/g) || []).length, 1);
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
  assert.match(bootstrap, /setRuntimeCollaborationPhase\(state, request, "live"\);[\s\S]*setHumanJoinSendEnabled\(true\)/);
});
