# AGENTS.md — WebCodex Repository Guide

These rules apply to ordinary repository work. Read linked domain guidance only when relevant; a deeper `AGENTS.md` governs its directory.

WebCodex is actively developed. Requested features, fixes, and reliability improvements are welcome; preserve credential, process-tree, transport, durability, and boundedness contracts.

## 1. Verify and preserve

- Work only in the repository, worktree, and external targets authorized by the user.
- At task start, verify root, branch, HEAD, status, relevant changes, and recent history. Recheck after Git operations, observed concurrent changes, or before committing.
- Treat prompt paths, hashes, branches, and runtime state as expectations to verify, not permission to overwrite actual state.
- Preserve unrelated and concurrent work. Use guarded edits; do not reset, rebase, restore, clean, or rewrite history without an explicit request for that operation.
- Read only implementation, tests, documentation, and diffs relevant to the task.

## 2. Design for the current need

- Make workflows simple and obvious. Choose the smallest coherent solution with the fewest concepts, states, and maintenance costs. Add abstractions, configuration, or compatibility only for a concrete need; do not build for hypothetical consumers or scale.
- Treat existing code, comments, and tests as evidence of current or historical behavior, not permanent product requirements. Trace current entry points, consumers, and Git history before retaining or removing a concept; retire obsolete configuration, adapters, and dedicated tests together while preserving real boundaries.
- Protect real boundaries: credentials, public entry points, destructive actions, execution targets, history, and published artifacts. Model new authority, credential audiences, and replaceable targets explicitly when sharing scopes, identities, or leases would broaden access or allow stale requests to retarget.
- Prefer structured process/argv and durable Job/observation primitives for model-facing execution. Use shell for shell semantics and short, tightly related command chains when that avoids unnecessary sequential tool round trips; keep unrelated effects, validation boundaries, and consequential mutations separate. Structured lifecycle state owns retry safety.
- Treat model-surface directness as preferred exposure and ergonomics, not as execution authority. A tool that is otherwise admitted may use an available generic dispatch path without bypassing its canonical schema, authorization, permission, capability, or effect checks; hidden/unadmitted tools remain unavailable.
- Design model-facing tool contracts for turn economy. Keep hard rejection for semantic ambiguity, authority changes, stale fences, replay/retry uncertainty, and uncertain effects. Treat model preferences, presentation limits, schema-cost targets, and other ergonomic budgets as normalization or soft targets unless exceeding them would violate a real correctness, transport, or resource-safety bound. Do not spend a model turn correcting a server-known mechanical detail.
- Keep one canonical machine representation per fact or follow-up. If removing a model-facing result field would leave the same business interpretation and the same unambiguous next action, omit it by default. Keep static/internal taxonomies and forensic bookkeeping in runtime types, tests, telemetry, or exceptional diagnostics instead of making the model memorize them. Model-facing compatibility requires a named concrete consumer; historical fields, aliases, and duplicate projections are not retained merely because an older implementation emitted them. See [`docs/agent/tool-contract-guidelines.md`](docs/agent/tool-contract-guidelines.md).
- Keep ToolSpec/OpenAPI descriptions self-contained and accurate about selection, authority, effects, retries, continuation, uncertainty, and recovery. Guide file transfer toward host-native import/export paths when available instead of routing complete binary payloads through model-generated Base64. Use up to `MODEL_TOOL_DESCRIPTION_MAX_CHARS` when needed; arbitrary brevity must not remove semantics.
- Keep core execution and Job semantics protocol-, UI-, transport-, and OS-neutral. Host-specific features such as MCP App orchestration belong in optional adapters.
- HTTP/MCP requests are stateless unless their exact adapter contract supplies a stable `ClientWindow`. Never infer Workflow Session, model-context retention, or authority from transport/audit IDs, credentials, projects, or prior requests. Keep Connector Tasks, Workflow Sessions, durable Agents/Conversations, and Agent Tasks distinct; follow the Session and Agent contracts linked below.

Product direction: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## 3. Implement and review

- User outcomes, scope, prohibitions, and acceptance conditions are hard constraints. Suggested files and steps are guidance unless explicitly mandatory; adapt them to verified code and conventions.
- Follow existing architecture and naming. Avoid unrelated cleanup, duplicate representations, speculative compatibility, and broad refactors without a current need.
- For cross-layer features, map the authoritative path before editing and complete it end to end. Update affected enums, registries, schemas, adapters, and projections; do not touch unrelated interfaces merely because they exist.
- Use compiler, type, schema, and exhaustiveness failures to locate missing integration before broadening validation.
- Add focused tests where practical and update documentation when public behavior or operations change.
- Before creating a GitHub issue, read the **Reporting issues** guidance in [`CONTRIBUTING.md`](CONTRIBUTING.md). Include relevant environment and exact evidence you can discover yourself, distinguish reproduced facts from source-based inference or user reports, and never invent missing details or a root cause.
- Review the completed diff separately from implementation. Check completeness, then relevant authority, bounds, privacy, and replay risks. Match review depth to the change; resolve demonstrated issues without designing around hypothetical reviewer concerns.
- Ask only when required information cannot be discovered, instructions materially conflict, or proceeding could destroy work. Otherwise continue and report material deviations.

