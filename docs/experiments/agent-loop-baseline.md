# Agent Loop Baseline Protocol

This experiment provides a repeatable measurement baseline for comparing a real
coding-agent run that uses Direct Tools with a run of the same task through Code
Mode. It measures facts already recorded by WebCodex plus an optional bounded
run-annotation sidecar. It does not infer model reasoning, intent, private
chain-of-thought, or unrecorded tool arguments.

The deterministic [`scripts/eval_coding_loop.sh`](../../scripts/eval_coding_loop.sh)
harness remains responsible for runtime mechanics. This protocol extends that
existing Agent Loop baseline; it does not create a second benchmark or reporting
framework.

## Typed Surface v1 dogfood goal

The Typed Surface v1 dogfood lane asks whether a typed callable/result surface
changes actual task throughput without weakening correctness. The paired report
is designed to expose whether Code Mode uses fewer meaningful model-facing outer
calls (a round-trip-pressure proxy, not an exact model-turn count), incurs fewer
avoidable contract repairs, keeps more mechanical work inside one cell, and
compresses nested raw results into a smaller returned projection.

A lower call count is not a correctness verdict. A Direct/Code Mode comparison is
decision-relevant only when both runs use the same exact case definition and
40-hex Git base and both satisfy the case correctness/validation gate. The report
never emits a winner, score, or overall ranking.

New Code Mode samples identify the actual experimental surface as `read_only`,
`validation`, or `guarded_edit`. They are not pooled under a generic `code_mode` surface.
For schema-v1 replay, the reporter still accepts the historical `e1` / `e2a` / `e2b`
aliases without rewriting the original case object or its fingerprint.

## Evidence contract

The profiler is [`scripts/agent_loop_report.py`](../../scripts/agent_loop_report.py).
It accepts the following evidence sources:

1. **ActionAudit SQLite** is the preferred source for outer tool calls. Select one
   run with an exact Workflow Session id. Existing rows provide the hashed
   `ClientWindow`, canonical meaningful classification, request-observed and
   response-handoff timestamps, serial/overlap classification, status, tool
   identity, and `model_ergonomics` metadata including serialized `ToolResult`
   bytes. Code Mode outer rows also persist the payload-safe
   `code_mode_composition` summary: nested call/success/failure counts, nested
   tool distribution, maximum in-flight calls, consequential outcome counters,
   slot wait/internal duration, and nested/raw versus returned byte counts.
2. **Per-trace `events.jsonl`** is an optional supplement for observed Runner
   enqueue events. The profiler reads only JSONL metadata; it never opens captured
   request/result payload files.
3. **Run annotation JSON** is optional and independent of ActionAudit. It records
   only bounded categorical facts needed when runtime metadata cannot prove them:
   repair-turn counts/reasons, explicit task start/end timestamps, and final
   correctness/validation verdicts. It stores no prompt completion, chain of
   thought, command, file content, credential, deployment identity, Project id,
   Workflow Session id, ClientWindow, tunnel id, or instance id.

On the current runtime, `WEBCODEX_TOOL_REQUEST_TRACE=true`/metadata mode does not
persist a per-trace `events.jsonl` tree. That is not a blocker for the core
baseline because ActionAudit already persists the payload-safe outer/Window facts.
If Runner enqueue observations are required, a capture made with full request
trace can be passed with `--trace-root`; the profiler still ignores all full
payload blobs. A missing trace root makes Runner-request metrics unavailable
rather than zero.

Trace persistence is fail-open observability, so `runner.requests_observed` means
exactly that: enqueue records observed in the supplied trace tree. The report does
not claim that the trace tree is a complete Runner-request ledger.

## Repair-turn and annotation semantics

A **repair turn** is a model turn whose primary purpose is to correct a previous
avoidable tool-contract, callable-surface, argument-shape, or result-field-use
error. Typical examples are a wrong argument shape, attempting an unavailable
callable, reading the wrong result projection field, a JavaScript runtime error
caused by surface misuse, or a follow-up that repairs the preceding tool
selection.

