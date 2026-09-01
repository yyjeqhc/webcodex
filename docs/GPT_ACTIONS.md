# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Use GPT Actions when a Custom GPT should call WebCodex through the Server's
OpenAPI surface. Use [MCP](MCP.md) when the client supports MCP directly.
Both are adapters over the same runtime, but the exposed schema depends on the
Server mode: a Connector-configured project-first Server exposes the
project-bound Connector schema, while a generic hosted/self-hosted Server
exposes the standard runtime OpenAPI schema.

## What a GPT Action is

WebCodex provides an OpenAPI-based **Custom GPT Action** integration. This is
not a published ChatGPT plugin. In current OpenAI terminology, an app, a
Custom GPT, and an Action are distinct layers; a plugin is an installable
bundle in the ChatGPT/Codex plugin directory. See OpenAI's
[GPT Actions introduction](https://developers.openai.com/api/docs/actions/introduction).

## Import the schema

Import the OpenAPI schema into your Custom GPT:

```text
https://your-domain.example/openapi.json
```

Inspect the imported operation names rather than assuming one schema shape. A
Server with project-bound Connector configuration returns the fourteen
capabilities documented below; a generic Server returns the standard runtime
OpenAPI projection instead.

ChatGPT requires public HTTPS. Configure API-key authentication as an HTTP
Bearer credential. Use a generated `webcodex-user-token` (`wc_pat_*`) — it is
for GPT Actions, MCP, and ordinary REST/project APIs. The Runner token
(`wc_agent_*`) is accepted only by Runner transport endpoints; do not paste
it, a bootstrap/admin token, or an account credential into a GPT.

The OpenAPI management surface intentionally excludes users, API tokens,
agent tokens, pairing/enrollment, setup, doctor, npm, server management, and
audit endpoints. Use the `webcodex` CLI for those tasks.

## The Connector surface

When the Server is running with project-bound Connector configuration,
OpenAPI is generated from the same fourteen capabilities as the canonical MCP
Connector:

```text
task_start
task_list
task_resume
files_list
files_read
files_search
code_navigate
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
code_impact
```

The Connector already owns a deterministic project binding. A Custom GPT must
not call `listProjects`, `runtime_status`, `tool_manifest`, `start_session`,
or Agent listing before normal coding, and the prompt must not contain an
Agent client ID or runtime project ID.

`task_start` accepts only `normal` (default) and `read_only`. `normal` performs
writable work in a managed isolated Git worktree and fails closed if that
workspace cannot be prepared; the model never writes the target checkout or
accepts its own result. `read_only` permits analysis but rejects edits, commands,
and checks. The pre-0.4 `inspect` mode is retired with no restricted-shell alias.

## Suggested GPT instructions

```text
Use the configured WebCodex project.
Start or continue each user instruction with task_start.
Let task_start reuse the current project context; do not ask the user for IDs.
Use task_list and task_resume only after WebCodex reports that automatic
transport-window recovery is unavailable.
Use files_list to see what the project contains before guessing paths.
Use files_read/files_search before edits_apply.
Use code_navigate for read-only semantic status, symbols, definitions,
references, diagnostics, and hover; provide only project-relative paths.
Use code_impact for bounded incoming/outgoing call hierarchy and change-impact
inspection; provide only a project-relative path and source position.
Use a stable operation_id for exact retry.
Run checks_run before task_finish.
Use task_review for execution progress and result review.
Use commands_run only when structured capabilities are insufficient and
approval is available.
Never ask the user for task, session, current-binding, Agent, transport, queue,
or workflow identifiers.
```

## Validation

`checks_run` is the only structured validation Action. It accepts an optional
`recipe` enum (`rust`, `node`, `python`, `go`); omit it for deterministic
nearest-manifest resolution. Recipes do not install dependencies, mutate
lockfiles, or use the network. A missing tool is an executor failure; a started
validator's non-zero verdict is an assertion failure. See
[MCP](MCP.md#validation-recipes) for the recipe table.

## Human decision

`task_finish` creates a stable result; it does not silently apply changes to
the target checkout. The host user reviews and decides locally:

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# or: webcodex task reject <task-id>
```

This keeps the acceptance authority local even when the model is hosted.

## Common errors

- An authentication error after copying a `wc_agent_*` value means the wrong
  credential type was selected. Use the generated `webcodex-user-token`
  instead; never paste complete token values into logs or bug reports.
- `project_not_configured`: run `webcodex setup`.
- `project_credential_invalid` / `project_credential_rejected`: resolve the
  reported private-state problem, then restore the matching credential.
- `server_unreachable` / `agent_offline`: run `webcodex doctor`, then the
  reported next action.
- `required_capability_unavailable` / `structured_validation_unavailable`:
  upgrade all WebCodex binaries.
- `checks_required`: call `checks_run`.
- `checks_stale`: run a fresh check with a new operation ID.

Every error carries a stable code, human message, retryability, and a suggested
next action. Control flow should use the code, never arbitrary English message
matching.

## Related

- [Full Setup](PERSONAL_SETUP.md)
- [Quick Trial](QUICK_START.md)
- [MCP](MCP.md)
- [Authentication](AUTH_MODEL.md)
- [Deployment](DEPLOYMENT.md)
- [SECURITY.md](../SECURITY.md)
