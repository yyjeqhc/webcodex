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

## Validation recipe contract

`checks_run` remains the only structured validation Action. It accepts an
optional `recipe` enum (`rust`, `node`, `python`, `go`); omit it for
deterministic nearest-manifest resolution from the Task workspace and relative
`cwd`. Supply it only when `validation_recipe_ambiguous` identifies multiple
markers at the same nearest root. The model cannot provide a program, argv,
script body, or shell command through this Action.

Rust supports `format/check/test`; Node uses an evidenced package manager and
fixed non-mutating script-name order; Python uses only configured Ruff/Black,
Ruff/Mypy, and pytest; Go supports `check/test` and reports `format`
unavailable. Recipes do not install dependencies, mutate lockfiles, or use the
network. A missing tool is an executor failure, while a started validator's
non-zero verdict is an assertion failure. Resolved recipe version, relative
root, invocation, and manifest/lock evidence bind `operation_id`, so use a new
ID after a recipe or workspace change.

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
- `validation_recipe_not_found` / `validation_recipe_ambiguous`: change `cwd`
  or provide the matching explicit recipe.
- `validation_recipe_mismatch` / `validation_manifest_invalid` /
  `package_manager_ambiguous`: correct the reported public project evidence.
- `validation_check_unavailable` / `test_filter_unsupported`: request only a
  semantic input the resolved recipe supports.
- `validation_tool_unavailable`: provide the project's existing tool on the
  Agent host, then use a new operation ID.
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
