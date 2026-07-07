# Agent Project Registry

[English](AGENT_PROJECTS.md) | [简体中文](AGENT_PROJECTS.zh-CN.md)

An agent reports project registry entries to the server. GPT Actions and MCP then use ids like:

```text
agent:<client_id>:<project_id>
```

Projects are registered by agents, not by server-side projects.toml.

## Project registry files

Each agent has a `projects_dir` containing one project file per registered project. The server sees those entries through the connected agent registry.

A project entry contains a human name, an absolute path on the agent host, and policy flags such as `allow_patch`.

## Agent `projects.d/*.toml` format

Agent project files are one-file-per-project TOML files in the agent's configured `projects_dir`.

Correct agent `projects.d/webcodex.toml` format:

```toml
id = "webcodex"
path = "/srv/webcodex/projects/webcodex"
name = "WebCodex"
kind = "repo"
description = "WebCodex repository"
allow_patch = true

[hooks]
status = ["git status --short"]
fmt = ["cargo fmt"]
check = ["cargo check --all-targets"]
test = ["cargo test"]
```

Incorrect for agent `projects.d/*.toml`:

```toml
[projects.webcodex]
path = "/srv/webcodex/projects/webcodex"
```

That nested `[projects.webcodex]` shape belongs to an old server-side projects file format. In an agent `projects.d/*.toml` file it leaves the top-level `id` absent and will fail with `missing field id`. Use top-level `id` and `path` fields instead.


## Agent-side project management tools

Current project management tools:

- `register_project` / `registerProject`: register an existing directory.
- `create_project` / `createProject`: create a new directory, optionally initialize a template and git repo, and register it.

These tools are available through the runtime tool list, MCP tools/list, and dedicated GPT Actions. They are constrained by the selected agent's policy.

## Policy boundaries

`allowed_roots` controls where project paths may be registered or created.

- Missing or empty `allowed_roots` defaults to `$HOME`.
- Explicit `allowed_roots` overrides the `$HOME` default.
- Use explicit roots to narrow an agent to a known workspace tree.

Example narrow policy:

```toml
[policy]
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
```

This is only an example of a narrowed deployment, not the default.

## Observability

`runtime_status`, `listAgents`, and `listProjects` show project summaries, redacted policy summaries, and a sanitized `shell_profiles` summary. They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, the full env snapshot, or shell `init_script` bodies.

Each project in `listProjects` also carries `agent_status` (`online` / `stale`), `connected`, `last_seen`, `shell_profile` (the project's setting), `resolved_shell_profile` (the actually-used name), and `shell_profile_status` (`configured` / `missing` / `not_configured` / `unknown`).

## Agent-registered runtime surface

Runtime tool execution (`run_shell`, `apply_patch`, git, files, jobs, sessions)
uses **agent-registered** projects only. Use the id returned by `listProjects`,
for example `agent:<client_id>:<project_id>`.

If you see older docs or deployment prompts telling you to configure a
server-side `projects.toml`, that is legacy guidance and is not required for new
deployments.

## Troubleshooting

If `createProject` or `registerProject` returns a policy error, check whether the requested path is under the agent's effective `allowed_roots`.

If a new project does not appear in `listProjects`, verify the agent is online and that its project registry refresh succeeded.

For shell-profile diagnostics (missing profile, prepare failure, project
binding), run `webcodex-cli doctor --agent-config /etc/webcodex/clients/workstation/agent.toml
--strict` and see [SHELL_PROFILES.md](SHELL_PROFILES.md).
