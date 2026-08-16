# 部署指南

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

本文档覆盖 WebCodex 的生产自托管：构建与安装二进制、bootstrap Server、接入
Runner 机器、连接 MCP/GPT 客户端以及 smoke 检查。最短的纯本地设置见
[快速开始](QUICK_START.zh-CN.md)。

## 组件

- `webcodex` —— 统一 CLI：项目工作流、Server/Runner 生命周期、接入与运维。
- `webcodex-server` —— Server 进程：暴露 REST、GPT Actions OpenAPI、MCP 与
  Runner endpoint。
- `webcodex-runner` —— 运行在持有仓库机器上的长驻 worker。

## 构建与安装

官方分发路径是 npm 薄安装器/包装器：

```bash
npm install -g @yyjeqhc/webcodex
```

支持 Linux x64、Linux arm64、macOS arm64 与 Windows x64。Windows x64 支持针对
远程 Linux Server 的 CLI + Runner 工作流；不支持长期运行的 Windows Server。
npm 包装器要求 Node.js 18 或更新。从 v0.3.5 起，Linux x64 native artifact 以
glibc 2.17 或更新为兼容基线。

从源码构建：

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

这会生成 `webcodex`、`webcodex-server`、`webcodex-runner`。

## 把仓库接入已有的 Server

hosted shared-key 路径不需要本地 Server、数据库、反向代理或 systemd unit：

```bash
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` 把当前目录作为项目，生成一把共享 key（`wck_...`，只打印一次），写入
owner-only profile，启动 detached Runner，并等待 Server 同时看到 Runner 与项目。
把输出的 `/mcp` URL 与 key 填入 MCP client。机器重启后，重新运行同一条
`connect` 或使用 `webcodex agent start --profile <profile>`。

自动化场景优先用 `--key-file <path>` 而不是 `--key`。不要同时传 `--key` 与
`--key-file`。

## 首次生产部署

初次运维者不需要 OAuth、QUIC 或 account credential。最小生产路径：

1. 使用带 systemd、`sudo` 与公网 HTTPS 域名（或受信 tunnel）的 Linux x64 主机。
2. 安装 `@yyjeqhc/webcodex`，运行 `webcodex server init`，安装
   `webcodex-server` 服务。
3. 配置反向代理并把 `WEBCODEX_PUBLIC_URL` 设为精确的公网 HTTPS origin。
4. 在 server 上创建短期 pairing code，并在持有仓库的机器上运行
   `webcodex login <server-url> --code <code>`。
5. 在该仓库机器上安装 `webcodex-runner` 服务。
6. 运行 `webcodex ops status --strict`；之后才导入 GPT Actions schema 或添加
   MCP connector。

### Server 设置

初始化 Server env 文件（创建 bootstrap `WEBCODEX_TOKEN` 与监听/数据设置）：

```bash
sudo webcodex server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env \
  --public-url https://your-domain.example
```

`server init` 只创建 server 侧 bootstrap/admin token。它不创建 user API token
或 agent token。

安装并启动 systemd 服务：

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex server status --env-file /etc/webcodex/webcodex.env
```

只有替换已有 unit 时才在 `server install` 上使用 `--overwrite`。

### 公网 HTTPS

Hosted MCP 客户端与 GPT Actions 需要公网 HTTPS URL。在 Server env 文件中设置
`WEBCODEX_PUBLIC_URL`，并在 `127.0.0.1:8080` 前面配置反向代理。支持 Nginx；
named Cloudflare Tunnel 也是有效入口。同一 hostname 必须承载普通 HTTPS 请求
与 `/api/agents/ws`（Cloudflare 支持 WebSocket upgrade）。WebCodex CLI 不会自动
配置反向代理或 tunnel。

### 接入仓库机器

在持有仓库的机器上，以将运行项目命令的普通用户身份执行（不要用 `sudo`）：

```bash
webcodex login https://your-domain.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex agent install --scope user \
  --config <login-reported-agent-config>
webcodex agent status --scope user \
  --config <login-reported-agent-config>
webcodex ops status --server-url https://your-domain.example \
  --token-file <login-reported-webcodex-user-token> --strict
```

`webcodex login` 是主要客户端入口：它自动派生唯一设备名、兑换 pairing code，并
写入客户端侧 `webcodex-user-token` 与 `agent.toml`。需要显式 client id 或自定义
输出目录时，`webcodex client enroll` 仍是高级替代方案。

pairing code 由 server/admin 侧创建：

```bash
webcodex pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --display-name "Friend Name" \
  --ttl-secs 600
