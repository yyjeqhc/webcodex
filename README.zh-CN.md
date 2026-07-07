# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

把 ChatGPT 接到仍然留在你机器上的私有代码。

WebCodex 是一个面向 MCP 和 GPT Actions 的自托管代码桥接层。它让线上模型可以查看真实仓库、做局部修改、运行验证，并返回紧凑的任务总结；同时不把裸文件系统权限交给模型，也不需要把仓库搬到托管式 AI 编程服务。

- 让 ChatGPT、Custom GPT 或其他支持 MCP 的客户端操作真实的本地/私有主机仓库。
- 通过长驻 WebCodex agent，只注册你明确允许的项目目录。
- 优先使用结构化读取、编辑、diff 和验证工具，再把 shell 作为受限兜底能力。
- 代码执行留在 agent 所在机器；server 负责认证、权限、会话记录和客户端协议。
- 任务收口时给出 changed files、validation results、workspace hygiene 和交接信息，接入你平时的 Git 检查流程。

```text
ChatGPT / MCP clients / Custom GPTs
        |
        | MCP / GPT Actions
        v
WebCodex Server
        |
        | authenticated agent bridge
        v
WebCodex Agent
        |
        v
Private Codebase / Git / Tests / Shell
```

## 安装

当前 Linux x64 release 可以直接安装：

```bash
npm install -g @yyjeqhc/webcodex
```

也可以从源码构建：

```bash
cargo build --release --bins
```

平台支持和安装细节见 [docs/BUILD_INSTALL.zh-CN.md](docs/BUILD_INSTALL.zh-CN.md)。

## 快速开始

下面命令假设 npm 安装后的 binaries 已经在 `PATH` 中。

终端 1 - 生成一个评估用 key 并启动 server：

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"

webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080

set -a
. "$WEBCODEX_ENV"
set +a
webcodex
```

终端 2 - 在你想让 WebCodex 操作的仓库中连接 agent：

```bash
export WEBCODEX_KEY="<同一个评估 key>"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite

webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

把同一个 `WEBCODEX_KEY` 作为 Bearer key 填到 MCP client 或 GPT Action 里。完整步骤见 [docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)。

## 客户端接入

- ChatGPT 托管的客户端，包括 GPT Actions 和 ChatGPT remote MCP，都需要公网 HTTPS URL 和有效证书。你需要把 WebCodex 放到 Nginx、Caddy 或 tunnel 后面，并用 `--public-url https://your-domain.example` 启动 server，然后使用 `https://your-domain.example/openapi.json` 或 `https://your-domain.example/mcp`。
- 本地或自建客户端如果能直接访问 server，可以使用 `http://127.0.0.1:8080` 或内网 URL，不需要公网 HTTPS。
- Claude 或其他支持 MCP 的客户端使用 `/mcp` endpoint。第一次评估可以用 shared Bearer key；生产部署应切到 scoped user token，或在客户端支持时使用 OAuth。

## ChatGPT 可以做什么？

项目注册后，MCP 或 GPT Actions 客户端可以：

- 读取文件、查看目录、搜索仓库；
- 通过结构化 edit 或 patch tools 做局部源码修改；
- 用内置验证工具或受限 shell/job tools 运行必要检查；
- 检查 changed files、diff hunks 和 workspace hygiene；
- 用紧凑的任务总结收口，方便贴到 issue、PR 或 handoff note。

## 为什么需要 WebCodex？

线上模型不能直接访问你的本地文件、Git 状态、测试 runner 或 shell。常见替代方案要么靠来回粘贴代码，要么把临时脚本暴露成 HTTP 接口，要么把仓库交给托管 coding agent。

WebCodex 提供的是有用但更窄的接口：

- server 暴露经过认证的 runtime tools，而不是裸文件系统访问。
- agent 从代码所在机器注册项目目录。
- project id 是显式的，例如 `agent:<client_id>:<project_id>`。
- 能用结构化文件编辑和 patch 工具时，不让模型靠 shell 盲写。
- validation tools 和 finish summary 在交接前给出证据。
- diff 和 hygiene tools 展示实际变化，再由你决定接受、回滚或继续。

## WebCodex 适合什么场景？

当你需要 server/agent 边界、显式项目注册、同时支持 MCP 和 GPT Actions、可部署的认证路径、会话记录、验证证据，以及从本地快速试用走向私有托管 runtime 的路径，WebCodex 更合适。

