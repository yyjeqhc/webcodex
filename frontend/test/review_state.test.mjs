// Unit tests for the pure review-identity state machine (Node built-in runner).
// Run against the built ESM module: `node --test`. `check:dist` guarantees the
// module matches its TypeScript source.
import test from "node:test";
import assert from "node:assert/strict";
import {
  initialState,
  selectTask,
  adoptReview,
  actionsEnabled,
  openConfirm,
  actionRequest,
  beginRefresh,
  endRefresh,
  createReviewController,
} from "../dist/review_state.js";

const reviewA = {
  task_id: "wc_task_aaaa",
  event_cursor: 5,
  result: { result_id: "wc_result_aaaa" },
  changed_paths: [],
};
const reviewB = {
  task_id: "wc_task_bbbb",
  event_cursor: 9,
  result: { result_id: "wc_result_bbbb" },
  changed_paths: [],
};

// A review loaded → select B → stale A confirm/action denied → B review loaded
// → B result identity submitted.
test("frontend_action_snapshot_is_bound_to_review_identity", () => {
  const s = initialState();

  const seqA = selectTask(s, "wc_task_aaaa");
  assert.equal(adoptReview(s, "wc_task_aaaa", seqA, reviewA), true);
  assert.equal(actionsEnabled(s), true);

  // Select B before B's review returns.
  const seqB = selectTask(s, "wc_task_bbbb");
  assert.equal(actionsEnabled(s), false);

  // A late review for A (old sequence) must be dropped, not adopted.
  assert.equal(adoptReview(s, "wc_task_aaaa", seqA, reviewA), false);
  // With no live snapshot, opening a confirm is denied.
  assert.equal(openConfirm(s, "accept"), null);

  // B's review arrives and is adopted; the action binds to B's identity.
  assert.equal(adoptReview(s, "wc_task_bbbb", seqB, reviewB), true);
  const pending = openConfirm(s, "accept");
  assert.ok(pending);
  assert.deepEqual(actionRequest(pending), {
    path: "result/accept",
    body: { task_id: "wc_task_bbbb", result_id: "wc_result_bbbb" },
  });
});

test("selection_change_disables_old_actions", () => {
  const s = initialState();
  const seq = selectTask(s, "wc_task_aaaa");
  adoptReview(s, "wc_task_aaaa", seq, reviewA);
  assert.equal(actionsEnabled(s), true);
  openConfirm(s, "accept");

  // Switching selection immediately disables actions and drops the pending
  // confirm, so a confirm click after a switch cannot act on the old task.
  selectTask(s, "wc_task_bbbb");
  assert.equal(actionsEnabled(s), false);
  assert.equal(s.pending, null);
  assert.equal(actionRequest(s.pending), null);
});

test("refresh_is_single_flight", () => {
  const s = initialState();
  assert.equal(beginRefresh(s, "tick"), true);
  // A second overlapping refresh on the same channel is refused.
  assert.equal(beginRefresh(s, "tick"), false);
  endRefresh(s, "tick");
  assert.equal(beginRefresh(s, "tick"), true);
  // Channels are independent.
  assert.equal(beginRefresh(s, "tasks"), true);
  assert.equal(beginRefresh(s, "tasks"), false);
});

test("accept_requires_a_bound_result_id", () => {
  const s = initialState();
  const noResult = { task_id: "wc_task_xxxx", event_cursor: 1, result: null };
  const seq = selectTask(s, "wc_task_xxxx");
  adoptReview(s, "wc_task_xxxx", seq, noResult);
  // Accept cannot proceed without a bound result id.
  assert.equal(actionRequest(openConfirm(s, "accept")), null);
});

test("reject_of_interrupted_task_omits_result_id", () => {
  const s = initialState();
  const interrupted = { task_id: "wc_task_iiii", event_cursor: 2, result: null };
  const seq = selectTask(s, "wc_task_iiii");
  adoptReview(s, "wc_task_iiii", seq, interrupted);
  assert.deepEqual(actionRequest(openConfirm(s, "reject")), {
    path: "result/reject",
    body: { task_id: "wc_task_iiii" },
  });
});

