# Explicit targets and discovery-first maintenance

This change is based on `9783ccca716ccc0426df0c0c248aa8b270df39e6` in
`special:/root/git/webcodex-review`. It integrates main through
`3cbf4341591fe438004c389da6cbd231200bee5c`, including the bundled MCP App registry
refactor, via a merge into the feature branch. It does not deploy or restart services.

## Choose a target without starting work

`resolve_workspace` is a **Gateway** observation. An exact Runner plus a registered
absolute `path`, or literal case-insensitive `query`, returns a unique `workspace`
or bounded `candidates`. Ambiguity is counted before limiting the response. There
is no first-match selection, filesystem search, symlink guessing, registration,
Workflow Session creation, or SSH routing fallback. It observes only the current
caller-visible registered inventory. A missing match does not prove a directory
is absent from that machine. Project refs are existing principal-scoped selectors,
not authority. The response is capped by candidate count and serialized bytes.
An in-progress, pending or degraded Runner inventory returns `resolution=incomplete`
and observed candidates, never a unique selection or a negative existence claim.

```json
{"tool":"resolve_workspace","arguments":{"client_id":"special","path":"/root/git/webcodex"}}
```

`list_projects(include_git_summary=true)` and resolver candidates expose Git
branch/HEAD/dirty with `source=runner_inventory`, `freshness=unverified`. Null is
unknown, not clean. These are not fresh filesystem observations or commit fences;
use the existing Git/validation workflow before dependent mutations.

`list_runners` accepts literal `query`, `status`, and bounded `limit`. Exact
selectors, then query/status, precede the response limit. Offline includes stale
(non-connected) rows; stale retains its existing heartbeat-expired meaning.
`summary_only` uses the existing read-only Console aggregate and active Job index,
not full Project bodies or terminal Job history. Full mode now uses that same
active-Job aggregate rather than scanning terminal history; its diagnostic fields
and existing default are retained. Truncated results require narrower selectors; no snapshot
cursor or new health-state taxonomy is invented.

## Workbench is optional explicit context

The existing Workbench now starts with **Runner → Project → optional exact
Workflow Session**. Project discovery is filtered on the Server before paging;
changing Runner clears Project, Session, previews and outstanding scope-dependent
responses. Named launches reauthorize the exact supplied identities. No list
response selects a Session, and late launcher results cannot overwrite a user's
new selection. Goals remain principal-owned, not implicitly filtered by Project.

The explicit **Use selected context** button revalidates the exact Project and
Session before submitting a bounded context declaration through the existing
`ui/update-model-context`. Only Host acknowledgement makes it attached. No
`ui/message`, automatic turn, Session creation or execution is involved. A Host
without text/resource context support receives copyable text, not a false claim
that a Session was attached through a Project resource link. Context is not an
execution credential and every tool reauthorizes its own target.

The background overview timer now requests the existing `automatic=true`
presentation lease. Explicit refresh still requests fresh observation. Hidden
views pause polling; selecting another Runner invalidates older results.

## Explicit batch registration maintenance

`unregister_projects` is a **Gateway** mutation with a default observational
`dry_run=true`. It accepts 1–16 explicit canonical Project/revision pairs. All
identifiers and duplicate entries are rejected before any effect. Executing needs
both `dry_run=false` and `confirm=true`.

Previews inspect current visible registrations and revisions without dispatching
Runner operations; `execution_checks_pending=true` makes clear that preview does
not reserve the registration or promise future success. Execution reuses the
canonical single-item owner/revision/active-Job/lifecycle path for every item,
sequentially. Results are indexed, per-item and non-atomic. Known pre-execution
conflicts remain partial results. An uncertain or malformed native outcome stops
later items rather than replaying a batch or interpreting an unavailable receipt
as evidence that no effect occurred. No files, branches, Sessions or Goal history
are deleted. Broad keep-list reconciliation, alias migration and directory moving
are intentionally separate work, not implicit extras in this operation.

## Dynamic tool discovery

No new Direct tool is introduced. The two new tools use `call_runtime_tool`.
The existing `read_tool_manifest` accepts bounded literal keyword `query` and
`limit`, in addition to exact tool names, categories and intents. Keyword lookup
filters admitted descriptions without materializing all input/output schemas;
exact-name lookup still returns the one canonical input schema and current route.
A `limit` on an exact-name lookup is harmless and ignored; conflicting semantic
filters still reject. Keyword lookup uses literal, case-insensitive AND terms, not
fuzzy identity resolution or an unbounded synonym/alias registry.
Queries never register tools, grant authority or expose hidden extensions.

`maintenance` and `resources` intents collect cross-category tasks without adding
a second category system. Audit discovery begins with read-only resolution, not
Session bootstrap. Selection-critical descriptions put purpose and target choice
in the first sentence because compact discovery uses that sentence. Window peer
communication is categorized as communication rather than Workflow Session.
The baseline audit covers all 177 default definitions (137 model-visible and 40
hidden); hidden App/extension contracts are not promoted just to make them easier
to find. After adding the two Gateway tools, the default catalog has 179 definitions
(139 model-visible, 40 hidden), with the same 26 declared Direct tools. Git summary,
workspace summary and staged-diff descriptions now lead with user purpose and
selection semantics rather than saying they are retained implementation specialists. Current capability gates continue to control availability.

