# On-demand tool discovery contracts

## Scope

Maintenance group 2, based on `b967a1556ac58b50e32c8399196979fdbae1713c` (#774).
This change removes repeated full ToolSpec construction from lightweight discovery
and from filtered model-facing descriptions. It does not add a cache, mutable
registry, tool, authority, configuration setting or protocol revision.

## One metadata source, explicit materialization

`ToolDescriptor` is a schema-free name/description projection from the same
canonical definitions and fixed operator-extension descriptions that already
produce public ToolSpecs. It does not admit tools or grant authority. Existing
full-schema callers still request `into_spec()` and receive the same ToolSpec
shape, descriptions, annotations and input/output contracts.

The surface keeps the existing admission and ordering rules: model-visible
entries, separately capability-admitted fixed extensions, and exact-only
specialists. Unknown or unadmitted names fail before constructing a contract.
Category, intent, count, risk and route calculations use descriptors. They never
reconstruct full input/output schema trees merely to read metadata.

| Request | Materialization at this boundary |
|---|---|
| Exact ordinary tool manifest | One requested input schema, no output schemas |
| Category/intent/broad compact manifest | No input/output schemas |
| Filtered model-facing description projection | No input/output schemas |
| Summary-only list_tools | No input/output schemas |
| Full list_tools with a limit/filter | Full specs only for selected returned tools |
| Explicit experimental Code Mode callable contract | Full contracts required by that optional nested surface |

The canonical input-schema implementation already has a process-local OnceLock
cache derived from the full ToolCall enum. This patch does not change that cache
or claim that the first cold request derives only one enum variant. The counts
above concern requested materialization/cloning and output-schema construction,
not hidden work inside canonical schema generation.

A test-thread-local counter protects these boundaries without production metrics
or timing thresholds. Existing schema, extension, authority and route tests
continue to check actual behavior and shape. The full descriptor/spec parity test
covers canonical and operator/specialist factories in their original order.

## Expensive policy matrix

The original test checks two Apps states, two Goal preferences and three Host
resume declarations. All twelve combinations and their six exact-manifest
checks remain. Its static full-schema builder takes no Runtime/policy argument;
reconstructing the same baseline inside every combination was redundant. The
baseline is now checked once before and once after the matrix. Runtime-aware
adapter assertions remain inside every combination.

Before this change, on special in the ordinary unoptimized test profile, this
single test took 54.79 seconds (compilation separately took 2m16s). After the
change, the same twelve-case test took 11.19 seconds (warm compilation 0.30s).
All policy combinations and six exact manifests per combination still ran.
These are local single observations, not production latency percentiles or
Host-token/throughput claims. The default full Server library also passed:
3,105 tests passed, three ignored, zero failures; its observed test time was
57.50 seconds. No before/after claim is made for the entire library suite.

## Validation strategy

The registry is used by many tools, so final validation includes the default
Server library suite rather than only the new counter test, plus the contract
crate with all features and the optional Code Mode manifest path. The new counter
test verifies exact/category/summary/full-list requests and refusal to expose
Memory without its capability. No published schema or card resource changes are
intended. No runtime restart or deployment is needed to measure the source tests.

The broad all-feature `tool_manifest` selection passed 35 tests. An additional
exact experimental Code Mode stage test exposed the existing 16 KiB guarded-edit
projection ceiling: read-only and validation stages succeeded, guarded-edit
reported `code_mode_projection_too_large`. A disposable immutable baseline
comparison is used to distinguish a pre-existing optional limitation from a
regression; no limit or constraint is loosened merely to pass the test.

This change is independent of the environment-isolation and large-module/test
organization PRs. Shared full ToolSpec construction remains available at real
protocol boundaries; optimizing discovery must not weaken authorization or
replace a full contract with guessed metadata.
