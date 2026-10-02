# Typed closeout facts and projections

Closeout and diagnostic handoff share `CloseoutFacts` and `evaluate_closeout`.
The policy module contains no JSON values, serialization, ToolRuntime reference,
transport logic or filesystem operations. Its output is a `CloseoutDecision`.

`closeout_projection` adapts existing subsystem reports once and serializes the
existing public result structure. The finish workflow captures workspace and
hygiene observation success directly from the source ToolResults before building
its result. Handoff retains the original inspection status beside its bounded
workspace display, rather than inferring success from that display's legacy
`clean` fallback. Different result layouts cannot change the captured policy.

`Observation` distinguishes NotChecked, Failed and Observed. An unknown conflict
count stays optional internally; non-Git observations remain N/A. A successful
hygiene report intentionally omits zero counts and false flags, so its sparse
contract is retained. It is never applied to failed or missing reports. Failed
observations and incomplete Git conflict counts are explicit advisories, not
fabricated clean evidence.

TaskOutputs are retained as their typed observation from collection through finish;
there is no production serialization/deserialization round-trip. The closeout seal
uses a typed blocking result, including unverified typed outputs, rather than
parsing task_outcome/blocking from presentation JSON.

Current validation, historical failures and evidence integrity remain distinct.
Stale failures are not made current blockers, unproven observations are not
upgraded by rerunning, and read-only work is not forced to execute tests. Existing
JSON field names, bounded detail projections, authorization checks, ledger
reconciliation and the TaskOutputs schema are unchanged.

This is a bounded first step: the underlying Job/validation/hygiene report APIs
still emit JSON, and handoff-brief/continuation presentation has its own adapters.
The refactor does not claim to make every workflow report typed at its producer.
