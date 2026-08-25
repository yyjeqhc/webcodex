# 认证和凭据模型

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex 把 bootstrap 管理、账号接入、runtime API 访问与 Runner 连接分开。不要
在所有 surface 上复用同一种凭据。

## 凭据总览

| 凭据 | 前缀 | 由谁创建 | 用途 | 不要用于 |
| --- | --- | --- | --- | --- |
| Server bootstrap token | `WEBCODEX_TOKEN`（env） | `webcodex server init` | server/admin 设置、建用户、pairing | GPT/MCP/agent 日常使用 |
| Project Credential | （私有文件） | `webcodex setup` | 精确访问单个私有项目授权 | 其他项目/admin/通用 quick start |
| 共享 key | `wck_...` | `webcodex connect`（一次性生成） | hosted shared-key 的 MCP + Runner | 生产 IAM |
| Account credential | `wc_acct_...` | `webcodex users create --issue-credential` | 本地创建令牌 | GPT/MCP/agent |
| 个人 API 令牌（PAT） | `wc_pat_...` | `webcodex token create-local` | GPT Actions、MCP、runtime API | Runner 连接 |
| Runner 令牌 | `wc_agent_...` | `webcodex agent-token create-local` | 仅 Runner 传输 | GPT/MCP/runtime/project API |
| OAuth 访问令牌 | `wc_oat_...` | OAuth2 授权流程 | 启用 OAuth 时的 GPT Actions / MCP | — |

