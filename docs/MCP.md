# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex exposes an MCP endpoint so ChatGPT, Claude, and other MCP clients can work with repositories through the Runner that owns them. Ordinary users only need to choose between **full use** and a **temporary trial**; protocol surfaces, scopes, and credential taxonomy are reference material, not onboarding prerequisites.

## ChatGPT: recommended full setup

For everyday use, run a regular Server + Runner. Follow the [Full Setup guide](PERSONAL_SETUP.md) for one-time login and project registration, and use `--print-mcp-config` during that same `webcodex login` to obtain regular HTTPS MCP connection values. Public HTTPS, Cloudflare Tunnel, and OpenAI Secure MCP Tunnel are reachability choices; they do not change the capabilities of this full development path.

If you only want to try one repository temporarily, use the `share` path below.

## ChatGPT: temporary `share`

Explicit `share` is supported on Linux, macOS, and Windows and owns a temporary single-project environment for that foreground run. Windows x64 can use the managed default Cloudflare Quick Tunnel; Windows ARM64 needs a trusted explicit/PATH `cloudflared` because the pinned Cloudflare release publishes no official ARM64 artifact. Managed OpenAI `tunnel-client` supports both Windows x64 and arm64.

For the default temporary public path, WebCodex reuses an explicit/PATH `cloudflared` or downloads its pinned verified managed copy automatically, then run:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

When the CLI says **WebCodex ready**:

1. In ChatGPT Developer Mode, create a custom app using MCP.
2. Paste the printed **MCP URL**.
3. Choose **Access token / API key** (Bearer token) for the default share.
4. Paste the printed temporary **Credential**.
5. Run **Scan Tools**.
6. Try: `Inspect this repository and summarize its structure. Do not make changes.`

The command performs project setup itself. Hosted ChatGPT cannot reach a
loopback-only `webcodex run`, so `setup`, `doctor`, and `run` are not required
steps before `share`. ChatGPT UI labels can vary by rollout; use the CLI output
as the source of truth for URL and authentication. Developer Mode, custom MCP
apps, and write/modify actions are controlled independently by the ChatGPT plan,
workspace, and admin settings; those client-side permissions are not widened by
WebCodex scopes.

## Claude and other MCP clients

Use the same printed `/mcp` URL and authentication values. In Claude, add a
custom connector and paste the MCP URL. Other MCP clients should be configured
with the same endpoint and the authentication mechanism reported by the CLI.
When a client cannot set a Bearer header, `webcodex share --auth query-token`
provides an explicit temporary-share fallback: paste the printed sensitive
`/mcp?token=...` URL and choose No authentication. The query accepts only the
current share Project Credential; it is not a general PAT/OAuth/shared-key query
auth mechanism. Treat the full URL as a secret because URL queries may be logged.
For local-only clients, `webcodex share --tunnel none` exposes the loopback MCP
endpoint without `cloudflared`.

For an OpenAI-only private transport, create/select a Secure MCP Tunnel, export
`CONTROL_PLANE_TUNNEL_ID` plus a Restricted `CONTROL_PLANE_API_KEY` with Tunnels
Read + Use, and run `webcodex share --tunnel openai`. ChatGPT uses Connection:
Tunnel + No authentication; the temporary WebCodex Bearer stays local and is
injected by the pinned verified OpenAI `tunnel-client`.

For a long-lived **loopback-only** Server reached through OpenAI Secure Tunnel,
ChatGPT host-file rewrites authenticated by the local user API token can be trusted by
setting `WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true`. Starting with
v0.4.2, WebCodex Desktop writes this value by default for the local loopback Server it
owns; an existing explicit value is never overwritten. The exception works only when
`WEBCODEX_ADDR` resolves to loopback and the authenticated credential is a normal user
API token. Independent/network-accessible Servers remain off by default and must not
use this as a substitute for OAuth.

For a regular independent Windows Server + Runner reached through OpenAI Tunnel, or to troubleshoot a case where local `/readyz` is healthy but ChatGPT Connector creation still fails, see the [Windows + OpenAI Secure MCP Tunnel deep dive](WINDOWS_OPENAI_TUNNEL.md). It is advanced setup/troubleshooting material, not required reading for a first-time user.

## Result cards

On Stateless MCP 2026 requests that advertise MCP Apps HTML support, clients can display a
small set of read-only milestone cards for `list_jobs`, `validation_summary`, and
`git_review_summary`. The Job card shows only active or attention-requiring Jobs;
routine successful terminal Jobs stay out of the foreground. Aggregate validation
and committed-range review cards remain bounded and do not embed raw logs, diffs,
or hunks.

