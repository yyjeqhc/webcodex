# Security Policy

## Supported versions

WebCodex v0.1.x is an early release. Security fixes are expected to target the latest v0.1.x release unless stated otherwise.

## Reporting vulnerabilities

Please report vulnerabilities through GitHub Issues on `yyjeqhc/webcodex` or by contacting the maintainer privately through GitHub if the report contains sensitive details. Do not publish real tokens, env files, complete agent configs, or exploit details in public issues.

## Secret handling

Never share or commit:

- server bootstrap tokens,
- user API tokens,
- agent tokens,
- env files,
- complete `agent.toml` files,
- Authorization headers.

Share only short-lived pairing codes when enrolling a client machine.

## Threat model summary

WebCodex is a remote tool execution system. Deploy it as if connected clients can request project file reads, writes, shell commands, git operations, and optional Codex CLI jobs within the agent policy. Keep `allowed_roots` narrow, use separate user and agent tokens, and keep GPT Actions/MCP surfaces limited to the intended tool APIs.

## Known limitations

v0.1.x is an early release intended for controlled environments. Review agent policy, reverse proxy configuration, token storage, and exposed GPT Actions before using it with sensitive repositories.
