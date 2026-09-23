# Tool composition research and development plan

Status: active experimental design record. E1 read-only orchestration, the E2a
structured-validation/Job foundation, and the narrow E2b guarded structured-mutation
slice are implemented behind the experimental Code Mode feature. The governing rule
is still that composition may reduce model-facing round trips but may not shortcut
existing tool, Session, Job, permission, audit, Project, recovery, or Runner boundaries.

## Motivation

Current WebCodex already has strong primitive tools and several homogeneous batch
surfaces: `read_files`, `search_project_texts`, `observe_jobs`, guarded edit
batches, and structured validation. `run_shell` and `run_script` can also execute
multiple local commands in one remote request.

Those capabilities reduce some transport cost, but they do not provide a general
way to combine *heterogeneous canonical WebCodex tools* in one model-facing MCP
round trip. A review may still need a sequence such as:

```text
git review summary
-> model decision
-> source search
-> model decision
-> multi-file read
-> model decision
-> focused validation
```

For ordinary local inspection, the child process itself is often much shorter
than the surrounding model/tool decision cycles and remote round trips. Using one
large shell script can reduce those cycles, but making shell the universal answer
would discard typed schemas, Project resolution, SHA guards, permission checks,
Workflow Session evidence, Job identity, structured validation, bounded results,
and audit policy. The target is therefore composition *above* canonical tools,
not a return to a remote-shell-only surface.

Window activity correlation provides a useful measurement boundary for this
work. WebCodex can measure time from one inbound request through Server/Runner
processing and can see when the next request arrives. It still cannot observe the
model's private reasoning or prove why time outside WebCodex elapsed. Performance
telemetry must preserve that distinction.

## Research snapshots

### WebCodex Next

The inspected WebCodex Next design already separates model-facing call economy
from canonical effect identity. Its `Apply` envelope can admit multiple typed
operations in one request while creating an independent durable Execution for
each operation. The safety model is "atomic admission, ordered effects": retry,
authority, recovery, and effect truth remain attached to each canonical
Execution rather than to a synthetic aggregate effect.

Later focused adapters apply the same principle more narrowly. In particular, a
multi-file patch adapter translates one model-facing call into several ordinary
canonical patch operations without inventing a new Node batch effect, transaction,
or rollback contract. This is a useful precedent for reducing model-facing
round trips without collapsing underlying effect identities.

WebCodex Next dogfood also found that a very large composed `Apply` schema could
project poorly through a ChatGPT host while focused composition-free tools in the
same server projected explicit arguments correctly. The lesson is not to weaken
the canonical internal model. Keep the canonical substrate expressive, but keep
the ordinary model-facing composition entry small and host-friendly.

### pi-coding-agent

The inspected pi-coding-agent runtime supports multiple tool calls from one
assistant turn. Independent calls can run concurrently, while a tool can request
sequential execution. Its file mutation queue adds a more specific resource rule:
mutations of the same canonical file are serialized while different files need
not share that lock.

The useful ideas are:

- concurrency is an explicit runtime/tool contract rather than a guess from tool
  names;
- a conservative sequential tool can fence a batch when necessary;
- serialization can eventually be scoped to the resource that actually carries
  the race, instead of forcing all unrelated work through one global lock.

WebCodex should adopt the first principle before attempting generalized resource
locking. Resource-level concurrency should be added only for concrete mutation
cases whose authority and race semantics are already understood.

### Codex code and Code Mode

The inspected Codex code has two relevant layers. Its normal tool runtime can
allow multiple compatible tool calls to execute concurrently, with handlers
explicitly opting into parallel execution. In addition, Code Mode exposes a
restricted JavaScript orchestration environment where model-written code can call
canonical tools and can use ordinary control flow or `Promise.all(...)` to compose
them.

The important boundary is that the orchestration code itself has no ambient Node
filesystem or network authority. Effects occur through `tools.*` calls, which
return to the existing canonical tool router. Code therefore expresses
orchestration while tools continue to own authority and effects. Recursive Code
Mode invocation is also excluded.

That shape is more attractive for WebCodex than a large user-facing JSON DAG:
code can express dependencies and parallel branches compactly, while the Server
can keep a small outer MCP schema and a closed nested-tool boundary.

## Prerequisite: remove primitive contract friction first

Composition must not be used to hide avoidable friction in the underlying tools.
Before implementing this plan, ordinary primitives should follow the standing
contract style: harmless bounded parameters normalize instead of wasting a turn,
failures expose actionable typed reasons, successful results stay sparse, and
follow-up calls have one parser-ready representation.

