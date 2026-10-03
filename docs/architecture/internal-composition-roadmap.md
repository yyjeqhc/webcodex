# Internal composition roadmap

Baseline: `main@3cbf4341`, after Context/Guidance (#870), owned model-result
projection (#871), Desktop panels (#873), MCP App registration (#874), and the
independent validation handoff fix (#875). This is an implementation roadmap,
not a commitment to external plugins or a wholesale runtime rewrite.

## Objective and protected core

Make non-authoritative display/context policies locally maintainable and testable
without creating a second execution path. Preserve ordinary tools at every merge.
Authority, Project resolution, credentials, Session/Job truth, process ownership,
audit, effect certainty, retry safety, read revisions and mutation fences remain
in their existing Rust owners. An exhaustive authoritative router is not itself
a defect; do not replace it merely to reduce the number of `match` statements.

Static bundled modules do not need hot-reload registries, global service locators,
locks, RPC or generic disposable stacks. Add such mechanisms only for an actual
lifetime or external-consumer requirement. File count and line count are not
success metrics; smaller change surfaces and preserved behavior are.

## Landed foundation

| Area | Landed ownership | Explicit limitation |
| --- | --- | --- |
| Context | A provider owns key, scope/surface requirements and implementation. | Bundled Rust only; trusted implementation still receives internal runtime services. |
| Guidance | Ordered core/profile contributors share startup and explicit refresh. | Rebuild required; not runtime-editable or external advice. |
| Model result | Family capture/bind/consume follows canonical recording. | Bundled trusted formatters; not a safe third-party modifier boundary. |
| Desktop extensions | Six-panel definitions plus retained shared drafts and controller. | No dynamic third-party WebView code. |
| MCP Apps | Exact resource identity, templates, listings and descriptor metadata. | App identity registry does not decompose every HTML renderer/controller. |

The domain contracts are documented in [context/guidance](builtin-context-guidance.md),
[model results](builtin-result-projection.md), [Desktop panels](bundled-extension-panels.md)
and [MCP Apps](bundled-mcp-apps.md).

## Current series: complete useful display boundaries

The independent Work Result sections change extracts synchronous checks/review,
Job summary and activity/detail rendering with display-only dependencies. It
adds deterministic bundled generation and source tests; HTML changes advance
Work Result v22 to v23. Host RPC, exact input validation, identity, polling,
lazy reads, drafts, focus/scroll and teardown remain in the existing controller.

This branch consolidates the eight bounded MCP presentation names into one
registration/dispatch inventory, and moves all validation display formatting with
its registrations into its own module. Job/Git formatting can remain in the
parent while this change is deployed. No App URI, template, wire or data migration
is required by this branch. The two changes touch disjoint paths and can land in
either order. These PR-local results are not a claim that both are already in main.

## Next stage 1: finish display-family ownership

Move the remaining Job/Git MCP display formatters and their existing tests into
cohesive modules. Avoid merging MCP display metadata with canonical evidence or
ToolRuntime model-result compaction: they serve different consumers. Review the
remaining Work Result file/diff/output view code, extracting synchronous formatters
before moving asynchronous controllers.

Acceptance: identical supported tool set; unchanged output allowlists, text/item
bounds, relative-path filtering, chronology and uncertain-state labels; no new
card admission or eager requests. Keep the old owner working for unmigrated
families. This stage is complete when family-local changes no longer require
editing a second dispatch table, not when every helper has its own file.

## Next stage 2: isolate actual UI lifetimes

Extract one asynchronous view at a time: file/diff preview, activity details,
then collaboration/reference UI. Give each view an explicit exact-scope binding
and an owned request generation; retain the existing deadline, cancellation,
cache and deduplication semantics. A collapsed, replaced or torn-down view must
not apply late replies or restart a request merely because its component moved.

Acceptance: tests for scope switch during an in-flight request, collapse/reopen,
repeated refresh, late success/failure, retained drafts/focus and teardown. Preserve
Host-origin checks, exact Project/Window/Session selection and immutable snapshot
identities. No new generic event bus or automatic retries. Registry APIs should
not be expanded until a second real consumer needs them.

## Next stage 3: reduce remaining core projection coupling

Split startup-brief composition by actual data ownership (instruction projection,
extension catalog bounds, continuation facts) and narrow oversized context
parameter groups where current callers demonstrate a need. Review the existing
ResultProjection helpers left behind in the parent. Keep authorization and data
loading distinct from deterministic serialization/budget selection.

Acceptance: startup/context payload bounds and instruction continuations preserved;
request-local guidance and principal isolation unchanged; no repeated observation
or second source of canonical truth. Do not turn `ToolRuntime` into a generic
service locator. Stop when the remaining shared code is genuinely shared.

## Conditional stage 4: external extension pilot

Only after a concrete consumer cannot be served well by bundled contributors or
the existing Native Tool Plugin API, select ONE read-only pilot: e.g. explicit
Project-scoped CI context or project-specific guidance. Before implementing a
bridge, record the trust model and a bounded protocol, exact generation/schema
fences, admission/scopes, output allowlist, time/byte quotas and failure behavior.
Native Tool Providers and behavior extensions must remain distinct concepts.

This is the point to implement owned activation/disposal: unregister in reverse
order, cancel work, fence late replies from old generations and test disable,
crash, reload and shutdown without stale registrations. Static bundled registries
do not need this machinery earlier. External content is advisory/untrusted input,
not permission or control state. No credential payload or unrestricted runtime
handle is passed to an extension. An isolated process is not by itself an authority
boundary; the Rust broker must enforce the allowed operations.

Acceptance: one useful end-to-end dogfood example; timeout, oversized/malformed
output, conflicting keys, crash and stale-generation tests; extension failure
cannot rewrite main success, Job identity, retry certainty or canonical audit.
Measure added latency and request volume before broadening the feature. If the
pilot offers no material benefit, stop rather than shipping a generic Mod platform.

## Later, only with a concrete policy/UI need

A restrict-only pre-execution hook may deny or constrain, never expand authority,
retarget a Project, replace arguments or substitute execution/permission/audit.
Third-party renderer code needs a separately isolated UI context and bounded host
API, not direct import into a credential-adjacent WebView/Tauri context. Neither
feature is a prerequisite for the internal refactor and neither is authorized by
this roadmap alone.

## Common merge and delivery gates

Each PR implements a usable slice, can be reviewed independently, preserves its
unmigrated peers, and has a documented fallback/rollback boundary. Freeze relevant
source before final focused tests; reuse valid evidence rather than repeatedly
running full workspace tests. Verify generated assets, local links, formatting,
workspace boundaries and the relevant existing regression suite. Keep missing,
failed, stale, skipped and successful evidence distinct.

No structural refactor deploys/restarts live services or migrates data implicitly.
When App HTML changes, publish a new template URI; do not claim same-URI refresh
or a retired-URI alias will repair Host caches. Live Host/desktop smoke and any
performance claims require separate actual evidence. A passing unit suite is not
a zero-regression guarantee or an unattended-continuation guarantee.
