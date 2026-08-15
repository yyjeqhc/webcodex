import test from "node:test";
import assert from "node:assert/strict";
import {
  initialWorkflowSessionState,
  selectWorkflowSession,
  refreshWorkflowSessionDetail,
  clearWorkflowSessionSelection,
  adoptWorkflowSessionDetail,
  updateWorkflowSessionFollowFromScroll,
  workflowSessionScrollTopAfterRender,
  jumpWorkflowSessionToLatest,
  shouldFollowWorkflowSessionLatest,
} from "../dist/workflow_session_state.js";

test("stale same-session detail response cannot overwrite newer snapshot", () => {
  const state = initialWorkflowSessionState();
  const older = selectWorkflowSession(state, "wc_sess_same");
  const newer = refreshWorkflowSessionDetail(state);

  assert.equal(adoptWorkflowSessionDetail(state, newer, { updated_at: 2 }), true);
  assert.equal(adoptWorkflowSessionDetail(state, older, { updated_at: 1 }), false);
  assert.equal(state.snapshot.updated_at, 2);
});

test("switching workflow sessions invalidates the previous detail snapshot", () => {
  const state = initialWorkflowSessionState();
  const first = selectWorkflowSession(state, "wc_sess_first");
  assert.equal(adoptWorkflowSessionDetail(state, first, { title: "first" }), true);

  const second = selectWorkflowSession(state, "wc_sess_second");
  assert.equal(state.selectedSessionId, "wc_sess_second");
  assert.equal(state.snapshot, null);
  assert.equal(adoptWorkflowSessionDetail(state, first, { title: "late first" }), false);
  assert.equal(adoptWorkflowSessionDetail(state, second, { title: "second" }), true);

  clearWorkflowSessionSelection(state);
  assert.equal(state.selectedSessionId, "");
  assert.equal(state.snapshot, null);
});

test("timeline follow stays enabled at bottom and manual upward scroll disables it", () => {
  const state = initialWorkflowSessionState();
  selectWorkflowSession(state, "wc_sess_follow");
  assert.equal(shouldFollowWorkflowSessionLatest(state), true);

  assert.equal(updateWorkflowSessionFollowFromScroll(state, 700, 300, 1000), true);
  assert.equal(shouldFollowWorkflowSessionLatest(state), true);
  assert.equal(workflowSessionScrollTopAfterRender(state, 700, 300, 1200), 900);

  assert.equal(updateWorkflowSessionFollowFromScroll(state, 400, 300, 1000), false);
  assert.equal(shouldFollowWorkflowSessionLatest(state), false);
  assert.equal(workflowSessionScrollTopAfterRender(state, 400, 300, 1200), 400);
});

test("jump restores follow and session switch resets follow state", () => {
  const state = initialWorkflowSessionState();
  selectWorkflowSession(state, "wc_sess_first");
  updateWorkflowSessionFollowFromScroll(state, 100, 300, 1000);
  assert.equal(shouldFollowWorkflowSessionLatest(state), false);

  jumpWorkflowSessionToLatest(state);
  assert.equal(shouldFollowWorkflowSessionLatest(state), true);

  updateWorkflowSessionFollowFromScroll(state, 100, 300, 1000);
  selectWorkflowSession(state, "wc_sess_second");
  assert.equal(shouldFollowWorkflowSessionLatest(state), true);

  updateWorkflowSessionFollowFromScroll(state, 100, 300, 1000);
  clearWorkflowSessionSelection(state);
  assert.equal(shouldFollowWorkflowSessionLatest(state), true);
});
