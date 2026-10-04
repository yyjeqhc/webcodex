# sf Server memory and Active Session residency — 2026-10-04

## Evidence scope

Read-only observations of `sf` / `webcodex.service`, with implementation work in
special `/root/git/webcodex`, based on `20ebf5f2dba0330c9039437c8f47b5fdf19564a5`.
The observed sf Server and Runner both reported clean build `20ebf5f2dba0`, version
0.5.0. No production restart, deployment, ledger edit or database change was made.
Only accounting facts and aggregate Session counts are retained in this report;
no Session id, message, instruction, principal or credential payload is copied.

The surviving journal begins on September 26. Rotated `/var/log/syslog`, `.1`,
`.2.gz`, `.3.gz` and `.4.gz` extend accounting coverage to September 6, 2026.
September 1–5 could not be recovered from those retained sources. This is NOT a
continuous historical per-process RSS series, and there is no historical heap
profile establishing per-component ownership. A later journal-only query for
startup counters timed out; it is not a source for restored-Session counts.

## Service-lifetime accounting, UTC

These are the 31 retained systemd `Consumed ... memory peak ... memory swap peak`
records for the exact Server service, indexed by the END of each running interval.
They are not daily averages; service uptime, traffic and deployments differ.
The systemd M/K values are shown as MiB/KiB. Memory and swap maxima may occur at
different times and must not be added to invent a simultaneous resident footprint.

| Interval ended (UTC) | Service memory peak (MiB) | Service swap peak (MiB unless noted) |
| --- | ---: | ---: |
| 2026-09-06 01:01:06 | 127.8 | 6.0 |
| 2026-09-07 04:21:39 | 157.1 | 52.0 |
| 2026-09-08 10:40:27 | 167.3 | 19.9 |
| 2026-09-09 13:20:57 | 122.5 | 0 |
| 2026-09-10 13:13:32 | 148.3 | 0 |
| 2026-09-10 23:17:54 | 134.6 | 21.3 |
| 2026-09-11 13:46:51 | 141.3 | 3.8 |
| 2026-09-12 02:48:54 | 134.1 | 2.4 |
| 2026-09-12 06:07:23 | 134.2 | 4.9 |
| 2026-09-12 11:53:38 | 67.5 | 0 |
| 2026-09-13 05:17:37 | 138.2 | 1.3 |
| 2026-09-13 07:15:12 | 116.8 | 0 |
| 2026-09-13 17:40:55 | 118.4 | 0 |
| 2026-09-14 07:05:36 | 137.4 | 0 |
| 2026-09-15 15:48:40 | 114.7 | 0 |
| 2026-09-18 02:21:27 | 139.3 | 24.9 |
| 2026-09-22 23:25:19 | 236.8 | 92.6 |
| 2026-09-25 07:00:40 | 261.6 | 116.5 |
| 2026-09-25 07:20:48 | 528.4 | 97.4 |
| 2026-09-26 02:25:37 | 237.3 | 55.7 |
| 2026-09-26 10:50:21 | 220.5 | 976 KiB |
| 2026-09-26 13:52:15 | 213.7 | 0 |
| 2026-09-27 09:15:34 | 310.7 | 103.1 |
| 2026-09-27 23:49:36 | 366.8 | 2.2 |
| 2026-09-28 11:07:16 | 389.6 | 48.3 |
| 2026-09-29 10:34:41 | 551.7 | 303.7 |
| 2026-09-30 13:27:59 | 616.0 | 501.3 |
| 2026-10-01 01:29:48 | 627.2 | 434.0 |
| 2026-10-02 07:22:12 | 620.7 | 349.5 |
| 2026-10-03 05:18:57 | 669.2 | 565.0 |
| 2026-10-04 01:10:24 | 688.0 | 703.5 |

A fresh running-interval snapshot during the 01:18–01:22 UTC observation window:

| systemctl property | Observed value |
| --- | ---: |
| MainPID | 1127035 |
| ExecMainStartTimestamp | 2026-10-04 01:10:24 UTC |
| MemoryCurrent | 449732608 bytes |
| MemoryPeak | 537878528 bytes |
| MemorySwapCurrent | 8204288 bytes |
| MemorySwapPeak | 8757248 bytes |
| TasksCurrent | 5 |
| NRestarts | 0 |

