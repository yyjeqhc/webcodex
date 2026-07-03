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

- **Prefer structured WebCodex edit tools** for source edits when available:
  - `replace_line_range`
  - `insert_at_line`
  - `delete_line_range`
  - `apply_text_edits`
  - `apply_patch_checked`
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
- `callRuntimeTool` is advanced/generic; use dedicated Actions only for stable
  common workflows that fit the operation budget.
- WebCodex GPT Actions must stay below the 30-operation GPT Actions limit. The
  current OpenAPI surface is 27 operations.
- Chunked artifact upload tools (`artifact_upload_begin`,
  `artifact_upload_chunk`, `artifact_upload_finish`, `artifact_upload_abort`)
  remain runtime-only through `callRuntimeTool`; do not promote them to
  dedicated GPT Action operations.
- When adding future runtime tools, default to `callRuntimeTool` exposure unless
  there is an explicit product reason and operation-count budget for a dedicated
  Action.
- GPT Actions should prefer **flattened top-level fields** over `params` / `arguments`.
- Use `recording_session_id` for generic wrapper recorder metadata.
- Use `session_id` as tool business input.
- When a runtime-only tool is expected to work through GPT Action `callRuntimeTool` with flattened top-level fields, `ToolCallRequest.properties` must expose **every** flattened field that GPT Actions need. This includes nested object/list payload fields such as `edits`, `validation`, `labels`, `checkpoint_id`, `confirm`, `dry_run`, `include_untracked`, and `include_diff_stat`. Add/update tests that fail when flattened Action fields are missing. Do **not** loosen `additionalProperties` to `true` as a workaround — list the needed flattened fields explicitly.

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

---

## 10. Standing Task Contracts / Prompt Compression

Subsequent tasks **inherit** the safety (§2), editing (§3), branch/commit (§4),
validation (§5), test organization (§6), architecture (§7), and session (§8)
rules defined in this document. User prompts do **not** need to repeat
no-tag / no-push / no-publish / no-release / no-rebase / no-amend / no-secrets
directives each time.

When a user prompt conflicts with AGENTS.md rules, **the stricter, safer
constraint wins**.

Pre-task checklist:

1. Run `git status --short --branch` and `git log` (recent 5-6 entries).
2. If the worktree is not clean, **stop and report** — do not overwrite another
   contributor's changes.
3. Prefer small, incremental commits. Separate behavior changes from
   pure-documentation changes.

Single-turn scope guideline: if a task would require more than roughly **30
distinct modification actions** or span multiple subsystems, stop and present a
plan before proceeding. This prevents runaway scope in a single pass.

Final report for development tasks should be concise:

- changed files
- behavior change (if any)
- validation result
- full suite yes/no
- commit hash
- known limitations

Review-only or analysis tasks do not need the full development report format.

---

## 11. Default WebCodex Implementation Constraints

These rules supplement §3 (Editing and Refactoring Rules) and are expected to
apply to every development task unless explicitly overridden.

- **Do not** use shell `sed`/`perl`/`python` as the primary editing mechanism.
  Prefer structured line-edit tools or standard editor patches.
- Shell is for **inspection, testing, and diagnostics only**.
- Release docs, changelogs, packaging, and deploy config are **read-only**
  unless the user explicitly requests changes.
- When adding or renaming a runtime tool, **all** registry, metadata, OAuth
  scope policy, MCP, OpenAPI, and test entries must be updated in the same
  commit (see §7 Architecture Invariants).
- Auth / OAuth / DB changes must pass at minimum:

  ```
  cargo fmt --check
  cargo check --all-targets
  cargo test --bin webcodex oauth -- --nocapture
  cargo test --bin webcodex scope -- --nocapture
  cargo test --bin webcodex metadata -- --nocapture
  git diff --check
  git status --short --branch
  ```

---

## 12. OAuth Bridge Standing Decision

Reference: `docs/OAUTH2_BRIDGE_THREAT_MODEL.md` (full threat model, endpoint
contract draft, scope policy, acceptance tests, and open questions).

These are **decided v1 constraints**. Do not re-litigate them in implementation
tasks unless the user explicitly opens a design discussion.

### Supported identity paths

- **Formal managed-user OAuth** remains fully supported (existing behavior).
- **Low-config OAuth onboarding** for MCP / AI platforms should use explicit
  shared-key OAuth principal support.
- **Synthetic managed users are rejected.** Do not create rows like
  `user_id = shared-key:<hash>` and do not auto-insert user rows for shared
  keys.

### Subject model contract

OAuth token subject model must explicitly distinguish two kinds:

| kind | identifier | notes |
|---|---|---|
| `managed_user` | `user_id` | existing managed-account OAuth flow |
| `shared_key` | `shared_key_hash` | non-managed principal; no user row required |

Key invariants:

- `shared_key_hash` affects shared-key project and job visibility.
- `shared_key_hash` does **not** convert an `OAuth2Token` into
  `AuthKind::SharedKey`.
- A shared-key OAuth principal **must not** receive `account:manage`, `admin`,
  or agent-transport scopes by default.
- A shared-key OAuth principal **may** use `runtime`, `project`, and `job`
  scopes according to OAuth scope policy.
- OAuth2 tokens remain **rejected** on agent transport endpoints.
- Current-session identity remains OAuth identity semantics unless a future
  task explicitly changes it.

### Non-goals for v1 bridge

- No blank OAuth field fallback.
- No open anonymous bridge.
- No plaintext shared key storage.
- No public bridge endpoint until subject model and tests are stable.

---

## 13. OAuth Bridge Implementation Order

This is the approved sequencing for implementing the shared-key OAuth bridge.
Do not skip ahead to later phases until earlier phases are stable and tested.

1. **Subject model schema refactor**
   - `oauth_authorization_codes`, `oauth_access_tokens`, and
     `oauth_refresh_tokens` support `managed_user` and `shared_key` subjects.
   - A shared-key subject must **not** require a managed-user lookup.

2. **OAuth2Verifier subject dispatch**
   - `managed_user` branch checks user existence and disabled state.
   - `shared_key` branch requires `shared_key_hash` and constructs a
     non-managed OAuth `AuthContext`.

3. **Scope policy enforcement**
   - Allow `runtime`, `project`, and `job` scopes as explicitly configured.
   - Reject `admin`, `account:manage`, and agent-transport scopes for
     shared-key OAuth tokens.

4. **Public authorize UI / route**
   - Implement **only after** subject model and tests are stable.
   - Must not include blank OAuth field fallback, open anonymous bridge, or
     plaintext shared key storage.
