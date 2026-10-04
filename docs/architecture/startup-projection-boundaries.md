# Startup and Context projection boundaries

## Concrete dependency problem

Before this change, explicit Context instruction projection and Skill/Plugin
catalog construction imported `startup_brief`, even though they need neither its
workflow composition nor Session/continuation verdict. Instruction shortening
also repeated the same scope-sensitive read-more rule in the Context budget and
two startup hard-budget passes. That made a local formatting change span multiple
owners and increased the risk of inconsistent continuation hints.

## Leaf ownership

`instruction_projection.rs` accepts already-observed instruction snapshots and
contains deterministic source/status/delta/body projection. It receives no
ToolRuntime, loader, auth evaluator, Session store or execution API. Context and
startup call the same leaf without reobserving files. Six existing instruction
functions retain their bodies; the three source-shortening sites share
`trim_instruction_source`. This preserves UTF-8/JSON-escaped byte accounting,
mid-line conservative rereads, source identity and the rule that Runner-scoped
instructions never acquire a project-relative continuation.

`startup_catalog.rs` owns the existing typed Skill/Plugin selection metadata and
its greedy-prefix JSON budget. Skill and Plugin discovery owners import it
directly. Their scope checks, actual discovery calls and catalog identities remain
unchanged. Shared recovery hint constants prevent available-catalog and aggregate
startup trimming from drifting. This is not another catalog store or registry.

`projection_text.rs` is only shared JSON measurement and string-prefix accounting.
It does not decide which domain loses content first. Those decisions remain in
startup and Context because their envelopes and priorities differ.

## Explicit Context input

`ContextProjectionRequest` names requested keys, the already-resolved Project,
auth, material capabilities, request-local guidance, Window and optional bootstrap
instruction observation. It replaces the long positional argument list in both
normal and early-failure sidecar paths. The early-failure path still supplies no
Project or instruction snapshot; the normal path retains the exact bootstrap
Project/snapshot pairing. Provider gates still run before using cached data or
loading missing observations. No default constructor guesses a target or authority.

## Kept together deliberately

The startup assembler, continuation projection, issues/verdict and aggregate
hard-limit policy remain in `startup_brief`. They share cross-domain priorities
and currently have no independent consumer that justifies another service graph.
Their composition/continuation/issues/verdict function bodies are unchanged.
The hard limiter keeps the same order, thresholds and fallback while delegating
only source shortening. Guidance/provider authority is not made dynamically
replaceable. The existing owned ResultProjection boundary is not redesigned.

## Validation and adoption

New source-trimming tests compare every small budget across ASCII, escaped,
Unicode and newline boundaries with an independent serialized-prefix oracle.
Existing startup catalog tests already compare complete envelopes with the old
prefix-selection oracle. Existing startup/Context tests cover scopes, cached
snapshot reuse, request-local guidance, source changes, independent material and
startup byte budgets, and unavailable-provider nonfatal behavior. Parser/tool
schemas, recorded evidence and persisted formats do not change.

This PR is independently based on the same main as the Job/Git and Work Result
read-lifetime changes. It changes no App template and requires no migration,
configuration flag, plugin bridge, service restart or deployment in order to be
merged independently. Runtime deployment remains a separate explicit operation.
