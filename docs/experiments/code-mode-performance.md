# Code Mode performance: measured baseline and next development steps

Review date: 2026-09-18. Source basis: `bc23562eaf988ef1ccb1eea17c8f210dec61a7d8`.
This is an engineering roadmap, not an expansion of the currently admitted tools
or a stable execution contract.

## Decision

Keep the existing V8 frontend and canonical orchestration host. Optimize useful
model round trips and the backend critical path before replacing the engine,
introducing persistent JavaScript globals, or raising every concurrency bound.
The current implementation already has concurrency and diagnostic composition
telemetry; those are not missing features to rebuild.

This review implements only dense host-array conversion and focused tests. All
other changes below are proposals with explicit validation boundaries.

## Current source, rather than an earlier stage plan

| Area | Already implemented | Source |
| --- | --- | --- |
| Execution | One-shot Server-side V8; child tools re-enter canonical ToolRuntime | `crates/webcodex-code-mode/src/runtime.rs`, `src/tool_runtime/orchestration_host.rs` |
| E1 | Read-only adaptive and parallel orchestration | `src/tool_runtime/code_mode.rs` |
| E2a | E1 reads plus `cargo_check` / `cargo_test`, canonical Job handoff and effect receipts | `src/tool_runtime/code_mode.rs` |
| E2b | E1 reads plus one `edit_project_files` attempt; no validation in the mutation cell | `src/tool_runtime/code_mode.rs` |
| Job continuation | E3 terminal attention and H1 Job-native Host carrier source; not nested Code Mode tools | `docs/experiments/code-mode.md` |
| Scheduling | Canonical Denied / Sequential / Parallel policy; a per-cell shared/exclusive fence | `src/tool_runtime/orchestration_host.rs` |
| Capacity | Two active cells by default, eight child futures per cell, 32 child calls per cell | `crates/webcodex-code-mode/src/lib.rs` |
| Native batching | Eight file reads in flight per `read_files`; two search queries per `search_project_texts` | `src/tool_runtime/read_files.rs`, `src/tool_runtime/search_project_texts.rs` |
| Observability | Outer service time, composition duration, slot wait, nested counts, raw/projected byte counts; SQL views | `docs/experiments/code-mode.md`, section E1.5 |
| Guidance / recovery | Direct vs Code Mode guidance profiles and structured failure recovery | Commits `e1a91868` and `bc23562e` |

Source presence does not establish deployment or Host behavior. During this review,
the available self-hosted connections did not provide a comparable live
Direct-vs-Code-Mode task pair. No Server or Runner was replaced. Consequently this
review has local runtime measurements, not a live ChatGPT Direct-vs-Code-Mode task
comparison.

## What to measure

Separate three questions:

1. Does composition remove model-visible tool/decision round trips while preserving
   findings, validation, and effect truth?
2. Does the canonical host / Runner critical path become shorter, or does it merely
   move waiting under one outer call?
3. How much local time is spent in initialization, V8 value conversion, and output
   projection after the first two costs are understood?

Outer service time already includes its nested work. Do not add every overlapping
child duration to it. Likewise, Window inter-call gaps include external scheduling,
network, inference, UI, and possibly user delays; they are not measured reasoning
time. Runtime `max_in_flight` includes host futures waiting for scheduling fences,
not necessarily simultaneously executing Runner commands.

Use the existing `code_mode_action_traces` and `code_mode_nested_tool_usage` views
first. Add narrowly scoped diagnostic spans for runtime startup, scheduling wait,
canonical dispatch, and conversion only where current evidence cannot attribute a
slow path. Do not create a second audit writer or add a verbose timing tree to every
model result.

## Local runtime experiment

The opt-in integration test is
`crates/webcodex-code-mode/tests/runtime_latency.rs`:

```bash
cargo test --locked --profile dogfood -p webcodex-code-mode \
  --features v8-runtime --test runtime_latency -- \
  --ignored --nocapture --test-threads=1
```

Each case reports its first run separately, then uses three warm-ups and 21 measured
runs. Only the first `noop` in each fresh process includes global V8 initialization.
Every run still creates a fresh thread, isolate, and context. The timer includes the
synthetic host's payload clone and runtime teardown, but excludes model turns,
canonical ToolRuntime, Session, network, and Runner work. There are semantic
assertions and **no timing pass/fail thresholds**. Ordinary package tests skip the
probe.

The optimized build used `rustc 1.95.0 (59807616e 2026-04-14)` on `x86_64` / `6.12.0-124.8.1.el10_1.x86_64`.
The baseline binary was built before modifying `runtime.rs` and retained locally;
the candidate uses the same test with only the array conversion implementation
changed. A second comparison used process order baseline / candidate / candidate /
baseline. The table shows **ranges of the two per-process percentiles**, in
microseconds, not pooled percentiles or confidence intervals:

