# Project-validation test selection and count requirements (#599)

## Current tree note

The current tree has advanced beyond the historical slice documented below. Rust/Go
`project_validate` now supports shared package scope, `all_packages=true` where the
Runner can prove the complete project unit, and `dependency_policy.mode=locked`.
Python `project_validate` supports `action=test` through pytest, including bounded
`test.filter` mapped to pytest `-k`; Python still rejects package scope and dependency
policy. Node remains unavailable. `project_build` remains a separate Rust/Go gateway.
The historical implementation record below is retained as the state of that slice.

## Status and bounded scope

This is one follow-up slice of #599, based on `1006a48206f9a9704114bbad6481f145e486928c`.
The issue is deliberately not closed by this change. #749 supplied portable
Rust/Go validation, #760 added package scope, and #766 centralized neutral recipe
facts in `webcodex-workspace`. This patch extends that path; it adds no tool,
Direct descriptor, adapter registry, crate, external dependency or scheduler.

That validation slice deferred `project_build`. The current tree now has a
separate Rust/Go `project_build` v1 gateway with Runner-owned recipe planning,
typed `StartBuild`, manifest/lock provenance, and same-Job admission fencing.
Build profile/target/artifact identity, mutating `project_format`, lint,
production Node and additional Python adapters, workspace/exclude, and offline/network
policies are tracked as additive lifecycle extensions in #962. Existing lower-level Cargo/Go tools retain
their options and default behavior. No claim of complete CLI parity is made.

## Request and planning

Optional `test` is accepted only with `action=test`. Its closed fields are
`filter`, `require_tests`, and `min_tests`. Omission preserves the prior policy:
no test filter, at least one proven executed test. An empty block has the same
execution intent but is an additive request and requires the new capability.

Filters are bounded to 200 UTF-8 bytes without control characters. Rust uses the
existing trimmed libtest substring normalizer and rejects option-like filters.
Go uses native `-run` regexp, without trimming meaningful whitespace. Go validates
regexp syntax itself; this layer does not implement a subtly different regexp
engine or split subtest expressions. Empty/omitted filters produce the original
unfiltered plan. The operator must understand that Go regex selection (and parent
subtest execution) is not equivalent to Rust substring selection.

The canonical semantic operation owns filter normalization and supplies both
argv and target identity. The recipe layer delegates to it, and the Runner uses
the same operation for the resolved target. Supported forms are ordinary Cargo
`test <substring>` plus existing flags/package selectors, and Go
`test -json [-run <regexp>] <bounded packages>`. No raw flags, executable, shell
string, environment, install step or extra input is admitted.

The shared recipe resolver still owns nearest-root/ambiguity/containment and
source digests. The Runner's retained provenance includes the original semantic
test request; admission replans it and compares the exact recipe/step/digests.
Changing a filter changes invocation and target identity. Count-policy changes
change retained request provenance but not argv or validation target identity.
Unfiltered existing target identities and omitted-field wire shapes stay stable.

## Evidence policy is not execution success

`require_tests` defaults true. Effective minimum is explicit `min_tests`, else
one when tests are required, else no minimum. Explicit false does not override an
explicit minimum. The upper bound is the existing 1,000,000 count-assertion bound.
A requirement cannot be attached to check/format_check, and there is no no_run
or compile-only mode on this gateway.

The existing Job metadata retains the effective requirement and minimum, checked
against retained request provenance rather than hardcoded project defaults.
The existing test parser, count assertion and source-evidence paths remain the
owners of results. A process can finish successfully yet not satisfy a requested
count. Unknown/truncated evidence must not be relabelled as a proven zero count.
A positive historical validation never certifies current external source state.

## Mixed Runner versions and dispatch

`project_validation_test_options_v1` is additive, false when omitted and never
inferred from the protocol version, old project-validation support or package
scope. The running Runner binary advertises it, not generated static config.

The Registry checks it under its Runner lock before sending a planning request
with any test block. Job admission checks it again for that retained provenance
and for filtered Go argv. A Runner replacement cannot reuse an earlier planning
success to bypass the current capability. Old callers omit the new field and
retain the original request shape; old Runner registrations do not acquire it.

This does not change authority, same-execution Job identity, timeout lifetime,
current-turn handoff policy, cancellation or retry rules. A Host delay or lost
response never grants permission to repeat validation. The original issue's
55-second example is not a new hardcoded sync wait; actual return timing remains
the existing transport policy, separate from total execution timeout.

## Test boundaries

Regression coverage includes closed/bounded schema and runtime input, legacy
wire omission, native filter normalization, default/filtered target identity,
recipe/operation plan parity, request count-policy versus Job metadata, count
failure despite exit zero, explicitly allowed zero, old-Runner pre-planning
rejection, rechecked Job admission and same-Job terminal observation. Existing
Node/Python and unsupported scope failures remain unchanged.

Tests use existing bounded fixtures; they do not run the installed Runner's new
feature (it remains the deployed base binary) or modify the user's repositories.
Commands and exact executed counts are recorded in the PR after final source
review. No broad benchmark or native cross-platform acceptance is implied.

## Validation results

On oe, using the new isolated worktree and the final implementation:

| Boundary | Command / selection | Result |
| --- | --- | --- |
| Server handoff and admission | `cargo test --locked -p webcodex --lib project_validation` | 9 passed |
| Core request and protocol | `cargo test --locked -p webcodex-core --lib project_validation` | 5 passed |
| Runner planning and fences | `cargo test --locked -p webcodex-runner --bin webcodex-runner project_validation` | 8 passed |
| Validation adapters/evidence | `cargo test --locked -p webcodex-validation --lib` | 108 passed |
| Second admission capability check | `cargo test --locked -p webcodex-runner-registry --lib project_test_options` | 1 passed |
| Canonical tool/schema contracts | `cargo test --locked -p webcodex-tool-contracts --all-features --lib` | 268 passed |
| Canonical argv | `cargo test --locked -p webcodex-core --lib filter_canonical` | 11 passed |
| Historical target identities | `cargo test --locked -p webcodex-core --lib validation_identity` | 6 passed |

The core selections overlap in one existing test; these are execution counts,
not an additive unique-test total. Server/core were initially selected together;
Runner has a binary test target and was explicitly verified with `--bin` rather
than treating its absent `--lib` target as validation. Final formatting,
whitespace and the 21-package dependency-boundary checks passed.

All intermediate implementation failures were corrected: a missing public
normalizer export, the now-obsolete Go-filter rejection assertion, and an
expanded description exceeding the unchanged 1,024-character contract. The final
description is 884 characters; the budget was not widened. No tests were ignored
or removed to hide failures. Full Server/workspace suites, native Windows/macOS,
and real Cargo/Go project end-to-end execution were not run. The deployed Runner
is not upgraded by this PR; the feature is verified in source-level adapters,
Runner tests and Server/Registry lifecycle fixtures.

## Design constraints from #599

A small number of lifecycle gateways remains preferable to exposing every CLI
verb or one universal `project_task`. But a closed adapter is not an OS sandbox:
Cargo build scripts, tests and future package-manager scripts are project code,
not inherently free of filesystem/network effects. Host annotations are not an
authorization bypass. Native format mutation and future build artifact handling
need their own reviewed semantics instead of being appended as arbitrary args.
