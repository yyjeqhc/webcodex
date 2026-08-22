# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex exposes an MCP endpoint so ChatGPT, Claude, and other MCP clients can
work with a repository through the Runner that owns it. For a first connection,
start with the client setup below; protocol surfaces and scope ceilings are
reference material, not onboarding prerequisites.

## ChatGPT Developer Mode: first connection

For the default temporary public path, install
[`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/) and run:

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
as the source of truth for URL and authentication.

## Claude and other MCP clients

Use the same printed `/mcp` URL and authentication values. In Claude, add a
custom connector and paste the MCP URL. Other MCP clients should be configured
with the same endpoint and the authentication mechanism reported by the CLI.
For local-only clients, `webcodex share --tunnel none` exposes the loopback MCP
endpoint without `cloudflared`.

## Existing Server

If you already have a stable WebCodex Server URL, use the long-lived path:

```bash
webcodex connect https://webcodex.example
```

`connect` starts/reuses the local Runner and prints the MCP URL and credential
source after the connection is verified. A generated shared key is fully
disclosed only on its allowed first output. `status` and log commands do not
reveal it. Self-hosting is documented in [Deployment](DEPLOYMENT.md).

Bearer/shared-key authentication is the simplest path. When a client requires
OAuth, use `share --auth oauth` or `connect --auth oauth` with that client's exact
callback URL and follow the CLI output. Managed-user OAuth remains a separate
advanced identity flow.

## Advanced / reference

### MCP model surfaces

The project-first `webcodex run` / `webcodex share` path starts a Server with the
project-bound `canonical_connector` surface. `webcodex connect <server>` uses
the MCP surface selected by that existing Server; without Connector
configuration, the default is the broader `local_coding` surface. Operators may
explicitly select `full_operator_runtime`. These names describe protocol/tool
contracts; a first-time user does not need to choose among them.

Hosted clients need public HTTPS. `share` supplies a temporary Cloudflare Quick
Tunnel by default; `connect` uses an existing hosted Server; self-hosted
deployments provide their own stable HTTPS origin. Do not use bootstrap/admin
tokens, Runner tokens, or the persistent project-first Connector credential as
a public sharing secret.

### Built-in local MCP gateway

A hosted WebCodex Server can also expose Runner-owned local stdio MCP providers through the same stable `/mcp` endpoint. The top-level catalog remains fixed: authorized callers see one `mcp_tool` meta-tool rather than one top-level tool per upstream provider. `mcp_tool` supports `list`, `describe`, and `call`. Provider ids and upstream tool names are logical identities; Runner/process/provider-instance identities and schema revision tokens stay internal. A successful `describe` records a bounded server-side schema observation. `call` resolves the current exact Runner/provider instance once, rechecks the current tool schema on that same persistent provider session, and refuses the effectful call when the schema changed. Provider replacement is reported separately and is never silently retargeted or replayed.

No-argument `mcp_tool(action=list)` reports whether a registered logical provider id is uniquely routable (`resolvable` versus `ambiguous`); it is not a provider health check. `list(server=...)` and `describe` perform provider interaction. The outer `/mcp` endpoint's 2025/2026 support is separate from the Runner-to-provider gateway V1 compatibility contract: configured local providers currently use the bounded 2025-06-18 stdio tool subset documented in [Runner](RUNNER.md#provider-side-gateway-v1-compatibility), not an arbitrary/latest transparent MCP bridge. Outer caller request `_meta` is intentionally not forwarded to local providers.

Configure providers on the Runner in `[mcp]`; no additional daemon, sidecar, public resource URL, or per-provider ChatGPT App is required. Local MCP access is guarded by the explicit `mcp:local` scope. Direct shared-key, project, open-anonymous, and legacy OAuth defaults do not acquire that scope. For ordinary hosted shared-key OAuth, `webcodex connect ... --auth oauth --oauth-local-mcp` explicitly adds the class-level authority for current and future local MCP providers in that same shared-key Runner group. A real ceiling change revokes existing grants and requires browser authorization again. Adding or replacing a provider later therefore does not require a new OAuth client/App, but only credentials that previously opted into `mcp:local` may use it.

### OAuth2

When OAuth is enabled on a managed/self-hosted Server, or by `webcodex share --auth oauth`, MCP clients can use
the authorization-code flow instead of a static token. Register the exact
ChatGPT callback URL as an OAuth client redirect URI; keep `offline_access`
enabled when offered (it is a protocol-level refresh-token scope and grants no
extra permission). Server-side OAuth setup is in
[Deployment](DEPLOYMENT.md#oauth2).

For project-first sharing, the authorization page asks for the temporary Project share credential and issues an `oauth2_project` identity carrying only `runtime:read`, `project:read`, `project:write`, and `job:run`. It does not create a managed user and OAuth tokens cannot be used on Runner transport. Quick Tunnel issuer URLs change between runs; use `--tunnel none --public-url https://...` behind your own stable HTTPS proxy/tunnel when the OAuth issuer must remain stable.

For an existing hosted Server, ordinary `connect --auth oauth` uses the shared-key OAuth bridge. The OAuth client and every code/access/refresh grant remain bound to the same `shared_key_hash` that groups the direct shared-key Runner/projects/jobs. Direct shared-key bearer authority stays fixed at `runtime:read`, `project:read`, `project:write`, `job:run`, `computer:read`, and `computer:control`. A fresh OAuth client starts with that full baseline, but a protected existing client may retain a narrower valid baseline subset. `--oauth-computer-permissions` appends only `computer:launch`, `computer:display_read`, `computer:pointer_control`, `computer:clipboard_read`, and `computer:clipboard_write` to that existing baseline subset; it never restores an absent baseline scope. The browser consent page leaves every optional permission unchecked and grants only selected permissions that the OAuth request actually requested. Launch consent requires both `computer:read` and `computer:launch`; display requires `computer:read` plus `computer:display_read`; pointer requires `computer:read`, `computer:control`, `computer:display_read`, and `computer:pointer_control`; clipboard read/write likewise require their baseline read/control prerequisite plus the matching optional scope. Missing request prerequisites are unavailable rather than auto-added, so consent, token projection, and runtime scope gates remain statically aligned. A real ceiling change revokes prior grants. `account:manage`, `admin`, `job:detach`, every `agent:*` transport scope, and future scopes remain outside this bridge. `offline_access` is protocol-only. Consent-page Runner capability is evaluated per same connected Runner and is rechecked on POST; it is backend availability, not a promise of OS/native permission or call success. At runtime, OAuth `tools/list` hides tools whose required scopes are absent, while direct `tools/call` authorization and live Runner/native checks remain authoritative. The managed-user flow remains separate as `connect --auth managed-oauth`.

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

## The project-bound surface

When the Server is started with project-first Connector configuration
(`canonical_connector`), MCP `tools/list` contains exactly these fourteen
operations. This is the surface used by `webcodex run` and `webcodex share`;
a generic hosted/self-hosted Server without Connector context exposes
`local_coding` by default (or explicit `full_operator_runtime`) instead:

```text
task_start
task_list
task_resume
files_list
files_read
files_search
code_navigate
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
code_impact
```

The Connector context already binds the configured repository. Start with
`task_start`; do not call project-discovery, session, or runtime tools, and do
not put a runtime project id in the prompt. The same chat window continues the
current repository automatically. `task_list` and `task_resume` are explicit
recovery tools when a client can no longer present its transport window
identity.

## Golden coding loop

```text
task_start
→ files_list
→ files_read / files_search / code_navigate / code_impact
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

- `files_list` answers "what is in this project" from the Git index, so
  ignored directories never appear. Call it before guessing paths.
- `code_navigate` provides read-only language-server status, document/workspace
  symbols, definitions, references, diagnostics, and hover. It accepts only
  project-relative paths and 1-based Unicode scalar positions; the Connector
  chooses the bound executor project. Arguments are operation-specific:
  `status` takes no extras; document symbols and diagnostics take `path`;
  workspace symbols takes `query`; definition, references, and hover take
  `path` + `line` + `column`. Unsupported fields are rejected. It is available
  in normal, inspect, and read-only tasks.
- `code_impact` performs one bounded call-hierarchy operation from a
  project-relative source position. It accepts `incoming`, `outgoing`, or
  `both`, breadth-first depth 1 or 2, and a global edge limit of 1..100. It
  returns only normalized project-local roots, edges, and bounded call-site
  ranges; unsupported language servers fail explicitly with no grep or AST
  fallback. It is available in normal, inspect, and read-only tasks.
- `edits_apply` is the guarded edit tool; `commands_run` is the bounded escape
  hatch for commands that need a shell.
- `checks_run` validates. Use a stable `operation_id` so an exact retry reuses
  the operation.
- `task_finish` produces a stable result; a human reviews and accepts or
  rejects it locally with `webcodex task accept <id>` / `webcodex task reject
  <id>`. The model can never accept its own work.

### Validation recipes

`checks_run` accepts `format`, `check`, and `test` plus an optional `recipe`
enum (`rust`, `node`, `python`, `go`). Omit `recipe` for automatic resolution
from the nearest `Cargo.toml`, `package.json`, `pyproject.toml`, or `go.mod`
relative to the task `cwd`. Recipes do not install dependencies, mutate
lockfiles, or use the network. A missing tool is an executor failure; a
started validator returning non-zero is an assertion failure.

| Recipe | Marker | `format` | `check` | `test` |
| --- | --- | --- | --- | --- |
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` |
| Node | `package.json` | first of `format:check`, `format-check`, `check:format` | first of `check`, `typecheck`, `lint` | exact `test` |
| Python | `pyproject.toml` | configured Ruff/Black | configured Ruff/Mypy | configured pytest |
| Go | `go.mod` | unavailable | `go vet ./...` | `go test -json ./...` |

### Long validation continues durably

`checks_run` and `commands_run` use durable executions and may quick-yield
after about 8 seconds while work continues. On the fourteen-tool Connector
surface, call `task_review` with `after_cursor` / `wait_ms` (and
`include_output_tail=true` when output is needed) until the execution becomes
terminal; use `task_cancel` to stop it. Do not re-run an operation merely to
poll it.

The broader `local_coding` and `full_operator_runtime` MCP surfaces expose raw
Job tools such as `job_status`, `job_log`, `validation_summary`, and
`stop_job`; those tools are not part of the fourteen Connector capabilities.

## First safe prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

No project discovery or runtime identifier belongs in this prompt.

## Read and search bounds

- `read_file` is a bounded streaming range reader: `start_line` (default 1),
  `limit` (default 2000, max 2000), returns the range plus the complete-file
  SHA-256 and line metadata, and a `next_start_line` to continue.
- `read_files` batches up to 8 single-file reads with independent item
  results.
- `search_project_text` is the default search tool (ripgrep first, bounded in
  work and bytes); `search_project_texts` batches up to 8 queries.

Failures return small structured errors with project-relative paths only —
never absolute paths, commands, or Runner stderr.

## Common errors

| Code | Meaning | Action |
| --- | --- | --- |
| `project_not_configured` | No canonical setup exists | Run `webcodex setup` |
| `project_credential_invalid` | Private Project Credential is missing or mismatched | Restore both matching private files or recreate the profile |
| `project_credential_rejected` | The reachable server rejected the credential | Restore the server-matching credential |
| `workspace_unavailable` | The configured Git workspace is unavailable | Restore the workspace, then run doctor |
| `server_unreachable` / `agent_offline` | The project runtime or Agent is unavailable | Run `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | The Agent lacks a coding capability | Upgrade all binaries |
| `task_not_active` | The task can no longer mutate or execute | Start a new task |
| `execution_not_terminal` | Finish is blocked by active/unknown work | Review/wait/cancel |
| `checks_required` | A normal task has not run checks | Call `checks_run` |
| `checks_stale` | The workspace changed after the last check | Run a new check |

## Advanced runtime surface

Beyond the project-bound Connector, WebCodex can run as a multi-project
management ToolRuntime with discovery, session, LSP, raw job, and artifact
tools. That is an advanced surface for operators, not the canonical project
Connector and not a prerequisite for ordinary coding.

### ChatGPT file bridge

On broader MCP operator surfaces that expose artifact tools, WebCodex supports
host-native file transfer in both directions without routing complete binary
payloads through model text:

- `import_conversation_files_to_project` imports 1..10 files supplied by the
  ChatGPT host through `openai/fileParams`. This applies to user-selected
  conversation attachments and to newly generated files when the host binds
  them as file parameters. The Control downloads the referenced bytes and
  commits them through the existing bounded artifact-write path; callers should
  not construct download URLs or manually Base64-transfer those files.
- `export_project_artifact` prepares one bounded project artifact for download
  and returns a short-lived authenticated MCP `ResourceLink` plus metadata.
  `tools/call` does not contain the complete binary. The host follows
  `resources/read` to obtain the binary resource; authentication and current
  project-read authority are checked again, and the artifact metadata is
  revalidated before the bytes are returned.
- The resource URI is not standalone bearer authority. Export handles are
  short-lived process-local presentation state, and the normal project artifact
  size, MIME, path, and authorization bounds remain in force.

`read_project_artifact` remains the bounded chunk-inspection API; it is not the
large-file download path. Office artifacts such as DOCX/PPTX/XLSX and PDFs use
the same artifact transport and can therefore move between a project and a
supporting ChatGPT host without a model manually carrying their Base64.

When a broader coding surface exposes `work_on_project` or `start_coding_task`,
use the [Coding Workflow](CODING_WORKFLOW.md) for the canonical bootstrap versus
behavioral-role mental model and validation/closeout guidance. See
[Architecture](ARCHITECTURE.md) and the `webcodex` CLI for operator tooling.
