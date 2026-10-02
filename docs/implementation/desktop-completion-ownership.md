# Desktop operation completion ownership

The admission model remains serial: `OperationController` permits one mutation,
`begin_operation` takes the Core without retaining its mutex, and ordinary reads
use the independently published snapshot. No new parallel operation admission or
cleanup callback is introduced.

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
