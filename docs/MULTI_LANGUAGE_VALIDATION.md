# Multi-Language Validation — Design

Status: **proposed** (design; implementation phased below)

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
shared `ValidationDiagnostics` shape, and returns that already-safe structure
across the bridge — the same shape the LSP tools already use. See D5.

Key safety invariants already enforced and to be preserved verbatim:
- Only **bounded** validation-output metadata is parsed — never full bodies.
- File paths must be **project-relative** and non-escaping
  (`sanitize_file_path` rejects absolute/UNC/`..`/`file:`); secrets are
  scrubbed (`looks_sensitive`); messages/names/codes are length-bounded.
- Diagnostics capped (20), failed tests capped (20), deterministic sort+dedup.

## 3. `ValidationProfile` registry

New module `tool_runtime/validation/registry.rs` (name TBD), analogous to
`lsp/language.rs`:

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
  **project-relative and re-validated** through the existing
  `sanitize_file_path` gate; anything escaping the project is dropped (counted
  as `invalid_diagnostics_omitted`), exactly like the LSP `external_results`
  handling.
- Positions from LSP-style tools are **0-based**; normalize to the 1-based
  convention used across WebCodex results.
- Structured parsers still pass every string field through the existing
  `sanitize_bounded_value` / `looks_sensitive` guards and the 20/20 caps.

## 4. Tool surface — the load-bearing decision

The registry is settled; the open decision is how models invoke it. Options:

- **D1a — Generic per-kind tools (recommended).** Add
  `typecheck`, `run_tests`, `lint`, `format_check`, each taking
  `{ project, language?, cwd?, … }`. `language` is optional and auto-detected
  from project markers when omitted. They serve Python and TypeScript now;
  **Rust keeps `cargo_*`** (its battle-tested, arg-rich adapter) and the
  generic tools reject `language="rust"` with a pointer to `cargo_check`, so
  there is exactly one canonical Rust entry (no dual surface — AGENTS.md §14).
  All new tools are `callRuntimeTool`-only (0 new GPT Actions; budget stays
  25/30). *Pro:* consistent per-kind shape, small surface, additive. *Con:*
  Rust is a documented exception until a future migration.
- **D1b — One `validate(project, kind, language?, …)` tool.** Single tool,
  `kind ∈ {typecheck,lint,test,format}`. *Pro:* smallest surface (1 tool).
  *Con:* collapses distinct operations behind a mode param; less discoverable;
  diverges from the established per-kind `cargo_*`/`validation_kind` pattern.
- **D1c — Explicit per-language tools** (`python_typecheck`, `ts_test`, …).
  *Pro:* most discoverable. *Con:* 3 languages × 3–4 kinds ≈ 9–12 tools;
  worst fit for the tool budget and the registry philosophy.

**Recommendation: D1a.** It matches the existing per-kind vocabulary, keeps the
surface small and additive, respects the GPT Action budget, and leaves a clean
future path to fold Rust in. Phase 1 ships only `typecheck` + `run_tests`
(Python); `lint`/`format_check` and TypeScript follow.

Second decision:
- **D2 — Auto-detect vs explicit `language`.** Recommended: optional
  `language`, auto-detected from the same manifest markers the LSP registry
  uses; ambiguous/polyglot roots require an explicit `language` and otherwise
  return a clear `language_required` error (no silent guess).

## 5. Evidence-flow integration

There are two evidence-capture paths, chosen per adapter:

- **Server-side text re-parse (existing, relative-path tools).** Used by
  `cargo_*`: the tool stores bounded stdout/stderr tails on the ledger event
  (`validation_output_summary`), and `validation_diagnostics_from_summary`
  re-parses them by tool name. Any structured tool that emits relative paths
  (e.g. `tsc` with a project-relative invocation) can reuse this path.
- **Agent-side structured capture (new, absolute-path tools).** Used by
  pyright/ruff/eslint: the agent-side adapter runs the tool, relativizes and
  sanitizes into `ValidationDiagnostics` (§2.1), and returns that structure.
  The server stores the **already-safe structured diagnostics** on the event
  rather than raw text, so no server-side re-parse and no root are needed.
  `validation_diagnostics_from_summary` reads the stored structure directly
  for these tools.

Additive changes, both paths:
- `validation_kind_for_tool` gains the new tool names (mapping to `check` for
  typecheck, `test`, and new `lint`/`format` kinds). Reuse existing kinds where
  they map cleanly so `validation_summary` status logic is unchanged.
- `validation_failure_kind` gains `lint_error` (new) and reuses
  `compile_error` (typecheck errors), `test_failure`, `format_diff`,
  `timeout`, `process_exit`.
- The ledger event records the `language` + adapter tool name and which capture
  path produced it, so downstream picks the right reader.

`validation_summary`, `finish_coding_task`, and `session_handoff_summary`
consume the same aggregated shape and need **no changes** beyond surfacing
`language` per event (already free-form).

