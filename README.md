# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

Connect ChatGPT to private code that stays on your machine.

WebCodex runs a small server and an agent next to your repo. ChatGPT can inspect files, request scoped edits, and run validation through WebCodex while your repository stays where it is.

- Choose exactly which project directories are available.
- Keep test and shell execution on the agent machine.
- Review changed files, validation output, and task summaries before accepting work.
- Start locally, then put the server behind HTTPS when using hosted ChatGPT clients.

## Install

For the current Linux x64 release:

```bash
npm install -g @yyjeqhc/webcodex
```

Or build from source:

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

See [docs/BUILD_INSTALL.md](docs/BUILD_INSTALL.md) for platform support and install details.

## Quick Start

For a first local run, use two terminals. The commands below assume the npm-installed binaries are on your `PATH`.
If you built from source, export `PATH="$PWD/target/release:$PATH"` first.

Terminal 1 - create the server config and start the server:

```bash
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"

webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080

WEBCODEX_ENV_FILE="$WEBCODEX_ENV" webcodex
```

`server up` creates `$WEBCODEX_ENV` if it does not exist, including the parent directory. That file stores server settings and a server admin key. It is not the evaluation key you paste into clients.

Terminal 2 - create one evaluation key, register a repo, and start the agent:

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
printf 'Use this value as your MCP/GPT Actions Bearer key: %s\n' "$WEBCODEX_KEY"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite

webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

Use the same key in three places: `webcodex-cli connect --key`, the Verify curl commands below, and your MCP/GPT Actions Bearer/API-key auth field. The server does not need the value ahead of time; quick-start mode matches clients and agents by the key's hash.

## Verify

In another terminal, paste the same evaluation key:

```bash
export WEBCODEX_KEY="<same evaluation key>"

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"runtime_status","summary_only":true}'

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"list_projects"}'
```

Use the returned `agent:<client_id>:<project_id>` value in MCP or GPT Actions prompts.

For the full walkthrough, see [docs/QUICK_START.md](docs/QUICK_START.md).

## Client Access

- Local clients can use `http://127.0.0.1:8080`.
- ChatGPT-hosted clients need a public HTTPS URL. Put WebCodex behind Nginx, Caddy, or a tunnel, start the server with `--public-url https://your-domain.example`, then use `https://your-domain.example/openapi.json` or `https://your-domain.example/mcp`.
- MCP clients use `/mcp`; GPT Actions use `/openapi.json`. For the first run, paste the same evaluation key into the Bearer/API-key auth field.

## How It Fits Together

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
