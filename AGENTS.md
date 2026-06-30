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

## 3. Editing Rules

- **Prefer structured WebCodex line-edit tools** for source edits when available:
  - `replace_line_range`
  - `insert_at_line`
  - `delete_line_range`
- **Do not** use shell `sed`/`perl`/`python` as the primary editing mechanism.
- Use shell for inspection, tests, and bounded diagnostics only.
- Keep changes scoped to the requested task.
- Avoid broad refactors unless explicitly requested.

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

## 6. Architecture Invariants

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

---

## 7. Session Invariants

- **Explicit `session_id` always wins** over current session.
- **Unknown explicit `session_id`** must return `unknown_session_id` — never silently fall back to current session.
- **`read_only` sessions** deny write-like and shell/job-like tools.
- **Guard denial must happen before** mutation or agent enqueue.
- Guard denial should record a failed session event when the session id is valid.
- `session_summary`'s required `session_id` is business input; do not implicitly replace it with current session.
- Current session bindings are **in-memory** and isolated by principal, transport, and resolved project.

---

## 8. Final Response Requirements

Every agent final response must include:

- **Files changed** — list of modified/added/deleted paths
- **Behavior changes** — what differs at runtime (if any)
- **Validation run** — commands executed and their outcomes
- **Full suite yes/no** — whether `cargo test --bin webcodex` was run
- **Commit hash** — if a commit was created
- **Known limitations** — skipped checks, ignored tests, or deferred work
