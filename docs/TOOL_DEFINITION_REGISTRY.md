# ToolDefinition registry migration plan

This document is a developer architecture note for moving WebCodex runtime tool
declarations toward a registry-driven `ToolDefinition` model over several
phases. It records the implemented migration state and the intended direction;
it does not claim that every target schema, OpenAPI, MCP, OAuth, or policy
generation path is complete today.

## Current state

The runtime registry and runtime surface have already been mechanically split
into smaller modules:

- `src/tool_runtime/registry/mod.rs`
- `src/tool_runtime/registry/input_schemas.rs`
- `src/tool_runtime/registry/output_schemas.rs`
- `src/tool_runtime/registry/annotations.rs`
- `src/tool_runtime/registry/tool_specs.rs`
- `src/tool_runtime/tool_call.rs`
- `src/tool_runtime/tool_definition.rs`
- `src/tool_runtime/tool_catalog.rs`
- `src/tool_runtime/tool_policy.rs`
- `src/tool_runtime/tool_inputs.rs`
- `src/tool_runtime/tool_result.rs`
- `src/tool_runtime/tool_spec.rs`
- `src/tool_runtime/surface.rs`
- `src/tool_runtime/dispatch.rs`

That split reduces file size and localizes schema/spec/surface/dispatch helpers.
`tool_definition.rs` now declares known runtime tool names, model-facing
visibility, manifest category, runtime metadata, and agent capability.
Agent capability lookup, model-facing hidden checks, manifest category lookup,
known-tool checks, MCP annotations, manifest summaries, permission/session
policy, and session ledger classification are now definition-backed.
`metadata.rs` now holds the `ToolMetadata` value type,
unknown-tool fallback, and explicit compatibility metadata for non-runtime route
names such as `delete_files`; runtime tool metadata is declared on
`ToolDefinition`. Missing capability definitions fail closed. Schemas, output
contracts, OpenAPI operation shapes, and parts of discovery are still maintained
across hand-written structures and kept aligned by contract tests.
Public `ToolSpec` rows are still hand-written for descriptions and input
schemas, but a shared constructor now derives each row's `output_schema` and MCP
`annotations` fields from the canonical spec name. `ToolDefinition` order is
canonical for known-tool and model-facing discovery order; `known_tool_names()`
and model-hidden name discovery derive directly from the definitions, and
`tool_specs()` emits model-visible specs by iterating that canonical definition
order and looking up the hand-written description/input-schema declaration.
The full-schema `list_tools` category map now formats model-visible discovery
groups declared beside `ToolDefinition`, so registry code no longer owns a
separate category membership table or needs the full `ToolSpec` list to build
categories.
Recommended-flow entries are also definition-backed and rendered into both the
short `list_tools` hints and the structured `tool_manifest` flow list.
Session ledger classification, session guard checks, cross-project session
escape checks, validation-output capture classification, and dev permission
risk labels now route through `ToolDefinition` semantic helpers/facades instead
of local tool-name matches at each call site. Current-session fallback
eligibility is also definition-backed: control tools, tools with required
business `session_id`, and tools that create/bind sessions do not implicitly use
the caller's current-session binding.

### Current three-layer relationship

WebCodex is intentionally in a middle state with three related layers:

- `ToolCall` enum: still the typed execution IR and JSON parser boundary.
  A runtime call is only accepted when its name resolves to a `ToolDefinition`;
  unknown names and legacy metadata-only names do not silently deserialize into a
  runtime call.
- `ToolDefinition` registry: the canonical runtime tool declaration layer for
  known names, ordering, model-facing visibility, manifest category, runtime
  metadata, session/permission policy facades, and agent capability. The current
  registry has 67 runtime definitions: 66 model-facing tools plus hidden
  disabled `run_codex`.
