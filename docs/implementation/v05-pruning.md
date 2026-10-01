# v0.5 pruning: compact names and one current contract

This pass follows `082dddd2` on the same unreleased v0.5 branch. It does not
publish, deploy, alter stored credentials, bump package versions or change the
parallel v0.4.4 release. No old tool name becomes an alias.

## Names: remove redundancy, not domain distinctions

The literal catalog scan before this pass found 180 unconditional definitions,
mean 19.9 characters, seven above 32 characters and a maximum of 38. The complete
feature-enabled catalog has 181 definitions. Most long names belong to low-volume
Agent/Endpoint/continuation operations; their nouns distinguish real identities.
They are not shortened into ambiguous acronyms or grouped under a mega-tool.

| Previous branch name | Current canonical name | Characters |
|---|---|---|
| `import_conversation_files_to_project` | `import_host_files` | 36 → 17 |
| `search_and_read_project_texts` | `search_file_context` | 29 → 19 |
| `read_session_handoff_summary` | `read_session_handoff` | 28 → 20 |

Scope, file provenance, snapshots, semantic output and admission stay identical.
The Project parameter selects the target; the name need not repeat the whole
source/destination relation. Import remains a Direct HostIntegration tool,
search stays read-only and batch-capable, and handoff remains an explicit read.
`write_job_input` keeps its Direct policy. No effect/retry/permission policy was
changed to meet a name-length target. These character counts are not a token or
end-to-end performance benchmark.

All current descriptors, parsers, generated calls, App references, first-party
scripts and maintained instructions use the new names. The test-only migration
fixture maps 86 original pre-v0.5 names to their final operations. A separate test
rejects the two intermediate unreleased spellings; production consults neither
mapping. Literal Rust operation symbols and native transport tags are not aliases.

## One App resource version, no historical read aliases

Fifty-four historical App resource identities are now rejected. Positive alias
reads are removed, replaced by one HTTP/MCP negative resource test. Current
resource tests still verify content, metadata, admission and native delivery.

| App | Current resource |
|---|---|
| Computer | `ui://webcodex/computer/v12` (unchanged template) |
| Results | `ui://webcodex/changes/v4` |
| Work Result | `ui://webcodex/work-result/v14` |
| Goal Plan | `ui://webcodex/goal-plan/v7` |
| Agent continuation | `ui://webcodex/agent-continuation/v18` |
| Job terminal continuation | `ui://webcodex/job-terminal-continuation/v2` |

Tool renames change the callable names inside several App templates. Keeping the
old URI could leave a Host using cached JavaScript that calls retired tools even
after the Server schema changed. Advance the current cache identity instead of
serving new JavaScript under old resource aliases. A stale mounted View must be
reopened from the refreshed descriptor; it is not auto-adopted or silently
redirected. Durable Endpoint/Job/Goal state and callback authority are unchanged.
The dormant continuation workflow remains available for deliberate future Host
integration; low usage is not evidence that its authority/replay state is dead.

## Retired permission-setting migration

Only `WEBCODEX_AUTHORITY_MODE=trusted_agent|restricted` selects a mode. Old
`dev_auto_approve` / `require_approval` values and automatic old-variable
translation are removed. The old variable's **presence** is rejected, even if
empty or accompanied by a valid new setting. This small rejection guard is not
compatibility: ignoring an old `require_approval` configuration could silently
activate the trusted default. Diagnostics tell the operator to unset the old
variable; stored permissions and hard Project/scope/path guards are not migrated.

## Retired 0.4 Runner startup compatibility

The 0.5 boundary also removes the narrow pre-0.4 Runner startup migration
surface. Automatic/default/profile config discovery now selects only
`runner.toml`; `WEBCODEX_AGENT_CONFIG` no longer participates in default-path
selection; `project_registry_dir` is the only accepted registry config field;
and default registry placement is always `project-registry/`. A historical
`agent.toml` or `projects.d/` artifact is not used for discovery or precedence.