它现在还不是 hosted SaaS，也不是完整浏览器 IDE。当前 workflow 以工具调用为主：ChatGPT 调用 WebCodex tools，agent 在仓库里执行操作，你再用平时的开发流程检查 diff 和验证证据。

第一次成功的标准很具体：`runtime_status` 可用，`list_projects` 能看到一个 `agent:<client_id>:<project_id>` 项目，client 能读取 `README.md`，一个只读 coding task 能正常结束，并且一个小修改可以被检查和回滚。

想先看一次完整工具流，请看 [docs/DEMO.zh-CN.md](docs/DEMO.zh-CN.md)。

## 常规 Coding Loop

WebCodex 围绕保守的 coding loop 设计：

1. `start_coding_task` - 创建显式 session，并收集有界 startup context。
2. Inspect - 使用 `list_project_files`、`search_project_text`、`read_file` 和 Git status tools。
3. Edit - 优先使用 `replace_line_range`、`insert_at_line`、`delete_line_range`、`apply_text_edits` 或 `apply_patch_checked`。
4. Validate - 根据项目运行 `validate_patch`、`cargo_fmt`、`cargo_check` 或 `cargo_test`。
5. Inspect outcome - 使用 `show_changes`、`git_diff_hunks` 和 `workspace_hygiene_check`。
6. Finish - 使用 `finish_coding_task` 或 `session_handoff_summary` 做收口。

`run_shell` 和 `run_job` 是受限 escape hatch。它们很强，不应该成为默认编辑或验证路径。

## 安全模型

WebCodex 不会把线上模型变成可信本地用户。模型只能调用暴露出来的 tools；这些 tools 会经过 server policy 和 agent project boundary；shell/job tools 仍然是显式高风险操作。

项目在 agent 所在机器上。agent 只把被允许的目录注册给 server；server 不扫描你的文件系统。

不要在 prompt、示例、tool output、文档或提交的配置文件中暴露 secrets。完整边界模型见 [SECURITY.md](SECURITY.md) 和 [docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)。

## 当前能力

- Remote MCP endpoint 和 GPT Actions OpenAPI surface，共用一个 ToolRuntime。
- agent-registered project model，project id 形如 `agent:<client_id>:<project_id>`。
- 面向局部修改的结构化源码编辑工具。
- patch validation、Cargo validation helpers、Git diff/status tools，以及受限 shell/job 执行。
- coding-task sessions、handoff、finish verdict、diff evidence 和 hygiene summary。
- 面向快速 shared-key 体验和生产部署的认证路径。
- 覆盖首次设置、概念、MCP、GPT Actions、安全、发布说明和路线图的文档。

## 已知限制

- WebCodex 是自托管基础设施，不是 hosted SaaS。
- 设置过程仍然偏技术化，默认你熟悉终端、server URL 和 agent 进程。
- 0.2.0 还没有 first-class 的 semantic code intelligence、LSP diagnostics 或 symbol navigation。
- 浏览器 console 是只读且最小化的；MCP、GPT Actions 和 CLI workflow 是主路径。
- WebCodex 还没有完整浏览器审批/评审队列。这里的 review 指检查 diff、validation、hygiene 和 session evidence，再在 Git 里决定是否接受。
- shell/job tools 需要 operator trust、有界配置和常规 code review discipline。

## 文档地图

- 第一次设置：[docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)
- Demo 工作流：[docs/DEMO.zh-CN.md](docs/DEMO.zh-CN.md)
- 概念：[docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)
- 架构：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- MCP：[docs/MCP.zh-CN.md](docs/MCP.zh-CN.md)
- GPT Actions：[docs/GPT_ACTIONS.zh-CN.md](docs/GPT_ACTIONS.zh-CN.md)
- 安全：[SECURITY.md](SECURITY.md)
- 认证模型：[docs/AUTH_MODEL.zh-CN.md](docs/AUTH_MODEL.zh-CN.md)
- 部署：[docs/DEPLOYMENT.zh-CN.md](docs/DEPLOYMENT.zh-CN.md)
- 发布说明：[docs/RELEASE_NOTES_v0.2.0.md](docs/RELEASE_NOTES_v0.2.0.md)
- Roadmap：[docs/ROADMAP.zh-CN.md](docs/ROADMAP.zh-CN.md)
- 完整索引：[docs/INDEX.zh-CN.md](docs/INDEX.zh-CN.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