## 6. Security & safety

Unchanged from the Rust path and enforced by shared helpers:
- Only bounded excerpts parsed; no raw stdout/stderr bodies stored or returned.
- Paths project-relative + non-escaping; secrets scrubbed; fields length-capped;
  20/20 result caps; deterministic ordering.
- Read-only: formatters run in check mode; no writes, no autofix, no installs.
- No network: `--outputjson`/`--check` are local; pyright/tsserver never
  execute project code; pytest **does** import project code to collect tests
  (documented limitation — same trust level as running `cargo test`, and only
  under an agent-backed project the operator already trusts to run shells).
- Missing toolchain → a clean `tool_unavailable` result (like LSP
  `lsp_server_unavailable`), never an install attempt.

## 7. AGENTS.md §7 synchronization checklist (per new tool)

Adding each generic tool must update, in one change:
1. `ToolCall` enum + parser
2. `KNOWN_TOOL_NAMES`
3. tool metadata
4. registry input/output schema (`registry/tool_specs`, `input_schemas`,
   `output_schemas`)
5. OAuth runtime tool policy (scope: `runtime`/`project`; read-only)
6. OpenAPI accepted names/examples — **`callRuntimeTool` only, no dedicated
   Action** (keep ≤30; currently 25)
7. MCP schema tests
8. consistency tests

Plus: `validation_kind_for_tool`, `validation_diagnostics_from_summary`, and
the runtime tool count references.

## 8. Phasing

- **Phase 0 — Foundation (no public surface).** `ValidationProfile` registry +
  Python parsers (pyright JSON, pytest) producing `ValidationDiagnostics`, with
  unit tests over captured real-tool fixtures. Decision-independent; de-risks
  the parsing, which is the hard part.
- **Phase 1 — Python tools.** `typecheck` (pyright) + `run_tests` (pytest)
  wired through all §7 points; evidence flows into `validation_summary`.
  Validate end-to-end against real pyright/pytest (ignored smoke tests, like
  `real_pyright_document_symbols_end_to_end`).
- **Phase 2 — Python lint/format + TypeScript.** `lint` (ruff/eslint),
  `format_check` (black/prettier), and the TS adapters (`tsc`, `vitest`/`jest`).
- **Phase 3 — Startup awareness.** Extend the coding-startup capability summary
  so the chat window knows which languages it can *validate* (parallels the
  Rust-only `semantic_navigation` summary note).

## 9. Testing strategy

- **Parser unit tests** over real captured fixtures (pyright JSON with
  absolute paths → relativized; pytest failures/summaries; tsc text) — assert
  bounded, deterministic, secret-safe output and path relativization.
- **Fake-command routing tests** (mirror the LSP fake server): a stub script
  emitting canned pyright/pytest output validates command selection, structured
  parsing, and evidence recording without the real toolchain.
- **Real-tool ignored smoke tests**: `real_pyright_typecheck_*`,
  `real_pytest_*` (and TS) — run the true toolchain end to end through the
  runtime, gated `#[ignore]` like the LSP real-server tests.
- **Evidence-flow tests**: a failing typecheck event yields
  `validation_summary.status="failed"` with `failure_kind="compile_error"` and
  relativized diagnostics; a mixed sequence stays `mixed` with correct
  `historical_failures`.

## 10. Open decisions (need confirmation)

- **D1** — tool surface shape (recommend D1a: generic per-kind tools, Rust
  stays on `cargo_*`).
- **D2** — auto-detect language with explicit override + `language_required`
  error for ambiguous roots (recommended).
- **D3** — pytest evidence source: JUnit XML (robust, needs a temp file) vs
  text scrape (no temp file, slightly less precise). Recommend JUnit XML with
  text fallback.
- **D4** — lint severity policy: does a lint **warning** (ruff/eslint) count as
  a validation *failure*, or only errors? Recommend: lint failure = non-zero
  exit (tool-defined), surfaced as `lint_error`, but never lowers a
  `finish_coding_task` verdict on its own (advisory, like diagnostics).
- **D5 (new, load-bearing)** — where absolute-path tools run and relativize.
  Recommend: an **agent-side validation adapter** in `webcodex_agent`
  (sibling to `lsp/`) that reuses the LSP path-relativization and produces the
  shared `ValidationDiagnostics` already project-relative and bounded, returned
  over the bridge — rather than shipping absolute paths to the server. This
  makes structured multi-language validation a bridge feature like LSP, not a
  plain `run_shell`-style capture. It is the biggest structural choice here and
  should be confirmed before Phase 1.

## Related

- [LSP_NAVIGATION.md](LSP_NAVIGATION.md) — the multi-language registry this
  mirrors, including "Adding a Language".
- [ARCHITECTURE.md](ARCHITECTURE.md) — validation evidence path.
- [ROADMAP.md](ROADMAP.md) — "Richer multi-language validation adapters".