- Legacy metadata fallback: `metadata.rs` still owns the `ToolMetadata` value
  type, the safe unknown-name fallback, and a small explicit compatibility
  allowlist for non-runtime route metadata. At this point the only metadata-only
  compatibility entry is `delete_files`, retained for legacy dedicated HTTP route
  metadata and intentionally not accepted by `ToolCall`.

Definition-backed paths today include known-tool checks, parser acceptance,
model-hidden and model-visible discovery, public `ToolSpec` ordering, MCP
annotations, tool manifest categories, recommended-flow summaries,
session-ledger classification, current-session fallback eligibility,
permission-risk labels, and agent capability dispatch checks.

Fallback-backed paths remain deliberately narrow. `runtime_tool_metadata()` and
the metadata facade can still return a safe `Unknown` metadata record for names
outside the runtime registry, and `lookup_tool_metadata()` can return the
explicit non-runtime `delete_files` compatibility metadata. These fallbacks are a
migration bridge, not the long-term design. Runtime tool metadata should be added
to `ToolDefinition`, not to `metadata.rs`.

The `#![allow(dead_code)]` on `tool_definition.rs` is also migration residue.
The definition layer intentionally exposes helper methods and constants ahead of
each call site moving over, and some of those helpers are used only by specific
test configurations or future registry generation steps. The goal is to narrow
or remove that module-wide allowance once the remaining helper surface is either
used by production code, kept behind `#[cfg(test)]`, or deleted.

## Current problems

- Tool names are still repeated in `ToolCall`, `tool_name()`, parser paths,
  OpenAPI descriptions, MCP discovery, `tool_manifest`, and tests.
- Input and output schemas are still hand-written separately from the execution
  IR and from GPT Action flattened fields.
- `ToolMetadata`, registry `ToolSpec`, OAuth runtime scope policy, OpenAPI
  accepted names, MCP annotations, and `tool_manifest` visibility/category data
  can drift when a new tool is added or an existing tool changes.
- Legacy metadata fallback remains as an explicit migration bridge for
  non-runtime route metadata and unknown-name safety. New runtime tools should
  not extend that fallback.
- `allow(dead_code)` still exists on a few runtime migration modules, including
  the ToolDefinition layer, and should be narrowed as soon as helper usage is
  clear.
- Session id behavior and project resolution behavior are encoded in variant
  helpers such as `session_id()` and `project()`, plus call-site logic.
- Guard denial, session recording, and redaction/logging rules depend on broad
  match statements and inferred risk/path behavior. Those large matches are
  easy to extend incompletely when adding shell-like, write-like, destructive,
  or session-aware tools.
- Contract tests catch drift, but they do not remove the maintenance cost of
  updating many declaration sites for each new tool.

## Stable core to preserve

The registry migration should be incremental. These parts remain the stable core
while declarations are centralized:

- `ToolCall` remains the execution IR. It is the typed parser boundary between
  external JSON and concrete runtime handlers.
- `ToolRuntime` and `ToolKernel` remain the unified dispatch path. The kernel
  continues to handle metadata-backed OAuth checks, session recording, parsing,
  and dispatch to existing runtime handlers.
- `ToolMetadata` remains the risk, scope, and policy value type. Runtime tool
  metadata is now declared on `ToolDefinition`; `metadata.rs` keeps only the
  compatibility facade and non-runtime route fallback entries.
- Existing contract tests must stay in place. A registry-driven model should add
  stronger invariants before deleting older drift tests.

## Target model

The long-term target is a central `ToolDefinition` declaration for each runtime
tool. It should become the single source for stable metadata while leaving
execution handlers and the `ToolCall` IR explicit.

Each definition should declare:

- `name`: canonical runtime tool name.
- `visibility`: whether the tool is model-facing, hidden from model-facing
  surfaces, or limited to internal/explicit dispatch.
- `category`: tool_manifest and discovery grouping.
- `risk`: read-only, project write, job run, account manage, or unknown.
- `oauth_scope`: required delegated OAuth scope, or explicit non-delegable
  policy where applicable.
