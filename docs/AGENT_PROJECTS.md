# Agent Project Registry

An agent reports project registry entries to the server. GPT Actions and MCP then use ids like:

```text
agent:<client_id>:<project_id>
```

## Project registry files

Each agent has a `projects_dir` containing one project file per registered project. The server does not need a matching server-side project block for agent-backed projects.

A project entry contains a human name, an absolute path on the agent host, and policy flags such as `allow_patch`.

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

## Server-side `projects.toml` vs agent-registered projects

> The server-side `projects.toml` config is legacy/metadata only. Runtime tool
> execution (run_shell, apply_patch, git, …) uses **agent-registered**
> projects. A project listed only in the server-side `projects.toml` is **not**
> executable through the runtime surface; use an agent-registered id like
> `agent:<client_id>:<project_id>` from `listProjects`.

This is why a project may appear in `runtime_status` (`projects.configured =
true`) but still be rejected by tool calls with an "Unknown project" /
"projects.toml" error: the executable set comes from the connected agent's
registry, not the server-side file. If a project seems to flicker in and out of
`listProjects`, check the owning agent's liveness (`agent_status`, `connected`,
`last_seen`): a `stale` or disconnected agent's projects are listed but cannot
execute until the agent reconnects.

## Troubleshooting

If `createProject` or `registerProject` returns a policy error, check whether the requested path is under the agent's effective `allowed_roots`.

If a new project does not appear in `listProjects`, verify the agent is online and that its project registry refresh succeeded.

For shell-profile diagnostics (missing profile, prepare failure, project
binding), run `webcodex-cli doctor --agent-config /etc/webcodex/agent.toml
--strict` and see [SHELL_PROFILES.md](SHELL_PROFILES.md).
