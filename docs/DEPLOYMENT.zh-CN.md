# 部署指南

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

本文档说明当前 WebCodex 的生产部署形态：server bootstrap、service 安装、agent 配置、GPT Actions、MCP 以及 smoke checks。

## 组件

- `webcodex`：公开统一 CLI，用于项目工作流、server/agent 生命周期、enrollment 和运维。
- `webcodex-server`：服务器进程，暴露 REST、GPT Actions OpenAPI、MCP 和 agent endpoints。
- `webcodex-runner`：长驻 worker，通过 `auto` transport 连接（先 QUIC，再 WebSocket，再 polling），也可显式指定单一 transport。

## 第一次部署先看这里

首次部署不需要先配置 OAuth、QUIC 或 account credential flow。最小生产路径是：

1. 准备一台带 systemd、`sudo` 和公网 HTTPS 域名或可信隧道的 Linux x64 主机。
2. 安装 `@yyjeqhc/webcodex`，运行 `webcodex server init`，再安装
   `webcodex-server` service。
3. 配置 reverse proxy，并把 `WEBCODEX_PUBLIC_URL` 设为准确的公网 HTTPS origin。
4. 在 server 上创建短期 pairing code，在持有代码仓库的机器上运行
   `webcodex login <server-url> --code <code>`（高级替代：`webcodex client enroll`）。
5. 在该代码机器上安装 `webcodex-runner` service。
6. 执行 `webcodex ops status --strict`；通过后再导入 GPT Actions schema 或添加
   MCP connector。

只想在同一台机器试用、不需要长期 service 或公网 ingress 时，请走主 README 的
project-first 路径。本文后续提供完整生产命令、可选 OAuth、多用户 enrollment 和
transport 细节。

## 服务器配置

生产环境通常需要这些配置：

```text
WEBCODEX_TOKEN=<bootstrap-admin-token>
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/var/lib/webcodex
```

`WEBCODEX_PUBLIC_URL=https://your-domain.example` 在 `server init` 阶段是可选的，因为这时你可能还不知道最终 HTTPS 域名。但在连接 GPT Actions、MCP 客户端、远程 agent 或任何面向用户的 OpenAPI flow 之前必须配置它；否则 runtime status 和 OpenAPI server URL 可能会指向错误地址。

`WEBCODEX_TOKEN` 只用于初始设置和管理操作。日常 GPT Actions 与 MCP 调用应使用用户 API token；agent 应使用 agent token。

## OAuth2

OAuth2 默认关闭。启用后，GPT Actions / MCP 客户端可通过 authorization code 流程获取委托的 `wc_oat_*` access token：

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

`WEBCODEX_OAUTH2_ISSUER` 优先于 `WEBCODEX_PUBLIC_URL` 用于 `/.well-known/*`
元数据中的端点 URL。生产环境请将两者都设为公开 HTTPS 域名，使 discovery
公布的 authorize/token/revocation 端点可被客户端访问，并让 authorize
session cookie 标记 `Secure`。