"我需要哪个令牌？"的快速答案见 [CLI.md](CLI.zh-CN.md#凭据我到底需要哪个令牌)。
本页详细介绍每种凭据以及如何恢复或轮换。

## `WEBCODEX_TOKEN`

`WEBCODEX_TOKEN` 是 Server bootstrap/root/admin 凭据。它由 `webcodex server init`
创建，存储在 server env 文件（通常 `/etc/webcodex/webcodex.env`）的
`WEBCODEX_TOKEN` 变量中，用于首次建用户、pairing 与紧急管理。

不要把它放进 GPT Actions、MCP 客户端或日常 agent 配置。

**恢复 / 轮换：** 若可能泄露请轮换：在 server env 文件中重新生成该值并重启
Server。env 文件只在 server 侧，不应复制到客户端机器。

## Project Credential

`webcodex setup` 为选定的 Git root、profile 与私有状态目录创建一把 Project
Credential。Connector credential 文件与生成的 Agent 配置携带同一 secret；精确
校验会把两个调用方映射到同一个稳定、非 secret 的 `project_grant_id`。Agent
registry 访问、就绪、文件操作、jobs、日志与取消都需要该 grant。

该 secret 只存在于 owner 保护的私有文件中。它不会写入数据库、不通过就绪接口
返回、不出现在 Browser JSON、日志与错误中。runtime 只持有其 SHA-256 校验值，
并以常数时间比较候选 hash。

Project 模式不是 shared-key quick start。它显式禁用直接 unknown-token fallback，
只接受配置好的凭据。因此任意非空 Bearer token 会收到 `401`，无法创建 Task、
Execution、binding 或 Agent 请求。loopback 上也同样如此。

**恢复 / 轮换：** setup 不会静默轮换存活的 Project Credential。可恢复丢失时，
请恢复匹配的 Connector 与 Agent 私有文件。若 secret 无法恢复，先停止 runtime，
再显式作废整个私有 project-state profile，然后重新运行 setup；这会创建新 secret，
同时作废该 profile 的本地 task/execution 历史。不存在就地 rotate 命令。

## 共享 key（`wck_...`）

共享 key 是 `webcodex connect` 生成的 quick-start secret，以
`Authorization: Bearer <KEY>` 提供给 MCP/Runner。同一把去空格后的 key 可以同时
认证 MCP/runtime 客户端与本地 Runner。WebCodex 以 `shared_key_hash =
SHA-256(trimmed key)` 对两端分组：相同的值看到自己的 Runners、项目与 Jobs；
不同的值形成隔离的轻量组。

key 只在首次创建时完整打印，然后保存在 owner-only profile 配置下：
`~/.config/webcodex/clients/<profile>/agent.toml`（或
`$XDG_CONFIG_HOME/webcodex/clients/<profile>/agent.toml`）的顶层
`token = "wck_..."` 字段中。重复 `connect` 复用 profile 且不再打印。如需人工
恢复该值，请复制那个 `token` 字段；status 与 log 命令故意不打印它。不存在
`show-token` 命令。AI agent 应定位该文件并把位置告诉人类，而不是回显该值。

共享 key 不是 admin 凭据、不是 managed user identity，也不是生产 IAM。它没有
独立的按设备撤销能力：请轮换整个组的共享 secret，或改用 managed 凭据。

`WEBCODEX_SHARED_KEY_ENABLED=true` 在普通 server 上启用直接 Bearer shared-key
fallback。managed `wc_*` 值与空/空白 Bearer 值永远不会回退到 shared-key 模式。
`WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE` 是 OAuth authorize 页面的独立 flag，不启用
直接 Bearer fallback。

## `wc_acct_xxx`（account credential）

`wc_acct_xxx` 在管理员用 `--issue-credential` 创建用户时一次性签发。用户用它
本地执行：

```bash
webcodex token create-local
webcodex agent-token create-local
```

这两个命令本地生成明文令牌，并只向 server 注册令牌 hash。

`webcodex token generate --kind api|agent` 是离线原语：打印令牌与 hash，但不会
注册任何东西。其输出在通过 managed credential flow 注册 hash 前无法认证。不要把
离线生成的 `wc_pat_*` 或 `wc_agent_*` 当作 hosted 共享 key。

不要把 `wc_acct_xxx` 用作 GPT Action token、MCP token、runtime API token 或
agent 连接 token。

## `wc_pat_xxx`（个人 API 令牌）

`wc_pat_xxx` 是用户本地生成的个人 API 令牌；server 只存其 hash。`webcodex login`
会把它写入该 server/user 登录目录下名为 `webcodex-user-token` 的文件。

`wc_pat_xxx` 用于：

- GPT Actions
- MCP
- Runtime API 调用
- 工具调用如 `/api/tools/list` 与 `/api/tools/call`

用 `--token-file <path>` 提供给 CLI 命令，而不是 `--token`，以免进入 shell 历史。
按工作流限制 PAT scope。例如，一个要检查和编辑项目的 GPT Action 可能需要
`runtime:read`、`project:read`、`project:write`、`job:run`；只有需要 post、resolve、
complete、replace、withdraw 或 close Workflow Session 协作状态时才额外授予
`session:collaborate`。仅有 `runtime:read` 对这类协作状态保持只读观察能力。

## `wc_agent_xxx`（Runner 令牌）

`wc_agent_xxx` 是用户本地生成的 Runner 令牌；server 只存其 hash，并绑定
`allowed_client_id`。只用于 `webcodex-runner` 连接。它不能调用 runtime、project、
tool、MCP 或 account endpoint。

`webcodex login` 只会把它**内联**写进生成的 `agent.toml`
（`~/.config/webcodex/<server-slug>/<user>/agent.toml`），不会创建
`webcodex-runner-token` 文件。高级的 `webcodex client enroll` 流程（以及遗留的
`webcodex setup single-user` 流程）会额外在 `webcodex-user-token` 旁边写入一个
`webcodex-runner-token` 文件。本地能诊断时会把 `wc_agent_*` 用于
user/runtime CLI 令牌标记为错误，且服务端仍返回 403。

## `wc_pair_xxx`（pairing code）

`wc_pair_xxx` 是由 `webcodex pairing create` 在服务端创建的短期一次性 pairing
code。只把该 code 传给要接入的客户端；客户端用 `webcodex login <server-url> --code <code>`
兑换。它不是长期 API 令牌，会过期。

## OAuth2

启用 OAuth2（`WEBCODEX_OAUTH2_ENABLED=true`）后，GPT Actions / MCP 客户端可以
使用 authorization-code 流程并获得委派的 `wc_oat_*` 访问令牌。OAuth 凭据有各自
的角色：

- **client id** —— 标识 OAuth client（`wc_client_...`）。
- **client secret** —— client 创建时只返回一次；服务端只存其 hash。
- **access token**（`wc_oat_*`）—— 同意后签发，委派自授权用户的 scopes。
- **refresh token** —— Authorization Server Metadata 会把 `offline_access` 作为协议级
  refresh-token scope 发布；它不授予额外 WebCodex 权限，不应写进 client 的
  `allowed_scopes`。

Server 支持 authorization-code grant、token 撤销与 OAuth metadata。动态 client
注册、OIDC、JWKS/JWT ID token 与 device-code 流程未实现。OAuth 设置步骤见
[部署指南](DEPLOYMENT.zh-CN.md#oauth2)。

OAuth client 的 `allowed_scopes` 是 client 注册时确定的委派权限上限；WebCodex 后续
增加 `session:collaborate`、`computer:control` 之类的新权限时，不会自动给历史 client 扩权。first-party
operator 可以通过 `POST /api/oauth/clients/update_scopes` 显式替换 active client 的
完整 allowlist。allowlist 真正变化时，Server 会在同一事务里撤销该 client 现有的
access token、refresh token 与尚存 authorization code，因此 client 必须重新完成
OAuth 授权，才能使用新的 scope 集合。

`webcodex connect <server> --auth oauth` 是普通 hosted shared-key OAuth bridge。OAuth client 归属于 direct shared key 的 SHA-256 group hash，authorization code、access token、refresh token 都保留同一个 `shared_key_hash` subject binding。direct shared-key bearer authority 始终保持显式 baseline：`runtime:read`、`session:collaborate`、`project:read`、`project:write`、`job:run`、`computer:read`、`computer:control`。fresh ordinary OAuth client 从完整 baseline 开始；已有受保护 client 则可以合法持有该 baseline 的任意 non-empty、unique subset。只有 connect 显式传 `--oauth-computer-permissions`，OAuth client ceiling 才在**现有 baseline subset**上追加固定 closed set：`computer:launch`、`computer:display_read`、`computer:pointer_control`、`computer:clipboard_read`、`computer:clipboard_write`；绝不会恢复此前缺失的 baseline scope。Computer-enabled ceiling 只有在完整包含这五个 optional scopes、至少保留一个 baseline scope，且不存在两个显式 closed universe 之外的 scope 时才合法。该集合绝不从全局 OAuth registry 自动派生，也永远不包含 `account:manage`、`admin`、`job:detach`、任何 `agent:*` transport scope 或未来新增 scope。client ceiling 不是实际 grant：WebCodex authorize 页面五项 optional Computer permission 默认全部未勾选，authorization code 只包含本次 request 中实际授予的 baseline scope、用户明确选择后由固定 bundle 映射的 optional scope，以及本次协议需要的 `offline_access`。access token 与 refresh rotation 逐字保留该实际 grant，不重新扩到 client ceiling。Launch selection 要求本次 request 同时包含 `computer:read` 与 `computer:launch`，但 launch permission bundle 自身仍只新增 `computer:launch`；其他 optional permission 同样要求其完整 runtime request prerequisite，Server 不会替 client 偷偷补 request 未包含的 scope。普通 reconnect 不会扩已有 baseline client；revoked/missing client replacement 会按 `previous_allowed_scopes` 保留受保护 baseline subset。只有显式 opt-in 真正改变 shared-key-owned client ceiling 时，才原子撤销其 access/refresh token 与未使用 authorization code，要求重新完成浏览器授权。Runner 继续使用 direct shared key，OAuth access token 永远不能用于 Agent transport；`--auth managed-oauth` 仍是独立 managed-user 流程。

shared-key authorize 页面只对 exact shared-key-owned client 做 optional Computer picker，并要求 `owner_shared_key_hash` 与提交的 shared key 精确匹配、对应 Runner group 在线。每个 permission 只有在**同一个在线 Runner**同时满足完整 capability requirement 时才显示 available，绝不把多个 Runner 的 capability 做 union；POST 会重新计算，因此 GET 后 capability 消失会 fail closed 且不创建 code。这里的 capability 只代表 WebCodex backend 当前支持，不代表 OS/native permission 或操作一定成功；authorize 过程中不会执行隐藏 display observation、pointer/clipboard effect、launch 或 OS-permission probe，runtime tool call 仍负责最新 native/OS preflight。

permission ID 与 grant bundle 是 closed mapping：`launch -> computer:launch`，`display -> computer:display_read`，`pointer -> computer:display_read + computer:pointer_control`，`clipboard_read -> computer:clipboard_read`，`clipboard_write -> computer:clipboard_write`。picker 会另外检查完整 runtime request prerequisite：launch 要求 `computer:read + computer:launch`，display 要求 `computer:read + computer:display_read`，pointer 要求 `computer:read + computer:control + computer:display_read + computer:pointer_control`，clipboard read/write 分别要求 baseline read/control 与对应 optional scope。这样 browser consent、token scope、OAuth `tools/list` projection 与 `tools/call` runtime gate 保持一致。OAuth caller 的 `tools/list` 复用同一 runtime scope policy 隐藏 token 实际 scope 不足的高权限工具；直接伪造 `tools/call` 仍会再次执行 scope gate。

`mcp:local` 是内建 local MCP gateway 的独立显式 authority，不属于 direct shared-key、Project Credential、open-anonymous 或冻结的 legacy OAuth 默认权限。hosted shared-key OAuth 只有在 `connect` 显式传 `--oauth-local-mcp` 时才会加入该 scope，已有 client ceiling 永远不会被普通升级静默扩大。该 scope 有意表示“同一 shared-key Runner group 内当前及未来配置的 local MCP provider”这一 class-level authority，从而避免 per-provider/per-instance OAuth。ceiling 真正变化会复用现有原子 grant revoke 路径，因此旧 access/refresh token 与 authorization code 不会继承新权限。

MCP Protected Resource Metadata 会明确省略 `scopes_supported`，因为不同的预注册 client
可能具有不同的委派权限上限。MCP client 因此可以在授权请求中省略 `scope`；WebCodex
随后会默认采用该 client 已注册的 `allowed_scopes`。OAuth Authorization Server Metadata
仍会发布 `account:manage`、`offline_access` 等 server-level capability。

## `client_id`

`client_id` 是一个 Runner/设备的稳定逻辑标识，例如：

```text
ubuntu-client
alice-macbook
ci-runner-1
```

Runner 令牌绑定 `allowed_client_id`，防止为某 client 签发的令牌以不同 client
注册。

## `agent_instance_id`

`agent_instance_id` 标识一个存活的 Runner **进程**——`client_id` 的一次具体
化身。`webcodex-runner` 启动时生成它，并在整个进程生命周期（包括 WebSocket
重连）中复用。Server 把它当作活跃租约身份：同 `client_id` 但不同
`agent_instance_id` 的第二个进程在第一个在线时会被拒绝，过期/被替换的实例
不能再 poll 或提交结果。它不是 secret。

简言之：`client_id` 是稳定的逻辑 Runner/设备身份（runtime project id、令牌
绑定）；`agent_instance_id` 是用于传输、恢复与 fencing 的进程级化身。

## Runtime project id

Agent-backed runtime project id 形如：

```text
agent:<client_id>:<project_id>
```

示例：

```text
agent:ubuntu-client:webcodex
agent:alice-macbook:my-repo
```

`<project_id>` 来自 agent `projects.d/*.toml` 文件的顶层 `id` 字段：

```toml
id = "webcodex"
path = "/srv/webcodex/projects/webcodex"
```

不要在 agent `projects.d/*.toml` 文件中使用服务端 `[projects.<id>]` 语法。

## Hash 存储

对用户创建的 PAT 与 Runner 令牌，server 存令牌 hash，而不是明文 `wc_pat_xxx`
或 `wc_agent_xxx`。明文令牌只在创建时显示一次，需要由用户或 agent 主机保存。

## 每种凭据存放位置

| 凭据 | 默认位置 |
| --- | --- |
| `WEBCODEX_TOKEN` | server env 文件（`/etc/webcodex/webcodex.env`） |
| 共享 key `wck_...` | `~/.config/webcodex/clients/<profile>/agent.toml` 顶层 `token` 字段（owner-only） |
| Project Credential | 项目私有状态目录（owner-only 文件） |
| `wc_acct_...` | `users create --issue-credential` 一次性提供 |
| `wc_pat_...`（`webcodex-user-token`） | `~/.config/webcodex/<server-slug>/<user>/webcodex-user-token` |
| `wc_agent_...` | 内联在 `~/.config/webcodex/<server-slug>/<user>/agent.toml` |

`~/.config/webcodex/` 下每个 (server, user) 的目录布局：

```text
~/.config/webcodex/
  <server-slug>/
    <user>/
      server.toml               规范 server URL、用户名、设备
      agent.toml                agent token 内联在此
      webcodex-user-token
      projects.d/
```

Server 身份是规范 URL，不是目录名；slug 只是给人类看的、可能有损的索引。当 AI
agent 需要凭据值时，请让它告诉人类具体该复制哪个文件，而不是把文件内容回显到
聊天里。
