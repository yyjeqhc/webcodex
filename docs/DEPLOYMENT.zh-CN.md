# 部署指南

[English](DEPLOYMENT.md) | [简体中文](DEPLOYMENT.zh-CN.md)

本文档说明当前 WebCodex 的生产部署形态：server bootstrap、service 安装、agent 配置、GPT Actions、MCP 以及 smoke checks。

## 组件

- `webcodex`：服务器进程，暴露 REST、GPT Actions OpenAPI、MCP 和 agent endpoints。
- `webcodex-agent`：长驻 worker，通过 `auto` transport 连接（先 QUIC，再 WebSocket，再 polling），也可显式指定单一 transport。
- `webcodex-cli`：推荐的管理 CLI，用于 server bootstrap、pairing/enrollment、status 和 doctor checks。

## 服务器配置

生产环境通常需要这些配置：

```text
WEBCODEX_TOKEN=<bootstrap-admin-token>
WEBCODEX_ADDR=127.0.0.1:8080
WEBCODEX_DATA=/var/lib/webcodex
```

`WEBCODEX_PUBLIC_URL=https://your-domain.example` 在 `server init` 阶段是可选的，因为这时你可能还不知道最终 HTTPS 域名。但在连接 GPT Actions、MCP 客户端、远程 agent 或任何面向用户的 OpenAPI flow 之前必须配置它；否则 runtime status 和 OpenAPI server URL 可能会指向错误地址。

`WEBCODEX_TOKEN` 只用于初始设置和管理操作。日常 GPT Actions 与 MCP 调用应使用用户 API token；agent 应使用 agent token。

## Server-first setup

推荐的分发路径是 npm thin installer/wrapper：

```bash
npm install -g @yyjeqhc/webcodex
```

v0.1.0 release artifacts 当前包含 `linux-x64`、`linux-arm64` 和 `darwin-arm64`。除非后续 release 增加 artifacts，否则 v0.1.0 不包含 `darwin-x64`、Windows 和其他目标平台。

初始化 env 文件：

```bash
sudo webcodex-cli server init \
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
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

兼容命令仍然可用：

```bash
webcodex users ...
webcodex tokens ...
webcodex agent-tokens ...
webcodex-cli setup single-user
```

新文档和自动化脚本应优先使用 `webcodex-cli`。

## 二进制部署清单

Server：

1. 安装 `webcodex` 和 `webcodex-cli` binaries。
2. 运行 `webcodex-cli server init`。
3. 仅在替换旧 unit 时运行 `webcodex-cli server install-service --overwrite`。
4. 运行 `sudo systemctl daemon-reload`。
5. 运行 `sudo systemctl enable --now webcodex`。
6. 运行 `webcodex-cli server status`。

Server/admin：

7. 运行 `webcodex-cli pairing create`。

Client：

8. 安装 `webcodex-agent` 和 `webcodex-cli` binaries。
9. 运行 `webcodex-cli client enroll`。
10. 运行 `webcodex-cli agent install-service`。
11. 运行 `sudo systemctl daemon-reload`。
12. 运行 `sudo systemctl enable --now webcodex-agent`。
13. 运行 `webcodex-cli agent status`。
14. 运行 `webcodex-cli doctor --strict`。

`/etc/webcodex/webcodex.env` 只属于 server 侧。把 client agent 部署为 service 时，`/etc/webcodex/agent.toml`、`webcodex-user-token` 和 `webcodex-agent-token` 是 client 侧文件。

## 账户凭据开通流程

如果部署不使用 pairing，可以使用下面的 account credential flow。环境特定 smoke 记录单独放在 [smoke-test-sg4.md](smoke-test-sg4.md)；本节命令统一使用 `https://your-domain.example` 占位符。

1. 使用 server env file 中的 `WEBCODEX_TOKEN` 启动服务器。它只是 bootstrap/root/admin 凭据。
2. 管理员运行 `webcodex-cli users create --issue-credential` 创建用户，并把返回的 `wc_acct_xxx` 一次性发给该用户。这个路径的二进制帮助使用 `users create` 和 `--server-url`；`token create-local` 与 `agent-token create-local` 使用 `--server`。
3. 用户运行 `webcodex-cli token create-local`，使用 `wc_acct_xxx` 在本地生成 `wc_pat_xxx`，服务器只登记其 hash。GPT Actions、MCP 和 runtime API 调用使用这个 PAT。
4. 用户运行 `webcodex-cli agent-token create-local`，使用 `wc_acct_xxx` 和 `--client-id <client_id>` 在本地生成 `wc_agent_xxx`，服务器只登记其 hash。该 token 只用于 `webcodex-agent`。
5. 初始化 `webcodex-agent`，添加顶层 agent `projects.d/*.toml` 文件，启动 agent，然后验证 `runtime_status`、`projects/list` 和一个只读 `tools/call`，例如 `git_status`。

不要把 `wc_acct_xxx` 当作 GPT Action/MCP token，也不要把它写进 `agent.toml`。

## 邀请另一个用户

server owner 邀请朋友或其他 operator 时，应使用短期 pairing code。不要在机器之间复制长期凭据。

Server/admin 侧：

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` 是 server/admin 侧操作。`/etc/webcodex/webcodex.env` 只属于 server 侧。只把短期 `wc_pair_*` code 发给对方。

Client/friend 侧：

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml \
  --projects-dir /etc/webcodex/projects.d \
  --allowed-root /home/friend/git

webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent

webcodex-cli doctor \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token \
  --strict
```

