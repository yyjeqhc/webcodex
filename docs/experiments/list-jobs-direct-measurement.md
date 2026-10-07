# `list_jobs` dedicated descriptor measurement

## Proposal and scope

Expose the existing `list_jobs` operation directly at rank 79, immediately
before `observe_jobs`. The descriptor gives Hosts the canonical bounded input
schema and read-only, non-destructive, idempotent, closed-world annotations for
inventory recovery, instead of requiring a generic execution descriptor.

The definition retains `runtime:read`, caller-visible Job selection, filter and
limit semantics, and omission of stdout/stderr bodies. Ordinary direct-call
fallback remains available through `call_runtime_tool`. Known pending Jobs still
use their exact continuation; inventory is not a polling or waiting primitive.

## Measured discovery cost

Measured on Windows from main base
`0d65db122d8d841cc9a98e0a5b5d2fc11a6d70f3`, with default production features,
using the existing `mcp_tools_list_stateless_serialized_size_budget` fixture.
The compact model projection excludes App-only descriptors and Host-only
`_meta`/`title`, as defined by that fixture.

The dedicated compact descriptor is **1,780 serialized bytes**, adding one
model-visible tool and 1,781 bytes including its array separator. This cost
includes the 641-byte stateless invocation envelope. Full and compact projections
retain the same admitted inventory and all existing byte budgets pass.

| Candidate projection | Model tools | Compact model bytes |
| --- | ---: | ---: |
| Anonymous, Apps disabled | 30 | 71,708 |
| Anonymous, Apps enabled | 31 | 72,332 |
| Scoped, Apps disabled | 31 | 73,676 |
| Scoped, Apps enabled | 32 | 74,300 |
| Admin, Apps disabled | 37 | 83,623 |
| Admin, Apps enabled | 38 | 84,247 |

## Validation and limits

Focused in-process tests cover both legacy and stateless adapters:

- compact/full discovery preserves canonical business constraints and all four
  effect annotations;
- direct and gateway calls return the same inventory and invalid-filter errors;
- an owned Job is returned while another caller's Job is excluded;
- both routes require `runtime:read` through the HTTP OAuth adapter;
- canonical manifests prefer the direct route and retain the ordinary gateway
  fallback;
- unknown-Job recovery in the MCP App projection uses the advertised direct
  callable;
- existing Project/Session/status filtering, log-body omission, bounds, and
  no-Runner-capability checks continue to pass.

The directly demonstrated benefit is an independently registered observation
contract. This is not a model-selection or latency A/B experiment. No OpenAI
safety classifier was instrumented, and no previously rejected execution was
replayed. A Host rejection reported before WebCodex dispatch remains unexplained;
the proposal does not establish that a descriptor change resolves it.
