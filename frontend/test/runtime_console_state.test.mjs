import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  preferredRuntimeProjectSelection,
  beginRuntimeCredential,
  refreshRuntimeOverview,
  isCurrentRuntimeOverviewRequest,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  refreshRuntimeRunner,
  isCurrentRuntimeRunnerRequest,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  selectRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
  runtimeCollaborationRequest,
  isCurrentRuntimeCollaborationRequest,
  adoptRuntimeCollaborationList,
  adoptRuntimeCollaborationObservation,
  setRuntimeCollaborationAvailable,
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

test("runtime device and project options use authoritative client ids with stable ordering", () => {
  const projects = [
    { id: "opaque-b", client_id: "device-b", name: "Beta" },
    { id: "agent:not-device-a:project", client_id: "device-a", name: "Zulu" },
    { id: "opaque-a2", client_id: "device-a", name: "Alpha" },
    { id: "opaque-a1", client_id: "device-a", name: "Alpha" },
  ];
  assert.deepEqual(runtimeDeviceIds(projects), ["device-a", "device-b"]);
  assert.deepEqual(runtimeProjectsForDevice(projects, "device-a").map((project) => project.id), ["opaque-a1", "opaque-a2", "agent:not-device-a:project"]);
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-b", "agent:not-device-a:project"), { device: "device-a", project: "agent:not-device-a:project" });
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-a", "missing-project"), { device: "device-a", project: "opaque-a1" });
});

test("runtime refresh preserves an authorized selected device and project", () => {
  const projects = [
    { id: "project-2", client_id: "device-z", name: "Second" },
    { id: "project-1", client_id: "device-z", name: "First" },
    { id: "project-3", client_id: "device-a", name: "Other" },
  ];
  assert.deepEqual(preferredRuntimeProjectSelection(projects, "device-z", "project-2"), { device: "device-z", project: "project-2" });
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

test("runtime collaboration rendering uses textContent and explicitly reloads on history loss", async () => {
  const source = await readFile(new URL("../src/runtime.ts", import.meta.url), "utf8");
  assert.equal(source.includes("innerHTML"), false);
  assert.match(source, /body\.textContent = String\(message\?\.message \|\| ""\)/);
  assert.match(source, /action === "reload"[\s\S]*loadRetainedCollaboration/);
  assert.match(source, /action === "drain"/);
  assert.match(source, /abortCollaboration\(\)/);
});
