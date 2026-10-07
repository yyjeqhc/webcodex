# `project_build` Adaptive Direct measurement

## Decision

Promote `project_build` to the Adaptive Runtime Direct surface as a
`CoreWorkflow` tool at rank 94, immediately before `project_validate` at
rank 95.

This changes model-facing discovery only. The canonical
`project_build_v1` Runner capability, Standard approval policy, typed build
planning, Job identity, Runner-owned execution, and `observe_jobs`
continuation remain unchanged.

## Base and scope

The experiment was run from exact benchmark base
`b543a416d9e09ba0d5486219a8bc9bd583bf638e`, after the #822 paired benchmark
lane had passed its local evidence gates, fork CI, and security analysis.

The final candidate changed only model-facing discovery: it declared the existing
`project_build` definition Adaptive Direct with rank 94 and reason
`CoreWorkflow`, and supplied a bounded compact selection description that retains
the existing same-Job/no-redispatch rule when compact `tools/list` omits the full
manifest prose. No build planner, Runner, authority, approval, execution, or
result-projection code differed between the two variants.

The #822 runner itself was not repurposed for this A/B. Its comparison axis is
Direct versus Code Mode, while `project_build` is a durable Job tool and the
benchmark contract keeps Job tools outside nested Code Mode. Reusing that axis
for gateway versus dedicated-descriptor exposure would conflate two different
experiments. Instead, this measurement reused the benchmark lane's
fresh-workspace, paired-order, independent-correctness, and ActionAudit
principles with a real Codex Host.

## Method

Two valid A/B pairs were run in opposite order. Every sample used:

- a fresh isolated Server database, Runner, registered Rust Project, and Git
  workspace;
- the same `gpt-6.1-sol` model at medium reasoning effort and the same task
  prompt;
- locked dependency selection;
- the same exact Runner binary and build implementation;
- ActionAudit as the authoritative WebCodex call record;
- independent checks that the build artifact existed and the source workspace
  remained clean.

The baseline exposed `project_build` through Adaptive discovery and
`call_runtime_tool`. The candidate exposed the same canonical tool as a
dedicated Direct MCP callable.

## Results

| Metric | Gateway baseline | Direct candidate |
| --- | ---: | ---: |
| Compact tools | 30 | 31 |
| Compact serialized tool bytes | 52,762 | 54,819 |
| Dedicated-surface cost | — | +2,057 bytes (+3.899%) |
| Successful samples | 2 / 2 | 2 / 2 |
| WebCodex call sequence | `read_tool_manifest ×2 → project_build` | `project_build ×1` |
| Completed Host MCP calls | 3 | 1 |
| Model responses per sample | 8 | 5 |
| Input tokens | 145,619 / 141,884 | 81,289 / 81,255 |
| Wall time | 25,189 / 16,602 ms | 9,980 / 11,564 ms |
| Median wall time | 20,895.5 ms | 10,772 ms |
| ActionAudit failures | 0 / 0 | 0 / 0 |
| Build artifact | 2 / 2 | 2 / 2 |
| Source workspace clean | 2 / 2 | 2 / 2 |

For this fixed task and Host, the final Direct candidate removed two manifest
lookups plus the gateway indirection and reduced model responses from 8 to 5.
Input-token and wall-time measurements moved in the same direction in both pair
orders, but remain Host- and environment-sensitive supporting observations rather
than a general latency claim. The Results table is from the final descriptor,
including its bounded same-Job/no-redispatch compact guidance.

## Interpretation

The dedicated descriptor adds a measurable permanent compact-surface cost:
2,057 bytes, or 3.899% in this snapshot. In return, the real Host consistently
selected the typed build lifecycle immediately, avoided repeated manifest
discovery, preserved all correctness checks, and materially reduced model
round trips and task time in both A/B orders.

That is sufficient evidence to make `project_build` Direct. Rank 94 keeps the
portable build lifecycle adjacent to, and immediately before, the existing
Direct `project_validate` rank 95 without changing either tool's authority or
execution semantics.
