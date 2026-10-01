# v0.5 structural boundaries: detached ownership and request tracing

## Scope and baseline

This round continues the existing oe branch after the user's rebase, at baseline
`185b0ebe452de0c09d50766c13f2761c3c82fad2`. It is code placement, not a new
execution architecture, runtime protocol, permission policy or tool-name change.
No dependency, package version, release workflow or production configuration is
changed. These two modules have independent commits and independent ownership
boundaries; they are not merged into a common abstraction.

The initial audit also covered the four large Git/file/bootstrap/transport test
files, ToolCall, Desktop state, and the proposed Attempt authority value object.
The Host blocked creation of the batch-test move script and execution of the
separate ToolCall extraction script. Neither operation was rerouted or replayed.
The four original test files, ToolCall and Desktop state remain unchanged. This
round completes the independent Detached Job and Trace slices instead; it does
not describe the entire larger roadmap as finished. Attempt selectors need their
own reviewed validation/fingerprint change, not an incidental mechanical move.

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
