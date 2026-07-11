# Architecture

WebCodex is a self-hosted tool runtime that lets online AI clients operate private code through a server and a local execution agent. This document starts with the product architecture, then maps that architecture to the main Rust modules.

For vocabulary, read [CONCEPTS.md](CONCEPTS.md). For setup, read [QUICK_START.md](QUICK_START.md).

## 1. Client / Server / Agent / Codebase

```mermaid
flowchart LR
  C[ChatGPT / Claude / Grok] -->|MCP or GPT Actions| S[WebCodex Server]
  S -->|authenticated agent bridge| A[WebCodex Agent]
  A --> P[Registered Project]
  A --> G[Git / Tests / Shell]
```

The online client calls WebCodex over MCP or GPT Actions. The server authenticates the caller and dispatches runtime tool calls. The agent owns the local project boundary and performs approved file, Git, validation, shell, and job work.

## 2. Security Boundary

```mermaid
flowchart TD
  M[Online model] -->|tool calls only| S[WebCodex Server]
  S -->|policy + auth + session ledger| A[Agent]
  A -->|allowed project dirs only| R[Private repo]
  M -. no direct filesystem access .- R
```

The model sees tool results, not arbitrary local files. Projects are registered by agents. The server does not scan the filesystem. Shell and job tools are bounded but powerful, so deployments should keep agent roots narrow and credentials scoped.

## 3. Runtime Module Map

```mermaid
flowchart TD
  HTTP[HTTP / MCP / OpenAPI] --> TR[ToolRuntime]
  TR --> TD[ToolDefinition registry]
  TR --> PR[Project resolution]
  TR --> AB[Agent bridge]
  TR --> FE[File/Edit/Git/Validation/Job tools]
  TR --> SH[Session / Handoff / Hygiene]
```

The protocol adapters translate incoming requests into runtime tool calls. The ToolRuntime applies shared dispatch, project resolution, session recording, and domain tool behavior before routing agent-backed work to the agent bridge.

## Runtime Surfaces

- `runtime_http` exposes REST runtime routes, including generic runtime tool calls and dedicated project/file wrappers.
- `mcp` exposes the remote MCP endpoint backed by the same ToolRuntime.
- `openapi` builds the GPT Actions schema for the focused public operation surface.
- `tool_runtime` owns protocol-independent tool parsing, dispatch, project resolution, registry metadata, sessions, handoff, hygiene, files, Git, patches, Cargo validation, shell, jobs, artifacts, and checkpoints.

## Agent Bridge

- `shell_client` is the server-side agent registry and transport bridge. It tracks connected agents, project registrations, request/response flow, job updates, and agent policy summaries.
- `src/bin/webcodex_agent/*` owns the agent binary behavior: config loading, transport fallback, project registry parsing, file/patch/artifact/checkpoint handling, shell execution, and response shaping.
- `src/bin/webcodex_agent/lsp/*` owns the Rust-only LSP process supervisor and read-only navigation handlers. Results use project-relative paths, 1-based Unicode scalar columns, bounded truncation, and omit external (registry/sysroot) locations.
- `tool_runtime::semantic_navigation` builds the always-present compact `start_coding_task.semantic_navigation` capability summary. It sends only typed `AgentLspRequest::Status` under one two-second deadline and parses the versioned result contract directly, without recursively dispatching the public `lsp_status` ToolCall or recording a nested session event. Agent status resolution may inspect Cargo workspace presence, executable availability, and an existing supervisor slot, but it never starts rust-analyzer, runs Cargo or shell commands, or retrieves symbol/location data. The summary is Rust-only, read-only, workspace-only, dependency-limited by `cargo.noDeps=true`, and marked `full_text_sync_only`: validated workspace `.rs` files refresh open LSP documents from current disk content, without editor-style incremental synchronization. Probe failure or unavailability remains optional acceleration metadata and does not affect the coding startup verdict or warnings.

The agent is where private repository paths are interpreted. The server routes by runtime project id, such as `agent:<client_id>:<project_id>`.

### Agent-Side LSP Architecture

```mermaid
flowchart LR
  S[WebCodex Server] --> TR[ToolRuntime]
  TR --> AB[Agent Bridge]
  AB --> LS[Agent LspSupervisor]
  LS --> RA[rust-analyzer]
```

The LSP process runs only on the agent, at the canonical root of a registered project. The server never reads project files directly and does not spawn a shell to run LSP work. Typed bridge requests preserve the project boundary and do not permit arbitrary LSP-method passthrough. The rust-analyzer profile is constrained: `cargo.noDeps=true`, build scripts and proc macros are disabled, and `checkOnSave` is disabled. Public read-only intelligence tools (`lsp_status`, `document_symbols`, `goto_definition`, `find_references`, `document_diagnostics`, `hover`, and `workspace_symbols`) follow the path shown above. Diagnostics are received only through an instance-local bounded `textDocument/publishDiagnostics` cache; freshness waiting uses one two-second deadline and does not invoke Cargo check. Hover content is normalized and bounded; workspace symbols are filtered to canonical project files and returned with project-relative paths.

## Auth, Policy, And Audit

- `auth` owns bearer authentication, principal modeling, scope constants, route gates, shared-key helpers, PAT verification, and OAuth token verification.
- `oauth_http` owns OAuth HTTP endpoints, consent, token exchange, revocation, metadata, and shared-key bridge UI.
- `db` owns persistence for users, tokens, agents, audit entries, OAuth rows, and schema migrations.
- Session and audit evidence is bounded and redacted. It is designed for task review and handoff, not for storing raw secrets, command bodies, or complete file contents.

## CLI And Operations

- `src/bin/webcodex_cli/*` owns setup and operations commands such as server bootstrap, connect, pairing, token creation, doctor checks, service installation, and profile handling.
- Deployment docs should use the CLI for management tasks rather than exposing management endpoints to GPT Actions or MCP.

## Frontend

The current product entry points are MCP, GPT Actions, REST, and CLI. Any frontend should remain an operator aid and should not become the model-facing trust boundary unless it uses the same runtime, auth, and session rules.

## Invariants For New Runtime Tools

When adding or renaming a runtime tool, keep these synchronized in the same change:

- `ToolCall` parsing and known tool names.
- Tool metadata and registry schema.
- OAuth scope policy.
- MCP `tools/list`.
- GPT Actions accepted names, examples, and flattened fields when applicable.
- Consistency tests.

Default to exposing new specialized behavior through the generic runtime tool path unless there is a clear product reason and GPT Actions operation-count budget for a dedicated operation.

### ToolDefinition Dead-Code Hygiene

`src/tool_runtime/tool_definition.rs` must not use a module-wide `#![allow(dead_code)]`.
During the ToolDefinition migration, unused residue should be removed when
possible. Test-only helpers should be placed behind `#[cfg(test)]`, and any
remaining temporary compatibility allowance must be item-scoped rather than
module-wide.

Schema migration tests enforce this documentation so the tool surface does not
quietly accumulate broad dead-code allowances while ToolDefinition, ToolCall,
MCP, OpenAPI, and metadata stay synchronized.
