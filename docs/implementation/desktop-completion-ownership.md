# Desktop operation completion ownership

Desktop admits one mutation at a time. The admission guard owns its exact
operation ID from admission through completion. Before dispatch, the supervisor
baseline and Core checkout share a cancellable 30-second wait. Dropping the
request during that wait releases admission. Ordinary reads can observe the
independently published snapshot while a mutation owns the Core.

Runner settings reads capture their target, release the Core before supervisor
and file I/O, then check the target and operation generation before returning.
A stalled inspection has a 30-second caller deadline. Its file worker retains
the single reader permit until it exits, so repeated reads stay bounded.

After dispatch, loss of the guard during a panic, task abort, or failed Core
return records the operation ID and closes mutation admission for this Desktop
process. Its snapshot reports an internal state error and directs the user to
fully quit Desktop, reopen it, and verify the outcome. A worker or external
effect can outlive a dropped future. Successful completion returns the same
Core before releasing admission. Failure cleanup and Core return each have a
30-second limit on lock acquisition, and the supervisor retains its existing
per-process cleanup deadlines.

Cooperatively cancellable operations have a ten-minute outer deadline. The UI
reports an overdue operation at that deadline, or after 30 seconds of waiting
for an explicit stop. A live owner retains its slot through settlement,
including during migration and update. Completion clears the overdue status.

Completion now separates two capabilities. AppState owns the supervisor, exact
process-generation baseline, cancellation normalization, Core return and admission
release. The `state::operation_completion` module owns an exhaustive typed policy
and can modify only `DesktopStateSnapshot`; it cannot access DesktopCore, provider
stores, configuration, the adapter or the process supervisor. Failure readiness
reconciliation likewise accepts only a snapshot, baseline snapshot and observed
cleanup result. Existing callers of start-state terminalization use this same
snapshot-only helper.

Migration and Desktop update explicitly select their durable coordinator as the
recovery owner. They never request generic new-generation cleanup: restoration or
unknown-result state belongs to those coordinators, and a restored original
process must not be killed by a broad cleanup pass. Other failed operations still
reclaim only newly owned generations with the original stop order. Completion
retains the original two publication phases, cancellation handling, baseline
restoration and RuntimeSwitch rollback/recovery diagnostics.

Tests cover every operation kind's cleanup ownership, migration/update
cancellation, cancelled observations, kind-scoped error publication and successful
switches that preserve rollback/recovery diagnostics. Existing serial-admission,
observable-cancellation and runtime-resume tests remain in place.

This is the bounded completion-state boundary from the architecture audit, not a
claim that all DesktopCore configuration/provider/runtime fields have independent
owners yet. Broader state decomposition should follow actual operation conflicts;
it is not a reason to weaken admission or process-generation checks now.
