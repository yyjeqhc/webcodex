# E2E Validation

[English](E2E_VALIDATION.md) | [简体中文](E2E_VALIDATION.zh-CN.md)

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
webcodex-cli server init
webcodex-cli server install-service
webcodex-cli server status
webcodex-cli pairing create --server-url URL --username alice --client-id alice-laptop
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id alice-laptop
webcodex-cli agent install-service --profile special --bin /opt/webcodex/bin/webcodex-agent
webcodex-cli agent status --profile special --server-url URL
webcodex-cli doctor --strict --profile special --server-url URL
```

`pairing create` is server/admin-side. `client enroll`, `agent install-service`, and `agent status` are client-side for the machine running `webcodex-agent`. Do not copy server tokens to the client; copy only the short-lived pairing code.
## Binary help validation

Before release or large documentation changes, verify the command examples against the binaries:

```bash
webcodex-cli -h
webcodex-cli server init -h
webcodex-cli server install-service -h
webcodex-cli server status -h
webcodex-cli pairing create -h
webcodex-cli client enroll -h
webcodex-cli agent install-service -h
webcodex-cli agent status -h
webcodex-cli doctor -h
webcodex-agent -h
webcodex -h
```

Pay special attention to `users create --server-url ...` for admin-created account credentials versus `token create-local --server ...` and `agent-token create-local --server ...` for local token creation.


The compatibility entry points still exist, but new validation docs should use `webcodex-cli`.

## Documentation scans

During docs polish or release validation, run the repository's standard documentation scans for three classes of mistakes:

1. references to deleted historical planning documents,
2. old product names or legacy environment/key names,
3. obvious real-looking token values in user-facing docs and examples.

Expected result: no stale deleted-doc references, no old product names, and no real token-looking values. Placeholder forms such as `<token>` and `<wc_pair_...>` are fine.
