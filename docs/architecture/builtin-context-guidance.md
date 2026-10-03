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

The seven advertised keys and their order are unchanged. Optional goal workflow
chapters remain callable through the stable workflow discovery mechanism without
being added to every tool descriptor. Adding an internal material requires one
complete provider definition and one registry entry, not a new ToolCall.

## Guidance contributors

`src/tool_runtime/guidance/contributors.rs` owns the ordered advice for core
safety, implementation, validation, Job lifecycle, Direct, Host Code Mode, and
feature-gated experimental Code Mode. The registry selects core or the exact
request-local strategy and serializes contributors in registration order. It
neither sorts nor silently deduplicates advice, and requires no intermediate
collection allocation.

`startup_brief` still owns the workflow envelope, model-protocol guidance,
deployment policy, role selection, and the orchestration catalog derived from
ToolDefinition. Startup and explicit workflow refresh use the same composition.
Profile selection remains model guidance only, not admission or Session state.
The migration preserves all 31 existing core/profile guidance strings and their
order; this is not a policy or wording change.

## Incremental rollout and rollback

This change is independently usable while result projection, UI renderers, and
all Native Tool Plugins retain their previous implementations. No migration
flag, dual execution path, registry readiness state, schema refresh, or persisted
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
profile isolation, complete bundles, and deterministic serialization; existing
workflow/startup suites retain schema, policy, and size-budget checks.

```sh
cargo test --locked -p webcodex --lib context_projection
cargo test --locked -p webcodex --lib guidance_registry
cargo test --locked -p webcodex --lib builtin_coding_workflow
cargo test --locked -p webcodex --lib startup_brief
```
