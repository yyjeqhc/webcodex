# GPT Actions

WebCodex exposes a focused OpenAPI schema for ChatGPT GPT Actions at:

```text
GET /openapi.json
```

GPT Actions and MCP share the same `ToolRuntime`; GPT Actions provides typed REST operations while MCP provides MCP framing.

## Authentication

Configure the GPT Action with Bearer/API-key authentication in the GPT Action settings. The secret value must be a `wc_pat_xxx` personal API token.

Use a `wc_pat_xxx` personal API token for GPT Actions and MCP. The recommended explicit flow is: an administrator issues a one-time `wc_acct_xxx` account credential, then the user runs `webcodex-cli token create-local` locally to generate a `wc_pat_xxx` and register only its hash with the server.

Do not paste or store `WEBCODEX_TOKEN`, `wc_acct_xxx`, or `wc_agent_xxx` as a GPT Actions or MCP credential. `WEBCODEX_TOKEN` is only for server bootstrap/root/admin work, `wc_acct_xxx` is only for local token self-registration, and `wc_agent_xxx` is only for `webcodex-agent` WebSocket connectivity. Pairing/enrollment remains available as a shortcut: `webcodex-cli pairing create` creates a short-lived `wc_pair_*` code on the server/admin side, and `webcodex-cli client enroll` exchanges that code on the client side.

`?token=` is not a GPT Actions auth mechanism. It is accepted only by `/api/agents/ws` for WebSocket handshake compatibility.

GPT Actions require a public HTTPS URL for the WebCodex server.


## Token selection

Credential purpose summary:

- GPT Actions / MCP / `/api/tools/list` / `/api/tools/call`: use `wc_pat_xxx`.
- Server bootstrap and emergency admin: use `WEBCODEX_TOKEN`.
- Local self-registration of PATs and agent tokens: use `wc_acct_xxx` only with `webcodex-cli token create-local` or `webcodex-cli agent-token create-local`.
- Agent connection: use `wc_agent_xxx` only in `webcodex-agent` config.

A GPT Action configured with `wc_acct_xxx` will not be able to call runtime tools and leaks the wrong secret into the wrong surface. Generate a PAT instead:

```bash
webcodex-cli token create-local \
  --server https://sg4.yyjeqhc.cn \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

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
