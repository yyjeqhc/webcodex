# Manual Multi-Window Collaboration

This document defines the first supported pattern for manually splitting one coding task across multiple ChatGPT windows or agents. It deliberately reuses the existing Workflow Session handoff and message-board primitives. It is not a scheduler, worker pool, claim service, or shared-transcript design.

## Scope

Use this pattern when a coordinator wants an independent worker to perform a bounded subtask such as analysis, review, investigation, or a clearly isolated deliverable.

The first version is manual:

- a human opens the additional window or agent;
- coordinator and worker keep separate Workflow Sessions;
- the coordinator Session owns the ledger-backed todo and receives the bounded result;
- the worker Session owns the worker's tool/evidence history;
- the worker reads a bounded coordinator handoff instead of inheriting the coordinator transcript;
- no primitive in this workflow grants filesystem authority, task ownership, or a concurrency lease.

The message board does not coordinate filesystem concurrency. Prefer read-oriented workers when several windows share one worktree, and use an explicitly isolated worktree/project when independent concurrent writes are intentional. A worker may still change course when its available authority and later task state allow it; any mutation or other material deviation from the requested posture must be reported, and the coordinator must re-observe current state before consequential follow-up.

## Canonical flow

Assume the coordinator has Session `C` and the worker will create Session `W`.

1. **Coordinator posts one bounded todo to `C`.** Use `post_session_message(kind="todo")`. The todo should contain the objective, narrow scope, important prohibitions, and expected result shape. Include a result path only when the worker is explicitly expected to produce an isolated document or artifact.
2. **Open the worker as a fresh Session.** The worker should normally start a new Workflow Session `W`; do not resume `C` merely to delegate work. Separate Sessions keep the worker's tool history and reasoning context independent.
3. **Worker reads the coordinator handoff.** Call `session_handoff_summary(session_id=C, ...)`, then inspect the relevant open todo with `list_session_messages(session_id=C, kind="todo", status="open")` or the bounded discussion summary. The handoff is continuity evidence, not transcript replay.
4. **Worker performs the subtask under `W`.** Use `W` for project reads, validation, mutation, or other worker-local evidence as the task actually requires. The coordinator todo does not widen project authority. A requested `read_only` posture or current Session guard is useful intent/safety context, but it is not authoritative proof that no later write occurred or that the Session could never change mode; actual tool/effect evidence and the worker's report remain the basis for handoff.
5. **Worker posts a bounded answer back to `C`.** Use `post_session_message(kind="answer", reply_to=<todo_id>)`. Return the conclusion, load-bearing evidence, and any explicit result path. When useful, include the worker Session id in the answer because Session messages do not currently carry a first-class author-Session field.
6. **Resolve the exact todo.** After the answer has been successfully recorded in the coordinator Session, call `resolve_session_message(session_id=C, message_id=<todo_id>, ...)`. Resolution means the coordination item was handled; it is not an implementation acceptance verdict.
7. **Coordinator consumes only the bounded result.** The coordinator should use the answer, referenced artifact/document, and its own source revalidation. It should not import the worker's long transcript into its context.
8. **Close the worker Session when appropriate.** A completed one-shot worker can be closed explicitly. The coordinator Session remains independent.

A compact mental model is:

```text
coordinator C
  -> post todo
  -> human opens worker
worker W
  -> read handoff(C)
  -> read open todo(C)
  -> perform bounded work under W
  -> post answer(C, reply_to=todo)
  -> resolve todo(C)
coordinator C
  -> consume bounded answer/evidence/result path
  -> revalidate current source before consequential follow-up
```

## Todo contents

A useful todo is small and explicit. Prefer fields expressed in ordinary prose:

- goal;
- allowed scope;
- prohibited actions;
- the requested worker posture, such as read-oriented or write-capable, while recognizing that this is intent rather than completion truth;
- expected output: conclusion, findings, evidence, or an isolated result path;
- relevant commit/path identifiers when they are stable enough to be useful.

