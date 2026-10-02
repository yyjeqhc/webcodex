import { randomUUID } from "node:crypto";
import { BridgeError, Catalog, fits, isObject, nativeResult, requireCondition } from "./catalog.mjs";

const fields = {
  sessions: ["action"],
  tools: ["action", "session", "query", "cursor", "limit"],
  describe: ["action", "session", "tool"],
  call: ["action", "session", "request", "calls", "timeout_ms", "deadline"],
  refresh: ["action", "session"],
  history: ["action", "session", "limit"],
};

/** Bounded application gateway, carried by the existing native Plugin transport. */
export class AgentEnvironmentBridge {
  constructor(createEnvironment) {
    this.createEnvironment = createEnvironment;
    this.environment = undefined;
    this.busy = false;
    this.poisoned = false;
    this.generation = 0;
    this.request = randomUUID();
  }

  identity() { return this.environment?.identity(); }
  assertIdentity() {
    const current = this.identity();
    requireCondition(current && current.session === this.initialIdentity.session && current.native_session_id === this.initialIdentity.native_session_id && current.cwd === this.initialIdentity.cwd && current.model === this.initialIdentity.model, "native_session_changed");
    return current;
  }
  catalog() {
    const current = this.assertIdentity();
    return new Catalog(this.environment.tools(), JSON.stringify([current, this.generation]));
  }

  status() {
    return { ...this.identity(), next_request: this.request, quarantined: this.poisoned, native_idle: this.environment.isIdle() };
  }

  async execute(input, signal) {
    let started = false;
    let ownsLock = false;
    let nativeRequest;
    try {
      requireCondition(isObject(input) && fields[input.action], "invalid_action");
      requireCondition(Object.keys(input).every((key) => fields[input.action].includes(key)), "invalid_arguments");
      requireCondition(!signal?.aborted, "cancelled_before_dispatch");
      requireCondition(!this.busy, "native_session_busy");
      this.busy = true;
      ownsLock = true;
      if (input.action === "sessions") {
        if (!this.environment) {
          requireCondition(!this.poisoned, "native_environment_quarantined");
          // Native extension loading/session_start can itself have effects.
          started = true;
          this.environment = await this.createEnvironment();
          this.initialIdentity = this.identity();
        }
        return { native_dispatch_state: started ? "completed" : "not_started", ...this.status() };
      }
      requireCondition(this.environment && input.session === this.identity().session, "native_session_unavailable");
      this.assertIdentity();
      switch (input.action) {
        case "tools": return { native_dispatch_state: "not_started", ...this.status(), ...this.catalog().page(input) };
        case "describe": {
          const catalog = this.catalog();
          return { native_dispatch_state: "not_started", ...this.status(), catalog: catalog.revision, tool: catalog.describe(input.tool) };
        }
        case "history": {
          requireCondition(input.limit === undefined || Number.isInteger(input.limit), "invalid_limit");
          const limit = Math.max(1, Math.min(16, input.limit ?? 8));
          return { native_dispatch_state: "not_started", ...this.status(), history: this.environment.history(limit).map((item) => ({ id: item.toolCallId, tool: item.toolName, is_error: item.isError === true, result: nativeResult(item) })) };
        }
        case "refresh": {
          requireCondition(!this.poisoned && this.environment.isIdle(), "native_session_quarantined_or_busy");
          // Invalidate observations even if reload fails partway through native hooks.
          this.generation++;
          started = true;
          await this.environment.refresh();
          return { native_dispatch_state: "completed", ...this.status(), catalog: this.catalog().revision };
        }
        case "call": break;
        default: throw new BridgeError("invalid_action");
      }
      requireCondition(!this.poisoned && this.environment.isIdle(), "native_session_quarantined_or_busy");
      requireCondition(input.request === this.request, "stale_request_no_replay");
      const budget = input.timeout_ms ?? 20_000;
      requireCondition(Number.isInteger(budget) && budget > 0, "invalid_timeout");
      requireCondition(input.deadline === undefined || (Number.isSafeInteger(input.deadline) && input.deadline > Date.now()), "deadline_before_dispatch");
      const admitted = this.catalog().admit(input.calls);
      requireCondition(!signal?.aborted, "cancelled_before_dispatch");
      nativeRequest = this.request;
      // Consume before native dispatch, including failures. Observation is never replay authority.
      this.request = randomUUID();
      const controller = new AbortController();
      const onAbort = () => controller.abort();
      signal?.addEventListener("abort", onAbort, { once: true });
      const timeout = Math.min(90_000, budget, input.deadline === undefined ? Infinity : Math.max(1, input.deadline - Date.now()));
      const timer = setTimeout(() => controller.abort(), timeout);
      let stopListening;
      const interrupted = new Promise((resolve) => {
        const abort = () => resolve({ interrupted: true });
        controller.signal.addEventListener("abort", abort, { once: true });
        stopListening = () => controller.signal.removeEventListener("abort", abort);
      });
      try {
        const operation = this.environment.invoke(admitted.map((call) => ({ ...call, id: `${nativeRequest}:${call.id}` })), {
          signal: controller.signal,
          beforeDispatch: () => {
            requireCondition(!controller.signal.aborted, "cancelled_before_dispatch");
            // Lazy native MCP discovery may add unrelated tools during startup.
            // Recheck exact called schemas/exposure, not the entire catalog hash.
            this.catalog().admit(input.calls);
            started = true;
          },
        }).then((results) => ({ results }), (error) => ({ error }));
        const outcome = await Promise.race([operation, interrupted]);
        if (outcome.interrupted) {
          // Even a native abort acknowledgement cannot undo effects already performed.
          this.poisoned = true;
          void this.environment.abort().catch(() => {});
          return { ...this.status(), request: nativeRequest, native_dispatch_state: started ? "outcome_unknown" : "not_started", error: started ? "native_deadline_or_cancel_after_dispatch" : "cancelled_before_dispatch", no_replay: true };
        }
        if (outcome.error) throw outcome.error;
        const byId = new Map();
        for (const result of outcome.results) {
          requireCondition(!byId.has(result.toolCallId), "native_result_identity_mismatch");
          byId.set(result.toolCallId, result);
        }
        requireCondition(byId.size === admitted.length, "native_result_identity_mismatch");
        const results = admitted.map((call) => {
          const result = byId.get(`${nativeRequest}:${call.id}`);
          requireCondition(result && result.toolName === call.name, "native_result_identity_mismatch");
          return { id: call.id, tool: call.name, is_error: result.isError === true, result: nativeResult(result) };
        });
        const response = { ...this.status(), request: nativeRequest, native_dispatch_state: "completed", results };
        requireCondition(fits(response, 256 * 1024, 3000, 13), "native_response_bounds_exceeded");
        return response;
      } finally {
        clearTimeout(timer);
        stopListening();
        signal?.removeEventListener("abort", onAbort);
      }
    } catch (error) {
      if (started) this.poisoned = true;
      return { ...(this.environment ? this.status() : {}), ...(nativeRequest ? { request: nativeRequest } : {}), native_dispatch_state: started ? "outcome_unknown" : "not_started", error: error instanceof BridgeError ? error.code : "native_environment_failure", no_replay: true };
    } finally { if (ownsLock) this.busy = false; }
  }

  async close() { if (this.environment) await this.environment.close(); }
}
