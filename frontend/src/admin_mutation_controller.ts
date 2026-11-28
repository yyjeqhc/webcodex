export type MutationKind = "register" | "create" | "enable" | "disable" | "unregister" | "unregister_runner";

export function mutationKindLabel(kind: MutationKind): string {
  return kind === "unregister_runner" ? "Unregister runner" : kind[0].toUpperCase() + kind.slice(1);
}

export type MutationErrorCode =
  | "invalid_request" | "revision_conflict" | "active_jobs_conflict"
  | "idempotency_conflict" | "unsupported_runner_version" | "agent_unavailable"
  | "operation_indeterminate" | "operation_failed" | "network_error" | "unauthorized"
  | "runner_connected" | "runner_online" | "runner_busy" | "runner_maintenance";

export class AdminMutationError extends Error {
  constructor(readonly status: number, readonly code: MutationErrorCode, readonly activeJobs?: number) {
    super(code); this.name = "AdminMutationError";
  }
}

type MutationContext = {
  kind: MutationKind; target: string; body: Record<string, unknown>; key: string;
  generation: number; token: string; controller: AbortController; pending: boolean;
};

type Dependencies = {
  request(kind: MutationKind, token: string, body: Record<string, unknown>, signal: AbortSignal): Promise<unknown>;
  keyFactory(): string;
  refresh(): Promise<void>;
  outcome(message: string): void;
  error(code: MutationErrorCode, context: MutationContext): void;
  pending(target: string, value: boolean): void;
  lock(message: string): void;
};

function aborted(error: unknown): boolean {
  return Boolean(error && typeof error === "object" && "name" in error && error.name === "AbortError");
}

export class AdminMutationController {
  private generation = 0;
  private token = "";
  private contexts = new Map<string, MutationContext>();

  constructor(private readonly deps: Dependencies) {}

  beginSession(token: string): void { this.invalidate(); this.token = token; }
  lock(): void { this.invalidate(); this.token = ""; }
  dispose(): void { this.invalidate(); this.token = ""; }

  start(kind: MutationKind, target: string, body: Record<string, unknown>): MutationContext | null {
    const existing = this.contexts.get(target);
    if (!this.token || existing) return null;
    const context: MutationContext = {
      kind, target, body: { ...body }, key: this.deps.keyFactory(), generation: this.generation,
      token: this.token, controller: new AbortController(), pending: false,
    };
    this.contexts.set(target, context);
    return context;
  }

  retry(target: string): Promise<void> {
    const context = this.contexts.get(target);
    return context ? this.submit(context) : Promise.resolve();
  }

  cancel(target: string): void {
    const context = this.contexts.get(target);
    if (!context?.pending) this.contexts.delete(target);
  }

  has(target: string): boolean { return this.contexts.has(target); }
  isPending(target: string): boolean { return this.contexts.get(target)?.pending === true; }

  async submit(context: MutationContext): Promise<void> {
    if (!this.current(context) || context.pending) return;
    context.pending = true;
    this.deps.pending(context.target, true);
    try {
      // Project lifecycle mutations are journaled Server-side and carry an
      // idempotency key for uncertain-delivery replay. Runner unregistration
      // converges on the target state (absent registration), so a repeat is
      // already safe and needs no key.
      const body = context.kind === "unregister_runner"
        ? { ...context.body }
        : { ...context.body, idempotency_key: context.key };
      await this.deps.request(context.kind, context.token, body, context.controller.signal);
      if (!this.current(context)) return;
      this.deps.outcome(`${mutationKindLabel(context.kind)} completed.`);
      this.contexts.delete(context.target);
      await this.deps.refresh();
    } catch (error) {
      if (!this.current(context) || aborted(error)) return;
      const classified = error instanceof AdminMutationError
        ? error
        : new AdminMutationError(0, "network_error");
      if (classified.code === "unauthorized") {
        this.deps.lock("Administrator authentication required.");
        return;
      }
      this.deps.error(classified.code, context);
      if (classified.code === "revision_conflict" || classified.code === "active_jobs_conflict") {
        await this.deps.refresh();
      }
      if (classified.code === "revision_conflict") this.contexts.delete(context.target);
    } finally {
      if (this.generation === context.generation && this.token === context.token) {
        context.pending = false;
        this.deps.pending(context.target, false);
      }
    }
  }

  private current(context: MutationContext): boolean {
    return this.generation === context.generation && this.token === context.token && this.contexts.get(context.target) === context;
  }

  private invalidate(): void {
    this.generation += 1;
    for (const context of this.contexts.values()) context.controller.abort();
    this.contexts.clear();
  }
}
