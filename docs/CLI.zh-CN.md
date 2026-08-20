# WebCodex CLI

`webcodex` 是统一的操作与开发命令行。它覆盖项目设置、Server 与 Runner 生命周期、
设备接入、令牌管理和只读运维检查。

远程操作（用户、令牌、pairing、运维检查）走 Server HTTP API，CLI 是它们的便捷
客户端；本地操作（项目设置、服务管理、task 审查决策）直接运行在主机上，无法通过
Server API 完成。

从源码构建会产生三个二进制：

- `webcodex` —— 本文档介绍的统一命令。
- `webcodex-server` —— Server 进程（用 `webcodex server ...` 启动与托管）。
- `webcodex-runner` —— 实际执行项目工作的 Runner 进程（用
  `webcodex agent ...` 启动与托管）。

`webcodex --help` 会列出顶层命名空间。下面按命名空间说明各自的用途。

## 命令总览

### 项目 / 本地工作流

以下命令作用于当前 Git 项目。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex setup` | 为当前 Git 项目配置 project-first 使用方式 | 创建私有状态与 Project Credential；不启动服务。 |
| `webcodex doctor` | 当前项目的只读就绪检查 | 输出稳定的 `next action`；用 `--json` 获取结构化结果。 |
| `webcodex run` | 启动 project-bound loopback Server 与本地 Runner | 前台运行；Ctrl-C 同时停止两者。 |
| `webcodex status` | 简洁的项目 coding 就绪状态 | `doctor` 提供完整检查。 |
| `webcodex share` | 通过 HTTPS 分享本地项目 | 默认使用临时 Bearer credential；`--auth oauth --oauth-redirect-uri <URL>` 可启用 project-bound OAuth。 |
| `webcodex connect <server>` | 把当前项目接入已有的 Server | 默认使用 hosted shared-key；`--auth oauth --oauth-redirect-uri <URL>` 可复用 managed login 并在远端创建 OAuth client。 |
| `webcodex disconnect [--project PATH] [--profile NAME]` | 移除一个 hosted 项目注册 | 是该仓库 `connect` 的精确逆操作；绝不删除仓库或 `.git`。 |

`webcodex share --auth oauth --oauth-redirect-uri <精确回调地址>` 使用 OAuth 2.0 Authorization Code + PKCE S256。OAuth client ID/secret 会按“项目 + 回调地址”保存在受保护的 project state 中；authorization code、access token、refresh token 与临时 Project Credential 则都被 fenced 到当前 `share` 进程。重启 `share` 会让旧 OAuth grant 失效，但不会改变 Connector 的稳定 project identity。OAuth access token 永远不能用于 Runner/Agent transport。

Cloudflare Quick Tunnel 的公网 origin 仍然是临时的。如需稳定 HTTPS origin，可使用 `--tunnel none --public-url https://share.example`，并由 operator 自己把该 origin 反向代理/隧道到 loopback WebCodex Server；`--public-url` 只声明外部 origin/issuer，不会创建代理或 tunnel。

`webcodex connect <server> --auth oauth --oauth-redirect-uri <精确回调地址>` 是 server-bound OAuth 路径。它要求该 Server 已存在 `webcodex login` 登录（同一 Server 有多个用户时用 `--user` 选择），从远端 discovery 读取当前 OAuth permission registry，为该 managed user 创建包含当前全部可委托 WebCodex permission scopes 的 OAuth client，并为本地 Runner 单独创建 Agent token。MCP 暴露哪些工具仍完全由远端 Server 当前配置的 model surface 决定。`agent:*`、`admin` 与协议级 `offline_access` 不进入 OAuth client permission allow-list；MCP 客户端仍可请求 `offline_access` 获取 refresh token。已有 profile 重连时不会因为 Server 新增 scope 而静默扩权。

`disconnect` 按 canonical 仓库路径匹配，不根据 basename 或 project id 猜测。如果同一仓库
注册在多个 hosted profile 中，必须显式指定 `--profile`。managed Runner 在线时，它先执行
带 fencing 的 structured unregister，再删除本地 registration；Runner 已停止时，只删除精确
匹配的本地项目 registration。其他项目、profile credential 和 `agent.toml` 都会保留。