The following are not repair turns: a normal search-then-read decision, ordinary
adaptive branching after new business information, fixing business code after a
real validation failure, or normal Job continuation.

ActionAudit does not prove that semantic distinction. The profiler therefore never
classifies repair turns from model text, tool payloads, or chain-of-thought.
Repair counts come only from the optional sidecar using this intentionally small
vocabulary:

- `invalid_arguments`
- `unknown_callable`
- `wrong_result_shape`
- `javascript_runtime`
- `tool_selection_repair`
- `other_contract_repair`

The sidecar schema is version 1 and rejects unknown fields. Example:

```json
{
  "schema_version": 1,
  "case_id": "<case-id>",
  "variant": "<direct-or-code_mode>",
  "surface": "<direct-or-read_only-or-validation-or-guarded_edit>",
  "base_revision": "<40-hex-base>",
  "case_fingerprint": "<64-hex-case-sha256>",
  "repair_turns": {
    "total": 1,
    "by_reason": {
      "invalid_arguments": 1
    }
  },
  "task_timing": {
    "started_at_ms": 1000,
    "ended_at_ms": 1700
  },
  "correctness": {
    "task_verdict": "pass",
    "validation_verdict": "not_required"
  }
}
```

`repair_turns`, `task_timing`, and `correctness` are individually optional.
Missing sections stay unavailable; they are never coerced to zero. The identity
fields are required so the sidecar cannot be accidentally attached to another
case definition, variant, surface, or Git base. `case_fingerprint` must match the
exact deterministic fingerprint emitted by the benchmark metadata.
A practical flow is to summarize once without `--run-annotation`, copy the emitted
`benchmark.case_fingerprint` into the sidecar, then rerun the same summary with the
annotation attached.

`task_wall_time_ms` is reported only when the sidecar contains both explicit
independent task start and task end timestamps. `observed_span_ms` is never used
as a substitute.

## Timing semantics

For a non-streaming outer call `i`, WebCodex-owned service time is:

```text
service_i = response_handed_at_i - request_observed_at_i
```

For two continuity-eligible, meaningful calls in the same hashed Window and
principal, a canonical serial transition permits:

```text
outside_webcodex_gap_i = request_observed_at_(i+1) - response_handed_at_i
```

The latter may include model inference, host scheduling, network delay, UI delay,
or user delay. It must not be named `model_think_time` or `reasoning_time`.
Non-meaningful calls do not consume the meaningful predecessor. When a report is
scoped to one Workflow Session or trace set, the profiler privately replays
same-Window/same-principal meaningful ActionAudit rows across the selected span so
an interleaved call outside the selection cannot be skipped over. Such context
rows never contribute call/tool/failure counts; if the real canonical predecessor
is outside the selection, that selected gap remains unavailable. Overlap is counted
separately and never converted into a negative gap. Streaming handoff does not
prove response completion. Continuity breaks remain missing evidence.

`observed_span_ms` is only the span covered by observed WebCodex outer-call
timestamps. It is not end-to-end task wall time.

Percentiles use deterministic nearest-rank semantics. For metrics with partially
missing samples, `observed_total` and sample counts remain visible, while `total`
is `null`; missing evidence is never substituted with zero.

## Benchmark cases

[`scripts/agent_loop_cases.json`](../../scripts/agent_loop_cases.json) is the
authoritative case manifest. Each run starts from a fresh clean target at an exact
Git base revision. Direct, Host Code Mode, and Code Mode runs must use the same
case id, exact 40-hex base revision, prompt/correctness definition, and validation
expectation.
The profiler includes a deterministic case fingerprint so changed case definitions
cannot silently compare as the same pair.

The corpus contains ten cases:

- `readonly_review`: read-only status/read/search/diff inspection.
- `focused_edit_validation`: one focused edit, diff review, and successful
  `cargo check`.
- `failed_validation_recovery`: a deliberately failing `cargo test`, diagnostic
  inspection, fix, and successful rerun.
- `long_validation_handoff`: real same-execution Job handoff and terminal
  observation when the validation actually hands off.
- `independent_observations`: independent status/diff/search/read observations,
  intended to expose meaningful-outer-call reduction and actual child concurrency.
