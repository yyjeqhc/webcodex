# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This is the canonical project-first path. It configures one local Git project
without asking you for an Agent client ID, runtime project ID, transport,
workflow session, executor reference, or internal config path.

## Prerequisites

- All three WebCodex binaries installed (`webcodex`, `webcodex-cli`,
  `webcodex-agent`).
- Git available on `PATH`.
- A Git project you can safely inspect and edit.

Install the packaged Linux x64 build:

```bash
npm install -g @yyjeqhc/webcodex
```

Or build from this checkout:

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

## 1. Set Up the Project

Change to the Git project and run:

```bash
webcodex setup
```

On the first run, setup:

- resolves the Git top-level directory;
- creates private state outside the checkout;
- creates the minimum project registration and Agent configuration;
- creates one exact Project Credential for this project's Connector and Agent
  without printing it;
- leaves the server and Agent stopped.

It does not edit project files, modify Git, start a service, change shell
configuration, open a network port, or upload source.

Run the same command again to verify idempotency:

```bash
webcodex setup
```

The second result is `already configured`. If one generated component is
missing, setup repairs only that component. If an existing field conflicts with
the current Git root or profile, setup stops and names the conflicting field;
it never overwrites the existing configuration.

The Connector credential file and the Agent configuration contain the same
secret and map to one stable, non-secret project grant identity. Both files are
owner-only private state; plaintext is not written to the database. Runtime
verification hashes the candidate and compares it in constant time. This path
is separate from ordinary shared-key quick start: unknown Bearer values are
rejected in project mode.

Setup never rotates a surviving credential silently. If the credential is
lost, restore both matching private files. If it is unrecoverable, stop the
runtime, intentionally retire the entire private project-state profile, and
run setup again; that explicit recreation also retires its local task and
execution history. Iteration 8.0 has no in-place rotate subcommand.

## 2. Diagnose the Next Step

```bash
webcodex doctor
```

Doctor is read-only. Before the Agent starts, its expected verdict is `Needs
action` with:

```text
Next:
  webcodex agent start
```

Each finding has a stable `name`, `status`, `code`, `summary`, and
`next_action`. Use `webcodex doctor --json` for the structured projection.

## 3. Start the Local Runtime

```bash
webcodex agent start
```

This explicit foreground action starts the project-bound loopback server and
local Agent. It does not install a service. Leave the terminal open; Ctrl-C
stops both processes. Loopback does not bypass authentication: only the exact
configured Project Credential can reach this project's Connector and Agent.

In another terminal, from the same project:

```bash
webcodex status
```

A ready project reports its Project, Connection, Agent, coding readiness, and
no next action. For full checks, run `webcodex doctor` again.

## 4. Use the Project-Bound Connector

The Connector profile created for this project binds one logical project to one
registered executor deterministically. A local MCP/OpenAPI client using that
approved connection and exact credential can begin directly with:

```text
task_start
```

It does not need to call `list_projects`, `runtime_status`, `tool_manifest`,
`start_session`, or `current_session`, and it does not put an
`agent:<client>:<project>` value in the prompt.

Hosted ChatGPT cannot reach a loopback address. An operator must provide an
approved HTTPS endpoint and authentication without changing the project
binding. See [DEPLOYMENT.md](DEPLOYMENT.md), [MCP.md](MCP.md), or
[GPT_ACTIONS.md](GPT_ACTIONS.md). Setup deliberately does not create a tunnel
or expose a port.

## 5. Run the Golden Coding Path

Ask the client for a small, reversible change. The canonical calls are:

```text
task_start
→ files_read or files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

Use `operation_id` for edits, commands, and checks. It provides exact retry
identity: retrying the same payload reuses the operation; a different payload
under the same ID fails closed.

Normal writable tasks cannot finish without a structured check. A check that
runs and exits non-zero is a project assertion failure. A check that cannot
spawn is an executor/infrastructure failure and does not create assertion
evidence or trusted workspace provenance.

### Project-aware validation recipes

`checks_run` accepts the existing `format`, `check`, and `test` semantic names
plus an optional `recipe` enum: `rust`, `node`, `python`, or `go`. Omit
`recipe` for auto resolution—there is no `auto` alias. Resolution starts at
the relative `cwd` inside the Task execution workspace, walks only toward that
workspace root, and picks the nearest manifest directory. Multiple supported
markers in that directory are ambiguous; an explicit matching recipe resolves
the ambiguity. A mismatched marker, missing manifest, absolute/parent path, or
symlink escape is rejected before an Execution is reserved.

| Recipe | Marker | `format` | `check` | `test` |
|---|---|---|---|---|
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` plus one safe argv filter |
| Node | `package.json` | first of `format:check`, `format-check`, `check:format` | first of `check`, `typecheck`, `lint` | exact `test` |
| Python | `pyproject.toml` | configured Ruff, otherwise Black | configured Ruff, otherwise Mypy | configured pytest |
| Go | `go.mod` | unavailable | `go vet ./...` | `go test ./...` |