Only after that baseline is stable should telemetry decide whether a repeated
sequence represents real independent work, a pruning candidate, or a composition
opportunity. A sequence caused mainly by schema correction, duplicate recovery,
or discovery noise is a primitive-contract bug, not evidence for a composition
runtime.

## Combined design principles

A WebCodex composition layer should follow these rules:

1. **One outer call may contain many canonical child invocations, but it must not
   collapse their identities.** Child authority, audit, validation, Job state,
   recovery evidence, and result semantics remain those of the existing tools.
2. **Orchestration never becomes authority.** The parent call supplies the same
   authenticated principal and trusted adapter context; every child still passes
   its ordinary scope, Project, permission, and governance checks.
3. **Code has no ambient effects.** No filesystem, network, process, environment,
   dynamic import, or host API is available except through an explicitly exposed
   `tools` object.
4. **No recursive composition.** A composition program cannot invoke the
   composition tool itself, directly or through adaptive discovery.
5. **Parallelism is opt-in and bounded.** A tool definition or adjacent canonical
   registry owns its concurrency policy. Unknown tools are sequential or denied,
   never optimistically parallel.
6. **Adaptive discovery is not bypassed.** Composition must not become a backdoor
   for model-hidden, gateway-only, or otherwise non-admitted tools. The first
   version should use a closed direct-tool allowlist; any later dynamic admission
   must consume the same canonical discovery policy as direct invocation.
7. **The parent is not a transaction.** If one child effect succeeds and a later
   child fails, WebCodex does not invent rollback. Existing child result truth
   remains authoritative.
8. **Output and execution are bounded independently.** Program size, child-call
   count, concurrent child count, wall time, emitted bytes, and retained child
   result bytes all need explicit ceilings.
9. **Window and Workflow Session semantics stay explicit.** A `ClientWindow`
   remains observation/correlation only. Composition never supplies sticky
   recorder behavior or infers a Workflow Session from a Window.
10. **Direct tools remain first-class.** Composition is an optimization for a
    known plan, not a requirement for ordinary single-tool work.

## Current execution selection contract (T1)

Ordinary model-facing execution primitives now declare a small canonical selection contract on `ToolDefinition`. It has four independent dimensions: `form` (`native_argv`, `typed_script`, `shell_command`, `structured_validation`, `persistent_shell_command`), `lifetime` (`runner`, `supervisor`, `session_shell`), `start` (`sync_first`, `async_immediate`, `existing_session`), and `continuation` (`observe_jobs`, `session_shell`, or `none`). `tool_manifest` projects these facts directly instead of asking the model to reconstruct them from descriptions or route names.

This vocabulary is selection metadata, not a new execution layer. `lifetime=supervisor` does not grant authority; none of the four fields changes Project resolution, scope checks, permission/approval, Runner capability admission, timeout behavior, Job transitions, retry safety, OutcomeUnknown semantics, detached-process fencing, or Session-shell lifecycle. `run_shell` and `run_job`, for example, remain distinct canonical tools: both are shell commands owned by the Runner, but the former is `sync_first` while the latter is `async_immediate`. `run_process` and `run_detached_process` are both native argv forms, but their lifetime carriers are `runner` and `supervisor` respectively. Persistent `session_shell_exec` reuses an existing Session shell rather than becoming a Job.

Structured validation keeps its tool-specific effects and evidence. `cargo_fmt`, `cargo_check`, `cargo_test`, and `go_test` share the selection shape `structured_validation / runner / sync_first / observe_jobs`; this does not imply identical mutation semantics. In particular, `cargo_fmt check=false` remains synchronous ensure-format mutation while `check=true` may use the existing same-execution Job handoff.

T1 also makes filtered recommended-flow projections explicit when only part of a canonical flow is visible: partial projections identify themselves and list omitted canonical members instead of silently pairing complete-flow prose with a truncated tool list. Composition work must build on these canonical primitive semantics; T1 does not add JavaScript Code Mode, a generic composition runtime, or a universal execution abstraction.

## Current result follow-up contract (T2)

T2 adds a second, orthogonal vocabulary for **result follow-up**, not tool selection. `ContinuationSemantics` classifies follow-up by `kind` (`page`, `batch`, `observe`, `checkpoint`, `refine`) and `carrier` (`position`, `index`, `opaque_token`, `observation_token`, `revision`, `none`). It is published only when the result itself needs to disambiguate a dynamic follow-up shape; tools whose input/output contract already names one unambiguous token mapping do not repeat a static descriptor on every result. Concrete domains still own encoding, validation, fences, lifecycle, and failure behavior.

