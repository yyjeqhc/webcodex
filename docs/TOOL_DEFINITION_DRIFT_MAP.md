# ToolDefinition drift map

Round 2 status for ToolDefinition source-of-truth convergence. This map is a
boundary document only: it records the current declaration sources, guard tests,
and next convergence steps without changing runtime behavior.

## Source-of-truth status summary

`ToolDefinition` is now the canonical source for runtime tool names, canonical
order, model-facing visibility, manifest category, runtime metadata and risk,
agent capability, permission/session helper facades, and current-session fallback
eligibility. Public `ToolSpec` order follows model-visible `ToolDefinition`
order, and public specs derive output schemas and MCP annotations from the spec
name.

The migration is not complete. The `ToolCall` enum, serde parser variants,
`tool_name()` mirror, dispatch routing, ToolSpec descriptions, input schemas,
dedicated OpenAPI operations, and some discovery/catalog placement remain
hand-written and contract-tested. Metadata fallback remains only for safe
unknown-name metadata and the legacy non-runtime `delete_files` route metadata.

Current inventory classification:

- definition-backed: known names, canonical order, visibility, category,
  runtime metadata/risk, MCP annotations, permission/session facades,
  current-session fallback eligibility, agent capability lookup.
- partially definition-backed: `ToolCall::from_tool_name()` acceptance,
  model-facing `ToolSpec` rows/order, compact `tool_manifest` entries, generic
  `callRuntimeTool` accepted-name text, flattened GPT Action argument discovery.
- hand-written but contract-tested: `tool_name()`, input schemas, output
  schemas, OpenAPI operation exposure, MCP `tools/list` visibility, and curated
  discovery/recommended-flow placement.
- hand-written and weakly guarded: dispatch routing and all ToolSpec
  descriptions beyond targeted description checks.
- legacy fallback only: safe unknown metadata and explicit `delete_files`
  compatibility metadata.

## Current counts

- ToolDefinition count: 67
- model-visible `tools.count`: 66
- output schema coverage: 66/66
- OpenAPI operation count: 25
- default-only output schema gap: 0

## Drift matrix