High-frequency calls such as `observe_jobs`, `cargo_check`, `cargo_test`, `go_test`,
and `show_changes` intentionally keep the Host's native tool presentation instead
of creating an extra custom App card for every call. Cards do not poll, retry, or
invoke tools; the canonical tool result remains available independently.
`WEBCODEX_MCP_APPS_ENABLED=false` disables App metadata and resources without
disabling the underlying tools.

The current Result App is intentionally static. September 2026 Host experiments proved that a separately designed MCP App controller can poll server-owned state and request later ChatGPT model turns, including a bounded foreground autonomous multi-turn loop, but background-tab model-turn scheduling is not an immediate guarantee. Those findings and the production design constraints are recorded in [`agent/mcp-app-continuation-experiments.md`](agent/mcp-app-continuation-experiments.md); they do not change the current Result App contract.

## Existing Server

For an existing hosted Server intentionally configured for shared-key clients,
use the long-lived shared-key path with the credential supplied by its operator:

```bash
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` starts/reuses the local Runner and prints the MCP URL and credential
source after the connection is verified. This is separate from fresh self-hosted
Docker enrollment: keep the Docker Server bootstrap administrator token on the
Server, create a short-lived pairing code there, and use `webcodex login` on the
repository machine. Self-hosting is documented in [Deployment](DEPLOYMENT.md).

Bearer/shared-key authentication is the simplest path. When a client requires
OAuth, use `share --auth oauth` or `connect --auth oauth` with that client's exact
callback URL and follow the CLI output. Managed-user OAuth remains a separate
advanced identity flow.

## Advanced / reference

### Adaptive Runtime routing

There is one model-facing MCP runtime contract: **Adaptive Runtime**. Canonical `ToolDefinition` rank decides the direct tools; ordinary model-visible long-tail tools are invoked through `call_runtime_tool`; server-owned protocol capabilities and MCP App admission may add hidden extensions for the relevant protocol request. There is no startup model-surface selector. `tool_manifest(tool_name=...)` is discovery only: it never dynamically registers a new Host tool. Its exact `route.primary` describes the preferred callable, and a normal direct tool also exposes `route.fallback` through `call_runtime_tool` for the case where that direct callable is not present; explicit MCP App presentation tools mark that fallback as blocked while Apps are enabled. Direct versus gateway routing changes presentation only and never bypasses the target tool's authentication, Project authority, permission, Runner capability, Session, or safety checks.

### Tool result framing

Machine-readable MCP tool results are returned in `structuredContent`; `content` is a concise human-readable/protocol-native fallback. Clients that need fields should consume `structuredContent` rather than parse text.

Ordinary clients retain standard MCP `isError` behavior. For requests whose `_meta["io.modelcontextprotocol/clientInfo"].name` is exactly `openai-mcp` (any version), the MCP adapter applies an **OpenAI structured-failure compatibility projection**: WebCodex-owned canonical `ToolResult` failures use `isError=false`, while `structuredContent.success=false` remains authoritative and the complete output/error is preserved. The current OpenAI Host promotes `isError=true` to an exception without exposing `structuredContent`; this projection preserves machine-actionable failure and recovery data and can be removed when that Host behavior changes. JSON-RPC/protocol errors remain errors, and upstream third-party MCP/Plugin passthrough results retain provider semantics.

Some MCP hosts do not expose `structuredContent` to the model. This has been observed with Claude Custom Connector even when WebCodex successfully executes the tool and returns the complete structured result. Operators serving such a host can explicitly set `WEBCODEX_MCP_TEXT_JSON_COMPAT=true`. Ordinary runtime tool results then keep `structuredContent` canonical while also serializing that same JSON value into `content[0].text`. The option is off by default because the duplicate representation increases response/model-context size; protocol-native image/resource framing and the existing App-only compatibility paths remain unchanged.

Recovery fields in a result describe the next safe **explicit** call. They never grant authority and never trigger a hidden retry. In particular, an uncertain outcome must be reconciled before repeating an effect.

### Built-in local MCP gateway

A hosted Server can expose Runner-owned local stdio MCP providers through the same `/mcp` endpoint. Authorized callers use the single `mcp_tool` entry to list, describe, and call configured providers; provider process/instance identities and schema-revision state stay internal.

