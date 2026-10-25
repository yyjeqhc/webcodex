# Native Runtime and CLI validation for the tool-experience series

Date: 2026-10-25. This records one successful isolated native Server/Runner run,
plus replay of its exact Runtime responses through the preserved before/after
CLI projections. It supplements the [Runtime adapter comparison](cli-runtime-compaction-validation.md)
and [long-task assessment](long-task-runtime-assessment.md).

## Result and provenance

The successful run used clean code commit
`d7a8b6eec06db49b46ab01e4018b66c888085a16`. The latest fetched
`origin/main` remained `f387d0a19f481bba0ead3bc2dd6b89581f074536`.
The default dev profile reused the existing `target/debug` cache. The build
completed with no compiler warnings/errors.

| Native binary | SHA-256 |
|---|---|
| Server | `1927d30a922db0b3a9a81193505e655b12a15f476015d0eccc43464ea1353557` |
| Runner | `e633d5158a16538222d055c0efcc8a3458a2f331968af08024c531733edd331e` |

**32 CLI samples passed; two Adaptive captures succeeded; all four replay
executions passed** (two captures times two compiled projections).
The [machine-readable evidence](cli-native-runtime-validation.json) includes
all 32 actual MCP responses, their text/HTTP byte counts, binary provenance,
original response hashes and safety oracles. Sample text is unnormalized:
opaque Job IDs, observation tokens, revisions, timestamps and fixture paths are
preserved.

Only a disposable loopback Server, WebSocket Runner, temporary database,
credentials and fixture projects were used. `WEBCODEX_ENV_FILE=/dev/null`
blocked default repository/system env-file loading. The Rust project had no
external dependencies and Cargo was offline for rust-analyzer. The harness
closed its own services in `finally`; no production configuration, identity,
Server, Runner or research data was changed.

## Actual coverage

| Area | Successful native checks |
|---|---|
| Discovery and files | Project inventory, work_on_project bootstrap, file enumeration, Git directory folding without duplicate slashes |
| Search | Ordered match/context lines, compound search plus versioned coalesced file reads |
| Versioned edits | Read revision, dry-run hash unchanged, actual edit applied, stale revision rejected with unchanged file and complete reread recovery |
| Shell | Plain successful stdout, exit-7 failure, explicit Workflow Session cwd/Bash, one-second timeout and partial output |
| Native Process/Script | Default success, explicit cwd, failed execution retaining cwd and diagnostics; Script language remains visible |
| Jobs | Queued receipt, same-Job readiness/observation, unknown-plus-valid partial batch, truncated logs/cursor evidence |
| Timeout policy | One-second Job timeout with Worker reason; 90000-second request visibly capped by the registered 3600-second Runner policy |
| Fixed credentials | Real PAT scope denial is HTTP 200 / MCP isError=true, with job:run, execution_started=false and no side-effect file |
| LSP | A nonempty real rust-analyzer workspace symbol with filename, range, kind and kind_code; CLI and Adaptive both ready |

The 22-sample Runtime adapter suite additionally covers injected
`outcome_unknown`, unknown read revisions, oversized/truncated file reads and
exact read recovery. Those cases are not claimed as native disconnection or
multi-hour experiments. The native Job log sample itself retains
`stdout_truncated: true`, returned/total line counts and cursors.

An observation request can succeed while the observed Job failed. For the
native timeout sample, `observe_jobs` has MCP `isError=false`, while the
text explicitly says `status: timeout`, `command_execution_state: timed_out`
and `exit_code: -1`. Readiness is never presented as execution success.

## Same-response before/after comparison

The producer is the successful final native run. Each saved Adaptive response
was fed unchanged to the two preserved PR3 test binaries. The before binary
contains the pre-compaction production renderer plus test-only capture support;
the after binary contains the PR3 renderer. Their full source/build provenance
is in the [adapter validation](cli-runtime-compaction-validation.md).

| Native Runtime output | Before characters / UTF-8 bytes | After characters / UTF-8 bytes | Byte reduction |
|---|---:|---:|---:|
| Two authorized projects | 637 / 637 | 469 / 469 | 168 (26.37%) |
| One real workspace symbol | 389 / 389 | 58 / 58 | 331 (85.09%) |

Raw artifacts:

- Projects: [Runtime capture](cli-native-runtime-captures/projects.runtime.json),
  [before text](cli-native-runtime-captures/projects.before.txt),
  [after text](cli-native-runtime-captures/projects.after.txt).
- Symbols: [Runtime capture](cli-native-runtime-captures/workspace_symbols.runtime.json),
  [before text](cli-native-runtime-captures/workspace_symbols.before.txt),
  [after text](cli-native-runtime-captures/workspace_symbols.after.txt).

