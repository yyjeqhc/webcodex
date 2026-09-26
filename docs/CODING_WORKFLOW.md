# Coding Workflow

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

This guide is for ordinary WebCodex coding/review work. It describes the model-facing workflow, not the internal continuity, audit, or transport protocols used to implement it.

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

For substantial coding, present the exact Workflow Session once with `present_work_result(project, session_id)` after it becomes materially stateful (for example after the first meaningful source mutation or when long-running validation begins). The mounted MCP App performs bounded live Workspace / Validation / Review reads itself, so do not repeatedly present it or spend model turns polling solely to keep it current. Tiny and read-only work does not need a progress card. A non-blocking `finish_coding_task` seals eligible final changes at closeout; the already-mounted card discovers that immutable snapshot on a later App refresh. If no card was mounted and closeout returns the explicit presentation suggestion, present it once then.
By default it also returns a small bounded `extensions` catalog for selection: Skill metadata comes from the canonical project/configured/managed Skill union, and Plugin metadata is restricted to ready providers whose configured working directory matches the Project root. This metadata grants no authority and does not load Skill bodies or create Plugin bindings; use `skill_read_file` for Skill text, `run_skill_resource` only for trusted Runner-configured live `scripts/` resources guarded by `expected_definition_revision` or Runner-installed managed resources additionally fenced by `expected_package_revision`, or `plugin_tool describe -> call` after selecting a relevant entry. Configured resource bytes remain live until execution rather than being package-revision-pinned. Set `include_extension_catalog=false` only when the current model context already retains that discovery metadata.

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

When `work_on_project`, `start_session`, `session_summary`, or an explicit handoff returns `session_ref`, prefer that short selector for later explicit Session selection. Business `session_id` and wrapper `recording_session_id` remain separate contracts, but either may explicitly carry the already-issued ref: Runtime canonicalizes it to the pinned `wc_sess_*` before the role-specific authorization and lifecycle/guard logic runs. The canonical identity remains valid and authoritative. The ref is principal-scoped convenience only; omission never infers a recorder and no sticky recorder context is created.

## Tool strategy guidance

`work_on_project` accepts `guidance_profile`, defaulting to `direct`. Workflow
contract v21 returns shared `guidance`, `model_protocol` and review `roles`, plus
only the selected `tool_strategy`, when explicitly requested
through `context_request=["webcodex.workflow"]`. The selection is request-local:
choose again on exact resume without changing Session identity or business state.
It is never inferred from a Window, Session or past tool use, and grants no tools,
admission, authority or execution semantics. Builds without Experimental Code Mode
reject explicit `code_mode` as an invalid profile. On a `work_on_project` call the
workflow sidecar uses that call's `guidance_profile`; unrelated tools that request
`webcodex.workflow` use the canonical default `direct` profile.

- `direct`: use the simplest sufficient primitive; batch predetermined independent
  observations and let the model inspect results before adaptive follow-up calls.
- `host_code_mode`: use Host-native orchestration when the Host provides it. Prefer a
  tool's native batch for predetermined same-kind inputs before Host concurrency.
  Predetermined independent cross-tool read-only observations may run in parallel;
  after native batches, prefer `Promise.allSettled` when partial evidence remains useful
  and `Promise.all` only for true all-or-nothing fan-out. Result-dependent
  search/read/branch chains should stay in one Host cell when the next call is
  mechanically determined. A child ToolResult arriving is not itself a
  model-turn boundary: return to the model for semantic choices, ambiguity, new user
  decisions, authority/permission requirements, uncertain outcomes, competing
  recovery choices, or unresolved mutation intent. Keep full ToolResults in the Host
  cell and return compact decision evidence. Treat each Host cell as a short dependency
  DAG, not a long-running Job lifetime. After Job handoff, retain exact identity and
  continue already-known independent work; if the remaining work is primarily waiting,
  end the cell and resume from the exact continuation instead of holding it open. Avoid
  mechanical `observe_jobs` polling. The
  startup `tool_strategy.host_orchestration` catalog and exact
  `tool_manifest(tool_name=...)` hint are both derived from canonical
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