Configure local providers on the Runner under `[mcp]`. Access requires the explicit `mcp:local` permission; hosted OAuth clients opt in with `webcodex connect ... --oauth-local-mcp`. See [Runner](RUNNER.md#provider-side-gateway-v1-compatibility) for provider compatibility details.

### Managed SSH resource onboarding

The `ssh_resource` tool provides a narrow Runner-local onboarding path for
named SSH resources. `list` observes safe logical names and returns an opaque
exact-Runner/revision binding; `register` and `remove` consume that binding and
change only durable desired state. They never silently retarget a replacement
Runner or replay after an uncertain outcome. Raw SSH targets are not returned
in tool results or normal/full trace bodies. When `restart_required=true`,
restart the Runner and list again before selecting the resource in a Workflow
Session.

This surface requires the optional `ssh:local` permission. It is not part of the
ordinary hosted OAuth baseline; opt in explicitly with
`webcodex connect ... --oauth-local-ssh`. See
[Runner](RUNNER.md#ssh-session-resources-advanced) for static-vs-managed and
PersistentShell details.

### OAuth2

When OAuth is enabled, MCP clients can use the authorization-code flow instead of a static token. Register the client's exact callback URL, keep `offline_access` when the host requests refresh-token support, and follow the connection values produced by `share --auth oauth` or `connect --auth oauth`. Server setup is in [Deployment](DEPLOYMENT.md#oauth2).

For ordinary hosted `connect --auth oauth`, the Runner keeps its hosted credential while the MCP client receives a separate OAuth credential. Add `--oauth-computer-permissions`, `--oauth-local-mcp`, or `--oauth-local-ssh` only when those optional capabilities are needed. Existing clients are not silently widened; a real permission change requires reauthorization.

Project-first `share --auth oauth` remains bound to that temporary share environment. Managed-user OAuth is a separate advanced flow (`connect --auth managed-oauth`). OAuth credentials are never valid on Runner transport.

For the credential and scope model, see [Authentication](AUTH_MODEL.md#oauth2).

### Grok custom connector (OAuth)

Grok supports custom MCP connectors and can complete the OAuth flow required by
the MCP server. For a self-hosted WebCodex Server, first expose
`https://your-domain.example/mcp` over public HTTPS and enable OAuth:

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

For the current Grok web connector flow (verified in August 2026), register this
redirect URI exactly:

```text
https://grok.com/connectors-oauth-exchange-code/
```

If Grok later presents or uses a different callback, register that exact value
instead. Create a dedicated OAuth client; the client secret is returned only
once:

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"Grok MCP","redirect_uris":["https://grok.com/connectors-oauth-exchange-code/"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

In Grok's **Custom Connector** form, use:

| Field | Value |
| --- | --- |
| MCP server URL | `https://your-domain.example/mcp` |
| Client ID | the returned `wc_client_*` value |
| Client Secret | the returned one-time `wc_csec_*` value |
| Authorization Endpoint | `https://your-domain.example/oauth/authorize` |
| Token Endpoint | `https://your-domain.example/oauth/token` |
| Scopes | `runtime:read`, `project:read`, `project:write`, `job:run`, `offline_access` |
| Token Auth Method | `client_secret_post` |

WebCodex advertises PKCE `S256`; Grok can use PKCE together with
`client_secret_post`. Do not select `none (PKCE only)` for a WebCodex OAuth
client that has a client secret. `offline_access` is a protocol-level scope for
refresh tokens and is intentionally not stored in the OAuth client's
`allowed_scopes` permission list. The MCP Protected Resource Metadata omits
`scopes_supported` because pre-registered clients can have different scope
ceilings. General-purpose MCP clients can therefore omit `scope` and let WebCodex
default the authorization request to that client's registered `allowed_scopes`.

When the WebCodex authorization page opens, sign in with a current user PAT
(`wc_pat_*`) for the user whose authority Grok should receive. A Runner token
(`wc_agent_*`) is not a user login token. The resulting OAuth access token is
bound to that user and remains constrained by the registered/requested scopes.

Common setup failures:

- **Save & Connect is disabled:** Grok requires a Client ID before it can start
  the OAuth flow.
- **`invalid token`:** the PAT must authenticate against the same current
  WebCodex Server database. Do not use a Runner token or a stale PAT left from
  an older Server/database.
- **`invalid scope`:** every requested WebCodex permission scope must be in the
  OAuth client's `allowed_scopes`. For normal Grok MCP use, do not request
  `account:manage`; `offline_access` is accepted separately as a protocol scope.
- **redirect mismatch:** the redirect URI must match the registered value
  exactly, including path and trailing slash.

See xAI's [Connector documentation](https://docs.x.ai/grok/connectors) for the
current Grok Custom MCP UI and availability.

## Project-scoped ordinary runtime

`webcodex run` and `webcodex share` bind one configured repository, start a local Server + Runner, and expose the ordinary Adaptive Runtime. The temporary or persistent Project Credential is an authentication/ProjectGrant boundary; it does not select a separate capability surface.

A typical coding flow is:

```text
work_on_project
→ read_files / search_project_texts / semantic navigation as needed
→ apply_text_edits or other canonical edit tools
→ run_process / run_shell / focused validation tools as needed
→ show_changes
→ finish_coding_task
```

`work_on_project` starts or resumes an explicit Workflow Session on an ordinary registered Project. If the user requests isolation, `work_on_project(mode=worktree)` asks the Runner to create its canonical managed worktree and registers that worktree as another ordinary Project. Without that request, local `share`/`run` work directly on the one Project already registered by setup.

Adaptive Runtime may expose common tools directly and long-tail tools through `call_runtime_tool`. Direct versus gateway exposure never changes schema validation, OAuth scope, Project authority, permission policy, Runner capability checks, Session fences, or effects.

The removed ProjectConnector capability names (`task_start`, `files_read`, `edits_apply`, `task_finish`, and related operations) are not compatibility aliases for runtime tools. Use the current ToolRuntime names returned by `tools/list`/`tool_manifest`.

### Long work continues as Jobs

`observe_jobs(summary_only=true)` is an opt-in presentation mode for proven successful
structured validation Jobs. It removes routine passed-test and Cargo progress lines,
while preserving test summaries, unknown text, warnings, validation evidence, lifecycle,
and truncation/reset/retention flags. Failures, zero/unproven tests, compile-only test
runs, incomplete validation evidence, and ordinary commands keep their normal output.
Tiny results are unchanged when summary metadata would make them larger.

A summarized item includes `logs_omitted` and a parser-ready `suggested_call` with
`summary_only=false`. That call preserves the **original** observation cursor; using
the newly returned observation token instead would skip omitted lines. Expansion is
bounded by the existing log retention and may report reset or unavailable history.
No log copy, model invocation, Job execution, permission, or waiting policy is added.
Omitting `summary_only` preserves the existing behavior.

`search_and_read` reuses ordinary search-result sparsification after read planning.
It omits redundant phase metadata, not source text, query indexes, failure evidence,
read revisions, or snapshot-bound continuations.


Long-running commands and validations use the canonical WebCodex Job lifecycle. Observe the exact Job returned by the initiating call with `observe_jobs` (or recover it with `list_jobs` when identity was genuinely lost) instead of starting another copy. Jobs are not wrapped as MCP Tasks; WebCodex does not advertise the former Connector-specific MCP Tasks extension.

## First safe prompt

```text
Use the configured WebCodex project. Inspect README.md and summarize the
project structure. Do not edit files or run commands.
```

No project discovery or runtime identifier belongs in this prompt.

## Read and search bounds

- `read_files` is the canonical bounded range reader for one to eight files. A
  one-item batch is the single-range path. Each successful item returns the
  complete-file SHA-256 and bounded line metadata; partial items carry a
  positional one-item `read_files` continuation and are not snapshot-stable.
- `search_project_texts` is the canonical bounded search surface for one to
  eight independent queries (ripgrep first with the existing bounded fallback).
  A one-query batch is the single-query path.

An empty search result is affirmative no-match evidence only after a recognized
backend reports a successful completed/no-match status. Missing or malformed
backend identity, missing completion status, status/output disagreement,
backend failure, Runner failure, timeout, request drop, and provider failure
return failures instead. Search failures retain the compatibility `code` and
add bounded `failure_stage` plus a specific `reason_code`. Batch failure items
retain their broad `reason_code` and preserve the single-search provenance as
`failure_stage` and `detail_code`; successful batch items remain sparse.

Failures return small structured errors with project-relative paths only —
never absolute paths, commands, Runner/provider stderr, or arbitrary provider
prose.

## Common errors

| Code | Meaning | Action |
| --- | --- | --- |
| `project_not_configured` | No canonical setup exists | Run `webcodex setup` |
| `project_credential_invalid` | Private Project Credential is missing or mismatched | Restore both matching private files or recreate the profile |
| `project_credential_rejected` | The reachable server rejected the credential | Restore the server-matching credential |
| `workspace_unavailable` | The configured Git workspace is unavailable | Restore the workspace, then run doctor |
| `server_unreachable` / `agent_offline` | The project Runner/runtime is unavailable | Run `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | The current Runner/runtime lacks a required coding capability | Upgrade all binaries |
| `project_registry_scope_denied` | A project-scoped credential tried to expand or mutate the Project registry outside its granted visibility | Use an already-visible Project or `work_on_project(mode=worktree)` |

## Adaptive Runtime extensions

The same ToolRuntime serves project-scoped local `share`/`run` instances and multi-project hosted Servers through one Adaptive Runtime contract. Project-scoped credentials change visibility and authority, not the model-facing runtime shape. Protocol-specific capabilities and MCP Apps may admit additional hidden presentation or resource operations without creating another runtime surface.

Stateless MCP keeps Memory tools and the Skill compatibility tools `skill_list`
and `skill_read_file` off the top-level `tools/list`, even with full OAuth scopes.
Their exact contracts remain available through `tool_manifest(tool_name=...)`
and execute through `call_runtime_tool` with unchanged scope, Project, permission,
and capability checks. Existing direct protocol compatibility and the
`memory.bootstrap` context sidecar remain supported. Ordinary Skill selection
and execution keep the direct `skill_load` and `run_skill_resource` paths.
The optional closeout helpers `workspace_hygiene_check` and `finish_coding_task`
are model-visible gateway tools; review/coding catalogs still recommend them.

`WEBCODEX_MCP_COMPACT_SCHEMAS` defaults to `true`. Compact `tools/list` omits
`outputSchema` and projects shorter MCP-specific tool/input descriptions for
selection: purpose, nearby tool distinctions, and essential continuation guidance.
Repeated Session/context wrapper and audited common-argument copy is shortened
too. Compact discovery omits only the exact opaque-ID regexes on
`recording_session_id`, `ack_session_message_ids.items`, and
`session_message_resolution.message_id`; their existing parent descriptions keep
the `wc_sess_*` / `wc_msg_*` type hints. Copy the exact returned IDs.
Business-ID, hash/Git fence and resource-path patterns, all bounds, field names,
required fields, enums, object/union shape, annotations, and MCP App/file metadata
are preserved. This is discovery presentation only; runtime argument validation
and execution authority do not change.

Use `tool_manifest(tool_name=...)` for the full exact input contract and operational
description, or set compact schemas to `false` for full discovery schemas.
Canonical ToolSpecs are never rewritten. Focused MCP tests compare inputs against
canonical schemas (with the explicit host-file reference overlay) and enforce
serialized-byte and advertised-tool-count budgets on final Stateless results,
including Session wrappers, gateway tools, and optional App metadata/tools.

### ChatGPT file bridge

When the connected MCP protocol/host admits the artifact capabilities, WebCodex supports
host-native file transfer in both directions without routing complete binary
payloads through model text:

- `import_conversation_files_to_project` imports 1..10 files supplied by the
  ChatGPT host through `openai/fileParams`. This applies to user-selected
  conversation attachments and to newly generated files when the host binds
  them as file parameters. The Control downloads the referenced bytes and
  commits them through the existing bounded artifact-write path; callers should
  not construct download URLs or manually Base64-transfer those files.
- `project_artifact` is the preferred Project-to-model/host read surface. Use
  `action=metadata` for existence/size/MIME/digest/image/archive facts,
  `action=inspect` for one bounded snapshot-fenced Base64 segment,
  `action=image` for native MCP image delivery, and `action=export` for complete
  host/user delivery. Do not loop `inspect` chunks to transfer a complete file.
- `action=export` reuses the existing artifact export authority and returns a
  short-lived authenticated MCP `ResourceLink` plus metadata. `tools/call` does
  not contain the complete binary. The host follows `resources/read` to obtain
  the binary resource; authentication and current project-read authority are
  checked again, and the artifact metadata is revalidated before the bytes are
  returned. The resource URI is not standalone bearer authority; export handles
  are short-lived process-local presentation state, and the normal size, MIME,
  path, and authorization bounds remain in force.

The lower-level `read_project_artifact_metadata` and `read_project_artifact`
tools remain operator/gateway primitives. The legacy `export_project_artifact`
compatibility tool has been removed; complete host delivery is exposed only as
`project_artifact(action=export)`. Office artifacts such as DOCX/PPTX/XLSX and
PDFs use the same underlying artifact transport and can therefore move between
a project and a supporting ChatGPT host without a model manually carrying their
Base64.

Use [Coding Workflow](CODING_WORKFLOW.md) for the canonical `work_on_project` bootstrap, behavioral-role mental model, and validation/closeout guidance. See [Architecture](ARCHITECTURE.md) and the `webcodex` CLI for operator tooling.
