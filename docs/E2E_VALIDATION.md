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
- Deployment artifact transfer smoke covers bounded runtime discovery, chunked
  artifact upload/readback, cleanup, and git cleanliness on a safe smoke
  project.

## Deployment artifact transfer smoke

Use `scripts/smoke_artifact_transfer.sh` after a deployment when you need a
focused artifact transfer and runtime discovery check. The default mode prints
the checklist only and does not read token variables or contact a server:

```bash
bash scripts/smoke_artifact_transfer.sh
```

Active mode requires an explicit public URL and a GPT/MCP-capable token:

```bash
WEBCODEX_SMOKE_RUN=1 \
WEBCODEX_PUBLIC_URL="https://webcodex.example.com" \
WEBCODEX_TOKEN="<wc_pat_or_allowed_shared_key>" \
bash scripts/smoke_artifact_transfer.sh
```

The smoke project should be a disposable, agent-backed git repository such as
`agent:special:webcodex-smoke`. Do not use `wc_agent_*` for GPT Actions, MCP, or
this smoke script; that token type is only for `webcodex-agent`.

## Authentication

REST, polling, MCP, and GPT Actions calls must use:

```text
Authorization: Bearer <token>
```

`?token=` is only for `/api/agents/ws` WebSocket handshake compatibility.

## Codex CLI

WebCodex no longer exposes `run_codex` or legacy `/api/codex/*` routes. GPT Actions and MCP clients should use structured edit tools plus `cargo_fmt`, `cargo_check`, `cargo_test`, `validate_patch`, and `apply_patch_checked` first. Treat `run_job` and `run_shell` as bounded fallback diagnostics/build/test tools, not the default validation source. Run Codex outside WebCodex for Codex-specific workflows.

## Management setup

Prefer:

```bash
webcodex-cli server init
webcodex-cli server install-service
webcodex-cli server status
webcodex-cli pairing create --server-url URL --username alice --client-id alice-laptop
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id alice-laptop
webcodex-cli agent install-service --profile workstation --bin /opt/webcodex/bin/webcodex-agent
webcodex-cli agent status --profile workstation --server-url URL
webcodex-cli doctor --strict --profile workstation --server-url URL
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
