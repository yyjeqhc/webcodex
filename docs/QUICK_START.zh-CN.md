# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这是唯一 canonical project-first 路径。它配置一个本地 Git 项目，不要求用户提供
Agent client ID、runtime project ID、transport、workflow session、executor
reference 或内部 config path。

## 前置条件

- 已安装三个 WebCodex binaries：`webcodex`、`webcodex-cli`、
  `webcodex-agent`；
- `PATH` 中有 Git；
- 一个可以安全查看和修改的 Git 项目。

安装 Linux x64 package：

```bash
npm install -g @yyjeqhc/webcodex
```

或从本仓库构建：

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

## 1. Setup 当前项目

进入 Git 项目并执行：

```bash
webcodex setup
```

第一次运行会：

- 解析 Git top-level directory；
- 在 checkout 外创建 private state；
- 创建最小 project registration 和 Agent config；
- 创建一个供本项目 Connector 与 Agent 使用的精确 Project Credential，但不打印
  其内容；
- 保持 server 和 Agent 停止。

它不会修改项目文件或 Git，不会启动 service、修改 shell config、开放网络端口或
上传源码。

再次执行同一命令验证幂等：

```bash
webcodex setup
```

第二次返回 `already configured`。若一个生成组件缺失，只修复该组件。若已有字段
与当前 Git root/profile 冲突，setup 会指出字段并停止，不覆盖现有配置。

Connector credential file 与 Agent config 保存同一个 secret，并映射到同一个稳定、
非秘密的 project grant identity。两个文件都属于 owner-only private state；数据库
不保存明文。runtime 会 hash candidate 并使用 constant-time comparison。这条路径
独立于普通 shared-key quick start：project mode 会拒绝任意未知 Bearer value。

Setup 不会静默轮换仍存在的 credential。credential 丢失时应恢复两份匹配的 private
file；若无法恢复，先停止 runtime，明确退役整个 private project-state profile，再
重新运行 setup。该显式重建也会退役其中的本地 Task/Execution history；Iteration
8.0 没有 in-place rotate subcommand。

## 2. 诊断下一步

```bash
webcodex doctor
```

Doctor 完全只读。Agent 尚未启动时，预期 verdict 是 `Needs action`：

```text
Next:
  webcodex agent start
```

每条 finding 都有稳定 `name`、`status`、`code`、`summary` 和 `next_action`。
需要结构化 projection 时使用 `webcodex doctor --json`。

## 3. 启动本地 runtime

```bash
webcodex agent start
```

这是显式 foreground action，会启动绑定当前项目的 loopback server 和本地 Agent；
不会安装 system service。保持该终端运行，Ctrl-C 会停止两个进程。Loopback 不构成
认证豁免；只有 setup 配置的精确 Project Credential 能访问该项目 Connector/Agent。

在同一项目的另一个终端运行：

```bash
webcodex status
```

ready 时只显示 Project、Connection、Agent、coding readiness 和 next action。需要
完整诊断时再次运行 `webcodex doctor`。

## 4. 使用 project-bound Connector

当前项目生成的 Connector profile 会把一个 logical project 确定性绑定到一个
registered executor。使用这份 approved connection 及其精确 credential 的本地
MCP/OpenAPI client 可以直接调用：

```text
task_start
```

它不需要 `list_projects`、`runtime_status`、`tool_manifest`、`start_session` 或
`current_session`，prompt 中也不需要 `agent:<client>:<project>`。

ChatGPT hosted client 无法访问 loopback address。operator 必须提供批准的 HTTPS
endpoint 和认证，同时保持 project binding 不变。见
[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)、[MCP.zh-CN.md](MCP.zh-CN.md) 或
[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)。Setup 不创建 tunnel，也不暴露端口。

## 5. 运行 golden coding path

让 client 完成一个小型、可逆修改。canonical 调用为：

```text
task_start
→ files_read 或 files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

edit、command 和 check 使用 `operation_id` 提供 exact retry identity：同一 payload
重试会复用 operation；同一 ID 搭配不同 payload 会 fail closed。

普通可写 task 未运行 structured check 时不能 finish。check 真正运行后 non-zero
才属于 project assertion failure；check 无法 spawn 属于 executor/infrastructure
failure，不产生 assertion evidence 或 trusted workspace provenance。

## 6. 本机 review 和 accept

coding result 会与 target checkout 保持隔离，直到人类决定：

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

使用 `webcodex task reject <task-id>` 丢弃结果。Accept 前会验证 target Git state
仍匹配 task baseline。

## Browser readiness

本地 runtime 运行时，`/console` 只显示：

- 当前 Project；
- Connection；
- Agent readiness；
- coding capability readiness；
- structured findings 和下一条 CLI action。

它消费 doctor/status 同一组 application readiness facts，不显示 Agent registry、
client ID、transport implementation、queue ID、token，也不提供 browser editor 或
terminal。

## Troubleshooting

始终先运行：

```bash
webcodex status
webcodex doctor
```

常见 stable code：

| Code | 含义 | 下一步 |
|---|---|---|
| `project_not_configured` | 当前 Git 项目/profile 没有 setup | `webcodex setup` |
| `project_registration_invalid` | 现有 state 冲突或不完整 | 解决指出的字段后重新 setup |
| `project_credential_invalid` | private credential 缺失、不可读、权限不安全、格式错误或两份不匹配 | 恢复两份匹配的 private file，或显式重建 profile |
| `project_credential_rejected` | server 拒绝本地配置的 credential | 恢复匹配 credential；不得折叠成 Agent offline |
| `server_unreachable` | loopback runtime 不可达 | `webcodex agent start` 或查看 doctor |
| `agent_offline` | server 可达但本地 Agent 不可用 | `webcodex agent start` |
| `required_capability_unavailable` | Agent 太旧或不完整 | 升级全部 WebCodex binaries |
| `structured_validation_unavailable` | Agent 缺少 structured validation | 升级全部 WebCodex binaries |
| `workspace_unavailable` | Git 或配置的项目路径不可用 | 恢复 path/Git workspace |
| `checks_required` | 普通 result 尚未运行 checks | 运行 `checks_run` 后 finish |
| `checks_stale` | 上次可信 check 后 workspace 改变 | 运行新的 check operation |

高级 server、enrollment、OAuth、transport 和 fleet diagnostics 继续放在
`webcodex-cli` 与 operations 文档中，不是 onboarding 步骤。
