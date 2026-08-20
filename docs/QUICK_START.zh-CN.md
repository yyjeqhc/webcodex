# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这是在单个本地 Git 项目上跑通 WebCodex 的最短路径。它使用 project-first 流程：
`webcodex setup` 配置当前目录，不需要你提供 client id、runtime project id、
transport 或内部配置路径。

接入已有 hosted Server 见[部署指南](DEPLOYMENT.zh-CN.md#把仓库接入已有的-server)；
让 AI agent 帮你搭建见 [AI 接入指南](AI_ONBOARDING.zh-CN.md)。

## 前置条件

- 三个二进制都已安装：`webcodex`、`webcodex-server`、`webcodex-runner`。
- Git 在 `PATH` 上。
- 一个可以安全查看和编辑的 Git 项目。

安装打包版本：

```bash
npm install -g @yyjeqhc/webcodex
```

或从本 checkout 构建：

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## 1. 设置项目

```bash
cd /path/to/your/repository
webcodex setup
```

首次运行 `setup` 会：解析 Git 顶层目录；在 checkout 之外创建私有状态；创建最小
的项目注册与 Agent 配置；为当前项目的 Connector 与 Agent 创建一把 Project
Credential——不会打印它。它会让 Server 与 Agent 保持停止。

再次运行可验证幂等性；第二次结果应为 `already configured`。如果某个生成的组件
缺失，`setup` 只修复该组件。

## 2. 检查就绪

```bash
webcodex doctor
```

`doctor` 只读。在 Agent 启动前，预期结果是 `Needs action`，并给出
`Next: webcodex run`。用 `--json` 获取结构化结果。

## 3. 启动本地 runtime

```bash
webcodex run
```

这会在前台启动 project-bound loopback Server 与本地 Agent。保持该终端打开；
Ctrl-C 会同时停止两者。在另一个终端、同一个项目下执行：

```bash
webcodex status
```

就绪项目会报告 Project、Connection、Agent、coding 就绪状态与 next action。

## 4. 接入客户端

### 临时通过 HTTPS 分享

Hosted 客户端无法访问 loopback 地址。若需要从 hosted MCP 客户端临时访问，
先停止 `webcodex run`，再执行：

```bash
webcodex share
```

`share` 复用项目设置与本地 runtime，但会额外启动 Cloudflare Quick Tunnel 与
一把独立的临时 Connector credential。它输出临时 `https://*.trycloudflare.com/mcp`
URL 与 Bearer token；命令退出后两者都失效。

如果 MCP 客户端要求 OAuth，先确定它的精确 callback，然后运行：

```bash
webcodex share --auth oauth --oauth-redirect-uri https://client.example/callback
```

命令会输出临时 Project share credential，以及 project-bound OAuth client ID/secret。Project share credential 只应输入 WebCodex 自己的授权页。OAuth grant 被 fenced 到本次 `share` 进程，因此重启后旧 access/refresh token 失效；同一项目与 callback 的 client ID/secret 会保存在受保护的 project state 中以便复用。

`webcodex share --tunnel none` 在不开放公网 tunnel 的情况下启动同一 runtime，用于本地调试。如需稳定、由 operator 管理的 OAuth origin，可再加 `--public-url https://share.example`，并自行把该 HTTPS origin 反向代理/隧道到本地 WebCodex 端口。Quick Tunnel 不提供稳定 origin，也不适合作为生产部署。

### 接入已有 Server

如需接入已有 Server 的稳定长期环境，仍可使用 shared-key：

```bash
webcodex connect https://webcodex.example --project .
```

如果希望得到与该 managed/self-hosted Server 一致的 OAuth 体验，先登录一次，再使用 MCP 客户端的精确 callback：

```bash
webcodex login https://webcodex.example --code wc_pair_...
webcodex connect https://webcodex.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback --project .
```

OAuth connect 使用远端 Server 当前配置的 MCP model surface 和可委托 OAuth permission registry；OAuth client secret 保存在受保护的 hosted profile 中，Runner 则使用独立 Agent token。见[部署指南](DEPLOYMENT.zh-CN.md)。

以后如果只想从 hosted `connect` profile 中注销当前仓库，可在仓库中运行：

```bash
webcodex disconnect
```

它只注销 canonical 路径精确匹配的仓库，保留 source tree、`.git`、profile credential
以及其他已注册项目。如果同一仓库存在于多个 hosted profile 中，请显式传
`--profile NAME`。

## 5. 运行 coding 任务

让客户端做一个小的、可回退的改动。典型调用序列：

```text
task_start
→ files_list
→ files_read 或 files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

编辑、命令与校验使用稳定的 `operation_id`：重试相同 payload 会复用同一操作；
同一 ID 下不同 payload 会 fail closed。

`checks_run` 支持 `format`、`check`、`test` 与可选 `recipe`（`rust`、`node`、
`python`、`go`）。省略 recipe 时自动从最近的 manifest 目录解析。

## 6. 本地审查与接受

coding 结果在人工决策前与目标 checkout 隔离：

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

用 `webcodex task reject <task-id>` 丢弃它。Accept 会先验证目标 Git 状态仍与
task 基线一致再应用结果。在线模型可以提出工作，但永远不能接受它。

也可以在浏览器中审查：打开 `/console` 使用工作队列。浏览器里的 Accept/Reject
与 CLI 使用同一套决策权威。

## 故障排查

先执行：

```bash
webcodex status
webcodex doctor
```

常见稳定错误码与下一步：

| 错误码 | 含义 | 下一步 |
| --- | --- | --- |
| `project_not_configured` | 该项目/profile 没有 setup | `webcodex setup` |
| `project_credential_invalid` | 私有凭据状态缺失或不匹配 | 恢复两个匹配的私有文件或重建 profile |
| `server_unreachable` | 无法访问 loopback runtime | `webcodex run` |
| `agent_offline` | Server 可达但本地 Agent 不可用 | `webcodex run` |
| `required_capability_unavailable` | 已安装 Agent 过旧 | 升级所有二进制 |
| `workspace_unavailable` | Git 或项目路径不可用 | 恢复路径/Git 工作区 |
| `checks_required` | 普通结果尚未运行检查 | 先 `checks_run` 再 finish |
| `checks_stale` | 上次检查后工作区已变化 | 运行一次新检查 |

完整运维检查清单见[故障排查](TROUBLESHOOTING.zh-CN.md)。
