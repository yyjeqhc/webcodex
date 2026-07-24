# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Use GPT Actions when a Custom GPT should call the project-bound WebCodex
Connector. Use MCP when the client supports MCP directly.

## Schema

Import:

```text
https://your-domain.example/openapi.json
```

ChatGPT requires public HTTPS. `webcodex setup` intentionally creates only a
loopback project runtime; ingress and production authentication are operator
responsibilities described in [DEPLOYMENT.md](DEPLOYMENT.md).

Configure Bearer/API-key authentication with a scoped runtime credential. Do
not paste bootstrap/admin, account, or Agent credentials into a GPT.

## Canonical Hosted Operations

For a project-bound Connector, OpenAPI is generated from the same nine
capabilities as MCP:

```text
task_start
files_read
files_search
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
```

The operation count is generated and tested; setup, pairing, token management,
Agent management, audit endpoints, and legacy `/api/codex` routes are not in
the Action schema.

The Connector already owns a deterministic project binding. A Custom GPT must
not call `listProjects`, `runtime_status`, `tool_manifest`, `start_session`, or
Agent listing before normal coding, and the prompt must not contain an Agent
client ID or runtime project ID.

## Suggested GPT Instructions

```text
Use the configured WebCodex project.
Start each bounded request with task_start.
Use files_read/files_search before edits_apply.
Use a stable operation_id for exact retry.
Run checks_run before task_finish.
Use task_review for execution progress and result review.
Use commands_run only when structured capabilities are insufficient and
approval is available.
Never ask the user for internal routing, Agent, transport, queue, or workflow
session identifiers.
```

## Human Decision

`task_finish` creates a stable result; it does not silently apply changes to the
target checkout. The host user reviews and decides:

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# or: webcodex task reject <task-id>
```

This keeps the acceptance authority local even when the model is hosted.

## Common Errors

- `project_not_configured`: run `webcodex setup`.
- `project_registration_invalid` / `project_credential_invalid`: resolve the
  reported private-state problem; setup will not overwrite or silently rotate
  it.
- `project_credential_rejected`: restore the credential matching the reachable
  server; this is not `agent_offline`.
- `server_unreachable` / `agent_offline`: run `webcodex doctor`, then the
  reported next action.
- `required_capability_unavailable` /
  `structured_validation_unavailable`: upgrade all WebCodex binaries.
- `task_not_active`: start a new task.
- `execution_not_terminal`: review, wait, or cancel the execution.
- `checks_required`: call `checks_run`.
- `checks_stale`: run a fresh check with a new operation ID.

Every error carries a stable code, human message, retryability,
`user_action_required`, and a suggested next action. Control flow must use the
code, never arbitrary English message matching.

## Related Documentation

- [QUICK_START.md](QUICK_START.md)
- [MCP.md](MCP.md)
- [AUTH_MODEL.md](AUTH_MODEL.md)
- [DEPLOYMENT.md](DEPLOYMENT.md)
- [../SECURITY.md](../SECURITY.md)
