# Built-in context and guidance composition

## Scope

Context material and model guidance are bundled Rust extensions, not a second
execution/plugin runtime. Native Tool Plugins remain Runner-owned capability
providers. This layer does not register outer MCP tools or change ToolCall,
permissions, Project resolution, Session/Job stores, effect certainty, audit,
read revisions, or mutation fences.

The initial registries are immutable static slices. There is no per-Runtime
registry clone, lock, mutable global registration, configuration toggle, Node
process, or hot-reload lifecycle. This deliberately solves the current duplicate
registration/dispatch problem without introducing a general extension framework.

## Context materials

`src/tool_runtime/context_projection/providers.rs` defines each material's key,
Project requirement, scope policy, surface capability, discovery visibility,
unavailable reason, and callable together. The registry lists those definitions
once; the sidecar driver no longer maintains a second string-key dispatch match.
A registered material therefore cannot have a descriptor but no provider.

The registry checks surface availability, Project presence, and scopes before
invoking a provider. A cached instruction observation does not bypass those
checks. Domain providers retain their own authorization of actual resources;
registration is not a grant of authority. Workflow resume remains an observation
of exact authorized candidates, never automatic Session selection.

The driver continues to own trimming, stable deduplication, the eight-request
bound, request order, the independent 20 KiB envelope, and nonfatal unsupported
or unavailable receipts. Instruction projection reserves the complete prospective
envelope, including any requested public workflow and later omission receipts.
It never speculatively invokes later providers to estimate their size. Complete
and incomplete startup instruction snapshots are reused without another load.

The advertised keys and their order are unchanged. Optional goal workflow and
coding guidance chapters use the existing context request mechanism without
being added to every tool descriptor. Adding an internal material requires one
complete provider definition and one registry entry, not a new ToolCall.

`webcodex.coding` is public model guidance with no Project requirement. On MCP
surfaces that support `_wc` sidecars, code implementation or explicitly requested
code review tasks may request `_wc.context=["webcodex.coding"]` and select the
applicable `coding` or `independent_review` role. The chapter is optional and
grants no authority. CLI presentation does not infer task type and has no `_wc`
request route; project instructions remain the source of repository conventions.

## Guidance contributors

`src/tool_runtime/guidance/contributors.rs` owns ordered advice for core safety,
task workflow, validation, Job lifecycle, optional coding/review roles, Direct,
Host Code Mode, and feature-gated experimental Code Mode. The registry projects
the requested target in registration order without sorting, silent deduplication,
or an intermediate collection.

`startup_brief` owns the workflow envelope, model-protocol guidance, deployment
policy, and the orchestration catalog derived from ToolDefinition. Startup and
explicit workflow refresh share this composition. Core guidance applies to any
project task; engineering architecture and refactoring guidance belongs to the
optional coding chapter. `guidance` owns that chapter's role selection and role
projections. Direct, Host Code Mode, and Code Mode still select orchestration
strategy only: task roles do not add a persistent profile or alter execution.
See [Coding workflow](../CODING_WORKFLOW.md#start-or-continue-a-task).

Guidance count and size limits, the 10 KiB instruction-content budget, and the
hard 30 KiB startup / 20 KiB context budgets remain unchanged. Optional coding
material uses the existing request order and trimming receipts; it has no new
budget priority. Workflow version 32 removes the default review roles and points
to the optional chapter. Existing conversations can request
`_wc.context=["webcodex.workflow"]` on supported MCP surfaces to refresh retained
guidance. The output schema reflects the new envelope; there is no persisted
data migration or Runner protocol change.

## Incremental rollout and rollback

This change is independently usable while result projection, UI renderers, and
all Native Tool Plugins retain their previous implementations. No migration
flag, dual execution path, registry readiness state, or persisted
data conversion is required. Built-in providers are present for the entire
binary lifetime rather than activated partway through startup.

Subsequent result/UI refactors must preserve their existing entrypoints and
canonical-recording boundaries in the same way. A new module should replace one
complete non-authoritative behavior at a time, with focused parity/authority
coverage, rather than require unrelated modules to migrate in lockstep.
Reverting this refactor does not need a database or plugin-protocol rollback.
Normal deployment and restart procedures remain separate operational actions.

Third-party callbacks, TS RPC, before/instead/wrap hooks, and dynamic registration
are not exposed here. Disposable handles would be necessary when an actual
reloadable owner is introduced; adding unused disposal infrastructure for
permanent static entries would only add lifecycle states today. An external
bridge must first define bounded calls, failure isolation, ownership/unload,
trust boundaries, and a restricted presentation API; these registries do not
implicitly authorize such a bridge.

## Focused validation

The context projection suite covers authorization, unsupported materials,
request order, primary-result preservation, cached instruction observations,
workflow reservation, independent budgets, and Project-scoped Job attention.
Registry tests additionally reject duplicate discovery keys and prove that
ineligible providers are never invoked. Guidance tests cover composition order,
strategy/task isolation, explicit coding chapter requests, complete bundles, and
deterministic serialization; workflow/startup suites retain schema, policy, and
size-budget checks. The Python preflight validates contributor structure and
bounds directly, without parsing Rust tests or fixing editorial wording.

```sh
cargo test --locked -p webcodex --lib context_projection
cargo test --locked -p webcodex --lib guidance_registry
cargo test --locked -p webcodex --lib builtin_coding_workflow
cargo test --locked -p webcodex --lib startup_brief
```
