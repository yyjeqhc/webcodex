// DOM-free review identity, action binding, and selected-task polling state.

export {};

export function initialState(): any {
  return {
    selectedTaskId: "",
    reviewSeq: 0,
    snapshot: null,
    pending: null,
    inFlight: { readiness: false, tasks: false, tick: false },
  };
}

export function reviewSnapshot(review: any): any {
  const result = review && review.result ? review.result : null;
  const cursor = review && typeof review.event_cursor === "number" ? review.event_cursor : 0;
  return {
    taskId: review && review.task_id ? String(review.task_id) : "",
    resultId: result && result.result_id ? String(result.result_id) : null,
    eventCursor: cursor,
    review: review,
  };
}

export function selectTask(state: any, taskId: string): any {
  state.selectedTaskId = taskId;
  state.reviewSeq = state.reviewSeq + 1;
  state.snapshot = null;
  state.pending = null;
  return state.reviewSeq;
}

export function adoptReview(state: any, taskId: string, seq: any, review: any): boolean {
  if (seq !== state.reviewSeq || taskId !== state.selectedTaskId) {
    return false;
  }
  const next = reviewSnapshot(review);
  if (state.snapshot && state.snapshot.resultId !== next.resultId) {
    state.pending = null;
  }
  state.snapshot = next;
  return true;
}

export function actionsEnabled(state: any): boolean {
  return !!state.snapshot && state.snapshot.taskId === state.selectedTaskId;
}

export function openConfirm(state: any, action: string): any {
  if (!actionsEnabled(state)) {
    state.pending = null;
    return null;
  }
  state.pending = { action: action, snapshot: state.snapshot };
  return state.pending;
}

export function closeConfirm(state: any): void {
  state.pending = null;
}

export function actionRequest(pending: any): any {
  if (!pending || !pending.snapshot || !pending.action || !pending.snapshot.taskId) {
    return null;
  }
  const snapshot = pending.snapshot;
  if (pending.action === "accept") {
    if (!snapshot.resultId) {
      return null;
    }
    return {
      path: "result/accept",
      body: { task_id: snapshot.taskId, result_id: snapshot.resultId },
    };
  }
  if (pending.action === "reject") {
    const body: any = { task_id: snapshot.taskId };
    if (snapshot.resultId) {
      body.result_id = snapshot.resultId;
    }
    return { path: "result/reject", body: body };
  }
  return { path: "task/cancel", body: { task_id: snapshot.taskId } };
}

export function beginRefresh(state: any, channel: string): boolean {
  if (state.inFlight[channel]) {
    return false;
  }
  state.inFlight[channel] = true;
  return true;
}

export function endRefresh(state: any, channel: string): void {
  state.inFlight[channel] = false;
}

export function reset(state: any): void {
  state.selectedTaskId = "";
  state.reviewSeq = state.reviewSeq + 1;
  state.snapshot = null;
  state.pending = null;
  state.inFlight = { readiness: false, tasks: false, tick: false };
}

export function createReviewController(options: any): any {
  let taskId = "";
  let cursor: any = null;
  let signature = "";
  let active: any = null;
  let scheduled: any = null;
  let failures = 0;
  let generation = 0;
  let visible = true;
  let authorized = true;
  let running = false;

  function invalidate(): void {
    generation += 1;
    if (scheduled !== null) {
      options.cancelSchedule(scheduled);
    }
    scheduled = null;
    if (active) {
      active.abort();
    }
    active = null;
    running = false;
  }

  function schedule(version: number, delay: number): void {
    scheduled = options.schedule(() => {
      if (version !== generation) {
        return;
      }
      scheduled = null;
      void request(false);
    }, delay);
  }

  function identity(review: any): string {
    const result = review && review.result ? review.result.result_id : null;
    const execution = review && review.recent_execution ? review.recent_execution : {};
    return JSON.stringify([
      review && review.task_id,
      result,
      review && review.status,
      review && review.run_status,
      execution.execution_status,
      execution.stdout_cursor,
      execution.stderr_cursor,
      execution.assertion_status,
    ]);
  }

  async function request(full: boolean): Promise<void> {
    if (!taskId || !visible || !authorized || running) {
      return;
    }
    running = true;
    const selected = taskId;
    const version = ++generation;
    const requestAbort = options.abort();
    active = requestAbort;
    const body = {
      task_id: selected,
      include_diff: true,
      include_output_tail: true,
      after_cursor: full ? null : cursor,
      wait_ms: full ? 0 : 15000,
    };
    let delay = 0;
    try {
      let response: any = null;
      try {
        response = await options.fetchReview(body, requestAbort.signal);
      } catch {}
      if (version !== generation || selected !== taskId) {
        return;
      }
      if (response && response.status === 401) {
        authorized = false;
        taskId = "";
        invalidate();
        options.unauthorized();
        return;
      }
      const valid =
        response &&
        response.ok &&
        response.data &&
        String(response.data.task_id) === selected;
      if (valid) {
        failures = 0;
        const nextSignature = identity(response.data);
        const nextCursor =
          typeof response.data.event_cursor === "number" ? response.data.event_cursor : cursor;
        if (full || nextCursor !== cursor || nextSignature !== signature) {
          cursor = nextCursor;
          signature = nextSignature;
          options.render(response.data);
        }
      } else {
        failures += 1;
        delay = Math.min(1000 * 2 ** (failures - 1), 15000);
        if (response && !response.ok) {
          options.error(response.data);
        }
      }
    } finally {
      if (version === generation) {
        active = null;
        running = false;
        if (taskId && visible && authorized) {
          schedule(version, delay);
        }
      }
    }
  }

  return {
    select(nextTaskId: string): void {
      invalidate();
      taskId = nextTaskId;
      cursor = null;
      signature = "";
      failures = 0;
      authorized = true;
      void request(true);
    },
    restart(): void {
      invalidate();
      cursor = null;
      signature = "";
      failures = 0;
      void request(true);
    },
    hide(): void {
      visible = false;
      invalidate();
    },
    show(): void {
      if (!visible) {
        visible = true;
        void request(cursor === null);
      }
    },
    stop(): void {
      invalidate();
      taskId = "";
      cursor = null;
      signature = "";
      failures = 0;
    },
    running(): boolean {
      return running;
    },
  };
}