```

只把短期 `wc_pair_*` code 传给客户端。不要跨机器复制 `WEBCODEX_TOKEN`、user API
token、agent token、env 文件或完整 `agent.toml`。每个用户使用唯一 `username`。

## Runner 服务 scope

`webcodex agent install` 支持 user 或 system scope。非 root 用户默认 user scope；
root 默认 system scope。

**User scope** 使用 `systemctl --user`，unit 写入
`$XDG_CONFIG_HOME/systemd/user`，配置放在 `$XDG_CONFIG_HOME/webcodex`，无需
`sudo`：

```bash
webcodex agent install --scope user --profile workstation
webcodex agent status --scope user --profile workstation
webcodex agent logs --scope user --profile workstation --lines 100
```

已启用的 user unit 跟随该账号的 user manager。如需无人值守的开机持久化，管理员
可显式运行 `sudo loginctl enable-linger <runner-user>`；WebCodex 不会自动修改
lingering。

**System scope** 使用 `/etc/systemd/system`，需要指定非 root `--user`：

```bash
sudo webcodex agent install \
  --scope system \
  --profile workstation \
  --user <runner-user> \
  --working-directory /home/<runner-user> \
  --config /etc/webcodex/clients/workstation/agent.toml
sudo webcodex agent status --scope system --profile workstation
```

除非显式 `--allow-root-runner`，否则拒绝以 root 运行 Runner（不推荐）。所有
生命周期命令使用相同 `--scope`。示例文件在 `deploy/`（`webcodex.env.example`、
`webcodex.service.example`、`webcodex-runner.toml.example`、
`webcodex-runner.service.example`、`nginx.webcodex.example.conf`）。

## Docker（仅 Server）

仓库提供 server-only Dockerfile 与 Compose 部署，运行 `webcodex-server` 与管理
CLI；有意不包含 Runner、项目仓库与工具链。

```bash
git clone https://github.com/yyjeqhc/webcodex.git
cd webcodex
./deploy/docker/bootstrap.sh https://webcodex.example.com
docker compose ps
```

默认绑定 `127.0.0.1:8080`。在前面配置 HTTPS 反向代理，再创建 pairing code 并
接入持有仓库的机器。Compose 会从 checkout 源码构建镜像。

## Agent 配置

客户端接入会生成 agent 配置。`agent.toml` 中的重要设置：

| 设置 | 说明 |
| --- | --- |
| `server_url` | 公网 WebCodex URL。 |
| `token` | Agent token。不要提交或打印。 |
| `client_id` | 用于 `agent:<client_id>:<project_id>` 的稳定 id。 |
| `owner` | 该 agent 的 owner principal。 |
| `transport` | 配置 `[quic]` 时优先用 `auto`。 |
| `projects_dir` | 项目注册文件目录。 |
| `[policy]` | 本地执行边界（`allowed_roots` 等）。 |
| `[shell]` | 可选 shell profile 定义与有界 persistent-shell 限制。 |
| `[ssh.resources.<name>]` | 可选命名 SSH 目标，用于 Session 绑定的 `run_shell` / `run_job`。 |

Policy 默认：`allowed_roots` 缺失或为空时默认 `$HOME`；显式 `allowed_roots`
覆盖默认值。用显式 roots 收窄 agent，例如只允许一个工作区：

```toml
[policy]
allow_raw_shell = true
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
max_timeout_secs = 3600
max_output_bytes = 262144
```

编辑 `agent.toml` 后 reload 对应服务
（user scope：`systemctl --user reload webcodex-runner`；system scope：
`sudo systemctl reload webcodex-runner`），以应用 policy、shell 与 SSH 资源设置。
身份、server/auth、项目来源、并发、能力与传输变更需要重启。

前台测试可运行 `webcodex-runner --profile workstation`。高级手动生成配置用
`webcodex agent init`。

## OAuth2

OAuth2 默认关闭。为使用 authorization-code 流程的 GPT Actions / MCP 客户端启用：

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

创建 OAuth client（`client_secret` 只返回一次；服务端只存其 hash）：

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"ChatGPT MCP","redirect_uris":["https://chatgpt.com/connector/oauth/<callback-id>"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

`allowed_scopes` 是该 OAuth client 持久化的委派权限上限。Computer 只读观察需要
`computer:read`；会产生 UI effect 的 Computer 工具还需要 `computer:control`。新增
scope 时，Server **不会**静默扩大历史 client 的 allowlist。若要给既有 client 显式
增加 Computer control，请把期望保留的**完整、非空** scope 列表提交到 first-party
管理接口：

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/update_scopes \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"client_id":"wc_client_<server-generated-id>","allowed_scopes":["runtime:read","project:read","project:write","job:run","computer:read","computer:control"]}'
```

allowlist 真正变化时，Server 会在同一事务中撤销该 client 现有的 access token、
refresh token 与尚存的 authorization code；OAuth 宿主随后必须重新授权并取得新
令牌。这对扩权和降权都适用。重复提交相同的 canonical allowlist 是 no-op，不会
撤销现有 grants。

