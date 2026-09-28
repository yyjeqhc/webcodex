# Structured validation success evidence

Baseline: `2532c294` (#765). Scope: synchronous terminal model projection only.
Canonical counts, assertions, diagnostics, source/target and Job/parser data remain complete; the existing synchronous policy-copy omission is repaired below.

## Proof inventory (before implementation)

| Category / canonical fact | Existing model representation | Independent decision / derivation |
| --- | --- | --- |
| Business: process and validator passed | outer success, canonical passed/lifecycle | Existing #765 projection owns lifecycle; this change requires canonical completed, passed, exit 0, no Job/recovery. |
| Executed tests | tests_detected, tests_run_count, tests_passed, tests_failed, zero_tests_run; diagnostics.test_summary | Keep tests_run_count only when detected, passed=count, failed=0, zero=(count=0), and complete diagnostic summary agrees. Go includes skipped tests in count: skipped cases cannot use this equivalence. |
| Assertion | require_tests, no_run, test_count_assertion.{minimum_tests,actual_tests_run,status,reason_code,evidence_reason_code} | Keep explicit minimum in test_count_assertion.minimum_tests; count already carries actual. Remove verdict/reasons only after checking passed/minimum_satisfied and complete count. Default positive test proof needs no policy echo. |
| Explicit zero acceptance | request require_tests=false | Keep require_tests=false with count=0 and no minimum. Missing policy is not acceptance. |
| Compile only | request no_run=true | Keep no_run=true; never replace absent executed count with 0. |
| Freshness | source_state freshness, observed_mutation_fence, start_fence | Keep unchanged. Even unproven/uncrossed does not certify current source. Stale/crossed and unknown stay rich. |
| Target | project_validate.validation_target_id | Keep: comparison identity used by Session validation_events and validation summaries, also Job validation consumers. |
| Selected implementation | project_validate backend/action/adapter | Keep runtime-selected adapter plus target; backend and action are uniquely implied by the verified adapter mapping (cargo_check=rust/check, cargo_test=rust/test, go_vet=go/check, go_test=go/test). detected_backend exists on unavailable planning, not terminal success. |
| Check result | warnings_count, errors_count | Preserve positive warnings. Zero counts may disappear only when both are known, error count is 0, and complete diagnostics contain no errors/loss. |
| Diagnostics | diagnostics list, failed details, test summary | Preserve real diagnostics. Empty lists, false loss flags and matching test summary carry no new happy-path fact. Nonzero ignored counts remain rich. |
| Parser bookkeeping | available, parser, reason, diagnostic_count, returned_diagnostic_count, diagnostics_truncated, invalid_diagnostics_omitted, failed_test_details_truncated, truncated | Omit only recognized complete parser output with consistent counts, no invalid omissions, no failed details, no truncation. Unknown/missing parser evidence stays rich. |
| Format check | source_state and bounded output | No further duplicate evidence to remove. |
| Ensure format | changed/state_changed | Mutation/retry/effect truth: unchanged, outside this change. |

## Verified baseline caveats

`resolve_cargo_test_minimum(None,None,None)` returns None. Exit-0 default
zero tests can therefore have ToolResult.success=true while Session classifies
it as `zero_tests_not_validation_proof` (inconclusive). Do not rewrite this
canonical verdict in a presentation change: retain rich evidence. An explicit
require_tests/min_tests assertion rejects zero with rich failure.

Baseline synchronous `apply_validation_projection_fields` did not copy
require_tests/no_run from the canonical parser projection. Session's validation
summary reads finished output policy, not the recorded request; consequently
explicit zero acceptance and compile-only incorrectly became inconclusive there.
The required integration test reproduced this. This change adds those two existing
fields to the existing copy list, preserving the policy before Session/Audit/model
projection. No parser, validation rule, identity or Session architecture changes.
This is a necessary policy-preservation fix, not a new proof or inferred verdict.

Go's parser counts pass + fail + skip; Cargo counts only pass + fail. Go assertion
`evidence_reason_code=no_complete_summary` is currently a Cargo-specific fallback;
complete Go JSON summary/count agreement, no invalid events and no truncation
supply the actual Go evidence. Do not redesign that parser taxonomy here.

The real project_validate dispatch currently participates in the generic potential
mutation fence (`observes_potential_mutation`), so its own active guard can yield
`unproven/unknown`. The existing Runner integration fixture reproduces this.
These receipts remain rich, including backend/action, as required for unknown
source evidence. Project byte fixtures below measure canonical uncrossed inputs;
they do not claim savings for today's unknown-fence dispatch. Changing source
classification is intentionally outside this work.

## Deterministic serialized receipt sizes

UTF-8 bytes of serialized ToolResult (including success/output/error), fixed
bounded stdout/stderr fixtures; not a transport framing or production benchmark.
Canonical is before late projection, with the repaired policy copy. Previous
replays the unchanged #765 sparsifiers and its old omitted-policy copy boundary.
Final uses the late ModelFacingProjection::Execution. Soft regression checks
require >150 bytes saved and <800 final bytes per fixture; these are test
budgets, never runtime/wire rejection limits.

| Fixture | Canonical | #765 model | Final model | Reduction vs #765 |
| --- | ---: | ---: | ---: | ---: |
| cargo_check clean | 840 | 426 | 105 | 75.4% |
| cargo_check warnings | 976 | 596 | 328 | 45.0% |
| cargo_test ordinary | 1051 | 671 | 255 | 62.0% |
| cargo_test min | 1211 | 831 | 299 | 64.0% |
| cargo_test zero | 1067 | 665 | 274 | 58.8% |
| cargo_test compile | 926 | 498 | 119 | 76.1% |
| go_test ordinary | 1136 | 756 | 342 | 54.8% |
| project Rust check | 955 | 541 | 186 | 65.6% |
| project Rust test | 1323 | 943 | 335 | 64.5% |
| project Go check | 948 | 534 | 181 | 66.1% |
| project Go test | 1405 | 1025 | 419 | 59.1% |

## Final model receipts

All compact receipts retain source_state verbatim, including start_fence when
present. Nonempty bounded stdout/stderr and line counts retain #765 behavior.
The following shows the changed business fields, in addition to those common
fields and any invocation sidecars:

| Validator / policy | Before | After |
| --- | --- | --- |
| cargo_check, clean | warnings_count=0, errors_count=0, empty diagnostic scaffold | No count/scaffold fields |
| cargo_check, warnings | warning/error counts and full diagnostic scaffold | warnings_count>0, diagnostics.diagnostics containing the actual warnings |
| cargo_test, ordinary | detected, count, passed, failed, zero flag, diagnostic summary/scaffold | tests_run_count=N |
| cargo_test, explicit min_tests | above plus five-field assertion | tests_run_count=M, test_count_assertion={minimum_tests:N} |
| cargo_test, require_tests=false, zero | zero-count tuple and scaffold; policy was dropped by synchronous copy | tests_run_count=0, require_tests=false |
| cargo_test, no_run=true | false detection and null counts/scaffold; policy was dropped by synchronous copy | no_run=true, no executed-test count |
| go_test, all passing | detected, count, passed, failed, zero flag and summary/scaffold | tests_run_count=N |
| project_validate, uncrossed check/test | adapter/backend/action/target plus corresponding validator fields | adapter + validation_target_id plus corresponding sparse proof |
| cargo_fmt check / mutation | existing #765 receipt | Unchanged, including changed/state_changed for ensure-format |

Explicit min_tests=1 remains explicit; require_tests=true's intrinsic minimum=1
is redundant with positive proven count. Explicit require_tests=false on a
positive-count result also adds no proof and is omitted. Compile-only never
gets a fabricated zero count. A requested minimum that fails or cannot be proven
retains the full assertion/verdict/reason. Default zero-count process success
remains rich and Session-inconclusive; this work does not turn it into a new
ToolResult failure.

Actual warnings remain actionable. diagnostic_count/returned count are omitted
only when checked against the retained complete list (or the duplicate zero
failed-test summary); no invalid omission, truncation, failed detail or ignored
count is hidden. Parser name/reason and false/empty scaffolding are not separate
business proof. A partial Cargo harness, incomplete stream, contradictory count,
Go skip, unproven assertion, stale or unknown source prevents deduplication.

There is no fresh=true alias. Unproven/uncrossed still cannot prove current source
coverage; stale/crossed and unproven/unknown remain explicit and rich. Target
identity stays available for Session/Job comparisons; runtime adapter identifies
both backend and action. No changes to later Job observation/attention projection,
execution lifecycle, pending, readiness, retries, telemetry tables or protocols.

## Validation and review

Focused package test builds cover the implementation and schema integration:

- `cargo test --locked -p webcodex --lib validation_success -- --nocapture`:
  receipt fixtures, positive/negative sparse shapes, MCP tools/list outputSchema,
  API/MCP byte telemetry parity, privacy-safe audit, real dispatch/Session
  ordinary/minimum/explicit-zero/compile-only policy.
- `cargo test --locked -p webcodex --lib validation_handoff`: synchronous and
  pending/error/timeout/uncertainty/Job paths, Go skip/failure evidence, resolved
  project identity, canonical Session assertions and formatting effect truth.
- `cargo test --locked -p webcodex --lib execution_projection`: existing #765
  canonical-before-projection behavior and unaffected shell/lifecycle contracts.
- `cargo test --locked -p webcodex --lib validation_summary`: Session evidence
  grouping/comparison and summary behavior.
- `cargo test --locked -p webcodex-tool-contracts --lib output_schemas`: closed
  published schemas, including compact-assertion rejection on prestart/unknown.
- `cargo test --locked -p webcodex-tool-runtime-contracts --lib canonical_execution_audit_projection_tests`:
  bounded warning/error/test counts, assertion verdicts and lifecycle/recovery
  classifications, with no raw output/command/diagnostic/fence identity leakage.

Separate diff review checked canonical recording order, authority/effect scope,
loss/staleness gates, sparse-shape ambiguity and rich exception schema retention.
The review also tightened the published rich-success schema so `source_state` is
mandatory for every non-format validation success, and `project_validate` rich
success additionally requires its canonical backend/action/adapter/target identity;
Runtime already emitted those facts, so this closes schema drift without changing
model receipts. No real-process/E2E, release build, deployment or external mutation
was needed.

The three propositions remain distinct: terminal validator success; sufficient
executed-test or compile-only/explicit-opt-out proof; and current source coverage.
Only the first two can be proven by a compact receipt. source_state continues to
state that the third is unproven (or stale on rich paths).

Final focused results: validation_success 7 passed; validation_handoff 43 passed;
execution_projection 6 passed; validation_summary 14 passed; output_schemas 59
passed; canonical_execution_audit_projection_tests 3 passed; ToolDefinition
metadata/descriptions (`cargo test --locked -p webcodex-tool-contracts --lib definitions`)
23 passed. Counts overlap across filters. Final Rust formatting and whitespace
checks passed. Initial development failures were resolved (missing import/match,
old receipt assertions, synchronous policy loss, and an MCP fixture incorrectly
assuming project_validate was directly advertised). No known remaining test
failure in these focused lanes; full workspace/E2E was not run.