| Case | Before p50 (us) | After p50 (us) | Before p95 (us) | After p95 (us) |
| --- | ---: | ---: | ---: | ---: |
| `noop` | 1,956..2,088 | 1,820..2,080 | 2,472..2,622 | 2,362..2,582 |
| `eight_immediate_sequential` | 2,360..2,406 | 2,166..2,306 | 2,819..2,897 | 2,491..2,929 |
| `eight_immediate_parallel` | 1,944..2,011 | 1,895..2,098 | 2,652..3,104 | 2,161..2,688 |
| `eight_20ms_sequential` | 171,907..171,994 | 171,563..172,278 | 172,276..172,650 | 172,305..173,845 |
| `eight_20ms_parallel` | 23,302..23,397 | 23,146..23,403 | 23,968..24,061 | 23,615..24,763 |
| `numeric_array_16384` | 5,722..5,946 | 2,312..2,514 | 7,261..7,600 | 2,885..3,208 |
| `search_rows_2048` | 6,744..6,773 | 6,046..6,458 | 7,975..8,323 | 7,540..7,837 |

Interpretation:

- Empty cells cost about two milliseconds on this host. Reusing an isolate cannot
  save more than the applicable part of that interval; it cannot by itself remove
  model or Runner delays.
- Eight synthetic independent 20 ms callbacks cost about 172 ms sequentially and
  23 ms concurrently. This demonstrates overlap in the existing runtime, not a new
  speedup produced by the array patch and not a real-model throughput result.
- The 16,384-number payload is 87,195 JSON bytes. Dense array construction removes
  roughly 3.4 ms from these measured cells. The object-heavy 2,048-row payload is
  188,245 bytes and shows a smaller, noisier improvement because object properties,
  strings, and host cloning still cost time.
- Small differences in the no-op and small-result controls should be treated as
  measurement noise. These runs are not a cross-device performance guarantee.

Local, non-versioned evidence from this run is in
`target/code-mode-latency-abba.json`; the retained baseline binary is
`target/code-mode-latency-baseline`. The table above retains the measured summary
when build artifacts are later removed.

### Implemented hot-path change

`json_to_v8` previously allocated a Rust decimal index string and a V8 string, then
called `create_data_property` for every array element. It now converts the elements
and uses `v8::Array::new_with_elements` once per array. The existing length check and
recursive failure propagation remain. There is a temporary vector of V8 handles;
this is not a zero-allocation claim.

Objects still use own-data-property creation, including `__proto__`. Arrays must
not use ordinary indexed assignment, which could invoke an inherited setter.
`tests/value_conversion.rs` exercises empty/nested/mixed JSON arrays, Unicode,
`__proto__` data, own writable/enumerable/configurable elements, and a poisoned
`Array.prototype[0]` getter/setter. Existing runtime tests cover concurrency,
child failures, output/call bounds, hard termination, and effect-aware drain.

## Prioritized next work

### P0: a task-throughput lane and a small typed Code Mode surface

Run three fixed workflows against comparable source snapshots and model contexts:
bounded repository review; search -> selected reads -> one guarded multi-file edit
-> post-edit inspection; and a validation launch -> canonical Job continuation.
Include the best existing direct/native batching path as the control, not an
artificially inefficient one-file-per-model-turn baseline.

Track task wall time, outer calls, canonical invocations, result bytes, first-pass
success, recovery turns, completed validation evidence, and final findings/diff
quality. Repeat and alternate order; keep human pauses separate. Use the current
phase protocol rather than inventing a benchmark service. An engineering target
such as materially fewer outer calls with non-regressing findings is a gate to
measure, not an already-achieved percentage.

Generate a bounded Code Mode-facing callable/type projection from canonical
ToolDefinition and typed schemas. Include only the selected stage's admitted tools,
omit Server-owned injected fields, and expose the output fields needed for reliable
programs. Load only the needed contracts and a few tested usage examples. Cache
pure generated schema material by the existing relevant schema/policy identity,
not authorization decisions or mutable project results. Keep normal child
re-authorization. Do not hand-maintain a second SDK registry.

Existing guidance profiles and actionable errors are a foundation. Measure whether
this next projection actually reduces discovery and repair turns before making
Code-Mode-only exposure a default.

### P1: eliminate attributable queue/startup and payload costs

`spawn_runtime` no longer waits on a synchronous handle receiver inside the async
execution path. Runtime startup now has an owned two-phase lifecycle: the V8 thread
performs the process-wide `OnceLock` initialization and creates a fresh isolate,
publishes readiness through a Tokio oneshot, then waits for an explicit activation
fence before evaluating user JavaScript. The existing absolute Code Mode deadline
covers slot acquisition, V8 initialization, and isolate readiness. Dropping the
startup owner before activation closes that fence and transfers the thread join to a
blocking reaper; a late handle is terminated and user JavaScript never starts.
Startup failure before readiness is reported as a Runtime error rather than a child
failure.