### 创建 OAuth client

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"ChatGPT Action","redirect_uris":["https://example.com/oauth/callback"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

请妥善保存响应中的 `client_secret` —— 它只返回一次，数据库只存其
SHA-256 哈希。省略 `allowed_scopes` 则授予完整可委托 OAuth scope 集合
（`runtime:read project:read project:write job:run account:manage`）。

使用 `POST /api/oauth/clients/list` 与
`POST /api/oauth/clients/revoke`（body `{"client_id":"wc_client_..."}`）
列出与撤销 client。撤销 client 会同时撤销该 client 下所有有效的 access
token、refresh token 与 authorization code。

### 浏览器 authorize 流程

将客户端指向 `https://your-domain.example/oauth/authorize?...`。在没有
Bearer token 且没有 session cookie 时，WebCodex 渲染一个最小登录页；输入
WebCodex PAT 即可获得 10 分钟的 `HttpOnly` session cookie，随后在 consent
页确认。点击 `Allow` 会重定向回注册的 `redirect_uri` 并携带 `wc_oac_*`
code；在 `POST /oauth/token` 用它换取 `wc_oat_*` access token。非浏览器
客户端仍可用 first-party Bearer PAT 走 `/oauth/authorize` 直接签发
authorization code 的路径。Bootstrap token 可以创建 OAuth client，但因为
没有 user id，不能完成 authorize。

完整的端到端 smoke test 演练（启用、创建 client、authorize、换 token、
撤销）见 [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md)。

### 暂不支持

动态客户端注册、OIDC / `/.well-known/openid-configuration`、JWKS/JWT ID
token、`userinfo_endpoint`、`client_credentials` grant、device code 流程，
以及 MCP resource/audience 绑定均未实现。默认 client scope 集合可授予
完整可委托权限，便于自托管 GPT Action / MCP 使用；对不可信客户端请使用
收窄的 `allowed_scopes`。

## Server-first setup

推荐的分发路径是 npm thin installer/wrapper：

```bash
npm install -g @yyjeqhc/webcodex
```

npm wrapper 当前支持 `linux-x64`、`linux-arm64`、`darwin-arm64` 和 `win32-x64`。Windows x64 支持 CLI + Runner 连接远端 Linux Server；长期运行的 Windows Server/service 路径仍不支持。release checksum 由 OE 动态生成并写入最终 npm package，不再提交到源码树。

初始化 env 文件：

```bash
sudo webcodex server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

`server init` 会创建 env 文件，并把 bootstrap admin token 写入 `WEBCODEX_TOKEN`。它也会写入 server listen address 和 data directory 设置。它不会创建 `wc_pat_...` 用户 API token，也不会创建 `wc_agent_...` agent token。

运行一次性 admin CLI 命令时，可以在命令支持时传入 `--env-file /etc/webcodex/webcodex.env`，也可以显式传入 `--token "$WEBCODEX_TOKEN"`，或者先把 env 文件加载到当前 shell：

```bash
set -a
. /etc/webcodex/webcodex.env
set +a
```

安装并启动 systemd service：

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex server status --env-file /etc/webcodex/webcodex.env
```

兼容命令仍然可用：

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex setup single-user
```

新文档和自动化脚本应优先使用 `webcodex`。

## 二进制部署清单

Server：

1. 安装公开 `webcodex` CLI 和 `webcodex-server` binary。
2. 运行 `webcodex server init`。
3. 仅在替换旧 unit 时运行 `webcodex server install --overwrite`。
4. 运行 `sudo systemctl daemon-reload`。
5. 运行 `sudo systemctl enable --now webcodex`。
6. 运行 `webcodex server status`。

Server/admin：

7. 运行 `webcodex pairing create`。

Client：

8. 安装公开 `webcodex` CLI 和 `webcodex-runner` binary。
9. 由实际运行项目命令的普通账户执行
   `webcodex login <server-url> --code <code>`（高级替代：
   `webcodex client enroll`），不要使用 `sudo`。
10. 运行 `webcodex agent install --scope user --config <login 报告的 agent config 路径>`。
11. 运行 `webcodex agent status --scope user --config <login 报告的 agent config 路径>`。
12. 运行 `webcodex ops status --strict`。

`agent install` 会自行对所选 service manager 执行 reload、enable 和 start；普通
流程不需要 `sudo`，也不需要另行调用 `systemctl`。
`/etc/webcodex/webcodex.env` 只属于 server 侧。普通用户的 client files 默认位于
`$XDG_CONFIG_HOME/webcodex`（未设置时为 `$HOME/.config/webcodex`）；管理员有意
配置 system scope 时，profile 可以放在 `/etc/webcodex` 下。

## 账户凭据开通流程

如果部署不使用 pairing，可以使用下面的 account credential flow。本节命令统一使用 `https://your-domain.example` 占位符。

1. 使用 server env file 中的 `WEBCODEX_TOKEN` 启动服务器。它只是 bootstrap/root/admin 凭据。
2. 管理员运行 `webcodex users create --issue-credential` 创建用户，并把返回的 `wc_acct_xxx` 一次性发给该用户。这个路径的二进制帮助使用 `users create` 和 `--server-url`；`token create-local` 与 `agent-token create-local` 使用 `--server`。
3. 用户运行 `webcodex token create-local`，使用 `wc_acct_xxx` 在本地生成 `wc_pat_xxx`，服务器只登记其 hash。GPT Actions、MCP 和 runtime API 调用使用这个 PAT。
4. 用户运行 `webcodex agent-token create-local`，使用 `wc_acct_xxx` 和 `--client-id <client_id>` 在本地生成 `wc_agent_xxx`，服务器只登记其 hash。该 token 只用于 `webcodex-runner`。
5. 初始化 `webcodex-runner`，添加顶层 agent `projects.d/*.toml` 文件，启动 agent，然后验证 `runtime_status`、`projects/list` 和一个只读 `tools/call`，例如 `git_status`。

不要把 `wc_acct_xxx` 当作 GPT Action/MCP token，也不要把它写进 `agent.toml`。

## 邀请另一个用户

server owner 邀请朋友或其他 operator 时，应使用短期 pairing code。不要在机器之间复制长期凭据。

Server/admin 侧：

```bash
webcodex pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` 是 server/admin 侧操作。这个普通流程创建未绑定 code，由执行 `login` 的设备使用自动生成的 id 认领。`/etc/webcodex/webcodex.env` 只属于 server 侧。只把短期 `wc_pair_*` code 发给对方。

Client/friend 侧：

```bash
webcodex login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"

webcodex agent install --scope user \
  --config "$HOME/.config/webcodex/https_your-domain.example/friendname/agent.toml"
webcodex agent status --scope user \
  --config "$HOME/.config/webcodex/https_your-domain.example/friendname/agent.toml"

webcodex ops status \
  --server-url https://your-domain.example \
  --token-file "$HOME/.config/webcodex/https_your-domain.example/friendname/webcodex-user-token" \
  --strict
```

`webcodex login` 是 client/friend 侧入口：自动生成唯一设备名、兑换 pairing code，
并在用户 WebCodex config 目录下写入 client 侧 `webcodex-user-token` 和
`agent.toml`。login 会打印确切 config 路径和
`webcodex agent install --scope user --config <path>` 命令。GPT Actions、MCP 和普通
REST/project API 使用 client 侧 `webcodex-user-token`；生成的 agent config 只把
`webcodex-runner-token` 用于 Agent transport。不要在机器之间复制
`WEBCODEX_TOKEN`、`wc_pat_*`、`wc_agent_*`、完整 env files 或完整
`agent.toml` files。每个 friend 都应使用唯一的 `username`；设备名会自动唯一。
如需显式 client id 或自定义输出目录，高级的 `webcodex client enroll` 流程仍可用。

## Runtime console

WebCodex 在这里提供一个只读浏览器 console：

```text
https://your-domain.example/console
```

静态 console bundle 不包含 secrets。它从受保护 Connector API 获取共享 project
readiness projection，只显示 Project、Connection、Agent/coding readiness、
findings 和下一条 CLI action，不暴露 Agent registry 或 transport 细节。console
不属于 GPT Actions OpenAPI，也不是完整 admin UI。

### Runtime job API 信任模型

WebCodex runtime job API 面向可信单操作者部署。
`job_status`、`job_log`、`list_jobs` 和 `job_tail` 不是不可信用户之间的租户边界。
不要在未实现 project-less job API owner isolation 的情况下，把同一个 WebCodex runtime
暴露给多个互不信任的用户。对不可信用户应使用独立 server/runtime 实例。

## Public HTTPS URL

GPT Actions 需要 public HTTPS URL。WebCodex CLI 不会自动配置 reverse proxy 或 tunnel，所以在把 `/openapi.json` 导入 ChatGPT 之前需要先配置好对外 HTTPS。

在 server env 文件中设置同一个 public URL：

```text
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

最小 Nginx 示例：

```nginx
server {
    listen 80;
    server_name your-domain.example;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl http2;
    server_name your-domain.example;

    ssl_certificate /etc/letsencrypt/live/your-domain.example/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/your-domain.example/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto https;
    }

    location /api/agents/ws {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto https;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_buffering off;
    }
}
```

建议让 WebCodex 继续在 proxy 后面监听 `127.0.0.1:8080`。QUIC agent transport 与这个 HTTPS path 是分开的；打开 UDP 8443 前请先看 [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)。

## Agent 配置

`login` / `client enroll` 会生成 agent config。普通非 root 安装使用 user unit
（`login` 会打印确切的 config 路径）：

```bash
webcodex agent install --scope user --profile workstation
webcodex agent status --scope user --profile workstation \
  --server-url https://your-domain.example