- `adaptive_search_read`: search first, then read only the result-dependent
  target ranges.
- `native_batching_observations`: several related reads and searches that have a
  natural native batch shape.
- `result_shape_pressure`: structured search-match output followed by a targeted
  read, exercising layered success/output/optional result fields without telling
  the model that field access is the subject of the test.
- `known_business_failure`: a safe expected missing-file result followed by the
  normal README fallback, distinguishing ordinary `success=false` business
  behavior from host/protocol/JavaScript failure.
- `compact_projection`: larger child read evidence distilled into exactly three
  concise final facts.

The fixture-oriented cases reuse the disposable Rust project recipe already owned
by `eval_coding_loop.sh`; this protocol does not create a second runtime harness.
Read-only WebCodex-repository cases must leave the workspace clean.

The corpus records an expected `code_mode_surface` for every current case:
read-only cases use `read_only`, the long validation/Job case uses `validation`,
and edit-oriented cases use `guarded_edit` with validation outside the mutating
cell where needed. The parser keeps `code_mode_surface` and `dogfood_focus`
optional for historical schema-v1 manifest compatibility, but a current case that
declares a surface rejects a Code Mode run labeled with another surface.

## Paired run protocol

For one case, Direct, Host Code Mode, and Code Mode runs must satisfy all of these constraints:

- same exact case definition and 40-hex Git base revision;
- fresh clean workspace for each run;
- same user task prompt and correctness expectations;
- Direct uses `guidance_profile=direct`;
- Host Code Mode uses `guidance_profile=host_code_mode` and is reported as `variant=host_code_mode`, `surface=host_code_mode`;
- Code Mode uses `guidance_profile=code_mode`;
- new Code Mode captures record `surface=read_only`, `validation`, or `guarded_edit` explicitly; historical schema-v1 `e1` / `e2a` / `e2b` labels remain replay-compatible aliases;
- the only intended experimental variable is the guidance/surface behavior being
  evaluated.

Runtime Project, Workflow Session, Window, tunnel, and host-internal identifiers
may be used transiently to select authoritative evidence. They must not be written
into the manifest, docs, tests, run annotation, or durable benchmark result.

## Typed Surface v1 observed dogfood — 2026-09-18

A real self-hosted dogfood build from the Typed Surface v1 branch ran the three
fixed acceptance tasks below. Each pair used the same task definition and Git
base, and the bounded annotation recorded zero contract-repair turns. These are
observations from this run, not a general speed claim.

These measurements predate the later `search_and_read` compound Direct primitive
added on main. Future paired runs must include that tool when it is the simplest
sufficient Direct control. The A/B outer-call deltas below remain evidence for the
recorded Git base, not an estimate of the current-main advantage; rebaseline them
before drawing a new throughput conclusion.

The exact Code Mode callable contracts measured 9,209 bytes for `read_only`,
12,108 bytes for `validation`, and 11,766 bytes for `guarded_edit`. All stayed
below the 16 KiB hard cap; ordinary Direct discovery did not carry the sidecar.

| Task | Direct outer / meaningful | Code Mode outer / meaningful | Nested evidence | Result bytes | Outcome |
| --- | ---: | ---: | --- | ---: | --- |
| A — bounded repository review | 4 / 4 | 3 / 2 | 3 children, `max_in_flight=3`; 86,298 raw bytes → 1,930 returned bytes | 90,987 → 19,773 | Both runs reached the same clean-worktree, no-diff, timing-contract, and search-location conclusions with zero repairs. |
| B — adaptive read → one guarded edit | 5 / 5 | 4 / 3 | 4 children, `max_in_flight=2`; 3,262 raw bytes → 295 returned bytes | 8,274 → 21,304 | Both edits succeeded on the first attempt, produced the same one-file change, preserved the unrelated function, and passed `cargo check`. The Code Mode result-byte total was larger because one progressive-discovery contract cost about 12 KiB on this tiny task. |
| C — validation launch → Job continuation | 11 / 11* | 5 / 4* | `validation` launched one validator child plus one independent read; `job_handoffs=1`; 46,882 raw bytes → 918 returned bytes | 70,722 → 29,326* | Both runs launched validation exactly once, retained the same execution/continuation identity through terminal success, and did not redispatch validation. |