The underlying mappings remain intentionally heterogeneous. Ordinary positional pagination such as `next_offset` or the next file range is `page / position`; aggregate packers may use `batch / index` boundaries internally, but when Runtime can reconstruct the complete follow-up and fit it inside the model-result budget it publishes only the parser-ready call; `git_diff_hunks` uses scope/fence-bound opaque page or hunk-fragment tokens; Job, CodingAgentRun, and Session-message stream cursors are `observe / observation_token`; and parameter changes such as increasing `max_result_bytes` or `max_hunk_lines` are `refine / none`. The wire fields remain domain-specific because they carry different invariants. CodingAgentRun and Session-message observation, for example, expose `observation_token` and the matching `after_observation_token` input directly without repeating `continuation_semantics` in every result.

Those similarities do **not** create a shared token runtime. Git continuation keeps its scope/fence/MAC and committed-range identity. Job observation tokens remain exact-Job delta cursors. CodingAgentRun tokens retain their own Run binding, epoch, MAC, stale-epoch and history-loss rules. Session-message observation retains its Session-bound durable cursor. Task-context recovery instead uses an explicit `session_handoff_summary(session_id=...)` observation with an internal event/message snapshot fence; it publishes no model-context token. No token implementation parses another domain's token merely because both project the same semantic vocabulary.

T2 also introduces one small shared `SuggestedToolCall { tool, arguments }` result-expression primitive and a matching schema helper. It only represents a bounded parser-ready advisory call already chosen by a domain producer. It does not dispatch, grant authority, carry retry permission, or replace the actual continuation identity. Existing domain envelopes remain intact: for example, `read_files` still reports `safe_cursor`, source SHA and snapshot stability; Git still distinguishes later-record continuation from omitted-current-hunk refinement; Session task-context recovery remains an explicit handoff observation rather than a generic continuation token.

Failure recovery remains a separate lane. When a domain can prove one complete parser-ready `{tool, arguments}` action, that `suggested_call` is the sole actionable machine expression and does not repeat `recovery_kind`. When only a non-actionable recovery family can be proven, the result retains `recovery_kind` together with the family hint. The retired `RecoveryTool` enum and `recovery_tool` output field are no longer published. `retry_same` continues to mean exact safe replay only, and `outcome_unknown` never becomes retry authority. Successful or partial business continuation does not acquire failure-recovery metadata merely because more work remains. Task-context recovery is explicit through `session_handoff_summary(session_id=...)`; it neither ACKs nor resolves collaboration messages and grants no authority.

After T1 and T2 the primitive foundation therefore has four distinct views:

```text
tool selection          execution form / lifetime / start / continuation primitive
result follow-up        continuation kind / carrier
failed invocation       recovery kind / domain-proven advisory action
task-context recovery   explicit Session handoff
collaboration retention request-scoped message ACK
```

These contracts reduce model inference cost without introducing a `NextAction` state machine, universal cursor, generic execution manager, workflow graph, or Code Mode. Any future composition layer must consume these canonical primitive facts while preserving the underlying child tool identities and domain state machines.

## Proposed shape

Conceptually:

```text
ChatGPT / model host
        |
        | one outer MCP request
        v
compose_tools / code-mode entry
        |
        v
orchestration frontend/runtime
        |
        v
CanonicalOrchestrationHost
        |
        +--> canonical child invocation A --+
        +--> canonical child invocation B --+--> existing ToolRuntime
        +--> canonical child invocation C --+
                         |
                         v
                 existing Runner / Job /
                 Session / audit machinery
```

A model-facing program could eventually look like:

```javascript
const [review, matches] = await Promise.all([
  tools.git_review_summary({ project: "agent:special:webcodex" }),
  tools.search_project_texts({
    project: "agent:special:webcodex",
    queries: [{ pattern: "ToolConcurrencyPolicy", pattern_mode: "literal" }]
  })
]);

emit(review);
emit(matches);
```

The exact language/runtime is deliberately undecided. Restricted JavaScript is
the leading ergonomic shape because it naturally represents dependencies,
branching, `Promise.all`, and result inspection, but an embedded runtime should
not be selected until portability, startup cost, memory bounds, cancellation, and
sandbox guarantees are measured. An internal test representation may use a
structured plan; that does not imply shipping a generic JSON DAG as the ordinary
model-facing contract.

