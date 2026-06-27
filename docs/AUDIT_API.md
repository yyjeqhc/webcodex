# Audit API

The Audit API is a server-side administrative/read-only API for reviewing recorded tool activity. It is intentionally not part of the GPT Actions OpenAPI schema and is not exposed as GPT Actions management operations.

Use normal REST authentication with:

```text
Authorization: Bearer <admin-or-user-token>
```

Do not send tokens in query strings for audit APIs.

## Scope

Audit endpoints are for operators and automation outside the GPT Actions surface. GPT Actions and MCP users should rely on the project/runtime tools exposed by `ToolRuntime`.

## Security expectations

Audit responses should not expose token values, env files, `Authorization` headers, or complete agent configuration files.

User, token, agent-token, setup, and audit management endpoints remain outside the GPT Actions OpenAPI management surface. Use `webcodex-cli` for management tasks.
