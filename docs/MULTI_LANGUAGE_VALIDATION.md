# Multi-Language Validation — Design

Status: **in progress** (foundation implemented; public multi-language tools not
shipped)

This document proposes extending WebCodex validation from Rust-only
(`cargo_check` / `cargo_test` / `cargo_fmt`) to a language-agnostic validation
surface, mirroring the multi-language LSP registry
([LSP_NAVIGATION.md](LSP_NAVIGATION.md), `lsp/language.rs`). After the LSP work,
the chat window can *navigate* Python and TypeScript but can only *validate*
Rust; this closes that asymmetry.

## 1. Goals & Non-Goals

**Goals**
- One `ValidationProfile` registry: adding a language = one profile plus its
  output parser and tests, with no changes to the generic run/evidence paths.
- Reuse the existing evidence model end to end: bounded output → session ledger
  → `validation_parser` → `validation_events` → `validation_summary` /
  `finish_coding_task` / `session_handoff_summary`. New languages produce the
  same `ValidationDiagnostics` shape, so downstream aggregation is unchanged.
- Read-only, bounded, secret-safe, and deterministic — identical guarantees to
  the Rust path and the LSP profiles.

**Non-Goals (this iteration)**
- No autofix / write-side actions (formatters run in `--check` mode only).
- No dependency installation or network fetch (a project is validated as it
  is on disk; missing toolchains are reported, not installed).
- No migration of `cargo_*` onto the generic surface (see §4 decision D1).
- No change to verdict semantics in `validation_summary` /
  `validation_events` (mixed/passed/failed, historical failures) — only new
  event sources feeding the same aggregation.
- **WebCodex does not yet product-support Python validation.** An internal
  Pyright adapter/parser exists to prove the agent-side bridge; there is no
  public `typecheck` tool and no MCP/OpenAPI exposure for Python validation.

## 2. Current Architecture (recap)

```
cargo_check/test/fmt (tool_runtime/cargo.rs)
  -> run_project_command_capture  (bounded stdout/stderr tail, exit code)
  -> ToolResult payload + session ledger event (validation_output_summary)
        |
        v
validation_events.rs
  - validation_kind_for_tool(tool_name) -> "check"/"test"/"format"/...   (allowlist)
  - validation_diagnostics_from_summary(finished) -> re-parse bounded excerpt
        by tool_name via validation_parser.rs
  - validation_failure_kind -> compile_error | test_failure | format_diff
        | timeout | process_exit | unknown
        |
        v
validation_summary  (status: passed|failed|mixed, latest, historical_failures)
  consumed by validation_summary tool, finish_coding_task, session_handoff_summary
```

`validation_parser.rs` is Rust-specific: it scrapes rustc text (`error[E0308]:`,
`--> file:line:col`, `test result:`, `thread '…' panicked at`, `… FAILED`).
Its **output types are already language-neutral**: `ValidationDiagnostics`
(`CargoDiagnostic { severity, code, file, line, column, message }`,
`FailedTestDetail { name, failure_kind, file, line, column }`,
`CargoTestSummary { passed, failed, ignored }`). We keep these types and add
new *producers*. (Their `Cargo*` names become misnomers under multi-language;
a rename to `ValidationDiagnostic` / `TestSummary` is a small follow-up
cleanup, deliberately out of scope for the parser work.)

### 2.1 Where paths are relativized — the load-bearing difference

`cargo` runs on the **agent**; `run_project_command_capture` returns bounded
stdout/stderr to the **server**, and `cargo.rs` parses it there. This works
because **rustc emits workspace-relative paths** (`src/main.rs:3:1`), so the
server-side parser never needs the agent's filesystem root and the ledger
never holds an absolute path.