The E1.x implementation now exercises this split concretely: V8 remains the
current frontend, while root-side canonical authority/admission/dispatch and
composition accounting live in `CanonicalOrchestrationHost`. This does not decide
that V8 is permanent. It establishes a seam where a later structured-plan or
reusable TypeScript frontend can be compared without creating a second execution
authority.

## Canonical nested invocation boundary

The composition runtime should call one internal canonical dispatch entry rather
than invoke handlers directly. That entry must receive trusted context from the
outer request and then perform the same policy path as a direct model call:

```text
nested tool name + typed arguments
        |
        +--> admission / model-surface allowlist
        +--> canonical ToolCall parsing
        +--> auth + scope checks
        +--> exact Project resolution
        +--> permission / specialized governance
        +--> normal tool dispatch
        +--> Job / validation / audit handling
        +--> bounded ToolResult
```

Each child needs an independent logical invocation identity. A parent composition
correlation id may connect those children for diagnostics, but it is not an
idempotency key, Job id, Session id, permission token, or replacement for the
child invocation identity. Canonical target, recorder, context sidecars/message ACK, expectation,
and private wrapper fields must remain host-owned invariants; frontend admission
policy may only narrow this boundary further, never re-enable those fields.

Specialized gateways should be excluded from the first version. If they are ever
admitted, they must continue through their existing action-specific governance
boundary rather than becoming generic nested callbacks.

## Concurrency contract

The implemented E2a runtime contract is deliberately smaller than the earlier two-axis sketch:

```text
ToolCompositionPolicy::Denied
ToolCompositionPolicy::Sequential
ToolCompositionPolicy::Parallel
```

This policy is canonical `ToolDefinition` metadata. The default, including unknown/future tools, is `Denied`. The exact E1 read allowlist is `Parallel`; `cargo_check`, `cargo_test`, and `apply_text_edits` are `Sequential`; everything else remains denied unless deliberately reviewed later. Frontend admission is separate and explicit: composition policy never makes a tool reachable and grants no scope, permission, Project, Runner, retry, or idempotency authority. E1 still admits only reads, E2a admits reads plus the two validators, and E2b admits reads plus only `apply_text_edits`.

`CanonicalOrchestrationHost` enforces the policy with one composition-local shared/exclusive scheduling fence. `Parallel` canonical child invocation intervals can overlap; `Sequential` is exclusive against every child interval. The fence ends when canonical ToolRuntime invocation returns, including an existing same-execution Job handoff. Durable Jobs then own their ordinary lifetime independently, so two predetermined E2a validators can enter sequentially yet later run concurrently as Jobs.

E2b adds one deliberately coarse cross-host fence only for orchestration-originated canonical mutation. A process-local registry is shared by cloned `ToolRuntime` state and keyed by canonical resolved Project id. An E2b mutation acquires that Project's exclusive fence before canonical dispatch and holds it until the child `ToolResult` is known; different Projects remain independent. Read-only orchestration and E2a validation never acquire this registry, and ordinary direct mutations deliberately bypass it. The experiment does not define file locks, Git locks, lock hierarchies, a generic resource graph, distributed locking, or Runner-protocol locking. Finer locking is deferred until telemetry demonstrates that Project-level serialization is a real bottleneck.

Even parallel-eligible tools need a composition-wide concurrency cap. Parallelism
must improve latency without turning one model call into unbounded Runner/process
fan-out.

## Failure, cancellation, and Job semantics

Child business failures should remain ordinary child results. The orchestration
language can choose fail-fast behavior (`Promise.all`) or explicit isolation
(`Promise.allSettled`) without rewriting already-completed child truth.

The composition runtime itself may fail for invalid code, an unavailable nested
tool, resource-budget exhaustion, runtime exception, cancellation, or a hard
orchestration timeout. Such a parent failure must not claim that already-entered
child effects did not occur.

Long-running child execution now uses the existing WebCodex Job path directly. E2a admits only structured `cargo_check` / `cargo_test` as consequential children and caps their nested synchronous handoff preference at five seconds without changing total `timeout_secs`. An unfinished validator returns the same canonical `job_id` and parser-ready ordinary Job continuation; E2a creates no second background-process or cell lifecycle and does not restart the child. `observe_jobs` intentionally remains outside nested Code Mode in E2a.

