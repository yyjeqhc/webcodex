# ToolRuntime domain convergence

Status: incremental internal decomposition; public ToolRuntime remains the facade.

The goal is to reduce dependency access and duplicated business interpretation,
not to move lines into more crates. Protocol, authorization, execution placement,
Session recording, and final model projection keep their existing owners.

## First service: presentation retention

`tool_runtime::presentation::PresentationRuntime` owns the review-snapshot registry,
Changes-snapshot registry, and automatic Work Result workspace cache. ToolRuntime
holds one Arc; request-local clones share retention and independent runtimes start
empty. The service owns lookup/insertion and locking, not just field grouping.
It has no ToolRuntime back-reference, Runner, database, permission evaluator, or
Session store. Producers still acquire canonical facts through the facade.

Each store retains its own lock, TTL, byte/cardinality budget, identity tests, and
freshness rules. No store lock crosses an await. In particular:

- review snapshots remain caller/Project/optional-Session scoped;
- final Changes and workspace-inspection snapshots retain separate retention quotas;
- Changes lookup alone is not authorization: readers still verify the exact caller,
  Project, Session, snapshot, and allowed file before producing content;
- automatic workspace reuse retains its short lease and source/target fences;
  explicit refresh and closeout remain fresh observations;
- restart empties retention rather than replaying an effect.

Read revisions, validation-source fences, and committed-diff continuation signing
are deliberately NOT presentation-cache members. They participate in mutation or
continuation correctness and require a separate ownership argument.

## Second service: workspace reads

`ToolRuntime::reads` shares one `WorkspaceReadRuntime` across clones. It owns the
read cache and revision registry, exposes observe/resolve/read operations, and
reuses one `ReadRevisionTarget::from_resolved` constructor for reads, cache keys
and edit preflight. The six target fields must move together: canonical Project,
path, Runner client, Runner instance, root path and optional root fingerprint.
The service takes only a narrow `ProjectFileReader` for physical work, not a
ToolRuntime back-reference. Read-scope authorization, range transfer bounds and
single-flight behavior remain unchanged.

`read_files_deadline` stays a ToolRuntime value: changing one view's policy must
not mutate another clone's policy. Neither writes nor new observations invalidate
old revision handles proactively. They still map to exact target + SHA, and the
Runner checks that SHA at use. Even a cache hit needs a fresh full-file SHA probe;
the cache saves range transfer, never filesystem evidence. This also covers edits
made outside WebCodex. Restart creates a fresh epoch and empty read cache.

## Candidate boundaries, not a mandatory decomposition tree

| Candidate | Useful boundary | What must remain explicit |
| --- | --- | --- |
| Workspace reads | Read cache and revision registry; view-local batch budgets | Revision epoch, exact owner/root fences and fresh Runner SHA evidence; no write-triggered invalidation |
| Execution / Jobs | Query/observation state and execution budgets | Runner lifecycle, dispatch certainty, Session evidence and terminal reconciliation are distinct |
| Projects | Authorized resolver/inventory service | Runner and root incarnation, registration versus execution authority |
| Collaboration | Domain-specific communication/query services | Agent, Conversation, Goal, Workflow Session and ClientWindow identities must not be merged because they share a database |
| Presentation | Non-authoritative bounded retention | Exact authorization, snapshot identity and source freshness stay at their owning boundaries |

Extract the next service only when a concrete caller can depend on that smaller
service instead of the whole ToolRuntime. Avoid a service that immediately takes
`&ToolRuntime`, a blanket Deref back to it, a new global cache, or one coarse lock.
Crate extraction is a later packaging decision, not the first step.

## Registration consistency without a large macro

ToolDefinition remains the identity/visibility/governance catalog. ToolCall owns
typed requests and Serde tags; the input schema cache is derived from ToolCall.
The handwritten tool_name match is retained: replacing it with runtime JSON
serialization or a large macro is not justified by removing one mechanical table.

`request_schema::registration_consistency` verifies exact sets, not just counts:
all derived schema tags equal all Definition/known names, and model specs equal
only the visible subset. It constructs, parses, serializes, and deserializes every
compiled variant, checking tool_name and the canonical Serde tag at each boundary.
Model-hidden variants are tested without registering them as public tools.
Run the checks for default, each optional contract feature, and both features;
feature-disabled variants must disappear rather than leave a stale registration.

The exhaustive round trip exposed a concrete optional read-fence bug:
ReadFilesItem serialized an absent expected_read_revision as explicit null, while
its custom parser intentionally rejects null. Serialization now omits the absent
field, preserving optional schema shape and strict validation of supplied fences.
Boundary-value tests cover both omission and valid fences without admitting null.

## JSON semantic audit

JSON mutation at the last projection boundary is expected. Type only decisions
that assign business meaning, rather than every get/remove call.

`EditOutcomeFacts` now centralizes transactional edit receipt interpretation for
Runner response validation and final edit projection: actual/no-op effects,
applied/planned counts, dry-run conventions, rollback and no-effect proof. A
missing planned_count retains the established older receipt convention; an
explicit null or malformed planned_count does not. Execution state reuses
ExecutionOutcomeFacts. Neither result success nor lack of an error proves that
an edit completed; omission of redundant lifecycle fields still needs every
canonical completion/effect proof.

Remaining review-coverage and validation-evidence JSON requires producer/consumer
mapping before further typing. The small reads/searches projection adapters mainly
capture request context and delegate bounded projections, so moving those adapters
alone would not eliminate business-semantic duplication. Keep continuation tokens,
source completeness, validation provenance and retry uncertainty unchanged.

## Canonical execution consumers

Mutation observation and Code Mode receipts consume `ExecutionOutcomeFacts`, but
retain their different proof conditions. Non-null Job markers, even malformed,
prevent mutation quiescence. Code Mode handoff remains exactly `pending` or
`terminal=false`, with the established continuation/Job-string precedence; it
must not use the broader `is_pending()` predicate. Business failure alone does
not discard known execution facts. The shared Browser/Computer terminal helper
preserves unknown state names and presence-before-type failure-kind fallback.
Runner raw mutation-receipt validation and audit Job extraction remain separate.

## CI cancellation versus required lane results

The four aggregate jobs use `!cancelled()` as their job-level status guard. GitHub
[defines cancelled() as workflow cancellation](https://docs.github.com/en/actions/reference/workflows-and-actions/expressions#cancelled)
and [re-evaluates job conditions during cancellation](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-cancellation).
A concurrency-superseded workflow should stop instead of starting always-running
aggregates that turn cancelled dependencies into additional failures. This does
not convert cancellation into success, and does not waive branch protection.

The unchanged aggregate scripts independently require successful required lanes;
a child result of failure, cancelled, unexpected skipped, or missing is rejected.
Only a classifier-disabled lane may be skipped. `test_ci_aggregate_cancellation`
pins those four scheduler guards and runs the actual checked-in Bash bodies over
these cases. It is local contract evidence, not a hosted scheduler/concurrency
simulation; final native CI and actual cancellation observations remain separate.

## Out of scope

No legacy installer/migration support is removed. No MCP protocol, Runner lifecycle,
public tool schema, credential policy, persistent format or platform boundary is
changed by this internal convergence work.
