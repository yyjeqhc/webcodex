# Runtime resource lifecycle and bounded observation follow-up

Implementation branch: `perf/runtime-resource-lifecycle`, based on clean
`special:/root/git/webcodex` / `origin/main`
`34de0604fd2894dd93c5a4aa8ab800ab1d24e007` after #861.

This is a source-and-isolated-fixture follow-up to the sf audit, not another sf
deployment. No production database, service unit, credentials, or process was
changed. The observed sf footprint (about 73 MiB `sessions.json`, around 690 MiB
Server RSS plus swap) motivated the work; it was not a heap profile and is not
proof of a leak. These fixtures do not establish a new production latency or RSS.

## 1. Bounded process capture owns cleanup on every exit

`webcodex-environment::process` no longer starts detached pipe-reader threads.

On Unix it creates a private process group, polls owned nonblocking pipes, and
checks the same absolute deadline while draining a bounded amount from each
stream. `waitid(..., WNOWAIT)` observes leader exit without reaping it. Keeping
that waitable leader anchors the PID/PGID until group cleanup; the group is
signalled before the leader is reaped, not afterward against a reusable PID.
Timeout, output overrun, read errors, normal completion and unwinding all take
the owner cleanup path. A process deliberately escaping its group via `setsid`
is outside this process-group contract; this helper is not a filesystem/process
sandbox.

Windows reuses `webcodex-process::ManagedChild`: it assigns a private kill-on-close
Job Object before resuming the child. `PeekNamedPipe` plus exclusive bounded reads
replaces reader threads; closing the Job Object owns descendants even when their
leader has already exited. The helper owns creation flags and uses
`CREATE_NO_WINDOW`, matching the current noninteractive Environment callers.
It does not enable silent child breakaway. No second Windows Job implementation
was added.

Regression coverage includes stdin EOF, both output channels, an exact output
limit, overrun, a live leader timeout, and an exited leader whose descendant
retains the pipes. Linux fixtures verify that descendants stop. Windows has an
owned native self-executable descendant fixture. On special, the actual capture
sources and tests were type-checked for both `aarch64-apple-darwin` and
`x86_64-pc-windows-msvc` in an isolated harness with the matching pinned libc and
windows-sys versions. That is cross-compilation evidence, not native Windows or
macOS execution.

## 2. Session restore reduces transient memory, not identity retention

The v2 loader now streams the root map and one JSON row at a time through the
existing validators. Each accepted row immediately becomes its final Hot or Cold
representation. There is no whole-file input String, whole-ledger `Vec<Value>`,
or extra whole-ledger vector of materialized Session records.

Version and sessions fields can occur in either order. Missing/duplicate/unknown
root fields, unsupported versions, trailing content and a truncated document
reject the entire restore. Malformed individual rows retain the existing
row-local rejection behavior. Closed records pass the same authority and event
sanitization before they become raw Cold records. Tombstones, retained order and
lifecycle rules are unchanged. Active sessions are never closed or removed to
satisfy a cache budget.

A manual Linux comparison runs the eager allocation pattern and the new loader
in separate child processes over the same synthetic 512-closed-session,
64-event-per-session ledger (27,721,242 bytes). Unoptimized build, single samples:

| Loader | Peak RSS | Elapsed |
|---|---:|---:|
| Eager allocation reference | 186,956 KiB | 3,593.449 ms |
| Streaming restore | 63,804 KiB | 4,577.798 ms |

This is about 66% less peak RSS in that fixture, with a CPU/elapsed tradeoff in
this build. It does **not** measure all sf allocations or guarantee faster cold
startup. The eager reference reproduces the former allocation pattern on valid
fixture data; it is not a separately deployed old Server.

Cold records still retain their raw bytes and active records are still Hot.
Moving inactive-but-active session bodies fully out of RAM, incrementally storing
sessions, and attributing the live heap remain separate work. This patch makes
no claim to have eliminated steady-state Session residency.

## 3. Large ledger write scheduling and cost metrics

The atomic v2 file format and exact generation flush barriers remain unchanged.
The ordinary writer now chooses a cost-aware coalescing window:

- Under 4 MiB: the existing 20 ms window.
- At least 4 MiB: `clamp(4 * previous_write_duration, 250 ms, 1 s)`.
- The first dirty mark captures its deadline; later marks and measurements cannot
  extend that existing deadline.
- Explicit durable flush and final-owner shutdown bypass coalescing.

