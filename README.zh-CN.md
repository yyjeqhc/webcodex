# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex 是一个自托管 runtime，用于让 ChatGPT GPT Actions 和 MCP 客户端通过受控服务器与本地执行 agent 操作私有代码。

WebCodex 面向希望让 AI 助手检查仓库、编辑文件、运行 Git/测试/构建命令，并可选启动 Codex CLI 工作流的开发者和团队；项目执行仍保留在你控制的机器上，而不是交给托管黑盒。

## 为什么需要它

大多数 AI 编码集成都要在便利性和控制权之间做取舍：

| 常见方式 | 问题 |
| --- | --- |
| HTTP 端点后面挂临时脚本 | 难以发现、审计、限定权限，也难以安全复用。 |
| 仅本地 MCP server | 适合桌面客户端，但不足以支持 ChatGPT GPT Actions 或远程工作流。 |
| 给笔记本临时开 tunnel | URL 易变，生命周期控制弱，客户端反复重配很麻烦。 |
| 托管 coding agent | 使用方便，但项目执行会离开你的机器或可信主机。 |

WebCodex 提供一个稳定的远程入口，同时把真实仓库和命令执行留在你控制的机器上。

## 工作方式

```text
ChatGPT GPT Action / MCP client
        |
        | HTTPS + wc_pat_xxx
        v
WebCodex server
        |
        | agent transport + wc_agent_xxx
        v
webcodex-agent
        |
        v
registered project directory
```

服务器暴露 GPT Actions、MCP 和 runtime API。agent 反向连接服务器，并在已注册的项目目录内执行允许的工作。GPT Actions 和 MCP 使用个人 API token；agent 使用绑定到 `client_id` 的独立 agent token。

## 能做什么

- 向 ChatGPT GPT Actions 暴露受控项目工具。
- 通过 MCP endpoint 暴露同一套 runtime。
- 把工具调用路由到已连接的 agent，而不是让服务器直接读取私有项目路径。
- 读取文件、列出文件、搜索文本、检查 Git 状态/diff、校验/应用 patch，并运行受限的项目命令。
- 已知目标行号时，优先使用 `replace_line_range`、`insert_at_line` 和 `delete_line_range` 做结构化源码编辑。
- 在配置完成后，通过结构化 Cargo 工具运行 Rust 相关检查。
- 在 agent 机器已经安装并认证 Codex CLI 时，可选启动 Codex CLI job。
- 区分管理员、账户开通、GPT/MCP token 和 agent token 的凭据边界。

## WebCodex 不是什么

- 它不是托管代码运行器。项目执行由你自己的机器或服务器上的 agent 完成。
- 它不是裸 tunnel 替代品。服务器保留稳定的 GPT/MCP-facing API，并施加自己的认证和工具边界。
- 它不是把 root/admin 凭据放进 GPT Actions 的理由。GPT Actions 和 MCP 只应使用 `wc_pat_xxx`。
- 它目前不是完整的外部 MCP marketplace。当前 runtime 暴露 WebCodex 工具；任意外部 MCP server 的 broker 式注册属于后续工作。

## 当前状态

| 能力 | 状态 |
| --- | --- |
| GPT Actions runtime tools | 可用；使用 `/openapi.json` 和 Bearer/API-key 认证。 |
| MCP endpoint | 可用；与 GPT Actions 使用同一个 `ToolRuntime`。 |
| agent-backed project registry | 可用；项目 id 格式为 `agent:<client_id>:<project_id>`。 |
| 结构化行编辑 | 可用；已知目标行号时是推荐的局部源码编辑方式。 |
| Git/file/patch/shell/Cargo tools | 可用；shell 执行应保持受限并限定在项目内。 |
| Codex CLI job launcher | 可选；需要 agent 机器上已有 Codex CLI。 |
| Release artifacts | v0.1.0 包含 `linux-x64`、`linux-arm64` 和 `darwin-arm64`。 |
| Windows 和 `darwin-x64` binaries | v0.1.0 release artifacts 暂不包含。 |

## 快速开始

这是从零到可用私有项目 runtime 的最短路径。生产部署细节、service 文件、反向代理配置以及完整 sg4 smoke 记录见 [docs/DEPLOYMENT.zh-CN.md](docs/DEPLOYMENT.zh-CN.md) / [English](docs/DEPLOYMENT.md) 和 [docs/smoke-test-sg4.md](docs/smoke-test-sg4.md)。

### 1. 安装

```bash
npm install -g @yyjeqhc/webcodex
```

也可以从项目 release artifacts 下载平台二进制文件。

### 2. 启动服务器

```bash
webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env

WEBCODEX_ENV_FILE=/etc/webcodex/webcodex.env webcodex
```

在连接 GPT Actions 或远程 agent 之前，请先把服务器放到你自己的 HTTPS 域名后面。

### 3. 创建用户和账户凭据

```bash
webcodex-cli users create \
  --server-url https://your-domain.example \
  --token "$WEBCODEX_TOKEN" \
  --username alice \
  --display-name "Alice" \
  --role user \
  --issue-credential
```

这会签发一次性的 `wc_acct_xxx` 账户凭据，用于本地创建 token。它不是 GPT/MCP token，也不是 agent token。

### 4. 用户为 GPT Actions、MCP 和 runtime API 创建 PAT

```bash
webcodex-cli token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

把生成的 `wc_pat_xxx` 作为 GPT Actions 和 MCP 客户端中的 bearer/API-key 值。

### 5. 用户创建 agent token

```bash
webcodex-cli agent-token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --client-id alice-laptop \
  --name alice-laptop
