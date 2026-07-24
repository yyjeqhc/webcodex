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
| `checks_required` | A normal result has not run checks | Run `checks_run`, then finish |
| `checks_stale` | The workspace changed after the last trusted check | Run a new check operation |

Advanced server, enrollment, OAuth, transport, and fleet diagnostics remain in
`webcodex-cli` and the operations documentation. They are not onboarding
steps.
