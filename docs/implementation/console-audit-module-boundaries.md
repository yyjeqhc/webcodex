# Console and audit module boundaries

Maintenance group 3 combines large-file organization with the tests that protect
those same responsibilities. It is independent of the Runner environment/CI
and on-demand discovery PRs, based on
`b967a1556ac58b50e32c8399196979fdbae1713c` (#774).

## Separate moves from production organization

The first commit relocates the Console tests and four audit regression suites.
Console tests are grouped under `runtime_console_http/tests/` by overview,
projects, Sessions, Window activity/visibility, Goal, communication and
collaboration. Shared fixtures remain in `tests/mod.rs`; the existing typed Job
query tests stay in the same tree. The move preserves every original Console
function body byte-for-byte before formatting, including raw string contents.
Audit suites preserve their existing enclosing module names and assertions.

Later commits isolate the production concerns and apply final Rust formatting.
No test assertion is weakened, no test is ignored or deleted to improve counts,
and no broad new test framework is introduced. Full Console test names acquire
a domain component; existing `runtime_console_http` filters still select them.
Compiler-produced test inventories are compared by unique leaf test identity,
not source line counts. A file move alone is not claimed to isolate processes,
improve runtime performance or reduce Cargo compilation.

## Window queries

`runtime_console_http/window_queries.rs` holds caller-authorized Window query
assembly. Routes, DTOs and common permission/error helpers remain at the Console
adapter boundary. The query module still uses the same exact principal/project
visibility checks, bounded scans, passive activity observations, job projection
and partial-result semantics. It does not choose a Workflow Session, gain new
authority or create a second Window state model.

The large entry file drops from 7,578 to 2,482 lines, including the removal of its
inline test block. The Window query module is 917 lines; shared test fixtures are
637 lines, and the new behavior groups range from 90 to 851 lines. These counts
are an organization aid, not a performance or quality metric by themselves.

## Audit projections

The original `tool_audit.rs` keeps request-policy admission and the exhaustive
`ToolCallAuditProjection` match. It delegates to three private modules:

| Module | Responsibility |
|---|---|
| `request.rs` | Explicit request-field allowlists for the relevant tool domains |
| `result.rs` | Bounded result-field allowlists and canonical execution evidence |
| `execution_identity.rs` | Existing normalized validation-execution identity calculations |

The public parent paths are preserved through explicit re-exports, including
`GenericValidationIdentity` and the existing core validation-identity helpers.
No accepted ToolCall input, authorization, data retention, result projection,
privacy allowlist or hash algorithm changes. There is no reflection-based
"serialize everything and delete sensitive fields" fallback. Unknown inputs
continue to fail closed through the original admission path.

The audit entry file drops from 6,463 to 2,355 lines. Its remaining large match is
intentional: it remains exhaustive and reviewable when a tool variant is added.
Request/result/identity modules are 1,100 / 516 / 264 lines respectively. A new
crate, generated registry or generic policy engine would add concepts without
improving this local boundary, so none is introduced.

## Validation and scope

Focused validation covers the full audit-contract package with all features,
all default Console tests and root tests selected by `audit`. These boundaries
exercise preserved authorization, result privacy, typed query projection and
adapter behavior. The full root suite is not rerun solely for these moves;
it is covered separately by CI. Test inventory, dependency boundaries, Rust
formatting and whitespace are checked after source stabilization.

Final local evidence: audit-contract package with all features **56 passed**;
root `runtime_console_http` selection **54 passed**; root `audit` selection
**70 passed**. These selections overlap and must not be added as a unique-test
total. The 50 tests under the old Console `tests` module match all 50 compiled
leaf identities after regrouping; four other Console-module tests account for
the 54-test filter result. Initial test-module name shadowing was corrected by
explicit production-module imports, without changing assertions. Formatting,
whitespace, the 21-package dependency boundary and both inventory self-tests pass.

No frontend source/bundle, tool schema, route name, App resource, configuration,
production data or deployment is changed. Rust module visibility is an internal
encapsulation choice, not runtime user authority. Follow-up changes should keep
query filters, permission checks and audit allowlists with their existing owner
rather than copying them into each smaller file.