A Server restart should not attempt to resume arbitrary process-local
orchestration code. Canonical child effects keep their existing recovery truth.
This is another reason not to make the composition program itself a new durable
workflow resource.

## Workflow Session recording

This is the main reason to stage composition conservatively.

An explicit authorized outer `recording_session_id` may be propagated as trusted
recorder context to admitted child calls, but each child must record through the
same existing Session path it would use directly. The composition wrapper must
not become a second authoritative business event that double-counts the children.

E1 read-only/re-observable children established the first slice. E2a proves the consequential validation rule: every child records through its ordinary canonical Session path; the parent does not become a fake validation identity or transaction. E2b applies the same rule to mutation: the nested canonical `apply_text_edits` event owns first-class Edit provenance and state-change evidence, while the outer `code_mode_exec_mutating` wrapper emits no generic top-level `state_changed` and does not independently set `repository_edit_observed`. No-op, dry-run, and provably pre-start edits remain non-provenance; a successful nested edit with canonical `state_changed=true` qualifies exactly as a direct edit would. Final Changes continues to compare the Session Git baseline with the complete final workspace, not with a Code Mode-local diff.

Once all already-started children have drained to a known result, same-execution Job handoff, or truthful uncertainty, a consequential parent is decorated from the latest monotonic Session state. It accepts only trusted outer ACK metadata, never lets JavaScript ACK guidance, and never compresses child revisions or evidence into a synthetic event.

Composition must never use Window affinity or recorder-gap hints to fill in a
missing recorder. The Window work remains diagnostic only.

## Window activity and audit visibility

Composition should reduce outer MCP calls without making the new Windows view
opaque. At minimum the operator must be able to distinguish:

```text
1 outer model-facing composition call
N canonical nested tool invocations
M Runner/process requests
```

The current Window event can remain the outer request boundary, but the design
needs bounded child evidence: for example a parent trace id plus child invocation
ids and safe child tool names. Arguments, outputs, command bodies, patches, raw
host Window ids, and credentials must not be copied into ActionAudit merely for
composition diagnostics.

This also gives performance telemetry a clean accounting model. Future timing
should distinguish, where the existing layers can prove it:

```text
outer MCP request duration
Server admission / dispatch time
nested child duration
Runner request/queue round trip
actual child-process duration
response projection time
```

The interval between a completed WebCodex response and the next inbound request
is outside WebCodex. It may be model inference, host scheduling, user delay, UI
behavior, or something else and must not be labeled as model reasoning without
host evidence.

Agent Loop Observability P1 makes that outer timing boundary concrete for
ordinary non-streaming MCP calls. Let `A_i` be request observation and `H_i` be
response handoff. WebCodex-owned service time is `H_i - A_i`; the adjacent
meaningful Window gap is `A_(i+1) - H_i`; and the call cycle is
`A_(i+1) - A_i`. Serial calls should therefore approximately satisfy cycle =
service + outside-WebCodex gap. Pairing uses hashed `ClientWindow` plus canonical
authenticated principal, never Workflow Session or Project identity. Overlap is a
separate relation, streaming handoff is excluded from completed-response gap
semantics, and restart does not reconstruct process-local predecessor state.

The P1 metrics boundary is fail-open and low-cardinality. It can emit
`tool_runtime_duration_seconds`, `mcp_call_duration_seconds`,
`window_inter_call_gap_seconds`, `window_meaningful_calls_total`,
`tool_result_bytes`, `tool_outcomes_total`, and `window_overlapping_calls_total`
without labeling by Window, Session, Job, request, trace, Project path, command,
or payload. The current repository has no Prometheus/OpenMetrics endpoint, so the
boundary and structured observations remain exporter-independent while the
Runtime Console supplies a bounded dogfood projection. Baseline data comes before
SLO targets: WebCodex-owned service/failure SLIs and interaction-efficiency gaps
must remain separate because only the former are wholly service-owned.

## Staged implementation plan

The concrete Direct-vs-Code-Mode capture/report protocol is documented in
[`../experiments/agent-loop-baseline.md`](../experiments/agent-loop-baseline.md).

The experiment now uses these concrete stage names:

```text
E1   read-only orchestration
E2a  structured validation + Job/effect foundation
E2b  guarded structured mutation: E1 reads + one apply_text_edits attempt
E2c  decide whether validation and mutation should coexist in one cell;
     consider selective generic process/shell only if telemetry justifies it
E3   implemented generic asynchronous Job terminal attention v1
E4   product/stability decision
```

