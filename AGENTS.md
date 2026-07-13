# AGENTS.md — WebCodex Agent Execution Contract

Executable rules for autonomous agents working in this repository.
Long-form design context lives under [`docs/agent/`](docs/agent/).

---

## 1. Project Identity

- **Project:** WebCodex
- **Default managed project id:** `agent:oe:private-drop`
- **Repository path:** `/root/git/private-drop` (deployment location may vary)
- Inspect `git status` and `git log` before editing; do not assume `main`.
- Do not infer repository identity from the directory name alone; confirm the
  project id and active repository when needed.
- Do not modify unrelated projects, repos, or worktrees outside the requested
  task scope.

---

## 2. Safety Rules

| Rule | Enforcement |
|---|---|
| No tag by default | Create git tags only under the Release Exception below |
| No push by default | Push only under the Release Exception |
| No npm publish by default | `npm publish` only under the Release Exception |
| No GitHub Release by default | Create releases only under the Release Exception |
| No deploy by default | Do not deploy or publish production artifacts unless explicitly requested under the Release Exception |
| No history rewrite | No `rebase`, `filter-branch`, or `commit --amend` unless explicitly requested; no destructive resets on others' work |
| No rebase/squash unless asked | Preserve merge topology |
| No secrets | Skip `.env`, `.env.*`, credential files, and any file containing tokens |
| No credential output | Never echo, log, or include tokens/secrets in output |
| No release-doc edits unless asked | Leave release checklists, changelogs, version docs, packaging/release files untouched unless the task is explicitly about them |
| No approval bypass | Do not skip, weaken, or circumvent human approval, release gates, guard checks, or policy enforcement to "make progress" |

These defaults apply to every ordinary task. User prompts need not restate them.
When a user prompt conflicts with this file, **the stricter, safer constraint
wins**.

### Release Exception (compressed)

Release operations (tag, push, npm publish, GitHub Release, deploy) are allowed
**only when all** of the following hold:

1. The user **explicitly** requests that release action.
2. The request names version, package, repository, and release target.
3. Worktree is clean at release start (except release files created for the task).
4. Remote tag / GitHub Release / npm version do not already exist.
5. No force-push, tag overwrite, published-commit amend, or release replacement.
6. Release gates run; stop on first failure; never print secrets.
7. Post-tag manifest/checksum commits are reported and must not move the tag.

**Who confirms:** the human requester of the named release task.
**What to record:** targets, gates, outcomes, deferred checks.
Detailed procedure: [`docs/agent/release-process.md`](docs/agent/release-process.md)
and [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md).

---

## 3. Editing Rules

- **Inspect before edit:** read relevant code/docs and existing diffs first.
- **Minimal change:** keep edits scoped to the requested task.
- **Prefer structured edit tools** when available (`replace_line_range`,
  `insert_at_line`, `delete_line_range`, `apply_text_edits`,
  `apply_patch_checked`, or equivalent editor patches).
- **Do not** use shell `sed` / `perl` / `python` as the primary editing mechanism.
- Shell is for **inspection, tests, and bounded diagnostics only**.
- Do not weaken, delete, or skip critical assertions, security checks, schema
  required fields, session guards, or consistency coverage **just to make tests
  pass**. Fix product code or add honest tests; do not hollow out the suite.
- Do not preserve obsolete compatibility layers by default; keep backward
  compatibility only for a named external contract, release artifact, or
  explicitly requested migration (see
  [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md)).
- Necessary structural refactors that reduce coupling or clarify ownership are
  allowed when in scope; do not mix silent behavior changes with mechanical
  moves—report any semantic change.

