# Coding Workflow

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

This guide is for ordinary WebCodex users and coding agents using WebCodex. It
explains how to bootstrap a coding task, choose model behavior, validate work,
and interpret closeout evidence. It is not a contributor architecture guide for
developing WebCodex itself.

## Canonical mental model

`work_on_project` is the **canonical model bootstrap** for ordinary coding and
review work. It establishes or continues project-scoped Workflow Session evidence
and returns bounded workflow guidance plus project-local instructions. It is not a
role selector.

- Use `work_on_project` for the normal coding bootstrap. Its task `instruction`
  is the natural place to state what the agent should do.
- `include_project_instructions=true` (the default) always projects the current
  bounded repository-instruction bodies, including on an exact Workflow Session
  continuation whose repository delta status is `reused`. Set it false only when
  the caller's current model context already retains those instructions. WebCodex
  still re-observes the files and updates Workflow Session instruction metadata.
- `include_workflow_guidance=true` (the default) always projects the canonical
  built-in coding workflow. Set it false only when the caller's current model
  context already retains that guidance. This is projection-only: it does not
  change Workflow Session state, authority, role selection, or execution semantics.
- WebCodex does not infer model-context retention from a `wc_sess_*` Workflow
  Session, MCP/HTTP transport identity, client window, credential, project, or
  server lifetime. A Workflow Session is business continuity and may be resumed
  by multiple independent model contexts. The caller-explicit include flags are
  the only inputs that suppress these static model-facing bodies.
- `work_on_project` success output is sparse by default. Omitted default sections
  mean no Session execution defaults, ordinary existing-project resolution, the
  intentionally skipped repository overview, pass/non-blocking readiness, no
  noteworthy Jobs, or empty blockers/warnings. Instruction sources always keep
  path/fingerprint identity; false/null/empty body-projection fields are omitted.
  Positive warnings, blockers, truncation, non-default resolution, and noteworthy
  Job state remain explicit.
- `work_on_project` is the canonical external coding bootstrap and continuation
  entry. The retired `start_coding_task` wire/API tool name now fails closed with
  guidance to use `work_on_project`; its advanced startup fields are not a public
  compatibility surface. An internal `StartCodingTask` primitive remains only as
  shared implementation plumbing for the canonical workflow.
- Choose behavioral roles in the **task instruction**. For implementation work,
  explicitly say to use `implementation_owner` guidance. For a separate review,
  explicitly say to use `independent_review` guidance.
- Returned role guidance is model guidance only. It does not create authority,
  permissions, a Session mode, or a capability. Authentication, project access,
  tool policy, and runtime guards remain authoritative independently of it.

There is no `role` wire field and no durable Session role state. A Session can
continue while the task instruction for a later pass asks the model to follow a
different behavioral role.

WebCodex also has a project-bound Connector surface whose entry tool is
`task_start`. When that surface is selected, follow its task workflow instead;
the same principle still applies: bootstrap state and behavioral guidance are
not execution authority.

## Copyable prompts

Implementation:

```text
Use WebCodex to bootstrap or continue this coding task. For this implementation,
use the implementation_owner guidance. Implement <task> through the existing
architecture, run focused structured validation, and review the resulting diff.
```

Independent review:

```text
Use WebCodex to bootstrap or continue this coding task. For this pass, use the
independent_review guidance. Independently review <change or commit>, correct
only concrete findings, and run focused regression validation before reporting
whether the change is acceptable.
```

The role names belong in the instruction text. Do not look for a role parameter
on `work_on_project`.

## Manual multi-window collaboration

For a bounded independent subtask, keep coordinator `C` and worker `W` in
**separate** Workflow Sessions. The coordinator posts a `todo` to `C`; `W` calls
`get_session_assignment(session_id=C, message_id=<todo_id>)` to atomically obtain the
exact todo, all retained direct replies within the bound, and its opaque
`assignment_fence`. After performing tools and validation under `W`, the worker calls
`complete_session_message` with the same concrete `session_id=C`, exact `message_id`,
caller `completion_key`, and the unchanged `expected_assignment_fence`. On stateless
MCP 2026, `recording_session_id=W` is separate wrapper provenance metadata and is
stripped before concrete parsing. `author_session_id` comes only from that already-
authorized explicit recorder; without one it is absent, never inferred from a client
window, credentials, project identity, or legacy `mcp-session-id`.

