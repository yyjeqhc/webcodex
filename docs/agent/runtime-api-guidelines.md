# Runtime API and MCP guidelines

WebCodex v0.5 has one canonical ToolRuntime and no GPT Actions adapter, feature,
frozen tool-name snapshots, or generated Action OpenAPI document. Model clients
use [MCP](../MCP.md); operational clients use the authenticated Runtime API.

## Canonical contract and dispatch

`ToolDefinition` owns tool identity, category, model visibility, schemas, effects,
authority, execution semantics, and optional Direct presentation policy. Keep
[tool-contract-guidelines.md](tool-contract-guidelines.md) authoritative for names
and sparse model results. An intentional tool rename replaces the name everywhere;
no old-name aliases or alternate registry remain.

MCP offers dedicated descriptors plus the admitted `call_runtime_tool` gateway.
Its closed envelope is `{tool, arguments}`. Runtime HTTP uses `/api/tools/call`
with `{tool, params, recording_session_id?}`. Recorder metadata is not a business
Session selector. Both adapters enter the canonical kernel; neither may bypass
OAuth/PAT scopes, Project authority, permission, Runner capability, path policy,
Session fences, destructive confirmation or uncertain-effect handling.

Hidden/App-only tools remain independently capability-admitted. A gateway cannot
turn presentation metadata into authority or call an unadmitted hidden tool.
Host schema refresh and workflow guidance refresh remain distinct operations.

## Maintained operational routes

`/healthz` is a public deployment/readiness probe. `/api/runtime/status` is an
authenticated operational API. `/api/projects/resolve-or-register` is the narrow
operator workflow for exact-path convergence; ordinary project lifecycle uses
canonical tools. Account, enrollment, token-management and Runner-transport routes
keep their existing authentication boundaries and are not model tools.

The retired `/openapi.json`, `/api/actions/{tool_name}`, and Action-only
`/api/artifacts/import` routes are not mounted and have no route metadata admission.
Do not replace them with dummy documents, redirects, or compatibility switches.
Removing those entrypoints does not remove ActionAudit history, credential kinds,
MCP OAuth, generic Runtime HTTP or Runner wire protocols.

## File provenance

Attachment import requires adapter-derived trusted MCP host-file provenance.
Business JSON cannot assert trust. The supported paths remain a configured trusted
MCP host and the authenticated OpenAI host-file path with bounded HTTPS download,
DNS/IP validation and pinned resolution. Raw API calls cannot impersonate a Host.
Keep streaming size bounds, content validation, partial failure, cleanup and
source/destination authorization tests. Test-only download fixtures never mount
an alternate public import adapter.

## Tests and review

Test canonical parser/catalog/schema consistency, generated follow-ups, Direct
and Gateway dispatch, App-only admission, OAuth/Project/permission boundaries,
file-provenance rejection and authenticated operational routes. Verify retired
routes and tool names fail closed in every build configuration. Test description
and serialized schema budgets against the maintained MCP surface, not a retired
300-character/30-operation importer limit.