### E1 — implemented read-only control

`code_mode_exec` remains a separate `Observe / Read / PureRead / project:read` control surface. Its closed read allowlist, V8 limits, canonical host boundary, composition telemetry, and direct-tools-vs-Code-Mode dogfood establish whether moving inspection decisions below the outer model round trip is actually useful.

### E2a — implemented consequential foundation

`code_mode_exec_effectful` is feature-gated and conservatively declares `Execute / JobRun / Standard / NonIdempotent / job:run`. It admits all E1 reads plus only `cargo_check` and `cargo_test`. It adds no source mutation.

E2a closes the correctness prerequisites that were previously future work:

- `ToolDefinition` owns default-denied Parallel/Sequential composition policy;
- the canonical host, not JavaScript, owns scheduling and a monotonic close gate;
- a Sequential waiter cannot begin canonical dispatch after frontend termination;
- timeout/JS failure drains children that already crossed canonical dispatch;
- a separate sparse effect receipt distinguishes known results, same-execution Job handoffs, and outcome uncertainty;
- failed validation is a known business result, not uncertainty;
- outer failure never claims no effect or whole-program retry safety after consequential dispatch;
- Job handoff preserves ordinary `job_id`/continuation and ordinary later observation;
- final parent Session continuity is projected from the latest canonical Session state after child evidence;
- outer durable audit stores only bounded effect counters, not Job tokens or child payloads.

`observe_jobs` is deliberately not nested: Code Mode has a 30-second frontend maximum while ordinary Job observation may wait up to 100 seconds. The parent returns the canonical handoff and the model later observes it normally.

### E2b — implemented guarded structured mutation

`code_mode_exec_mutating` is feature-gated and conservatively declares `Mutate / ProjectWrite / Standard / NonIdempotent / project:write`. It admits the E1 read set plus only canonical `apply_text_edits`; validators, `apply_patch`, whole-file write, generic process/shell, Job observation, gateways, Computer control, Git/Session mutation, and recursive Code Mode remain excluded.

The first-version mutation contract is intentionally narrow:

- one E2b cell may cross the canonical mutation boundary at most once, counted by canonical `ToolEffect::Mutate` rather than a tool-name registry;
- the one mutation may still use `apply_text_edits`' existing transactional multi-file batch and `read_revision` guards;
- a guarded `replace_exact` may state `expected_match_count` for repetitive exact text, with optional `line_scope`; dry-run match evidence is bounded and does not authorize the later actual request;
- a second mutation attempt is rejected before canonical business dispatch and cannot reach the Runner;
- same-Project E2b mutations are serialized by the process-local Project fence described above; different Projects may proceed independently;
- direct writes are unchanged and are not silently serialized against the experimental fence;
- mutation receipts preserve optional authoritative `state_changed`: missing mutation truth fails closed to `outcome_unknown`, never false;
- JavaScript failure or timeout after dispatch preserves completed mutation truth, while unresolved work after the existing bounded five-second reconciliation remains `outcome_unknown`;
- E2b adds no retry engine, mutation transaction coordinator, JS patch parser, filesystem API, or second write protocol.

The primary adaptive workflow is therefore `canonical read -> JavaScript decision -> one canonical apply_text_edits -> canonical post-edit inspection`. Validation is intentionally outside the mutation-capable cell. Combining validation Jobs and later mutation would otherwise make validation freshness ambiguous without a workspace-snapshot fence; that question is deferred rather than hidden.

### E3 — implemented generic Job terminal attention v1

E3 is a generic Job lifecycle capability, not a Code Mode child tool. The model-visible operation is `wait_for_job_terminal(job_id, idempotency_key)`: it arms one caller-owned, bounded, one-shot terminal wait for one exact already-dispatched public Job. `job_id` remains execution identity; the wait never starts, retries, stops, replaces, or redispatches work. Observation tokens remain opaque log/lifecycle cursors and are not E3 identity or authority. The sparse terminal event contains only exact Job identity, canonical terminal status, and a bounded outcome classification; detailed logs and validation evidence remain available only through ordinary `observe_jobs`.

Authoritative triggering comes only from the existing RunnerRegistry Job lifecycle. Accepted sequenced Runner terminal updates, protocol-violation terminalization, stop/lost/timeout paths, and authoritative receipt/reconciliation hydration all converge on the existing first-terminal-observation hook. That hook marks an in-memory candidate while the registry mutex is held; an immutable sparse terminal fact is captured and sent to the E3 sink only after the mutex is released. E3 therefore creates neither a second Job tracker nor SQLite I/O under the RunnerRegistry async mutex. Stale, duplicate, out-of-order, wrong-instance, or otherwise rejected Runner updates cannot create a second logical terminal event.