接入 MCP coding client 后，可阅读 [Coding 工作流](CODING_WORKFLOW.zh-CN.md)，了解 canonical
`work_on_project` / `start_coding_task` bootstrap 心智模型、behavioral guidance、validation 与
closeout evidence。

### 设备接入

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex login <server-url> --code <wc_pair_...>` | 用 pairing code 把本机接入 Server | 主要客户端入口，写入 user token 与 `agent.toml`。 |
| `webcodex pairing create` | Server/admin 侧：创建短期 pairing code | 需要 server bootstrap/admin 认证。 |
| `webcodex client enroll` | 高级客户端接入，可显式指定 `--client-id` | 高级入口；普通用户应使用 `login`，它会自动派生 client id 并一步写入规范的 server/user 本地连接布局。 |
| `webcodex logout <server-url>` | 移除本机对某 Server 的凭据 | |

### Runner（`agent` 命名空间）

Runner 可执行文件是 `webcodex-runner`。对应 CLI 命名空间叫 `agent`（历史原因）：
`webcodex agent ...` 管理的就是 `webcodex-runner` 进程与服务。"Agent" 与 "Runner"
指同一个执行组件，但它们不是同一个程序：`webcodex`（包含 `agent` 命名空间）与
`webcodex-runner` 是两个独立可执行文件。

| 命令 | 用途 |
| --- | --- |
| `webcodex agent init` | 手动生成 `agent.toml` 配置 |
| `webcodex agent install` | 安装、启用并启动 Runner 服务 |
| `webcodex agent run` | 前台运行 `webcodex-runner` |
| `webcodex agent start` | 启动 hosted 后台 Runner 或已安装的 profile 服务 |
| `webcodex agent stop` | 停止 |
| `webcodex agent restart` | 重启 |
| `webcodex agent status` | 检查 Runner 生命周期、配置与连通性 |
| `webcodex agent logs` | 读取 Runner 日志（有界） |
| `webcodex agent uninstall` | 移除服务单元（需要 `--confirm`） |

服务命令接受 `--scope user|system`。非 root 用户默认 user scope；root 默认
system scope。`webcodex connect` 创建的 profile 在不传 `--scope` 时保持其
detached-process 行为。

### Server

| 命令 | 用途 |
| --- | --- |
| `webcodex server init` | 初始化或更新 Server env 文件（创建 bootstrap token） |
| `webcodex server install` | 安装 `webcodex-server` 的 systemd 服务 |
| `webcodex server run` | 前台运行 `webcodex-server` |
| `webcodex server start` / `stop` / `restart` | 控制已安装的服务 |
| `webcodex server status` | 检查 systemd、HTTP 可达性与构建版本 |
| `webcodex server logs` | 读取服务日志 |
| `webcodex server uninstall` | 移除服务单元 |

### 运维（只读操作检查）

| 命令 | 用途 |
| --- | --- |
| `webcodex ops status` | 汇总 runtime、工具、任务、agent 与项目 |
| `webcodex ops agents` | 简洁的 agent 队列状态 |
| `webcodex ops projects` | 项目清单与 smoke 适用性 |
| `webcodex ops smoke-preflight --project <id>` | 为某项目做 deploy smoke 预检 |

`ops` 命令只读。支持 `--server-url`、`--token-file`、`--env-file`、`--token`、
`--json` 与 `--strict`。优先使用 `--token-file`；`--token` 容易泄露到 shell
历史或进程列表。`--strict` 会让 FAIL 报告以状态码 2 退出。

### 审查（本地决策）

| 命令 | 用途 |
| --- | --- |
| `webcodex task list` | 列出当前项目的近期任务 |
| `webcodex task show <id>` | 显示任务的结果、审批与时间线 |
| `webcodex task accept <id>` | 把已审查的结果应用到 checkout |
| `webcodex task reject <id> [reason]` | 拒绝稳定结果；reason 会到达模型 |
| `webcodex task resume <id>` | runtime 重启后恢复保留的运行 |
| `webcodex task guide <id> <message>` | 向运行中的任务发送纠偏指引 |
| `webcodex task approve <id> <approval> [reason]` | 批准某条原始命令使用一次 |
| `webcodex task deny <id> <approval> [reason]` | 拒绝；reason 会显示给模型 |
| `webcodex task activity` | 显示近期变更性工具执行（workspace ledger） |

`task` 命令默认作用于当前项目；可用 `--root PATH`、`--profile NAME` 或
`--state-dir PATH` 指向其他项目。Accept 与 Reject 是人工在本地应用或丢弃 coding
结果的两种方式——在线模型永远不能接受自己的工作。

### 凭据与账号

Admin 用户/令牌操作由 Server API 支撑；`auth status` 读取本机连接状态，而
`create-local` 命令在本地生成凭据，只向 Server 注册其 hash。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex auth status` | 显示本机已登录哪些 Server | 只读；支持 `--dir` 与 `--json`。 |
| `webcodex users create` | 创建用户；`--issue-credential` 返回一次性 account credential | Server/admin 侧；使用 `--server-url`。 |
| `webcodex users list` | 列出用户 | |
| `webcodex token create-local` | 本地生成 `wc_pat_*` 个人 API 令牌并注册其 hash | 使用 `--server` 与 account credential。 |
| `webcodex tokens create` | Admin：在服务端创建 PAT | 使用 `--server-url`。 |
| `webcodex token generate` | 离线生成令牌素材 | **不会**在 Server 注册。 |
| `webcodex tokens list` / `revoke` / `register-hash` | 列出或撤销 PAT；注册外部计算的 hash | Admin 侧；使用 `--server-url`。 |
| `webcodex agent-token create-local` | 本地生成 `wc_agent_*` Runner 令牌并注册其 hash | 绑定 `--client-id`。 |
| `webcodex agent-tokens create` / `list` / `revoke` / `register-hash` | Admin 变体 | |

