# v0.5 structural boundaries: detached ownership and request tracing

## Scope and baseline

This round continues the existing oe branch after the user's rebase, at baseline
`185b0ebe452de0c09d50766c13f2761c3c82fad2`. It is code placement, not a new
execution architecture, runtime protocol, permission policy or tool-name change.
No dependency, package version, release workflow or production configuration is
changed. These two modules have independent commits and independent ownership
boundaries; they are not merged into a common abstraction.

The initial checkpoint also audited the four large Git/file/bootstrap/transport
test files, ToolCall, Desktop state, AgentTask Attempt authority, and deeper
AgentTask store layering. At that checkpoint the Host had blocked two mechanical
move scripts, so those files were intentionally left unchanged while Detached Job
and Trace landed independently. The follow-up continuation described below
completes those remaining structural boundaries without changing product
capabilities or public tool behavior.

## Detached Job

The existing `detached_job.rs` module remains the caller-facing facade. There is
no new trait registry, terminal Session, process manager or execution identity.
Its implementation is organized as:

```text
detached_job.rs             existing module identity and facade
  detached_job/
    model.rs                records, limits, validation, transitions, snapshots
    store.rs                bounded persistence, retention and restart recovery
    supervisor.rs           shared handoff and acceptance protocol
    platform/
      mod.rs                compile-time platform selection
      unix.rs               locks, Unix process-tree ownership, watchdog, I/O
      linux.rs              Linux start identity and liveness
      macos.rs              Darwin start identity and liveness
      windows.rs            handles, creation time, file replacement, Job ownership
      other_unix.rs         existing explicit unsupported identity behavior
      unsupported.rs        existing unsupported-platform filesystem behavior
    tests.rs                existing test module and subprocess fixture identity
```

Whole platform-gated functions move under equivalent module gates. Shared
supervisor sequencing and platform-dependent fallback ordering stay unchanged.
Some small conditional setup remains in the shared supervisor/store; extracting
whole OS implementations does not justify rewriting those decisions or pretending
Windows and Unix ownership are the same operation.

The serialized record schema, phase order, execution fingerprint, start-identity
validation, lock ownership, no-follow/path checks, atomic persistence, supervisor
acceptance boundary, timeout/cancellation and retained output limits are unchanged.
Windows breakaway fallback still occurs only after the same definite pre-start
failure; no new retry or cleanup policy is introduced. Publicly consumed types
and functions keep their existing facade names. Cross-child implementation
visibility is restricted to this subsystem, not expanded to a public API.

The real-process test entrypoint remains
`webcodex_runner::detached_job::tests::internal_mode_subprocess_entrypoint`.
Moving this entrypoint without updating its exact child-process selector would
turn an apparently mechanical test split into a behavior change; this round
leaves that namespace and its source intact.

## Request tracing

The existing `tool_request_trace.rs` keeps public entrypoints, shared safe
measurement helpers, configuration access and process-wide capture health.
`ToolRequestLifecycle` and the existing request/job observation functions remain
available through the same facade paths. New internal files own:

- `writer.rs`: the one bounded writer queue/thread and enqueue/flush paths;
- `store.rs`: permissions, accounting, retention, reconciliation and persistence;
- `payload.rs`: bounded forensic payload/index read and validation;
- `correlation.rs`: request/job associations, expiry and terminal cleanup;
- `lifecycle.rs`: request guard, timings, effective-argument/evidence capture;
- `tests.rs`: the former inline regression module, without changing its namespace.

The existing `reader.rs` and `diagnostics.rs` remain separate. Constants and
process-wide storage are moved, not duplicated: there is still one queue, one
store-accounting mutex and one correlation map. Trace failures remain fail-open
with respect to the business operation. Metadata/full modes, body omission,
redaction/provenance, limits, deadlines, file permissions, retention and structured
event fields do not change. Rust source locations and implicit logging callsite /
module paths naturally follow the new files; those are not durable identities. Diagnostic observation does not become execution proof.

## Review method

A temporary, isolated syntax-inspection helper uses the locally cached Rust parser
to record complete item boundaries and function/method bodies before moving code.
It is not a workspace member or a new product dependency and is not committed.
Edits are fenced against exact original source snapshots. Module moves preserve
function bodies; only parent-qualified paths and minimum intra-subsystem visibility
change where the Rust module boundary requires them.

The independent before/after comparison covers all **142 Trace** and **89 Detached
Job** functions/methods, including the former inline Trace tests and the Windows /
macOS native bodies. Multiplicity is checked, so duplicating one implementation
cannot compensate for losing another. Canonical constants/record declarations
and platform gate placement are reviewed separately. Body equivalence is useful
refactoring evidence, not a substitute for compilation or native testing.

The original compiled Server and Runner test inventories were captured before
source changes. Final inventories must match exactly; no removed, ignored,
renamed or silently disconnected tests are accepted. Cross-platform source-body
preservation is not represented as native Windows/macOS execution.

## Verification

Final Linux validation on frozen, formatted implementation source:

| Check | Result |
|---|---:|
| Trace targeted library tests | 44 passed |
| Detached Job real-process/domain tests | 23 passed |
| Complete default Server library | 3,152 passed, 3 existing ignored |
| Complete default Runner binary tests | 975 passed, 4 existing ignored |
| Compiled Server test inventory before/after | Identical 3,155 identities |
| Compiled Runner test inventory before/after | Identical 979 identities |
| Function/method body comparison | Identical 142 Trace + 89 Detached bodies |
| Type/constant/static declaration comparison | Identical 29 Trace + 41 Detached declarations |