The writer seeds its size policy from the existing file length. Small ledgers
therefore retain their prior behavior. Large ledgers may intentionally buffer
ordinary events for up to one second before attempting a write. This is not a
crash-loss bound: serialization, I/O, an outstanding write and failures can add
latency. Ordinary events did not have an immediate-durable guarantee before.
Operations promising durable generation completion retain their required wait. This remains whole-ledger atomic replacement,
not a claim of per-session incremental persistence or power-loss fsync semantics.

The retained short-burst experiment (256 sessions, 128 additional calls spaced
5 ms apart, final ledger 6,875,674 bytes) produced three full writes for each of
immediate, fixed-20-ms and cost-aware scheduling. Mutation plus explicit flush
was respectively 1080.742, 1101.077 and 1342.050 ms. Thus that burst does **not**
prove fewer writes or lower latency; its explicit final flush and serialization
cost dominate. The deterministic policy tests prove the fixed deadline, size/cost
bounds and flush bypass, not a universal write-amplification improvement.

Low-cardinality debug events under `webcodex::session_cost` report restore bytes,
hot/cold counts and raw cold bytes, snapshot mutex wait/hold and row count, and
atomic-write bytes/duration/success. They never include session messages or
credentials. The private benchmark policy seam can run the old fixed/immediate
schedules over exactly the same implementation; it is not a production knob.

## 4. Presentation reuse is distinct from validation proof

The old Work Result cache required `ValidationSourceFence::quiescent`, but a Job
handoff intentionally makes validation uncertainty sticky. A later terminal Job
therefore could not make the presentation cache useful again.

A presentation-only invalidation snapshot now records the source epoch/generation,
in-flight dispatches, and at most 128 exact known handoff Job IDs per tracked
Project. The overall Project registry bound remains 4096. After a known handoff,
reuse is blocked until an immutable authorized registry lookup proves those exact
Jobs ended. Missing, unauthorized, wrong-Project, recovering, Lost and explicitly
`outcome_unknown` Jobs do not qualify, even if an outer lifecycle is terminal.
The source fence is compared again before removing tracked IDs.

Resolving that presentation state never clears validation uncertainty. The native
handoff regression observes one fresh Git probe while running, one after terminal
completion, and zero on the next unchanged automatic poll while validation
quiescence remains false. Unknown non-Job effects remain conservative. There is
no optimistic reset on an arbitrary successful call.

The 30-second presentation lease, principal/Project identity, Runner registration
and revision/root fences, explicit refresh and closed-session behavior remain in
force. Jobs, collaboration and validation still project live state independently.
There is no singleflight implementation here, nor a cache of the entire Work
Result. External filesystem changes remain bounded by explicit refresh/lease
expiry, not a filesystem watcher. `workspace_observation` phase events distinguish
fresh/uncacheable/crossed-fence/failure work; cache hits remain a debug signal.

## 5. Immutable registry reads have no post-unlock receipt side effects

A dedicated Deref-only registry read guard now backs immutable Console aggregates,
exact visibility/workspace identity, passive snapshots, common Job attribution
and maintenance admission observations. It has no receipt-flush Drop behavior.
A failed durable receipt retry can no longer make one of those reads unexpectedly
perform unrelated SQLite I/O. The regression retains a failed candidate across
repeated snapshots and verifies that the next mutation boundary retries it.

Mutation and lazy-lifecycle paths still use the original post-unlock receipt and
terminal-event persistence. Their ordering, retry candidates and accepted Job
verdicts are preserved. They now expose a `registry_durable_finalize` duration
when work exists. This patch does **not** move all mandatory persistence from
Tokio workers to an asynchronous writer and does not claim otherwise. That is a
larger durability handoff design, not safely replaceable by detached writes.

## 6. Console historical reads own an end-to-end budget

One 5-second absolute deadline covers the Console Store-read entrypoint's semaphore
admission, connection lock wait and SQL. An HTTP future dropping or timing out
sets a cancellation flag owned by that exact read. The admitted blocking worker
retains its sole semaphore permit until it actually unwinds.

The synchronous Store budget is scoped to the exact Database and blocking thread,
restored on scope exit, and cannot be nested to extend a deadline. A connection
progress handler checks the owned deadline/cancellation. Busy waiting is disabled
only while this budgeted read or its derived repair owns the connection; the old
busy policy and progress handler state are restored before unlock. One interrupt
is issued so rollback can finish. Canonical Audit, ACK and effect writes outside
this read scope are not optionalized.

This avoids a shared `sqlite3_interrupt` handle which could hit a subsequent
connection user. SQL interruption and cancelled lock waiting leave the next reader
healthy. Derived repair interruption rolls back before releasing the writer.
Timeout returns HTTP 503 rather than a misleading empty history page. Existing
explicit page transactions still preserve event/link snapshot consistency.

