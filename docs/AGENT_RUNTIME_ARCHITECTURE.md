# Agent Runtime Architecture

[English](AGENT_RUNTIME_ARCHITECTURE.md) | [简体中文](AGENT_RUNTIME_ARCHITECTURE.zh-CN.md)

This document captures the long-term WebCodex runtime direction. It is a design anchor, not a release checklist and not a promise that every section is already implemented.

WebCodex started as a remote coding bridge for GPT Actions, MCP clients, and local agents. The current direction is broader: WebCodex should become a remote, auditable, policy-aware agent runtime for real machines, real projects, and future multi-agent workflows.

## Core thesis

WebCodex should not be treated as a bag of MCP tools. It should be treated as an agent runtime:

```text
LLM / online agent platform
  -> WebCodex Agent Operating Contract
  -> runtime tool IR
  -> policy and scope checks
  -> project-scoped execution
  -> session, artifact, and audit records
  -> final report
```

The near-term product remains practical: an online coding and operations agent that can connect GPT Actions, MCP clients, and future hosted AI clients to registered machines. The architecture should still be shaped so later coding harnesses, operations workflows, artifact generation, and multi-agent shared spaces fit naturally.

## Do not copy another agent's prompt

Codex, Claude, Gemini, and other agents may have their own built-in instructions. WebCodex can borrow the idea that agents need a stable operating protocol, but it should not copy another agent's prompt text.

WebCodex has its own runtime model:

- remote agents connected by WebSocket, polling, or QUIC;
- registered projects and canonical project ids;
- `allowed_roots` and project-scoped execution;
- OAuth2, PATs, account credentials, and agent tokens;
- MCP tools, GPT Action operations, and CLI/admin surfaces;
- session recorder, current-session bindings, message boards, and task guards;
- tool metadata, risk classes, OAuth scopes, and MCP annotations;
- workspace checkpoints, artifacts, jobs, and bounded logs.

A WebCodex-native instruction layer should describe this environment. It should be called something like **Agent Operating Contract** or **WebCodex Runtime Instruction**. It is not a replacement for `AGENTS.md`, project instructions, or a user's task prompt. It is the stable runtime contract injected by WebCodex so the model knows how to act safely through this runtime.

## Agent Operating Contract

The operating contract should teach the model the invariant workflow:

1. Identify the target project and resolve it to a canonical runtime project id.
2. Inspect before editing: runtime status, project status, relevant files, and current session state.
3. Prefer bounded read/search/diff tools before shell.
4. Prefer structured edit tools and patch validation before broad writes.
5. Keep mutations minimal and project-scoped.
6. Use shell/job tools only when necessary, bounded, and allowed by policy.
7. Preserve secrets and never print token values, full env files, or credentials.
8. Record or bind a session when the task spans multiple calls.
9. Use checkpoints, diffs, and validation before reporting completion.
10. On tool failure, narrow the request and inspect the reason instead of blind retries.

This contract should be stable across GPT Actions, MCP clients, CLI helpers, and future online clients. Platform-specific guidance can still be layered on top, but the core behavior should remain WebCodex-native.

## Tools as a runtime standard library

Runtime tools should be organized like a standard library, not as a flat function table.

Suggested conceptual layers:

```text
core:
  manifest, status, project identity, policy metadata

project:
  list projects, resolve project ids, list files, read files, search text

edit:
  line edits, exact block edits, text edit batches, patch validation, patch apply

git:
  status, diff, diff hunks, git log, restore/discard helpers, show_changes

session:
  start_session, current session binding, session messages, summary, guards

job:
  run_shell, run_job, job_status, job_log, job_tail, bounded async execution

artifact:
  save, inspect, chunked read, generated images, imported files, reports, zips

checkpoint:
  create, list, show, delete workspace checkpoints

admin:
  register/create projects, token and client management, server operations
```

Tool names may remain stable for compatibility, but documentation, metadata, recommended flows, and future `tool_manifest` output should reinforce these conceptual layers.

## Tool calls as execution IR

A model prompt is not an execution plan. WebCodex should treat tool calls as a structured intermediate representation:

```text
inspect -> locate -> read -> edit -> diff -> validate -> checkpoint -> report
```

The runtime can then reason about risk, policy, scope, ordering, and observability. This makes the system more like a compiler and runtime than a function router:

- user request: source-level intent;
- planner: semantic analysis and task decomposition;
- tool call sequence: execution IR;
- policy/scope checks: type system and borrow rules;
- tool metadata: standard-library signatures and risk annotations;
- session ledger: execution trace;
- show_changes and checkpoints: review and rollback support;
- validation tools: test and diagnostics passes;
- final response: build artifact/report.

This analogy is not meant to make WebCodex behave like Rust. It is a design discipline: explicit effects, scoped authority, bounded execution, and reviewable outputs.

## Safety model as a type system

WebCodex should increasingly make authority explicit:

```text
&Project       read-only project access
&mut Project   project write access
Job            async execution capability
Artifact       bounded generated/imported object
Checkpoint     restorable workspace snapshot
unsafe         shell, destructive, or admin-class operation
```

Current mechanisms already point in this direction:

- OAuth scopes and tool metadata;
- read-only session mode and task guards;
- destructive/consequential annotations;
- agent policy summaries and `allowed_roots`;
- project-scoped tool execution;
- redaction and bounded output handling.

Future policy work should refine this into clearer runtime states: read-only, writable, approval-required, shell-enabled, admin, and dangerous. The goal is not to block useful automation; the goal is to make the authority boundary visible before an agent crosses it.

## Runtime optimizer

