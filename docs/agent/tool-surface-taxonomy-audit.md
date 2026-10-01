# Tool surface taxonomy and Direct audit (2026-09-28)

> Historical snapshot: names and exposure below describe the recorded baseline,
> not the v0.5 catalog. v0.5 removes the Action adapter and replaces tool names.

## Scope and evidence

Workspace: special `/root/git/webcodex-review`, baseline
`c70f35d4d5a166b8a596f4e9ff4b71e22e715b41`. No deployed process,
Runner configuration, database, permissions, or execution lifetime was changed.

Source: sf `/var/lib/webcodex/webcodex.db`, `action_events`, opened with SQLite
`mode=ro` and `PRAGMA query_only=ON`. The snapshot contains 271,429 `toolsCall`
events from 2026-07-31 02:40:28 UTC through 2026-09-28 11:18:31 UTC.
The recent window is the seven days ending at that snapshot, not seven calendar days.
Only aggregate operation counts were read; request/response bodies and credentials
are not part of this report.

| Canonical operation | Entire sample | Recent 7 days | Decision |
|---|---:|---:|---|
| run_process | 44,013 | 5,047 | Keep Direct: literal executable + argv |
| run_shell | 36,650 | 3,049 | Keep Direct: actual shell grammar/chains |
| run_script | 9,741 | 510 | Keep Direct: typed program source |
| run_detached_process | 100 | 5 | Already gateway-only; preserve supervisor lifetime |
| run_job | 3,981 | 0 | Keep gateway-only immediate async variant |
| cargo_test | 8,711 | 837 | Keep Direct: executed-test evidence and advanced options |
| cargo_check | 2,443 | 144 | Keep Direct: structured diagnostics and package scopes |
| review_changes | 347 | 347 | Keep Direct: primary bounded review |
| show_changes | 2,270 | 107 | Gateway: specialized workspace/closeout projection |
| session_handoff_summary | 412 | 28 | Gateway: explicit context recovery |
| session_discussion_summary | 82 | 12 | Keep Direct: existing compact collaboration hint target |
| rotate_agent_continuation_endpoint | 7 | 2 | Gateway: occasional setup, not card presentation |
| run_skill_resource | Not observed | Not observed | Gateway: dedicated trusted-Skill specialist |

Calls are not completed tasks, latency measurements, or evidence that one route
causes better outcomes. The fleet spans different versions, feature availability
and workflows; recently introduced tools have especially limited exposure.
`toolsCall` also contains App/internal polling such as `work_result_state` and
`agent_continuation_state`; those are not model tool-selection demand and are not
used to rank the model surface. The operation field identifies the canonical
operation, not a reliable Direct-versus-gateway A/B assignment. No throughput or
Host-token savings are claimed from these counts.

