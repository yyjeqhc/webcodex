# E2E Validation

The E2E validation script is for local validation of a WebCodex server, agent, GPT Actions schema, and MCP endpoint without depending on ChatGPT UI.

## What it checks

A typical validation should confirm:

- `/openapi.json` is valid and has the expected GPT Actions operations.
- `runtime_status` reports `service=webcodex`.
- `listAgents` reports an online agent and redacted policy summary.
- `listProjects` reports agent-backed project ids.
- Read-only project tools work.
- Mutation tools work only against disposable projects.
- MCP tools/list matches the expected runtime tool surface.

## Authentication

REST, polling, MCP, and GPT Actions calls must use:

```text
Authorization: Bearer <token>
```

`?token=` is only for `/api/agents/ws` WebSocket handshake compatibility.

## Codex CLI

Codex validation is optional. `runCodexTask` requires the Codex CLI on the agent host. Local E2E tests may use a stub Codex binary; that does not start a separate `webcodex-agent`.

## Management setup

Prefer:

```bash
webcodex-cli pairing create --server-url URL --username alice --client-id alice-laptop
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id alice-laptop
webcodex-cli doctor --server-url URL --user-token-file PATH
```

The compatibility entry points still exist, but new validation docs should use `webcodex-cli`.