- `input_schema`: runtime JSON input schema and flattened GPT Action fields
  needed by `callRuntimeTool`.
- `output_schema`: runtime output schema for discovery and contract tests.
- `session_id_behavior`: whether `session_id` is business input, recorder
  metadata, optional project-session association, current-session fallback, or
  unsupported.
- `project_resolution_behavior`: whether a project is required, optional, absent,
  or resolved through special-case logic.
- `agent_capability`: whether the tool requires no agent, an agent project,
  shell capability, patch capability, file capability, job capability, or another
  explicit executor feature.
- `shell_like`, `write_like`, `destructive`, and `read_only` flags for guards,
  session policies, and UI annotations.
- `redaction_logging_policy`: input summary, output summary, error summary,
  secret-looking key handling, path handling, job id handling, and event
  recording limits.
- `openapi_exposure`: dedicated GPT Action operation, generic
  `callRuntimeTool` only, hidden, or forbidden.
- `mcp_exposure`: listed in `tools/list`, callable but hidden, or forbidden.
- `tool_manifest_exposure`: category, risk summary, path hints, and recommended
  flow placement.

The definition model should support generated mirrors only after the handwritten
behavior has been proven equivalent. Generated output should be reviewed through
tests before replacing any security-sensitive matches.

## Non-goals

- Do not rewrite the `ToolCall` enum in one migration.
- Do not immediately delete existing `ToolMetadata`.
- Do not reduce contract, OAuth, scope, session, MCP, or OpenAPI safety tests.
- Do not automatically expose `run_codex` on model-facing surfaces. Hidden
  behavior remains explicit until there is a separate opt-in design and tests.
- Do not weaken OAuth checks, shared-key OAuth bridge constraints, read-only
  session guards, destructive-tool checks, or model-facing visibility rules.
- Do not turn this into a mandatory one-shot migration. Each phase must be
  reviewable and able to preserve runtime behavior.

## Migration route

### Phase 0: split registry modules

Current baseline. The registry code is split across `input_schemas.rs`,
`output_schemas.rs`, `annotations.rs`, and `tool_specs.rs`, but each file still
contains hand-written declarations.

### Phase 1: adding-tool checklist

Document the complete set of files, tests, and policy decisions required when a
new runtime tool is added. The goal is not to reduce the work yet; the goal is to
make every declaration site explicit so reviewers can catch omissions.

### Phase 2: central ToolDefinition metadata mirror

Introduce a `ToolDefinition` declaration that records the same facts previously
spread across `ToolMetadata`, tool specs, tool manifest categories, visibility
filters, and policy matches. Keep parity tests against existing hand-written
structures while routing low-risk runtime behavior through the definitions.

Started: `src/tool_runtime/tool_definition.rs` contains the first explicit
declaration for known tool names, model-facing visibility, manifest category,
runtime metadata, and agent capability. Agent capability lookup,
model-facing hidden checks, known-tool checks, and manifest category lookup now
read from the declaration/facade. OAuth runtime tool scope policy reads the same
metadata facade so legacy non-runtime route metadata remains covered. MCP
annotations, permission decisions, session guard classification, and session
ledger classification also read metadata through that facade. The old runtime
metadata table has been
removed; `lookup_tool_metadata()` now returns definition metadata for runtime
tools and falls back only for non-runtime route metadata. The current tests
verify definition-derived known-tool and hidden-tool discovery, public
`ToolSpec` exposure, metadata facade behavior, OAuth scope policy, and the
expected capability policy. `tool_specs()` also builds each public spec
through a shared constructor that derives output schema and MCP annotations from
the canonical tool name, then emits public specs in model-visible
`ToolDefinition` order to avoid local string/order drift while the hand-written
spec table is reduced.
`tool_categories()` also reads definition-layer discovery groups and visibility
directly, then only formats the JSON response. Recommended-flow summaries and
structured manifest flows now share a definition-layer declaration. Public
`tool_names()` derives from model-visible `ToolDefinition` order, and contract
tests verify that definition-derived known-name iteration and public `ToolSpec`
order match the canonical definition order. Session guard, ledger classification,
validation-output capture, cross-project session escape, and permission-risk
decisions now also read definition-layer semantic helpers/facades, with
contract tests covering the metadata-derived rules and the remaining explicit
semantic groups. Current-session fallback eligibility now uses the same
definition-layer semantic facade.

