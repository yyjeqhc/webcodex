# Coding Workflow

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

This guide is for ordinary WebCodex users and coding agents using WebCodex. It
explains how to bootstrap a coding task, choose model behavior, validate work,
and interpret closeout evidence. It is not a contributor architecture guide for
developing WebCodex itself.

## Canonical mental model

`start_coding_task` and `work_on_project` are **bootstrap and continuity tools**.
They establish or continue project-scoped Workflow Session evidence and return
bounded workflow guidance plus project-local instructions. They are not role
selectors.

- Use `work_on_project` for the normal coding bootstrap. Its task `instruction`
  is the natural place to state what the agent should do.
- Use `start_coding_task` when you need its advanced continuity controls, such
  as exact `resume_session_id` or deliberate `new_session=true` isolation.
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
on `start_coding_task` or `work_on_project`.

## Manual multi-window collaboration

For a bounded independent subtask, keep the coordinator and worker in **separate**
Workflow Sessions. The coordinator posts a `todo` to its own Session; the worker
starts a fresh Session, reads the coordinator's `session_handoff_summary` plus the
relevant open todo, performs the subtask under the worker Session, then posts a
bounded `answer` with `reply_to=<todo_id>` and resolves that exact todo.

The first version is intentionally manual. There is no automatic claim, worker
scheduler, shared transcript, or implicit cross-Session authority. Assign one
worker per todo. Do not concurrently mutate the same worktree from multiple
windows; prefer read-only workers or explicitly isolated worktrees/projects.
Return conclusions, load-bearing evidence, and result paths instead of injecting
the worker transcript into the coordinator.

See [Manual Multi-Window Collaboration](agent/manual-window-collaboration.md) for
the detailed protocol and the dogfood gates for any future convenience primitive.

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
work_on_project (or start_coding_task for advanced continuity)
→ inspect/search/read
→ guarded edits
→ structured focused validation
→ review diff/workspace
→ finish_coding_task
```

Continue to follow the exact tool surface and project instructions returned by
the connected Server. The workflow guidance helps the model reason about the
pass; it never widens what the caller is allowed to do.
