# OpenAPI / GPT Action Guidelines

Product and integration rules for the default-off legacy GPT Actions compatibility surface.

Related: [`GPT_ACTIONS.md`](../GPT_ACTIONS.md), [`MCP.md`](../MCP.md), and [`tool-contract-guidelines.md`](tool-contract-guidelines.md).

## 1. Frozen compatibility ownership

`ToolDefinition` remains the source of truth for runtime tool identity, canonical `ToolSpec`, schemas, semantics, authority, risk, and execution. GPT Actions is no longer a maintained projection of the current Adaptive Runtime surface.

The legacy adapter owns only two explicit frozen name snapshots:

```text
LEGACY_GPT_ACTION_DIRECT_TOOL_NAMES
LEGACY_GPT_ACTION_SUPPORTED_TOOL_NAMES
```

Those snapshots describe compatibility, not authority. Every admitted call still resolves its canonical `ToolDefinition` and executes through ToolRuntime. New Adaptive Runtime, Host, MCP, Plugin, or Code Mode tools must **not** enter GPT Actions automatically.

Tests must lock the frozen snapshots. Changing `adaptive_runtime_direct(..., rank, reason)`, adding a model-visible tool, or changing Host routing must not require a GPT Actions change. Update a frozen snapshot only for a deliberate compatibility fix to an existing legacy deployment.

## 2. Action-specific state is legacy presentation/transport only

Existing Action-specific declarations may remain for compatibility:

- optional compact Action descriptions for frozen direct operations;
- `Unsupported` for concrete protocol incompatibility;
- `GatewayOnly` where retained definitions still document historical Action routing.

Do not add new Action-specific metadata for newly developed tools. Do not move maintained Adaptive Runtime tools merely to satisfy the retiring Action operation budget. Protocol-only tools such as MCP App presentation, MCP ResourceLink export, or Host-bound operations stay outside the frozen snapshot.

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

The frozen legacy GPT Actions document must remain below the host's 30-operation limit. This is a constraint on the frozen adapter only, not on Adaptive Runtime Direct.

Do not silently truncate operations and do not reshape the maintained Host/MCP surface to make the legacy budget fit. The separate legacy CI must fail if the frozen snapshot exceeds the limit. The Custom GPT importer also rejects OpenAPI schemas at 1 MB, so legacy CI keeps compact and pretty-printed JSON below the internal 800,000-byte budget.

## 6. HTTP adapter and authority

A Server built with `legacy-gpt-actions` exposes one shared authenticated adapter; default builds do not mount it:

```text
POST /api/actions/<canonical_tool_name>
```

OpenAPI enumerates the concrete direct paths plus `/api/actions/call_runtime_tool`; the HTTP router may use one dynamic path handler internally.

Runtime path admission and OpenAPI exposure must consume the same frozen snapshot. A manually constructed `/api/actions/internal_tool` request must not bypass frozen membership, canonical definition checks, or ToolRuntime authority.

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

`/healthz` is the public deployment/readiness probe and must not depend on `legacy-gpt-actions`. `/api/runtime/status` remains a stable authenticated operational/status API for real CLI and diagnostics consumers. `/api/projects/resolve-or-register` remains a hidden internal operator workflow endpoint because it provides atomic exact-path convergence that is intentionally absent from the model-visible tool registry. Ordinary Project lifecycle and Runner-config execution use canonical `/api/tools/call`. These retained compatibility/operator routes are not generic GPT Action operations and stay `Hidden` from `/openapi.json`. Route metadata describes HTTP security/surface facts; it no longer owns a generic `PublicAction` operation registry.

## 10. Project-scoped runtime boundary

Project-scoped `share`/`run` authentication does not create another registry. When the binary was built without `legacy-gpt-actions`, no GPT Actions surface is mounted. When the feature is present, the same frozen compatibility snapshot is exposed and ProjectGrant/ToolRuntime policy still supplies authority.

## 11. Tests that matter

Legacy compatibility CI should keep focused invariants for:

- direct operation names equal `LEGACY_GPT_ACTION_DIRECT_TOOL_NAMES`;
- gateway admission is limited to `LEGACY_GPT_ACTION_SUPPORTED_TOOL_NAMES`;
- adding an Adaptive Direct or Code Mode tool does not expand either snapshot;
- unsupported protocol-only tools are neither direct nor gateway-callable;
- operation IDs remain canonical snake_case and old camelCase IDs stay absent;
- generated operation count is < 30 with no truncation;
- compact and pretty-printed legacy OpenAPI JSON stay below the 800,000-byte budget;
- direct request schemas retain canonical ToolSpec input semantics except declared host overlays;
- every generated OpenAPI `description` is <= 300 characters;
- gateway body remains exactly `{tool, arguments}`;
- unknown/non-frozen targets fail closed;
- representative Action calls still enter the same kernel and preserve scope, Project authority, permission, and file-provenance behavior.

Ordinary CI should protect canonical ToolRuntime/MCP/Host behavior without enabling `legacy-gpt-actions`. Tests for the retiring adapter belong in the separate legacy lane so unrelated tool development does not inherit its constraints.

## 12. Review checklist

Before landing a deliberate legacy GPT Actions change:

- verify the change is compatibility-driven rather than required by normal tool development;
- inspect the frozen direct and supported snapshots;
- scan the generated OpenAPI document for description and size limits;
- verify no management/internal route leaked into the legacy schema;
- verify `/healthz` remains available in default builds and independent of the legacy Action feature;
- verify the adapter still delegates authority and execution to ToolRuntime;
- run the feature-enabled legacy test path.

For ordinary runtime/Host/MCP changes, no GPT Actions review is required unless the code intentionally touches the frozen adapter.
