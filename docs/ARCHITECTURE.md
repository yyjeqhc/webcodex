# Architecture Boundary Map

This document is a maintenance map for the current Rust module split. It is not
a product architecture narrative. Its purpose is to keep new code inside the
right boundary and prevent recently split root files from becoming large mixed
responsibility files again.

For product vocabulary and the runtime surface overview, see
[CONCEPTS.md](CONCEPTS.md).

## Maintenance Rules

- Root files should stay thin. A root may own route wiring, public re-exports,
  request/response glue, and small compatibility shims, but domain behavior
  belongs in a named child module.
- Binary roots should orchestrate. They may parse top-level CLI shape, choose a
  command, and call module functions; command behavior belongs under the binary
  module directory.
- Tests should follow the code boundary. When a domain already has a
  `tests/` submodule, add tests there instead of growing the production root.
- Runtime tools, metadata, registry schemas, OAuth scope policy, MCP
  `tools/list`, and OpenAPI names must stay synchronized when tools are added
  or renamed.
- Do not create compatibility layers by default. Keep them only for an external
  public contract, release artifact, documented API, or explicit migration path.
- Do not use module-wide `#![allow(dead_code)]` in runtime registry modules.
  Removed dead-code residue should stay removed; unavoidable test-only helpers
  should use `#[cfg(test)]` or item-scoped allowances.

## Runtime HTTP

`src/runtime_http.rs` is the route root for runtime HTTP endpoints and the
generic `callRuntimeTool` adapter. It should stay focused on:

- Salvo handler entry points and HTTP status rendering.
- Extracting request bodies into runtime tool calls.
- Passing depot `AuthContext` into `ToolRuntime`.
- Recording route-level action audit entries.
- Re-exporting route handlers from `src/runtime_http/*`.

Feature behavior belongs in child modules:

- `src/runtime_http/projects.rs` for project list/register/create HTTP routes.
- `src/runtime_http/project_files.rs` for file, patch, git status/diff, and
  project-file routes.
- `src/runtime_http/jobs.rs` for runtime job and shell job HTTP routes.
- `src/runtime_http/import_http.rs` for conversation-file import routes.
- `src/runtime_http/tests/*` for runtime HTTP tests.

Do not add new project, file, job, or import behavior directly to
`src/runtime_http.rs`. A new dedicated REST route should get a child module or
join the closest existing child module. A generic model-facing runtime tool
should usually be implemented in `src/tool_runtime/*`, then exposed through the
generic adapter or a thin dedicated HTTP wrapper.

## OAuth HTTP, Auth, And DB

These three areas are intentionally separate:

- `src/oauth_http.rs` and `src/oauth_http/*` own OAuth HTTP endpoints, form
  parsing, authorize/token/revoke flows, metadata responses, browser HTML, and
  OAuth-specific HTTP errors.
- `src/auth/mod.rs` and `src/auth/*` own bearer authentication, principal and
  auth context modeling, token verifier dispatch, scope constants, route surface
  gates, and Salvo auth middleware.
- `src/db.rs` and `src/db/*` own SQLite schema creation, migrations, and
  persistence/query methods for users, tokens, agents, audit, and OAuth rows.

Boundary guidance:

- Put OAuth route behavior in `src/oauth_http/*`; do not move route rendering or
  form-body policy into `auth` or `db`.
- Put bearer-token verification and scope authorization in `src/auth/*`; do not
  make OAuth HTTP handlers duplicate verifier-chain semantics.
- Put storage shape and SQL in `src/db/*`; do not make DB modules know about
  Salvo responses, HTML, or transport-specific route policy.
- Keep OAuth subject-model changes coordinated across OAuth HTTP, auth verifier
  dispatch, DB schema/queries, and scope policy tests.

`src/oauth_http.rs` is the OAuth route facade. New endpoint code should land in
the relevant child module:

- `clients.rs` for OAuth client management routes.
- `managed_authorize.rs` for managed-user authorize and consent flow.
- `shared_key_bridge.rs` for shared-key OAuth bridge authorize flow.
- `token.rs` for authorization-code and refresh-token exchange.
- `revoke.rs` for RFC 7009 revocation.
- `metadata.rs` for well-known metadata.
- `scope_registry.rs` for OAuth scope normalization and supported-scope lists.
- `responses.rs` and `html.rs` for response helpers and HTML rendering.

`src/oauth_http/tests.rs` is only the test module root. Add endpoint coverage to
`src/oauth_http/tests/*` by domain rather than adding large test blocks to the
OAuth production facade.

`src/auth/mod.rs` is the auth facade and compatibility re-export layer. Add new
auth logic to:

- `principal.rs` for identity/auth context types.
- `scopes.rs` for scope constants and validation helpers.
- `middleware.rs` for HTTP extraction and route surface enforcement.
- `tokens.rs` for verifier-chain behavior.
- `pat.rs` for managed token generation, hashing, and validation utilities.
- `shared_key.rs` for shared-key and open-anonymous helpers.

`src/db.rs` still contains some general database methods, but new table-specific
or feature-specific persistence should prefer:

