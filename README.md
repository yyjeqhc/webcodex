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

For the localhost example, start with two terminals on the same machine: one for the server, one from the repo you want WebCodex to work on. The commands below assume the npm-installed binaries are on your `PATH`.
If you built from source, export `PATH="$PWD/target/release:$PATH"` first.

Server terminal - create the server config and start WebCodex:

```bash
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"

webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080

WEBCODEX_ENV_FILE="$WEBCODEX_ENV" webcodex
```

`server up` creates `$WEBCODEX_ENV` if it does not exist, including the parent directory. That file stores server settings and a server admin key. It is not the key you paste into MCP or GPT Actions.

Repo terminal - from the repo you want WebCodex to work on, create a key, register the repo, and start the agent:

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
printf 'Copy this key into MCP/GPT Actions auth: %s\n' "$WEBCODEX_KEY"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite

webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

Keep the printed key for this setup. Paste that same value when you verify with `curl` and when you configure MCP or GPT Actions auth. You do not need to add this key to the server config.

## Verify

In a client terminal, paste the same key printed above:

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
- MCP clients use `/mcp`; GPT Actions use `/openapi.json`. For the first run, use the key printed in the repo terminal as the Bearer/API-key value.

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
2. Inspect - use `list_project_files`, `search_project_text`, `read_file`, optional read-only LSP intelligence (`lsp_status`, `document_symbols`, `workspace_symbols`, `goto_definition`, `find_references`, `hover`), and Git status tools.
3. Edit - prefer `replace_line_range`, `insert_at_line`, `delete_line_range`, `apply_text_edits`, or `apply_patch_checked`.
4. Validate - run `validate_patch`, `cargo_fmt`, `cargo_check`, or `cargo_test` where appropriate.
5. Inspect validation evidence - use `validation_summary` to query the existing session ledger without rerunning tests.
6. Inspect outcome - use `show_changes`, `git_diff_hunks`, and `workspace_hygiene_check`.
7. Finish - use `finish_coding_task` or `session_handoff_summary` for a compact closeout.

`start_coding_task` always returns a compact `semantic_navigation` summary so the coding loop can decide whether to prefer the seven Rust LSP tools. Its bounded status-only probe inspects Rust workspace detection, rust-analyzer availability, and an existing supervisor slot without starting rust-analyzer, running Cargo, or injecting symbol, definition, or reference data. The capability is read-only and workspace-only; dependency navigation remains limited by `cargo.noDeps=true`. Open Rust documents are refreshed from validated workspace files with bounded full-text `didChange` notifications; editor-style incremental synchronization is not supported. An unavailable, crashed, legacy, disconnected, or timed-out LSP path does not block coding-task startup or change its startup verdict.

For Rust projects, the recommended semantic feedback loop is:

```text
start_coding_task
→ document_symbols / workspace_symbols
→ goto_definition / find_references / hover
→ read_file
→ edit
→ document_diagnostics
→ cargo_check / cargo_test
```

`document_diagnostics` is quick, bounded rust-analyzer feedback. The constrained profile can return no diagnostics or time out waiting for a fresh publication, and it never replaces final Cargo validation.

### Validation Intelligence MVP

Validation parser v2 (`structured_validation_parser`, version 2) deterministically extracts structured evidence from bounded, sanitized metadata already recorded for validation-like tools. A failing `cargo_check` can expose up to 20 sorted diagnostics with safe project-relative locations and messages of at most 240 Unicode scalars. A failing `cargo_test` can expose up to 20 failed-test names and conservative `assertion` / `panic` / `unknown` details without panic bodies, assertion values, or backtraces.

The parser never retains or returns complete stdout/stderr, never executes diagnostic text, and does not infer root causes. Incomplete excerpts are represented with `truncated`, `diagnostics_truncated`, omitted locations, and `unknown` classifications rather than guesses. `validation.status` remains the ledger history (`mixed` is preserved), while `latest_status` and `historical_failures` distinguish the final run from earlier failures. Resolved failures remain visible as audit evidence but do not lower an otherwise passing final task outcome; zero-test runs do not resolve earlier `cargo_test` failures.

`validation_summary(project, session_id, limit?)` is a read-only query over existing session evidence. It does not run Cargo, shell, an agent request, or file reads, and it is not a replacement for `finish_coding_task`, which still owns overall closeout. Recommended loop:

```text
edit
→ document_diagnostics
→ cargo_check / cargo_test
→ validation_summary
→ targeted fix
→ cargo_check / cargo_test
→ finish_coding_task
```

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
- Coding-task sessions with validation parser v2 evidence, read-only `validation_summary`, handoff, finish verdicts, diff evidence, and hygiene summaries.
- Authentication paths for quick shared-key evaluation and production deployments.
- Documentation for first setup, concepts, MCP, GPT Actions, security, release notes, and roadmap.

## Known Limitations

- WebCodex is self-hosted infrastructure, not a hosted SaaS.
- Setup is still technical and assumes comfort with a terminal, server URL, and agent process.
- Read-only Rust LSP intelligence is available through seven tools (`lsp_status`, `document_symbols`, `goto_definition`, `find_references`, `document_diagnostics`, `hover`, `workspace_symbols`) via agent-side rust-analyzer with a constrained profile (`cargo.noDeps=true`, no build scripts/proc macros/checkOnSave). Paths are project-relative; columns are 1-based Unicode scalars; external/dependency results are omitted. Open documents refresh from bounded workspace file content using full-text sync only. Diagnostics are bounded `publishDiagnostics` feedback with explicit freshness/timeout state, not a replacement for Cargo validation. Editor-style incremental sync, multi-language support, dependency navigation, and LSP write operations are not first-class.
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
