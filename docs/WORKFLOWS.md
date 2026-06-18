# Project Workflows

Project workflows are a small convenience layer over project state checks and
configured hook commands.

- `doctor`: collect project status, git state, hooks, and recent jobs.
- `hook`: run a named hook from `projects.toml`.
- `workflow`: collect before/after git evidence and optionally run doctor or a hook.

Workflow does not stage, commit, push, deploy, or delete files.

There are two workflow families:

1. Server project workflow: project name comes from server `projects.toml` and
   the endpoint is `/api/codex/project_workflow`.
2. Agent-native project workflow: `client_id` and `project_id` come from the
   agent registry under `~/.config/private-drop-agent/projects.d/`, and the
   endpoint is `/api/shell/projects/workflow`.

For longer-running agent work, prefer async jobs:

1. `createShellClientShellJob` for a shell command.
2. `createShellClientProjectWorkflowJob` for a project workflow.
3. Poll `getShellClientJobStatus` or `getShellClientJobLog`.
4. Use `stopShellClientJob` for best-effort cancellation.

Async jobs are the right path when a GPT Action request should return
immediately instead of waiting on the agent. The job id is the only isolation
token; there is no session, cookie, or window affinity.

New GPT Action windows should prefer agent-native workflow after listing
clients and projects. The server still owns auth, owner checks, audit, request
forwarding, and result cache updates; the agent reads local project config and
executes only the structured request.

## Environment

```bash
export PRIVATE_DROP_URL="https://example.com"
export PRIVATE_DROP_TOKEN="..."
```

`scripts/pdctl.py` also accepts `DROP_URL` and `DROP_TOKEN`. The
`PRIVATE_DROP_*` variables take precedence.

## Daily Commands

Check project state:

```bash
python3 scripts/pdctl.py doctor private-drop
```

Run a git evidence snapshot:

```bash
python3 scripts/pdctl.py snapshot private-drop
```

Run the configured precommit hook:

```bash
python3 scripts/pdctl.py precommit private-drop
```

Run a named hook:

```bash
python3 scripts/pdctl.py hook private-drop doctor
```

Run the same kinds of workflow directly against an agent-owned project:

```bash
python3 scripts/pdctl.py agent-snapshot oe foo
python3 scripts/pdctl.py agent-precommit oe foo
python3 scripts/pdctl.py agent-hook oe foo doctor
```

Print the full API response:

```bash
python3 scripts/pdctl.py workflow private-drop --mode snapshot --json
```

## Exit Codes

- `0`: API response reported `success=true`.
- `1`: workflow, doctor, or hook ran but reported `success=false`.
- `2`: local config, HTTP, network, or response parsing error.

## Hook Configuration

Hooks live under each project in `projects.toml`:

```toml
[projects.private-drop]
path = "/root/git/private-drop"
executor = "agent"
client_id = "oe"

[projects.private-drop.hooks]
doctor = [
  "git status --short",
  "git log --oneline -5"
]
precommit = [
  ". /root/.cargo/env && cargo fmt --check",
  ". /root/.cargo/env && cargo test"
]
```

See `examples/projects.toml.example` for Rust, Python, and Android/Gradle
examples. Hooks are project-defined command lists, not a fixed Cargo workflow.

## SSH Disabled Behavior

SSH execution is disabled by default. For SSH projects, workflow and hook calls
do not execute remote commands unless `DROP_ENABLE_SSH=true` is set. The API
returns this warning:

```text
SSH executor is disabled; use agent executor
```

Prefer the agent executor for routine workflow and hook execution.

## Troubleshooting

Agent offline:
Run `doctor` or `workflow --mode snapshot --json` and check `warnings` for
`agent client ... not found` or `agent client ... is not connected`.

Missing hook:
The response error names the hook, for example
`project hook 'precommit' is not configured`. Add the hook under
`[projects.<name>.hooks]` or run a configured hook.

Dirty workspace:
Workflow responses include `git_before`, `git_after`, `dirty`,
`status_short`, `diff_stat`, and `changed_files`. Review the diff before
committing.

Hook failed:
The response includes `hook_result.steps`, the failing command, exit code, and
stdout/stderr tails. Fix the failing step and rerun the same hook.

Agent-native workflow hook commands run with cwd set to the agent project path.
`timeout_secs` is applied per hook command. The first failing hook command stops
the remaining steps, and git snapshots are still returned before and after.

Multiple async jobs may touch the same project directory at once. Private Drop
does not serialize them with a project lock in this release.

No git repo:
Git evidence reports `available=false` or an `error` field. The workflow still
returns a structured response instead of panicking.

## Non-Goals

Workflow does not:

- run `git add`
- run `git commit`
- run `git push`
- deploy
- delete files
- call any LLM API