Reproducible aggregation (bind `:end` to the snapshot's maximum `started_at`):

```sql
SELECT operation, COUNT(*) AS entire_sample,
       SUM(started_at >= :end - 7 * 86400) AS recent_7_days
FROM action_events
WHERE action_name = 'toolsCall' AND started_at <= :end
GROUP BY operation
ORDER BY recent_7_days DESC;
```

## One taxonomy, separate workflow recommendations

The former manually curated discovery groups overlapped, and their names and
membership differed from `ToolDefinition.category`. For example, execution was
advertised under `shell`/`jobs` while exact/category manifest filtering used `job`;
`runtime` discovery also contained Session and Skill tools. A displayed group was
therefore not a dependable filter for the same membership.

`ToolDefinition.category` is now the single taxonomy owner.
`group_tool_names_by_category` derives sorted, deduplicated category members from
an already-admitted selection. The helper does not discover or admit tools.
Both `list_tools` and `tool_manifest` use it. Intent rankings and recommended
flows remain useful cross-category task views, without becoming a second taxonomy.

| Category | Contents |
|---|---|
| execution | Native argv, shell, typed scripts, immediate async, detached launch and persistent shell lifecycle |
| job | Job observation, readiness/terminal waits, inventory, stop and continuation adapters |
| validation | Structured project/Cargo/Go validation |
| skill | Skill loading, trusted resource execution and separately admitted management extensions |
| plugin | Plugin gateway |
| memory | Separately admitted Memory extensions; absent when not admitted |
| runtime | Fleet health, discovery, configuration, diagnostics, SSH resources and optional Code Mode entrypoints |
| Other canonical categories | file, edit, git, artifact, project, session, goal, communication, agent_task, agent_wait, coding_agent, browser, computer, lsp, cleanup and feature-gated checkpoint |

This deliberately removes the old cross-list aliases `inspect`, `shell`, `jobs`,
`projects`, `review` and `file_transfer` from advertised category inventories;
callers should use the returned canonical category names. Task intent names such
as `coding`, `audit` and `file_transfer` remain independent and unchanged.
Category discovery includes model-visible specialists; hidden exact-edit tools
and gated operator extensions retain their separate admission policies.

## Direct selection and preserved boundaries

Four descriptors move to the existing gateway: `show_changes`,
`session_handoff_summary`,
`rotate_agent_continuation_endpoint`, and `run_skill_resource`.
Their canonical schemas, metadata, authority, idempotency, capability checks and
handlers remain available; this is not deletion or a merged execution API.

All explicit presentation entries stay Direct. `wait_for_agent_events` also stays
Direct because its descriptor carries the Agent continuation App association;
low or absent observed usage is insufficient reason to remove a Host integration
boundary. Host-native attachment import likewise keeps its dedicated descriptor.

`session_discussion_summary` stays Direct because existing compact Session hints
name it as their recovery target. A prototype demotion required expanding the
shared output schema and repeated hint payload; it was rejected to avoid adding
complexity and schema cost just to remove one descriptor. Collaboration hint,
acknowledgement and Session authority behavior remain unchanged.

Execution is selected by form and lifetime, not just duration. Runner-owned
sync-first work still continues the same execution through `observe_jobs` when
pending. Detached execution retains its extra permission, supervisor ownership
and restart-survival semantics. Persistent-shell Activity uses its semantic
contract rather than the display category. No mega-tool, new gateway, new
permission bypass, or speculative category-to-authority mapping is introduced.

## Measured discovery surface and validation

Measured with the default-feature Stateless `tools/list` serialization budget test.
Byte counts include the optional invocation envelope and gateways, excluding the
JSON-RPC envelope. These are serialized bytes, not Host token measurements.
Baseline counts come from the original inventory test; no exact baseline byte
measurement or causal throughput comparison was performed in this audit.

| Auth profile | No-Apps count before → after | Compact bytes after | Apps-enabled count before → after | Compact bytes after |
|---|---:|---:|---:|---:|
| anonymous | 30 → 26 | 67,351 | 48 → 44 | 87,190 |
| scoped | 31 → 27 | 69,292 | 49 → 45 | 89,131 |
| admin | 37 → 33 | 78,180 | 55 → 51 | 98,019 |

The no-Apps byte ceilings are tightened from 77/79/88 kB to 68/70/79 kB;
existing independently admitted App and experimental-Code-Mode allowances remain.
App-only descriptors contribute to the Apps-enabled inventory; not every
advertised descriptor is an ordinary model-callable tool.

The regression suite checks complete/unique/sorted category membership, category
filter parity, admitted-only extension projection, exact gateway contracts,
existing Session-hint routes, execution metadata and Direct surface budgets.
It also exercises gateway error framing, independent business/recording Sessions,
and enabled/disabled App result projection. The frozen legacy GPT Action surface
is preserved; its explicit handoff description is shortened to its existing limit.

Reproduction commands:

```sh
cargo fmt --all --check
cargo test -p webcodex --lib
cargo test -p webcodex-tool-contracts --all-features --lib
cargo test -p webcodex --lib mcp_tools_list_stateless_serialized_size_budget -- --nocapture
```

No live process was restarted or deployment changed to obtain these measurements.