function deferred() {
  let resolve;
  const promise = new Promise((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function harness(render = () => {}) {
  const calls = [];
  const scheduled = [];
  const aborts = [];
  const errors = [];
  let unauthorized = 0;
  let credential = "in-memory-secret";
  const controller = createReviewController({
    fetchReview(body, signal) {
      const response = deferred();
      calls.push({ body, signal, response });
      return response.promise;
    },
    render,
    schedule(next, delay) {
      next.delay = delay;
      next.cancelled = false;
      scheduled.push(next);
      return next;
    },
    cancelSchedule(next) {
      next.cancelled = true;
    },
    abort() {
      const signal = { aborted: false };
      const request = {
        signal,
        abort() {
          signal.aborted = true;
          aborts.push(request);
        },
      };
      return request;
    },
    unauthorized() {
      unauthorized += 1;
      credential = "";
    },
    error(data) {
      errors.push(data);
    },
  });
  return {
    controller,
    calls,
    scheduled,
    aborts,
    errors,
    unauthorized: () => unauthorized,
    credential: () => credential,
  };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
}

test("controller keeps stale A action from retargeting B", async () => {
  const state = initialState();
  const rendered = [];
  const h = harness((review) => {
    rendered.push(review.task_id);
    adoptReview(state, review.task_id, state.reviewSeq, review);
  });

  selectTask(state, reviewA.task_id);
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  assert.ok(openConfirm(state, "accept"));
  h.scheduled.shift()();
  assert.equal(h.calls.length, 2);

  selectTask(state, reviewB.task_id);
  h.controller.select(reviewB.task_id);
  assert.equal(state.pending, null);
  assert.equal(h.calls[1].signal.aborted, true);
  h.calls[1].response.resolve({ status: 200, ok: true, data: reviewA });
  h.calls[2].response.resolve({ status: 200, ok: true, data: reviewB });
  await settle();
  assert.deepEqual(rendered, [reviewA.task_id, reviewB.task_id]);
  assert.deepEqual(actionRequest(openConfirm(state, "accept")), {
    path: "result/accept",
    body: { task_id: reviewB.task_id, result_id: "wc_result_bbbb" },
  });
});

test("controller advances cursor and ignores unchanged heartbeat", async () => {
  const rendered = [];
  const h = harness((review) => rendered.push(review.event_cursor));
  h.controller.select(reviewA.task_id);
  assert.deepEqual(h.calls[0].body.after_cursor, null);
  assert.equal(h.calls[0].body.wait_ms, 0);

  h.calls[0].response.resolve({ status: 200, ok: true, data: { ...reviewA, event_cursor: 10 } });
  await settle();
  h.scheduled.shift()();
  assert.equal(h.calls[1].body.after_cursor, 10);
  assert.equal(h.calls[1].body.wait_ms, 15000);
  h.calls[1].response.resolve({
    status: 200,
    ok: true,
    data: { ...reviewA, event_cursor: 10, heartbeat: true },
  });
  await settle();
  assert.deepEqual(rendered, [10]);

  h.scheduled.shift()();
  h.calls[2].response.resolve({ status: 200, ok: true, data: { ...reviewA, event_cursor: 11 } });
  await settle();
  assert.deepEqual(rendered, [10, 11]);
});

test("controller aborts on switch and hidden, resumes once, stops on 401", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.controller.select(reviewB.task_id);
  assert.equal(h.calls[0].signal.aborted, true);
  h.controller.hide();
  assert.equal(h.calls[1].signal.aborted, true);
  h.controller.show();
  h.controller.show();
  assert.equal(h.calls.length, 3);
  h.calls[2].response.resolve({ status: 401, ok: false, data: {} });
  await settle();
  assert.equal(h.unauthorized(), 1);
  assert.equal(h.credential(), "");
  assert.equal(h.scheduled.length, 0);
  assert.equal(h.controller.running(), false);
});

test("action completes beside long-poll and restarts one full review", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  h.scheduled.shift()();
  assert.equal(h.controller.running(), true);

  const action = deferred();
  let actions = 0;
  const runAction = async () => {
    actions += 1;
    await action.promise;
    h.controller.restart();
  };
  const pending = runAction();
  assert.equal(actions, 1);
  assert.equal(h.controller.running(), true);
  action.resolve();
  await pending;
  assert.equal(h.calls[1].signal.aborted, true);
  assert.equal(h.calls.length, 3);
  assert.equal(h.calls[2].body.after_cursor, null);
  assert.equal(h.calls[2].body.wait_ms, 0);
  h.calls[1].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  assert.equal(h.calls.length, 3);
});

test("network failure uses delayed retry", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve(null);
  await settle();

  assert.equal(h.scheduled[0].delay, 1000);
  assert.equal(h.calls.length, 1);
  h.scheduled[0]();
  assert.equal(h.calls.length, 2);
});

