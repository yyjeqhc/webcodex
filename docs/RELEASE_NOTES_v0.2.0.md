# WebCodex v0.2.0

[English](RELEASE_NOTES_v0.2.0.md)

**Status:** GitHub release preparation. This is a binary release from the `v0.1.0` tag to the current `main` HEAD. npm publish is intentionally out of scope for this release.

## Upgrade from v0.1.0

Binary users: download new platform artifacts from the GitHub release and replace existing `webcodex`, `webcodex-cli`, and `webcodex-agent` binaries.

npm wrapper users: the npm `@yyjeqhc/webcodex` package version remains at `0.1.0` for this release. The npm wrapper is a thin installer that downloads platform binaries; it will be updated in a later release.

Rust crate version: `Cargo.toml` is updated from `0.1.0` to `0.2.0`.

## Highlights

- OAuth2 authorization code flow with client management, PKCE, browser consent, refresh token rotation, and token revocation.
- Structured line edit tools: `replace_line_range`, `insert_at_line`, `delete_line_range` for scoped source edits with known line numbers.
- `show_changes` tool for worktree inspection, session activity summary, and diff review.
- Session event recorder foundation: `start_session` and `session_summary` for task tracking.
- Session-aware `show_changes`: pass `session_id` to include session activity alongside git state.
- `ToolMetadata` foundation: centralized risk, OAuth scope, read-only/destructive hints, and path hints for each tool.
- `ToolKernel` facade: unified dispatch layer for both REST `callRuntimeTool` and MCP `tools/call` with metadata-backed OAuth scope checks and session recording.
- Client profile isolation: agent enrollment files are isolated by profile under `/etc/webcodex/clients/<profile>/`.
- QUIC agent transport: experimental QUIC transport with auto-fallback to WebSocket and polling.
- Build revision metadata: `runtime_status` reports `git_commit`, `git_dirty`, and `built_at` for server and CLI.
- Shell profile support: prepared environment snapshots for Rust/Cargo, Python venv, and Conda project development.
- Chinese (zh-CN) documentation for most guides.

## New tools and runtime capabilities

### Structured line edit tools

Preferred for scoped source edits when line numbers are known:

- `replace_line_range` — replace a range of lines in a file.
- `insert_at_line` — insert content at a specific line.
- `delete_line_range` — delete a range of lines.

These tools go through the agent dispatch path and respect project boundaries.

### show_changes

A read-only project inspection tool that summarizes:

- Branch, HEAD, and commit state.
- Modified, added, deleted, renamed, and untracked files.
- `git diff --stat` and optional bounded hunks.
- Simple warnings for untracked smoke/tmp/test/anchor files.
- Optional session activity summary when `session_id` is provided.
- Suggested next actions.

Requires `project:read`. Never modifies, cleans, stages, commits, or restores the worktree.

### Session tracking foundation

- `start_session` creates a bounded in-memory session recorder with a `wc_sess_*` id.
- `session_summary` returns recorded tool calls, success/failure status, project ids, inferred write-like paths, and returned job ids.
- `show_changes` accepts `session_id` to include session activity alongside git state.
- Sessions are in-memory and bounded; server restart loses session data.

### ToolMetadata and ToolKernel

`ToolMetadata` centralizes per-tool facts:

- Risk class (`ReadOnly`, `ProjectWrite`, `JobRun`, `AccountManage`).
- OAuth scope requirement.
- Read-only, destructive, and open-world hints.
- Project requirement and path hints.

`ToolKernel` is a lightweight facade used by both REST `callRuntimeTool` and MCP `tools/call`. It performs:

- Metadata-backed OAuth scope checks before dispatch.
- Session event recording (start and finish events).
- `ToolCall` parsing and dispatch to existing `ToolRuntime` handlers.

This is preparation for a later provider system. It does not change runtime dispatch behavior, OAuth grant management, or the existing tool API. No external MCP provider system is included yet.

## GPT Action and MCP improvements

