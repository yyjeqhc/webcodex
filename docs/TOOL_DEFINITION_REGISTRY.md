# ToolDefinition registry migration plan

This document is a developer architecture note for moving WebCodex runtime tool
declarations toward a registry-driven `ToolDefinition` model over several
phases. It documents the intended direction only. It does not claim that schema,
metadata, OpenAPI, MCP, OAuth, or policy generation is implemented today.

## Current state

The runtime registry has already been mechanically split into smaller modules:

- `src/tool_runtime/registry/mod.rs`
- `src/tool_runtime/registry/input_schemas.rs`
- `src/tool_runtime/registry/output_schemas.rs`
- `src/tool_runtime/registry/annotations.rs`
- `src/tool_runtime/registry/tool_specs.rs`

That split reduces file size and localizes schema/spec helpers, but it does not
make one declaration the source of truth. Runtime tool facts are still maintained
across multiple hand-written structures and kept aligned by contract tests.

## Current problems

- Tool names are repeated in `ToolCall`, `KNOWN_TOOL_NAMES`, `tool_name()`,
  parser paths, hidden-tool filters, OpenAPI descriptions, MCP discovery,
  `tool_manifest`, and tests.
- Input and output schemas are still hand-written separately from the execution
  IR and from GPT Action flattened fields.
- `ToolMetadata`, registry `ToolSpec`, OAuth runtime scope policy, OpenAPI
  accepted names, MCP annotations, and `tool_manifest` visibility/category data
  can drift when a new tool is added or an existing tool changes.
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
- `ToolMetadata` remains the risk, scope, and policy foundation until a
  `ToolDefinition` mirror is complete and tested.
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

Introduce a `ToolDefinition` mirror that records the same facts currently spread
across `ToolMetadata`, tool specs, tool manifest categories, visibility filters,
and policy matches. At this stage it should validate consistency with existing
hand-written structures rather than drive behavior.

Expected checks:

- every public `ToolSpec` has a definition;
- every known `ToolCall` name has a definition, including hidden implemented
  tools such as `run_codex`;
- definition risk/scope/project/session flags match `ToolMetadata` and existing
  helper behavior;
- hidden tools remain hidden from model-facing discovery.

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

- `src/tool_runtime/types.rs`: add the `ToolCall` variant, parser handling,
  `KNOWN_TOOL_NAMES`, `tool_name()`, `session_id()`, and `project()` behavior.
- `src/tool_runtime/mod.rs` or a domain module under `src/tool_runtime/`: add
  runtime handler logic and any `tool_manifest` category/recommended-flow
  changes.
- `src/tool_runtime/metadata.rs`: add `ToolMetadata` risk, OAuth scope, project
  requirement, path hint, read-only/destructive/shell-like classification.
- `src/tool_runtime/registry/input_schemas.rs`: add or extend input schema
  helpers.
- `src/tool_runtime/registry/output_schemas.rs`: add output schema if the tool
  is discoverable or needs structured output documentation.
- `src/tool_runtime/registry/annotations.rs`: ensure MCP annotations match
  metadata.
- `src/tool_runtime/registry/tool_specs.rs`: add or intentionally hide the
  `ToolSpec`; keep `run_codex`-style hidden behavior explicit.
- `src/auth/scopes.rs`: verify OAuth runtime tool policy still resolves from
  metadata and fails closed for unknown tools.
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
