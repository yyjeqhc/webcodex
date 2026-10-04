# Bounded Runtime resource diagnostics

## Entry and scope

Explicit `get_runtime_status(compact=false)` (and the corresponding full Runtime
API tool call) now includes `resources` and `session_persistence`. A focused
`client_id` still filters Runner/project/job data as before; the resource object
is explicitly `scope=server_process`, with the Server PID and sample timestamp.
It never claims to measure the selected remote Runner. No new MCP tool or input
field is introduced. Existing admission/visibility checks run before observation.

Sparse status, `summary_only`, invalid/unknown focus and the internal startup
`runtime_status` aggregate do not execute these diagnostic reads. No background
sampler, subprocess, automatic retry, cache or extra audit query is introduced.
Small OS/filesystem observations run on Tokio's existing blocking pool, outside
async workers and Session payload locks; a diagnostic failure does not fail the
main status result. Numbers from sequential observations are not an atomic snapshot.

## Linux resources

Read only fixed own-process procfs files, an own-membership-verified cgroup-v2
under the conventional mount, and descriptor entry counts. Each text read is
limited to 32 KiB, with oversized/malformed data unavailable. Descriptor counting
is capped at 4,096 and reports `fd_count_truncated`; it never opens descriptor
targets and includes the sampling descriptor when present. There are no command
lines, environment, usernames, mount paths, arguments or payloads in the output.

Process fields: RSS/current high-water mark, anonymous/file/shared RSS components,
private-anonymous swap, threads, bounded descriptor count. RSS is approximate
kernel accounting, not heap-live bytes or a precise page-table snapshot.

Cgroup fields: current/peak charged memory, current swap, memory PSI avg10
(some/full), OOM and OOM-kill counters. Group memory includes charges distinct
from process RSS; no maxima are summed. PID membership and path containment are
checked before reading counters. Missing controller, nonstandard mount/root or
unverifiable membership is unavailable, never replaced with whole-host counters.

Non-Linux platforms explicitly report unsupported resource measurements while
retaining portable Session writer observations. Missing values are null or an
explicit unavailable state; valid zero counters remain zero. This patch does not
pretend to provide Windows/macOS memory telemetry.

Semantics: [Linux procfs](https://docs.kernel.org/filesystems/proc.html) and
[cgroup-v2 memory](https://docs.kernel.org/admin-guide/cgroup-v2.html).

## Session persistence

`mode` distinguishes memory, background writer and synchronous fallback.
`ledger_bytes` is one file metadata observation, not a payload scan or disk-allocation
estimate. Missing/inaccessible files are unavailable; no configured path is emitted.
The existing writer records only bounded counters and its most recent attempt:

- Attempt/failure counts and dirty marks not yet attempted.
- Last attempt success, successfully written bytes (null on failure), serialize/write
  elapsed time, existing snapshot-lock wait/hold times and coalesced dirty marks.

A dirty mark is not a distinct Session or an exact business-mutation count. Zero
pending marks only means an attempt completed, not that persistence succeeded.
The most recent attempt and cumulative failures are separate from current durable
truth. Synchronous fallback has no background-counter fiction; its writer block
is absent. Observation does not call mark_dirty/flush, update a generation, extend
a deadline, change write ordering, or govern authority/retry. Counters reset when
the writer instance restarts. The existing persistence error field remains the
current error authority, and atomic JSON-v2 durability is unchanged.

## Deliberate exclusions

Do not compute ActionAudit COUNT/oldest-age or scan sessions.json on every poll.
Do not introduce retention deletion, VACUUM, per-Session WAL migration, optimistic
hydration, new locks/indices, allocator changes, or guessed heap-byte estimates as
part of diagnostics. These require separate policy or evidence. The attachment's
historical sizes/deployment claims are not new measurements from this patch.

## Verification

Fixtures check process/cgroup separation, numeric units/overflow/duplicates,
unknown versus zero, secret/path omission, descriptor/file bounds, escaped or
foreign cgroup rejection, sparse/startup/error no-observation paths, failed writer
attempt semantics, coalescing, zero side-effect queries and synchronous fallback.
No production service is restarted or instrumented by these tests.