Do not put credentials, bearer tokens, private keys, secret values, or sensitive connection details in Session messages. The message board is ledger-backed task state, not a secret transport. Message mutations are recorded in the in-memory Session state and queued to the existing persistence path; a successful tool return does not by itself promise synchronous disk flush.

## Correlation and completion

`reply_to` is the correlation mechanism between an answer and the coordinator todo. It does not claim the todo, grant authority, or prove that only one worker acted on it.

There is no atomic claim in the first version. Manually assign one worker per todo. If several independent workers are desired, create distinct todos. Add an atomic claim primitive only after real dogfood shows workers racing for the same todo often enough that manual assignment is unreliable.

Completion is currently two explicit metadata operations: post the answer, then resolve the todo. This is acceptable while the workflow is still low-volume and manual. If repeated use shows that this pair is a persistent source of mistakes or boilerplate, prefer one narrow `complete_session_message`-style convenience over a general collaboration framework.

## Worktree, evidence, and result safety

The Session message board is not a filesystem lock, and a task label such as `read_only` is not authoritative outcome truth.

- Read-oriented review/analysis workers are the lowest-conflict parallel use case when several windows share one worktree.
- If independent concurrent writes are intended, prefer explicit isolation at the worktree/project layer; if a worker writes in a shared worktree anyway, it must report that fact and the coordinator must re-observe the resulting state rather than relying on the original todo.
- Session mode and guards describe effective state at a point in the workflow. They can be changed only through normal accepted Session/tool paths and never widen underlying project authority, but they should not be treated as proof that a worker remained read-only for its entire lifetime.
- Worker results should explicitly report material operations and deviations: mutations, shell/process execution, validation, external effects, or anything else that changes how the coordinator should interpret the result.
- A result path is only a reference. The coordinator must re-read/revalidate current source or artifact state before a consequential follow-up.
- Recorded tool/effect evidence and current workspace state are stronger evidence of what happened than the worker's requested posture alone; the worker's bounded report should accurately summarize that evidence rather than conceal or normalize deviations.
- Worker completion does not make `finish_coding_task` or any other advisory projection authoritative completion truth.

## What this deliberately does not build

Do not add any of the following merely to make the first manual workflow look more automatic:

- automatic worker spawning;
- scheduler or worker pool;
- generic WorkItem/lease framework;
- automatic todo claim;
- shared multi-window transcript;
- implicit cross-Session authority;
- concurrent same-worktree edit orchestration;
- automatic injection of worker history into the coordinator.

Each would add a new correctness or ownership model. Introduce one only for a demonstrated repeated problem that existing Session/message primitives cannot handle cleanly.

## Dogfood results

The first protocol dogfood on 2026-08-16 exercised the intended sequence against the existing runtime: coordinator todo -> fresh worker Session -> bounded handoff -> open-todo read -> independent inspection -> answer with `reply_to` -> explicit todo resolution. It used two independent Workflow Sessions in one host interaction.

A second dogfood used a physically separate ChatGPT window. The coordinator supplied only the coordinator Session/todo identifiers; after the worker finished, the coordinator recovered the result entirely from the board. The answer correlated to the exact todo, the todo was resolved, the worker Session id was discoverable from the bounded answer, and direct inspection of that independent worker ledger showed read/search activity with no write-like or shell-like project operations during the task.

That run also showed why `read_only` should remain an intent/guard concept rather than completion truth: the worker followed a read-only instruction while its Session remained in normal mode. This was not a correctness problem because the recorded operation history accurately showed what actually happened. Future workers may use `read_only` mode when useful, or later change effective Session posture through normal accepted paths; coordinator acceptance should remain based on actual recorded operations, current state, and an accurate bounded worker report.

The existing primitives were sufficient. Minor friction remains: answer and resolution are separate calls, and worker Session identity must be carried explicitly in the bounded answer when useful. Neither currently justifies a new runtime primitive, claim/lease system, or worker scheduler.
