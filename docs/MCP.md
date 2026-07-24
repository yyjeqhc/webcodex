# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

Use MCP when the client can connect to the project-bound WebCodex endpoint.
Complete [QUICK_START.md](QUICK_START.md) first.

## Endpoint and Authentication

Local clients can use:

```text
http://127.0.0.1:<configured-port>/mcp
```

Hosted clients require an operator-managed HTTPS endpoint:

```text
https://your-domain.example/mcp
```

Configure Bearer authentication with a credential issued for runtime/project
access. Do not use or expose bootstrap/admin, account, or Agent credentials.
Prefer the client secret store; never commit a token.

Canonical setup does not print credential values or secret paths, create a
tunnel, or expose a public port. Production enrollment, scoped user tokens, and
OAuth are described in [AUTH_MODEL.md](AUTH_MODEL.md) and
[DEPLOYMENT.md](DEPLOYMENT.md).

## Project-Bound Surface

When the runtime is started from `webcodex setup`, MCP `tools/list` contains
exactly:

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

The Connector context already binds the project. Start with `task_start`; do
not call `list_projects`, `runtime_status`, `tool_manifest`, `start_session`,
or `current_session`, and do not ask the user for an Agent client ID, runtime
project ID, executor reference, or workflow session.

The stable visible IDs have product purposes:

- `task_id`: continue/review one bounded task;
- `operation_id`: exact retry identity for a mutation or execution;
- `execution_id`: inspect/wait/cancel one durable execution;
- `result_id`: review and decide one stable result.

Agent transport, executor routing, and pending request IDs remain internal.

## Golden Coding Loop

```text
task_start
→ files_read / files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

Use `commands_run` only as an approved escape hatch. Use `task_cancel` for a
queued/running execution that should stop.

Normal writable tasks require structured checks before finish. A successful
check carries trusted workspace provenance; any subsequent mutation makes it
stale and requires a new operation ID. A command that cannot spawn is an
executor failure, not assertion evidence.

### `checks_run` recipes

The `checks_run` schema still exposes only `format`, `check`, and `test`, with
an optional `recipe` enum (`rust`, `node`, `python`, `go`). Omit it to select
the nearest `Cargo.toml`, `package.json`, `pyproject.toml`, or `go.mod` from
the Task workspace and relative `cwd`. An explicit matching recipe resolves a
same-root ambiguity. The only markerless exception is explicit
`recipe=python` with `checks=["test"]`, which runs the fixed unittest discovery
plan from `cwd` when no `pyproject.toml` is selected. Resolution never scans
sibling projects or permits path/symlink escape.

Rust supports all three checks and is the only recipe with a one-argv
`test_filter`. Node resolves an evidenced package manager and fixed script
allowlist; Python selects configured Ruff/Black, Ruff/Mypy, or pytest, while
manifestless Python supports only `python -B -m unittest discover -v`; Go
supports `check` and `test`, with `format` unavailable. No recipe installs
dependencies, changes lockfiles, or uses the network. Tool absence is an
executor failure; a started validator returning non-zero is an assertion
failure. Recipe version, relative root, and invocation/manifest evidence bind
the operation exact-retry identity. This does not add a tenth MCP tool; MCP,
OpenAPI, and the capability registry still share the same nine-item source.
At finish, untracked interpreter/test caches, coverage output, and
`node_modules` are omitted with bounded warnings; tracked paths are retained.

After review, the human accepts or rejects on the host:

```bash
webcodex task show <task-id>
webcodex task accept <task-id>
# or: webcodex task reject <task-id>
```

## First Safe Prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

No project discovery or runtime identifier belongs in this prompt.

## Common Errors

| Code | Meaning | Action |
|---|---|---|
| `project_not_configured` | No canonical setup exists | Run `webcodex setup` |
| `project_registration_invalid` | Local project state is malformed, incomplete, or conflicting | Resolve the reported private-state conflict |
| `project_credential_invalid` | The private Project Credential is missing, unsafe, malformed, or mismatched | Restore both matching private files or explicitly recreate the profile |
| `project_credential_rejected` | The reachable server rejected the configured Project Credential | Restore the server-matching credential; do not treat this as Agent offline |
| `workspace_unavailable` | The configured Git workspace is unavailable | Restore the workspace, then run doctor |
| `server_unreachable` | The project runtime is unavailable | Run `webcodex agent start` |
| `agent_offline` | The local Agent is not ready | Run `webcodex doctor` |
| `required_capability_unavailable` | The Agent lacks a coding capability | Upgrade all binaries |
| `structured_validation_unavailable` | The Agent cannot run structured checks | Upgrade all binaries |
| `task_not_active` | The task can no longer mutate or execute | Start a new task |
| `execution_not_terminal` | Finish is blocked by active/unknown work | Review/wait/cancel |
| `validation_recipe_not_found` / `validation_recipe_ambiguous` | Auto resolution found no recipe or multiple nearest recipes | Change `cwd` or provide a matching `recipe` |
| `validation_recipe_mismatch` / `validation_manifest_invalid` | Explicit recipe, path, marker, or manifest evidence is invalid | Correct the reported public evidence |
| `validation_check_unavailable` / `test_filter_unsupported` | No safe mapping exists for the requested semantic input | Change the check/filter |
| `package_manager_ambiguous` | Node package-manager evidence is absent or conflicting | Correct `packageManager` or lockfiles |
| `validation_tool_unavailable` | The selected executable/module is absent | Provide the existing project tool and use a new operation ID |
| `checks_required` | A normal task has not run checks | Call `checks_run` |
| `checks_stale` | The workspace changed after the last check | Run a new check |

Run `webcodex status` for the short answer and `webcodex doctor` for full
read-only findings.

## Advanced Runtime Surface

WebCodex can also run as a multi-project management ToolRuntime. Its discovery,
session, LSP, raw job, artifact, and operator tools remain documented in
[OPERATIONS.md](OPERATIONS.md). That is an advanced surface, not the canonical
project Connector and not a prerequisite for ordinary coding.