Expected checks:

- every public `ToolSpec` has a definition;
- every known `ToolCall` name has a definition, including hidden implemented
  tools such as `run_codex`;
- definition risk/scope/project/session flags drive `ToolMetadata` facade
  behavior and existing helper behavior;
- definition-derived known-tool iteration, public `ToolSpec` order, and public
  `tool_names()` mirror canonical `ToolDefinition` order;
- public `ToolSpec` output schemas and MCP annotations derive from the
  canonical spec name;
- `list_tools` category groups derive from definition-layer discovery groups;
- `list_tools` and `tool_manifest` recommended flows derive from one
  definition-layer declaration and reference only model-visible tools;
- hidden tools remain hidden from model-facing discovery.
- session/permission semantic helpers preserve read-like, write-like,
  shell-like, git-like, change-summary, validation-output, cross-project
  escape, and permission-risk behavior.
- current-session fallback eligibility is explicit and keeps control tools,
  session-creation tools, and required-business-session tools out of implicit
  fallback.

### Phase 3: generate discovery mirrors from definitions

Once Phase 2 has stable parity, use definitions to derive low-risk mirrors such
as registry `ToolSpec`, `tool_manifest` entries, accepted OpenAPI
`callRuntimeTool` names, and MCP annotations. Keep output deterministic and keep
contract tests that compare generated data against expected public surfaces.

Dedicated GPT Action operations may still need hand-written request/response
schemas during this phase, especially where GPT Actions require flattened
top-level fields.

### Phase 4: metadata-driven redaction and policy

Move guard denial classification, session recording summaries, redaction policy,
write/shell/destructive flags, and read-only session enforcement toward
definition-backed policy. This phase has higher security risk than generating
discovery metadata, so it needs focused tests before removing old matches.

### Phase 5: optional macro, derive, or codegen

Consider macro, derive, or codegen support only after the definition model is
behaviorally stable. The goal is to reduce boilerplate without hiding security
classification from reviewers.

Code generation is optional. It should not be used to bypass explicit review of
OAuth scope, visibility, destructive behavior, redaction, or OpenAPI exposure.

## Adding a new tool checklist

### Current checklist

When adding a runtime tool today, expect to update or verify:

- `src/tool_runtime/tool_call.rs`: add the `ToolCall` variant, parser handling,
  `tool_name()`, `session_id()`, and `project()` behavior.
- `src/tool_runtime/mod.rs` or a domain module under `src/tool_runtime/`: add
  runtime handler logic and any `tool_manifest` category/recommended-flow
  changes.
- `src/tool_runtime/dispatch.rs`: wire the `ToolCall` variant to the runtime
  handler while preserving session recording, current-session fallback, guard
  denial, permission, and authorization behavior.
- `src/tool_runtime/surface.rs`: update recommended-flow, bounded `list_tools`,
  and flattened-argument discovery behavior when applicable.
- `src/tool_runtime/tool_definition.rs`: add `ToolMetadata` risk, OAuth scope,
  project requirement, path hint, read-only/destructive/shell-like
  classification, manifest category, visibility, and agent capability.
- `src/tool_runtime/tool_catalog.rs`: add or update discovery group and
  recommended-flow placement when the tool should appear in those model-facing
  summaries.
- `src/tool_runtime/metadata.rs`: update only for non-runtime compatibility
  route metadata such as legacy dedicated HTTP routes.
- `src/tool_runtime/registry/input_schemas.rs`: add or extend input schema
  helpers.
