# LSP Navigation

Read-only Rust semantic navigation for agent-backed projects, powered by an
agent-side rust-analyzer under a constrained profile.

This document covers the seven runtime tools, the `start_coding_task`
`semantic_navigation` capability summary, position conventions, the security
boundary, resource lifecycle, error codes, and troubleshooting.

## Overview

WebCodex exposes seven read-only LSP tools:

| Tool | Purpose |
|---|---|
| `lsp_status` | Language/server availability for a project. Never starts a server. |
| `document_symbols` | Symbols declared in one `.rs` file. |
| `goto_definition` | Definition location(s) for a source position. |
| `find_references` | Reference locations for a source position. |
| `document_diagnostics` | Bounded rust-analyzer diagnostics for one `.rs` file, with explicit `fresh` / `timed_out` state. |
| `hover` | Normalized, bounded hover content for a source position. |
| `workspace_symbols` | Workspace-only symbol search with project-relative results. |

All of them flow through ToolRuntime → typed agent bridge payload →
`LspSupervisor` → rust-analyzer. The server never reads agent project files
and never accepts arbitrary LSP methods: the bridge payload is a closed enum
of exactly these operations, so unknown operations fail deserialization
before reaching any agent.

## Requirements

- The project must be agent-backed (`agent:<client_id>:<project_id>`), and the
  owning agent must advertise the `lsp_read_only_navigation` capability
  (agents older than this feature do not).
- rust-analyzer must be resolvable on the agent machine, in priority order:
  1. `WEBCODEX_RUST_ANALYZER` environment variable (explicit executable path);
  2. `rust-analyzer` on the agent's `PATH`.
- Rust detection is `Cargo.toml` at the registered project root. Projects
  without it report `rust` as not detected and navigation is not offered.

## Coding-Startup Capability Summary

`start_coding_task` always returns a compact `semantic_navigation` object so
a coding loop can decide up front whether to prefer LSP tools over text
search. The summary is produced by a bounded status-only probe:

- One typed `Status` request under a single two-second deadline covering both
  enqueue and response. On timeout the pending waiter is cancelled.
- The probe never starts rust-analyzer, never runs Cargo or shell commands,
  and never returns symbol/location data.
- Probe failure, timeout, agent disconnection, or a legacy agent never blocks
  startup and never changes the startup verdict or warnings.

Fields:

| Field | Meaning |
|---|---|
| `supported` | Project is agent-backed, agent connected, capability advertised. |
| `available` | An executable is available or a server slot is running/initializing. A crashed slot stays available only while the agent still reports the executable as available. |
| `recommended` | `true` only for `running` or `available` status. |
| `status` | `running`, `available`, `initializing`, `crashed`, `unavailable`, `not_applicable`, `agent_unavailable`, `agent_capability_unavailable`, `probe_timeout`, `probe_failed`. |
| `position_encoding` | Negotiated encoding, only surfaced for a running server. |
| `tools` / `preferred_flow` | Tool names and the suggested inspect order (`preferred_flow` is empty unless recommended). |
| `limitations` | `rust_only`, `read_only`, `workspace_only`, `no_dependency_navigation`, `full_text_sync_only`. |
| `reason_code` | Machine-readable cause when navigation is degraded or off, e.g. `rust_not_detected`, `agent_not_connected`, `server_crashed`, `status_probe_timed_out`. |

## Position And Path Conventions

- Input and output paths are project-relative with `/` separators. Absolute
  paths, `..` traversal, symlinks escaping the project, and non-`.rs` files
  are rejected before any server interaction.
- Lines are 1-based. Columns are 1-based **Unicode scalar** offsets (not
  UTF-16 code units, not bytes). The end-of-line caret position
  `scalar_count + 1` is valid input.
- The agent negotiates `utf-8`, `utf-16`, or `utf-32` with the server and
  converts positions in both directions; rust-analyzer defaults to `utf-16`.
- Result ordering is deterministic (sorted by path, then range) and
  duplicates are removed.

## Security Boundary

Starting the language server must not execute repository code. The agent
sends a pinned, constrained rust-analyzer profile in `initialize`:

- `cargo.buildScripts.enable=false` and `procMacro.enable=false` — no
  `build.rs` execution, no proc-macro loading.
- `checkOnSave=false` — no Cargo check runs.
- `cargo.noDeps=true` — external dependencies are not fetched or analyzed
  (this is also why dependency navigation is unavailable).
- `cachePriming.enable=false`, `files.watcher=server`.

These fields cannot be overridden by environment variables and are pinned by
a security regression test. This is a constrained profile, not an OS sandbox;
rust-analyzer still reads workspace metadata (`cargo metadata`).

Result boundaries:

- Locations outside the canonical project root (registry crates, sysroot,
  anything reached through symlinks) are omitted and counted in
  `external_results_omitted`; they never appear as paths.
- Results never contain absolute paths or `file://` URIs. The server
  re-validates returned payloads and rejects any that carry path material.
- Error messages are redacted (`file:` URIs, absolute POSIX/Windows paths,
  and UNC prefixes become `<path>`), stripped of control characters, and
  truncated to 240 characters.
