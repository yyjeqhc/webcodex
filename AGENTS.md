# AGENTS.md — WebCodex Agent Development Guide

Guide for autonomous agents (Codex, GLM, ChatGPT, MCP, GPT Action) developing against this repository.

---

## 1. Project Identity

- **Project:** WebCodex
- **Default project id:** `agent:oe:private-drop`
- **Default path:** `/root/git/private-drop`
- Current development branch may vary; **inspect `git status` and `git log` before editing**.
- Do not assume `main` is the active branch.

---

## 2. Non-Negotiable Safety Rules

| Rule | Enforcement |
|---|---|
| Do not tag. | Never create git tags. |
| Do not push. | Never push to any remote. |
| Do not npm publish. | Never run `npm publish` or equivalent. |
| Do not create GitHub Releases. | Never create releases via API or CLI. |
| Do not rewrite history. | No `rebase`, `filter-branch`, or `commit --amend` unless explicitly requested; do not use destructive resets on changes you did not create. |
| Do not rebase/squash unless explicitly requested. | Preserve merge topology. |
| Do not touch secrets or token files. | Skip `.env`, `.env.*`, credential files, and any file containing tokens. |
| Do not print token values. | Never echo, log, or include tokens in output. |
| Do not modify release docs unless explicitly requested. | Leave release checklists, changelogs, version docs, and packaging/release files untouched. |

---

## 3. Editing and Refactoring Rules

- **Prefer structured WebCodex line-edit tools** for source edits when available:
  - `replace_line_range`
  - `insert_at_line`
  - `delete_line_range`
- **Do not** use shell `sed`/`perl`/`python` as the primary editing mechanism.
- Use shell for inspection, tests, and bounded diagnostics only.
- Keep changes scoped to the requested task.
- Necessary structural refactors are allowed when they reduce coupling, clarify ownership, or keep modules/test files from growing without bound.
- Do not preserve obsolete compatibility layers by default. Keep backward compatibility only when it is part of an external public contract, release artifact, documented API, or explicitly requested migration path.
- Prefer small, reviewable refactor commits over conservative accretion. If a file has become a dumping ground, split it rather than adding more local exceptions.
- Do not mix behavior changes with mechanical moves unless unavoidable; when unavoidable, report the semantic change explicitly.
- Avoid broad refactors only when they are unrelated to the task or would obscure an active feature/fix.

---

## 4. Branch and Commit Rules

- Always check `git status` and `git log` before editing.
- Worktree must be clean before starting.
- Create a local commit **only when explicitly requested**.
- Commit message prefix convention:
  - `feat:` — new functionality
  - `fix:` — bug fix
  - `refactor:` — structural change, no behavior change
  - `docs:` — documentation only
  - `test:` — test additions or changes
- Final report must include **commit hash** and **validation run output**.

---

## 5. Validation Matrix

Lightweight strategy — do not run the full suite every time.

### Always (code changes)

```
cargo fmt --check
cargo check --all-targets
git diff --check
git status --short
```

### Docs-only

```
git diff --check
git status --short
```

### Session / current-session / guard changes

```
cargo test --bin webcodex session -- --nocapture
cargo test --bin webcodex metadata -- --nocapture
```

### Runtime HTTP / REST changes

```
cargo test --bin webcodex runtime_http -- --nocapture
cargo test --bin webcodex openapi -- --nocapture
```

### MCP changes

```
cargo test --bin webcodex mcp -- --nocapture
```

### OpenAPI / registry / metadata changes

```
cargo test --bin webcodex openapi -- --nocapture
cargo test --bin webcodex metadata -- --nocapture
```

### Auth / OAuth / scope changes

```
cargo test --bin webcodex oauth -- --nocapture
cargo test --bin webcodex scope -- --nocapture
cargo test --bin webcodex metadata -- --nocapture
```

### Full suite

```
cargo test --bin webcodex
```

Required only before merging to `main`, after broad core changes, or when explicitly requested.

> **Note:** Existing ignored `import_http` tests may remain ignored unless the task specifically targets them.

---

## 6. Test Organization Rules

- Do not add large ordinary test blocks to production `mod.rs` files when a `tests/` submodule exists or can be created.
- `src/tool_runtime/mod.rs` must remain a runtime module, not a test warehouse. New tool-runtime tests should live under `src/tool_runtime/tests/` and be grouped by domain such as `schema`, `tool_call`, `dispatch`, `sessions`, `checkpoint`, `files`, `git`, `jobs`, and `metadata`.
- Shared test setup belongs in `tests/support.rs` or a narrow domain helper, not copied across many tests.
- Prefer table-driven tests for repeated schema, parser, route, and auth matrix coverage, but keep exact assertions for security, destructive actions, schema required fields, session guards, and transport envelopes.
- Do not reduce test line count by deleting critical assertions, skipping tools from consistency tests, or weakening schema/serialization checks.
- Slow tests should be made deterministic first: replace sleeps/polling with channels, notifications, bounded retries, or direct state inspection when possible.
- Use `#[ignore]` only for tests that require real external dependencies, long-running network behavior, or intentionally heavy integration coverage. Document why the test is ignored.
- Suggested size limits: if a test file exceeds roughly 2,000 lines or mixes unrelated domains, split it; if a test function exceeds roughly 80 lines, extract fixtures or use a table.
- After moving tests mechanically, keep names and assertions stable first. Do semantic cleanup or deduplication in a separate commit.

---

## 7. Architecture Invariants

Tool metadata, registry, OAuth scope policy, MCP `tools/list`, and OpenAPI `callRuntimeTool` names **must stay synchronized**.

If adding or renaming a runtime tool, update **all** of:

1. `ToolCall` enum / parser
2. `KNOWN_TOOL_NAMES`
3. metadata
4. registry schema
5. OAuth runtime tool policy
6. OpenAPI accepted names / examples
7. MCP schema tests
8. consistency tests

### OpenAPI exposure rules

- Do **not** expose legacy `/api/codex` routes in GPT Action OpenAPI.
- Do **not** expose agent token management or pairing endpoints in GPT Action OpenAPI.

### GPT Action / OpenAPI Product Rules

- Prefer **non-consequential** labels for read-only, discovery, onboarding, and bounded local project setup actions.
- `registerProject` and `createProject` are **non-consequential** onboarding actions when constrained by agent policy, allowed roots, and non-overwrite defaults.
- Keep **destructive actions consequential**: shell/job execution, raw writes, patch application, delete/restore/discard, imports, and generic dispatch.
- `callRuntimeTool` is advanced/generic; prefer dedicated actions when available.
- GPT Actions should prefer **flattened top-level fields** over `params` / `arguments`.
- Use `recording_session_id` for generic wrapper recorder metadata.
- Use `session_id` as tool business input.

---

## 8. Session Invariants

- **Explicit `session_id` always wins** over current session.
- **Unknown explicit `session_id`** must return `unknown_session_id` — never silently fall back to current session.
- **`read_only` sessions** deny write-like and shell/job-like tools.
- **Guard denial must happen before** mutation or agent enqueue.
- Guard denial should record a failed session event when the session id is valid.
- `session_summary`'s required `session_id` is business input; do not implicitly replace it with current session.
- Current session bindings are **in-memory** and isolated by principal, transport, and resolved project.

---

## 9. Final Response Requirements

Every agent final response must include:

- **Files changed** — list of modified/added/deleted paths
- **Behavior changes** — what differs at runtime (if any)
- **Validation run** — commands executed and their outcomes
- **Full suite yes/no** — whether `cargo test --bin webcodex` was run
- **Commit hash** — if a commit was created
- **Known limitations** — skipped checks, ignored tests, or deferred work