The deterministic startup tests use per-execution private gates rather than sleeps or
process-global configuration. They cover normal activation, a startup gate held past
the deadline on a single-thread Tokio runtime, task cancellation before readiness,
cancellation immediately after readiness publication but before activation,
cancellation after activation while V8 is executing CPU-bound JavaScript, and thread
exit before readiness. Ownership transfers from the startup guard to `SpawnedRuntime`,
whose drop path terminates and reaps an activated isolate if the outer future is
cancelled. The execution-slot permit remains attached to that same join owner until
normal decision-phase code explicitly releases it or cancellation cleanup has actually
joined the runtime thread. Timeout/cancellation therefore cannot expose spare V8
capacity while a late or terminated runtime thread is still alive; the tests observe
the reap before verifying the slot becomes available again. A separate panic-before-
readiness case proves channel-close cleanup also remains bounded.

The final comparison used process order baseline / candidate / candidate / baseline
on the same host, `dogfood` profile, benchmark source, and 21 measured samples per
case. The table shows the range of the two per-process percentiles in microseconds:

| Case | Before p50 (us) | After p50 (us) | Before p95 (us) | After p95 (us) |
| --- | ---: | ---: | ---: | ---: |
| `noop` | 1,817..2,040 | 1,828..2,138 | 2,512..2,703 | 1,928..2,580 |
| `eight_immediate_sequential` | 2,270..2,373 | 2,096..2,550 | 2,972..3,928 | 2,237..2,987 |
| `eight_immediate_parallel` | 2,009..2,018 | 1,952..2,242 | 2,440..2,862 | 2,928..3,533 |
| `eight_20ms_sequential` | 171,829..171,965 | 172,047..172,377 | 172,438..172,850 | 173,170..174,051 |
| `eight_20ms_parallel` | 23,238..23,570 | 23,361..24,095 | 24,052..25,177 | 23,538..25,279 |
| `numeric_array_16384` | 2,439..2,652 | 2,589..2,697 | 2,872..4,260 | 3,279..3,331 |
| `search_rows_2048` | 6,575..6,623 | 7,298..7,460 | 8,940..9,247 | 8,885..11,821 |

The tiny-case p50 ranges overlap substantially and their direction changes across
processes; p95 is even noisier. The activation handshake therefore is **not cleanly
separable from process-level microbenchmark noise** at this scale. The 20 ms child
cases remain dominated by child latency, and the larger payload cases likewise do not
isolate startup cost. This patch is scheduler-hygiene and lifecycle-correctness work,
not a measured single-cell speedup and not evidence of model-level throughput
improvement. A Direct-vs-Code Mode E4 rebaseline belongs after merge/deployment, not
in this local runtime measurement.

Tune capacity using `slot_wait_ms`, queue tails, CPU, RSS, and actual Runner limits.
A cell waiting for child I/O still holds its V8 slot. Conversely eight `read_files`
children can each fan out eight file reads: nested call concurrency is not a global
Runner request budget. Do not change the deliberately lower search concurrency
merely to match reads. Prefer existing Runner admission and narrowly scoped
resource controls over a new universal scheduling framework.

At `CanonicalOrchestrationHost::prepare_arguments`, the owned `Value` is currently
converted with `arguments.as_object().cloned()`. Destructuring `Value::Object` would
remove a redundant deep copy while retaining exactly the same forbidden-field
checks and injected context. This is a small follow-up with canonical host tests,
not a reason to weaken dispatch or audit. Profile raw-result byte counting before
adding caches or removing traversal: diagnostic fidelity must remain unchanged.

Only consider a bounded runtime worker pool or preinitialized snapshot once startup
is a material measured fraction. Keep fresh per-cell authority and state; do not
trade a few milliseconds for cross-Session globals, outstanding Promises, or stale
bindings. Review heap and process containment before increasing active-isolate
capacity substantially; a timeout/output cap is not a memory limit.

### P1: E2c as a bounded coding loop, not arbitrary shell orchestration

The next capability likely to remove useful development round trips is one guarded
edit followed by a scoped validation launch, not a larger set of unrelated tools.
Reuse the current mutation primitive, validator, Job, and effect receipt.

Define freshness before admitting this combination. The current composition fence
ends at a validator's Job handoff; it does not freeze the workspace for the Job's
lifetime. Another cell or a direct write can otherwise invalidate which source the
validation proves. Either validate an immutable identified snapshot, or explicitly
mark mutable-workspace results as unproven/stale for later source revisions.
Comparing a Git HEAD alone does not cover uncommitted edits, and a mere start/end
comparison cannot rule out changes restored during execution.