test("http failures back off and cap", async () => {
  const h = harness();
  const expected = [1000, 2000, 4000, 8000, 15000, 15000];
  h.controller.select(reviewA.task_id);

  for (const [index, delay] of expected.entries()) {
    h.calls[index].response.resolve({
      status: index % 2 ? 429 : 500,
      ok: false,
      data: { error: { message: "temporary failure" } },
    });
    await settle();
    const next = h.scheduled.shift();
    assert.equal(next.delay, delay);
    assert.equal(h.calls.length, index + 1);
    next();
  }
  assert.equal(h.errors.length, expected.length);
});

test("successful response resets backoff", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);

  h.calls[0].response.resolve(null);
  await settle();
  const firstFailure = h.scheduled.shift();
  assert.equal(firstFailure.delay, 1000);
  firstFailure();
  h.calls[1].response.resolve(null);
  await settle();
  const secondFailure = h.scheduled.shift();
  assert.equal(secondFailure.delay, 2000);
  secondFailure();

  h.calls[2].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  const afterSuccess = h.scheduled.shift();
  assert.equal(afterSuccess.delay, 0);
  afterSuccess();

  h.calls[3].response.resolve(null);
  await settle();
  assert.equal(h.scheduled[0].delay, 1000);
});

test("switch invalidates delayed retry", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve(null);
  await settle();
  const retryA = h.scheduled.shift();

  h.controller.select(reviewB.task_id);
  assert.equal(retryA.cancelled, true);
  retryA();
  assert.equal(h.calls.length, 2);
  assert.deepEqual(
    h.calls.filter((call) => call.body.task_id === reviewB.task_id).map((call) => call.body),
    [{
      task_id: reviewB.task_id,
      include_diff: true,
      include_output_tail: true,
      after_cursor: null,
      wait_ms: 0,
    }]
  );
});

test("hide and stop cancel delayed retry", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve(null);
  await settle();
  const hiddenRetry = h.scheduled.shift();

  h.controller.hide();
  assert.equal(hiddenRetry.cancelled, true);
  hiddenRetry();
  assert.equal(h.calls.length, 1);
  h.controller.show();
  h.controller.show();
  assert.equal(h.calls.length, 2);

  h.calls[1].response.resolve(null);
  await settle();
  const stoppedRetry = h.scheduled.shift();
  h.controller.stop();
  assert.equal(stoppedRetry.cancelled, true);
  stoppedRetry();
  h.controller.show();
  assert.equal(h.calls.length, 2);
});

test("action restart replaces old poll and retry", async () => {
  const h = harness();
  h.controller.select(reviewA.task_id);
  h.calls[0].response.resolve(null);
  await settle();
  const delayedRetry = h.scheduled.shift();

  h.controller.restart();
  assert.equal(delayedRetry.cancelled, true);
  delayedRetry();
  assert.equal(h.calls.length, 2);
  assert.equal(h.calls[1].body.wait_ms, 0);

  h.calls[1].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  const nextPoll = h.scheduled.shift();
  nextPoll();
  assert.equal(h.calls[2].body.wait_ms, 15000);
  h.controller.restart();
  assert.equal(h.calls[2].signal.aborted, true);
  assert.equal(h.calls.length, 4);
  assert.equal(h.calls[3].body.wait_ms, 0);

  nextPoll();
  h.calls[2].response.resolve({ status: 200, ok: true, data: reviewA });
  await settle();
  assert.equal(h.calls.length, 4);
});