These are cgroup service measurements, not process RSS. Kernel accounting covers
anonymous memory, file cache and other charged resources. Reference for metric
semantics: [Linux cgroup v2 memory](https://www.kernel.org/doc/html/v6.14/admin-guide/cgroup-v2.html).
The shorter new interval must not be compared to the preceding interval as proof
of a code improvement; the new residency code was not deployed.

## Stable open-inode Session ledger observation

At `2026-10-04T01:21:49.332741Z`, a streaming JSON reader inspected one open ledger
inode and emitted aggregates only. Atomic replacement of the path cannot retarget
that descriptor to a different generation during the scan.

| Measurement | Value |
| --- | ---: |
| sessions.json bytes | 85169524 (about 81.22 MiB) |
| Retained Session rows / Active | 533 / 533 |
| Closed rows / tombstones | 0 / 0 |
| Retained events | 63622 |
| Events per row: median / p95 / max | 65 / 415 / 1589 |
| Retained messages | 29 |
| Messages per row: median / p95 / max | 0 / 0 / 6 |
| Encoded row bytes: median / p95 / max | 92838 / 554654 / 2130440 |
| Sum of encoded row bytes | 85168965 |

Retained rows grouped by creation date (UTC): September 19:11, 20:18, 21:7,
22:15, 23:13, 24:14, 25:11, 26:24, 27:68, 28:74, 29:64, 30:23;
October 1:63, 2:69, 3:47, 4:12. This is a distribution of today's retained rows,
NOT a reconstructed daily Session census: earlier eviction/deletion is unknown.

## What the evidence supports

The service-lifetime peaks rise materially after September 26, and the current
implementation retains every Active Session as an expanded record. The identity
fix #693 (`d7c6470e`, merged September 26 at 12:08:02 UTC) deliberately stopped
capacity churn from deleting Active canonical identities. That correctness
contract must not be reverted to save memory.

The timing is consistent with growing retained Active history raising the memory
floor. It does not prove that #693 caused every byte or that memory is leaking:
the 528.4 MiB peak on September 25 predates #693, and request load, concurrent
snapshots, allocator retention, buffers and SQLite/file-cache charges were not
controlled. No exact production saving or ownership percentage is asserted.

Two premises in the supplied proposal need qualification against current source:
startup already streams and sanitizes one row at a time, rather than keeping a
whole-file String plus full JSON tree; and events/messages already have independent
caps (2,000 retained events, rather than an unlimited archive). The missing piece
is final Active residency after streaming. Existing Cold storage is RAM JSON,
not a durable disk payload index.

## Implemented response and explicit limits

[Active Session residency](session-active-residency.md) separates lifecycle from
expanded residency, reuses the existing count target/access order, restores rows
Cold, and preserves exact mutation/replay/instruction behavior. It avoids a ledger
migration and preserves dirty data even when persistence fails.

It is intentionally NOT the proposal's entire Metadata Stub + 256 MiB Weighted
CLOCK + SQLite scheme. A count target cannot guarantee a heap-byte ceiling, Cold
JSON still grows with retained history, and cold mutation/reads add deserialization
cost. Disk-only payloads and weighted admission remain a subsequent measured
storage change, not a claim made by this patch. No allocator replacement, arbitrary
history reduction, implicit Active close, periodic sweeper or new external pin
protocol was introduced.

## Reproducible isolated benchmark

The ignored `active_restore_residency_memory_benchmark` test generates a 512-row,
128-event-per-row ledger and uses separate child processes. The control reproduces
the immediately preceding streamed per-row Active/Hot restore, not a more expensive
historical whole-document parser. The other cases retain Cold rows or materialize
100 rows. RSS/HWM and isolated materialization time are measured; these are not
production service metrics or end-to-end latency guarantees.

```sh
cargo test --locked --profile dogfood -p webcodex-workflow-session --lib \
  persistence::stream::tests::active_benchmark::active_restore_residency_memory_benchmark \
  -- --exact --ignored --nocapture
```

Observed on special in the optimized `dogfood` profile, using the same 55,083,034-byte
synthetic ledger (65,536 events). Each mode ran in a fresh child process:

| Mode | Hot / Cold | Process RSS / HWM (KiB) | Stream restore (ms) | 50 record-access/materializations (ms) |
| --- | ---: | ---: | ---: | ---: |
| Previous streamed Active/Hot policy | 512 / 0 | 130816 / 130816 | 1362.04 | 0.016 |
| New Cold restore | 0 / 512 | 60788 / 60788 | 1486.58 | 16.862 |
| New Cold plus 100 materialized | 100 / 412 | 73816 / 73816 | 1490.08 | 16.635 |

RSS was sampled after the optional 100-row materialization; `restore_ms` measures
only streamed restore and excludes that optional step. The last column is a loop
accessing 50 retained records (Cold cases deserialize), not a summary request or
network/tool latency. Cold restore's RSS was about 53.5% below the streamed Hot
control; 100 materialized rows remained about 43.6% below it. Cold restore took
about 9.1% longer in this single run. These are workload-specific process numbers,
not a forecast that sf's entire service cgroup will shrink by either percentage.
Allocator state, real row sizes, retained instructions and other Server domains
must be measured after an explicitly authorized deployment before claiming a
production saving. This experiment neither changed sf nor required heap dumps.