- `src/db/accounts.rs`
- `src/db/agents.rs`
- `src/db/audit.rs`
- `src/db/oauth.rs`
- `src/db/schema.rs`

## Tool Runtime

`src/tool_runtime/*` is the protocol-independent execution layer. It should not
depend on Salvo or HTTP request/response types. HTTP, MCP, and GPT Action
wrappers translate their transport envelope into a `ToolCall` and call
`ToolRuntime`.

Current responsibilities:

- `mod.rs` is the tool-runtime facade and central dispatch implementation.
- `types.rs` owns `ToolCall`, `ToolResult`, `ToolSpec`, known tool names, and
  tool input structs.
- `kernel.rs` owns the transport-aware call envelope and error status used by
  wrappers.
- `runtime.rs` owns `ToolRuntime` construction/state.
- `sessions.rs` and `session_context.rs` own current-session bindings, session
  guards, session telemetry, and session-scoped messages.
- `project_resolution.rs` and `projects.rs` resolve runtime project ids and
  project management behavior.
- `agent_authorization.rs` enforces agent-backed project ownership and
  capability checks.
- Domain modules such as `files.rs`, `git.rs`, `patch.rs`, `jobs.rs`,
  `shell.rs`, `checkpoint.rs`, `handoff.rs`, `hygiene.rs`, `metadata.rs`,
  `cargo.rs`, and `codex.rs` own tool behavior for their domain.
- `src/tool_runtime/tests/*` owns runtime tool tests by domain.

Do not add HTTP-only parsing, Salvo responses, CLI output formatting, or agent
transport protocol handling here. Keep those concerns in their adapter modules.

### Tool Registry

`src/tool_runtime/registry/*` owns tool metadata surfaces:

- `tool_specs.rs` builds exposed `ToolSpec` values.
- `input_schemas.rs` builds JSON input schemas.
- `output_schemas.rs` builds JSON output schemas.
- `annotations.rs` builds tool annotations.
- `mod.rs` only wires the registry modules together.

Registry modules describe tools; they should not implement tool behavior. When
adding or renaming a tool, update the parser/types, dispatch, metadata,
registry, OAuth scope policy, OpenAPI accepted names/examples, MCP schema tests,
and consistency tests together.

## Shell Client And Agent Runtime

`src/shell_client/mod.rs` is the server-side facade for registered shell agents
and agent transport routes. It owns the shared `ShellClientRegistry` type and
thin route/audit glue. Child modules own the domain behavior:

- `agents.rs` for agent registration and registry views.
- `auth.rs` for agent owner and agent-transport scope enforcement.
- `handlers.rs` for HTTP handler entry points.
- `jobs.rs` and `job_updates.rs` for shell job lifecycle and job update state.
- `projects.rs` for agent project registry operations.
- `requests.rs` for queued request/response flow.
- `polling.rs` for polling transport behavior.
- `state.rs` for registry state structures.
- `validation.rs` for shell/file request validation helpers.

Keep server-side agent transport and registry behavior in `shell_client`. Do not
put it in `runtime_http`; runtime HTTP should call runtime tools, and runtime
tools should ask the registry or agent protocol layer to perform agent-backed
work.

`src/bin/webcodex-agent.rs` is the agent binary root. It still owns top-level
argument parsing, process/job orchestration, and the executable entry point, but
new agent domain behavior should go under `src/bin/webcodex_agent/*`:

- `config.rs` for config loading and policy types.
- `transport.rs` for polling/WebSocket/QUIC client transport behavior.
- `dispatch.rs` for mapping protocol requests to domain handlers.
- `projects.rs` for project config parsing and project operations.
- `shell.rs` for shell command execution and prepared shell profiles.
- `files.rs`, `patches.rs`, `artifacts.rs`, and `checkpoints.rs` for file and
  artifact/checkpoint request domains.
- `output.rs` for agent command result shaping.

The agent binary should not become the home for new file, project, transport,
or shell features. Add or split modules under `webcodex_agent` instead.

## CLI Binary

`src/bin/webcodex-cli.rs` is the standalone management/setup binary root. It may
own top-level action enums, top-level argument parsing, and command dispatch,
but command implementation belongs under `src/bin/webcodex_cli/*`.

Current command/module boundaries:

- `usage.rs` for help text.
- `tokens.rs` and `token_commands.rs` for local token generation and token
  creation commands.
- `setup.rs` for single-user setup orchestration.
- `pairing.rs` for pairing create/enroll behavior.
- `connect.rs` for generated connection guidance.
- `doctor.rs` and `doctor_support.rs` for diagnostics.
- `server.rs` for local server init/install/status/up helpers.
- `agent_service.rs` for agent service install/status helpers.
- `profiles.rs` for client profile path rules.
- `env.rs`, `http.rs`, `output.rs`, and `system.rs` for shared command support.
- `src/bin/webcodex_cli/tests/*` for CLI tests.

New CLI commands should add command-specific behavior under
`src/bin/webcodex_cli/*` and keep the binary root to orchestration. Help smoke
coverage should stay near the CLI tests, not in the binary root.
