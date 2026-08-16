# Manual Multi-Window Collaboration

This document defines the first supported pattern for manually splitting one coding task across multiple ChatGPT windows or agents. It deliberately reuses the existing Workflow Session handoff and message-board primitives. It is not a scheduler, worker pool, claim service, or shared-transcript design.

## Scope

Use this pattern when a coordinator wants an independent worker to perform a bounded subtask such as analysis, review, investigation, or a clearly isolated deliverable.

The first version is manual:

- a human opens the additional window or agent;
- coordinator and worker keep separate Workflow Sessions;
- the coordinator Session owns the durable todo and receives the bounded result;
- the worker Session owns the worker's tool/evidence history;
- the worker reads a bounded coordinator handoff instead of inheriting the coordinator transcript;
- no primitive in this workflow grants filesystem authority, task ownership, or a concurrency lease.

Do not use this protocol as permission to concurrently mutate the same worktree from multiple windows. Prefer read-only workers. If a worker must modify files concurrently, give it an explicitly isolated worktree/project rather than relying on message state for filesystem coordination.

## Canonical flow

Assume the coordinator has Session `C` and the worker will create Session `W`.

1. **Coordinator posts one bounded todo to `C`.** Use `post_session_message(kind="todo")`. The todo should contain the objective, narrow scope, important prohibitions, and expected result shape. Include a result path only when the worker is explicitly expected to produce an isolated document or artifact.
2. **Open the worker as a fresh Session.** The worker should normally start a new Workflow Session `W`; do not resume `C` merely to delegate work. Separate Sessions keep the worker's tool history and reasoning context independent.
3. **Worker reads the coordinator handoff.** Call `session_handoff_summary(session_id=C, ...)`, then inspect the relevant open todo with `list_session_messages(session_id=C, kind="todo", status="open")` or the bounded discussion summary. The handoff is continuity evidence, not transcript replay.
4. **Worker performs the subtask under `W`.** Use `W` for project reads, validation, or other worker-local evidence. The worker must obey the same project, authority, guard, and tool contracts as any other Session; the coordinator todo does not widen them.
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
- whether the worker is read-only;
- expected output: conclusion, findings, evidence, or an isolated result path;
- relevant commit/path identifiers when they are stable enough to be useful.

Do not put credentials, bearer tokens, private keys, secret values, or sensitive connection details in Session messages. The message board is durable task state, not a secret transport.

## Correlation and completion

`reply_to` is the correlation mechanism between an answer and the coordinator todo. It does not claim the todo, grant authority, or prove that only one worker acted on it.

There is no atomic claim in the first version. Manually assign one worker per todo. If several independent workers are desired, create distinct todos. Add an atomic claim primitive only after real dogfood shows workers racing for the same todo often enough that manual assignment is unreliable.

Completion is currently two explicit metadata operations: post the answer, then resolve the todo. This is acceptable while the workflow is still low-volume and manual. If repeated use shows that this pair is a persistent source of mistakes or boilerplate, prefer one narrow `complete_session_message`-style convenience over a general collaboration framework.

## Worktree and result safety

The Session message board is not a filesystem lock.

- Do not let coordinator and worker concurrently edit the same worktree.
- Read-only review/analysis workers are the default safe parallel use case.
- Concurrent write workers require explicit isolation at the worktree/project layer.
- A result path is only a reference. The coordinator must re-read/revalidate current source or artifact state before a consequential follow-up.
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

## Initial dogfood result

The first protocol dogfood on 2026-08-16 exercised the intended sequence against the existing runtime: coordinator todo -> fresh worker Session -> bounded handoff -> open-todo read -> independent read-only inspection -> answer with `reply_to` -> explicit todo resolution. It used two independent Workflow Sessions in one host interaction; it did not validate the ergonomics of physically opening and switching between separate ChatGPT UI windows.

The existing primitives were sufficient for the protocol flow. Two minor friction points were observed: answer and resolution are separate calls, and worker Session identity must be carried explicitly in the bounded answer when useful. Neither blocked the workflow, so this phase adds documentation rather than a new runtime primitive. Real multi-window dogfood remains the next source of evidence for UI/operator friction.