```

npm 安装位置与 systemd scope 相互独立。user scope 使用 `systemctl --user`，unit
位于 `$XDG_CONFIG_HOME/systemd/user`（未设置时为
`$HOME/.config/systemd/user`），使用 `default.target`，不生成 `User=` 或
`Group=`，也不需要管理员权限。非 root 调用者默认使用 user scope。install、
status、start、stop、restart、logs 和 uninstall 必须使用同一 scope。

启用后的 user unit 跟随该账户的 user manager 生命周期；这并不自动保证它能在首次
登录前启动，或在最后一次注销后继续运行。若需要无人值守的开机常驻，管理员应先
评估该账户长期运行 service 的权限，再显式执行
`sudo loginctl enable-linger <runner-user>`。WebCodex 不会自动修改 linger 设置。

管理员管理的 service 使用 system scope：`/etc/systemd/system`、`systemctl` 和
`multi-user.target`。正常安装应指定非 root 用户及其可用的 working directory：

```bash
sudo webcodex agent install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webcodex/clients/workstation/agent.toml
sudo webcodex agent status --scope system --profile workstation
```

`--group` 可选。WebCodex 不创建系统用户、不修改 sudoers，也不迁移权限。只有
管理员同时传入 `--allow-root-runner` 时才允许 root Runner；这个不推荐的例外会
输出并写入醒目警告。显式 config 和 working-directory override 仍然可用，但
所选 service account 必须能够读取或使用它们。显式 service-file 必须匹配所选
manager scope：user scope 拒绝 system unit path，system scope 拒绝 user-unit
path；已有 unit 不会被静默覆盖。

前台测试可直接启动 agent：

```bash
webcodex-runner --profile workstation
```

高级手工初始化使用 `webcodex agent init`；重复的
`webcodex-runner init` alias 已删除。

重要 agent 设置：

| Setting | 说明 |
| --- | --- |
| `server_url` | WebCodex public URL。 |
| `token` | Agent token。不要提交或打印。 |
| `client_id` | 稳定 id，用于 `agent:<client_id>:<project_id>`。 |
| `owner` | 该 agent 的 owner principal。 |
| `transport` | 推荐配置 `[quic]` 并使用 `auto`：先 QUIC，再 WebSocket，再 polling。只有明确需要单一 transport 时才使用 strict `quic`、`websocket` 或 `polling`。 |
| `projects_dir` | 项目注册文件目录。 |
| `temporary_projects_root` | 可选的、已存在的 Runner 托管临时项目根目录；它会按 effective Runner path policy 校验（收窄部署时必须位于 `allowed_roots` 内）。 |
| `[policy]` | 本地执行边界。 |
| `[shell]` | 可选 shell profile 定义和有界 persistent-shell 限额。 |

Policy 行为：

- 缺失或为空的 `allowed_roots` 默认使用 `$HOME`。
- 显式 `allowed_roots` 会覆盖 `$HOME` 默认值。
- 需要收窄 agent 权限时，使用显式 roots，例如限制到某个 workspace tree。

示例窄权限 policy：

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

Persistent-shell 配置可省略，旧配置文件继续使用默认值：

```toml
[shell]
# 当前 Runner 进程拥有的活动长生命周期 Shell 数量，范围 1..64。
max_persistent_shells = 8
# 空闲回收秒数，范围 1..86400。
persistent_shell_idle_timeout_secs = 1800
```

空闲回收不会中断正在执行的命令。显式 close、Workflow Session close、项目
disable/unregister、Shell 自行退出、Runner 断连/退出或命令失去同步时也会释放
进程；Server 或 Runner 重启后不会恢复这些 Shell。

`projects_dir` 中的 agent project files 可以设置 `shell_profile = "rust"`，把项目绑定到已配置 profile。

Shell profiles 会为一次性命令准备 environment snapshot。显式 Session
persistent shell 则只在打开长生命周期进程时应用一次所选 profile，后续命令使用
该进程状态；两条路径默认都不会 source `.bashrc` 或 `.profile`。Rust/Cargo、
Python venv、Conda 示例、解析规则和安全边界见
[SHELL_PROFILES.md](SHELL_PROFILES.md)。修改配置不会静默移动、重启或改变已打开
Shell；需要显式 close/reopen 才应用新默认值。当前 policy 会在后续 exec/status
操作前重检，而 close 仍可用于清理。
修改 `agent.toml` 后，应 reload 匹配的 manager（user scope 使用
`systemctl --user reload webcodex-runner`，system scope 使用
`sudo systemctl reload webcodex-runner`）。这会把新的 policy、shell、SSH
resource 和 tool-provider generation 应用于后续请求；已打开的
persistent shell 仍保持原进程状态。无效 reload 保留当前 generation，
`projects.d` 继续独立刷新。

`runtime_status` 和 `listAgents` 会暴露 redacted policy summary，以及经过清理的 `shell_profiles` 摘要，包括 profile names、`has_init_script`、`env_keys_count`、`program`、`args_count`。`listProjects` 会暴露 `shell_profile`、`resolved_shell_profile` 和 `shell_profile_status`（`configured` / `missing` / `not_configured` / `unknown`）。这些接口不会暴露 tokens、env values、`Authorization` headers、完整 `agent.toml`、完整 env snapshot 或 shell profile `init_script` bodies。

## Authentication and transport

普通 REST、polling、MCP 和 GPT Actions 必须使用生成的
`webcodex-user-token`（`wc_pat_*`）：

```text
Authorization: Bearer <token>
```

`?token=` 只允许用于 `/api/agents/ws` WebSocket handshake 兼容场景。不要把 query-string token 用在 polling、REST、MCP 或 GPT Actions。

`webcodex-runner-token`（`wc_agent_*`）只允许访问 Agent transport endpoints；
project/runtime endpoint 仍会返回 403，不要绕过这条边界。

Agent 推荐配置 QUIC 并使用 `transport = "auto"`。WebSocket 和 polling 继续作为受限网络下的 fallback。

## GPT Actions 和 MCP

从这里导入 GPT Actions：

```text
https://your-domain.example/openapi.json
```

在 GPT Actions 中把认证配置为 HTTP Bearer/API key，并放在 `Authorization` header。

这里指基于 OpenAPI 的 Custom GPT Action 接入，并不声称 WebCodex 已发布到 ChatGPT
插件目录。plugin、app、Custom GPT 和 GPT Action 属于不同层次，详见
[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)。

OpenAPI GPT Actions 管理面有意排除 users、API tokens、agent tokens、pairing/enrollment、setup、doctor、npm、server management 和 audit endpoints。这些任务请使用 `webcodex`。

MCP 使用同一个用户 API token，并使用与 GPT Actions 相同的 `ToolRuntime`。

## Codex-specific workflows

WebCodex 不再暴露 `run_codex` 或 legacy `/api/codex/*` routes。GPT Actions 和 MCP clients 应使用 structured edit tools、patch validation、cargo validation、受限 `run_shell` / `run_job` escape hatches、`show_changes`、`workspace_hygiene_check` 和 `finish_coding_task`。需要 Codex-specific workflows 的 operator 应在 WebCodex 外部运行 Codex。

## Smoke checks

推荐的生产 smoke sequence：

1. `webcodex ops status --server-url https://your-domain.example --token-file PATH --strict` 通过只读检查。
2. `POST /api/runtime/status` 返回 `service=webcodex` 和预期 public URL。
3. `listAgents` 显示至少一个 online agent。
4. `listProjects` 显示 `agent:<client_id>:<project_id>` ids。
5. 已知项目上的只读 project tools 可用。
6. 写入、替换、验证类测试只在 disposable smoke projects 中执行。

## Troubleshooting

部署排障和运维检查清单见 [TROUBLESHOOTING.md](TROUBLESHOOTING.md)，包括已有 systemd services、`HTTP reachable: no`、client CLI 不在 `PATH`、server-side pairing 与 client-side enrollment、agent-only client warnings、`client online: no` 等常见问题。
