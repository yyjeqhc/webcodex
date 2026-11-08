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

The MCP adapter owns `WEBCODEX_MCP_TOOL_EXPOSURE`. Optional workflow surfaces
remain independently configurable on Adaptive, including bounded authenticated
OAuth-client overrides. CLI is execution-only even if legacy CLI exposure flags
are enabled. Configuration never changes canonical authority. Discovery,
manifests, suggested calls, resources and dispatch must agree on admission.
See [client configuration](../MCP.md#optional-workflow-tools-per-client).

`X-WebCodex-MCP-Surface: cli` selects canonical execution tools and text projection.
Use one default command entry (`run_shell`), `observe_jobs` for bounded wait/read,
and `stop_job` for cancellation; specialized execution and navigation remain
available by manifest/gateway. Workflow, communication, Skills, Memory and Apps
are excluded on both dispatch routes. `_wc` is absent and rejected. The kernel's
request-local execution-only capability skips optional workflow/collaboration
collection before it happens; it never suppresses execution evidence or audit.
The client reads repository instructions and owns its planning/context. The
optional default Project supplies only omitted required selectors, with normal
reauthorization and source fencing. Render text after canonical audit/Job
correlation so presentation cannot change execution success or retry evidence.
Third-party passthrough retains provider semantics. Adaptive behavior is unchanged.

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

The old `/api/shell/run`, `/api/shell/file`, `/api/shell/job`, and
`/api/shell/jobs/{status,log,stop,list}` execution routes are also unmounted. Use
canonical tools through MCP or `/api/tools/call`; no deprecated endpoint or
forwarding adapter is maintained for these unused internal APIs. This does not
retire `/api/shell/agent/*`, whose registration, polling, result and Job-update
handlers remain part of the Runner transport. Shared Registry primitives, existing
Job recovery/cancellation and negotiated mixed-version capabilities are unchanged.
See [compatibility policy](../compatibility-policy.md#internal-execution-interfaces).

### Admin-only update of an existing Project's `allow_patch`

The narrow `POST /api/admin/projects/update-allow-patch` route changes **only**
an existing Runner Project's `allow_patch` flag. It does not re-register the
Project. This is a privileged admin operation: the standard JSON same-origin
check, bootstrap/admin route policy and protected credential-kind checks apply.
It is not a model-facing tool or an approval bypass.

Request body:

```json
{
  "project": "agent:<runner-client-id>:<project-id>",
  "expected_revision": "sha256:<64 hexadecimal characters>",
  "allow_patch": false,
  "idempotency_key": "<unique-request-key>",
  "confirm": true
}
```

Get the current `revision` and Project identity from the administrator
dashboard or an authorized Project inventory view. The request accepts no
additional fields. The Server requires the separate Runner capability
`project_settings_update`, not just the older `project_lifecycle` capability;
this safely rejects a mixed-version Runner that does not understand the new
wire operation.

The Runner checks the revision and canonical Project-root identity **under
the Project registry write lock**, including for no-op updates. A successful
response returns `operation`, `project`, `outcome`
(`allow_patch_updated` or `already_updated`), `changed`,
`previous_revision`, the new `revision`, the resulting `allow_patch`,
`active_jobs` and `warnings`. The existing redacted admin audit and
idempotency receipt record the operation without storing credentials. A changed project is persisted
atomically without modifying its other TOML keys, comments, registration
identity or other Projects; the Server updates only that permission and revision
in the cached Project view. A stale revision or a changed root is rejected
(HTTP 409); a missing Project is HTTP 404. The changed flag does not authorize
any separate file edit, approval, policy exception or action on existing Jobs.

For rollback, reread the Project's **current** revision and issue a new
confirmed request with the previous boolean value and a fresh idempotency key.
If the operation times out, returns `operation_indeterminate` or reports a
projection reconciliation error, **read the authoritative Project state before
retrying**; persistence may already have completed. Do not use
`register_project(overwrite=true)` for one-field updates.

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
