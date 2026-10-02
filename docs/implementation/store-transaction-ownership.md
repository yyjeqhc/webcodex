# Store primitives and transaction ownership

## Internal foundation

`store_primitives` owns shared identity allocation, proof validation, streaming
JSON/text digests, principal validation, error conversion and operation-scoped
idempotency records. It does not import Conversation, Task, Wake, Wait or Database
operations. Consumers import the primitives directly rather than obtaining them
through `communication`. Existing public root exports and type names are retained.
The schema, digest domains, operation keys, IDs/proofs and stored bytes are not
renamed or migrated. `communication` continues to own participants, transcript,
Inbox and Endpoint access checks; the foundation is not an authorization shortcut.

## Explicit atomic completion

The direct TaskAttempt completion entry retains request validation, owned-task
lookup, replay checks and its immediate transaction. `agent_task::completion`
coordinates the current-attempt fence, terminal Task/Attempt writes, pre-dispatch
execution retirement, attention and Wait matches using that exact transaction.
The outer entry writes the replay receipt and commits only after all these steps
succeed. Helpers take `&Transaction`, not `Database`, and cannot independently
commit. `AttemptAuthority` remains the sole validated attempt selector.

Endpoint-loss controller fencing and endpoint-execution binding cleanup are now
AgentTask-owned transaction helpers. Wake reconciliation calls them before its
own state transitions, with the same generation predicates and statement order.
The existing prepared-dispatch grace path is not weakened. This is an incremental
ownership boundary, not a claim that every Task/Wake table read or delegated-run
reconciliation path has been redesigned.

## Regression contract

Existing principal isolation, replay/collision, proof/digest, endpoint-loss,
Task/Attempt and attention rollback tests remain applicable. A new forced Wait
insertion failure proves Task and Attempt terminal changes, already-created
attention/Wake state and the completion replay receipt all roll back together.
Retry with the same key commits once; subsequent replay cannot add Wait matches.
No asynchronous event replaces a transactional write, and no new ORM/framework or
crate is introduced.
