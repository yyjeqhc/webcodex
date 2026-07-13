# Concepts

[English](CONCEPTS.md) | [简体中文](CONCEPTS.zh-CN.md)

WebCodex lets an online AI client operate a private repository through a self-hosted, auditable tool runtime. This page defines the terms used across the setup, MCP, GPT Actions, security, and architecture docs.

## Mental Model

```text
Online model / client
        |
        | MCP / GPT Actions / REST tool calls
        v
WebCodex server
        |
        | authenticated agent bridge
        v
WebCodex agent
        |
        v
Agent-registered project
```

Projects live on the agent machine. The agent registers allowed directories with the server. The server does not scan your filesystem.

## Core Terms

### Online Model / Client

The online model is ChatGPT, Claude, Grok, or another hosted model. The client is the host surface that sends tool calls to WebCodex, such as remote MCP, GPT Actions, or a REST integration.

The model does not receive direct filesystem access. It can only call the tools exposed by the configured client and authorized by WebCodex.

### WebCodex Server

`webcodex` is the self-hosted server. It exposes the MCP endpoint, the GPT Actions OpenAPI schema, and runtime REST APIs. It authenticates callers, applies tool policy, records bounded session evidence, and routes project work to connected agents.

The server is the stable online entry point. It should be deployed behind HTTPS before connecting hosted clients.

### WebCodex Agent

`webcodex-agent` runs on the machine that has the code. It connects back to the server, registers allowed projects, and executes file, Git, patch, validation, shell, job, artifact, and checkpoint requests inside those project boundaries.

The agent is the trust boundary closest to your repository. Configure it with narrow allowed roots and shell profiles appropriate for the projects it serves.

### Agent-Registered Project

An agent-registered project is a directory the agent has made available to the server. The server does not invent or discover project paths on its own.

Runtime project ids use this shape:

```text
agent:<client_id>:<project_id>
```

`client_id` identifies the agent connection profile. `project_id` is the project id registered by that agent. Put the full runtime project id in prompts and tool calls so the model does not choose the wrong repository.

### Tool Runtime

The ToolRuntime is the protocol-independent execution layer. MCP, GPT Actions, and REST wrappers translate client requests into the same runtime tool calls.

Common tool groups:

- Discovery: `runtime_status`, `list_projects`, `list_agents`, `tool_manifest`.
- Inspect: `list_project_files`, `search_project_text`, `read_file`, `git_status`, `git_diff_hunks`.
- Edit: `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`, `apply_patch_checked`.
- Validate: `validate_patch`, `cargo_fmt`, `cargo_check`, `cargo_test`.
- Review: `show_changes`, `workspace_hygiene_check`.
- Finish: `finish_coding_task`, `session_handoff_summary`.
- Escape hatch: `run_shell`, `run_job`.

### MCP

MCP clients connect to:

```text
https://your-domain.example/mcp
```

Use MCP if your client supports remote MCP. MCP exposes WebCodex runtime tools through MCP framing while keeping the same server, agent, project id, and safety boundaries used by GPT Actions.

### GPT Actions

GPT Actions import the WebCodex OpenAPI schema:

```text
https://your-domain.example/openapi.json
```

Use GPT Actions if you are building a Custom GPT. GPT Actions exposes a focused REST operation surface and a generic `callRuntimeTool` path for runtime tools. It shares the same WebCodex ToolRuntime as MCP.

### Session

A session is a bounded task record. `start_coding_task` creates the recommended coding session and returns an explicit `session_id`. Keep that id and pass it to later review, validation, handoff, or finish calls when the tool accepts it.

Sessions are task-continuity evidence, not a full surveillance log. They record bounded, redacted facts such as tool names, status, project id, validation summaries, permission decisions, and closeout state.

### Handoff / Finish

`finish_coding_task` is the normal closeout tool. It can include review evidence, workspace hygiene, validation summary, job state, warnings, and canonical task/evidence outcomes.

`session_handoff_summary` is the read-only handoff tool. Use it when another operator, client, or later session needs to continue from the current state.

### Validation

Validation is evidence that the change was checked. WebCodex provides structured helpers such as `validate_patch`, `cargo_fmt`, `cargo_check`, and `cargo_test`.

Choose validation that fits the change. A docs-only edit may need `git diff --check` outside WebCodex or a focused review; a Rust behavior change should run Cargo checks or tests.

### Review / Hygiene

Review tools show what changed before the user accepts it. Use `show_changes` for file lists, status, diff stats, and optional bounded hunks. Use `workspace_hygiene_check` to detect untracked smoke files, temporary files, blocking jobs, and other closeout risks.

### `run_shell` As Escape Hatch

`run_shell` can run bounded project commands through the agent. It is useful for project-specific checks that do not have a structured helper yet.

It is not the default editing path, not the first validation choice, and not a way to bypass project policy. Treat shell/job tools as powerful operations that require trusted configuration and human review.

## Default Coding Loop

1. `start_coding_task`
2. Inspect with `list_project_files`, `search_project_text`, and `read_file`.
3. Edit with structured edit or patch tools.
4. Validate with structured validation tools.
5. Review with `show_changes`, `git_diff_hunks`, and `workspace_hygiene_check`.
6. Finish with `finish_coding_task` or hand off with `session_handoff_summary`.

## Where To Go Next

- First setup: [QUICK_START.md](QUICK_START.md)
- Demo workflow: [DEMO.md](DEMO.md)
- Architecture: [ARCHITECTURE.md](ARCHITECTURE.md)
- MCP: [MCP.md](MCP.md)
- GPT Actions: [GPT_ACTIONS.md](GPT_ACTIONS.md)
- Security: [../SECURITY.md](../SECURITY.md)