```

生成的 `wc_agent_xxx` 只用于 `webcodex-agent`。

### 6. 初始化 agent

```bash
webcodex-agent init \
  --server-url https://your-domain.example \
  --token "$WEBCODEX_AGENT_TOKEN" \
  --client-id alice-laptop \
  --owner alice \
  --display-name "Alice Laptop" \
  --transport websocket \
  --projects-dir ~/.config/webcodex/projects.d \
  --allowed-root ~/git \
  --output ~/.config/webcodex/agent.toml \
  --overwrite
```

### 7. 注册项目

在 agent 机器上创建 `~/.config/webcodex/projects.d/my-repo.toml`：

```toml
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true

[hooks]
status = ["git status --short"]
check = ["cargo check --all-targets"]
```

然后启动 agent：

```bash
webcodex-agent --config ~/.config/webcodex/agent.toml
```

runtime 项目 id 使用这种格式：

```text
agent:<client_id>:<project_id>
```

例如：`agent:alice-laptop:my-repo`。

### 8. 测试 runtime tool list

```bash
curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

## 创建自己的 GPT

GPT Actions 是使用 WebCodex 的主要场景之一：你的 GPT 获得的是结构化、带 scope 的 runtime，而不是一堆临时脚本。

1. 在 ChatGPT 中创建 GPT。
2. 添加 Action。
3. 从 `https://your-domain.example/openapi.json` 导入 OpenAPI schema。
4. 在 GPT Action 设置中配置 Bearer/API key 认证。
5. 使用 `wc_pat_xxx` 个人 API token。不要使用 `WEBCODEX_TOKEN`、`wc_acct_xxx` 或 `wc_agent_xxx`。
6. 对已注册项目测试 `listRuntimeTools` 和 `callRuntimeTool`，例如 `agent:alice-laptop:my-repo`。

完整 GPT Action 设置指南和支持的工具面见 [docs/GPT_ACTIONS.md](docs/GPT_ACTIONS.md)。

## 配合 MCP 使用

WebCodex 暴露一个远程 MCP endpoint，背后使用与 GPT Actions 相同的 runtime。

- Endpoint: `https://your-domain.example/mcp`
- Auth: Bearer `wc_pat_xxx`
- Runtime: 与 GPT Actions 相同的 `ToolRuntime`
- Project ids: `agent:<client_id>:<project_id>`
- Token boundary: MCP 不要使用 `WEBCODEX_TOKEN`、`wc_acct_xxx` 或 `wc_agent_xxx`

客户端配置示例和排障见 [docs/MCP.md](docs/MCP.md)。

## 凭据模型

| Credential | 使用方 | 用途 | 不要用于 |
| --- | --- | --- | --- |
| `WEBCODEX_TOKEN` | server admin | bootstrap/root admin | GPT/MCP/agent daily use |
| `wc_acct_xxx` | user CLI | create local PAT/agent token | GPT/MCP/agent |
| `wc_pat_xxx` | GPT Action/MCP/API | runtime tools | agent connection |
| `wc_agent_xxx` | `webcodex-agent` | connect agent to server | GPT/MCP/runtime API |

服务器只保存用户创建的 PAT 和 agent token 的 hash。完整凭据模型见 [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md)。

## 文档

- 安装与部署：[docs/DEPLOYMENT.zh-CN.md](docs/DEPLOYMENT.zh-CN.md) / [English](docs/DEPLOYMENT.md)
- 创建 GPT Action：[docs/GPT_ACTIONS.zh-CN.md](docs/GPT_ACTIONS.zh-CN.md) / [English](docs/GPT_ACTIONS.md)
- 快速开始：[docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md) / [English](docs/QUICK_START.md)
- 配合 MCP 使用：[docs/MCP.zh-CN.md](docs/MCP.zh-CN.md) / [English](docs/MCP.md)
- 凭据模型：[docs/AUTH_MODEL.zh-CN.md](docs/AUTH_MODEL.zh-CN.md) / [English](docs/AUTH_MODEL.md)
- Agent projects：[docs/AGENT_PROJECTS.zh-CN.md](docs/AGENT_PROJECTS.zh-CN.md) / [English](docs/AGENT_PROJECTS.md)
- Agent transports：[docs/AGENT_TRANSPORTS.zh-CN.md](docs/AGENT_TRANSPORTS.zh-CN.md) / [English](docs/AGENT_TRANSPORTS.md)
- Shell profiles：[docs/SHELL_PROFILES.zh-CN.md](docs/SHELL_PROFILES.zh-CN.md) / [English](docs/SHELL_PROFILES.md)
- 排障：[docs/TROUBLESHOOTING.zh-CN.md](docs/TROUBLESHOOTING.zh-CN.md) / [English](docs/TROUBLESHOOTING.md)
- 完整文档索引：[docs/INDEX.zh-CN.md](docs/INDEX.zh-CN.md) / [English](docs/INDEX.md)

## 安全提示

- 连接 GPT Actions、MCP 客户端或远程 agent 前，请先把服务器放到 HTTPS 后面。
- `WEBCODEX_TOKEN` 保持在服务器侧。它是 bootstrap/admin 凭据，不是集成 token。
- 每个 GPT Action、MCP 客户端或自动化入口最好使用独立的 `wc_pat_xxx`。
- 每个 agent `client_id` 最好使用独立的 `wc_agent_xxx`。
- 优先使用结构化文件编辑工具，再退回 shell 编辑。
- 暴露公网服务器前，请阅读 [SECURITY.md](SECURITY.md)。

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
