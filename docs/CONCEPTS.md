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
Actions, and MCP. Examples include `list_projects`, `read_file`, `git_status`,
`replace_line_range`, `validate_patch`, `apply_patch_checked`, `run_shell`,
`run_job`, `show_changes`, `start_session`, and `session_handoff_summary`.

The recommended model workflow is to inspect first, use structured edit tools
when line ranges are known, validate patches before applying them, run bounded
shell/job checks, then use `show_changes` and session tools before summarizing.

For developer architecture details on migrating runtime tool declarations toward
a `ToolDefinition` registry, see
[TOOL_DEFINITION_REGISTRY.md](TOOL_DEFINITION_REGISTRY.md).

Codex delegation (`run_codex`) is currently hidden/disabled from model-facing
surfaces: GPT Actions, MCP `tools/list`, runtime tool discovery, and generic
model-facing dispatch. Do not treat it as the recommended path.

### GPT Actions surface

GPT Actions use the WebCodex OpenAPI schema from:

```text
https://your-domain.example/openapi.json
```

This surface is intentionally narrower than the admin API. It is for runtime,
project, file, Git, patch, shell/job, artifact, and session workflows. It does
not expose user creation, PAT creation, agent-token creation, pairing,
enrollment, setup, server management, or audit endpoints.

### MCP surface

MCP clients connect to:

```text
https://your-domain.example/mcp
```

MCP and GPT Actions share the same `ToolRuntime`, agent registry, project ids,
metadata-backed OAuth checks, and session recording. MCP is a remote WebCodex
runtime endpoint; external MCP-server brokering is future work, not required for
the current endpoint.

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

`start_session` creates an in-memory task tracking session and returns a
`wc_sess_*` id. Later REST calls can pass it as tool metadata, and MCP calls can
pass it as reserved `_session_id` metadata. Session recording is bounded and
redacted; it is not a complete audit log and is lost on server restart.

Explicit `session_id` always wins over a current-session binding. An unknown
explicit session id must fail as `unknown_session_id`; it should not silently
fall back to another session.

`bind_current_session`, `current_session`, and `unbind_current_session` let a
caller bind a project-scoped session to later project tool calls for the same
principal, transport, and project. This is convenience state, not persistent
identity.

`session_handoff_summary` is a read-only structured handoff tool. It summarizes
session info, message-board state, recent progress/decisions, open
todos/risks/questions/guidance, recent failed tools, and optional bounded
workspace/checkpoint context. It does not call an LLM.

`session_hint` is an optional lightweight hint added to recorded tool outputs
when the session has open guidance, question, todo, or risk messages. It
contains counts and priority only; it does not include message text.

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