## 4. Validate changed behavior

- Start with the smallest check that can detect a regression; stop when sufficient relevant evidence passes. Reuse results when the validated source and relevant dependencies are unchanged. Repeat only after relevant changes, invalidating conflicts, incomplete/flaky runs, or a user request.
- Documentation-only changes need no Cargo compilation or tests. Rust changes normally need the smallest relevant package check when compilation may be affected and focused tests. Treat Rust formatting as finalization, not per-edit validation: do not run `cargo fmt` after each mutation or intermediate commit; run it once after relevant Rust source stabilizes and before final diff/commit/closeout, then rerun only if later Rust edits can change formatting. A dedicated final formatting commit is fine when branch history policy permits; CI/release formatting checks remain final-state gates.
- Use the `dogfood` Cargo profile for optimized development builds; reserve `release` for formal release/publication artifacts.
- Full library/workspace tests, `--all-targets`, ignored tests, real-process harnesses, and E2E are not defaults. Use them for explicit requests, release/deployment, cross-cutting boundaries, broad conflict resolution, or gaps in focused coverage. State the reason first.
- Put tests in existing dedicated module trees, grouped by domain. Keep inline `#[cfg(test)]` blocks small and tied to private helpers; process/network/integration fixtures belong in dedicated test modules.
- Process fixtures must mirror production I/O ownership: absent stdin means EOF, explicit input means those bytes followed by EOF, and stdout/stderr must be drained or captured without pipe deadlocks. Never inherit the invoking terminal by accident. Prefer explicit config inputs and child-local environment over process-global mutation.
- For console-only or flaky failures, compare the same source with one boundary changed and add a deterministic regression that also fails in headless CI. Retries, longer timeouts, and reduced concurrency are diagnostic/resource controls, not substitutes for fixing the demonstrated cause.
- Async readiness must use a `wait_*` path with one absolute deadline that progress never resets. Use `probe_*` only when immediate absence is valid or inside an already-owned deadline.
- Treat a normal `execution_state=pending` result as the same canonical execution, never as retry authority. Keep its exact continuation as fallback, continue independent work in the same turn, and let ordinary exact Window/Project/Session calls surface sparse terminal `job_attention` when available. Do not immediately poll/follow the continuation while independent work remains. Use `observe_jobs` for logs/details/recovery, `wait_for_job_terminal` only when terminal outcome is the hard dependency and no independent work remains, explicit `stop_job(confirm=true)` for control, and `list_jobs` only for inventory/identity recovery. `context_request=["jobs.attention"]` remains an explicit Project-level fallback, not the normal Session sidecar. Job tools remain outside nested Code Mode.
- Distinguish current failures, pre-existing failures, expected negative cases, and failures resolved by retry. Never weaken authentication, authorization, validation, schemas, sandboxing, or tests to get a passing result.

Testing guidance: [`docs/TESTING.md`](docs/TESTING.md).

## 5. Deliver only what is authorized

- Never print, commit, or expose credentials, authorization headers, private keys, tokens, secret files, or sensitive command output.
- Before committing, review status and diff. Keep unrelated work separate; do not amend an unrelated commit.
- Push, publish, tag, release, deploy, restart services, or alter external systems only when explicitly requested with a named destination.
- Never force-push, move published tags, overwrite releases, or rewrite published history without an explicit request naming the operation and target. Release operations must also obey the immutable artifact contract below; failed pre-publication tag recovery is permitted only through its explicitly authorized guarded reclaim path.
- Named development/dogfood deployments are separate from releases. Deploy the reviewed source commit, record its build identity without hiding dirty state, preserve rollback, and run focused smoke. They do not authorize version bumps, tagging, or publication.
- Before finishing, check the final diff, whitespace, worktree, conflicts, and active jobs. Report the outcome, changed files/targets, validation, Git/job state, material deviations, and remaining risks. State whether push, publication, deployment, or service operations occurred.

For release, recovery, or deployment work, follow [`docs/agent/release-process.md`](docs/agent/release-process.md) and, for release/publication gates, [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md).

## 6. Load relevant domain rules

- Public runtime and API surfaces: [`docs/agent/openapi-guidelines.md`](docs/agent/openapi-guidelines.md).
- Model-facing tool contract style and friction policy: [`docs/agent/tool-contract-guidelines.md`](docs/agent/tool-contract-guidelines.md).
- Workflow Sessions and request identity: [`docs/agent/session-model.md`](docs/agent/session-model.md).
- Manual multi-window collaboration: [`docs/agent/manual-window-collaboration.md`](docs/agent/manual-window-collaboration.md).
- Durable Agent identity, Conversation/Wake, and asynchronous Agent work: [`docs/architecture/durable-agent-runtime.md`](docs/architecture/durable-agent-runtime.md).
- Current durable Agent/Conversation/Wake implementation contract: [`docs/architecture/durable-agent-conversation.md`](docs/architecture/durable-agent-conversation.md).
- Authority boundaries: [`docs/agent/permission-model.md`](docs/agent/permission-model.md).
- Architecture decisions: [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md).

Use these as domain sources of truth rather than duplicating their detailed contracts here.
