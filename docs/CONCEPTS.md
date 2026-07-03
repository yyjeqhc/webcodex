# Concepts

[English](CONCEPTS.md) | [简体中文](CONCEPTS.zh-CN.md)

This is the WebCodex vocabulary map for onboarding. Use it with
[QUICK_START.md](QUICK_START.md); use the focused docs for exact setup
commands.

## Mental model

```text
GPT Actions / MCP / REST client
        |
        | HTTPS + shared key, wc_pat_*, or wc_oat_*
        v
WebCodex server
        |
        | agent transport + wc_agent_*
        v
webcodex-agent
        |
        v
registered project directory
```

The server exposes stable APIs and authentication boundaries. The agent connects
back to the server and performs allowed work inside registered project roots.
WebCodex is self-hosted; it is not hosted SaaS, tenant isolation, OIDC, JWKS,
JWT ID tokens, or userinfo.

## Core components

### Server

`webcodex` is the HTTP server. It exposes REST APIs, `/openapi.json` for GPT
Actions, `/mcp` for MCP clients, and agent connection endpoints. It stores
runtime state and credential hashes, but project execution is normally routed to
connected agents.

### Agent

`webcodex-agent` is the execution worker. It connects to the server with a
`wc_agent_*` token and a `client_id`, reads `projects.d/*.toml`, enforces local
allowed roots, and runs file, Git, patch, shell, job, and Cargo requests inside
registered project directories.

### Project

A project is a registered workspace. Agent-backed project ids use:

```text
agent:<client_id>:<project_id>
```

The `<project_id>` comes from the top-level `id` field in an agent
`projects.d/*.toml` file. The path remains on the agent host.

### Runtime tool

Runtime tools are the typed operations exposed through `/api/tools/call`, GPT
Actions, and MCP. Examples include `list_projects`, `read_file`,
`search_project_text`, `git_status`, `replace_line_range`, `insert_at_line`,
`delete_line_range`, `apply_text_edits`, `validate_patch`,
`apply_patch_checked`, `cargo_fmt`, `cargo_check`, `cargo_test`, `run_shell`,
`run_job`, `show_changes`, `start_session`, `start_coding_task`,
`finish_coding_task`, and `session_handoff_summary`.

The recommended coding workflow is:

1. Start with `start_coding_task` and keep the explicit `session_id`.
2. Inspect with `read_file`, `search_project_text`, and `show_changes`.
3. Edit with `replace_line_range`, `insert_at_line`, `delete_line_range`,
   `apply_text_edits`, or `apply_patch_checked`.
4. Validate with `cargo_fmt`, `cargo_check`, `cargo_test`, `validate_patch`, or
   `apply_patch_checked`.
5. Review with `show_changes`, `git_diff_hunks`, and
   `workspace_hygiene_check`.
6. Finish with `finish_coding_task` and, for multi-step handoff,
   `session_handoff_summary`.

`run_shell` and `run_job` remain bounded command/job escape hatches. They are not
the default validation source and are not the primary source-editing path.

For developer architecture details on migrating runtime tool declarations toward
a `ToolDefinition` registry, see
[TOOL_DEFINITION_REGISTRY.md](TOOL_DEFINITION_REGISTRY.md).

Codex delegation (`run_codex`) is currently hidden/disabled from model-facing
surfaces: GPT Actions, MCP `tools/list`, runtime tool discovery, and generic
model-facing dispatch. The legacy `/api/codex/run` endpoint is default-off and
only mounted with `WEBCODEX_ENABLE_LEGACY_CODEX_RUN=1`; that opt-in does not
re-enable `run_codex`. Do not treat it as the recommended path.

### Artifact transfer

Artifact transfer is a bounded project artifact transfer primitive. It is for
importing or exporting binary and external files associated with a project, using
project-relative paths, byte limits, chunk limits, and sha256 guards. It is not
the source-editing path, object storage, a gallery, or a large-file platform.

For source edits, continue to use `replace_line_range`, `insert_at_line`,
`delete_line_range`, `apply_text_edits`, and `apply_patch_checked`. Do not treat
`save_project_artifact`, `artifact_upload_begin`, `artifact_upload_chunk`,
`artifact_upload_finish`, or `artifact_upload_abort` as replacements for
source-editing tools. Compatibility edit tools such as `write_project_file` and
`replace_in_file` remain available through `callRuntimeTool`.

### GPT Actions surface

GPT Actions use the WebCodex OpenAPI schema from:

```text
https://your-domain.example/openapi.json
```

This surface is intentionally narrower than the admin API. It is for runtime,
project, file, Git, patch, shell/job, artifact, and session workflows. It does
not expose user creation, PAT creation, agent-token creation, pairing,
enrollment, setup, server management, or audit endpoints.

GPT Actions must stay below the 30-operation limit. The current WebCodex
OpenAPI surface is 25 operations, so chunked artifact upload and compatibility
edit tools remain available through `callRuntimeTool` rather than dedicated GPT
Action operations.

### MCP surface

MCP clients connect to:

```text
https://your-domain.example/mcp
```

MCP and GPT Actions share the same `ToolRuntime`, agent registry, project ids,
metadata-backed OAuth checks, and session recording. MCP is a remote WebCodex
runtime endpoint; external MCP-server brokering is future work, not required for
the current endpoint.

Runtime tools can be exposed directly as MCP tools, subject to the tool manifest
and client constraints. This is separate from GPT Actions, where the dedicated
operation surface must stay under the 30-operation limit.

## Authentication vocabulary

