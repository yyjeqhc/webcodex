# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

把 ChatGPT 接到仍然留在你机器上的私有代码。

WebCodex 会在你的机器上运行一个小型 server 和一个靠近仓库的 agent。ChatGPT 可以通过 WebCodex 查看文件、请求局部修改、运行验证；你的仓库仍然留在原地。

- 只开放你明确选择的项目目录。
- 测试和 shell 执行留在 agent 所在机器。
- 接受修改前，先检查 changed files、validation output 和任务总结。
- 本地可以直接试用；接入 ChatGPT 托管客户端时，再把 server 放到 HTTPS 后面。

## 安装

当前 Linux x64 release 可以直接安装：

```bash
npm install -g @yyjeqhc/webcodex
```

也可以从源码构建：

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

平台支持和安装细节见 [docs/BUILD_INSTALL.zh-CN.md](docs/BUILD_INSTALL.zh-CN.md)。

## 快速开始

localhost 示例建议先在同一台机器上开两个终端：一个运行 server，另一个进入你想让 WebCodex 操作的仓库。下面命令假设 npm 安装后的 binaries 已经在 `PATH` 中。
如果你从源码构建，请先执行 `export PATH="$PWD/target/release:$PATH"`。

server 终端 - 创建 server 配置并启动 WebCodex：

```bash
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"

webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080

WEBCODEX_ENV_FILE="$WEBCODEX_ENV" webcodex
```

`server up` 会在 `$WEBCODEX_ENV` 不存在时创建它，也会创建父目录。这个文件保存 server 设置和 server admin key。它不是要复制到 MCP 或 GPT Actions 里的 key。

仓库终端 - 进入你想让 WebCodex 操作的仓库，创建 key、注册仓库并启动 agent：

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
printf 'Copy this key into MCP/GPT Actions auth: %s\n' "$WEBCODEX_KEY"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite

webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

先留好打印出来的 key。后面用 `curl` 验证、配置 MCP client 或 GPT Actions auth 时，都粘贴同一个值。不需要把这个 key 加到 server 配置里。

## 验证

在客户端终端中，粘贴上面打印的同一个 key：

```bash
export WEBCODEX_KEY="<同一个评估 key>"

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"runtime_status","summary_only":true}'

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"list_projects"}'
```

把返回的 `agent:<client_id>:<project_id>` 值用在 MCP 或 GPT Actions prompts 中。

完整步骤见 [docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)。

## 客户端接入

- 本地客户端可以直接使用 `http://127.0.0.1:8080`。
- ChatGPT 托管的客户端需要公网 HTTPS URL。你可以把 WebCodex 放到 Nginx、Caddy 或 tunnel 后面，并用 `--public-url https://your-domain.example` 启动 server，然后使用 `https://your-domain.example/openapi.json` 或 `https://your-domain.example/mcp`。
- MCP client 使用 `/mcp`；GPT Actions 使用 `/openapi.json`。第一次运行时，把仓库终端打印出来的 key 填到 Bearer/API-key auth 配置里。

## 整体结构

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
3. Edit - 局部精确修改优先 `apply_text_edits`；多文件协调 patch 用 `apply_patch_checked`；新建或有意整文件重写用 `write_project_file`。行/模式类工具仍可用，作为兼容路径。
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
