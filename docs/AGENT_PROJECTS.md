# Agent Project Registry

Private Drop now supports an agent-owned project registry for local projects.
The agent is the source of truth for local project metadata. The server only
caches summaries reported by each `private-drop-agent` client.

## Default Location

By default, the agent scans:

```bash
~/.config/private-drop-agent/projects.d/*.toml
```

An agent config may override this path:

```toml
projects_dir = "/root/.config/private-drop-agent/projects.d"
```

Each file describes one local project:

```toml
id = "private-drop"
path = "/root/git/private-drop"
name = "private-drop"
kind = "rust"
description = "Private Drop server and agent"
disabled = false

[hooks]
doctor = [
  "git status --short",
  "git log --oneline -5"
]

precommit = [
  ". /root/.cargo/env && cargo fmt --check",
  ". /root/.cargo/env && cargo test"
]
```

`id` and `path` are required. `id` must contain only ASCII letters, digits,
`-`, `_`, and `.`. Hook names are reported to the server, but hook commands
remain local to the agent and are not managed by the server. Files with
`disabled = true` are skipped and not reported.

## Reporting

The agent reports project summaries during registration and during poll
requests. It rescans the projects directory through a short local cache, so a
new `projects.d/foo.toml` file should appear on the server after the next poll
cycle plus the cache interval.

The server cache includes project id, name, path, kind, description, hook names,
disabled flag, best-effort git branch/head/dirty state, and `updated_at`.

## Creating Projects Through The Agent

Use `pdctl.py new` to ask a specific agent to create a project locally:

```bash
python3 scripts/pdctl.py new oe foo /root/work/foo --template rust --git-init
python3 scripts/pdctl.py new oe notes /root/work/notes --template docs
python3 scripts/pdctl.py new oe py-demo /root/work/py-demo --template python --allow-existing
```

The server checks that the caller may operate the target `client_id`, then
queues a structured `create_project` request for the agent. The agent creates
the directory, writes template files, optionally runs `git init`, and writes:

```bash
~/.config/private-drop-agent/projects.d/<project-id>.toml
```

The generated `projects.d` file is the source of truth. The server only caches
the summary returned by the agent and summaries reported in later polls. Hook
commands remain in the agent-local TOML file; the server only sees hook names.

Verify the new project after the agent reports it:

```bash
python3 scripts/pdctl.py projects oe
```

Manual creation is still possible:

1. Create the project directory on the agent host.
2. Write `~/.config/private-drop-agent/projects.d/foo.toml`.
3. Wait for the agent to poll and report the new summary.
4. Run `python3 scripts/pdctl.py projects oe` to confirm it is visible.

`projects.toml` remains supported on the server for existing project workflow,
doctor, and hook APIs.

## Workflow Status

`python3 scripts/pdctl.py snapshot <project>` and the server
`project_workflow`, `project_doctor`, and `project_hook` APIs still use server
`projects.toml` ProjectConfig entries. Agent-owned project discovery and
creation are available now; running workflows directly by agent project id is a
later phase.

## Current Limits

- Create project on an agent: available through `pdctl.py new` and
  `POST /api/shell/projects/create`.
- Delete project: not implemented.
- Run workflow by agent project id: not implemented yet.
- Path access is controlled by the local agent policy.
