# LSP Navigation

Read-only semantic navigation for agent-backed projects, powered by
language-specific servers under constrained per-language profiles. The tool
surface is language-agnostic; the set of supported languages comes from a
single agent-side registry (see [Supported Languages](#supported-languages)
and [Adding a Language](#adding-a-language)).

This document covers the seven runtime tools, the `start_coding_task`
`semantic_navigation` capability summary, position conventions, the security
boundary, resource lifecycle, error codes, and troubleshooting.

## Overview

WebCodex exposes seven read-only LSP tools, each operating on any supported
language:

| Tool | Purpose |
|---|---|
| `lsp_status` | Per-language server availability for a project. Never starts a server. |
| `document_symbols` | Symbols declared in one supported source file. |
| `goto_definition` | Definition location(s) for a source position. |
| `find_references` | Reference locations for a source position. |
| `document_diagnostics` | Bounded diagnostics for one source file, with explicit `fresh` / `timed_out` state. |
| `hover` | Normalized, bounded hover content for a source position. |
| `workspace_symbols` | Workspace-only symbol search with project-relative results. |

All of them flow through ToolRuntime → typed agent bridge payload →
`LspSupervisor` → the language server selected for the target file's
extension. The server never reads agent project files and never accepts
arbitrary LSP methods: the bridge payload is a closed enum of exactly these
operations, so unknown operations fail deserialization before reaching any
agent.

## Supported Languages

The agent registry (`src/bin/webcodex_agent/lsp/language.rs`) is the single
source of truth. Each entry pairs a language server with the file extensions,
project markers, and constrained read-only profile it owns.

| Language | Server | Extensions | Detected by |
|---|---|---|---|
| Rust | `rust-analyzer` | `.rs` | `Cargo.toml` |
| Python | `pyright` (`pyright-langserver`) | `.py`, `.pyi` | `pyproject.toml`, `setup.py`, `setup.cfg`, `requirements.txt`, `Pipfile`, `pyrightconfig.json` |
| TypeScript / JavaScript | `typescript-language-server` | `.ts`, `.tsx`, `.mts`, `.cts`, `.js`, `.jsx`, `.mjs`, `.cjs` | `tsconfig.json`, `jsconfig.json`, `package.json` |

A file's extension selects its server and the LSP `languageId` announced on
`didOpen` (for example `.tsx` → `typescriptreact`). Result payloads report the
language's primary label (`typescript`) for a stable vocabulary.

## Requirements

- The project must be agent-backed (`agent:<client_id>:<project_id>`), and the
  owning agent must advertise the `lsp_read_only_navigation` capability
  (agents older than this feature do not).
- The language server must be resolvable on the agent machine, in priority
  order, per language:
  1. the language's env override (`WEBCODEX_RUST_ANALYZER`, `WEBCODEX_PYRIGHT`,
     `WEBCODEX_TYPESCRIPT_LANGUAGE_SERVER`) — an explicit executable path;
  2. the server executable (`rust-analyzer`, `pyright-langserver`,
     `typescript-language-server`) on the agent's `PATH`.
  Servers resolved from the env override or `PATH` receive the profile's
  default arguments (e.g. `--stdio` for pyright and typescript-language-server).
- Language detection uses the project markers above at the registered project
  root. A language whose markers are absent reports as not detected, and
  navigation for it is not offered.

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
  converts positions in both directions; most servers default to `utf-16`.
- Result ordering is deterministic (sorted by path, then range) and
  duplicates are removed.

## Security Boundary

Starting a language server must not execute repository code. Each language
profile pins a constrained read-only `initialize` payload; the fields cannot
be overridden by environment variables and are pinned by per-language security
regression tests. These are constrained profiles, not OS sandboxes — a server
still reads workspace metadata to resolve the project.

- **rust-analyzer**: `cargo.buildScripts.enable=false` and
  `procMacro.enable=false` (no `build.rs` execution, no proc-macro loading),
  `checkOnSave=false` (no Cargo check), `cargo.noDeps=true` (external
  dependencies are not fetched or analyzed — also why dependency navigation is
  unavailable), plus `cachePriming.enable=false` and `files.watcher=server`.
- **pyright**: a pure type checker that never executes project code, so the
  code-execution boundary holds intrinsically. `diagnosticMode=openFilesOnly`
  bounds analysis, `typeCheckingMode=basic`, and `autoImportCompletions=false`.
- **typescript-language-server**: `disableAutomaticTypingAcquisition=true` is
  the network boundary — it stops tsserver from downloading `@types/*` packages
  from npm (the analog to rust's `cargo.noDeps`) — plus
  `includePackageJsonAutoImports=off`.

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
| `lsp_server_unavailable` | The language server executable could not be resolved. |
| `lsp_server_failed` | Server crashed, exited, restart budget exhausted, or capacity exceeded. |
| `lsp_request_timeout` | The LSP request exceeded its deadline (a cancel was sent). |
| `lsp_protocol_error` | Malformed or protocol-violating server traffic. |
| `invalid_project_path` | Project root failed policy or path validation (also covers escaping inputs). |
| `unsupported_language` | Target file extension is not routed to any registered language server. |
| `file_not_found` | Target file does not exist or could not be read. |
| `document_too_large` | Target file exceeds the 8 MiB document cap. |
| `invalid_position` / `invalid_arguments` | Position or argument validation failed. |
| `malformed_agent_lsp_result` | Agent result failed the typed contract or carried forbidden path material. |
| `unknown_project` | Agent-local project id did not resolve. |
| `missing_lsp_payload` | Transport request lacked the typed LSP payload. |

## Known Limitations

- Read-only: no rename, code actions, or other write-side operations, for any
  language.
- One server per language; `workspace_symbols` (which carries no file path)
  targets the first detected language. Fanning a project-scoped query across
  every detected language's server is a deliberate follow-up, not implemented.
- `document_diagnostics` is fast semantic feedback from the constrained
  read-only profile (e.g. for Rust: no Cargo check, no build scripts, no proc
  macros); it never substitutes for final language-native validation.
- No dependency navigation: definitions that resolve outside the project root
  (external crates/packages, sysroot/stdlib) are omitted and counted. This is
  enforced per profile (`cargo.noDeps=true` for Rust,
  `disableAutomaticTypingAcquisition=true` for TypeScript).
- Full-text sync only (`full_text_sync_only`): document state refreshes from
  disk per request; there is no incremental (`didChange` range) sync and no
  unsaved-buffer concept. Edits made through WebCodex tools are picked up on
  the next request because content is re-read from disk.
- `document_symbols` currently negotiates flat symbol output; nested symbol
  hierarchies may be flattened.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `lsp_status` reports a language `unavailable` | Its server is not installed or not on the agent's `PATH`; set the language's env override (`WEBCODEX_RUST_ANALYZER`, `WEBCODEX_PYRIGHT`, `WEBCODEX_TYPESCRIPT_LANGUAGE_SERVER`) to an executable path. |
| `unsupported_language` on a real source file | The file extension is not routed by any profile; the error message lists the supported extensions. |
| `status = "agent_capability_unavailable"` | Agent binary predates LSP navigation; upgrade the agent. |
| `lsp_server_failed` with capacity message | More than 4 distinct project roots navigated concurrently; idle servers are reaped after 15 minutes, or reduce concurrent projects. |
| `document_diagnostics` returns `fresh=false`, `timed_out=true` | The server did not publish fresh diagnostics within the 2 s wait (common right after a large edit or cold start); the stale/empty snapshot is a successful result — retry after a moment. |
| A language reports not detected on its own repo | No project marker at the registered root (e.g. no `Cargo.toml` / `pyproject.toml` / `tsconfig.json`, or the root points at a parent directory). |

> **Coding-startup note:** the `start_coding_task.semantic_navigation` summary
> currently reports **Rust** readiness specifically (it is a Rust-focused
> startup hint). The seven runtime tools work for every
> [supported language](#supported-languages) regardless of that summary.

## Adding a Language

The generic supervisor and navigation handlers hold no per-language knowledge,
so adding a language is additive and localized:

1. Add a variant to `LspServerKind` in `supervisor.rs` (a pure discriminant —
   it carries no behavior).
2. Add a `LanguageProfile` entry to `LANGUAGES` in `language.rs`: extensions
   (with their `languageId`), project markers, env override, executable,
   `default_args`, and a constrained read-only `initialization_options`
   function. Set `unusable_command_probe` / `startup_stderr_classifier` only if
   the server needs them.
3. Extend the `ALL_KINDS` list in the `language.rs` tests, and add a security
   regression test pinning the new profile's `initialize` options.

No changes to process management, request routing, position conversion, or the
bridge contract are required. The `real_pyright_document_symbols_end_to_end`
ignored test shows how to smoke-test a real server end to end.

## Related Docs

- [ARCHITECTURE.md](ARCHITECTURE.md) — module map, including
  `src/bin/webcodex_agent/lsp/*` and `tool_runtime::semantic_navigation`.
- [../SECURITY.md](../SECURITY.md) — overall boundary model.
- [AGENT_PROTOCOL.md](AGENT_PROTOCOL.md) — agent transport and request kinds.
- [ROADMAP.md](ROADMAP.md) — planned LSP follow-ups.
