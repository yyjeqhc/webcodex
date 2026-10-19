# Coding Workflow

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

This guide is for coding/review through a connected WebCodex runtime. Its tool
names and Workflow Sessions belong to that runtime. For local development of the
WebCodex repository with another host's tools, follow [CONTRIBUTING.md](../CONTRIBUTING.md)
and [AGENTS.md](../AGENTS.md). This guide does not require installing WebCodex to contribute.

## Plan and continue

For substantial development, first inspect relevant rules/code and define observable
completion conditions. Show a short plan organized by outcomes, then continue
with authorized implementation; do not stop merely to ask whether to proceed.
Ask only when a material requirement or authority is missing. Tiny work needs no
plan, and a visible plan does not require a durable Goal. Existing Goal selection
preferences remain unchanged.

Choose the next action from actual tool results. Revise the remaining approach
when evidence disproves a hypothesis or the user changes requirements; retain
useful decisions/progress in the established Session. Never pre-complete tests or
repeat the same failure without new diagnosis. Keep one Job for each execution.
A truncated read uses its returned continuation, a stale edit needs a fresh read,
and an uncertain effect needs reconciliation rather than replay.

A Project selector rejection is input validation, not proof of an offline Runner
or a missing Session. For an already registered Project use its canonical id or
issued `project_ref`. If unknown, use authorized `list_projects` discovery; do not
guess a Runner from a filesystem path. `client_id + path` remains an explicit
bootstrap form, with the same authorization as before.

## Normal loop

The ordinary WebCodex coding loop is intentionally small:

```text
work_on_project
→ inspect / search / read
→ edit
→ present_work_result once for substantial work
→ focused validation
→ review changes
→ finish_coding_task
```

`work_on_project` is the canonical bootstrap for normal coding and review. Give it the current task instruction and then follow the project instructions and tools returned by the connected Server.
Its bounded workspace snapshot reuses the same startup Git observation for branch/HEAD, upstream tracking and ahead/behind state, plus a bounded dirty-path list when available. Treat these fields as the initial observation instead of immediately repeating `git status`/branch probes; refresh only after relevant mutations or when exact additional Git facts are required. Independent Git, runtime, instruction, semantic-navigation and extension startup observations are scheduled concurrently where their authority and results do not depend on one another.

For substantial coding, present the exact Workflow Session once with `present_work_result(project, session_id)` after it becomes materially stateful (for example after the first meaningful source mutation or when long-running validation begins). The mounted MCP App performs bounded live Workspace / Validation / Review reads itself, so do not repeatedly present it or spend model turns polling solely to keep it current. Tiny and read-only work does not need a progress card. A non-blocking `finish_coding_task` seals eligible final changes at closeout; the already-mounted card discovers that immutable snapshot on a later App refresh. If no card was mounted and closeout returns the explicit presentation suggestion, present it once then.
By default it also returns a small bounded `extensions` catalog for selection: Skill metadata comes from the canonical project/configured/managed Skill union, and Plugin metadata is restricted to ready providers whose configured working directory matches the Project root. This metadata grants no authority and does not load Skill bodies or create Plugin bindings; use `read_skill_file` for Skill text, `run_skill_resource` only for trusted Runner-configured live `scripts/` resources guarded by `expected_definition_revision` or Runner-installed managed resources additionally fenced by `expected_package_revision`, or `plugin_tool describe -> call` after selecting a relevant entry. Configured resource bytes remain live until execution rather than being package-revision-pinned. Set `include_extension_catalog=false` only when the current model context already retains that discovery metadata.

Skill reads (`load_skill` and `read_skill_file`) return `skill_path`, the normalized package-relative path used by `read_skill_file.path`. Project Skills also return authoritative `project_path`: use it with returned source line ranges for Project citations, even when the package folder differs from the metadata name. Runner-configured and managed Skills omit `project_path`; cite their Skill identity, `skill_path`, resource `sha256`, revisions and source lines without inventing a Project path. Native Skill roots are not exposed.