Acceptance cases must include edit success followed by JS failure, timeout after
validation start, same-Job recovery without redispatch, concurrent writes, failed
and no-op edits, and validation that completes for an older source. Preserve
`outcome_unknown`; never blindly replay the whole program. Keep generic shell,
release, and arbitrary multi-mutation orchestration out of this first slice.

### P2: reuse and broader capabilities only after the throughput gate

Consider small reviewed reusable snippets to reduce repeatedly generating the same
orchestration source. Reuse existing Skill/resource ownership and bind snippets to
current canonical contracts; do not create another workflow or retry authority.
A bounded explicit result handle may later avoid re-fetching large intermediates,
but it needs caller/Project/Session ownership, byte/TTL limits, and invalidation of
mutable observations. Persistent V8 globals are not a substitute for that design.

Runner-local read batching or projection is worth considering only if measured
transport dominates. It is not a reason to move the full V8 orchestration runtime
onto every Runner or bypass canonical child dispatch.

## Cleanup worth doing, and cleanup to avoid

The redundant owned-argument clone is concrete low-risk cleanup. The three outer
Code Mode methods also repeat result/telemetry finalization; a small private helper
could reduce future drift, but their distinct effect and termination contracts must
remain explicit. This is maintainability work, not a demonstrated major speedup.

The runtime's large inline test module can move to a dedicated module when next
edited substantially. New coverage here already uses integration test files. Avoid
unrelated file shuffles, another metadata catalogue, removed Session-context ACK
machinery, or unmeasured serialization rewrites. Keep snapshot `read_revision` and
canonical permission/effect checks; shorter code is not improved correctness.

## Open-source comparison and source boundaries

- [OpenAI Codex scheduling](https://github.com/openai/codex/blob/main/codex-rs/core/src/tools/parallel.rs)
  uses shared/exclusive scheduling and separates dispatch waiting from handler time.
  WebCodex already has the scheduling analogue. The additional useful lesson is
  attribution without counting nested calls as independent model round trips.
  Local Codex source was inspected at
  `4701aa4b4239c70063ab6f2fcb835324f9c109f4`, including
  `code-mode-runtime/src/session_runtime/mod.rs` and `cell_actor/mod.rs` for
  explicit cells, stored values, and yield/continuation. This identifies the local
  reference; it does not claim that it is the latest upstream commit.
- [Cloudflare Code Mode](https://github.com/cloudflare/agents/blob/main/packages/codemode/README.md)
  derives TypeScript declarations from tools, separates executor from host dispatch,
  and provides discovery and reusable snippets. Its Workers execution/egress model
  is different; reusing those product patterns does not require adopting Workers
  or a second durable runtime inside WebCodex.
- [UTCP Code Mode](https://github.com/universal-tool-calling-protocol/code-mode)
  is useful for interface generation, progressive discovery, and orchestration
  examples. Do not substitute its VM-based integration for WebCodex's authority and
  lifecycle model. [Node's own VM documentation](https://nodejs.org/api/vm.html)
  explicitly warns that `node:vm` is not a security mechanism for untrusted code.
- [Anthropic's code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp)
  explains on-demand discovery, in-code control flow, and selecting results before
  returning them to the model. Those mechanisms support the task-throughput focus;
  published examples' savings are not evidence of WebCodex's speedup.

## Validation for this patch

```bash
cargo test --locked --profile dogfood -p webcodex-code-mode --features v8-runtime
cargo test --locked --profile dogfood -p webcodex-code-mode --no-default-features
cargo test --locked --profile dogfood --features experimental-code-mode \
  code_mode_binds_exact_project_and_session_through_real_canonical_reads -- \
  --nocapture --test-threads=1
cargo check --profile dogfood --features experimental-code-mode --all-targets
cargo fmt --all -- --check
git diff --check
```

The feature-enabled default parallel package command passes all 28 unit tests plus both
value-conversion integration tests; the latency probe remains ignored unless explicitly
requested. During review, two effect-aware timeout tests initially stalled only under the
parallel harness because they shared the process-wide execution slots with unrelated
runtime tests and could wait forever for a host-start notification after their own call
had timed out before dispatch. Those lifecycle tests now reuse the private per-test
semaphore seam already used by the startup tests, so they still exercise the same runtime
semantics without cross-test capacity contention. The feature-disabled package run passes
its single configuration test with V8-only cases excluded. Root integration checks pass
for exact Project/Session binding, E1 read-only orchestration, E2a validation admission,
and E2c frontend-timeout preservation of the exact validation Job. The all-targets
experimental feature check also passes. No root runtime contract, permission rule, stage
allowlist, Server deployment, Runner deployment, or configured concurrency limit is
changed by this patch.
