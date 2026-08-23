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
  workflowSessionListOverviewFacts,
  workflowSessionOverviewPresentation,
  workflowSessionIdleAttentionLabel,
  workflowSessionLivenessPresentation,
} from "../dist/workflow_session_state.js";

test("workflow session list overview stays compact and labels retained evidence", () => {
  const facts = workflowSessionListOverviewFacts({
    work: { edits: 2, validations: 3, exploration: 7, reviews: 1, runs: 2, history_truncated: true },
    validation: { state: "passed", history_truncated: true, unresolved_failure_count: 0 },
    attention: { open_guidance: 1, open_questions: 0, open_risks: 1, open_todos: 2 },
  });
  assert.equal(facts.length, 3);
  assert.deepEqual(facts[0], { text: "Latest retained validation passed", tone: "pass" });
  assert.deepEqual(facts[1], { text: "Retained: 1 risk · 2 todos", tone: "fail" });
  assert.deepEqual(facts[2], { text: "Recent 2 edits · 3 validations", tone: "runtime" });

  const failed = workflowSessionListOverviewFacts({
    work: { history_truncated: true },
    validation: { state: "failed", history_truncated: true, unresolved_failure_count: 2 },
    attention: {},
  });
  assert.deepEqual(failed[0], {
    text: "Retained: 2 unresolved validation failures",
    tone: "fail",
  });
});

test("workflow session detail overview separates runtime validation attention and reported progress", () => {
  const view = workflowSessionOverviewPresentation({
    work: { edits: 1, validations: 2, exploration: 3, reviews: 1, runs: 1, history_truncated: false },
    validation: {
      state: "failed",
      latest_kind: "test",
      latest_at: 123,
      unresolved_failure_count: 1,
      tests_run_count: 4,
      history_truncated: false,
    },
    attention: { open_guidance: 0, open_questions: 1, open_risks: 1, open_todos: 0 },
    reported_progress: { reported_at: 124, text: "model says it is nearly done" },
  });
  assert.match(view.workText, /^Observed work:/);
  assert.match(view.validationText, /1 unresolved validation failure/);
  assert.match(view.validationText, /latest test/);
  assert.match(view.validationText, /4 tests/);
  assert.equal(view.validationTone, "fail");
  assert.match(view.attentionText, /Retained open messages: 1 risk · 1 question/);
  assert.equal(view.attentionTone, "fail");
  assert.equal(view.progressText, "model says it is nearly done");
  assert.equal(view.progressAt, 124);

  const guidanceOnly = workflowSessionOverviewPresentation({
    work: {},
    validation: { state: "not_run", unresolved_failure_count: 0, history_truncated: false },
    attention: { open_guidance: 1, open_questions: 0, open_risks: 0, open_todos: 0 },
  });
  assert.equal(guidanceOnly.attentionTone, "warn");

  const unavailable = workflowSessionOverviewPresentation({
    work: { validations: 1, history_truncated: false },
    validation: { state: "unavailable", unresolved_failure_count: 0, history_truncated: false },
    attention: {},
  });
  assert.equal(unavailable.validationText, "Terminal validation evidence unavailable");
  assert.equal(unavailable.progressText, "No retained model-reported progress.");
});

test("Session liveness stays factual across working recent idle and attention states", () => {
  const overview = { attention: { open_guidance: 0, open_questions: 1, open_risks: 0, open_todos: 1 } };
  const workingCall = workflowSessionLivenessPresentation({ running_call: true, running_jobs: 0, updated_at: 100, overview }, 1000);
  const workingJob = workflowSessionLivenessPresentation({ running_call: false, running_jobs: 1, updated_at: 100, overview }, 1000);
  const recent = workflowSessionLivenessPresentation({ running_call: false, running_jobs: 0, updated_at: 950, overview: { attention: {} } }, 1000);
  const attention = workflowSessionLivenessPresentation({ running_call: false, running_jobs: 0, updated_at: 700, overview }, 1000);
  const idle = workflowSessionLivenessPresentation({ running_call: false, running_jobs: 0, updated_at: 700, overview: { attention: {} } }, 1000);
  assert.equal(workingCall.label, "working");
  assert.equal(workingJob.label, "working");
  assert.equal(recent.label, "recently active");
  assert.equal(attention.label, "idle · pending attention");
  assert.equal(idle.label, "idle · 5m");
  assert.equal(idle.tooltip, "WebCodex activity only; host/model state is unknown.");
  for (const view of [workingCall, workingJob, recent, attention, idle]) {
    assert.equal(/stalled|abandoned|model failed|host frozen/i.test(view.label), false);
  }
  assert.equal(workflowSessionIdleAttentionLabel(false, overview), "idle · pending attention");
  assert.equal(workflowSessionIdleAttentionLabel(true, overview), "working");
  assert.equal(workflowSessionIdleAttentionLabel(false, { attention: {} }), "idle");
});

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