`list_session_messages(message_id=...)` / `list_session_messages(reply_to=...)` remain
generic browsing/result lookup rather than executable-assignment input. If completion
returns `assignment_stale`, re-evaluate the returned current assignment before using
a durable fresh fence; history-loss/oversize assignment errors are not blind-retry
signals. `observe_session_messages` is optional generic delta observation, not this
happy-path assignment protocol. The coordinator then re-observes authoritative
project/Git/artifact state. Worker execution history
is never copied into `C`, and Session/message ids never grant authority. Any recording
Session is authorized before ledger/lifecycle/provenance use. Project-scoped targets
require both current stored-project authorization and their immutable creation-time
canonical authority-group fingerprint; project-less Sessions use the same internal
durable fence. Direct shared-key and OAuth shared-key bridge presentations normalize
to one authority group. Cross-Session collaboration requires exact project-scope
matching, so scoped/unscoped pairs cannot bridge project authority. The message board
is collaboration metadata, not a claim, lease, filesystem lock, or branch lock. Use
separate worktrees/WebCodex Projects for concurrent writers.
There is no automatic worker spawning, scheduler, shared transcript, or implicit
cross-Session/cross-owner delegation.

For a coordinator or worker window that needs to notice later message-board changes without re-listing history, call `observe_session_messages(session_id=...)` once without a token to establish the current baseline, then reuse its opaque `observation_token`. A token call may optionally wait once for up to 60 seconds. The result reports retained current-state delta, `has_more`, and explicit `history_lost` when retention prevents a complete delta. Use `list_session_messages` for historical or exact filtered reads; observation does not replace it.

Message observation is not a delivery receipt, model-context-retention proof, subscription, or Agent Wake. It does not automatically resume a model, route a Conversation, or spawn a worker. Durable Agent/Conversation/Delivery/Wake state is a separate Server-owned domain and is not part of this Workflow Session observation primitive.

See [Manual Multi-Window Collaboration](agent/manual-window-collaboration.md) for
coordinator/implementation, implementation/reviewer, parallel-worktree, and
cross-host examples plus retry/idempotency semantics.

## Habits that make the workflow reliable

**Reuse validation identity after a fix.** When a structured validation uses
`assertion_name`, rerunning the same logical validation after correcting a
failure should reuse the same `assertion_name`. The validation ledger can then
represent that one logical assertion as resolved instead of treating the rerun
as an unrelated check.

**Treat guarded edit conflicts as zero-write failures.** A stale SHA or edit
anchor fails closed. Re-read the current file, inspect the new exact content,
and retry the intended edit with fresh guards. Do not weaken the guard merely
to make the edit apply.

**Use `apply_patch` as the default model-generated edit path.** Use
`apply_text_edits` for small exact SHA-guarded edits and `apply_unified_diff`
only when the input is already a raw unified diff. Ordinary `apply_patch`
matching remains Codex-compatible: `exact` → `trim_end` → `trim`. Each update
chunk reports bounded positioning metadata: `match_mode`, `match_source`,
`matched_start_line`, `candidate_count`, and `strict_match`. A text-positioned
chunk has `strict_match=true` only when every match used for positioning was
exact and unique. An unanchored append is strict-safe without a text match and
reports `match_source=append` with null `match_mode` / `candidate_count`.
WebCodex 0.4 requires the Runner to advertise `apply_patch_match_metadata` before
any `apply_patch` dispatch. Successful patch-plan/match metadata is sanitized and
validated by the Server; older apply_patch success shapes fail closed rather than
being accepted as a compatibility case.

Set `strict_matching=true` when every positioned chunk must satisfy that
exact-and-unique rule before any file is written. This mode requires the Runner
`apply_patch_strict_matching` capability and rejects fuzzy or ambiguous
placement instead of silently downgrading. The Server validates successful
Runner match metadata against its parsed patch; missing or contradictory
success metadata is surfaced as `outcome_unknown`, not as clean success.

**Prefer structured validation.** Use focused tools such as `cargo_test`,
`go_test`, or other structured validation when they express the check you need.
Use a shell only when the structured surface does not cover the validation.
Structured results give the Session ledger safer evidence than parsing arbitrary
command text.

When a focused `cargo_test` must prove that tests actually ran, set
`require_tests: true` for a minimum of one test or `min_tests: N` for a larger
bounded minimum. When both are present, the stricter minimum applies. Without
either field, an exit-zero invocation that runs zero tests remains execution
success and reports `tests_run_count: 0` plus `zero_tests_run: true`. An
explicit count assertion passes only when complete parser evidence proves the
minimum; incomplete or truncated evidence fails the validation contract without
rewriting the process exit code. Count assertions cannot be combined with
`no_run: true`.

**Read closeout as evidence, not completion authority.** `finish_coding_task`
returns a deterministic advisory snapshot of recorded Session evidence,
including validation, workspace, jobs, and tool history when requested. It does
not decide that the task is complete, replace direct diff/test review, or make
the final user-facing acceptance decision.

## A typical broader-runtime loop

```text
work_on_project
→ inspect/search/read
→ guarded edits
→ structured focused validation
→ review diff/workspace
→ finish_coding_task
```

Continue to follow the exact tool surface and project instructions returned by
the connected Server. The workflow guidance helps the model reason about the
pass; it never widens what the caller is allowed to do.
