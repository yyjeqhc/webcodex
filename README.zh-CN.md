# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex 让 ChatGPT 等线上模型通过自托管、可审计的工具 runtime，安全操作你的私有代码仓库。

它把线上 AI 编程从“盲改文件”变成“有权限边界、有验证证据、有审计记录的工程流程”。

- 给线上模型提供有边界的读取、编辑、验证和 review 工具。
- 通过连接到 server 的 WebCodex agent，把代码执行留在你的机器或可信主机上。
- 只有 agent 注册过的目录才会成为可见项目。
- 记录 session、validation、handoff 和 finish evidence，方便人类 review。
- MCP 和 GPT Actions 使用同一个 WebCodex runtime，不需要两套工具语义。

```text
ChatGPT / Claude / Grok
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

## 为什么需要 WebCodex？

线上模型不能直接访问你的本地文件、Git 状态、测试 runner 或 shell。常见替代方案要么靠来回粘贴代码，要么把临时脚本暴露成 HTTP 接口，要么把仓库交给托管 coding agent。

WebCodex 提供的是更窄的接口：

- server 暴露经过认证的 runtime tools，而不是裸文件系统访问。
- agent 从代码所在机器注册项目目录。
- project id 是显式的，例如 `agent:<client_id>:<project_id>`。
- 能用结构化文件编辑和 patch 工具时，不让模型靠 shell 盲写。
- validation tools 和 finish summary 在交接前给出证据。
- review tools 展示 diff 和 workspace 状态，再由人决定是否接受。

## 快速开始

先走 [docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md) 的 local-first 路径。它会带你启动一个 server、一个 agent、注册一个项目，并连接 MCP 或 GPT Action client。

第一次成功的标准很具体：`runtime_status` 可用，`list_projects` 能看到一个 `agent:<client_id>:<project_id>` 项目，client 能读取 `README.md`，一个只读 coding task 能正常结束，并且一个小修改可以被 review 和回滚。

想先看一次完整工具流，请看 [docs/DEMO.zh-CN.md](docs/DEMO.zh-CN.md)。

## 选择客户端

- 如果 client 支持 remote MCP，优先使用 MCP。
- 如果你在构建 Custom GPT，使用 GPT Actions。
- 两者调用同一个 WebCodex ToolRuntime。

MCP client 连接：

```text
https://your-domain.example/mcp
```

GPT Actions 导入：

```text
https://your-domain.example/openapi.json
```

## 默认 Coding Loop

WebCodex 围绕保守的 coding loop 设计：

1. `start_coding_task` - 创建显式 session，并收集有界 startup context。
2. Inspect - 使用 `list_project_files`、`search_project_text`、`read_file` 和 Git review tools。
3. Edit - 优先使用 `replace_line_range`、`insert_at_line`、`delete_line_range`、`apply_text_edits` 或 `apply_patch_checked`。
4. Validate - 根据项目运行 `validate_patch`、`cargo_fmt`、`cargo_check` 或 `cargo_test`。
5. Review - 使用 `show_changes`、`git_diff_hunks` 和 `workspace_hygiene_check`。
6. Finish - 使用 `finish_coding_task` 或 `session_handoff_summary` 做收口。

`run_shell` 和 `run_job` 是受限 escape hatch。它们很强，不应该成为默认编辑或验证路径。

## 安全模型

WebCodex 不会把线上模型变成可信本地用户。模型只能调用暴露出来的 tools；这些 tools 会经过 server policy 和 agent project boundary；shell/job tools 仍然是显式高风险操作。

项目在 agent 所在机器上。agent 只把被允许的目录注册给 server；server 不扫描你的文件系统。

不要在 prompt、示例、tool output、文档或提交的配置文件中暴露 secrets。完整边界模型见 [SECURITY.md](SECURITY.md) 和 [docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)。

## 0.2.0 包含什么

- Remote MCP endpoint 和 GPT Actions OpenAPI surface，共用一个 ToolRuntime。
- agent-registered project model，project id 形如 `agent:<client_id>:<project_id>`。
- 面向局部修改的结构化源码编辑工具。
- patch validation、Cargo validation helpers、Git diff/status tools，以及受限 shell/job 执行。
- coding-task sessions、handoff、finish verdict、review evidence 和 hygiene summary。
- 面向快速 shared-key 体验和 managed token 部署的认证路径。
- 覆盖首次设置、概念、MCP、GPT Actions、安全、发布说明和路线图的文档。

## 已知限制

- WebCodex 是自托管基础设施，不是 hosted SaaS。
- 设置过程仍然偏技术化，默认你熟悉终端、server URL 和 agent 进程。
- 0.2.0 还没有 first-class 的 semantic code intelligence、LSP diagnostics 或 symbol navigation。
- UI/dashboard 不是主要入口；MCP、GPT Actions 和 CLI workflow 是主路径。
- shell/job tools 需要 operator trust、有界配置和 review discipline。

## 文档地图

- 第一次设置：[docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)
- Demo 工作流：[docs/DEMO.zh-CN.md](docs/DEMO.zh-CN.md)
- 概念：[docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)
- 架构：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- MCP：[docs/MCP.zh-CN.md](docs/MCP.zh-CN.md)
- GPT Actions：[docs/GPT_ACTIONS.zh-CN.md](docs/GPT_ACTIONS.zh-CN.md)
- 安全：[SECURITY.md](SECURITY.md)
- 发布说明：[docs/RELEASE_NOTES_v0.2.0.md](docs/RELEASE_NOTES_v0.2.0.md)
- Roadmap：[docs/ROADMAP.zh-CN.md](docs/ROADMAP.zh-CN.md)
- 完整索引：[docs/INDEX.zh-CN.md](docs/INDEX.zh-CN.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
