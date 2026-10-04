# Internal composition roadmap

Current series baseline: `main@d62d5b22`, after the foundation PRs and Work Result
synchronous sections (#876) / bounded MCP presentation selection (#877). This is
an implementation roadmap, not a commitment to external plugins or a wholesale
runtime rewrite. PR completion below does not imply production deployment.

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

## Current series: finish the useful internal boundaries

| Stage | Implemented slice | Boundary deliberately retained |
| --- | --- | --- |
| 1: display-family ownership | PR #878 moves complete Job/Git formatters and their registrations to domain modules. All 31 existing functions retain their bodies; four family tests supplement unchanged integration coverage. | Shared text/scalar bounds and optional metadata attachment remain together. No wholesale rewrite of file-view DOM. |
| 2: actual UI read lifetimes | PR #879 fixes three reproduced activity-detail races and gives activity/final diff/workspace diff explicit shared-read ownership. Current views join one promise; generation and per-view checks fence cache cleanup and DOM writes. | Full-text page accumulation and collaboration/reference delivery already have useful local state/fences. Do not merge those different state machines just to reduce file size. |
| 3: startup/Context dependencies | This change extracts pure instruction projection, typed startup catalog bounds, common JSON accounting and a named Context request bundle. Three instruction-shortening paths now share one scope-sensitive implementation. | Startup composition, continuation, verdict and aggregate priority stay together: their existing shared dataflow is not a defect. No service locator or new runtime registry. |

The stage-2 change advances only Work Result v23 to v24; stages 1 and 3 change no
App identity. These three branches touch disjoint paths and can merge separately.
Each leaves existing tool authority, schemas, Session/Job/audit truth, Native Tool
Plugin protocol and persisted formats intact. No dual execution or rollout flag
is needed to keep a partially adopted series usable.

See [MCP families](mcp-presentation-families.md) and
[startup boundaries](startup-projection-boundaries.md). The independent UI branch
contains `docs/architecture/work-result-read-lifetimes.md`; it is not a required
file dependency of this branch.

## Evidence and stopping criteria

The activity defect was verified before implementation: refresh replaced the
pending read's DOM root, then its success/failure updated only the old root; a
collapsed root could also receive late rendering. Regression tests fail on the
parent and pass after the fix. This justifies the small read owner with three real
consumers, rather than an abstract UI lifecycle framework.

Startup instruction and catalog projection have actual independent consumers.
Separating them removes imports of the whole startup assembler from Context
instruction and Skill catalog code. Named request fields make the normal and
fail-closed sidecar target/snapshot differences reviewable. Existing evidence,
identity, authorization and byte-limit regression suites remain authoritative.

Internal stages 1–3 are now addressed to these boundaries, not declared complete
for every possible formatter or controller extraction. Further internal work is
conditional on a demonstrated cross-owner change, duplicate observation, stale
response, resource leak or inability to test a real behavior locally. Do not keep
splitting files, relocating tests or registering helpers after that benefit ends.
In particular, no need has yet been established to replace the exhaustive
execution router, turn continuation facts into a service, rewrite working
collaboration acknowledgement/delivery logic, or build a universal async facade.

Ordinary tools remain usable after each independent merge. Focused tests do not
prove zero regression or live Host behavior; deployment smoke and performance
claims require their own evidence.

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
