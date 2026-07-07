# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这是 WebCodex 推荐的 local-first 首次体验路径。

术语见 [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md)。想看一次真实工具流，见 [DEMO.zh-CN.md](DEMO.zh-CN.md)。

## 最快路径

第一次评估时使用一个 shared key：server runtime calls、agent connect、MCP/GPT Actions 都使用同一个 key。scoped token、OAuth 和生产部署后面再看。

## 你会运行什么

- 一个 WebCodex server，可以是本地 URL 或 HTTPS URL。
- 一个 WebCodex agent，运行在拥有代码的机器上。
- 一个由 agent 注册的项目。
- 一个线上 client：client 支持 remote MCP 时用 MCP；构建 Custom GPT 时用 GPT Actions。

MCP 和 GPT Actions 调用同一个 WebCodex ToolRuntime。client 改变的是协议封装，不改变项目边界和工具行为。

## 前置条件

- 从本仓库运行时需要 Rust 和 Cargo。
- 一台能同时运行 server 和 agent 的机器，用于第一次测试。
- 一个愿意以受控方式检查和编辑的代码仓库。
- 一个 client：
  - ChatGPT MCP / remote MCP client，支持时优先使用。
  - ChatGPT Custom GPT + GPT Actions，用于 Custom GPT 场景。

第一次运行不要使用真实 secrets、生产仓库或高权限 shell profile。

## 1. 构建二进制

在 WebCodex checkout 中运行：

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

对应 release artifacts 可用后，可以用 release binaries 替代 `cargo build`。

## 2. 选择一个 shared key 并启动 server

终端 1，先为这次评估选择一个长随机 key：

```bash
export WEBCODEX_KEY="$(openssl rand -base64 32)"
export WEBCODEX_ENV="$HOME/.config/webcodex/webcodex.env"
```

后续 `webcodex-cli connect`、`curl`、MCP 和 GPT Actions 都使用同一个 `WEBCODEX_KEY`。不要把真实 key 值写入提交文件。

准备 server env：

```bash
webcodex-cli server up \
  --env-file "$WEBCODEX_ENV" \
  --listen 127.0.0.1:8080 \
  --public-url http://127.0.0.1:8080
```

`server up` 会启用 shared-key quick-start mode，并写入 server env file。它没有 `--key` 参数，也会故意隐藏完整 server bootstrap key。

加载 env 并启动 server：

```bash
set -a
. "$WEBCODEX_ENV"
set +a
webcodex
```

保持 `webcodex` 进程运行。

如果要接入公网 ChatGPT，把 server 放到 HTTPS 后面，并使用公网 URL。本地 runtime sanity check 用 localhost 即可。

## 3. 连接 agent 并注册项目

终端 2，在你希望 WebCodex 操作的仓库中运行：

```bash
export WEBCODEX_KEY="<同一个评估 shared key>"

webcodex-cli connect http://127.0.0.1:8080 \
  --key "$WEBCODEX_KEY" \
  --root "$PWD" \
  --client-id local-dev \
  --overwrite
```

该命令会生成 agent config，并为所选 root 生成项目注册条目。使用 `connect` 打印的 config 路径启动 agent；如果 client id 使用默认示例，则是：

```bash
webcodex-agent --config "$HOME/.config/webcodex/clients/local-dev/agent.toml"
```

项目在 agent 所在机器上。agent 只把被允许的目录注册给 server；server 不扫描你的文件系统。

## 4. 验证 runtime health

终端 3：

```bash
export WEBCODEX_KEY="<同一个评估 shared key>"

curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"runtime_status","summary_only":true}'
```

再验证项目：

```bash
curl -sS \
  -H "Authorization: Bearer $WEBCODEX_KEY" \
  -H 'Content-Type: application/json' \
  http://127.0.0.1:8080/api/tools/call \
  -d '{"tool":"list_projects"}'
```

你应该能看到这种形状的 project id：

```text
agent:local-dev:<project_id>
```

如果通过 `connect` 从仓库 root 生成配置，命令会打印生成的 project id。client prompt 和 tool call 中应使用完整 runtime project id。

## 5. 连接 ChatGPT MCP

client 支持 remote MCP 时，使用 MCP。

配置 client：

```text
URL:  http://127.0.0.1:8080/mcp
Auth: Bearer <shared key>
```

对 ChatGPT 或其他 hosted client，把 localhost 换成公网 HTTPS server URL：

```text
https://your-domain.example/mcp
```