Node selects a package manager from a valid `packageManager` declaration or
one unambiguous supported lockfile (`pnpm-lock.yaml`, `yarn.lock`,
`package-lock.json`, `npm-shrinkwrap.json`, `bun.lock`, or `bun.lockb`).
Conflicting or absent evidence fails closed; a selected script is invoked only
as `<manager> run --silent <allowlisted-name>`. Script bodies are never copied
into the plan or error. Python enables only tools evidenced by
`pyproject.toml`; Ruff wins over Black for format and over Mypy for check.

Recipes do not install dependencies, run install hooks, generate
configuration, create environments, modify lockfiles, or use the network.
Only Rust accepts `test_filter`, as one argv value; every other recipe rejects
it instead of silently running all tests. Missing executables or Python
modules produce an executor failure with no failed check or assertion
evidence. A real process exit with a non-zero validation verdict is an
assertion failure.

The durable plan records recipe ID/version, relative root, semantic checks,
tool identities, and invocation/manifest evidence digests. They participate in
the request hash, so one `operation_id` reuses only the exact resolved plan.
A recipe binary change conflicts with an old operation ID; use a new ID to
resolve under the new recipe. Manifest, lockfile, or workspace changes make
successful provenance stale.

## 6. Review and Accept Locally

The coding result stays isolated from the target checkout until a human
decision:

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

Use `webcodex task reject <task-id>` to discard it. Acceptance verifies that
the target Git state still matches the task baseline before applying the
result.

## Browser Readiness

When the local runtime is running, `/console` shows only:

- current Project;
- Connection;
- Agent readiness;
- coding capability readiness;
- structured findings and the next CLI action.

It consumes the same application readiness facts as doctor/status. It does not
show the Agent registry, client IDs, transport implementation, queue IDs,
tokens, or a browser editor/terminal.

## Troubleshooting

Always start with:

```bash
webcodex status
webcodex doctor
```

Common stable codes:

| Code | Meaning | Next action |
|---|---|---|
| `project_not_configured` | No setup belongs to this Git project/profile | `webcodex setup` |
| `project_registration_invalid` | Existing state conflicts or is incomplete | Resolve the named field, then rerun setup |
| `project_credential_invalid` | Private credential state is missing, unreadable, unsafe, malformed, or mismatched | Restore both matching private files or explicitly recreate the profile |
| `project_credential_rejected` | The server rejected the locally configured credential | Restore the matching credential; do not treat this as Agent offline |
| `server_unreachable` | The loopback runtime cannot be reached | `webcodex agent start`, or inspect doctor |
| `agent_offline` | Server is reachable but the local Agent is unavailable | `webcodex agent start` |
| `required_capability_unavailable` | The installed Agent is too old/incomplete | Upgrade all WebCodex binaries |
| `structured_validation_unavailable` | The Agent lacks structured validation | Upgrade all WebCodex binaries |
| `workspace_unavailable` | Git or the configured project path is unavailable | Restore the path/Git workspace |
| `validation_recipe_not_found` | No supported marker exists from `cwd` to the Task root | Choose a manifest-bearing `cwd` |
| `validation_recipe_ambiguous` | The nearest root has multiple supported markers | Provide the matching explicit `recipe` |
| `validation_recipe_mismatch` / `validation_manifest_invalid` | Recipe, marker, safe path, or manifest evidence is invalid | Correct the reported public evidence |
| `validation_check_unavailable` / `test_filter_unsupported` | The recipe cannot safely map the requested check/filter | Change checks/filter or choose the matching recipe |
| `package_manager_ambiguous` | Node package-manager evidence is absent or conflicting | Correct `packageManager` or lockfiles |
| `validation_tool_unavailable` | The selected executable/module is not available on the Agent host | Provide the project's existing tool, then use a new operation ID |
| `checks_required` | A normal result has not run checks | Run `checks_run`, then finish |
| `checks_stale` | The workspace changed after the last trusted check | Run a new check operation |

Advanced server, enrollment, OAuth, transport, and fleet diagnostics remain in
`webcodex-cli` and the operations documentation. They are not onboarding
steps.