`client enroll` 是 client/friend 侧操作。GPT Actions 应使用 client 侧的 `webcodex-user-token`；生成的 agent config 使用 client 侧 agent token 连接 `webcodex-agent`。不要在机器之间复制 `WEBCODEX_TOKEN`、`wc_pat_*`、`wc_agent_*`、完整 env files 或完整 `agent.toml` files。每个 friend 都应使用唯一的 `username` 和 `client_id`。

## Runtime console

WebCodex 在这里提供一个只读浏览器 console：

```text
https://your-domain.example/console
```

静态 console bundle 不包含 secrets。Runtime data 由浏览器使用用户凭据、session 或 token 从受保护 API 获取。console 不属于 GPT Actions OpenAPI，也不是完整 admin UI。

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
    }
}
```

建议让 WebCodex 继续在 proxy 后面监听 `127.0.0.1:8080`。QUIC agent transport 与这个 HTTPS path 是分开的；打开 UDP 8443 前请先看 [AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)。

## Agent 配置

`client enroll` 会生成 agent config。使用下面命令安装 systemd unit：

```bash
sudo webcodex-cli agent install-service \
  --config /etc/webcodex/agent.toml \
  --bin /opt/webcodex/bin/webcodex-agent
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent
webcodex-cli agent status \
  --config /etc/webcodex/agent.toml \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token
```

前台测试可直接启动 agent：

```bash
webcodex-agent --config ~/.config/webcodex/agent.toml
```

`webcodex-agent init` 仍保留为兼容入口。

重要 agent 设置：

| Setting | 说明 |
| --- | --- |
| `server_url` | WebCodex public URL。 |
| `token` | Agent token。不要提交或打印。 |
| `client_id` | 稳定 id，用于 `agent:<client_id>:<project_id>`。 |
| `owner` | 该 agent 的 owner principal。 |
| `transport` | 推荐配置 `[quic]` 并使用 `auto`：先 QUIC，再 WebSocket，再 polling。只有明确需要单一 transport 时才使用 strict `quic`、`websocket` 或 `polling`。 |
| `projects_dir` | 项目注册文件目录。 |
| `[policy]` | 本地执行边界。 |
| `[shell]` | 可选 shell profile 定义，用于项目开发环境。 |

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

`projects_dir` 中的 agent project files 可以设置 `shell_profile = "rust"`，把项目绑定到已配置 profile。

Shell profiles 会为每个 project/profile 准备一次性 environment snapshot；它不是持久 shell，默认不会 source `.bashrc` 或 `.profile`。Rust/Cargo、Python venv、Conda 示例、解析规则和安全边界见 [SHELL_PROFILES.md](SHELL_PROFILES.md)。修改 profile 后需要重启 `webcodex-agent`，当前没有 reload API。

`runtime_status` 和 `listAgents` 会暴露 redacted policy summary，以及经过清理的 `shell_profiles` 摘要，包括 profile names、`has_init_script`、`env_keys_count`、`program`、`args_count`。`listProjects` 会暴露 `shell_profile`、`resolved_shell_profile` 和 `shell_profile_status`（`configured` / `missing` / `not_configured` / `unknown`）。这些接口不会暴露 tokens、env values、`Authorization` headers、完整 `agent.toml`、完整 env snapshot 或 shell profile `init_script` bodies。

## Authentication and transport

普通 REST、polling、MCP 和 GPT Actions 调用必须使用：

```text
Authorization: Bearer <token>
```

`?token=` 只允许用于 `/api/agents/ws` WebSocket handshake 兼容场景。不要把 query-string token 用在 polling、REST、MCP 或 GPT Actions。

Agent 推荐配置 QUIC 并使用 `transport = "auto"`。WebSocket 和 polling 继续作为受限网络下的 fallback。

## GPT Actions 和 MCP

从这里导入 GPT Actions：

```text
https://your-domain.example/openapi.json
```

在 GPT Actions 中把认证配置为 HTTP Bearer/API key，并放在 `Authorization` header。

OpenAPI GPT Actions 管理面有意排除 users、API tokens、agent tokens、pairing/enrollment、setup、doctor、npm、server management 和 audit endpoints。这些任务请使用 `webcodex-cli`。

MCP 使用同一个用户 API token，并使用与 GPT Actions 相同的 `ToolRuntime`。

## Optional Codex CLI jobs

`runCodexTask` 是可选能力。它要求 agent 机器已经安装并配置 Codex CLI。它不会启动新的 `webcodex-agent`；它只是把工作委托给已经连接的 agent。

## Smoke checks

推荐的生产 smoke sequence：

1. `webcodex-cli doctor --server-url https://your-domain.example --user-token-file PATH` 通过非破坏性检查。
2. `POST /api/runtime/status` 返回 `service=webcodex` 和预期 public URL。
3. `listAgents` 显示至少一个 online agent。
4. `listProjects` 显示 `agent:<client_id>:<project_id>` ids。
5. 已知项目上的只读 project tools 可用。
6. 写入、替换、验证类测试只在 disposable smoke projects 中执行。

## Troubleshooting

部署排障和运维检查清单见 [TROUBLESHOOTING.md](TROUBLESHOOTING.md)，包括已有 systemd services、`HTTP reachable: no`、client CLI 不在 `PATH`、server-side pairing 与 client-side enrollment、agent-only client warnings、`client online: no` 等常见问题。