## Validation

Integrated-main validation (default Runtime unchanged by the final feature-only projection cleanup):

| Scope | Result |
| --- | --- |
| Default Runtime library | 3,299 passed, 3 ignored; no warnings |
| Tool Contracts, all features | 284 passed; no warnings |
| Tool Runtime Contracts / audit | 55 passed; no warnings |
| Workbench Node protocol tests | 47 passed |
| Workspace all-features test compilation | 29 test artifacts; zero compiler warnings/errors |
| Targeted inventory | 23 passed |
| Default discovery | 38 passed |
| Final all-features discovery, including executable examples | 41 passed |
| Final schema projection / literal preservation | 2 passed |

Focused suites overlap the full Runtime suite. `--no-run` means compilation, not
execution, and ignored tests are not counted as executed. These are Linux tests,
not native Windows/macOS execution or a live Host deployment.
UI coverage includes explicit Runner selection, scoped Project paging, no default
Session, stale-response rejection, selected-context acknowledgement/fallback,
revoked selections and automatic versus explicit overview refresh. Runtime tests
cover unique/ambiguous resolution, current authorization, cached Git truth,
preflight/duplicate rejection, exact per-item CAS and uncertain-result stopping.

## Remaining limits

Registered-path matching is literal, not a durable role/alias service. Git values
from inventory may be old. Batch unregister is bounded but not a distributed
transaction. Disk/service metrics still belong to explicit native observations;
no generic machine monitor or `runner_overview` is added. Deployment of the Server
and bundled App is necessary to use these changes; old loaded Host schemas may
need refresh for added input fields, independently of runtime Gateway discovery.

### Scope of Host integration

This change controls WebCodex's own manifest, descriptors, Gateway routes and
Workbench App. It does not change when a Host loads schemas or promise that all
Direct descriptors are loaded when a user omits an @ mention. A client can discover
`read_tool_manifest` once, query a bounded set, read the selected exact contract,
and call the admitted Gateway. Workbench model-context attachment is exercised by
protocol tests, not a production Host rollout in this task.

## Additional discovery-path review

The optional Code Mode exact manifest had an obsolete `tools.git_status` example
and copied both read-only examples into every larger stage. Its validation and
edit projections could exceed their existing budgets. Examples now use the
canonical `get_git_status`, current sparse edit `changed` and pending Job receipt,
and show the selected stage rather than repeating simpler-stage prose.

Projection compaction removes null default annotations but keeps non-null defaults
and every validation constraint. Literal values under `const`, `enum` and `default`
are instance data, not schemas, so they are never recursively stripped. A regression
keeps business keys named `description`, `title` and `default` intact in those values.
No callable tools, authority checks, semantic schema restrictions, or size gates
are dropped or relaxed.

The advisory `code_mode_callable_contract` is now version 2: it declares the common
`success / output / error` envelope once and uses output-relative field paths.
Collections and validation freshness keep useful nested paths; diagnostics and
follow-up objects are shown as whole values rather than repeated implementation
subfields. Actual ToolResults, input schemas, execution admission and public
Runner protocol are unchanged. Canonical constraint and useful-path assertions
remain in the tests, reconstructed against the shared envelope.

Final serialized sizes are 7,725 bytes (read-only), 10,528 (validation), and 15,337
(guarded edit), inside the unchanged 10/13/15 KiB soft gates and 16 KiB hard gate.
The final entire-workspace all-features no-run compile produced 29 test artifacts
with no compiler warnings or errors.

The failed pre-fix logs are retained under `/tmp/webcodex-workbench-target/continuation/`.
They include the reproduced incomplete-inventory uniqueness claim, obsolete
fixture/API and descriptor assertions, and optional Code Mode failures. Fixes are
validated on the same source boundary rather than treating a retry as a fix.

### Reproduction and review evidence

```sh
cargo test --locked -p webcodex --lib
cargo test --locked -p webcodex-tool-contracts --all-features --lib
cargo test --locked -p webcodex-tool-runtime-contracts --lib
node --test src/mcp_tests/workbench_app.test.mjs
cargo test --locked --workspace --all-features --no-run --message-format=json
cargo test --locked -p webcodex --all-features --lib schema::discovery
cargo test --locked -p webcodex --all-features --lib code_mode_projection_tests
cargo fmt --all -- --check
git diff --check
```

For the final feature tests, the exact root test executable from the successful
Cargo JSON build artifacts was invoked with the two filters above. The runner
reported 41 and 2 passing tests respectively; this avoids rebuilding the same
source under an incidental alternative workspace feature-unification graph.

Remaining ergonomic opportunities are bounded multi-name exact contract lookup,
consistent inspection budget names across batch tools, and clearer visual distinction
between dropdown selection and context already acknowledged by the Host. They are
not implemented by adding another Direct tool in this change.

No DB or ledger schema, Runner capability or Runner wire-generation changes are
required. Server and bundled App deployment are required, and an already loaded
Host descriptor may need a schema refresh for new inputs. No production benchmark,
service restart, implicit Session selection, directory migration, or broad
reconciliation operation was performed.
