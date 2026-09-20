# OpenAPI / GPT Action Guidelines

Product and integration rules for the generic GPT Actions compatibility surface.

Related: [`GPT_ACTIONS.md`](../GPT_ACTIONS.md), [`MCP.md`](../MCP.md), and [`tool-contract-guidelines.md`](tool-contract-guidelines.md).

## 1. Canonical ownership

`ToolDefinition` is the single source of truth for runtime tool identity, canonical `ToolSpec`, input/output schemas, semantic contract, scope/authority policy, risk/approval metadata, and Adaptive Runtime direct rank.

Generic GPT Actions **must not** maintain a second tool registry, operation vocabulary, rank table, permission table, Project-authority table, Runner-capability table, or retry/execution policy.

For a generic runtime Server:

```text
Adaptive Runtime Direct
  - ToolDefinition.gpt_action_exposure in {Unsupported, GatewayOnly}
  = GPT Action direct operations

Adaptive Runtime model-visible long tail + definition-owned GatewayOnly
  + GPT-Action-supported
  = call_runtime_tool targets
```

Tests must lock this relationship, not a hard-coded current direct-tool list. Changing `adaptive_runtime_direct(..., rank)` should automatically change ordinary GPT Action direct exposure unless the same definition declares an explicit exposure exception.

## 2. Action-specific state is presentation/transport only

The only normal GPT Action-specific declarations are:

- an optional short presentation description when the canonical description exceeds the Action importer limit;
- an explicit `Unsupported` exposure exception for a concrete protocol incompatibility;
- an existing definition-owned `GatewayOnly` policy to preserve the direct-operation budget while retaining the same canonical gateway-callable tool. For example, `stop_job` is MCP/Adaptive direct but GPT Actions gateway-only, with unchanged effect, approval, authority, parser, and handler. All three experimental Code Mode entrypoints use the same gateway-only Actions policy; their ordinary Adaptive directness and nested allowlists are unchanged. Gateway target enums must include definition-owned GatewayOnly entries, not just Adaptive long-tail routes.

Do not add `gpt_action_rank` or a name-based exposure allowlist. Do not exclude a tool merely because its schema is complex, its canonical description is long, or it is used infrequently.

Protocol-only tools such as MCP App presentation, MCP ResourceLink export, or operations whose useful semantics depend on a separately authorized MCP Host binding may be marked unsupported. The reason must be protocol capability, not model preference.

## 3. Canonical operation names and schemas

Direct operation IDs are the canonical snake_case runtime tool names. Do not introduce camelCase aliases or alternate operation spellings.

Each direct Action request body starts from the canonical `ToolSpec.input_schema`. A GPT Action projector may alter only presentation/host-transport details that the Action host actually rewrites. It must not change canonical business semantics such as type, required fields, enum values, oneOf/anyOf shape, numeric/string bounds, patterns, or `additionalProperties` policy.

The one generic escape hatch is `call_runtime_tool`:

```json
{
  "tool": "exact_runtime_tool_name",
  "arguments": {}
}
```

The gateway schema is closed and contains only `tool` and `arguments`. No `params` alias and no flattened top-level business-argument union are part of the GPT Action contract. Direct targets should use their direct Action operation; ModelHidden, unknown, recursive, and GPT-Action-unsupported targets fail closed.

The generic REST `/api/tools/call` endpoint uses one explicit envelope: `tool`, optional `params`, and optional `recording_session_id` request metadata. Tool arguments belong under `params`; this HTTP envelope remains separate from model-facing OpenAPI authority and must not leak into `/openapi.json` or `tool_manifest`.

## 4. Description limits

GPT Actions has a hard **300-character** limit for operation/tool descriptions. Keep this separate from the canonical/MCP model-description budget.

Rules:

- canonical description <= 300 characters: inherit it unchanged;
- canonical description > 300 characters: provide an explicit compact Action description on the same `ToolDefinition`;
- never mechanically truncate an operation/tool description;
- generated schema/property descriptions may use the shared sentence-aware bounded presentation projector;
- schema description projection must change only description text, never schema semantics.

The generated generic OpenAPI document must be recursively tested so **every** `description` field is <= 300 characters.

Compact Action operation copy should prioritize: what the tool does, when to choose it, and the key continuation/recovery choice. Do not repeat Bearer-auth, scope, Runner-authority, or general safety boilerplate in every operation description.

## 5. Operation budget

Generic GPT Actions must stay below the host's 30-operation limit. The generated surface is Adaptive Direct minus definition-owned `Unsupported` and `GatewayOnly` exceptions plus `call_runtime_tool`.

Do not silently truncate operations. CI must fail if the derived projection reaches the limit so the developer explicitly chooses a definition-owned GatewayOnly policy, moves a tool out of Adaptive Direct, or identifies a real protocol incompatibility. Do not raise the budget or add a second name-based registry.

The Custom GPT importer also rejects OpenAPI schemas at 1 MB. Keep the generic Action document comfortably below that host ceiling: CI checks both compact and pretty-printed JSON against an internal 800,000-byte budget. Direct request schemas remain canonical, but response schemas intentionally expose only the real `ToolResult { success, output, error? }` envelope with generic `output`; complete canonical output schemas stay in `ToolSpec`/MCP rather than being duplicated into every Action response.

