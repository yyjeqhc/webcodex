import test from "node:test";
import assert from "node:assert/strict";
import {
  initialRuntimeConsoleState,
  runtimeDeviceIds,
  runtimeProjectsForDevice,
  preferredRuntimeProjectSelection,
  beginRuntimeCredential,
  refreshRuntimeProjects,
  isCurrentRuntimeProjectsRequest,
  selectRuntimeProject,
  refreshRuntimeSessionList,
  isCurrentRuntimeSessionListRequest,
  selectRuntimeWorkflowSession,
  isCurrentRuntimeWorkflowSessionRequest,
  adoptRuntimeWorkflowSessionDetail,
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

test("runtime device and project options use authoritative client ids with stable ordering", () => {
  const projects = [
    { id: "opaque-b", client_id: "device-b", name: "Beta" },
    { id: "agent:not-device-a:project", client_id: "device-a", name: "Zulu" },
    { id: "opaque-a2", client_id: "device-a", name: "Alpha" },
    { id: "opaque-a1", client_id: "device-a", name: "Alpha" },
  ];

  assert.deepEqual(runtimeDeviceIds(projects), ["device-a", "device-b"]);
  assert.deepEqual(
    runtimeProjectsForDevice(projects, "device-a").map((project) => project.id),
    ["opaque-a1", "opaque-a2", "agent:not-device-a:project"]
  );
  assert.deepEqual(
    preferredRuntimeProjectSelection(projects, "device-b", "agent:not-device-a:project"),
    { device: "device-a", project: "agent:not-device-a:project" }
  );
  assert.deepEqual(
    preferredRuntimeProjectSelection(projects, "device-a", "missing-project"),
    { device: "device-a", project: "opaque-a1" }
  );
});

test("runtime refresh preserves an authorized selected device and project", () => {
  const projects = [
    { id: "project-2", client_id: "device-z", name: "Second" },
    { id: "project-1", client_id: "device-z", name: "First" },
    { id: "project-3", client_id: "device-a", name: "Other" },
  ];
  assert.deepEqual(
    preferredRuntimeProjectSelection(projects, "device-z", "project-2"),
    { device: "device-z", project: "project-2" }
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