ChatGPT MCP 的宿主文件导入使用独立的 operator trust anchor，因为宿主提供的临时
下载 URL 不受 GPT Action `files.oaiusercontent.com` hostname policy 限制。正常创建
ChatGPT OAuth client 后，取创建接口返回的 server-generated `wc_client_*` ID，并把
这个精确 ID 配入下列设置；多个可信 client 用逗号分隔：

```text
WEBCODEX_OAUTH2_TRUSTED_MCP_FILE_CLIENT_IDS=wc_client_<server-generated-id>
```

服务端要求当前 OAuth access token 的 `allowed_client_id` 精确命中该 allowlist，且
对应 OAuth client record 仍为 active。Redirect URI 与 client display name 都不是
trust identity。重新创建 ChatGPT OAuth client 会产生新的 client ID，因此属于显式的
trust rotation，operator 必须同步更新此设置。普通 API token/raw MCP caller 仍不能
使用该下载路径。

用 `POST /api/oauth/clients/list` 与 `POST /api/oauth/clients/revoke` 列出与
撤销 client。OAuth 使用 authorization-code 流程；动态 client 注册、OIDC 与
device-code 流程未实现。宿主提供 `offline_access` 时保持勾选——它是协议级
refresh-token scope，不授予额外 WebCodex 权限。

## GPT Actions 与 MCP

- **MCP：** 用 user API token（`wc_pat_*`）把客户端连接到
  `https://your-domain.example/mcp`；启用 OAuth 时用 OAuth 流程。
- **GPT Actions：** 把 `https://your-domain.example/openapi.json` 以 HTTP Bearer
  认证导入 Custom GPT。

两者使用同一个 user API token 与同一个 ToolRuntime。OpenAPI schema 有意排除
users、token、pairing/enrollment、setup、doctor、npm、server 管理与 audit
endpoint。这些请用 `webcodex` 完成。

MCP 与 GPT Actions 见 [MCP.md](MCP.zh-CN.md) 与客户端特定设置
[AI 接入指南](AI_ONBOARDING.zh-CN.md)。

## 运维

### Authority mode

`WEBCODEX_AUTHORITY_MODE` 控制有后果的 runtime 工具是自动执行还是需要人工审批：

| 值 | 行为 |
| --- | --- |
| 未设置 / 空 | `trusted_agent`（自托管单运维者部署的默认值）。 |
| `trusted_agent` | 项目工作、shell、jobs、git、校验在硬安全检查后自动执行，无审批中断。Push/tag/publish/release/deploy 仍要求用户任务显式包含该动作。 |
| `restricted` | 有后果的工具在人工批准前被拒绝（`webcodex task approve/deny`）。 |

`trusted_agent` 永不放松硬安全边界（项目根、只读会话、路径策略、凭据脱敏、
job 取消语义）。`WEBCODEX_PERMISSION_MODE` 已移除；若设置，配置视为无效。

### 运维检查

```bash
webcodex ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE" --strict
webcodex ops agents --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex ops projects --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
webcodex ops smoke-preflight --server-url "$SERVER_URL" \
  --token-file "$USER_TOKEN_FILE" --project agent:workstation:my-repo
```

`ops` 命令只读，绝不打印 token 或 env 值。`--strict` 会让 FAIL 报告以状态码 2
退出。`WARN` 表示值得关注但不是 deploy blocker。

### Smoke 检查

推荐的生产 smoke 序列：

1. `webcodex ops status ... --strict` 通过。
2. `POST /api/runtime/status` 返回 `service=webcodex` 与预期公网 URL。
3. `listAgents` 显示至少一个在线 agent。
4. `listProjects` 显示 `agent:<client_id>:<project_id>` id。
5. 已知项目上的只读项目工具可用。
6. 写入/替换/校验测试只针对一次性 smoke 项目。

### Runtime console

Server 在 `/console` 提供只读浏览器 console，展示共享 project-readiness 投影
（Project、Connection、Agent/coding 就绪、findings、next action）。它不暴露
Agent registry 或传输细节，也不是完整 admin UI。

### Runtime job API 信任模型

`job_status`、`job_log`、`list_jobs` 与 `job_tail` 面向受信的单运维者部署。它们
不是互不信任用户之间的租户边界。不要把单个 runtime 暴露给多个不受信用户，除非
为无项目 job API 增加 job-owner 隔离；否则请使用独立的 server/runtime 实例。

## 故障排查

运维检查清单与常见修复（已有 systemd 服务、`HTTP reachable: no`、客户端 PATH
缺少 CLI、server 侧 pairing 与客户端 enrollment 的区别、`client online: no`）
见[故障排查](TROUBLESHOOTING.zh-CN.md)。