- `src/tool_runtime/registry/output_schemas.rs`: add output schema if the tool
  is discoverable or needs structured output documentation.
- `src/tool_runtime/registry/annotations.rs`: ensure MCP annotations match
  metadata.
- `src/tool_runtime/registry/tool_specs.rs`: add or intentionally hide the
  `ToolSpec`; keep `run_codex`-style hidden behavior explicit.
- `src/auth/scopes.rs`: verify OAuth runtime tool policy still resolves from
  the metadata facade and fails closed for unknown tools.
- `src/openapi.rs`: update dedicated GPT Action operations or
  `ToolCallRequest` accepted-name/flattened-field descriptions when applicable.
- `src/mcp.rs`: verify `tools/list` and `tools/call` behavior needs no protocol
  special case.
- Tests under `src/tool_runtime/tests/`, `src/auth/scopes.rs`, `src/openapi.rs`,
  and MCP/openapi/metadata test lanes: add consistency coverage for name,
  metadata, schema, visibility, OAuth scope, session behavior, and hidden-tool
  behavior.

Reviewers should ask these questions for every new tool:

- Is it model-facing, hidden, or internal-only?
- Is it read-only, write-like, shell-like, destructive, or account-managing?
- Which OAuth scope is required for OAuth2 tokens?
- Does it require an explicit project? Can it use the current-session fallback?
- Is `session_id` recorder metadata, tool business input, both, or neither?
- Does it need agent ownership, shell capability, patch capability, or local-only
  execution?
- What should be redacted from inputs, outputs, errors, and session summaries?
- Does GPT Actions need a dedicated operation or flattened
  `callRuntimeTool` fields?

### Future registry-driven checklist

After the definition model drives low-risk mirrors, adding a tool should move
toward:

- add the `ToolCall` variant and parser;
- implement the runtime handler;
- add one `ToolDefinition` with name, visibility, category, risk, OAuth scope,
  schemas, project/session behavior, agent capability, guard flags, redaction
  policy, and OpenAPI/MCP exposure;
- add focused behavior tests for the handler and any security-sensitive policy;
- rely on registry consistency tests to verify generated `ToolSpec`,
  `tool_manifest`, OpenAPI allowed names, MCP annotations, and OAuth mapping.

The future checklist intentionally keeps execution and security review explicit.
It reduces duplicated declarations; it does not remove policy decisions.

## Required validation matrix

Use focused lanes first, then broaden when core paths are touched.

| Change area | Required validation |
| --- | --- |
| Metadata, risk, visibility, category, or `tool_manifest` | `cargo test --bin webcodex metadata -- --nocapture` |
| Registry input/output schema or ToolSpec changes | `cargo test --bin webcodex schema -- --nocapture` and relevant tool tests |
| Tool parser, handler, or dispatch changes | relevant `cargo test --bin webcodex <tool-domain> -- --nocapture`; include `dispatch` when parser or kernel paths change |
| MCP exposure or annotations | `cargo test --bin webcodex mcp -- --nocapture` |
| OpenAPI dedicated actions, accepted names, flattened fields, or examples | `cargo test --bin webcodex openapi -- --nocapture` |
| OAuth scope or shared-key OAuth bridge policy | `cargo test --bin webcodex oauth -- --nocapture`, `cargo test --bin webcodex scope -- --nocapture`, and `cargo test --bin webcodex metadata -- --nocapture` |
| Session id behavior, current-session fallback, read-only sessions, or guard denials | `cargo test --bin webcodex session -- --nocapture` and `cargo test --bin webcodex metadata -- --nocapture` |
| Broad core paths, dispatch/kernel behavior, or generated policy replacement | `cargo test --bin webcodex` before merge |

For all code changes, also run:

```bash
cargo fmt --check
cargo check --all-targets
git diff --check
git status --short
```

For docs-only updates to this architecture note, `git diff --check` and
`git status --short --branch` are sufficient unless the task explicitly requests
the Rust checks.
