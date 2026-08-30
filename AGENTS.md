# AGENTS.md — WebCodex Repository Guide

These are the always-on rules for ordinary repository work. Read only the linked sections needed for the current task. A deeper `AGENTS.md` governs its directory.

## V1 active development

V2 development is paused; V1 is the active product line and is not feature-frozen. Ordinary V1 work may add explicitly requested capabilities as well as correctness, reliability, and test improvements. Keep changes focused and do not weaken safety, credential, process-tree, transport, durability, or boundedness contracts.

## 1. Verify and preserve

- Work only in the repository, worktree, and external target authorized by the user.
- At task start, verify the root, branch, HEAD, status, relevant changes, and recent history. Recheck only after a Git operation, an observed concurrent change, or before committing.
- Treat prompt hashes, paths, branches, and runtime state as expectations to verify, never as reasons to overwrite the actual repository.
- Preserve unrelated work. Do not reset, rebase, restore, clean, or rewrite history unless the user explicitly requests that operation.
- Inspect only the implementation, tests, documentation, and diff relevant to the task; do not bulk-read unrelated material.

## 2. Build the simplest useful product

- Optimize for a workflow that is simple, obvious, and easy to operate.
- Prefer the smallest direct solution. Add abstractions, configuration, modes, compatibility layers, or extension points only for a concrete current need or a demonstrated reliability problem.
- Do not build a general framework for one use case. Wait for a second concrete use case before extracting shared machinery.
- Do not design for hypothetical consumers, tenants, deployment scales, or trust boundaries that the product does not have.
- Protect real boundaries—credentials, public entry points, destructive actions, wrong-target execution, repository history, and published artifacts—without adding policy machinery for imaginary ones.
- A feature that introduces a new externally reachable authority, credential audience, or replaceable runtime target creates a concrete present boundary. Model it explicitly when reusing an existing scope, identity, or lease would broaden existing credentials or allow stale requests to retarget; fewer concepts is not a reason to collapse distinct authority.
- When two designs satisfy the current need, choose the one with fewer concepts, states, configuration paths, and maintenance costs.
- For model-facing execution, prefer structured process/argv and durable Job/observation primitives over shell-text orchestration. Keep shell as an escape hatch; structured lifecycle state is the source of truth for retry safety.
- Keep model-facing ToolSpec/OpenAPI operation descriptions concise and prefer 300 characters or fewer when semantics remain complete. Never delete necessary authority, retry, continuation, uncertainty, safety, or recovery semantics merely for brevity; the canonical tested hard ceiling is `MODEL_TOOL_DESCRIPTION_MAX_CHARS`.
- Treat demonstrated host features such as MCP App orchestration as optional adapters. Core execution and Job semantics must remain protocol-, UI-, transport-, and OS-neutral.
- Never assume a model-facing HTTP/MCP request has stable model-window or Workflow Session identity. Treat requests as stateless unless that exact adapter/protocol contract explicitly supplies a stable `ClientWindow`. Stateless MCP 2026 must not derive hidden continuity from `Mcp-Session-Id`, connection state, credentials, project identity, or prior requests. Transport/audit/session correlation ids are not Workflow Session, model-context-retention, or authority proofs by themselves; use explicit durable identifiers from their owning domain. Connector Tasks, Workflow Sessions, durable Agents/Conversations, and future Agent Tasks are distinct identities and must not be inferred from one another.

Product direction: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## 3. Follow intent and make focused changes

- The requested outcome, scope, prohibited actions, and acceptance conditions are hard constraints. Suggested files, symbols, commands, and step order are guidance unless explicitly mandatory.
- If guidance conflicts with current code or repository conventions, make the smallest adjustment that still satisfies the task.
- Do not knowingly create a bug, inconsistent state, resource leak, compatibility hazard, or false validation result.
- Prefer the smallest coherent change. Follow existing architecture and naming; avoid speculative compatibility, duplicate representations, unrelated cleanup, and broad refactors without a named current need.
- For explicit cross-layer feature work, map the complete authoritative vertical slice before editing and close the existing architecture end to end before local hardening.
- Minimize new concepts rather than the number of touched files. A coherent vertical slice may update several existing layers when each is part of the same current capability.
- Use compiler, type, schema, and exhaustiveness failures to find missing enum, registry, adapter, and projection closure before broadening validation.
- After focused tests pass, perform a separate completeness audit and then a trust/bounds/privacy/replay audit before considering the implementation finished.
- Implementation ownership and independent adversarial review are separate passes. In an implementation-owner pass, prioritize a complete authoritative vertical slice and strong first delivery within the current contract; resolve known correctness issues, including concrete authority/identity boundaries created by the feature, but do not fragment the implementation around speculative reviewer concerns. A later review pass independently challenges the resulting design and implementation.
- Keep only the interfaces actually affected by the change consistent. Do not touch or revalidate unrelated projections merely because they exist.
- Add focused tests for changed behavior when practical. Update documentation when public behavior or operations change.
- When a subsystem already has a dedicated `tests/` module tree, put ordinary new tests there instead of growing production facade files. Keep inline `#[cfg(test)]` blocks small and tightly coupled to private implementation helpers; process, network, and integration fixtures belong in dedicated test modules.
- Do not grow one test file into a multi-domain catch-all. When an already-large test module needs coverage for a distinct lifecycle or contract domain, create or reuse a domain-specific test module and split by canonical ownership, not arbitrary line-count chunks.
- In async tests, required readiness must use a `wait_*` path with one absolute deadline that partial progress never resets. `probe_*` helpers are only for observations where immediate absence is valid or for one iteration inside an already-owned outer deadline; never use a probe when absence means test failure.
- Ask only when required information cannot be discovered, instructions materially conflict, or proceeding could destroy work. Otherwise continue and report any material deviation.

