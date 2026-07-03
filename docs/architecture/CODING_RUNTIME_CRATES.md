# Coding Runtime Crate Strategy

This document records the product and architecture decision for the next phase
of WebCodex coding-runtime work. It is a direction-setting architecture note, not
a release checklist and not a stable public API promise.

## Decision

WebCodex server continues to be a remote coding runtime product.

The long-term implementation direction is to gradually extract reusable
coding-runtime crates from the server codebase. Those crates should be usable by
WebCodex server, a local CLI, a local agent, and third-party products that need
the same workspace, session, policy, command, and edit runtime behavior.

This is not a decision to make WebCodex a larger remote operations platform by
default. The near-term product work should instead tighten a Codex-quality
remote coding loop: inspect, edit, validate, review, and hand off with clear
state.

## Product Position

WebCodex should remain centered on remote coding execution:

```text
client or model surface
  -> WebCodex server
  -> coding runtime
  -> project workspace
  -> structured edits, bounded commands, sessions, diffs, and handoff
```

The server owns remote access, identity, authorization, transport adapters,
project registration, and operational policy. The coding runtime underneath it
should become more modular and reusable over time, but the product identity does
not change from server-backed remote coding.

### Non-goals

- Do not turn WebCodex into a local agent product.
- Do not turn WebCodex into a LangGraph, AutoGen, or generic agent workflow
  framework.
- Do not prioritize adding more remote capabilities before the coding loop is
  consistently high quality.
- Do not publish or promise a stable crate API too early.

## Near-term Remote Coding Loop

The primary path should optimize the coding loop that models and agents repeat
for most development tasks:

```text
resolve project
load rules/context
search/read files
structured edit
run bounded validation
review diff
summarize/handoff
```

The default inspect tools should be:

```text
read_file
search_project_text
show_changes
```

The default edit path should be:

```text
replace_line_range
insert_at_line
delete_line_range
apply_text_edits
apply_patch_checked
```

The following tools remain available, but should not be the default
recommendation for ordinary coding tasks:

```text
raw run_shell
write_project_file
pattern edit
generic callRuntimeTool fallback
```

These weaker paths are still useful for escape hatches, compatibility, and
advanced clients. The design goal is to make the best path obvious and
repeatable, not to remove existing capabilities prematurely.

## Stage 1: Better Inspect/Edit Loop

Stage 1 improves the existing loop without introducing a new workflow engine.

- Make `search_project_text` prefer `rg` when available.
- Provide deterministic fallbacks for search, such as `grep` or a native file
  walker, when `rg` is unavailable.
- Return structured search output with `matches`, `context`, `backend`, and
  `truncated` fields so clients can reason about result quality and retry
  narrowly.
- Fix tool metadata and recommended flows around the inspect, edit, validate,
  and review path.
- Correct documentation drift around session persistence and current-session
  in-memory binding behavior.
- Reduce the default recommendation weight of raw shell, whole-file write,
  pattern edit, and generic `callRuntimeTool` fallback for ordinary coding
  tasks.

Stage 1 should make the common path dependable before adding larger abstractions.

## Stage 2: Deterministic Workflow Tools

Stage 2 may add small deterministic workflow tools:

```text
start_coding_task
finish_coding_task
```

These tools are not an agent framework and must not perform LLM summarization.
They must not hide the underlying runtime state. They should only aggregate
existing runtime, session, workspace, Git, and hygiene information into
structured JSON.

`start_coding_task` should aggregate:

```text
start_session
project resolve
runtime_status
project rules summary
git status
recent commits
initial warnings
recommended flow
```

`finish_coding_task` should aggregate:

```text
show_changes
validation summary
session summary
handoff summary
workspace hygiene
dirty-state warnings
```

The purpose is to reduce boilerplate and drift between model-facing clients. The
tools should still expose enough fields for a client to inspect exactly what the
runtime observed.

## Stage 3: Eval Before Advanced Language Intelligence

Stage 3 should add an evaluation harness before investing in advanced language
intelligence. The runtime should first measure whether the coding loop is
actually improving.

The eval harness should track:

```text
task success
tool calls
raw shell ratio
failed call recovery
edit precision
validation coverage
dirty worktree cleanup
handoff completeness
```

Only after this baseline exists should WebCodex add language-aware helpers such
as:

```text
test failure parser
diagnostics parser
```

The final step is richer code intelligence:

```text
tree-sitter outline
LSP definition/reference/rename
```

