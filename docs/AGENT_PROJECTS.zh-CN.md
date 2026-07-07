# Agent Project Registry

[English](AGENT_PROJECTS.md) | [简体中文](AGENT_PROJECTS.zh-CN.md)

Agent 会把 project registry entries 报告给 server。GPT Actions 和 MCP 使用如下形式的 id：

```text
agent:<client_id>:<project_id>
```

## Project registry files

每个 agent 都有一个 `projects_dir`，其中每个已注册项目对应一个项目文件。对于 agent-backed projects，server 不需要匹配的 server-side project block。

项目 entry 包含人类可读名称、agent host 上的绝对路径，以及 `allow_patch` 等 policy flags。

## Agent `projects.d/*.toml` 格式

Agent project files 是 agent 配置的 `projects_dir` 中的一项目一文件 TOML。它们**不是** server-side `projects.toml` 的格式。

正确的 agent `projects.d/webcodex.toml` 格式：

```toml
id = "webcodex"
path = "/root/git/private-drop"
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

错误的 agent `projects.d/*.toml` 格式：

```toml
[projects.webcodex]
path = "/root/git/private-drop"
```

这种 nested `[projects.webcodex]` 形状属于 legacy server-side `projects.toml`。如果写进 agent `projects.d/*.toml`，顶层 `id` 会缺失，并报 `missing field id`。请使用顶层 `id` 和 `path` 字段。

## Agent-side project management tools

当前 project management tools：

- `register_project` / `registerProject`：注册现有目录。
- `create_project` / `createProject`：创建新目录，可选初始化 template 和 git repo，并注册它。

这些工具可通过 runtime tool list、MCP tools/list 和 dedicated GPT Actions 使用，并受所选 agent policy 约束。

## Policy boundaries

`allowed_roots` 控制哪些 project paths 可以被注册或创建。

- 缺失或为空的 `allowed_roots` 默认使用 `$HOME`。
- 显式 `allowed_roots` 会覆盖 `$HOME` 默认值。
- 如需收窄 agent 到固定 workspace tree，请使用显式 roots。

示例窄权限 policy：

```toml
[policy]
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
```

这只是 narrowed deployment 示例，不是默认值。

## 可观测性

`runtime_status`、`listAgents` 和 `listProjects` 显示 project summaries、redacted policy summaries 和 sanitized `shell_profiles` summary。它们不会暴露 tokens、env values、`Authorization` headers、完整 `agent.toml`、完整 env snapshot 或 shell `init_script` bodies。

`listProjects` 中的每个项目还包含 `agent_status`、`connected`、`last_seen`、`shell_profile`、`resolved_shell_profile` 和 `shell_profile_status`。

## Server-side `projects.toml` 与 agent-registered projects

Server-side `projects.toml` config 是 legacy/metadata only。Runtime tool execution（`run_shell`、`apply_patch`、git 等）使用 **agent-registered** projects。只存在于 server-side `projects.toml` 的项目不能通过 runtime surface 执行；请使用 `listProjects` 返回的 `agent:<client_id>:<project_id>`。

如果某个项目在 `runtime_status` 中出现，但 tool call 报 “Unknown project” / “projects.toml” 相关错误，通常说明可执行项目集来自 connected agent registry，而不是 server-side file。

如果 `runtime_status.projects.server_static.status = "not_configured"`，并且
message 是 `projects.toml not configured; using agent-registered projects`，
意思只是这个可选 metadata 文件不存在。Agent-only deployment 中这是正常状态。
如果 operator 仍然想保留 server-side metadata file，可以设置
`PROJECTS_CONFIG=/path/to/projects.toml`，并使用一个很小的内联格式：

```toml
[projects.webcodex]
path = "/root/git/private-drop"
executor = "agent"
client_id = "workstation"
allow_patch = true
```

这个文件可以让 server-static 状态不再显示 not_configured，也可兼容 legacy
metadata path；但 runtime tools 仍应使用 `listProjects` 返回的
`agent:<client_id>:<project_id>` id。

## Troubleshooting

如果 `createProject` 或 `registerProject` 返回 policy error，请确认请求路径在 agent effective `allowed_roots` 下。

如果新项目没有出现在 `listProjects` 中，请确认 agent 在线，并且 project registry refresh 成功。

Shell-profile diagnostics 见 [SHELL_PROFILES.md](SHELL_PROFILES.md) / [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md)。
