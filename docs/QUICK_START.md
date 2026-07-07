# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This is the recommended local-first path for trying WebCodex.

For vocabulary, read [CONCEPTS.md](CONCEPTS.md). For a realistic tool flow, read [DEMO.md](DEMO.md).

## Fastest Path

Use one shared key for the first evaluation: server runtime calls, agent connect, and MCP/GPT Actions all use the same key. Move to scoped tokens, OAuth, and production deployment later.

## What You Will Run

- A WebCodex server reachable at a local or HTTPS URL.
- A WebCodex agent running on the machine that has the code.
- One project registered by the agent.
- One online client: remote MCP if your client supports it, or GPT Actions if you are building a Custom GPT.

MCP and GPT Actions call the same WebCodex ToolRuntime. The client changes the protocol framing, not the project boundary or tool behavior.

## Prerequisites

- Rust and Cargo if you are running from this checkout.
- A machine that can run both the server and the agent for the first test.
- A code repository you are willing to inspect and edit in a controlled way.
- A client:
  - ChatGPT MCP / remote MCP client, preferred when available.
  - ChatGPT Custom GPT with GPT Actions when MCP is not the target.

Do not use real secrets, production repositories, or privileged shell profiles for the first run.

## 1. Build The Binaries

From the WebCodex checkout:

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

Released binary artifacts can replace the `cargo build` step once the matching release is available.

## 2. Choose One Shared Key And Start The Server

In terminal 1, choose one long random key for this evaluation:

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"
```

Use the same `WEBCODEX_KEY` value later for `webcodex-cli connect`, `curl`, MCP, and GPT Actions. Do not paste real key values into committed files.

Prepare the server env:

```bash
webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080
```

`server up` enables shared-key quick-start mode and writes the server env file. It does not take a `--key` flag, and it intentionally does not print the full server bootstrap key.

Load the env and start the server:

```bash
set -a
. "$WEBCODEX_ENV"
set +a
webcodex
```

Keep the `webcodex` process running.

For a public ChatGPT connection, put the server behind HTTPS and use that public URL instead. Localhost is enough for a local runtime sanity check.

## 3. Connect An Agent And Register A Project

In terminal 2, from the repository you want WebCodex to operate:

```bash
export WEBCODEX_KEY="<same evaluation shared key>"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite
```

The command generates an agent config and a project registry entry for the selected root. Start the agent with the config path printed by `connect`; with the default client id it is:

```bash
webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

Projects live on the agent machine. The agent registers allowed directories with the server. The server does not scan your filesystem.

## 4. Verify Runtime Health

In terminal 3:

```bash
export WEBCODEX_KEY="<same evaluation shared key>"

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"runtime_status","summary_only":true}'
```

Then verify projects:

```bash
curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"list_projects"}'
```

You should see a project id shaped like:

```text
agent:local-dev:<project_id>
```

If you used `connect` from a repository root, the generated project id is printed by that command. Use the full runtime project id in client prompts and tool calls.

## 5. Connect ChatGPT MCP

Use MCP if your client supports remote MCP.

Configure the client with:

```text
URL:  http://127.0.0.1:8080/mcp
Auth: Bearer <shared key>
```

For ChatGPT or another hosted client, replace localhost with the public HTTPS server URL:

```text
https://your-domain.example/mcp
```

For the first evaluation, use `Bearer <shared key>`. Production auth comes later. See [MCP.md](MCP.md) for screenshots and common MCP errors.

## 6. Or Connect GPT Actions

Use GPT Actions if you are building a Custom GPT.

Import the schema:

```text
http://127.0.0.1:8080/openapi.json
```

For ChatGPT, use a public HTTPS URL:

```text
https://your-domain.example/openapi.json
```

Configure Action authentication as Bearer/API-key auth and use the same shared key for this first evaluation. See [GPT_ACTIONS.md](GPT_ACTIONS.md) for the setup guide.

## 7. Run A Read-Only Task

Ask the client to stay read-only first:

