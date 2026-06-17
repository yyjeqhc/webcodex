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

## Temporary Project Creation Flow

Until a full `createAgentProject` API exists:

1. Create the project directory on the agent host.
2. Write `~/.config/private-drop-agent/projects.d/foo.toml`.
3. Wait for the agent to poll and report the new summary.
4. Run `python3 scripts/pdctl.py projects oe` to confirm it is visible.

`projects.toml` remains supported on the server for existing project workflow,
doctor, and hook APIs.
