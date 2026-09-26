---
name: webpi-handoff
description: Produce or recover a compact, evidence-backed WebPi task handoff across long sessions, model/context boundaries, or interruptions. Use when pausing/resuming substantial work, transferring a task, or when the active context is becoming too large.
---
# WebPi Handoff

Use WebPi's authoritative Project / Workflow Session / Job state as the source of truth. Do not reconstruct a handoff from memory when exact session evidence is available.

## Create a handoff

Collect only the information needed to resume safely:

1. Exact Project id and Workflow Session id.
2. User goal and acceptance criteria.
3. Current Git/workspace state and changed paths.
4. Architectural decisions and important rejected alternatives.
5. Validation already run, with fresh pass/fail evidence and known flaky/transient observations separated from real failures.
6. Active durable Jobs / waits and their exact ids.
7. Consequential effects already performed (deployment, token rotation, package mutation, extension approval), including rollback point.
8. Unresolved risks, blockers, and the smallest next action.

Prefer `session_handoff_summary`, `session_discussion_summary`, `show_changes`, `git_status`, and exact Job observations where available.

## Resume from a handoff

1. Re-observe runtime and project/session state before acting.
2. Verify the Git/workspace and any Job ids still match the handoff.
3. Treat old validation as historical evidence; after any new code change, re-run the relevant tests.
4. If the handoff says an effect was outcome_unknown, reconcile it before retrying.
5. Continue from the smallest uncompleted step rather than replaying the whole task.

## Compact output format

```text
Goal:
Authority: project / session
Current state:
Changed paths:
Decisions:
Validation:
Active jobs/effects:
Remaining risk:
Next exact action:
```

Do not include secrets, full logs, large diffs, or irrelevant history. Reference exact paths/ids instead.