The durable `wc_job_terminal_waits` store is separate from Durable Agent waits because its authority model is different. Registration first re-authorizes the exact visible Job and persists only a digest of caller ownership plus the minimum Job authority partition. Workflow Session, ClientWindow, MCP request id, recording provenance, and observation cursor are absent from execution authority. Exact keyed replay returns the same wait; changed reuse conflicts. Registration uses a two-snapshot handshake around the durable insert: a terminal Job is matched immediately, while a transition racing the insert is caught either by the post-lock terminal sink or the second canonical snapshot. Active-wait retention is derived from the structured execution ceiling, recovery grace, and existing Job terminal-retention window rather than a short arbitrary TTL.

Restart preserves durable event truth but not a process-local Host callback. Waiting rows survive Store reopen; Runner reconciliation or retained terminal-receipt hydration can match them to the same Job without redispatch. A triggered event remains `pending` when no eligible carrier exists. A failed preflight remains pending because the durable dispatch fence was not crossed. `delivered` means the configured carrier accepted dispatch after that fence. `delivery_unknown` means dispatch crossed the durable fence but acknowledgement is not authoritative; it is never silently retried. A Server takeover conservatively converts any old process-local `prepared` delivery to `delivery_unknown`.

E3-H1 adds the Job-native MCP App Host carrier without wrapping Jobs in Durable Agents or Agent Endpoints. The one model-visible presentation operation is `present_job_terminal_continuation(wait_id)`: presentation always names one exact caller-owned wait, independently re-authorizes that wait and its underlying Job visibility, and creates only a bounded App card. It cannot infer a wait from Project, Workflow Session, ClientWindow, Window Peer, credential, or recent activity, and it cannot start, retry, stop, replace, or otherwise mutate the Job lifecycle. The App-only bind/state/prepare/finish/unbind coordination operations remain ModelHidden.

The H1 carrier keeps Host routing separate from Job authority. A View generates a cryptographically random process-local `binding_id`; canonical `ClientWindow` arrives only from MCP Host sideband and is already domain-separated/hashed. The process binding is therefore `(wait_id, authorized wait principal, ClientWindow, binding_id)`. ClientWindow is a routing-continuity fence only: it is not Job, Project, Workflow Session, execution, or bearer authority. The raw Host session value is neither accepted as tool input nor persisted. Window Peer is deliberately not a continuation transport: H1 does not read or write peer-message rows, and a peer id never substitutes for either wait authority or ClientWindow continuity.

Only one process-local App carrier is active per exact wait. Exact binding replay is idempotent; a refreshed View from the same ClientWindow may replace its stale binding and immediately fences the old View; a different ClientWindow cannot steal a live carrier. The carrier registry is process-local, so Server restart clears it. `automatic_resume_available` is consequently calculated for the exact wait at call time: an eligible exact binding makes only that wait true, an unrelated or stale/unbound wait remains false, and restart makes surviving waits false until an eligible App rebinds. Exact `wait_for_job_terminal` keyed replay reports this current per-wait truth rather than a global MCP-App capability.

The MCP App is a bounded pull bridge rather than a Server-push channel. While a card is live it reads only its App-hidden exact state at a bounded foreground/background cadence; no model-facing `observe_jobs` polling is needed merely to discover terminal completion. When canonical E3 terminalization moves the durable wait to triggered/pending, the View calls prepare, which re-authorizes the exact wait and live binding before crossing the existing durable `pending -> prepared` fence. The returned private message contains only sparse Job id/status/outcome continuation instructions. The View then has one `ui/message` dispatch site and records only Host `dispatch_accepted` or conservative `delivery_unknown` through finish. Host acceptance means the Host accepted the dispatch request, not that a fresh model turn has already completed.

No second delivery ledger exists. Before prepare, binding/state/preflight failure leaves durable pending truth recoverable by a later valid binding. After prepare, missing/malformed responses, Host rejection/timeout/response loss, View replacement, or teardown must never reopen pending merely to retry; authoritative prepared state is reconciled to `delivery_unknown` when safe evidence cannot prove that dispatch never occurred. A pending wait may rebind after Server restart, while the existing restart recovery converts any pre-restart prepared attempt to `delivery_unknown` and never redispatches it. `delivered` and `delivery_unknown` stop App polling. `observe_jobs` remains the canonical optional detail/recovery primitive for logs and validation evidence.

