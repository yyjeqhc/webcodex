# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

Connect ChatGPT to private code that stays on your machine.

WebCodex is a self-hosted coding bridge for MCP and GPT Actions. It lets an online model inspect a real repo, make scoped edits, run validation, and hand back a compact task summary without giving the model raw filesystem access or moving your repository into a hosted coding service.

- Work on real local or private-hosted repositories from ChatGPT, Custom GPTs, or other MCP-capable clients.
- Register only the project directories you choose through a long-running WebCodex agent.
- Prefer structured read, edit, diff, and validation tools before falling back to shell commands.
- Keep execution on the agent machine while the server handles auth, policy, sessions, and client protocols.
- Finish tasks with changed files, validation results, workspace hygiene, and handoff evidence for your normal Git review flow.

```text
ChatGPT / MCP clients / Custom GPTs
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

## Install

For the current Linux x64 release:

```bash
npm install -g @yyjeqhc/webcodex
```

Or build from source:

```bash
cargo build --release --bins
```

See [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) for platform support and install details.

## Quick Start

The commands below assume the npm-installed binaries are on your `PATH`.

Terminal 1 - choose one evaluation key and start the server:

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"

webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080

set -a
. "$WEBCODEX_ENV"
set +a
webcodex
```

Terminal 2 - connect an agent from the repo you want WebCodex to operate:

```bash
export WEBCODEX_KEY="<same evaluation key>"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite

webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

Use the same `WEBCODEX_KEY` as the Bearer key in your MCP client or GPT Action. For the full walkthrough, see [docs/QUICK_START.md](docs/QUICK_START.md).

## Client Access

- ChatGPT-hosted clients, including GPT Actions and ChatGPT remote MCP, require a public HTTPS URL with a valid certificate. Put WebCodex behind Nginx, Caddy, or a tunnel, start the server with `--public-url https://your-domain.example`, then use `https://your-domain.example/openapi.json` or `https://your-domain.example/mcp`.
- Local or self-hosted clients that can reach the server directly can use `http://127.0.0.1:8080` or a private network URL without public HTTPS.
- Claude or other MCP-capable clients use the `/mcp` endpoint. The first evaluation path uses the shared Bearer key; production deployments should move to scoped user tokens or OAuth when the client supports it.

## What ChatGPT Can Do

Once a project is registered, an MCP or GPT Actions client can:

- read files, inspect directories, and search the repo;
- make scoped source edits through structured edit or patch tools;
- run focused validation with built-in helpers or bounded shell/job tools;
- inspect changed files, diff hunks, and workspace hygiene;
- finish with a compact task summary that is easy to paste into an issue, PR, or handoff note.

## Why WebCodex?

Online models cannot directly access your local files, Git state, test runner, or shell. The usual workaround is to paste snippets into chat, expose ad hoc scripts, or hand a repository to a hosted coding agent.

WebCodex gives the model a useful but narrower interface:

- The server exposes authenticated runtime tools, not raw filesystem access.
- The agent registers project directories from the machine that owns the code.
- Project ids are explicit, for example `agent:<client_id>:<project_id>`.
- Edits go through structured file and patch tools when possible.
- Validation tools and finish summaries create evidence before handoff.
- Diff and hygiene tools show what changed before you accept, revert, or continue the work.

## Where It Fits

Use WebCodex when you need a server/agent boundary, explicit project registration, MCP and GPT Actions support, deployable auth paths, session records, validation evidence, and a path from quick local testing to a private hosted runtime.

It is still not a hosted SaaS or a full browser IDE. The current workflow is tool-first: ChatGPT calls WebCodex tools, the agent operates on the repo, and you review the resulting diff and validation evidence in your normal development process.

The first success criteria are simple: `runtime_status` works, `list_projects` shows an `agent:<client_id>:<project_id>` project, the client can read `README.md`, and a small edit can be inspected and reverted.

For a walkthrough of the expected tool flow, see [docs/DEMO.md](docs/DEMO.md).

## Normal Coding Loop

WebCodex is designed around a conservative coding loop:

1. `start_coding_task` - create an explicit session and collect bounded startup context.
2. Inspect - use `list_project_files`, `search_project_text`, `read_file`, and Git status tools.
3. Edit - prefer `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`, or `apply_patch_checked`.
4. Validate - run `validate_patch`, `cargo_fmt`, `cargo_check`, or `cargo_test` where appropriate.
5. Inspect outcome - use `show_changes`, `git_diff_hunks`, and `workspace_hygiene_check`.
6. Finish - use `finish_coding_task` or `session_handoff_summary` for a compact closeout.

`run_shell` and `run_job` exist for bounded escape hatches. They are powerful and should not be the default editing or validation path.

## Safety Model

WebCodex does not make an online model a trusted local user. The model can only call exposed tools, those tools run through server policy and agent project boundaries, and shell/job tools remain explicit high-risk operations.

Projects live on the agent machine. The agent registers allowed directories with the server. The server does not scan your filesystem.

Do not expose secrets in prompts, examples, tool output, docs, or committed config files. For the full boundary model, read [SECURITY.md](SECURITY.md) and [docs/CONCEPTS.md](docs/CONCEPTS.md).

## Current Capabilities

- Remote MCP endpoint and GPT Actions OpenAPI surface backed by one ToolRuntime.
- Agent-registered project model with `agent:<client_id>:<project_id>` ids.
- Structured source editing tools for scoped changes.
- Patch validation, Cargo validation helpers, Git diff/status tools, and bounded shell/job execution.
- Coding-task sessions with handoff, finish verdicts, diff evidence, and hygiene summaries.
- Authentication paths for quick shared-key evaluation and production deployments.
- Documentation for first setup, concepts, MCP, GPT Actions, security, release notes, and roadmap.

## Known Limitations

- WebCodex is self-hosted infrastructure, not a hosted SaaS.
- Setup is still technical and assumes comfort with a terminal, server URL, and agent process.
- Semantic code intelligence, LSP diagnostics, and symbol navigation are not first-class in 0.2.0.
- The browser console is read-only and minimal; MCP, GPT Actions, and CLI workflows are the primary paths.
- WebCodex does not provide a full UI approval/review queue yet. Review means inspecting diff, validation, hygiene, and session evidence before accepting the work in Git.
- Shell and job tools require operator trust, bounded configuration, and normal code review discipline.

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