- OAuth2 authorization code flow: clients can obtain delegated `wc_oat_*` access tokens via browser consent or direct Bearer issuance.
- OAuth2 client management API: create, list, and revoke OAuth clients with `allowed_scopes`.
- Protected resource metadata at `/.well-known/oauth-protected-resource`.
- Authorization server metadata at `/.well-known/oauth-authorization-server`.
- Route scope policy enforcement: OAuth tokens are checked against per-route scope requirements.
- Refresh token rotation with one-time-use enforcement.
- Token revocation endpoint.
- `ToolKernel` facade ensures consistent OAuth scope checks across REST and MCP.
- Tool annotations in `tools/list` responses derived from `ToolMetadata`.

## Session-aware workflow

The recommended session workflow for GPT Actions and MCP:

1. `start_session` — create a session and keep the `wc_sess_*` id.
2. `list_projects` / `runtime_status` — discover projects and verify agent health.
3. `read_file` / `search_project_text` / `list_project_files` — inspect before editing.
4. `replace_line_range` / `insert_at_line` / `delete_line_range` — preferred structured edits.
5. `cargo_fmt` / `cargo_check` / `cargo_test` / `run_shell` — validate changes.
6. `show_changes` with `session_id` — review worktree state and session activity.
7. `session_summary` — inspect recorded tool calls.

**REST / GPT Action session id semantics:**

- Top-level `session_id` in the request body is recorder metadata for the current call.
- `params.session_id` is the `show_changes` or `session_summary` business argument that selects which session to summarize.
- The two ids may be the same or different.

**MCP session id semantics:**

- `_session_id` in `arguments` is reserved recorder metadata. WebCodex strips it before dispatch.
- `session_id` in `arguments` is the business parameter for `show_changes` or `session_summary`.

## Deployment and operations changes

- Client profile isolation: `webcodex-cli client enroll --profile <name>` stores agent config, tokens, and project files under `/etc/webcodex/clients/<profile>/`.
- `webcodex-cli agent install-service --profile <name>` installs a profile-specific systemd unit.
- `webcodex-cli doctor --profile <name>` validates a profile-specific agent setup.
- `webcodex-cli server install-service` installs the server systemd unit.
- Build revision mismatch detection: `webcodex-cli` warns when the CLI and server binaries report different git commits.
- Artifact chunked content reads: `read_project_artifact` supports `offset`/`length` for bounded binary reads.
- Conversation file import: `importConversationFilesToProject` for ChatGPT file-passing (images, uploads, Code Interpreter outputs).

## Known issues

- **4 import HTTP tests are currently ignored.** These tests exercise HTTP redirect and import safety behavior and exhibit flaky or full-suite interaction behavior. They should be stabilized in a later hardening pass. The full test suite otherwise passes with 0 failures.
- **npm package version is not updated.** The `@yyjeqhc/webcodex` npm package remains at `0.1.0`. npm publish is out of scope for this release.
- **npm manifest.json points to v0.1.0 artifact URLs.** The manifest will be updated when v0.2.0 binaries are built and published.
- **Sessions are in-memory only.** Server restart loses all session data. Persistent session storage is future work.
- **ToolKernel is a facade, not a full provider system.** Concrete tool handlers and schemas remain in `ToolRuntime`. External MCP provider registration is not implemented.
- **No external MCP host/provider system.** The current MCP endpoint exposes WebCodex runtime tools only.
- **Dynamic client registration, OIDC, JWKS/JWT, `client_credentials` grant, and device code flow are not implemented.**

## Validation checklist

- [x] `cargo fmt` — clean
- [x] `cargo fmt --check` — passes
- [x] `cargo check --all-targets` — passes
- [x] `cargo test --bin webcodex metadata` — passes
- [x] `cargo test --bin webcodex kernel` — passes
- [x] `cargo test --bin webcodex session` — passes
- [x] `cargo test --bin webcodex show_changes` — passes
- [x] `cargo test --bin webcodex oauth_route_policy` — passes
- [x] `cargo test --bin webcodex mcp` — passes
- [x] `cargo test --bin webcodex openapi` — passes
- [x] `git diff --check` — clean
- [x] Full test suite: 0 failures, 4 ignored (import HTTP flaky tests)

## Commits since v0.1.0

This release includes 79 commits from `v0.1.0` to `main` HEAD, covering OAuth2, structured edits, session tracking, tool metadata, transport improvements, documentation, and operational tooling.