`*` Task C's raw outer-call and result-byte counts are not a clean throughput
comparison. The client/tool wrapper used short waits while a roughly 50-second
validation ran, producing eight Direct and two Code Mode `observe_jobs` polls of
their respective same-execution continuations. That cadence is Host/tooling
behavior, not model reasoning. The decision-relevant correctness fact is that
both paths launched one validation execution, received one canonical Job
continuation identity, and followed that same execution to terminal success.

Across A and B, the observed `model_round_trip_proxy` fell by two meaningful
outer calls in each pair, with no invalid-argument, wrong-result-shape, or child
call repair. Task A also materially reduced model-visible result bytes. Task B
shows the opposite byte tradeoff for a very small edit: progressive typed
discovery can cost more bytes than it saves when the task itself has little raw
evidence. That is a reason to keep the contract progressive rather than preload it
into every startup response.

## Capture and summarize a run

Record the exact Workflow Session id and 40-hex Git base revision transiently for
each real run. Session selection is authoritative only through ActionAudit;
trace-only input cannot apply `--workflow-session-id`.

Start each benchmark run with its fresh `work_on_project` bootstrap; that call
links its own ActionAudit row through the canonical `WorkOnProject` relation.
Once the bootstrap returns the exact run Session id, every subsequent model-facing
outer call in the run must pass that id as `recording_session_id`. A business
`session_id` does not substitute for ActionAudit recorder provenance. Code Mode
calls that require a business Session should pass both fields with the same exact
benchmark Session id.

A Direct summary with optional sidecar:

```bash
python3 scripts/agent_loop_report.py summarize \
  --audit-db <server-sqlite-db> \
  --workflow-session-id <workflow-session-id> \
  --case-id <case-id> \
  --variant direct \
  --surface direct \
  --base-revision <40-hex-base> \
  --run-annotation <direct-run-annotation.json> \
  --output <direct-report.json>
```

A Host Code Mode summary:

```bash
python3 scripts/agent_loop_report.py summarize \
  --audit-db <server-sqlite-db> \
  --workflow-session-id <workflow-session-id> \
  --case-id <case-id> \
  --variant host_code_mode \
  --surface host_code_mode \
  --base-revision <40-hex-base> \
  --run-annotation <host-code-mode-run-annotation.json> \
  --output <host-code-mode-report.json>
```

A Code Mode summary:

```bash
python3 scripts/agent_loop_report.py summarize \
  --audit-db <server-sqlite-db> \
  --workflow-session-id <workflow-session-id> \
  --case-id <case-id> \
  --variant code_mode \
  --surface <read_only-or-validation-or-guarded_edit> \
  --base-revision <40-hex-base> \
  --run-annotation <code-mode-run-annotation.json> \
  --output <code-mode-report.json>
```

When an `events.jsonl` trace tree was captured and Runner enqueue observations are
useful, add `--trace-root <tool-request-trace-root>`.

## Reported metrics

The schema-v1 JSON summary reports, when evidence is available:

- exact `model_round_trips` remains unavailable because ActionAudit does not persist
  a model response/turn identity; parallel meaningful outer calls may share one
  model response.
- `model_round_trip_proxy`: the existing meaningful outer-call count, retained only
  as an explicit round-trip-pressure proxy. Nested Code Mode children never count.
- outer model-facing calls: total, meaningful, success/failure, and tool-name
  distribution;
- Direct and Host Code Mode canonical-call counts from outer `model_ergonomics` records;
- `host_short_chain`: exact same-Window serial transition count, multi-call chain count, calls participating in those chains, maximum chain length, and observed tool-pair distribution. An exact predecessor outside the selected benchmark run is a chain boundary; a declared serial transition whose exact predecessor cannot be resolved makes the aggregate unavailable. This is ordering evidence only; `same_model_turn_proven` is always false because ActionAudit has no Host-cell/model-response identity;
- authoritative Code Mode composition: nested calls/successes/failures,
  `max_in_flight`, nested tool counts, consequential known/Job/unknown outcomes,
  internal duration, slot wait, optional program input bytes, and nested raw versus
  returned bytes;