Project output retains both complete IDs, compact refs, names, paths, online
state, differing Git capabilities and source. Only null description and the
exact null-profile/not_configured pair disappear.

The actual symbol output is:

```text
src/lib.rs:1:8-1:22 function fixture_symbol
kind_code: 12
```

The capture contains one returned result, no truncation and zero external or
invalid omissions. It has no index-state field. This success sample does not
prove the nonzero-loss or indexing-failure branches; their distinct regression
tests remain necessary.

The separately requested native CLI responses happened to be byte-identical
to the replay-after texts for both tools. The requests remain independent:
the A/B claim uses the same saved Adaptive response, not an assumed identical
cold/warm LSP state.

## Budget and recovery evidence

The real capped Job receipt preserved:

```text
status: queued; execution_state: queued
timeout_limit_source: runner_policy
timeout_notice: Dispatch budget only; Runner enforces its current policy.
timeout_policy_basis: last_registered_runner_policy
command_started: false
execution_lifetime: runner
requested_timeout_secs: 90000
effective_timeout_secs: 3600
```

Its complete observe_jobs continuation retains the original Job ID, opaque
token, wait_secs=5 and wake_on=terminal. Startup, readiness and completion all
refer to that same Job. The payload was an immediate printf, so this verifies
admission and execution identity, not a one-hour lifetime guarantee.

The one-second timeout case retained `job-timeout-partial` and the actual
Worker error `job timed out after 1 seconds`. Shell timeout output also
retained that execution started, completion was not proved, and side effects
must be inspected before retry. No unknown-result command was redispatched.

The Runner registration snapshot can differ from the policy captured by a
hot-reloaded Worker. Final Worker budget acknowledgement remains outside this
series. The receipt exposes that evidence boundary; no Runner cap was raised.
See the [long-task assessment](long-task-runtime-assessment.md) for ownership,
restart boundaries, terminal retention and training-artifact persistence.

## Independent verification and measurement limits

A separate read-only review recomputed all 32 text/Unicode/UTF-8 measurements,
compared each text with MCP content, reconstructed compact response lengths,
and checked both capture hashes and all four binary hashes. It also parsed
the recovery JSON and verified Job identity, explicit cwd/Shell, timeout
reason, scope denial and log-truncation evidence.

These are final CLI text bytes and captured JSON body bytes. Pretty capture
file size, replay JSON serialization size and HTTP body size are distinct.
They are not Host tokens, typical-task tool-call counts or end-to-end latency.
The 32 fixture samples are not a claim that a user task needs 32 calls.
Native fixture paths/configuration differ from the local adapter fixtures, so
their absolute sample sizes are not interchangeable.

The fixed-credential error currently repeats its recovery advice in the
headline and recovery field (385 bytes). Explicit full Job observations also
retain optional metadata nulls. These are documented remaining presentation
opportunities; the isolated unknown-job null line and duplicate complete
recovery call are fixed without globally deleting unknown evidence.

## Reproduction

Run from the repository root. Output directories must not already exist.
Use the preserved projection binaries, or rebuild their documented source
variants; their temporary local paths are not permanent distribution URLs.

```sh
cargo build --locked -p webcodex -p webcodex-runner \
  --bin webcodex-server --bin webcodex-runner
python3 scripts/e2e_cli_runtime_output.py \
  --bin-dir target/debug --output-dir /tmp/webcodex-native-new
python3 scripts/measure_cli_runtime_projection.py \
  --before-test-bin "$BEFORE_TEST_BIN" --after-test-bin "$AFTER_TEST_BIN" \
  --output-dir /tmp/webcodex-replay-new \
  /tmp/webcodex-native-new/projects.runtime.json \
  /tmp/webcodex-native-new/workspace_symbols.runtime.json
```

The replay command can also use the two committed capture files. It starts no
Server/Runner. Real rust-analyzer and rust-src must already be installed for
the successful native symbol case; the harness does not install dependencies.

## Validation history

The first native run at `1019a324` completed six CLI samples, then stopped
because the capture helper reused the CLI direct-tool inventory for an
Adaptive request. The original rejected reply was not saved, so its exact
HTTP/RPC rejection is not claimed as observed. Source tracing identified the
route mismatch. The fix separately discovers Adaptive routes and adds bounded,
credential-checked failure diagnostics while keeping all successful-result
assertions. This incomplete attempt is not included in the successful totals.

The final native build also includes the PR4 CI follow-up: old Runner
diagnostic assertions and MCP schema expectations were aligned, and an actual
compact-description defect was fixed. The observation description now remains
407 characters within the existing 420-character budget; no budget was raised.
Local follow-up validation passed 59 MCP tool tests and 49 Runner navigation
tests (three external-language-server tests remained explicitly ignored).
