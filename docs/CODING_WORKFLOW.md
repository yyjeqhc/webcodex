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
- `start_coding_task` remains an advanced/direct compatibility bootstrap for callers
  that explicitly need managed temporary projects, Session mode/guards, execution
  context, startup detail, exact resume, current binding, or new-session isolation.
  It is intentionally absent from ordinary MCP/OpenAPI model discovery and generic
  Action flattened fields. Ordinary model coding/review should use `work_on_project`.
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
on `work_on_project`; advanced `start_coding_task` does not add one either.

## Manual multi-window collaboration

For a bounded independent subtask, keep coordinator `C` and worker `W` in
**separate** Workflow Sessions. The coordinator posts a `todo` to `C`; `W` reads
`session_handoff_summary(C)` plus that exact `message_id`, performs all tools and
validation under `W`, then uses `complete_session_message` to atomically create one
bounded answer and resolve the exact todo. On stateless MCP 2026, send
`recording_session_id=W` as wrapper metadata while the concrete
`complete_session_message.session_id=C` remains the coordinator/business target;
WebCodex strips the recorder metadata before concrete parsing. The answer carries
trusted `author_session_id` provenance from the recording worker Session first,
falling back to the current worker Session binding only when no recording Session
exists; the caller cannot forge that field, and legacy `mcp-session-id` is not used
as Workflow Session provenance.

The coordinator can retrieve the exact todo or its replies with
`list_session_messages(message_id=...)` / `list_session_messages(reply_to=...)`,
then re-observe authoritative project/Git/artifact state. Worker execution history
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

Message observation is not a delivery receipt, model-context-retention proof, subscription, or orchestrator wake-up. It does not automatically resume a model, route a conversation, or spawn a worker. Room/Discussion remains a future additive collaboration container, not part of this Workflow Session observation primitive.

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

**Prefer structured validation.** Use focused tools such as `cargo_test`,
`go_test`, or other structured validation when they express the check you need.
Use a shell only when the structured surface does not cover the validation.
Structured results give the Session ledger safer evidence than parsing arbitrary
command text.

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
