# Agent Protocol

WebCodex agents connect to the server and execute registered project tools. WebSocket is preferred; polling remains available as a fallback.

## Authentication

Agents should use agent tokens created during client enrollment:

```bash
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID
```

The server/admin side creates the temporary code with `webcodex-cli pairing create`. The agent token is returned to the client during enroll and written into the generated `agent.toml`; do not copy agent token files from the server.

Transport auth rules:

- WebSocket: `Authorization: Bearer <agent-token>` in the handshake headers is preferred.
- WebSocket compatibility: `/api/agents/ws?token=...` is accepted for handshake compatibility only.
- Polling: every request must use `Authorization: Bearer <agent-token>`.
- REST, MCP, and GPT Actions ordinary APIs must use `Authorization: Bearer ...`.

Do not use query-string tokens outside `/api/agents/ws`.

## Registration and identity

Agents register with:

- `client_id`
- `owner`
- `transport`
- `agent_instance_id`
- capabilities
- registered projects
- redacted policy summary

`agent_instance_id` identifies a running agent instance separately from the stable `client_id`.

## Policy summary

`runtime_status` and `listAgents` expose a redacted summary for operators:

- `allow_raw_shell`
- `allow_cwd_anywhere`
- `allowed_roots`
- `max_timeout_secs`
- `max_output_bytes`

They do not expose tokens, full env, `Authorization` headers, complete `agent.toml`, or shell `init_script` values.

Policy default:

- If `allowed_roots` is missing or empty, it defaults to `$HOME`.
- Explicit `allowed_roots` replaces that `$HOME` default.

## Project ids

Agent-backed project ids are reported as:

```text
agent:<client_id>:<project_id>
```

The server routes project tool calls to the owning connected agent.

## Optional Codex jobs

Codex jobs are a project tool path, not an agent lifecycle mechanism. `runCodexTask` requires the Codex CLI on the agent host and does not start a new `webcodex-agent`.