Deterministic controller and Host-harness tests can prove the local topology `running Job -> wait -> explicit App bind -> canonical terminal -> pending -> prepare -> exactly one ui/message -> finish`, including zero model-facing `observe_jobs` calls for terminal discovery, no second execution, no second logical event, and no second Host dispatch. They do not prove live ChatGPT auto-resume. That production claim remains behind a separately authorized candidate deployment built from the then-current `main` plus H1 and a real long-Job test whose first post-terminal model activity is the Host-generated continuation turn.

E3 and E3-H1 deliberately do not enter the E1/E2a/E2b allowlists. `wait_for_job_terminal`, `present_job_terminal_continuation`, all App-hidden Job continuation operations, and `observe_jobs` remain outside Code Mode. E2a and direct validators still return the same canonical Job handoff, E2b remains mutation-only, and no Code Mode cell waits on Jobs. The intended relationship is `E2a or Direct validator -> canonical Job handoff -> E3 terminal attention -> explicit H1 Host carrier`, with an optional later `observe_jobs` call only when the model needs detailed evidence.

### E2c / E4

E2c still decides whether validation and mutation should coexist in one cell and whether any selective process/shell composition is justified by telemetry. Finer mutation locking is also evidence-driven, not assumed. E3 remains independent of that decision and is not evidence for nested Job waiting. Product/stability commitment comes only after the experimental execution, E3 Host-integration dogfood, and broader evidence are mature.

## What not to build

This work should not become:

- a generic durable workflow/DAG engine;
- a new Session or Agent lifecycle;
- a replacement for Jobs;
- a shell-script wrapper marketed as typed composition;
- a way to bypass adaptive discovery or hidden-tool policy;
- a new permission evaluator;
- an implicit sticky recorder keyed by Window;
- an automatic retry engine;
- a transaction/rollback abstraction over unrelated tools;
- an unbounded parallel task runner.

WebCodex already has the canonical primitives. The composition layer should stay
thin enough that deleting it would leave the underlying tools and their direct
semantics intact.

## Acceptance criteria

A production-ready first slice should satisfy all of the following:

- one outer MCP request can execute multiple explicitly independent canonical
  inspection tools;
- each child sees the same auth/Project/permission rules as direct invocation;
- composition cannot reach tools outside its current admitted allowlist;
- child invocation/audit identity remains observable and bounded;
- concurrent read-only children demonstrate wall-time benefit under controlled
  delay tests;
- sequential policy reliably prevents unsafe overlap;
- total child count, concurrency, program size, wall time, and output are bounded;
- cancellation/timeout never fabricates rollback or "no effect" truth;
- Window activity can still explain outer versus nested WebCodex work without
  exposing raw host identity or payload bodies;
- direct tools continue to work unchanged;
- no Workflow Session is selected from Window identity;
- guarded mutation reuses canonical `apply_text_edits`, allows at most one mutation attempt per cell, preserves exact state-change/uncertainty truth, and leaves direct mutation semantics unchanged.

## Open design questions

1. Which embedded restricted-JavaScript runtime, if any, meets Linux/macOS/Windows
   portability, startup, memory, cancellation, and sandbox requirements?
2. Should the model-facing API expose named `tools.<name>(args)` methods, a single
   `tools.call(name, args)` primitive, or generated helpers over the currently
   admitted direct surface?
3. How should nested tool schemas be made ergonomic without recreating the large
   composed-schema projection problem observed in WebCodex Next?
4. What is the smallest canonical concurrency metadata that supports Phase 1
   without prematurely designing resource locks for mutation?
5. How should parent/child invocation identities appear in ActionAudit and the
   Windows view while preserving current privacy policy?
6. Does E2b dogfood show enough same-Project mutation contention to justify anything finer than the current coarse Project fence?
7. Can validation and mutation safely coexist in one E2c cell without a canonical workspace-snapshot/freshness fence, or should ordinary post-cell validation remain the boundary?
8. Does nested short Job observation remove enough outer turns to justify widening E2a, or is ordinary `observe_jobs` the better boundary?
9. Which timing boundaries can the current Runner protocol prove directly, and which require additive privacy-safe telemetry?

These questions should be answered with focused prototypes and dogfood traces,
not by widening the first implementation preemptively.