For substantial cross-turn work, a durable Goal can record the objective and completion conditions without requiring user-managed steps. `update_goal` edits an active Goal with an exact `expected_revision` and `idempotency_key`: omitted `completion_conditions` preserves the list, while `[]` clears it. Replacement conditions use the same bounds as creation (at most eight, each nonempty after trimming and at most 512 UTF-8 bytes). Editing conditions preserves existing steps, checkpoints and progress; it does not evaluate acceptance, resume execution or reopen a completed/cancelled Goal. Re-observe a revision conflict; reuse the same key and exact request after an uncertain result.

## Workbench app

The **Projects & Resources** Workbench adds Files, Work and Goals views to the existing Project and Session resource browser. Files are read-only previews and references; choosing **Add selected references** updates the Host model context when the Host advertises a supported modality. It does not edit Project files. Goals belong to the current communication account and remain available across Projects.

The Workbench instance label defaults to `WebCodex`. Set `WEBCODEX_INSTANCE_NAME` on the Server to choose a shared instance name. Each authenticated account can also save a personal display-name override in Workbench settings; clearing it returns to the configured instance name. If the Workbench commands do not appear after upgrading, refresh the Host's tool list or reconnect the Server so it can discover the current App tools. Native Host rendering has separate capability requirements; a local fixture test does not certify a live Host.

Submitting work requires an explicitly selected Project, an owned Agent, and one exact currently available route: a verified continuation in this chat, or a provider exposed by that Project's Runner. The Workbench creates the canonical AgentTask and Attempt and dispatches through that existing route. It does not accept a free-form command or create a new execution backend. If submission returns an uncertain result, retry the same draft with the same key and unchanged fields; inspect the returned Task or reconcile it before starting a different submission. Optional Goal association uses the selected caller-owned Goal.

Session todos appear only after selecting both a Project and one authorized Workflow Session. The Workbench does not infer a Session from recent activity. Historical 32-hex Goal identifiers came from pre-stable development; supported stable versions use compact `wc_goal_` identifiers. Do not rewrite database IDs by hand. If an older development database produces an invalid Goal ID or storage error, preserve a backup and report the WebCodex version and sanitized error to the maintainer for version-specific migration guidance; do not delete or reset the database as a first step. See the [Workbench implementation notes](implementation/workbench-resource-references.md).

## Start or continue a task

Use `work_on_project` for both a new coding task and an explicit continuation. WebCodex keeps bounded Workflow Session evidence so validation, review, and handoff can refer to the same unit of work, but that Session is not an authentication credential and does not widen project access.

For ordinary use you do not need to reason about WebCodex's internal continuity or audit fields. Those are implementation/maintainer contracts.

The built-in default guidance is the ordinary implementation workflow. A normal
“implement/fix/refactor” task needs no role name: carry authorized work to a
concrete, reviewable completion, map cross-layer changes end to end, keep the
design minimal, validate proportionally, and report evidence honestly. Use only
tools and protocol fields supported by the current exposed schemas.

`independent_review` is the only optional named role because it changes behavior.
Use it only when the task explicitly asks for an independent review pass:

```text
Use the independent_review guidance. Review <change or commit> independently,
report concrete findings with file/line evidence and impact, and do not edit.
```

To request corrections as well, explicitly add “fix concrete findings and run
focused regression validation.” Naming a review role alone does not authorize edits,
and no role ever grants authority.

Guidance is delivered in tool results; it is not the client's system prompt and
does not grant execution authority. Host instructions, the user's task,
applicable project rules, authentication, and runtime safety policy still apply.
Delivery is not proof that a model read, retained, or followed the guidance.
`work_on_project` keeps its primary result compact: request static model-facing
material only when the current model context needs it, using
`context_request=["project.instructions"]` and/or
`context_request=["webcodex.workflow"]`. Workflow Session identity never proves
that the current model retained either material.

When bootstrap or discovery returns `project_ref`, reuse it as the `project` selector on ordinary Project-scoped calls. The canonical `agent:<client_id>:<project_id>` identity remains visible for diagnostics and explicit addressing, but the model does not need to mechanically repeat it. A short ref is Server-owned, durable and principal-scoped, carries no authority, and is reauthorized against its pinned canonical Project/root identity on every call.