- `child_calls.failed`: authoritative Code Mode nested failure count when the
  composition summary is complete;
- WebCodex service time, ToolRuntime duration, canonical outside-WebCodex serial
  gaps, and overlap;
- exact serialized outer `ToolResult` byte totals/distributions;
- optional `repair_turns`, `task_wall_time_ms`, and correctness verdicts from
  the bounded sidecar;
- structured outer `error_kind`, `failure_kind`, and `recovery_kind`
  distributions;
- observed Runner enqueue count/request-kind distribution from trace JSONL.

The summary includes an explicit `availability` object. Consumers must inspect it
rather than assuming an absent metric is zero.

### Input ergonomics (ModelErgonomics v13+)

`input_ergonomics` reads the existing `action_events.summary_json.model_ergonomics`
record. It adds no production table or migration. The report envelope stays schema
v1, consistent with its existing additive metric sections; the persisted telemetry
producer moves from v12 to v13 so old missing fields are not counted as zero use.

The section reports `normalization_events`, `normalization_by_code`, and
`normalization_by_tool`, plus `invalid_argument_rejections` and
`invalid_arguments_by_tool`. Only successful canonical outer invocation records
with a known closed normalization code contribute to normalization counts. Success
is the invocation outcome, including successful durable admission, not a promise
that an asynchronous command eventually succeeds. Failed results do not contribute
even if malformed historical data contains a normalization field.

Coverage is explicit: `measured_canonical_calls`, `eligible_events` (v13+),
`eligible_successful_events`, `legacy_schema_events`, and
`missing_or_invalid_record_events`. The rate is exactly
`normalization_events / eligible_successful_events`; legacy v12 rows and records
missing canonical identity/outcome evidence never enter that denominator. Zero
eligible successes yields null, not zero. Unknown/malformed codes are omitted and
counted in `unrecognized_normalization_events`; they make rate availability false
rather than silently reporting an artificially low rate. The offline reader has a
closed v13 wire-contract allowlist, not a runtime normalization/alias registry.
Consult `availability.input_ergonomics` before interpreting the rate.

`immediate_same_tool_repair_proxy` considers each selected canonical meaningful
`success=false, error_kind=invalid_arguments` call. It inspects only the next
meaningful call in the same exact ClientWindow and principal correlation. Both
calls must have non-streaming, eligible continuity and ordered request/handoff
timestamps; the successor must persist the exact predecessor trace relation with
`window_transition_kind=serial`, have canonical telemetry, and not have ambiguous
trace/predecessor identity. Count only a same-tool successful successor. Never skip
an intervening meaningful call (including one without telemetry), cross principals,
or use an arbitrary seconds threshold. Existing continuity lookup may resolve an
unlinked intervening call or the exact successor outside the selected Session;
those context rows never expand the normalization denominator.

This is a **WebCodex-observed immediate same-tool corrective-call proxy**. It does
not prove model turn identity, a causal repair, or that field spelling caused the
failure. Legacy canonical records may contribute when they retain all necessary
continuity evidence. Missing successor, streaming/overlap, incomplete identities,
or context limits make the proxy `available=false` with null `count`/`by_tool`.
`observed_count`/`observed_by_tool` retain only the proven subset; coverage and
`unavailable_by_reason` explain the gap. A different immediate tool is a measured
non-match, not permission to search ahead for eventual same-tool success. Existing
manually annotated `repair_turns` remain separate and are never filled from this
proxy. Trace-only reports mark input ergonomics unavailable.

No raw arguments, parser error text, hints, stdout, Window/principal identities or
per-event payloads are emitted by this section. Normalization counts describe use,
not an experiment proving saved model turns. Collect v13 coverage after an explicit
deployment, compare repeated correction patterns and known provider spellings,
and measure proposed descriptor bytes before deciding whether another alias has
concrete value. Do not infer the misspelled field from this aggregate alone.

### Authoritative, annotated, and unavailable facts

