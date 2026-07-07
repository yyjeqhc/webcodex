# Agent Protocol

[English](AGENT_PROTOCOL.md) | [简体中文](AGENT_PROTOCOL.zh-CN.md)

WebCodex agents connect to the server and execute registered project tools. New deployments should prefer `transport = "auto"` with QUIC configured; WebSocket and polling remain fallback transports.

## Authentication

Agents should use agent tokens created during client enrollment:

```bash
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id CLIENT_ID
```

The server/admin side creates the temporary code with `webcodex-cli pairing create`. The agent token is returned to the client during enroll and written into the generated `agent.toml`; do not copy agent token files from the server. For binary deployments, install the client-side service with `webcodex-cli agent install-service` and inspect it with `webcodex-cli agent status`.

Transport auth rules:

- QUIC: the agent token stays in the top-level agent config and is sent inside the agent registration envelope over the QUIC stream.
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

## Codex-specific workflows

WebCodex no longer exposes `run_codex` or legacy `/api/codex/*` routes. Agent lifecycle and project dispatch use structured runtime tools, agent-registered projects, bounded shell/job validation, MCP, and GPT Actions. Run Codex outside WebCodex when needed.