| Credential | Used for | Not for |
| --- | --- | --- |
| `WEBCODEX_TOKEN` | Server bootstrap/admin setup | GPT Actions, MCP, agents, daily runtime calls |
| Shared key | Fast agent + GPT/MCP quick start when the host supports static Bearer/API-key auth | Production IAM, admin, managed-user identity |
| `wc_acct_*` | One-time local creation of PATs and agent tokens | GPT Actions, MCP, runtime API, agent transport |
| `wc_pat_*` | Managed runtime API, GPT Actions, MCP, REST tools | Agent transport |
| `wc_oat_*` | OAuth2 delegated runtime access | Agent transport, admin by default |
| `wc_agent_*` | `webcodex-agent` connectivity only | GPT Actions, MCP, runtime API |

Static Bearer/API-key host auth can use either a shared key for quick start or a
`wc_pat_*` token for managed mode:

```text
Authorization: Bearer <token-or-shared-key>
```

OAuth is a separate flow. Blank OAuth client fields do not become no-auth,
shared-key fallback, or static Bearer auth. OAuth2 access tokens remain rejected
on agent transport endpoints.

The shared-key OAuth bridge is for OAuth-only hosts where the operator still
wants low-config shared-key onboarding. It is disabled by default and must be
explicitly enabled with `WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true`. The user enters
a shared key on the WebCodex OAuth page; WebCodex stores only the shared-key
hash and issues OAuth tokens through the authorization-code flow. Bridge-issued
tokens are capped to runtime/project/job scopes and do not receive `admin`,
`account:manage`, or `agent:*` scopes.

## Sessions, handoff, and hints

`start_session` creates a bounded task tracking session record and returns a
`wc_sess_*` id. It does not automatically bind that session as current for
future calls. Later generic `/api/tools/call` calls can pass the id as
`recording_session_id` recorder metadata, tool-specific calls can pass explicit
`session_id` input, and MCP calls can pass it as reserved `_session_id`
metadata. Session records, events, and messages are bounded, redacted
task-recorder metadata. When session persistence is configured, WebCodex can
persist and restore them through the `sessions.json` ledger. The ledger is a
durable task-continuity and handoff record, not a complete audit log.

Explicit `session_id` always wins over a current-session binding. An unknown
explicit session id must fail as `unknown_session_id`; it should not silently
fall back to another session.

`bind_current_session`, `current_session`, and `unbind_current_session` let a
caller bind a project-scoped session to later project tool calls for the same
principal, transport, and project. This is process-local in-memory convenience
state, not the durable session ledger. Do not assume it survives process
restart; pass an explicit `session_id` or `recording_session_id` for
deterministic handoff.

`session_handoff_summary` is a read-only structured handoff tool. It summarizes
session info, message-board state, recent progress/decisions, open
todos/risks/questions/guidance, recent failed tools, and optional bounded
workspace/checkpoint context. It requires an explicit `session_id`, does not
fall back to current-session binding, and does not call an LLM.

`start_coding_task` is the recommended coding-loop entry point. It creates a
session, returns the explicit `session_id`, gathers deterministic project/runtime
context, and defaults `bind_current=false`. `finish_coding_task` is the matching
closeout aggregate for an explicit `session_id`; it can include `show_changes`,
workspace hygiene, handoff, and validation summary sections.

`session_hint` is an optional lightweight hint added to recorded tool outputs
when the session has open guidance, question, todo, or risk messages. It
contains counts and priority only; it does not include message text.

## Validation summaries

Validation summaries come from session ledger events. Validation-like tools are
`cargo_fmt`, `cargo_check`, `cargo_test`, `validate_patch`, and
`apply_patch_checked`. `run_shell` is not classified as validation by default.

The session ledger may store a small, sanitized, bounded
`validation_output_summary` for Cargo validation helpers, derived from already
bounded output tails and filtered before persistence. `finish_coding_task` and
`session_handoff_summary` validation outputs do not expose raw stdout/stderr,
excerpt fields, or `validation_output_summary`.

The minimal parser extracts only stable facts from safe bounded metadata, such
as Cargo severity/code/span and test summary counts. It does not infer root
causes, suggest fixes, call an LLM, use LSP, or use tree-sitter.

## Operating modes

Service mode uses systemd units for the server and agent. It is recommended for
long-running self-hosted servers and stable agent hosts because it gives restart
and boot persistence. Configure command environment through agent shell profiles
because systemd does not read interactive shell files such as `.bashrc`.

Manual/no-service mode runs the agent in the foreground or with a simple
background wrapper such as `nohup`. It is useful for local evaluation,
containers, smoke tests, and hosts where systemd is not available. It is easier
to inspect and stop manually, but it does not provide the same lifecycle
management as a service.

For agent transport, `transport = "auto"` tries QUIC first only when a `[quic]`
section is configured, then falls back to WebSocket and then polling. Without
`[quic]`, `auto` starts at WebSocket. GPT Actions and MCP still use HTTPS; QUIC
is only for `webcodex-agent` connectivity.

## Where to go next

- First setup and decision tree: [QUICK_START.md](QUICK_START.md)
- GPT Actions: [GPT_ACTIONS.md](GPT_ACTIONS.md)
- MCP: [MCP.md](MCP.md)
- Deployment and systemd: [DEPLOYMENT.md](DEPLOYMENT.md)
- Authentication model: [AUTH_MODEL.md](AUTH_MODEL.md)
- OAuth2 smoke test: [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md)
- Testing and CI lanes: [TESTING.md](TESTING.md), [CI_LANES.md](CI_LANES.md)
- Security: [../SECURITY.md](../SECURITY.md), [AUTH_ARCHITECTURE.md](AUTH_ARCHITECTURE.md)