- `lsp_status` reports availability and source kind (`configured`,
  `environment`, `path`) but never absolute executable paths.

## Resource Lifecycle And Limits

| Limit | Value |
|---|---|
| Servers per project / per agent | 1 / 4 |
| Idle TTL | 15 minutes, enforced by a background reaper thread |
| Request / initialize / shutdown timeouts | 10 s / 15 s / 2 s |
| Startup probe deadline | 2 s (single deadline, server-side) |
| Diagnostics publication wait | 2 s after document sync; on expiry the result is a successful stale/empty snapshot with `fresh=false`, `timed_out=true` |
| Diagnostics cache | 256 documents per server, 500 diagnostics per document |
| Diagnostic text truncation | message 4096 / source 128 / code 128 characters, 64 KiB total text per response |
| Hover content truncation | 16 KiB (characters) |
| Workspace symbol field truncation | 256 characters per field |
| LSP message cap (both directions) | 8 MiB |
| Document size cap | 8 MiB, checked before reading (`document_too_large`) |
| stderr capture | last 64 KiB |
| Symbol name / detail truncation | 256 / 512 characters |
| Result limits (`limit` input) | `document_symbols` ≤ 500, `goto_definition` ≤ 100, `find_references` ≤ 200, `document_diagnostics` ≤ 200, `workspace_symbols` ≤ 200 |

Servers start lazily on the first navigation request per project, restart at
most once per request on recoverable failures, and are reaped when idle past
the TTL or when their connection becomes unusable, freeing capacity without
an agent restart.

Document synchronization is disk-backed full-text sync: on every request that
targets a file, the agent re-reads the file from disk and sends `didOpen` (or
`didChange` when the content fingerprint changed) with the full text. Models
never supply document text, and there is no editor-style incremental sync.

## Error Codes

| Code | Meaning |
|---|---|
| `agent_capability_unavailable` | Project is not agent-backed, or the agent does not advertise LSP navigation. |
| `lsp_server_unavailable` | rust-analyzer executable could not be resolved. |
| `lsp_server_failed` | Server crashed, exited, restart budget exhausted, or capacity exceeded. |
| `lsp_request_timeout` | The LSP request exceeded its deadline (a cancel was sent). |
| `lsp_protocol_error` | Malformed or protocol-violating server traffic. |
| `invalid_project_path` | Project root failed policy or path validation (also covers escaping inputs). |
| `unsupported_language` | Target file is not a `.rs` file. |
| `file_not_found` | Target file does not exist or could not be read. |
| `document_too_large` | Target file exceeds the 8 MiB document cap. |
| `invalid_position` / `invalid_arguments` | Position or argument validation failed. |
| `malformed_agent_lsp_result` | Agent result failed the typed contract or carried forbidden path material. |
| `unknown_project` | Agent-local project id did not resolve. |
| `missing_lsp_payload` | Transport request lacked the typed LSP payload. |

## Known Limitations

- Rust only; one server kind (rust-analyzer).
- Read-only: no rename, code actions, or other write-side operations.
- `document_diagnostics` is fast semantic feedback from the constrained
  rust-analyzer profile (no Cargo check, no build scripts, no proc macros);
  it never substitutes for final Cargo validation.
- No dependency navigation (`cargo.noDeps=true`): definitions that resolve
  into external crates are omitted and counted.
- Full-text sync only (`full_text_sync_only`): document state refreshes from
  disk per request; there is no incremental (`didChange` range) sync and no
  unsaved-buffer concept. Edits made through WebCodex tools are picked up on
  the next request because content is re-read from disk.
- `document_symbols` currently negotiates flat symbol output; nested symbol
  hierarchies may be flattened.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `semantic_navigation.status = "unavailable"` | rust-analyzer not installed or not on the agent's `PATH`; set `WEBCODEX_RUST_ANALYZER` to an executable path. |
| `status = "agent_capability_unavailable"` | Agent binary predates LSP navigation; upgrade the agent. |
| `status = "probe_timeout"` on every startup | Agent transport is congested or the agent host is overloaded; navigation tools may still work with their longer 30 s budget. |
| `lsp_server_failed` with capacity message | More than 4 distinct project roots navigated concurrently; idle servers are reaped after 15 minutes, or reduce concurrent projects. |
| `document_diagnostics` returns `fresh=false`, `timed_out=true` | rust-analyzer did not publish fresh diagnostics within the 2 s wait (common right after a large edit or cold start); the stale/empty snapshot is a successful result — retry after a moment. |
| `rust_not_detected` on a Rust repo | No `Cargo.toml` at the registered project root (e.g. the root points at a parent directory). |

## Related Docs

- [ARCHITECTURE.md](ARCHITECTURE.md) — module map, including
  `src/bin/webcodex_agent/lsp/*` and `tool_runtime::semantic_navigation`.
- [../SECURITY.md](../SECURITY.md) — overall boundary model.
- [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) — agent transport and request kinds.
- [ROADMAP.md](ROADMAP.md) — planned LSP follow-ups.