The budget is cooperative: it cannot preempt an uninterruptible kernel I/O syscall,
arbitrary non-SQL native closure, or OS scheduling delay. The Window history helpers in `store_read.rs`
use the owner path, but unrelated Store callers remain unchanged. A successful
large dirty-summary repair is still a full repair, not chunked maintenance.

## 7. Bundled SQLite and deployment boundaries

The workspace now locks rusqlite 0.40.2 / libsqlite3-sys 0.38.2, whose bundled
runtime is SQLite 3.53.2. A runtime test fences against versions older than the
3.51.3 WAL fix on this release line. A separate temporary WAL writer/checkpoint
fixture verifies all 512 committed rows and `quick_check`. It is a concurrency
regression, not a reproduction of the rare WAL-reset race or evidence of existing
production corruption.

Primary upstream explanation: https://sqlite.org/wal.html#the_wal_reset_bug

The dependency upgrade requires explicit signed SQLite integer conversion and
handling fallible progress/authorizer registration. No application DB schema,
Session ledger version, MCP model-facing tool schema or Runner wire protocol was
changed. Console history budget exhaustion adds a truthful 503 response on that
existing route.

Server-side changes require a Server upgrade. Environment helper changes require
the executable that uses those helpers (CLI/desktop/service tooling) to be rebuilt;
no new Runner protocol is required. Replacing sf's Server binary alone does not
rewrite its installed Runner unit. The previously observed missing effective
`RestartPreventExitStatus=2` must still be reconciled during an explicitly
authorized deployment; no unit was changed or daemon reloaded here.

## Functional fixtures versus optional delivery budgets

The first broad Runtime run exposed seven Peer fixture assertions that demanded
immediate delivery from a production optional 40-ms enrichment call. A separate
budgeted native Store contract test passed. Functional route/delivery fixtures
now use the existing deadline-taking projection with an explicit fixture watchdog;
all authority, payload and exact-once assertions remain. No production timeout
was increased and no direct SQL route insertion was substituted. An additional
expired-budget regression verifies unchanged business output, zero retained
routes, and correct later delivery. Store interruption/rollback and postprocess
contention tests continue to cover production omission semantics. This is not an
assumption that every first optional projection must succeed under CPU/I/O load.

## Reproduction and acceptance evidence

All process/database fixtures are isolated on special. No production stress test
or sf file copy is needed. Commands used for the affected boundaries:

```sh
cargo test -p webcodex-environment --lib
cargo test -p webcodex-workflow-session --lib
cargo test -p webcodex-workflow-session --lib streaming_restore_memory_benchmark -- --ignored --nocapture
cargo test -p webcodex-workflow-session --lib populated_ledger_write_coalescing_experiment -- --ignored --nocapture
cargo test -p webcodex-store --lib
cargo test -p webcodex-runner-registry --lib
cargo test -p webcodex --lib
cargo test -p webcodex --lib workspace_cache -- --nocapture
cargo test -p webcodex --lib runtime_console_http::store_read -- --nocapture
cargo test -p webcodex --lib --features experimental-code-mode runtime_console_http::tests::window
cargo test -p webcodex-cli --lib
cargo fmt --all -- --check
git diff --check
```

Final validation results on the reviewed implementation (overlapping focused
suites are not additional unique tests):

| Scope | Result |
|---|---|
| Root Runtime library | 3,260 passed; 3 ignored (`runtime-reviewed.log`) |
| Store library | 262 passed; 4 ignored (`store-final.log`) |
| Workflow Session library | 249 passed; 3 ignored (`session-final.log`) |
| Runner Registry library | 324 passed (`registry-reviewed.log`) |
| Environment library, Linux | 117 passed (`environment-full.log`) |
| CLI library | 450 passed (`cli-final.log`) |
| Work Result cache subset | 6 passed (`workspace-reviewed.log`) |
| Console history scheduler subset | 3 passed (`history-console.log`) |
| Experimental Code Mode Window Console | 21 passed (`window-feature.log`) |
| SQLite version/checkpoint subset | 2 passed; actual runtime 3.53.2 (`sqlite-runtime.log`) |
| Two synthetic ledger benchmarks | Explicitly run, results and limitations above |
| Process sources/tests cross-check | macOS ARM64 and Windows MSVC type-check passed; not native execution |
| Formatting/whitespace | `cargo fmt --all -- --check` and `git diff --check` passed |

Logs are retained under `/tmp/webcodex-resource-lifecycle/`. Ordinary ignored tests
are not counted as executed; the named synthetic benchmarks are explicit runs.
Cross-target checks cover the actual process capture source/tests and existing
ManagedChild dependency only, not a complete native platform product test.