Use `apply_text_edits` as the canonical default model-generated editing path after `read_files`. `read_revision` is the model-facing snapshot handle: use it as `expected_read_revision` when a whole-file stale-context fence is required. Globally unique exact local edits may omit the revision; positional `line_scope`/`occurrence`, delete, and rename require it. ToolRuntime resolves the revision to the Runner's exact SHA guard internally, so the model does not copy digests. This remains the normal path even when many lines change; line count alone is not a reason to choose `apply_patch`. Use `apply_patch` only when contextual patching is materially more natural, a large/multi-hunk rewrite is awkward to express as guarded exact edits, or patch-style context itself expresses the change relationship more clearly. For repetitive code, each patch chunk needs stable, unique surrounding context such as the containing function, impl, type, test, or module; do not use a repeated single line or short fragment as the mutation anchor. Keep the requested matching guard; never weaken an explicit stale-context/concurrency fence. Use `apply_unified_diff` only when the input is already a standard unified diff.

Guard failures are **zero-write conflicts**, not reasons to weaken the guard. Re-read the current source and regenerate the intended edit against that state.

For `matching_mode_rejected`, keep the matching guard and do not switch to `first_match`. Re-read the current source. If the intended change is easy to express as exact edits, prefer `apply_text_edits` and use the current `read_revision` when a positional or stronger whole-file fence is needed. If patch form is still materially clearer, consume the bounded parser-ready `read_files` recovery call and preserve the requested patch guard. Never downgrade an explicit stale-context/concurrency fence.

For deterministic `context_mismatch`, consume the bounded `read_files` recovery and regenerate against current source; do not blindly repeat the same patch. If the result is `outcome_unknown`, inspect the workspace before deciding whether any write should be retried.

The exact matching metadata and transactional protocol are maintainer details; see the tool contract/tests when developing WebCodex itself.

## Validation

Formatting is finalization, not per-edit validation. The normal loop is edit → focused validation → further edits if needed → source stabilizes → format once → final review/validation. For Rust, run formatting after relevant source stabilizes and before final diff/closeout; rerun only after later Rust edits that can change formatting. Use `cargo_fmt(check=false)` for intentional final formatting and `check=true` when read-only final formatting proof is needed. CI and release formatting gates remain unchanged.

Prefer structured validation such as `cargo_test`, `cargo_check`, or `go_test` when available. Use the smallest check that can detect the regression, and broaden only when the affected boundary requires it.

For one Cargo workspace package, `cargo_check` accepts `package`. For several packages, pass `packages`; WebCodex sorts and deduplicates that set, then runs one Cargo process with repeated `-p` selectors. The two selectors are mutually exclusive, and an explicit empty list is invalid.

When a required validation outlasts its Server-managed synchronous grace, it hands off automatically as the **same execution** Job. The model should not tune handoff timing. Continue only independent reads, search, diff/architecture inspection, or review, then observe that Job. Do not start extra CPU-heavy validations merely for parallelism. If source covered by the running validation changes afterward, its result is stale/cache-warmup evidence rather than proof of the final workspace; run task-appropriate validation again on the final source.

When a test invocation must prove that tests actually ran, use `require_tests: true` or `min_tests: N`. These are request-scoped evidence assertions, not persistent Workflow Session requirements. If validator execution succeeds but the requested count cannot be satisfied or proven, closeout retains that invocation as an evidence gap rather than a code/test correctness failure. Otherwise, an exit-zero command that legitimately runs zero tests remains an execution result rather than proof of test coverage.

Treat validation failures as evidence, not queue-cleanliness work. If a failure invalidates the current implementation direction or blocks dependent work, diagnose and fix it before continuing that dependent work. Otherwise keep the evidence visible and continue useful independent work; resolve or revalidate when a dependency or closeout requires it. Reuse the same `assertion_name` when intentionally rerunning the same logical assertion. Mutation makes relevant earlier evidence stale, and `outcome_unknown` remains fail-closed.

Use shell/process escape hatches only when the structured validation surface cannot express the check.

## Review and closeout

Review the actual workspace/diff after editing and validation. Passing tests do not replace diff review, and a clean diff does not replace focused validation when behavior changed.

`finish_coding_task` returns a bounded evidence summary for closeout. Treat it as advisory evidence, not as a decision that the work is correct or complete. The model still makes the final engineering judgment and reports the result to the user.

## Long-running work

A command or validation that outlives the synchronous grace period continues as the same WebCodex Job. Keep its exact Job identity and parser-ready continuation. If useful independent work remains, continue that work and observe the Job later; do not repeatedly poll a running Job merely to keep it visible. When the next useful action actually depends on the terminal result, use the returned continuation; the Server bounds its observation wait for the configured MCP Host profile. The Runtime still supports its transport-neutral observation ceiling internally, while MCP waiting is adapted to the Host budget. For one Job or when any terminal result unblocks progress, use `terminal`; when every Job in a predetermined set is required before progress, use `all_terminal`. Recovery/continuation hints never authorize a retry of an uncertain effect.

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
