# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex lets ChatGPT and other MCP/GPT Action clients safely operate your private codebase through a self-hosted, auditable tool runtime.

It turns online AI coding from blind file edits into a permissioned, validated, reviewable engineering workflow.

- Give an online model bounded tools for reading, editing, validating, and reviewing a private repository.
- Keep code execution on your machine or trusted host through a connected WebCodex agent.
- Expose projects only after the agent registers allowed directories with the server.
- Capture session, validation, handoff, and finish evidence so changes can be reviewed.
- Use MCP or GPT Actions without changing the underlying WebCodex runtime.

```text
ChatGPT / Claude / Grok
        |
        | MCP / GPT Actions
        v
WebCodex Server
        |
        | authenticated agent bridge
        v
WebCodex Agent
        |
        v
Private Codebase / Git / Tests / Shell
```

## Why WebCodex?

Online models cannot directly access your local files, Git state, test runner, or shell. The usual workaround is to paste snippets into chat, expose ad hoc scripts, or hand a repository to a hosted coding agent.

WebCodex gives the model a narrower interface:

- The server exposes authenticated runtime tools, not raw filesystem access.
- The agent registers project directories from the machine that owns the code.
- Project ids are explicit, for example `agent:<client_id>:<project_id>`.
- Edits go through structured file and patch tools when possible.
- Validation tools and finish summaries create evidence before handoff.
- Review tools show the diff and workspace state before the user accepts work.

## Quick Start

Start with the local-first path in [docs/QUICK_START.md](docs/QUICK_START.md). It walks through one server, one agent, one registered project, and either an MCP or GPT Action client.

The fastest evaluation path uses one shared key for server runtime calls, agent connect, and MCP/GPT Actions. Production auth uses scoped tokens or OAuth later; see [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md) and [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

The first success criteria are simple: `runtime_status` works, `list_projects` shows an `agent:<client_id>:<project_id>` project, the client can read `README.md`, and a small edit can be reviewed and reverted.

For a walkthrough of the expected tool flow, see [docs/DEMO.md](docs/DEMO.md).

## Choose A Client

- Use MCP if your client supports remote MCP.
- Use GPT Actions if you are building a Custom GPT.
- Both surfaces call the same WebCodex ToolRuntime.

MCP clients connect to:

```text
https://your-domain.example/mcp
```

GPT Actions import:

```text
https://your-domain.example/openapi.json
```

## Default Coding Loop

WebCodex is designed around a conservative coding loop:

1. `start_coding_task` - create an explicit session and collect bounded startup context.
2. Inspect - use `list_project_files`, `search_project_text`, `read_file`, and Git review tools.
3. Edit - prefer `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`, or `apply_patch_checked`.
4. Validate - run `validate_patch`, `cargo_fmt`, `cargo_check`, or `cargo_test` where appropriate.
5. Review - use `show_changes`, `git_diff_hunks`, and `workspace_hygiene_check`.
6. Finish - use `finish_coding_task` or `session_handoff_summary` for a compact closeout.

`run_shell` and `run_job` exist for bounded escape hatches. They are powerful and should not be the default editing or validation path.

## Safety Model

WebCodex does not make an online model a trusted local user. The model can only call exposed tools, those tools run through server policy and agent project boundaries, and shell/job tools remain explicit high-risk operations.

Projects live on the agent machine. The agent registers allowed directories with the server. The server does not scan your filesystem.

Do not expose secrets in prompts, examples, tool output, docs, or committed config files. For the full boundary model, read [SECURITY.md](SECURITY.md) and [docs/CONCEPTS.md](docs/CONCEPTS.md).

## What 0.2.0 Includes

- Remote MCP endpoint and GPT Actions OpenAPI surface backed by one ToolRuntime.
- Agent-registered project model with `agent:<client_id>:<project_id>` ids.
- Structured source editing tools for scoped changes.
- Patch validation, Cargo validation helpers, Git diff/status tools, and bounded shell/job execution.
- Coding-task sessions with handoff, finish verdicts, review evidence, and hygiene summaries.
- Authentication paths for quick shared-key evaluation and production deployments.
- Documentation for first setup, concepts, MCP, GPT Actions, security, release notes, and roadmap.

## Known Limitations

- WebCodex is self-hosted infrastructure, not a hosted SaaS.
- Setup is still technical and assumes comfort with a terminal, server URL, and agent process.
- Semantic code intelligence, LSP diagnostics, and symbol navigation are not first-class in 0.2.0.
- The UI/dashboard is minimal; MCP, GPT Actions, and CLI workflows are the primary paths.
- Shell and job tools require operator trust, bounded configuration, and review discipline.

## Docs Map

- First setup: [docs/QUICK_START.md](docs/QUICK_START.md)
- Demo workflow: [docs/DEMO.md](docs/DEMO.md)
- Concepts: [docs/CONCEPTS.md](docs/CONCEPTS.md)
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- MCP: [docs/MCP.md](docs/MCP.md)
- GPT Actions: [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md)
- Security: [SECURITY.md](SECURITY.md)
- Auth model: [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md)
- Deployment: [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)
- Release notes: [docs/RELEASE_NOTES_v0.2.0.md](docs/RELEASE_NOTES_v0.2.0.md)
- Roadmap: [docs/ROADMAP.md](docs/ROADMAP.md)
- Full index: [docs/INDEX.md](docs/INDEX.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