注意 flag 差异：`users create` 以及 admin 的 `tokens`/`agent-tokens` 命令使用
`--server-url`；本地的 `token create-local` 与 `agent-token create-local`
使用 `--server`。

### 高级与兼容命令

以下命令覆盖不常见场景；上面的推荐路径才是常规入口。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex client enroll` | 显式指定 `--client-id` 的高级接入 | 其 help 说明：高级用法；优先用 `webcodex login`，它会派生 client id 并一步写入规范的 server/user 本地连接布局。 |
| `webcodex pairing create` | Server/admin 侧：创建短期 pairing code | 需要 server bootstrap/admin 认证。 |
| `webcodex token generate` | 离线生成令牌素材 | 不注册任何东西；若需要服务端注册 hash，把输出配 `tokens register-hash` 使用。 |
| `webcodex tokens register-hash` | Admin：注册外部计算的 PAT hash | 使用 `--server-url`；用于离线生成的素材。 |
| `webcodex agent-tokens register-hash` | Admin：注册外部计算的 Runner 令牌 hash | 使用 `--server-url`；用于离线生成的素材。 |
| `webcodex setup single-user` | 遗留的单用户 bootstrap 流程 | 不是常规路径。 |

## 术语

### 人员与机器

- **Server** —— 负责认证调用方并路由工具请求的 `webcodex-server` 进程。
- **CLI** —— 本文档介绍的 `webcodex` 命令。
- **Runner** —— 运行在持有仓库机器上的 `webcodex-runner` 进程，执行实际工作。
- **Agent / agent CLI 命名空间** —— `webcodex agent ...` 管理的就是 Runner。
  "Agent" 与 "Runner" 指同一个执行组件，而不是同一个程序：`webcodex` 与
  `webcodex-runner` 是两个独立可执行文件。"agent" 一词来自旧的 `webcodex-agent`
  名称。
- **profile** —— 用户 WebCodex 配置目录下的一个命名客户端配置（路径、
  `agent.toml`、令牌）。`webcodex connect` 会创建一个；
  `webcodex agent ... --profile <name>` 指向它。
- **client_id** —— 一个 Runner/设备的稳定逻辑标识（如 `workstation` 或
  `alice-macbook`）。它是 runtime project id 的一部分，也是 Runner 令牌所绑定
  的对象。
- **agent_instance_id** —— `webcodex-runner` 启动时生成的进程级身份，整个进程
  生命周期（包括 WebSocket 重连）复用。Server 把它当作活跃租约身份：同
  `client_id` 但不同 `agent_instance_id` 的第二个进程在第一个在线时会被拒绝，
  过期/被替换的实例不能再 poll 或提交结果。它不是 secret。
- **Connector** —— 已配置本地项目暴露出的 project-bound coding surface。
  Connector 把一个逻辑项目绑定到其注册的执行器，因此模型无需管理 project id。

### 项目与工作

- **project_id** —— agent 在其 `projects.d` 注册表中注册的项目 id。
- **runtime project id** —— 完整的 `agent:<client_id>:<project_id>`，用于定位
  已注册项目。project-bound Connector 内部会解析它；普通用户不需要输入。
- **Task** —— 模型创建、人工审查的一个有界项目工作单元。Task 有稳定 id
  （`task_...`）与审查结果。
- **Job** —— 在发起调用返回后仍继续运行的长命令或校验。Job 有 id 与有界日志，
  可以停止。
- **Workflow Session** —— 运维 runtime 中一个长期 coding 会话的有界证据账本。
  Connector 用户不需要管理 session id；其连续性来自 project-bound task context。
- **request / operation id** —— 模型使用的关联与重试标识（`operation_id`、
  `request_id`、`execution_id`、`result_id`）。它们属于内部机制，普通用户无需管理。

## 凭据：我到底需要哪个令牌？

WebCodex 把 bootstrap 管理、账号接入、runtime API 访问与 Runner 连接分开。
不要跨 surface 复用同一凭据。完整模型见
[AUTH_MODEL.md](AUTH_MODEL.zh-CN.md)；下表是快速答案。

| 凭据 | 前缀 | 由谁创建 | 用途 | 不要用于 |
| --- | --- | --- | --- | --- |
| Server bootstrap token | （env `WEBCODEX_TOKEN`） | `webcodex server init` | server/admin 设置、建用户、pairing | GPT Actions、MCP、Runner、日常使用 |
| 共享 key | `wck_...` | `webcodex connect`（一次性生成） | hosted shared-key 的 MCP + Runner | 生产 IAM |
| Project Credential | （私有文件） | `webcodex setup` | 单个项目的 Connector + Agent | 其他项目、admin |
| Account credential | `wc_acct_...` | `webcodex users create --issue-credential` | 本地创建令牌 | GPT Actions、MCP、agent |
| 个人 API 令牌（PAT） | `wc_pat_...` | `webcodex token create-local` | GPT Actions、MCP、REST API | Runner 连接 |
| Runner 令牌 | `wc_agent_...` | `webcodex agent-token create-local` | 仅 `webcodex-runner` 传输 | MCP、REST、GPT Actions |
| OAuth 访问令牌 | `wc_oat_...` | OAuth2 授权流程 | 启用 OAuth 时的 GPT Actions / MCP | — |

### Hosted 共享 key（`wck_...`）

- 未提供 `--key` 或 `--key-file` 时由 `webcodex connect` 生成。
- 仅在首次创建时完整显示；profile 保存后，重复 `connect` 会复用而不再次打印。
- 保存在 owner-only profile 配置下：
  `~/.config/webcodex/clients/<profile>/agent.toml`（或
  `$XDG_CONFIG_HOME/webcodex/clients/<profile>/agent.toml`），即顶层
  `token = "wck_..."` 字段。
- 如需人工恢复该值，请复制那个 `token` 字段。status 与 log 命令故意不打印它。
  不存在 `show-token` 命令。AI agent 应定位 profile 并把文件位置告诉人类，而不是
  回显该值。
- 重复 `connect` 复用 profile 且不再次打印 key。
- 不要把 `wck_` 当作 managed `wc_*` 使用；shared-key 认证永远不会回退到
  managed identity。

### Project Credential

- 由 `webcodex setup` 为选定的 Git root 与 profile 创建，保存在 owner-only
  私有文件中（Connector credential 文件与生成的 Agent 配置）。
- 丢失时请恢复两个匹配的私有文件。不存在就地 rotate 命令；若无法恢复，请停止
  runtime 并显式重建私有 project-state profile（这也会同时作废该 profile 的本地
  task 历史）。

### `WEBCODEX_TOKEN`

- Server bootstrap/admin 凭据，由 `webcodex server init` 创建，存储在 server
  env 文件中（通常 `/etc/webcodex/webcodex.env`），变量名为 `WEBCODEX_TOKEN`。
- 它不是 MCP、Runner 或日常令牌。只用于初始设置、建用户、pairing 与紧急管理。
- 如需确认 env 文件与变量名，请查看 Server 服务单元或服务加载的 env 文件。
  读取令牌值属于会泄露密钥的人工操作；不要粘贴到客户端或提交。

### `wc_pat_*`（个人 API 令牌）

- 由 `webcodex token create-local` 本地生成的 managed user token；Server 只存
  hash。
- `webcodex login` 会把它写入该 server/user 登录目录下名为 `webcodex-user-token`
  的文件。
- 用 `--token-file <path>` 提供给命令，而不是 `--token`，避免进入 shell 历史。
- 如需粘贴到 MCP 客户端，只读取那一个 `webcodex-user-token` 文件，不要回显整个
  配置文件。

### `wc_agent_*`（Runner 令牌）

- 由 `webcodex agent-token create-local` 本地生成并绑定 `client_id` 的 Runner
  传输令牌。
- `webcodex login` 只会把它**内联**写进生成的 `agent.toml`（位于
  `~/.config/webcodex/<server-slug>/<user>/`）——不会创建单独的
  `webcodex-runner-token` 文件。高级的 `webcodex client enroll` 流程（以及遗留的
  `webcodex setup single-user` 流程）会额外在 `webcodex-user-token` 旁边写入一个
  `webcodex-runner-token` 文件。
- 只被 Runner 传输 endpoint 接受；用在 MCP/REST 上会返回 403。不要当作
  MCP/API 令牌。

### `wc_pair_*`（pairing code）

- 由 `webcodex pairing create` 在服务端创建的短期一次性 code。
- 只把该 code 传给要接入的客户端；客户端用 `webcodex login <server-url> --code <code>`
  兑换。
- 它不是长期 API 令牌，过期后无法使用。

### OAuth

当 Server 启用 OAuth 时，MCP/GPT 客户端可以使用 authorization-code 流程而非
静态 PAT。client id、client secret 与 `wc_oat_*` 访问令牌属于委派凭据；见
[AUTH_MODEL.md](AUTH_MODEL.zh-CN.md#oauth2) 与 [MCP.md](MCP.zh-CN.md#oauth2)。

## 常用示例

Project-first 工作流：

```bash
webcodex setup
webcodex doctor
webcodex run          # 保持该终端打开
webcodex status       # 在另一个终端
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

Hosted 接入：

```bash
webcodex connect https://your-server.example
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
```

Managed 接入：

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex agent install --scope user --config <login-reported-agent-config>
webcodex agent status --scope user --config <login-reported-agent-config>
webcodex ops status --server-url https://your-server.example \
  --token-file <login-reported-webcodex-user-token> --strict
```

## 代理与网络

CLI 请求默认遵循标准代理环境变量（`HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`、
`NO_PROXY`）。用 `--proxy http://HOST:PORT` 为单次调用覆盖，或用
`--no-system-proxy` 忽略代理环境直连。这些 flag 只影响 CLI 自身的 HTTP 请求；
`webcodex connect` 不会把它们持久化或注入 Runner 配置。
