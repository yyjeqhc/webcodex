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

### Project-aware validation recipes

`checks_run` 保留现有 `format`、`check`、`test` 语义名，并增加可选 enum
`recipe: rust|node|python|go`。省略 `recipe` 即 auto resolution，不存在 `auto`
alias。resolver 从 Task execution workspace 内的相对 `cwd` 开始，只向该 workspace
root 逐级查找并选择最近的 manifest 目录。同一最近目录存在多个 supported marker
时 ambiguous；显式提供实际存在的 recipe 可解除歧义。唯一的 markerless 例外是：
没有选中 `pyproject.toml` 时，显式 `recipe=python` 加 `checks=["test"]` 会从
`cwd` 运行 `python -B -m unittest discover -v`。其他 recipe 的 marker 不匹配、
auto 模式 manifest 缺失、绝对/父目录路径或 symlink escape 都在 reservation 前
拒绝。

| Recipe | Marker | `format` | `check` | `test` |
|---|---|---|---|---|
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` 加一个安全 argv filter |
| Node | `package.json` | 依次选择 `format:check`、`format-check`、`check:format` | 依次选择 `check`、`typecheck`、`lint` | 精确 `test` |
| Python | `pyproject.toml`，或显式 markerless test | 已配置 Ruff，否则 Black | 已配置 Ruff，否则 Mypy | 已配置 pytest；markerless 时 `unittest discover` |
| Go | `go.mod` | unavailable | `go vet ./...` | `go test ./...` |

Node 从有效 `packageManager` 声明或唯一、无歧义的 supported lockfile
（`pnpm-lock.yaml`、`yarn.lock`、`package-lock.json`、`npm-shrinkwrap.json`、
`bun.lock`、`bun.lockb`）选择 package manager。证据冲突或缺失时 fail closed；
script 只以 `<manager> run --silent <allowlisted-name>` 调用，script body 不进入
plan 或 error。Python 的 format/check 和 pytest 仅启用 `pyproject.toml` 有配置
证据的工具；format 时 Ruff 优先于 Black，check 时 Ruff 优先于 Mypy。
Manifestless Python 只支持固定的 unittest test plan。

recipe 不安装依赖、不运行 install hook、不生成配置、不创建 environment、不修改
lockfile、不联网。只有 Rust 支持 `test_filter`，且作为单独 argv；其他 recipe
会拒绝 filter，绝不忽略后运行全量测试。executable 或 Python module 缺失属于
executor failure，不生成 failed check 或 assertion evidence；真实进程以 non-zero
返回 validation verdict 才属于 assertion failure。

`task_finish` 会从 result patch 排除 untracked interpreter/test cache、coverage
output 和 `node_modules`，并以 bounded warning 报告；项目已 tracked 的同名路径
绝不排除。

durable plan 记录 recipe ID/version、相对 root、semantic checks、tool identity 和
invocation/manifest evidence digest，并全部进入 request hash。因此同一
`operation_id` 只复用完全相同的 resolved plan；recipe binary 变化会与旧 ID
conflict，使用新 ID 才按新 recipe 解析。manifest、lockfile 或 workspace 改变会使
成功 provenance stale。

## 6. 本机 review 和 accept

coding result 会与 target checkout 保持隔离，直到人类决定。该决定是本机授权，有两条
入口——离线 CLI 与浏览器 console——两者共用同一套 accept/reject 授权：

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

使用 `webcodex task reject <task-id>` 丢弃结果。Accept 前会验证 target Git state
仍匹配 task baseline。

在浏览器中，打开 `/console`，输入 project credential（仅保存在内存中、绝不持久化），
用工作队列选择任务。review 详情展示目标、状态、validation、changed files、bounded
unified diff 与 bounded output tail，**Accept / Reject / Cancel** 均需页面内显式确认。
Accept 与 Reject 调用与 CLI 相同的授权；Cancel 停止正在运行的 execution。Hosted Chat
只能提议工作、永远无法接受；server 在应用前会重新校验 checkout 与 result——浏览器上
的点击无法绕过这些前置条件。

## Browser console

本地 runtime 运行时，`/console` 显示：

- Project header（当前 Project、Connection、Agent readiness、coding capability
  readiness、下一步 action）；
- 可执行工作队列（最近需要关注的任务）；
- 选中任务的 review 详情，含 bounded diff 与 output tail。

它消费 doctor/status 同一组 application readiness facts，并驱动与 CLI 相同的本机
决策授权。它不显示 Agent registry、client ID、transport implementation、queue ID
或 token，也不是 browser editor / terminal——无法编辑代码、运行命令或启动任务。

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
| `validation_recipe_not_found` | auto resolution 从 `cwd` 到 Task root 没有 supported marker | 选择包含 manifest 的 `cwd`，或显式使用 markerless Python unittest test recipe |
| `validation_recipe_ambiguous` | 最近 root 有多个 supported marker | 提供匹配的显式 `recipe` |
| `validation_recipe_mismatch` / `validation_manifest_invalid` | recipe、marker、安全路径或 manifest evidence 无效 | 修复报告的公开 evidence |
| `validation_check_unavailable` / `test_filter_unsupported` | recipe 无法安全映射 check/filter | 修改 checks/filter 或选择匹配 recipe |
| `package_manager_ambiguous` | Node package-manager evidence 缺失或冲突 | 修正 `packageManager` 或 lockfile |
| `validation_tool_unavailable` | Agent host 缺少所选 executable/module | 提供项目已有工具并使用新 operation ID |
| `checks_required` | 普通 result 尚未运行 checks | 运行 `checks_run` 后 finish |
| `checks_stale` | 上次可信 check 后 workspace 改变 | 运行新的 check operation |

高级 server、enrollment、OAuth、transport 和 fleet diagnostics 继续放在
`webcodex-cli` 与 operations 文档中，不是 onboarding 步骤。