**Authoritative from ActionAudit:** meaningful outer calls (also reported as the
`model_round_trip_proxy`), outer success/failure/tool distribution, service timing
when timestamps exist, serialized result bytes when persisted, canonical serial gaps
and Host short-chain ordering evidence when continuity evidence exists, and Code Mode
composition counts/timing/bytes/concurrency.

**Manual bounded annotation:** repair turns, full task wall time, and final
correctness/validation verdicts.

**Unavailable when not explicitly recorded:**

- exact model round trips. ActionAudit can prove meaningful model-facing outer tool
  calls and overlap, but it does not persist the model response/turn identity needed
  to prove how many inference round trips produced those calls.
- whether two serial Host Code Mode calls came from one JavaScript cell or one model response. `host_short_chain` intentionally does not infer that fact;
- per-child Code Mode failure-kind distribution. Current composition persists the
  authoritative nested failure count but not each child failure category.
- native batch item counts. A `read_files` or `search_project_texts` child call
  is visible, but ActionAudit does not persist payload-safe item/query counts.
- exact `Promise.all` usage. `max_in_flight > 1` proves overlapping nested
  execution; it does not prove which JavaScript syntax created it.
- generic same-execution Job handoff count and terminal-observation count outside
  the authoritative consequential-child receipt evidence.
- resolved recovery count. `recovery_kind` is guidance on one failed result, not
  proof that a later call resolved it.
- complete Runner-request total from trace files; trace persistence has no
  completeness fence.

Contract/surface failure analysis uses existing structured outer
`error_kind`/`failure_kind`/`recovery_kind` distributions directly. For
example, `invalid_arguments`, `child_call_failed`, or Code Mode runtime failure
kinds are counted when they are actually present. The profiler does not invent a
larger taxonomy or parse error prose to infer one.

## Compare two reports

```bash
python3 scripts/agent_loop_report.py compare \
  --baseline <direct-report.json> \
  --candidate <code-mode-report.json> \
  --output <comparison.json>
```

JSON is the authoritative comparison shape. A Direct baseline can be paired with
either a Host Code Mode candidate or a typed Code Mode candidate. Numeric entries
carry baseline, candidate, candidate-minus-baseline delta, and a `comparable` flag.
The paired output includes at least:

- meaningful outer-call/`model_round_trip_proxy` delta;
- Host short-chain serial-transition, multi-call-chain, participating-call, maximum-chain-length deltas, plus side-by-side observed tool-pair distributions;
- repair-turn delta and side-by-side repair reason counts;
- failed outer-call delta;
- failed Code Mode child calls;
- structured contract/surface failure and recovery distributions;
- outside-WebCodex gap and service time;
- Code Mode nested-call count and `max_in_flight`;
- nested raw-result bytes versus returned bytes;
- task wall time when independently annotated;
- full evidence availability for both runs.

`case_compatibility` requires the same case id, exact base revision, and exact
case fingerprint. `pair_compatibility` additionally requires Direct as the baseline
and either Host Code Mode with `surface=host_code_mode`, or Code Mode with an
explicit `read_only`, `validation`, or `guarded_edit` surface, as the candidate.
`correctness_compatibility` requires both runs to pass the case correctness gate
and any required validation. `throughput_compatibility` is true only when both
the pair and correctness gates pass.

The profiler still emits descriptive numeric deltas when a gate is closed so the
raw evidence is inspectable, but those deltas are not treated as a valid throughput
comparison. No metric ordering, score, winner, or automatic “better” judgment is
produced.

## Privacy and non-inferences

The report never emits raw `ClientWindow` values, principal ids, Workflow Session
ids, trace ids, runtime Project ids, tunnel/instance ids, commands, patches, file
contents, credentials, or tool arguments. The run annotation rejects unknown
fields so it cannot become a catch-all payload store.

These measurements cannot establish model reasoning time, reasoning quality,
intent, exact JavaScript source usage, causal attribution for outside-WebCodex
gaps, or a general claim that Code Mode is faster. They provide bounded
observations for controlled Direct-vs-Code-Mode experiments only.