```text
Use WebCodex on project agent:local-dev:<project_id>.
Start a coding task, inspect README.md, summarize what the project does,
show changes without a diff, run workspace hygiene, and finish the task.
Do not edit files.
```

Expected flow:

1. `start_coding_task`
2. `read_file` or `search_project_text`
3. `show_changes`
4. `workspace_hygiene_check`
5. `finish_coding_task`

## 8. Run A Small Reversible Edit

Use a disposable branch or a tiny documentation edit:

```text
Use WebCodex on project agent:local-dev:<project_id>.
Make one small documentation edit, validate what is appropriate for a docs-only
change, show changes, run workspace hygiene, and finish with a clear verdict.
Prefer structured edit tools. Do not use run_shell unless needed.
```

Review the changed files and diff before accepting the result. Revert the edit manually or with your usual Git workflow if it was only a smoke test.

## First Success Criteria

You are set up when:

- `runtime_status` works.
- `list_projects` shows an `agent:<client_id>:<project_id>` project.
- The client can read `README.md`.
- A read-only coding task finishes cleanly.
- A small edit can be reviewed and reverted.

## Production Auth Comes Later

This shared-key path is for first evaluation. For production, read [AUTH_MODEL.md](AUTH_MODEL.md), [DEPLOYMENT.md](DEPLOYMENT.md), and [OPERATIONS.md](OPERATIONS.md), then move to scoped user tokens or OAuth, reverse proxy HTTPS, service management, and token rotation.

## MCP Vs GPT Actions

- Use MCP if your client supports remote MCP.
- Use GPT Actions if you are building a Custom GPT.
- Both surfaces call the same WebCodex ToolRuntime.

The safest first prompt should name the exact project id and ask for a read-only task. Move to write tasks only after the client can inspect and finish cleanly.

## Safety Defaults

- Project access is agent-registered.
- The server does not scan the filesystem.
- The model can only call exposed tools.
- Structured edit and validation tools are preferred.
- `run_shell` is a bounded escape hatch, not the default editing or validation path.
- Do not paste bootstrap, account, or agent credentials into MCP or GPT Actions.

For the full boundary model, read [../SECURITY.md](../SECURITY.md).

## Troubleshooting

### Agent Not Connected

Check the agent process logs and confirm it was started with the generated config. Run `runtime_status` and look for online agent counts before asking the model to edit.

### Project Not Listed

Run `list_projects`. If the project is missing, rerun `webcodex-cli connect` from the intended repository root or inspect the generated agent project registry. No server-side project registry is required.

### Auth Failed

Use the same `WEBCODEX_KEY` value for agent connect, runtime checks, MCP, and GPT Actions. For production auth, switch to [AUTH_MODEL.md](AUTH_MODEL.md) instead of reusing bootstrap, account, or agent credentials.

### Model Chose The Wrong Project Id

Put the full `agent:<client_id>:<project_id>` value in the prompt. Ask the client to call `list_projects` and confirm the selected project before reading or editing files.

### Response Too Large

Use compact summaries: `runtime_status(summary_only=true)`, focused `tool_manifest` discovery, bounded file ranges, `show_changes(include_diff=false)`, and `finish_coding_task(summary_only=true)`.

### Shell Or Job Feels Too Broad

Prefer structured tools first: `read_file`, `search_project_text`, line edits, `apply_text_edits`, `validate_patch`, `cargo_fmt`, `cargo_check`, `cargo_test`, `show_changes`, and `workspace_hygiene_check`.

## Next Docs

- Demo workflow: [DEMO.md](DEMO.md)
- Concepts: [CONCEPTS.md](CONCEPTS.md)
- MCP setup: [MCP.md](MCP.md)
- GPT Actions setup: [GPT_ACTIONS.md](GPT_ACTIONS.md)
- Auth model: [AUTH_MODEL.md](AUTH_MODEL.md)
- Deployment details: [DEPLOYMENT.md](DEPLOYMENT.md)
- Operations: [OPERATIONS.md](OPERATIONS.md)
- Troubleshooting: [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
