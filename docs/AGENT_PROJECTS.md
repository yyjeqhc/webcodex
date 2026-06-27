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

`runtime_status` and `listAgents` show project summaries and redacted policy summaries. They do not expose tokens, env values, `Authorization` headers, full `agent.toml`, or shell `init_script` values.

## Troubleshooting

If `createProject` or `registerProject` returns a policy error, check whether the requested path is under the agent's effective `allowed_roots`.

If a new project does not appear in `listProjects`, verify the agent is online and that its project registry refresh succeeded.
