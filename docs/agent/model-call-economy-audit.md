# Model-call economy audit (2026-09-26)

Baseline: `2f5d34b4` (`main`, including #691 and #692). The measured MCP
surface is the default-feature Stateless result, without the JSON-RPC envelope.
No live Server/Runner or deployed fleet was changed. **usage evidence unavailable**:
no production ActionAudit distribution was supplied or used. Decisions below use
static contracts and tested canonical gateway parity, not estimated frequency.

## Runtime status

MCP omission (including gateway calls and null arguments) defaults to sparse.
Explicit `compact=false`, without `summary_only=true`, returns full diagnostics.
Canonical ToolRuntime/API omission retains full output. Explicit compact/summary
output intentionally becomes sparse. Sparse collection shares Job and compatibility
logic, branches before inventory/configuration projection, and computes connection
states without diagnostic timestamps or activity observations. Focus does not
compute fleet compatibility rows. Startup's existing owning-Runner brief still
uses its full internal observation and a small model projection.

The supplied **deployed** measurements were ~36,811 full / ~12,220 compact /
~3,522 focused bytes. These are not comparable to a different local registry;
there is no claimed post-deployment measurement. The paired local test uses
0, 1 and 8 registered Runners, one Project each, mixed build versions and source
revisions, and exact `status-0` focus. Counts and byte ceilings are assertions,
not snapshots. Separate focused regressions cover stale/offline observation,
auth filtering, missing client, and active/queued/recovering/lost Job counts.

| Local 8-Runner fixture | Before bytes | After bytes | Reduction |
|---|---:|---:|---:|
| Fleet full | 24,193 | 24,200 | -0.03% |
| Fleet compact/model | 10,742 | 579 | 94.61% |
| Focused compact/model | 2,740 | 665 | 75.73% |

Full observations retain build/time metadata, so their exact serialized length
can vary between builds/runs. Full diagnostics retain authority, effective auth,
Session store, QUIC, capabilities, provider/Project inventories, host context,
build metadata, compatibility rows and connection timestamps. Sparse ceilings:
fleet 900 bytes; focused 1,050 bytes; full fixture 26,000 bytes. Empty fleet
sparse is 603 bytes; one Runner sparse/focused is 579/665 bytes in this run.

## Compact tools/list

| Auth | Apps | Before tools / bytes | After tools / bytes | Byte reduction |
|---|---|---:|---:|---:|
| anonymous | false | 34 / 107,803 | 30 / 82,310 | 23.65% |
| anonymous | true | 51 / 126,894 | 47 / 101,036 | 20.38% |
| scoped | false | 35 / 110,655 | 31 / 84,797 | 23.37% |
| scoped | true | 52 / 129,746 | 48 / 103,523 | 20.21% |
| admin | false | 41 / 122,571 | 37 / 94,821 | 22.64% |
| admin | true | 58 / 141,662 | 54 / 113,547 | 19.85% |

Wrapper compaction alone, holding the pre-pruning inventory fixed (after P1):

| Auth | Apps | Wrapper bytes before | Wrapper bytes after | Saved |
|---|---|---:|---:|---:|
| anonymous | false | 47,396 | 30,464 | 16,932 |
| anonymous | true | 48,595 | 31,298 | 17,297 |
| scoped | false | 48,595 | 31,298 | 17,297 |
| scoped | true | 49,794 | 32,132 | 17,662 |
| admin | false | 54,846 | 35,657 | 19,189 |
| admin | true | 56,045 | 36,491 | 19,554 |

The four gateway-only descriptors save another 8,660 bytes per surface, including
array separators. The canonical registry, parser, and exact manifest remain
complete. Full discovery preserves complete schemas for its admitted direct
inventory; removed direct tools remain available by exact manifest/gateway.
New compact ceilings are 84,000 / 87,000 / 97,000 bytes without Apps and another
19,000 bytes with Apps; experimental Code Mode retains its feature allowance.

## Direct candidate decisions

Bytes below are per descriptor after wrapper compaction, before pruning, on the
admin + Apps surface. Every candidate has an exact `tool_manifest` route. A
“yes” in gateway parity refers to canonical runtime behavior; descriptor-time
Host behavior is called out separately.

| Tool | Prior rank | Compact bytes | App descriptor required | Ordinary coding path | Recovery/blocking path | Canonical gateway parity | Common extra discovery turn | Decision |
|---|---:|---:|---|---|---|---|---|---|
| `list_jobs` | 85 | 1,837 | No | No; identity inventory | Identity loss; formal recovery calls already carry route | Yes | No for normal known-Job continuation | Gateway |
| `stop_job` | 81 | 1,785 | No | Explicit control only | Stop/control | Yes, including confirm and ownership | Only explicit control needing discovery | Gateway |
| `run_detached_process` | 72 | 3,105 | No | No; Runner-independent lifetime | Advanced execution setup | Yes, including scopes/key/capabilities | Only detached setup | Gateway |
| `wait_for_job_terminal` | 79 | 1,850 | No | Bounded terminal fallback | Yes, terminal outcome blocks work | Yes | Discovery would delay blocking fallback | Keep direct |
| `transfer_project_artifact` | 57 | 1,929 | No | Cross-Project artifact transfer | No | Yes; both Project authorities | Only explicit cross-Project transfer | Gateway |
| `import_conversation_files_to_project` | 55 | 2,435 | No App; Host file-reference schema matters | Attachment import | No | Runtime yes; generic descriptor does not offer native file population | May impair Host-native import | Keep direct |
| `session_discussion_summary` | 15 | 1,601 | No | Explicit collaboration inspection | Existing session_hint direct-only target | Runtime yes; current hint is not a parser-ready gateway call | Would make existing hint need discovery | Keep direct |
| `session_handoff_summary` | 16 | 2,502 | No | Explicit context recovery | Yes, lost context | Yes | Expensive extra turn during recovery | Keep direct |
| `rotate_agent_continuation_endpoint` | 19 | 2,316 | No | Durable Agent continuation setup | Exact generation setup/recovery | Yes | Extra setup step before direct presentation | Keep direct conservatively |
| `wait_for_agent_events` | 21 | 3,413 | Descriptor has continuation App association | Agent workflow wait | Blocking/continuation path | Runtime yes; Host descriptor association differs | Could delay wait/continuation | Keep direct |

All required coding tools and all four explicit App presentation tools stay direct.
No mutable tool registration, new gateway, persistent telemetry, execution lifetime,
Host timing, or Job/Session semantic changes were introduced.

## Top 10 final compact direct descriptors

Test-only deterministic attribution uses serialized UTF-8 field bytes. Description
includes its JSON field overhead; business schema includes its schema envelope;
wrappers are the exact difference after removing the seven root wrapper fields;
App bytes cover `_meta`. Remaining bytes belong to names, annotations and JSON
structure. No production telemetry was added.

| Tool | Total | Description | Business schema | Wrappers | App |
|---|---:|---:|---:|---:|---:|
| `search_and_read` | 6162 | 428 | 4693 | 896 | 0 |
| `apply_text_edits` | 4660 | 353 | 3264 | 896 | 0 |
| `search_project_texts` | 3822 | 405 | 2371 | 896 | 0 |
| `run_script` | 3596 | 384 | 2176 | 896 | 0 |
| `run_process` | 3479 | 257 | 2185 | 896 | 0 |
| `wait_for_agent_events` | 3413 | 432 | 1863 | 896 | 70 |
| `cargo_test` | 3297 | 404 | 1855 | 896 | 0 |
| `observe_jobs` | 3291 | 282 | 1971 | 896 | 0 |
| `run_skill_resource` | 3166 | 371 | 1751 | 896 | 0 |
| `run_shell` | 3039 | 259 | 1745 | 896 | 0 |

Reproduce aggregate and attribution with:

```sh
cargo test -p webcodex --lib mcp_tools_list_stateless_serialized_size_budget -- --nocapture
cargo test -p webcodex --lib runtime_status_serialization_budget -- --nocapture
```

## Validation

- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Focused runtime_status: 33 passed; model_surface: 23 passed; MCP compact:
  8 passed; six-way serialization budget: passed. Final library run also covers
  all updated runtime_info, exact manifest/gateway, direct-rank, OAuth scope,
  output projection and MCP App presentation regressions.
- `cargo test -p webcodex-tool-contracts`: 239 passed, no failures.
- `cargo test --lib -p webcodex`: 2,980 passed, no failures, 3 existing ignored tests.
- `cargo check --all-targets --all-features -p webcodex -p webcodex-tool-contracts -p webcodex-runner-registry`:
  passed; existing unused `ObservedRunnerRequest.login` test-field warning remains.

Review found and fixed one integration gap: legacy Job App failure projection
recognized only the bare `list_jobs` recovery call and dropped its new gateway
form. It now preserves either exact empty-argument recovery shape, and rejects
extra arguments; model output still uses the canonical SuggestedToolCall routing
projection. Other initial test failures were stale compact-field, description,
MCP-default, or direct-route assertions; all were resolved without changing
canonical validation. No unrelated flaky failure needed a retry or semantic change.

Canonical ToolSpec remains the source, full diagnostics remain available, and
`call_runtime_tool` still enters canonical ToolRuntime. Exposure changes do not
change authority, permission, idempotency, Job or Session semantics. App
presentation retains descriptor admission. Ordinary pending/passive Job attention
is unchanged. No push, PR, deploy, publish, release or service restart occurred.