When a registered Project already exists and isolated work is needed, use `work_on_project(project=<canonical id or project_ref>, mode="worktree", ...)`. Do not reconstruct `client_id`, the source absolute path, or a managed destination. The Server reauthorizes the source Project on that call; the Runner derives and owns managed placement, persists source/base provenance, and returns a new canonical Project plus short ref. Creating that managed Project is a fresh-Session transition, so do not pass the source Project's Session. Continue with the returned managed `project_ref` and Session. The compatibility `client_id + path + mode="worktree"` form remains subject to ordinary path authority and is not the recommended flow when a Project identity is already available. Managed storage layout is an implementation detail and should not be inferred or guessed by the model.

When `work_on_project`, `start_session`, `read_session_summary`, or an explicit handoff returns `session_ref`, prefer that short selector for later explicit Session selection. Business `session_id` and wrapper `recording_session_id` remain separate contracts, but either may explicitly carry the already-issued ref: Runtime canonicalizes it to the pinned `wc_sess_*` before the role-specific authorization and lifecycle/guard logic runs. The canonical identity remains valid and authoritative. The ref is principal-scoped convenience only; omission never infers a recorder and no sticky recorder context is created.

On Stateless MCP, carry the chosen Session selector as `_wc.record` on each ordinary call intended for that Session's recorder provenance. A business `session_id` does not select the recorder, and calls that omit `_wc.record` are not backfilled into the Session ledger.

### Switch back to an exact workspace context

For an already selected Active Session, `work_on_project(session_id="~s12", instruction="Continue the review")` is sufficient. Omit `project`, `client_id`, and `path`; the default `checkout` mode authorizes that exact Session, derives its bound Project, reauthorizes current Project access, and resumes the same Session. The result supplies `project_ref` for ordinary Project tools. Supplying an explicit Project is still supported and must match; `_wc.record` remains independent recorder provenance and never supplies the target.

Fresh work still needs `project` or `client_id + path`. `mode="worktree"` always needs an explicit source and does not accept a Session as that source. Closed, missing, inaccessible or unscoped Sessions do not trigger a fresh fallback. No current Project/Session is remembered implicitly in a Window.

When the Session selector was lost in the original Window, request `_wc.context=["workflow.resume"]` and explicitly choose a candidate. Authorized candidates retain canonical identity and also expose `session_ref` and, when the current root fingerprint/reference store is available, `project_ref`. This is discovery, not automatic resume; read the exact handoff when task context was lost. Across windows use `list_sessions(project)` when needed.

## Tool strategy guidance

For account/window switches, save agreed decisions and current progress with
`post_session_message`. If the old Session identity is missing, discover with
`list_sessions(project)`, explicitly select a candidate, then read its handoff
before resuming. See [Session continuity](SESSION_CONTINUITY.md) for the full
recovery and configured model review flow.

