# MCP

WebCodex exposes the same runtime tools used by GPT Actions through an MCP endpoint.

## Endpoint

```text
https://your-domain.example/mcp
```

Use your own WebCodex HTTPS domain in place of `your-domain.example`.

## Authentication

Use Bearer authentication with a `wc_pat_xxx` personal API token.

Do not use these credentials for MCP:

- `WEBCODEX_TOKEN`: server bootstrap/root/admin credential.
- `wc_acct_xxx`: account credential used only by the user CLI to create local PATs and agent tokens.
- `wc_agent_xxx`: agent token used only by `webcodex-agent`.

The recommended flow is to issue a user account credential once, then have the user run `webcodex-cli token create-local` locally. That command generates a `wc_pat_xxx` and registers only its hash with the server.

## Runtime surface

MCP and GPT Actions share the same `ToolRuntime`. A tool call made through MCP reaches the same runtime, agent registry, project ids, and safety boundaries as a GPT Action call.

Typical MCP tools include:

- `list_tools`
- `runtime_status`
- `list_projects`
- `git_status`
- `read_file`
- `validate_patch`
- `run_shell`
- `run_codex`

Use agent-backed project ids such as:

```text
agent:<client_id>:<project_id>
```

For example, the sg4 smoke test used `agent:ubuntu-client:webcodex`.

## Example client configuration

The exact shape depends on your MCP client. Use placeholders and environment variables for secrets; do not paste real tokens into committed config files.

```json
{
  "mcpServers": {
    "webcodex": {
      "url": "https://your-domain.example/mcp",
      "headers": {
        "<bearer-auth-header-name>": "Bearer ${WEBCODEX_PAT}"
      }
    }
  }
}
```

Where `WEBCODEX_PAT` contains a `wc_pat_xxx` value generated with `webcodex-cli token create-local`.

## Common errors

### 401 Unauthorized

The token is missing, malformed, expired, revoked, or not recognized by the server. Generate a fresh `wc_pat_xxx` and verify the MCP client is reading the intended environment variable.

### 403 Forbidden

The token is valid but lacks the scope needed for the requested tool or project operation. Create a PAT with the scopes needed for your workflow.

### Wrong token type

MCP requires `wc_pat_xxx`. `WEBCODEX_TOKEN`, `wc_acct_xxx`, and `wc_agent_xxx` are intentionally for other surfaces.

### Agent offline

The server is up, but the selected `client_id` is offline or stale. Start `webcodex-agent` and check `runtime_status` or `list_agents`.

### Project not registered

The agent is online, but the requested `agent:<client_id>:<project_id>` does not exist. Add a top-level agent `projects.d/*.toml` file with `id` and `path`, then restart or refresh the agent.
