import test from "node:test";
import assert from "node:assert/strict";
import {
  initialRuntimeConsoleState,
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

  const listA = selectRuntimeProject(state, "agent:a:project");
  const projectsDuringA = refreshRuntimeProjects(state);
  const refreshedA = refreshRuntimeSessionList(state);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), true);

  const listB = selectRuntimeProject(state, "agent:b:project");
  assert.equal(isCurrentRuntimeProjectsRequest(state, projectsDuringA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, refreshedA), false);
  assert.equal(isCurrentRuntimeSessionListRequest(state, listB), true);
  assert.equal(state.workflow.selectedSessionId, "");
  assert.equal(state.workflow.snapshot, null);
});

test("runtime workflow detail identity includes project plus session id", () => {
  const state = initialRuntimeConsoleState();
  beginRuntimeCredential(state);
  selectRuntimeProject(state, "agent:a:project");
  const detailA = selectRuntimeWorkflowSession(state, "wc_sess_same");
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailA), true);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailA, { title: "A" }), true);
  assert.equal(state.workflow.snapshot.title, "A");

  selectRuntimeProject(state, "agent:b:project");
  assert.equal(state.workflow.snapshot, null);
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailA), false);

  const detailB = selectRuntimeWorkflowSession(state, "wc_sess_same");
  assert.equal(detailB.project, "agent:b:project");
  assert.equal(isCurrentRuntimeWorkflowSessionRequest(state, detailB), true);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailA, { title: "late A" }), false);
  assert.equal(adoptRuntimeWorkflowSessionDetail(state, detailB, { title: "B" }), true);
  assert.equal(state.workflow.snapshot.title, "B");
});