Targeted counts overlap the full suites and are not additional unique tests.
The declaration comparison excludes only necessary local visibility and outer
platform gates moved to the equivalent module declarations. The production
Server/Runner `cargo check --locked` passed without warnings. Source hashes are
recorded before final validation and rechecked before commit; tests were neither
renamed, newly ignored nor removed. The original Detached subprocess test target
remains unchanged. Initial extraction compile errors were limited to a missing
`cfg(test)` on a re-export and redundant imports; their resolution did not modify
runtime logic or test expectations.

The Detached facade is now **69 lines** (previously 3,174); its largest extracted
platform file is 698 lines. The Trace facade is **261 lines** (previously 3,362),
with the existing 1,284-line test body now in its own module. This is redistribution,
not elimination of the implementation or a hard line-count rule.

No native Windows/macOS compile/execution, full all-features workspace suite,
MCP conformance referee, paid model, deployment, production service operation or
state migration was performed. Native platform body/gate comparison does not
substitute for the platform CI lanes before any eventual merge or release.

## Follow-up: remaining v0.5 structural cleanup

The continuation starts from `d8ccd1d31bde25143170ec8f191a39bcdbffb117`
and keeps the existing `feat/v0.5-tool-surface-standardization` branch. It adds
six reviewable commits after the earlier Detached Job and Trace work:

| Commit | Boundary |
|---|---|
| `e6b93b67` | split the four oversized runtime/Runner test sources by behavior |
| `df2fb3f2` | split ToolCall input types, canonical enum, and parsing/normalization |
| `a4b95f29` | model the exact AgentTask Attempt authority tuple |
| `abbaa8d9` | separate Desktop state facade/core/readiness/persistence/tunnel/tests |
| `5e176f4d` | split AgentTask models and invariants from the facade |
| `837c10b6` | track the AgentTask operations slice under a non-ignored filename |

### Same-module physical splits

The large tests, ToolCall, Desktop state, and the final AgentTask file split use
same-module `include!` boundaries. This keeps Rust module identity, private helper
visibility, test names, and caller paths unchanged while making file ownership
readable:

```text
src/tool_runtime/tests/
  git.rs                         86-line facade
    git/                         review/mutations, diff hunks, show changes, review reads
  work_on_project.rs             1,062-line facade/bootstrap helpers
    work_on_project/             surface, bootstrap/path source, resume, workflow/overview
  files.rs                       55-line facade
    files/                       mutations/audit, listing/parsing, search, artifacts/reads

crates/webcodex-runner/src/webcodex_runner/
  transport_tests.rs             26-line facade
    transport_tests/             telemetry, shutdown, polling, transport, inventory, proxy/QUIC

crates/webcodex-tool-contracts/src/
  tool_call.rs                   54-line facade
    tool_call/                   input_types.rs, canonical_enum.rs, parsing.rs

apps/desktop/src-tauri/src/
  state.rs                       97-line facade
    state/                       app_state, desktop_core, readiness, persistence, tunnel, tests

crates/webcodex-store/src/
  agent_task.rs                  44-line facade
    agent_task/                  models.rs, operations.rs, invariants.rs
```

For each of the four test files, ToolCall, Desktop state, and the final AgentTask
physical split, concatenating the facade prefix and included files reproduced the
pre-split logical source byte-for-byte. The first two attempted cut points exposed
only incomplete Rust item attributes (`#[test]` and multi-line derive/serde
attributes); the cuts were moved to complete item boundaries before validation.

`agent_task/operations.rs` deliberately avoids the filename `database.rs`: the
repository root `.gitignore` has a broad `data*` rule, which would silently ignore
that pathname. The immediately following `837c10b6` commit records the complete
operations slice so a fresh checkout does not depend on an ignored working-tree
file.

### AttemptAuthority

AgentTask's exact live-Attempt authority is now represented internally as one
validated value object containing:

- `task_id`;
- `attempt_id`;
- `assignee_agent_id`;
- `attempt_fence`;
- `attempt_controller_generation`.

These are the same five fields previously validated and compared independently.
The Endpoint continuation start fingerprint consumes the same object, so the
fingerprint and current-Attempt fencing cannot drift by accidentally omitting one
selector. Completion idempotency keeps the same flat JSON field set. Public
Database method signatures, durable schema, proof formats, error codes, SQL
state transitions, lease rules, and retry semantics are unchanged. Agent Wake
controller replacement constructs the same authority object before entering the
shared transaction helper.

### Follow-up verification

Final Linux validation after the Rust source was frozen:

| Check | Result |
|---|---:|
| Complete default Server library | 3,152 passed, 3 ignored |
| Complete Runner all-target tests | 975 passed, 4 ignored |
| Complete `webcodex-store` tests | 230 passed, 1 ignored |
| Complete `webcodex-tool-contracts` tests | 274 passed |
| AgentTask focused regression set | 51 passed |
| Desktop all-target tests, final rerun | 270 passed, 4 ignored |
| Workspace `cargo check --all-targets` | passed |
| Desktop `cargo check --all-targets` via its manifest | passed |

The first Desktop all-target run had one failure in
`unowned_remote_system_or_changed_targets_cannot_edit_the_server_environment`:
the observed error was `setup_busy` instead of the expected `server_not_owned`.
The exact test passed alone immediately afterward, and the complete Desktop suite
then passed 270/270 on the single full rerun. No source change was made between
those runs; this is retained as validation history rather than hidden as a clean
first pass.

No push, deploy, rebase, release, native Windows/macOS execution, all-features
workspace suite, or production state migration is part of this continuation.