| Surface / concern | Current source | Backed by ToolDefinition? | Guard test exists? | Risk if drift occurs | Recommended next action |
| --- | --- | --- | --- | --- | --- |
| ToolDefinition known names | `src/tool_runtime/tool_definition.rs` grouped definitions | Yes | Yes: `schema/definitions.rs`, `schema/migration.rs` | Missing or extra runtime name changes discovery, parser acceptance, or policy | Keep as canonical; fail counts/order on every runtime tool change |
| ToolCall parser accepted names | `ToolCall::from_tool_name()` gates on `lookup_tool_definition()`, then serde parses `ToolCall` | Partially | Yes: `schema/migration.rs`, `tests/tool_call.rs` | Definition accepts a name whose enum variant or args do not parse | Round 4+ only: generate or table-drive parser parity after dispatch coverage is stronger |
| `tool_name()` | Manual match in `src/tool_runtime/tool_call.rs` | No | Yes: `schema/definitions.rs`, `schema/migration.rs` | Parsed calls record, dispatch, audit, or policy under wrong names | Keep mirror tests; do not generate in Round 1 |
| dispatch handlers | Manual match in `src/tool_runtime/dispatch.rs` plus domain dispatchers | No | Partial domain coverage; no full generated dispatch mirror | New tool can parse but route to wrong handler, miss guards, or miss auth behavior | Add non-invasive dispatch inventory tests before any generation |
| ToolSpec descriptions | Hand-written per registry module | No | Partial: `schema/descriptions.rs` spot checks | Model guidance can become stale or unsafe | Round 2 can add concise required-description invariants for high-risk tools |
| input schemas | Hand-written `src/tool_runtime/registry/input_schemas/*` | No | Yes: `schema/specs.rs`, `schema/flattened_args.rs`, domain schema tests | Parser, MCP, and GPT Action flattened args drift apart | Round 3: add input-schema and flattened-arg drift matrix tests |
| output schemas | Hand-written `src/tool_runtime/registry/output_schemas/*`, attached by spec name | Partially | Yes: `schema/outputs.rs`, `schema/specs.rs` | Discovery returns default-only or misleading output contracts | Keep 66/66 and default-gap zero checks |
| MCP annotations | `registry/annotations.rs` from `runtime_tool_metadata()` | Yes for runtime names | Yes: `schema/annotations.rs`, `schema/specs.rs`, `mcp` tests | MCP clients see wrong read-only/destructive/open-world hints | Keep definition-backed generation and MCP parity tests |
| OpenAPI operation exposure | Hand-written `src/openapi.rs` paths and schemas | Partially for generic accepted names | Yes: `src/openapi.rs` tests, `schema/migration.rs` | Dedicated Actions count or visibility changes unexpectedly | Keep operation count 25; do not generate operations yet |
| GPT Action flattened args | Derived from ToolSpec input schemas plus definition extra args, inserted in `src/openapi.rs` | Partially | Yes: `schema/flattened_args.rs`, `src/openapi.rs` tests | `callRuntimeTool` loses required top-level fields or loosens schema | Round 3: assert every accepted flattened arg has a declared source |
| tool_manifest categories | Compact manifest uses `runtime_tool_category()`; discovery groups live in `tool_catalog.rs` | Partially | Yes: `schema/discovery.rs` checks ToolDefinition category membership, compact manifest vs bounded `list_tools` parity, category filters, and hidden exclusions | Tools appear in wrong category, multiple categories, hidden categories, or no category | Round 2 complete: keep parity/filter guards while categories remain hand-written |
| list_tools discovery groups | `TOOL_DISCOVERY_GROUPS` in `tool_catalog.rs`, rendered by `registered_tool_categories()` for full discovery groups | Partially | Yes: `schema/discovery.rs` checks group keys, known/model-visible members, hidden exclusions, explicit cross-list allowlist, and group/category exception allowlist | Discovery categories drift away from known model-visible tools or expose hidden/runtime-only names | Keep workflow cross-listing explicit until group placement is generated or replaced |
| recommended flows | `tool_catalog.rs` re-exported through definition layer | Partially | Yes: `schema/discovery.rs` checks list summaries, compact manifest name/purpose/tool order, known/model-visible tool refs, category presence, omission when disabled, and hidden exclusions | Flow references hidden/unknown tools or stale recommended paths | Round 2 complete: keep structured manifest parity guard |
| metadata fallback | `metadata.rs` for unknown and `delete_files`; policy facade in `tool_policy.rs` | Legacy fallback only | Yes: `schema/migration.rs`, `tests/metadata.rs` | New runtime metadata bypasses ToolDefinition | Round 4: concentrate fallback boundary and prevent new runtime fallback names |
| permission risk labels | `ToolDefinition` policy plus metadata-derived fallback for non-runtime names | Yes for runtime names | Yes: `schema/policy.rs`, `schema/migration.rs` | Write/shell/destructive tools receive wrong permission prompt or guard treatment | Keep facade tests; clean fallback only after route metadata is separated |
| session/current-session behavior | `ToolDefinition` policy facades plus `ToolCall` accessors and dispatch/session logic | Partially | Yes: `schema/policy.rs`, session tests | Current-session fallback or explicit `session_id` behavior changes silently | Add accessor-policy drift tests before moving session behavior into definitions |
| agent capability dispatch | `ToolDefinition.agent_capability` and `required_agent_capability()` parity | Yes for declared capability | Yes: `schema/definitions.rs`, `schema/policy.rs` | Agent-backed calls bypass or over-require capabilities | Keep strict no-fallback capability lookup |
| delete_files compatibility metadata | `metadata.rs` legacy route metadata only | Legacy fallback only | Yes: `schema/definitions.rs`, `schema/migration.rs`, metadata tests | Legacy route becomes a runtime tool or public model-facing name | Keep explicit allowance until route metadata is removed or separated |
| run_codex hidden behavior | Hidden `ToolDefinition`, disabled message, parser-known but model-hidden | Partially | Yes: `schema/definitions.rs`, `schema/migration.rs`, OpenAPI and MCP tests | Hidden disabled tool appears in model-facing specs or Action names | Keep hidden-only contract; do not expose without separate design |

## Explicit non-goals

- Do not generate the `ToolCall` enum yet.
- Do not generate input schemas yet.
- Do not remove metadata fallback yet.
- Do not remove `delete_files` compatibility metadata yet.
- Do not change runtime handlers, dispatch behavior, OpenAPI operation exposure,
  MCP names, auth policy, guard behavior, or session semantics as part of this
  drift-guard work.

## Round 2 discovery guard status

Round 2 added tests only; it did not change runtime, server, agent, auth,
permission, guard, session, OpenAPI, MCP, ToolCall parser, or handler behavior.

Current model-facing discovery shapes are intentionally split:

- compact `tool_manifest.categories` and bounded `list_tools.categories` are
  ToolDefinition category maps.
- full discovery groups from `registered_tool_categories()` are curated workflow
  groups declared in `TOOL_DISCOVERY_GROUPS`.

The workflow groups intentionally cross-list some tools, for example shared
review/git/checkpoint tools and the `workflow` category tools
`start_coding_task` and `finish_coding_task`. The exact cross-listed tools and
the group/category exception map are explicit allowlists in
`src/tool_runtime/tests/schema/discovery.rs`, so future drift must be reviewed
rather than silently absorbed.

## Recommended next rounds

### Round 3: input schema / flattened args drift tests

Add guards that compare ToolSpec input-schema properties, required fields,
accepted flattened args, and `ToolCallRequest.properties`. The goal is to prove
flattened GPT Action compatibility remains explicit without generating input
schemas.

### Round 4: metadata fallback / policy boundary cleanup

Shrink the named metadata fallback boundary after route metadata is separated or
retired. Keep `delete_files` as the only explicit non-runtime compatibility name
until then, and keep unknown-name metadata safe and non-callable.