## 4. Validate only changed behavior

Validation is evidence, not ceremony.

- Start with the smallest check that can detect a regression. Stop when sufficient relevant evidence has passed.
- Reuse a successful result from the exact same HEAD when relevant files have not changed.
- Do not rerun the same command unless relevant code changed, a rebase or conflict invalidated it, the earlier run was incomplete or flaky, or the user requested a repeat.
- Documentation-only changes do not require Cargo compilation or tests.
- Rust changes normally require affected formatting, the smallest relevant package check when compilation may be affected, and focused tests.
- For ordinary development builds that need optimized binaries, use the `dogfood` Cargo profile by default (for example, `cargo build --profile dogfood --bin webcodex-server`). Reserve the `release` profile for formal release/publication artifacts.
- Full library or workspace tests, `--all-targets`, ignored tests, real-process harnesses, and E2E scripts are not defaults. Run them only for an explicit user request, release or deployment, a genuinely cross-cutting boundary, broad conflict resolution, or when focused validation cannot cover the change. State the reason first.
- Distinguish current failures from pre-existing failures, expected negative cases, and failures resolved by a successful retry.
- Before finishing, review the diff and confirm whitespace, worktree, conflict, and active-job state.

Testing guidance: [`docs/TESTING.md`](docs/TESTING.md).

## 5. Protect work and keep delivery explicit

- Never print, commit, or expose credentials, authorization headers, private keys, tokens, secret files, or sensitive command output.
- Do not silently overwrite concurrent changes. Prefer guarded or conflict-detecting edits.
- Do not weaken meaningful authentication, authorization, validation, schemas, sandboxing, or tests merely to obtain a green result.
- Do not force-push, move published tags, overwrite releases, destructively reset other work, or rewrite published history without an explicit request naming the operation and target.
- A failed **pre-publication** version tag is not yet a published release identity only when no GitHub Release exists, the npm version is absent, and no successful authoritative release-build exists. Such a tag may be reclaimed only in an explicitly requested release-recovery task through the repository release operator's guarded reclaim path; active/successful build or any publication makes the tag immutable.
- Push, publish, tag, release, deploy, restart services, or alter external systems only when the task explicitly includes it and identifies the destination.
- An explicit development/dogfood deployment of a reviewed commit to named targets is not a release rollout. It may install and restart that exact development build without a version bump, Git tag, GitHub Release, npm publication, or release-artifact preparation.
- For a development/dogfood deployment, change only the named targets, record the requested source commit and the existing build identity (`git_commit`, `git_dirty`, `built_at`), never hide dirty build state, preserve a rollback path, and run focused post-deployment smoke.
- Release or publication rollouts still follow the immutable version/tag/artifact contract in the release guidance below.
- Review status and diff before committing. Do not mix unrelated work or amend an unrelated commit.

Release guidance: [`docs/agent/release-process.md`](docs/agent/release-process.md) and [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md).

## 6. Load domain rules only when relevant

- Public runtime and API surfaces: [`docs/agent/openapi-guidelines.md`](docs/agent/openapi-guidelines.md).
- Workflow Sessions: [`docs/agent/session-model.md`](docs/agent/session-model.md).
- Manual multi-window collaboration: [`docs/agent/manual-window-collaboration.md`](docs/agent/manual-window-collaboration.md).
- Durable Agent identity, Conversation/Wake, and asynchronous Agent work: [`docs/architecture/durable-agent-runtime.md`](docs/architecture/durable-agent-runtime.md).
- Current durable Agent/Conversation/Wake implementation contract: [`docs/architecture/durable-agent-conversation.md`](docs/architecture/durable-agent-conversation.md).
- Authority boundaries: [`docs/agent/permission-model.md`](docs/agent/permission-model.md).
- Architecture decisions: [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md).

Use these documents as sources of truth instead of copying subsystem invariants into every task prompt.

## 7. Report the completed state

Report the outcome, changed files or external resources, validation performed, final Git and job state, material deviations, and remaining risks. State whether push, release, deployment, or service operations were performed. Tool output is evidence, not a substitute for engineering judgment.