Test layout guidance (soft limits, domain folders):
[`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md).

---

## 4. Git and Commit Rules

- Always check `git status` and recent `git log` before editing.
- **Dirty workspace:** inspect existing changes and protect them. Continue only
  when those changes are understood, unrelated to your task, and will not be
  overwritten. **Stop only when ownership is unclear, changes overlap the
  requested work, or existing work may be lost.** Do not require a globally
  clean tree for ordinary development (release tasks have their own clean-tree
  rule under Release Exception).
- Create a local commit **only when explicitly requested**.
- Commit message prefixes: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`.
- Prefer small, incremental commits when commits are requested; separate pure
  docs from behavior when practical.
- If a single turn would need roughly **30+ distinct modification actions** or
  span multiple subsystems, stop and present a plan first.

---

## 5. Validation Rules

- **Code changes must be validated** before claiming done. Prefer repository
  structured checks (`cargo fmt`, `cargo check`, focused `cargo test`, and
  domain tools) over ad-hoc scripts.
- **Docs-only** changes: `git diff --check` and `git status --short` (and link
  checks when relevant). Do not run the full Rust suite for pure docs unless
  asked.
- Report **current validation failures** distinctly from **historical ledger
  failures** or pre-existing known failures; do not claim a green tree by
  ignoring real new breakages, and do not "fix" history by deleting evidence
  or weakening tests.

### Always (code changes)

```
cargo fmt --check
cargo check --all-targets
git diff --check
git status --short
```

### Domain add-ons (run the lanes you touched)

| Change domain | Minimum extra tests |
|---|---|
| Session / current-session / guard | `cargo test --bin webcodex session -- --nocapture` and `metadata` |
| Runtime HTTP / REST | `runtime_http` and `openapi` |
| MCP | `mcp` |
| OpenAPI / registry / metadata | `openapi` and `metadata` |
| Auth / OAuth / scope | `oauth`, `scope`, and `metadata` |

### Full suite

```
cargo test --bin webcodex
```

Required only before merging to `main`, after broad core changes, or when
explicitly requested. Existing ignored `import_http` tests may stay ignored
unless the task targets them. Broader lanes:
[`docs/TESTING.md`](docs/TESTING.md).

---

## 6. Architecture Invariants

### Runtime tool surface

Tool metadata, registry, OAuth scope policy, MCP `tools/list`, and OpenAPI
`callRuntimeTool` names **must stay synchronized**. Adding or renaming a
runtime tool requires updating parser/`KNOWN_TOOL_NAMES`, metadata, registry,
OAuth policy, OpenAPI, MCP schema tests, and consistency tests in the same
change.

### OpenAPI / GPT Actions (must)

- Do **not** expose legacy `/api/codex` routes or agent token management /
  pairing endpoints in GPT Action OpenAPI.
- Stay below the **30-operation** GPT Actions limit; **verify via generation or
  tests** — never hard-code a live operation count in this file.
- Prefer flattened top-level Action fields; use `session_id` for tool business
  input and `recording_session_id` for wrapper recorder metadata.
- Do not loosen `additionalProperties` to hide missing flattened fields.

Product detail (consequential labels, discovery, artifacts, smoke paths):
[`docs/agent/openapi-guidelines.md`](docs/agent/openapi-guidelines.md).

### Session (must)

- **Workflow session IDs** use the `wc_sess_*` form; do not change ID format,
  ledger event schema, or lifecycle semantics without an explicit design task.
- **Explicit `session_id` always wins** over current session.
- **Unknown explicit `session_id`** → `unknown_session_id` (never silent
  fallback to current session).
- **`read_only` sessions** deny write-like and shell/job-like tools.
- **Guard denial before** mutation or agent enqueue; record a failed session
  event when the session id is valid.
- `session_summary`'s required `session_id` is business input; do not replace it
  with current session.
- Current-session bindings are in-memory and isolated by principal, transport,
  and resolved project.

Architecture note — two different "session" concepts (workflow ledger vs
HTTP action audit), responsibilities, and non-goals:
[`docs/agent/session-model.md`](docs/agent/session-model.md) (detail) and
[`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md)
(summary).

### Compatibility (must)

- Do not retain compatibility fields for hypothetical consumers.
- Do not emit both a canonical field and an alias for the same concept.
- Do not add deprecated aliases / dual shapes without a named migration.
- Background and examples:
  [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md).

### OAuth bridge design (pointer only)

v1 subject model, scope non-goals, and implementation order:
[`docs/agent/oauth-bridge-plan.md`](docs/agent/oauth-bridge-plan.md) and
[`docs/OAUTH2_BRIDGE_THREAT_MODEL.md`](docs/OAUTH2_BRIDGE_THREAT_MODEL.md).

---

## 7. Security and Privacy Rules

- Do not read, copy, or commit secrets, tokens, private keys, or `.env` contents.
- Do not print credentials, authorization headers, shared keys, or token hashes
  that could be reused.
- Stay inside the resolved project / allowed roots for the task; do not widen
  access or register unrelated project paths without request.
- Treat shell/job execution, raw writes, patches, delete/restore/discard, and
  imports as **consequential**; do not bypass policy, session guards, or
  approval flows.
- Redact sensitive validation/tool output in reports; prefer structured
  summaries over full stdout/stderr dumps.

---

## 8. Final Report Requirements

### Development / code or docs change tasks

Must report:

- **Files changed** — modified / added / deleted paths
- **Behavior changes** — runtime differences, if any
- **Validation run** — commands and outcomes
- **Full suite yes/no** — whether `cargo test --bin webcodex` was run
- **Commit status** — commit hash if created; otherwise exactly:

  ```
  no commit created
  ```

- **Known limitations** — skipped checks, ignored tests, deferred work

### Review-only or analysis tasks

May use a shorter report (findings and recommendations). Full development
fields are not required when no tree changes were made.

---

## Design docs index

| Doc | Contents |
|---|---|
| [`docs/agent/architecture-decisions.md`](docs/agent/architecture-decisions.md) | Session dual model, API evolution, test layout, validation evidence |
| [`docs/agent/oauth-bridge-plan.md`](docs/agent/oauth-bridge-plan.md) | Shared-key OAuth bridge decisions and phase order |
| [`docs/agent/openapi-guidelines.md`](docs/agent/openapi-guidelines.md) | GPT Action / OpenAPI product rules |
| [`docs/agent/release-process.md`](docs/agent/release-process.md) | Expanded release exception procedure |