第一次评估使用 `Bearer <shared key>`。生产认证后面再看。截图和常见 MCP 错误见 [MCP.zh-CN.md](MCP.zh-CN.md)。

## 6. 或连接 GPT Actions

构建 Custom GPT 时，使用 GPT Actions。

导入 schema：

```text
http://127.0.0.1:8080/openapi.json
```

对 ChatGPT，使用公网 HTTPS URL：

```text
https://your-domain.example/openapi.json
```

Action authentication 选择 Bearer/API-key auth，首次体验使用同一个 shared key。设置说明见 [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)。

## 7. 运行只读任务

先要求 client 保持只读：

```text
Use WebCodex on project agent:local-dev:<project_id>.
Start a coding task, inspect README.md, summarize what the project does,
show changes without a diff, run workspace hygiene, and finish the task.
Do not edit files.
```

预期流程：

1. `start_coding_task`
2. `read_file` 或 `search_project_text`
3. `show_changes`
4. `workspace_hygiene_check`
5. `finish_coding_task`

## 8. 运行一个小而可回滚的修改

使用 disposable branch，或做一个很小的文档修改：

```text
Use WebCodex on project agent:local-dev:<project_id>.
Make one small documentation edit, validate what is appropriate for a docs-only
change, show changes, run workspace hygiene, and finish with a clear verdict.
Prefer structured edit tools. Do not use run_shell unless needed.
```

接受结果前，先 review changed files 和 diff。如果只是 smoke test，用你平时的 Git 流程回滚该修改。

## 第一次成功标准

完成设置的标准：

- `runtime_status` 可用。
- `list_projects` 显示 `agent:<client_id>:<project_id>` 项目。
- client 能读取 `README.md`。
- 一个只读 coding task 能干净结束。
- 一个小修改可以被 review 和回滚。

## 生产认证后面再看

这条 shared-key 路径只用于第一次评估。生产环境请阅读 [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)、[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) 和 [OPERATIONS.md](OPERATIONS.md)，再迁移到 scoped user tokens 或 OAuth、反向代理 HTTPS、服务管理和 token rotation。

## MCP vs GPT Actions

- 如果 client 支持 remote MCP，使用 MCP。
- 如果你在构建 Custom GPT，使用 GPT Actions。
- 两者调用同一个 WebCodex ToolRuntime。

第一次 prompt 应明确写出完整 project id，并要求只读任务。等 client 能完成 inspect 和 finish，再进入写入任务。

## 安全默认值

- 项目访问由 agent 注册。
- server 不扫描文件系统。
- 模型只能调用暴露出来的 tools。
- 优先使用结构化编辑和验证工具。
- `run_shell` 是受限 escape hatch，不是默认编辑或验证路径。
- 不要把 bootstrap、account 或 agent credential 粘贴到 MCP 或 GPT Actions。

完整边界模型见 [../SECURITY.md](../SECURITY.md)。

## 排障

### Agent Not Connected

检查 agent 进程日志，确认它使用生成的 config 启动。让模型编辑前，先运行 `runtime_status` 并确认有 online agent。

### Project Not Listed

运行 `list_projects`。如果项目缺失，在目标仓库 root 重新运行 `webcodex-cli connect`，或检查生成的 agent project registry。不需要 server-side project registry。

### Auth Failed

agent connect、runtime checks、MCP 和 GPT Actions 都使用同一个 `WEBCODEX_KEY`。生产认证请切到 [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)，不要复用 bootstrap、account 或 agent credential。

### Model Chose The Wrong Project Id

把完整 `agent:<client_id>:<project_id>` 写进 prompt。让 client 先调用 `list_projects`，确认选中的 project，再读取或编辑文件。

### Response Too Large

使用 compact summaries：`runtime_status(summary_only=true)`、focused `tool_manifest` discovery、有界文件范围、`show_changes(include_diff=false)` 和 `finish_coding_task(summary_only=true)`。

### Shell Or Job Feels Too Broad

优先使用结构化工具：`read_file`、`search_project_text`、line edits、`apply_text_edits`、`validate_patch`、`cargo_fmt`、`cargo_check`、`cargo_test`、`show_changes` 和 `workspace_hygiene_check`。

## 下一步文档

- Demo 工作流：[DEMO.zh-CN.md](DEMO.zh-CN.md)
- 概念：[CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md)
- MCP 设置：[MCP.zh-CN.md](MCP.zh-CN.md)
- GPT Actions 设置：[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)
- 认证模型：[AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)
- 部署细节：[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)
- 运维：[OPERATIONS.md](OPERATIONS.md)
- 排障：[TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md)
