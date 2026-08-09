# Roadmap

WebCodex is a remote, auditable, bounded execution layer for coding assistants. It is not an embedded model, autonomous agent loop, or full browser IDE.

## Current baseline

- Project-bound MCP and OpenAPI surfaces expose a small canonical capability set.
- Durable tasks, executions, events, results, approvals, resumable review, and bounded output are persisted.
- Server, CLI, and runner share code through workspace library crates with enforced package boundaries.
- Authentication, project grants, allowed roots, path policy, authority mode, and audit evidence remain explicit boundaries.
- Structured validation supports Rust, Node, Python, and Go recipes without installing dependencies or running networked setup hooks.
- Typed process/script execution can continue as the same durable Job, and models can observe up to eight existing local or Agent Jobs through one bounded read-only batch wait.
- Normal polling dispatch is bounded to two in-flight workers with no local pending queue, so one ordinary long request no longer blocks the next poll or replays execution.
- Runner Job execution defaults to four concurrent Jobs, normalizes operator configuration to the effective range 1 through 64, and exposes authorization-safe running/queued/limit facts through existing runtime observability. The 64 ceiling is the unchanged active Job inventory hard bound, not a new scheduling policy; polling dispatch remains independently fixed at two.
- The review console, reconnect continuity, read-only LSP navigation, shell profiles, and transport fallbacks are available.

## Next priorities

The execution cycle remains centered on **model execution friction**, not
fleet-management breadth. See [MODEL_EXECUTION.md](MODEL_EXECUTION.md) for the
design contract.

1. Execution Phases A–E are complete: lifecycle truth, structured process and script payloads, same-execution Job handoff, bounded batch observation, non-pinning polling dispatch, and practical Job concurrency/observability are implemented.
2. Phase F, Windows output normalization, is the next execution-core phase. Keep Job state OS-neutral and make shell/native stderr UTF-8-stable without changing execution identity or timeout semantics.
3. Preserve structured MCP results and allow an optional conversation-level Orchestrator without making UI or optional MCP 2026 extensions part of execution truth.

## Deferred until there is a current need

- Runner drain/maintenance/self-upgrade and richer fleet dashboards.
- Productized Windows SCM service lifecycle; external/manual service wrapping is sufficient for current dogfood.
- A generic process/service management API, batch Job launch, PTY terminal UX, or polished MCP App UI.
- Depending on MCP Tasks, MRTR, elicitation, or progress extensions before the target host makes them a dependable product contract.

## Completion criteria

A roadmap item is complete only when its public contract is documented, focused and regression validation pass, failure behavior is explicit, and deployment or rollback guidance exists when operations are affected.

## Non-goals

- Built-in model selection, prompt loops, context compaction, or token budgeting.
- Full IDE replacement or arbitrary computer use.
- Autonomous deployment or production mutation by default.
- Compatibility aliases for hypothetical consumers.
- Treating tool count, test count, or lines of code as product progress.
