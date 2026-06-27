# GPT Actions

WebCodex exposes a focused OpenAPI schema for ChatGPT GPT Actions at:

```text
GET /openapi.json
```

GPT Actions and MCP share the same `ToolRuntime`; GPT Actions provides typed REST operations while MCP provides MCP framing.

## Authentication

Configure the GPT Action with HTTP Bearer authentication:

```text
Authorization: Bearer <wc_pat_user_api_token>
```

Do not paste or store the bootstrap server token as the day-to-day GPT Actions credential. Use pairing/enrollment instead: `webcodex-cli pairing create` creates a short-lived code on the server/admin side, and `webcodex-cli client enroll` exchanges that code on the client side. GPT Actions should use the generated client-side `webcodex-user-token` file. Do not copy token files from the server to the client.

`?token=` is not a GPT Actions auth mechanism. It is accepted only by `/api/agents/ws` for WebSocket handshake compatibility.

## Tool surface

The GPT Actions surface is intentionally smaller than the full admin API. It includes runtime, project, git, patch, file, shell/job, and optional Codex task operations.

It does not expose user, API-token, agent-token, pairing/enrollment, setup, doctor, npm, server management, or audit endpoints such as:

```text
/api/users/create
/api/tokens/create
/api/agent-tokens/create
/api/pairing/create
/api/pairing/enroll
/api/audit/sessions
```

Use `webcodex-cli` for those management tasks.

## Recommended flow

1. `getRuntimeStatus` — verify runtime health and redacted agent policy summaries.
2. `listAgents` — confirm an online agent and its `agent_instance_id`.
3. `listProjects` — choose an `agent:<client_id>:<project_id>`.
4. `getProjectGitStatus`, `listProjectFiles`, `readProjectFile` — inspect first.
5. `validateProjectPatch` — dry-run patches before applying.
6. `applyProjectPatchChecked`, `writeProjectFile`, or `replaceProjectFileText` — mutate only when intended.
7. `runProjectShellCommand` or `startProjectShellJob` — execute only bounded commands in registered projects.
8. `runCodexTask` — optional advanced path when Codex CLI is installed and configured on the agent machine.

`runCodexTask` does not launch a new agent. It asks the already connected agent to run the Codex CLI in a project.

## Observability

`getRuntimeStatus` and `listAgents` may show a redacted policy summary:

- `allow_raw_shell`
- `allow_cwd_anywhere`
- `allowed_roots`
- `max_timeout_secs`
- `max_output_bytes`

They must not expose tokens, env values, `Authorization` headers, full `agent.toml`, or shell `init_script` values.

## Compatibility notes

The management CLI compatibility commands `webcodex users`, `webcodex tokens`, and `webcodex agent-tokens` still work, but `webcodex-cli` is the recommended CLI for current setup and operations.
