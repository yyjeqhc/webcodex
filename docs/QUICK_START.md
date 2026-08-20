# Quick Start

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

This is the shortest path to a working WebCodex setup on one local Git project.
It uses the project-first flow: `webcodex setup` configures the current
directory without asking for client IDs, runtime project ids, transports, or
internal config paths.

For connecting to an existing hosted Server, see
[Deployment](DEPLOYMENT.md#connect-a-repository-to-an-existing-server). For
letting an AI agent do the setup, see [AI Onboarding](AI_ONBOARDING.md).

## Prerequisites

- All three binaries installed: `webcodex`, `webcodex-server`,
  `webcodex-runner`.
- Git available on `PATH`.
- A Git project you can safely inspect and edit.

Install the packaged build:

```bash
npm install -g @yyjeqhc/webcodex
```

Or build from this checkout:

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## 1. Set up the project

```bash
cd /path/to/your/repository
webcodex setup
```

On first run, `setup` resolves the Git top-level, creates private state outside
the checkout, creates the minimum project registration and Agent
configuration, and creates one Project Credential for this project's Connector
and Agent — without printing it. It leaves the Server and Agent stopped.

Run it again to verify idempotency; the second result is `already configured`.
If one generated component is missing, `setup` repairs only that component.

## 2. Check readiness

```bash
webcodex doctor
```

`doctor` is read-only. Before the Agent starts, its expected verdict is
`Needs action` with `Next: webcodex run`. Use `--json` for the structured
projection.

## 3. Start the local runtime

```bash
webcodex run
```

This starts the project-bound loopback Server and local Agent in the
foreground. Leave the terminal open; Ctrl-C stops both. In another terminal,
from the same project:

```bash
webcodex status
```

A ready project reports its Project, Connection, Agent, coding readiness, and
no next action.

## 4. Connect a client

### Temporarily share over HTTPS

Hosted clients cannot reach a loopback address. For temporary development or
testing access from a hosted MCP client, stop `webcodex run` and run:

```bash
webcodex share
```

`share` reuses the project setup and local runtime but starts a Cloudflare
Quick Tunnel and a separate temporary Connector credential. It prints a
temporary `https://*.trycloudflare.com/mcp` URL and Bearer token; both stop
being usable when the command exits.

For an MCP client that requires OAuth, register its exact callback and run:

```bash
webcodex share --auth oauth --oauth-redirect-uri https://client.example/callback
```

The command prints the temporary Project share credential plus a project-bound OAuth client ID/secret. Enter the Project share credential only on the WebCodex authorization page. OAuth grants are fenced to that `share` process, so a restart invalidates old access and refresh tokens. The client ID/secret remain in protected project state for reuse with the same callback.

`webcodex share --tunnel none` starts the same runtime without a public tunnel for local debugging. For a stable operator-managed OAuth origin, combine it with `--public-url https://share.example` and route that HTTPS origin to the local WebCodex port yourself. Quick Tunnels are not a stable-origin or production deployment mechanism.

### Connect to an existing Server

For a stable, long-lived setup against an existing Server, shared-key connect remains available:

```bash
webcodex connect https://webcodex.example --project .
```

For ChatGPT OAuth, use the same hosted shared-key identity; no login, pairing, PAT, or account identity is required:

```bash
webcodex connect https://webcodex.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback --project .
```

The Runner keeps the shared key while ChatGPT receives only OAuth credentials/tokens. Browser authorization asks for that key only on WebCodex's authorize page. OAuth preserves the same shared-key group and is capped to the direct shared-key model-facing scopes (`runtime/project/job`, `computer:read`, `computer:control`), never account/admin/Agent or broader Computer/future scopes. Advanced managed-user OAuth remains available as `--auth managed-oauth` after `webcodex login`. See [Deployment](DEPLOYMENT.md).

To remove only this repository from a hosted `connect` profile later, run from the repository:

```bash
webcodex disconnect
```

This unregisters the exact canonical repository and leaves the source tree, `.git`, profile
credential, and any other registered projects intact. If more than one hosted profile registers
the same repository, rerun with `--profile NAME`.

## 5. Run a coding task

Ask the client for a small, reversible change. The canonical calls are:

```text
task_start
→ files_list
→ files_read or files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

Use a stable `operation_id` for edits, commands, and checks: retrying the same
payload reuses the operation; a different payload under the same ID fails
closed.

`checks_run` accepts `format`, `check`, and `test` plus an optional `recipe`
(`rust`, `node`, `python`, `go`). Omit the recipe for automatic resolution
from the nearest manifest directory.

## 6. Review and accept locally

The coding result stays isolated from the target checkout until a human
decision:

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

Use `webcodex task reject <task-id>` to discard it. Acceptance verifies that
the target Git state still matches the task baseline before applying the
result. The online model can propose work but can never accept it.

You can also review in the browser: open `/console` and use the work queue.
Accept and Reject in the browser call the same authority the CLI uses.

## Troubleshooting

Start with:

```bash
webcodex status
webcodex doctor
```

Common stable codes and next actions:

| Code | Meaning | Next action |
| --- | --- | --- |
| `project_not_configured` | No setup for this project/profile | `webcodex setup` |
| `project_credential_invalid` | Private credential state is missing or mismatched | Restore both matching private files or recreate the profile |
| `server_unreachable` | Loopback runtime cannot be reached | `webcodex run` |
| `agent_offline` | Server reachable but the local Agent is unavailable | `webcodex run` |
| `required_capability_unavailable` | Installed Agent too old | Upgrade all binaries |
| `workspace_unavailable` | Git or project path unavailable | Restore the path/Git workspace |
| `checks_required` | Normal result has not run checks | Run `checks_run`, then finish |
| `checks_stale` | Workspace changed after the last check | Run a new check |

See [Troubleshooting](TROUBLESHOOTING.md) for the full operational checklist.
