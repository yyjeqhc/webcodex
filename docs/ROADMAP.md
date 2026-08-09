# Roadmap

WebCodex is a remote, auditable, bounded execution layer for coding assistants. It is not an embedded model, autonomous agent loop, or full browser IDE.

## Current baseline

- Project-bound MCP and OpenAPI surfaces expose a small canonical capability set.
- Durable tasks, executions, events, results, approvals, resumable review, and bounded output are persisted.
- Server, CLI, and runner share code through workspace library crates with enforced package boundaries.
- Authentication, project grants, allowed roots, path policy, authority mode, and audit evidence remain explicit boundaries.
- Structured validation supports Rust, Node, Python, and Go recipes without installing dependencies or running networked setup hooks.
- The review console, reconnect continuity, read-only LSP navigation, shell profiles, and transport fallbacks are available.

## Next priorities

The next cycle is centered on **model execution friction**, not fleet-management breadth. See [MODEL_EXECUTION.md](MODEL_EXECUTION.md) for the design contract.

1. Make execution lifecycle truthful: structured state, retry safety, and human-readable guidance must agree on whether work definitely did not start, is running, completed, or has an unknown outcome.
2. Add the smallest structured process/argv and script-payload path so ordinary native commands do not require shell quoting; keep `run_shell` as an escape hatch.
3. Generalize the existing validation pattern so one execution can return synchronously when short or continue as the same durable Job when long; add bounded batch Job observation using existing observation tokens.
4. Fix transport-level execution reliability, especially polling dispatch starvation, before relying on higher concurrency; then expose only the practical running/queued/limit facts needed to tune deployments.
5. Keep Job state OS-neutral and MCP-App-ready: normalize Windows output, preserve structured MCP results, and allow an optional conversation-level Orchestrator without making UI or optional MCP 2026 extensions part of execution truth.

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