Structured tools break that assumption: **pyright, ruff, and eslint emit
absolute paths** (`/home/…/proj/src/app.py`). Those can only be made
project-relative where the canonical project root is known — **on the agent**,
exactly as `lsp/navigation.rs` already relativizes LSP locations. Server-side
re-parsing cannot do it (the server only has the project *id*, never the
agent's secret filesystem path), and storing absolute paths in the ledger
would violate the no-absolute-path invariant.

**Consequence:** structured, absolute-path validation is not a pure
server-side command runner like `cargo_*`. It needs an **agent-side adapter**
that runs the tool, relativizes + validates + bounds the output into the
shared diagnostics shape, and returns that already-safe structure
across the bridge — the same shape the LSP tools already use. See D5
(**accepted**).

Key safety invariants already enforced and to be preserved verbatim:
- Only **bounded** validation-output metadata is parsed — never full bodies.
- File paths must be **project-relative** and non-escaping
  (`sanitize_file_path` rejects absolute/UNC/`..`/`file:`); secrets are
  scrubbed (`looks_sensitive`); messages/names/codes are length-bounded.
- Diagnostics capped (20), failed tests capped (20), deterministic sort+dedup.

### 2.2 Pre-execution rejections are not validation evidence

Local parameter validation failures (for example synchronous `timeout_secs`
outside `1..=120`), schema rejections, permission denials, session guard
denials, and other command-not-started outcomes:

- **must not** be recorded as `cargo_check` / `cargo_test` / `cargo_fmt`
  validation execution failures;
- **must not** make `validation_summary` report `mixed` / `failed`;
- **may** remain ordinary tool failure / session events for audit;
- are filtered at validation-event generation when there is no execution
  evidence (`exit_code` and `validation_output_summary` both absent).

Only calls that actually entered validation execution form validation ledger
evidence.

### 2.3 Synchronous timeout contract (cargo_* / run_shell)

Shared public contract for synchronous agent-wait tools:

| Field | Rule |
|---|---|
| `timeout_secs` minimum | 1 |
| `timeout_secs` maximum | 120 |
| Out of range | **reject** with `failure_kind=invalid_arguments` before enqueue (no silent clamp) |
| Error text | names the calling tool; never leaks `runShell` / underlying shell request details |
| Defaults | `cargo_*` → 120; `run_shell` → 60 |

`search_project_text` keeps its own clamp-to-1..120 semantics and is unchanged.
`run_job` remains the path for longer-running work.

## 3. `ValidationProfile` registry

Internal module `tool_runtime/validation_profile/` (Rust profile implemented):

```rust
struct ValidationProfile {
    language: &'static str,              // "python", "typescript", "rust"
    manifest_markers: &'static [&'static str],   // reuse LSP detection set
    kinds: &'static [ValidationAdapter],
}

struct ValidationAdapter {
    kind: ValidationKind,                // Typecheck | Lint | Test | Format
    tool_name: &'static str,             // public event name, e.g. "pyright"
    executable: &'static str,            // "pyright", "ruff", "pytest", "tsc"
    env_override: &'static str,          // "WEBCODEX_PYRIGHT", ...
    // Build the argv for a validated cwd/target; prefers structured output.
    build_command: fn(&ValidationRequest) -> Vec<String>,
    // Deterministic, bounded parse of the captured excerpt -> shared model.
    parse: fn(&CapturedOutput) -> ValidationDiagnostics,
    // How a non-zero exit maps to failure_kind when the parser is silent.
    default_failure_kind: FailureKind,
}
```

Design notes:
- **Prefer structured output.** Unlike rustc text, the new tools emit
  machine-readable output, so parsers are simpler and more robust:
  | Tool | Command | Format |
  |---|---|---|
  | pyright | `pyright --outputjson` | JSON: `summary`, `generalDiagnostics[]{file,severity,message,range,rule}` |
  | ruff | `ruff check --output-format json` | JSON: `[]{filename,location{row,column},code,message}` |
  | pytest | `pytest -q --junit-xml=<tmp>` (or text fallback) | JUnit XML `<testcase>`/`<failure>`, or `FAILED path::name` + `= N failed, M passed =` |
  | tsc | `tsc --noEmit --pretty false` | text: `file(line,col): error TSxxxx: message` (stable; tsc has no JSON) |
  | eslint | `eslint --format json` | JSON: `[]{filePath,messages[]{line,column,ruleId,severity,message}}` |
  | black | `black --check --quiet` | exit code + `would reformat <file>` lines |
  | prettier | `prettier --check` | exit code + file list |
- Absolute paths in structured output (pyright emits absolute `file`) are made
  **project-relative and re-validated** on the agent; anything escaping the
  project is dropped (counted as `external_results_omitted` /
  `invalid_diagnostics_omitted`), exactly like the LSP `external_results`
  handling.
- Positions from LSP-style tools are **0-based**; normalize to the 1-based
  convention used across WebCodex results.
- Structured parsers still pass every string field through the existing
  length/safety guards and the 20/20 caps.

### 3.1 Agent-side validation bridge (implemented)

Shared contract: `src/validation_bridge.rs`

- **Protocol version:** `VALIDATION_BRIDGE_PROTOCOL_VERSION = 1`
- **Result format:** `webcodex.validation_bridge_result.v1`
- **Agent request kind:** `validation` (never falls through to shell)
- **Request:** declarative `ValidationBridgeRequest` (`adapter_id`, `language`,
  `validation_kind`, agent-local `project_id`, project-relative `cwd`/`targets`,
  `timeout_secs` in `1..=120`). **No shell command strings.**
- **Response:** sanitized `ValidationBridgeResponse` with
  `command_started`, `exit_code`, `failure_kind`, bounded
  `BridgeDiagnostics` (project-relative paths only). **No raw Pyright JSON,
  no absolute paths, no unbounded stdout/stderr.**

Agent modules: `src/bin/webcodex_agent/validation/`

| Concern | Location |
|---|---|
| Adapter registry (metadata) | `registry.rs` — stable `adapter_id`s |
| Execution + path sanitization | `execute.rs`, `path.rs` |
| Pyright adapter + parser | `pyright.rs` (builds `pyright --outputjson` locally) |
| Dispatch entry | `mod.rs` + `dispatch.rs` kind branch |

Separation of concerns:

| Layer | Role |
|---|---|
| `ValidationProfile` (server) | Language/capability metadata for future public tools |
| Agent adapter registry | Executable discovery, argv, parse, sanitize |
| Bridge protocol | Cross server↔agent data contract |

Server must not hold parser function pointers. Adapter ids are shared strings;
consistency is enforced by tests.

### 3.2 Complete JSON capture policy (Pyright)

- **Hard byte cap:** `MAX_VALIDATION_STDOUT_BYTES` = 2 MiB.
- Capture must be **complete JSON only**. If the cap is exceeded:
  - return `failure_kind=output_too_large` (also documented as
    `validation_output_oversized` synonym intent — wire kind is
    `output_too_large`);
  - **do not** tail-truncate and attempt parse;
  - **do not** parse incomplete JSON.
- Stderr is at most a short summary across the bridge (or omitted).
- Pass/fail uses structured diagnostics / summary (`errorCount` /
  error-severity diagnostics), not exit code alone.

## 4. Tool surface — decisions

- **D1 — Accepted: generic per-kind tools (future public surface).** Add
  `typecheck`, `run_tests`, `lint`, `format_check`, each taking
  `{ project, language?, cwd?, … }`. `language` is optional and auto-detected
  from project markers when omitted. They serve Python and TypeScript;
  **Rust keeps `cargo_*`** and generic tools reject `language="rust"` with a
  pointer to `cargo_check`, so there is exactly one canonical Rust entry
  (no dual surface — AGENTS.md §14). All new tools are `callRuntimeTool`-only
  (0 new GPT Actions; budget stays 25/30).
  - **Current status:** public tools **not implemented**.
- **D2 — Open:** auto-detect language with explicit override +
  `language_required` for ambiguous roots (recommended).
- **D3 — Open:** pytest evidence source (JUnit XML vs text).
- **D4 — Open:** lint severity policy for finish verdicts.
- **D5 — Accepted: agent-side adapter for absolute-path tools.** Execution,
  parse, path relativization, and sanitization happen on the agent. The bridge
  carries only declarative requests and already-safe diagnostics.

## 5. Evidence-flow integration

There are two evidence-capture paths, chosen per adapter:

- **Server-side text re-parse (existing, relative-path tools).** Used by
  `cargo_*`: the tool stores bounded stdout/stderr tails on the ledger event
  (`validation_output_summary`), and `validation_diagnostics_from_summary`
  re-parses them by tool name.
- **Agent-side structured capture (new, absolute-path tools).** Used by
  pyright (internal): the agent-side adapter runs the tool, relativizes and
  sanitizes into bridge diagnostics, and returns that structure. Future public
  tools will store the already-safe structure on the session event.

## 6. Security & safety

Unchanged from the Rust path and enforced by shared helpers:
- Only bounded excerpts / bridge diagnostics stored; no raw stdout/stderr
  bodies returned to the model for structured adapters.
- Paths project-relative + non-escaping; fields length-capped; 20 diagnostic
  cap; deterministic ordering.
- Read-only: formatters run in check mode; no writes, no autofix, no installs.
- No network: `--outputjson`/`--check` are local.
- Missing toolchain → clean `tool_unavailable` (like LSP
  `lsp_server_unavailable`), never an install attempt.

## 7. AGENTS.md §7 synchronization checklist (per new public tool)

Adding each generic tool must update, in one change:
1. `ToolCall` enum + parser
2. `KNOWN_TOOL_NAMES`
3. tool metadata
4. registry input/output schema
5. OAuth runtime tool policy
6. OpenAPI accepted names — **`callRuntimeTool` only**
7. MCP schema tests
8. consistency tests

Plus: `validation_kind_for_tool`, diagnostics readers, and runtime tool count
references. **Not done yet** — no public multi-language tools in this phase.

## 8. Implementation status

| Item | Status |
|---|---|
| Rust `ValidationProfile` foundation | **implemented** |
| Agent-side validation bridge foundation | **implemented** |
| Internal Pyright adapter/parser | **implemented** (fixture/fake-exec tests) |
| Public `typecheck` / `run_tests` / `lint` / `format_check` | **not implemented** |
| Python LSP | **not implemented** (existing multi-language *navigation* LSP is separate) |
| Product claim "Python validation supported" | **false / not claimed** |
| `cargo_*` migration onto bridge | **not done** (intentionally unchanged) |

## 9. Phasing

- **Phase 0 — Foundation (partially done).** Rust ValidationProfile + agent-side
  bridge + internal Pyright adapter/parser with fixture/fake-executable tests.
  No public surface.
- **Phase 1 — Python tools.** Public `typecheck` (pyright) + `run_tests`
  (pytest) through §7 points; evidence into `validation_summary`.
- **Phase 2 — Python lint/format + TypeScript.**
- **Phase 3 — Startup awareness** of which languages can be validated.

## 10. Testing strategy

- **Parser unit tests** over fixtures (pyright JSON with absolute paths →
  relativized).
- **Fake-command routing tests:** stub `pyright` on `PATH` emitting canned JSON.
- **Bridge security tests:** absolute path leak checks, traversal reject,
  oversized output, malformed JSON, adapter/language mismatch.
- **No real Pyright install required** for CI unit tests.
- **Real-tool ignored smoke tests** (future): `real_pyright_typecheck_*`.

## 11. Open decisions (remaining)

- **D2** — auto-detect language with explicit override + `language_required`
  error for ambiguous roots (recommended).
- **D3** — pytest evidence source: JUnit XML vs text scrape.
- **D4** — lint severity policy for finish verdicts.

## Related

- [LSP_NAVIGATION.md](LSP_NAVIGATION.md) — the multi-language registry this
  mirrors, including "Adding a Language".
- [ARCHITECTURE.md](ARCHITECTURE.md) — validation evidence path.
- [ROADMAP.md](ROADMAP.md) — "Richer multi-language validation adapters".