## 6. HTTP adapter and authority

The generic runtime exposes one shared authenticated adapter:

```text
POST /api/actions/<canonical_tool_name>
```

OpenAPI enumerates the concrete direct paths plus `/api/actions/call_runtime_tool`; the HTTP router may use one dynamic path handler internally.

Runtime path admission and OpenAPI exposure must consume the same canonical derived surface. A manually constructed `/api/actions/internal_tool` request must not bypass model visibility or GPT Action exposure policy.

The adapter owns only:

- HTTP/OpenAPI decoding;
- canonical tool identity and direct/gateway admission;
- host-derived protocol provenance;
- response framing/presentation.

Real execution must enter `ToolRuntime::call_tool_with_context(...)` (or the same canonical kernel path). OAuth/PAT scopes, Project authority, permission/approval gates, Runner capability checks, path policy, Session fences, destructive rules, idempotency, retry semantics, and effects remain kernel-owned.

If HTTP middleware needs body/path-aware scope admission, derive it from canonical tool metadata. Do not create a per-Action scope table. The kernel remains the final fail-closed authority.

## 7. Consequential hint

`x-openai-isConsequential` is a ChatGPT host UX hint, not permission authority.

Derive it from canonical semantic/approval metadata. A simple projection is:

- `ToolApprovalPolicy::None` -> `false`;
- any standard, inherited, or unknown approval requirement -> `true`.

`call_runtime_tool` itself is consequential because it may dispatch a mutating long-tail target. Never maintain a tool-name consequential allowlist.

## 8. Host file provenance

`import_conversation_files_to_project` has two distinct legitimate host provenance paths:

- GPT Action/OpenAI Action file-reference rewrite;
- trusted MCP host-file rewrite: normally an exact configured OAuth client, or the explicit loopback-only user-API-token exception for OpenAI Secure Tunnel.

Authorization still comes from canonical ToolRuntime policy. Provenance is private adapter-derived metadata and selects the correct bounded download policy.

The public ToolSpec must not contain a provenance field. Model JSON must not be able to assert GPT Action provenance, MCP trust, or convert one mode into the other. Action-specific file-reference field-name adaptation belongs in the Action adapter/projector, not in a duplicate business handler.

## 9. Legacy REST routes

Dedicated REST adapters may remain only when they serve a real current CLI, product, or external REST consumer. Tests by themselves are not compatibility consumers.

`/api/runtime/status` remains a stable operational/status API for real CLI, deployment, readiness, and diagnostics consumers. `/api/projects/resolve-or-register` remains a hidden internal operator workflow endpoint because it provides atomic exact-path convergence that is intentionally absent from the model-visible tool registry. Ordinary Project lifecycle and Runner-config execution use canonical `/api/tools/call`. These retained compatibility/operator routes are not generic GPT Action operations and stay `Hidden` from `/openapi.json`. Route metadata describes HTTP security/surface facts; it no longer owns a generic `PublicAction` operation registry.

## 10. Project-scoped runtime boundary

Project-scoped `share`/`run` authentication does not create another OpenAPI or MCP tool registry. It projects the same canonical Adaptive Runtime and relies on scopes, ProjectGrant Runner visibility, ToolRuntime project resolution, and normal permission policy for authority. Do not introduce a project-share-specific operation vocabulary or compatibility alias layer.

## 11. Tests that matter

At minimum, keep focused invariants for:

- GPT Action direct definitions equal current Adaptive Direct definitions minus explicit `Unsupported` definitions, preserving Adaptive rank order;
- `apply_text_edits` is direct while an ordinary long-tail tool such as `apply_patch` routes through the gateway;
- unsupported protocol-only tools are neither direct nor gateway-callable;
- operation IDs are canonical snake_case names and old camelCase IDs are absent;
- generated operation count is < 30 with no truncation;
- compact and pretty-printed generic OpenAPI JSON remain below the internal 800,000-byte import budget;
- direct request schemas equal canonical ToolSpec input schemas except declared presentation/host overlays;
- every generated OpenAPI `description` is <= 300 characters while canonical/MCP descriptions retain their independent budget;
- canonical descriptions over 300 require an explicit Action presentation override;
- gateway body is exactly `{tool, arguments}`;
- ModelHidden/unknown/unsupported targets fail closed;
- direct Action requests enter the same kernel and match MCP authority outcomes for representative read and mutating/execution tools;
- permission-gate denial remains effective through Action HTTP;
- GPT Action and MCP file-import provenance cannot be asserted by public JSON;

Retire tests that only preserve the old camelCase facade, `PublicAction` registry, giant `ToolCallRequest` flattened schema, or flattened manifest guidance. Tests should protect current authority/schema truth, not dead compatibility architecture.

## 12. Review checklist

Before landing a GPT Actions change:

- inspect generated operation count and names;
- scan the complete generated OpenAPI document for description length;
- verify no legacy REST route leaked into generic model-facing OpenAPI;
- verify no second scope/permission/exposure table was introduced;
- verify all direct operations still use canonical ToolSpec inputs;
- verify gateway admission is canonical and fail-closed;
- verify file provenance remains private adapter metadata;
- run generic OpenAPI, HTTP Action adapter, MCP surface/scope, project-scoped authority, and file-import focused tests.