`work_on_project` accepts an optional `guidance_profile`. An explicit value always
wins for guidance only. MCP omission uses the current request's
`X-WebCodex-MCP-Profile`, or the deployment `WEBCODEX_MCP_HOST_PROFILE` when absent.
See [request-local client policy](MCP.md#request-local-client-policy); HTTP headers
select transport timing independently from this tool argument. Non-MCP/internal
omission falls back to `direct`.
The current workflow contract returns shared `guidance`, `model_protocol` and review `roles`,
plus only the selected `tool_strategy`, when explicitly requested through
`context_request=["webcodex.workflow"]`. The selection is request-local: choose again
on exact resume without changing Session identity or business state. It is never
remembered from a Window, Session or past tool use, and grants no tools, admission,
authority or execution semantics. Builds without Experimental Code Mode reject
explicit `code_mode` as an invalid profile. Startup and later `webcodex.workflow`
context refreshes use the same effective-profile rule.

- `direct`: use the simplest sufficient primitive; batch predetermined independent
  observations and let the model inspect results before adaptive follow-up calls.
- `host_code_mode`: use Host-native orchestration when the Host provides it. Prefer a
  tool's native batch for predetermined same-kind inputs before Host concurrency.
  Predetermined independent cross-tool read-only observations may run in parallel;
  after native batches, prefer `Promise.allSettled` when partial evidence remains useful
  and `Promise.all` only for true all-or-nothing fan-out. Result-dependent
  search/read/branch chains may stay in one Host cell when the Server returns a parser-ready
  `follow_up_kind=mechanically_followable`; copy those generated arguments unchanged after
  current Host input-schema validation. `fallback_recovery` is recovery/detail/dependency
  evidence and must not be auto-followed merely because it is present. A child ToolResult
  arriving is not itself a model-turn boundary: return to the model for semantic choices,
  ambiguity, new user decisions, authority/permission requirements, uncertain outcomes,
  competing recovery choices, unresolved mutation intent, or any effectful replay after a stale
  revision/fence. An exact stale-source reread may still be mechanically followable, but
  `reread_required=true` or `direct_retry_safe=false` is a hard boundary for effectful replay:
  recovery may identify the next observation; it does not authorize automatic mutation retry.
  Keep full results in the Host cell and emit compact decision evidence. Run ready independent
  work and explicit mechanical continuations to quiescence. Only when pending Job dependencies
  really block further useful progress, pass the entire blocked exact set to one
  `wait_for_job_readiness` join barrier. Use `any` when one terminal Job can unlock a useful
  dependent branch, then recompute ready/blocked work; use `all` only at a true join where every
  blocked dependency is required. Never create per-Job long waits or use `Promise.race` for
  first-ready aggregation. Choose `wait_secs` as the largest safe value from the remaining Host
  activation budget after preserving its return guard, capped by the canonical 45s maximum; do not
  prefer fixed 10/15/20s slices. Continue newly ready work in the same cell. After a deadline,
  recompute ready work and the blocked set; if neither changed and no new semantic information
  exists, do not mechanically repeat the same-set wait and yield near the budget boundary. Job
  terminal does not imply mechanically_followable; fallback recovery, authority changes, ambiguity,
  outcome_unknown and effect uncertainty still return to the model. Returning from a Host cell
  does not complete the current model turn. Continue the task within that turn; never assume an
  automatic next turn or use `observe_jobs` heartbeat polling. The
  startup `tool_strategy.host_orchestration` catalog and exact
  `read_tool_manifest(tool_name=...)` hint are both derived from canonical
  `ToolDefinition` metadata. They are guidance only and do not alter
  `ToolCompositionPolicy`, authority, effects, permissions, retry, idempotency, or
  runtime scheduling; broad/default ToolSpecs do not carry them. This profile grants
  no WebCodex capability or authority and does not require nested WebCodex Code Mode.
- `code_mode`: still use a direct primitive for one simple observation. Prefer
  read-only orchestration when related search/read work, cross-file investigation
  or synthesis saves outer model turns. Keep dependent follow-ups sequential inside
  one cell; parallelize only independent observations. Keep raw child results in
  the cell, filter and synthesize them, then emit compact decision evidence through
  `text(...)`. Avoid `text(results)` dumps and project before reaching output limits.

All strategies retain bounded targeted reads, narrow discovery, first-class native
commands/structured tools, and the same recovery, authority, review and closeout.
Canonical edits and structured validators remain the default. Effectful composition
is useful only when related validations save outer turns; guarded mutation composition
is useful only when adaptive read -> one guarded edit benefits. Nested canonical
permissions, effects, validation evidence, Jobs and retry certainty remain unchanged.

## Inspect before editing

Choose the simplest sufficient inspection primitive. If the symbol, test, or implementation region is already known, prefer bounded targeted ranges and batch several related ranges when they are predetermined. For broad discovery, first request a narrow projection such as files-with-matches, count, or a small bounded match set with little context, then read the relevant ranges. A small predictable known-scope native `rg` via `run_process`/`run_shell` is first-class.

The bootstrap reads a fixed set of instruction entry points; it does not scan
every subdirectory for rules. Before changing a path, inspect applicable nested
instructions and recover any relevant missing or truncated rule content.

For branch/PR review, start with the bounded review/change-summary tools exposed by the current Server, then narrow to targeted reads or diff hunks when needed.

## Editing

Use `read_files -> edit_project_files` as the canonical model-generated editing path. `read_revision` is the model-facing snapshot handle, and every edit/delete/rename change carries it as `expected_read_revision`; create needs only its new content. ToolRuntime resolves that revision to the Runner's exact SHA guard internally, so the model never copies digests. Use exact edits for unique source text and `replace_range` for deterministic 1-based inclusive whole-line replacement against the same original snapshot. All edits in one file are planned against that original snapshot and the batch is preflighted transactionally before mutation.

Exact-match ambiguity is a zero-write conflict. The bounded recovery may report candidate line ranges. When the source revision is still current, the model can select the intended lines and retry the same change as `replace_range` with the same `expected_read_revision`; a stale revision instead returns a parser-ready `read_files` recovery and requires a fresh read. `outcome_unknown` is different: inspect the workspace before deciding whether any write should be retried.

`write_project_file`, `apply_patch`, and `apply_unified_diff` are exact-name specialists, intentionally absent from ordinary coding discovery. Use them only when the input is already most naturally a whole-file replacement, Codex patch, or standard unified diff. They are not default recovery paths from `edit_project_files` failures.

Guard failures are **zero-write conflicts**, not reasons to weaken the guard. Re-read the current source and regenerate the intended edit against that state.

For specialist `apply_patch` `matching_mode_rejected`, keep the matching guard and do not switch to `first_match`. Re-read the current source or return to `edit_project_files` when the intended change is naturally exact/range based. If patch form is still materially clearer, consume the bounded parser-ready `read_files` recovery call and preserve the requested patch guard. Never downgrade an explicit stale-context/concurrency fence.

For deterministic `context_mismatch`, consume the bounded `read_files` recovery and regenerate against current source; do not blindly repeat the same patch. If the result is `outcome_unknown`, inspect the workspace before deciding whether any write should be retried.

The exact matching metadata and transactional protocol are maintainer details; see the tool contract/tests when developing WebCodex itself.

## Validation

Formatting is finalization, not per-edit validation. The normal loop is edit → focused validation → further edits if needed → source stabilizes → format once → final review/validation. For Rust, run formatting after relevant source stabilizes and before final diff/closeout; rerun only after later Rust edits that can change formatting. Use `cargo_fmt(check=false)` for intentional final formatting and `check=true` when read-only final formatting proof is needed. CI and release formatting gates remain unchanged.

Prefer `project_validate` for ordinary portable structured validation. Keep `cargo_check`, `cargo_test`, and `go_test` for advanced ecosystem-specific options that need their specialist contracts. Use the smallest check that can detect the regression, and broaden only when the affected boundary requires it.

For ordinary package-scoped validation, use `project_validate.scope.packages`; WebCodex validates a bounded set and lets the selected adapter translate that portable scope. Rust maps it to one Cargo invocation with repeated `-p` selectors after deterministic sort/dedup. Use the `cargo_check` `package` / `packages` specialist selectors only when its advanced Cargo-specific contract is actually needed.

When a required validation outlasts its Server-managed synchronous grace, it hands off automatically as the **same execution** Job. The model should not tune handoff timing. Continue only independent reads, search, diff/architecture inspection, or review, then observe that Job. Do not start extra CPU-heavy validations merely for parallelism. If source covered by the running validation changes afterward, its result is stale/cache-warmup evidence rather than proof of the final workspace; run task-appropriate validation again on the final source.

When a test invocation must prove that tests actually ran, use `require_tests: true` or `min_tests: N`. These are request-scoped evidence assertions, not persistent Workflow Session requirements. If validator execution succeeds but the requested count cannot be satisfied or proven, closeout retains that invocation as an evidence gap rather than a code/test correctness failure. Otherwise, an exit-zero command that legitimately runs zero tests remains an execution result rather than proof of test coverage.

Treat validation failures as evidence, not queue-cleanliness work. If a failure invalidates the current implementation direction or blocks dependent work, diagnose and fix it before continuing that dependent work. Otherwise keep the evidence visible and continue useful independent work; resolve or revalidate when a dependency or closeout requires it. Reuse the same `assertion_name` when intentionally rerunning the same logical assertion. Mutation makes relevant earlier evidence stale, and `outcome_unknown` remains fail-closed.

Use shell/process escape hatches only when the structured validation surface cannot express the check.

## Review and closeout

Review the actual workspace/diff after editing and validation. Passing tests do not replace diff review, and a clean diff does not replace focused validation when behavior changed.

`finish_coding_task` returns a bounded evidence summary for closeout. Treat it as advisory evidence, not as a decision that the work is correct or complete. The model still makes the final engineering judgment and reports the result to the user.

Closeout (including `summary_only`), `read_session_handoff`, and a Session-linked Work Result expose an optional `acceptance` summary derived from the same observations. Its `state` is `ready_for_review`, `needs_evidence`, `failed`, or `not_applicable`; none means that natural-language requirements have been accepted. `reason_codes` identify missing/stale/unproven evidence, and `evidence_refs` name the existing workspace, validation, review or output evidence. Follow the existing next actions and read the exact Session evidence when details are needed. A changed handoff snapshot must be re-read before dependent work.

Execution completion and successful historical checks do not prove current source correctness. Source fences currently report `unproven` or `stale`, so even a successful check still needs source/applicability review. Retained artifact metadata is a historical observation, not current file freshness or content correctness. Read-only work may legitimately require no tests: decide applicability explicitly, without running tests merely to change the badge. A card with no task Session reports `not_applicable`; it never infers a task from Project activity.

## Long-running work

A command or validation that outlives the synchronous grace period continues as the same WebCodex Job. Retain its exact identity and fallback continuation, then finish independent work in the current turn. When Job dependencies block progress, use one bounded `wait_for_job_readiness` join: `any` when one terminal outcome unlocks work, `all` only when every selected dependency is needed. Read logs/details with `observe_jobs`; observation may itself use bounded `terminal` / `all_terminal` waiting when needed. MCP waiting respects the current request policy without shortening execution lifetime. At deadline reassess work and dependencies, never mechanically refill waits or redispatch. A Host cell ending is not task completion; ordinary work does not require an automatic next turn. Preserve pending identities when the task cannot finish. Durable `wait_for_job_terminal` is only for an explicitly established optional continuation workflow, not ordinary waiting. Recovery hints never authorize uncertain-effect retries.

## Manual multi-window collaboration

Multi-window coordination is an advanced maintainer workflow, not part of the ordinary coding loop. Keep independent writers in separate worktrees/Projects and keep their Workflow Sessions separate. Use the assignment/completion tools returned by the current Server rather than copying another window's execution history.

The exact concurrency, retry, provenance, and cross-Session authorization rules are documented in [Manual Multi-Window Collaboration](agent/manual-window-collaboration.md). Their protocol fields are intentionally omitted here.

## Assessing effectiveness

Runtime tests check that guidance is delivered consistently, remains bounded,
matches its schema, and never becomes authority. The scripted
`scripts/eval_coding_loop.sh` checks tool-loop mechanics; it does not run a model
or measure instruction following.

To measure behavioral benefit, compare the same model, tools, settings, and task
fixtures with and without guidance over repeated runs. Include a small fix, a
review-only task, existing unrelated changes, nested instructions, and an
uncertain long-running effect. Compare correctness and scope preservation first,
then unnecessary clarification, duplicate execution, validation quality, tool
calls, and token cost. Do not infer a success-rate improvement from schema tests.

## Internal protocol details

When developing WebCodex itself, use the maintainer contracts rather than expanding this user guide:

- [Session model](agent/session-model.md) — Workflow Session continuity, messages, and evidence semantics.
- [Authority model](agent/permission-model.md) — execution authority and hard-safety layering.
- [Job reliability and concurrency](agent/job-reliability-and-concurrency.md) — Job recovery/observation contracts.
- [Architecture decisions](agent/architecture-decisions.md) — standing implementation decisions.

Ordinary coding clients should not need those documents to complete normal repository work.