This order keeps the product honest. WebCodex should prove that the basic remote
coding loop works before introducing higher-level code understanding features
that are harder to validate and easier to overfit to demos.

## Long-term Reusable Crate Direction

The long-term decomposition target is a set of reusable crates. These names are
directional and should not be treated as stable package names yet.

### `webcodex-types`

Shared primitive types, identifiers, structured error shapes, tool envelopes,
session ids, project ids, workspace paths, validation summaries, and DTOs that
are safe to share across adapters.

This crate should avoid depending on server, HTTP, OAuth, MCP, or OpenAPI
implementation details.

### `webcodex-policy`

Policy evaluation for risk classes, scope checks, allowed roots, path
constraints, read-only sessions, shell permissions, write guards, and
consequential action classification.

This crate should express policy decisions independently from transport.

### `webcodex-workspace`

Project resolution, workspace root handling, safe path normalization, file
reading, text search, structured line edits, checked patch application, and
workspace hygiene inspection.

This crate should provide the local workspace model that both remote and local
coding runtimes can use.

### `webcodex-vcs`

Git and future VCS operations: status, diff, recent commits, checkpoint support,
restore/discard operations, and dirty-state warnings.

This crate should expose deterministic VCS primitives without knowing whether
the caller is WebCodex server, a CLI, or an adapter.

### `webcodex-command`

Bounded command execution, process limits, timeout handling, output capture,
redaction hooks, shell profile selection, and validation command summaries.

This crate should be the reusable command runner layer, not a general remote
shell product.

### `webcodex-session`

Session records, current-session binding abstractions, event recording, task
state, handoff state, and session summaries.

This crate should separate durable session semantics from any one in-memory or
database-backed storage implementation.

### `webcodex-runtime`

The coding runtime composition layer. It combines workspace, VCS, command,
session, and policy behavior into the tools and workflows used by WebCodex.

This crate owns runtime orchestration, tool dispatch, recommended coding flows,
and deterministic workflow-tool aggregation when those tools exist.

### `webcodex-agent-protocol`

Remote DTOs and protocol-level contracts for agent communication. This may
include request and response shapes, capability advertisements, runtime status
payloads, and transport-neutral message types.

This crate must not contain server business logic.

### `webcodex-server`

The WebCodex server application: HTTP routing, OAuth and account integration,
project registration, transport lifecycle, persistence wiring, server
configuration, and adapter mounting.

This crate depends on the runtime and adapter crates. It should not be the home
for reusable coding-runtime primitives.

### `webcodex-mcp`

MCP adapter logic, schema exposure, annotations, tool listing, transport
envelopes, and MCP-specific compatibility behavior.

This crate adapts the runtime to MCP. It should not own core policy, workspace,
or command behavior.

### `webcodex-openapi`

OpenAPI and GPT Action adapter logic, including schema generation, flattened
Action fields, action metadata, consequential flags, and examples.

This crate adapts the runtime to OpenAPI and GPT Actions. It should not own
runtime business logic.

## Dependency Direction

The target dependency direction should stay strict:

```text
workspace/vcs/command/session/policy do not depend on HTTP/MCP/OpenAPI/OAuth/server
runtime combines workspace/vcs/command/session/policy
server depends on runtime
mcp/openapi are adapters
agent-protocol contains remote DTOs/protocol, not server business logic
```

This direction is more important than the exact crate names. If the dependency
direction is wrong, physical crate splits will only make the architecture harder
to maintain.

## Do Not Split Physical Crates Yet

WebCodex should not immediately split the repository into physical Cargo crates.

Reasons:

- The repository is still a single crate today.
- A premature Cargo workspace migration would add mechanical cost before the
  internal ownership lines are stable.
- Runtime APIs are not stable enough to publish or support externally.
- Publishing crates too early would freeze design mistakes and make later
  refactors more expensive.
- Internal module boundaries, traits, and dependency direction can stabilize the
  seams first.

The near-term target is internal structure, not package publication.

Recommended internal seams:

```text
WorkspaceBackend
VcsBackend
CommandRunner
SessionStore
Policy
CodingRuntime
```

These seams should be introduced only where they reduce coupling or make current
runtime behavior easier to test and reuse. They should not become abstract
interfaces for their own sake.

## Validation and Completion Criteria

For this documentation-only change, validation should be:

```bash
cargo fmt --check
cargo check --all-targets
git diff --check
git status --short --branch
```

If the project later adds a Markdown lint command, documentation-only changes
can run it as well. This note does not introduce a new Markdown lint dependency.