WebCodex can improve agent reliability without changing the model by improving execution ergonomics:

- **Lazy context loading:** search first, then read only relevant file ranges.
- **Common subexpression elimination:** avoid repeatedly reading the same file or running the same status command.
- **Dead work elimination:** do not inspect unrelated files or run unrelated commands.
- **Memoization:** reuse file hashes, git status, search results, and project manifests when still valid.
- **Query planning:** choose read/search/diff/edit tools based on task risk and project size.
- **Backpressure:** use bounded logs, tails, pagination, and summaries instead of dumping full output.
- **Streaming:** prefer `job_status` and `job_tail` for long work rather than waiting for all output.
- **Checkpointing:** create reviewable recovery points around risky multi-step changes.

These are runtime and tool-design improvements. They are as important as adding new tools.

## Capability providers

The current `ToolKernel` and metadata foundation should eventually support provider-style capabilities. Providers are backend integrations that implement stable runtime capabilities.

Examples:

```text
LSP provider:
  code.diagnostics, code.references, code.rename, code.format

Tree-sitter provider:
  code.symbols, code.node_range, syntax-aware edit planning

Git provider:
  status, diff, log, restore/discard, change review

System provider:
  system.status, process listing, service status, port checks

Docker/systemd/nginx/cert providers:
  operations diagnostics and controlled remediation workflows

Artifact providers:
  generated images, PDFs, zips, imported files, reports

Message providers:
  future email, chat, webhook, or agent-to-agent notifications
```

The model-facing surface should avoid exposing backend implementation details when a higher-level capability is available. For example, prefer `code.diagnostics` over raw LSP JSON-RPC, and prefer `system.service_status` over arbitrary shell when the system provider can answer safely.

## Coding capability direction

WebCodex coding should become more reliable by improving the workspace rather than only relying on model strength.

Near-term capabilities:

- canonical project id resolution;
- project-scoped sessions with validation;
- compact tool manifests and recommended flows;
- file range reads with line numbers;
- atomic multi-block edits;
- workspace checkpoints;
- session-aware `show_changes`;
- bounded validation commands.

Next capabilities:

- code symbols and file outline;
- diagnostics after edits;
- reference and rename support;
- formatter integration;
- compile/test error summarization;
- edit transactions and rollback hints.

LSP and Tree-sitter should be treated as providers, not as the public protocol. The public protocol should remain stable capability names such as `code.symbols`, `code.diagnostics`, and `code.rename`.

## Operations product direction

WebCodex can become useful as an AI operations control plane before it becomes a full IDE backend. Operations tasks are often state inspection and bounded remediation:

- runtime status and agent inventory;
- process, port, disk, memory, and log inspection;
- service status and restart workflows;
- Nginx, certificate, Docker, and systemd diagnostics;
- deployment smoke tests;
- incident reports and artifact bundles.

This direction must be policy-first. Read-only diagnostics should be separate from mutating operations. Restart, delete, deploy, raw shell, and admin-class operations should have explicit scopes, risk metadata, and approval semantics.

## Multi-agent and open-world direction

A long-term extension of WebCodex is a shared agent runtime space:

```text
World/session = persistent collaboration context
Agent         = human, GPT, Claude, Gemini, Grok, local worker, or service bot
Capability    = scoped tool/provider access
Artifact      = object created or imported into the world
Event log     = durable history of actions and messages
Invite link   = controlled entry into a scoped world/session
Policy        = role, permission, approval, and isolation boundary
```

This can support game-like experiments, but the same abstraction also supports practical engineering workflows: builder/reviewer/operator agents, shared artifacts, deployment rooms, incident rooms, and long-running maintenance sessions.

The current session recorder, message board, artifacts, jobs, project identity, OAuth2, and tool metadata are early building blocks for this future. It should remain a long-term direction until the core runtime contract, policy model, and provider model are stable.

## Current development signal

Recent WebCodex work already points toward this architecture:

- OAuth2 and client authorization expand the platform surface beyond a single PAT workflow.
- Tool metadata and `ToolKernel` move tool execution toward a policy-aware runtime layer.
- Session ledgers, message boards, current-session bindings, and task guards create a harness-like execution trace.
- `show_changes`, git log, and checkpoints improve reviewability and recovery.
- `tool_manifest` makes runtime introspection more compact and ergonomic.
- `apply_text_edits` and line-edit tools reduce reliance on shell-based source rewriting.
- Artifact read/write tools prepare the runtime for generated media, imported files, reports, and future world objects.

The next step is to keep these features coherent under one architecture rather than letting them become unrelated utilities.

## Near-term priorities

1. Finish project identity ergonomics: resolver, validation, and clear ambiguity errors.
2. Keep OAuth2 and GPT Action/MCP setup documented as first-class entry points.
3. Strengthen `tool_manifest`, `ToolMetadata`, and recommended flows so models choose safer tools.
4. Expand session semantics carefully: persistence, message board, guards, and current-session rules must stay consistent.
5. Add policy-first operations capabilities before dangerous remediation tools.
6. Design provider abstractions before implementing LSP, Tree-sitter, systemd, Docker, or messaging integrations.
7. Treat design documents as architecture constraints, not marketing text.

## Non-goals

This document does not require immediate implementation of:

- a full LSP bridge;
- Tree-sitter indexing;
- a plugin marketplace;
- multi-agent open-world hosting;
- image generation or message sending;
- read-only shell policy redesign;
- a replacement for existing GPT Action or MCP compatibility.

The immediate goal is coherence: preserve compatibility while shaping the runtime so those future capabilities have a clear place to land.