The retired `projects_dir` field is still detected only to reject it before
runtime use. Silently ignoring it would allow an old config to start against a
different default registry, so this is a fail-closed retirement guard rather
than compatibility or normalization. Explicit `--config PATH` remains exact
and can use any caller-chosen filename. Historical config and registry names
remain on sensitive-path/package exclusion lists so stale credentials and
registration records are not accidentally exposed.


## CI: eliminate a repeated Server build, retain actual consumers

The `contract` job replaces three filtered root-crate test invocations with
`cargo test --locked -p webcodex-tool-contracts --all-features --lib`. The mandatory
Linux Server shard already owns all default root tests; its success still gates
the stable aggregate. This removes a second root compilation in another job and
also checks optional tool schemas not covered by default builds. No claim of a
measured CI wall-time saving is made.

Retain MCP conformance, Windows/macOS execution, old-installed-version upgrade,
artifact provenance, process-tree and release verification. Those test different
clients, operating systems or existing installation data, not old model tool
names. The v0.4.3 Windows upgrade fixture protects real installed binaries; it is
not removed just because the new API is intentionally breaking. Removed-name and
retired-resource rejection tests prevent reintroduction; they are not alias code.

## Dead production helpers

Delete Desktop's unused `ResolvedBinaries::resolve` / `resolve_until` wrappers and
unused executable-name helper. Production already calls `resolve_source_until`
with an explicit selected runtime. Drop two never-consumed fields from Desktop's
private deserialization projection; the typed runtime contract remains the
actual compatibility/identity check.

Gate the macOS-only process fixture helper/export to macOS tests and keep a trace
helper's test-only `Value` import out of normal builds. Consolidate three identical
stateless App test adapters, while retaining the separate Window-aware carrier
fixture. No OS cleanup test is removed.

The dormant push-callback integration was inspected but not partially pruned:
gating only its two Runtime wrappers leaves an unusable production carrier and
dispatch path behind. That experimental edit was withdrawn. The whole existing
controller contract and its App/push replacement tests remain unchanged; retiring
that integration requires a separate complete decision, not a dead-code warning
suppression or a silent loss of the planned Host continuation extension point.

## Verification

The covered source was frozen and rechecked against saved SHA-256 values before
local commits. Final evidence on Linux:

| Check | Result |
|---|---:|
| Production Server `cargo check --locked -p webcodex` | Passed, no warnings |
| Complete default Server library | 3,156 passed; 3 existing ignored |
| Tool contracts with all features | 279 passed |
| Full Linux Desktop library | 270 passed; 4 existing ignored; no warnings |
| MCP App DOM/message-order suite | 328 passed |
| Frontend types, tests, production rebuild | Passed; 157 tests in 24 files |
| Python script suite (includes CI ownership/path-risk) | 381 passed; 1 skipped |
| Optional Codex adapter | 54 passed; 3 skipped |
| Workspace/Desktop rustfmt, whitespace, links, Python syntax | Passed |
| Workspace dependency policy | All 21 packages passed |

The 21 focused permission/resource checks are included in the Server total; the
41 CI ownership/path-risk checks are included in the Python total. They are not
additional independent test counts. No existing test was ignored or loosened to
achieve these results. Development checks caught the explicit serde rename needed
for the importer, the test-adapter ownership mismatch, and a deleted method-header
boundary; each was corrected before the final runs. The incomplete callback-only
pruning experiment was withdrawn rather than adding dead-code allowances.

A full pre/post catalog comparison covers all 181 definitions: only three names
change, while categories, visibility, Direct policy, effects, scopes, risk and
execution remain the same. Structural input/output business keys, required sets
and bounds match after replacing those tool identifiers and ignoring explanatory
description/title text. This does not claim the old authority-variable behavior
is compatible; that separate configuration change is intentionally breaking.

No dependency or lockfile change, OS service/configuration mutation, schema refresh,
push, PR, deployment or release occurred. No native Windows/macOS execution,
real Host cache test, external conformance referee, V8-backed Code Mode or newly
built dogfood process E2E was run in this pass. Existing in-process Server/MCP,
resource/admission, recovery and Desktop coverage passed; a local unit test is
not a claim that an already-mounted remote Host View has refreshed.
