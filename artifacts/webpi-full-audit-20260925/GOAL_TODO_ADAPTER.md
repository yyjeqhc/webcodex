# WebPi-native Goal/Todo adapter design

## Decision

Do not introduce a second durable task database. WebPi already owns the authoritative lifecycle primitives needed for long-running work: Goal, AgentTask, WorkflowSession, Job and validation evidence. The Pi reference plugins are useful for interaction semantics, not as a storage layer to transplant.

## Reference semantics retained

From `pi-goal-x` / cachefix: revisioned goal contract, bounded ledger/projection, scheduler/auditor separation, cache-friendly stable prompt projection. From `@juicesharp/rpiv-todo`: dependency graph, cycle rejection, one item `in_progress` by default, explicit blockers, and never marking work complete while required validation is red.

## Mapping

- **Goal**: one durable objective; revisioned and project-visible under existing authority.
- **AgentTask**: one durable executable work item. It owns attempt/reconciliation state; task completion never implicitly completes its Goal.
- **Goal↔AgentTask association**: identity/control relation only, never an authority grant.
- **WorkflowSession**: the audit/message/context carrier for work performed against a task or goal.
- **Job**: execution identity for commands that outlive synchronous grace. Observation always continues the same Job; a missing response is not retry authority.
- **Todo presentation adapter**: derives `pending / in_progress / completed / blocked`, dependency edges, blocker text and validation gates from the above objects plus a small bounded planning document. It must not duplicate AgentTask execution state.

## Adapter invariants

1. One source of truth for execution state: AgentTask/Job, never a plugin-local copy.
2. Dependency edges are acyclic; updates require the current planning revision.
3. At most one default `in_progress` todo unless an explicit parallel group exists.
4. Completion requires all declared validation gates green.
5. A failed/unknown execution cannot be converted into `completed`; unknown stays attention-required until reconciled.
6. Goal/task/project references grant no additional authority. Every execution reauthorizes the concrete project/task.
7. Model-facing projections are bounded and cache-stable; private instructions, terminal text and secrets do not enter audit summaries.
8. The adapter is optional presentation/control glue. Existing Goal/AgentTask APIs remain canonical and independently usable.

## WebPi implementation shape

Preferred path for the eventual webpage GPT plugin: expose existing Goal/AgentTask/Session/Job tools directly through WebPi MCP/plugin surfaces, then add a thin skill/presentation layer for plan/todo UX. Do not ship a Pi extension that bypasses WebPi's communication scopes or writes an independent todo store.

The current runtime credential lacks `communication:read/manage`, so native Goal mirroring is intentionally blocked. This design can be implemented/tested in source, but live Goal creation must wait for those normal WebPi scopes; no alternate interface will be used to bypass the denial.
