# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这是 WebCodex 的 canonical onboarding 入口。它帮助首次部署者选择路径、安装匹配的二进制文件、连接 server 和 agent，然后再进入 GPT Actions、MCP、Deployment、OAuth 或 Testing 文档，而不需要一次读完整个系统。

先理解术语时看 [CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md)。如果只是想跑一个不需要 `sudo`、`/etc`、systemd、HTTPS、Nginx 或 QUIC 的单机 demo，请先看 README quick start。

下面的命令形态已对照当前 `webcodex-cli`、`webcodex-agent` 和 `webcodex` 的二进制 help 输出检查。

## 决策树

| 目标 | 从哪里开始 | 下一步阅读 |
| --- | --- | --- |
| 单机本地快速体验 | README quick start | 需要 shared-key、service、GPT Actions、MCP 或远程 agent 时再回到本文 |
| 用一个共享密钥快速评估 server + agent | 下方 A 节 | [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)、[AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md) |
| 单用户自托管部署，使用可撤销 token | 下方第 1-3 节 | [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)、[OPERATIONS.md](OPERATIONS.md) |
| 接入 GPT Actions | 先完成 server + online agent，再看 [GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md) | 使用 OAuth 时看 [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) |
| 接入 MCP | 先完成 server + online agent，再看 [MCP.zh-CN.md](MCP.zh-CN.md) | 使用 OAuth 时看 [OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md) |
| systemd service 部署 | 下方第 1-3 节 | [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)、[TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md) |
| manual/no-service agent 运行 | 下方第 4 节 | [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md)、[AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md) |
| OAuth-only Host 但希望 low-config shared-key onboarding | 显式启用 OAuth2 和 shared-key bridge | [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md#oauth2)、[OAUTH2_BRIDGE_THREAT_MODEL.md](OAUTH2_BRIDGE_THREAT_MODEL.md) |

## 快速开始：三条路径

### A. 共享密钥（推荐早期体验）

启动 server。`server up` 默认不允许匿名访问，并启用共享密钥 quick-start 路径：

```bash
webcodex-cli server up --public-url <URL>
```

从项目目录用同一个共享密钥连接 agent；这个密钥也会给 GPT Actions 或 MCP 使用：

```bash
webcodex-cli connect <URL> --key <KEY> --root <PROJECT>
```

当 GPT Action / MCP Host 支持静态 Bearer/API-key 认证时，GPT Action / MCP 使用同一个密钥：

```text
Authorization: Bearer <KEY>
```

在 ChatGPT custom connectors 中，这条路径应选择 **访问令牌/API 密钥**，不要选择 OAuth。server 按 `shared_key_hash` 给共享密钥 caller 分组：使用同一个 key 的 agent 和 GPT/MCP caller 可以互相可见，不同 key 的 group 互不可见。共享密钥是非管理员 quick-start auth；它不是 managed user，也不应被当作生产 IAM。

### B. Open demo mode（匿名，仅限临时 demo）

```bash
webcodex-cli server up --open --public-url <URL>
webcodex-cli connect <URL> --open --root <PROJECT>
webcodex-agent --config <generated-agent.toml>
```

server 必须显式使用 `--open`（`WEBCODEX_ALLOW_ANONYMOUS=true`），client 也必须显式使用 `connect --open`。生成的 agent config 使用 `token = ""`；`webcodex-agent` 会把它当作不发送 Authorization。对 GPT Actions / MCP Host 来说，只有在 Host 明确提供 **无认证**、**None** 或 no-auth 设置时才使用这条路径。OAuth client 字段留空并不等于 no auth。

Open demo mode 只适合 localhost、可信 LAN 和临时 demo。不要把它作为长期公网模式使用。匿名 open caller 共享同一个 demo current-session principal，因此 open group 内的 session state 是共享的。

### C. Managed self-hosted mode

长期自托管部署使用 pairing 或 account credential、`wc_pat_*` user token 和 `wc_agent_*` agent token。Managed mode 提供可撤销凭据、带 scope 的 PAT 和更清晰的 ownership 记录。它不是 hosted SaaS、租户隔离层或外部身份提供商。完整 managed 流程保留在下方第 1-4 节。

## GPT Action / MCP Host 认证兼容性

不同 Host 展示认证设置的方式不一样。

| Host UI 选项 | WebCodex 模式 | 说明 |
| --- | --- | --- |
| 访问令牌/API 密钥、静态 Bearer 或自定义 `Authorization` header | 共享密钥 quick start 或 managed PAT | quick start 使用 shared key；managed mode 使用 `wc_pat_*`。Host 会把它作为 `Authorization: Bearer ...` 发送。 |
| 无认证、None 或不配置认证 | Open demo mode | server 必须用 `--open` 启动；不要把它暴露成长期公网模式。 |
| OAuth | Managed OAuth，或显式启用的 shared-key OAuth bridge | 只有在 WebCodex 已按该 Host 期望配置 OAuth flow 时才选择。 |

不要选择 OAuth 后把 client 字段留空，并期待它变成 shared-key 或 open 行为。OAuth client 字段留空通常表示 Host 会尝试 OAuth metadata discovery、dynamic client registration 或 client metadata discovery。

如果某个 Host 只支持 OAuth，而你希望 low-config shared-key onboarding，请在 WebCodex server 上显式启用 OAuth2 和 shared-key bridge（`WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true`）。bridge 会让用户在 WebCodex OAuth 页面输入 shared key，并在 authorization-code exchange 后得到 OAuth token。它仍然遵守 OAuth 语义和 scope，只保存 shared-key hash，不会把匿名 open mode 变成 OAuth，也不会授予 `admin`、`account:manage` 或 `agent:*` scope。

## 0. 安装 binaries

在每台要运行 server 或 agent 的机器上安装：

```bash
npm install -g @yyjeqhc/webcodex

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

当前安装路径和发布过渡状态：

- `npm install -g @yyjeqhc/webcodex` 是当前公开 npm wrapper 路径，会安装已发布 package/manifest 对应的版本。
- 在这个仓库中，npm wrapper 仍是 `0.1.0`，manifest 指向 v0.1.0 GitHub release artifacts。
- v0.2.0 release-prep 路径是在 artifacts 发布后使用 GitHub release artifacts：`linux-x64`、`linux-arm64` 和 `darwin-arm64`。在 npm wrapper 和 manifest 更新前，不要假设 `npm install -g @yyjeqhc/webcodex` 会安装 v0.2.0。
- v0.2.0 暂不计划包含 Windows 和 `darwin-x64` artifacts。
- 从开发 checkout 评估未发布代码时，应从本仓库构建二进制文件，而不是让 npm wrapper 代表未发布代码。

## 1. 第一次部署服务器

在服务器主机上执行。

### 1.1 初始化 server env

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env \
  --public-url https://your-domain.example
```

这会创建 `/etc/webcodex/webcodex.env`，并写入 bootstrap/admin `WEBCODEX_TOKEN`、`WEBCODEX_ADDR`、`WEBCODEX_DATA` 和 `WEBCODEX_PUBLIC_URL`。它不会创建 `wc_pat_xxx` user token 或 `wc_agent_xxx` agent token。

在同一个 shell 中运行 admin 命令时，命令支持的话优先使用 `--env-file /etc/webcodex/webcodex.env`；也可以先加载 env 文件：

```bash
set -a
. /etc/webcodex/webcodex.env
set +a
```

### 1.2 放到 HTTPS 后面

配置反向代理，让公网 URL 转发到本地 HTTP server：

```text
https://your-domain.example  ->  http://127.0.0.1:8080
```

GPT Actions 和 MCP 需要公网 HTTPS URL。WebCodex CLI 不会自动配置 DNS、TLS、反向代理或 tunnel。[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md#public-https-url) 中包含最小 Nginx 配置。

### 1.3 安装并启动 server service

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin "$(command -v webcodex)"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex

webcodex-cli server status \
  --env-file /etc/webcodex/webcodex.env \
  --url http://127.0.0.1:8080
```

只有替换已有 unit 时才给 `server install-service` 加 `--overwrite`。

### 1.4 可选：为 agent 启用 QUIC

QUIC 只用于 `webcodex-agent` 连接。GPT Actions 和 MCP 仍然走 HTTPS。

要启用 QUIC，在 `/etc/webcodex/webcodex.env` 中加入：

```bash
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/your-domain.example/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/your-domain.example/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-agent/1
```

然后重启 server，并对 agent 主机开放 UDP 8443：

```bash
sudo systemctl restart webcodex
webcodex-cli doctor --quic --server-only \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --strict
```

如果暂时不启用 QUIC，保留 `--transport auto` 且不添加 `[quic]` section；agent 会从 WebSocket fallback 启动，之后添加 `[quic]` section 后即可优先尝试 QUIC。

## 2. 邀请或 enroll 第一个客户端

第一次部署 client 最简单的是 pairing flow。在 server/admin 侧运行：

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username alice \
  --client-id alice-laptop \
  --display-name "Alice" \
  --ttl-secs 600
```

只把短期 `wc_pair_xxx` code 发给客户端。不要把 `WEBCODEX_TOKEN`、`wc_pat_xxx`、`wc_agent_xxx`、`/etc/webcodex/webcodex.env` 或完整 `agent.toml` 复制到另一台机器。

## 3. 第一次客户端部署：system service 模式

在 client/agent 机器上执行。

### 3.1 Enroll client

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir /etc/webcodex/clients/alice-laptop \
  --agent-config /etc/webcodex/clients/alice-laptop/agent.toml \
  --projects-dir /etc/webcodex/clients/alice-laptop/projects.d \
  --allowed-root /home/alice/git
```

`client enroll` 会通过 HTTPS 接收 `wc_pat_xxx` 和 `wc_agent_xxx`，并以受限权限写到本机。默认 profile 使用 `client_id`，因此 root enrollment 会写入 `/etc/webcodex/clients/alice-laptop/`；显式 `--output-dir` 仍然优先，且应指向目标 profile 目录。

如果 server 已启用 QUIC listener，在 `/etc/webcodex/clients/alice-laptop/agent.toml` 中添加 `[quic]`：

```toml
transport = "auto"

[quic]
server_addr = "your-domain.example:8443"
server_name = "your-domain.example"
alpn = "webcodex-agent/1"
connect_timeout_secs = 10
keepalive_interval_secs = 20
```

`transport = "auto"` 且配置了 `[quic]` 时，agent 会先尝试 QUIC，然后 fallback 到 WebSocket，再 fallback 到 polling。

### 3.2 注册项目

在配置的 `projects_dir` 下创建项目注册文件：

```bash
sudo mkdir -p /etc/webcodex/clients/alice-laptop/projects.d
sudo tee /etc/webcodex/clients/alice-laptop/projects.d/my-repo.toml >/dev/null <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"

[hooks]
status = ["git status --short"]
check = ["cargo check --all-targets"]
EOF
```

runtime project id 使用这种格式：

```text
agent:<client_id>:<project_id>
```

以上示例对应：`agent:alice-laptop:my-repo`。

### 3.3 配置项目命令环境

systemd service 不会读取 `~/.bashrc` 这类交互式 shell 文件。项目命令环境应通过 agent shell profiles 配置，不要依赖登录 shell 状态。

在 `/etc/webcodex/clients/alice-laptop/agent.toml` 中添加或调整：

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "bash"
args = ["-lc"]

[shell.profiles.rust.env]
PATH = "/home/alice/.cargo/bin:/home/alice/.local/bin:/usr/local/bin:/usr/bin:/bin"
CARGO_HOME = "/home/alice/.cargo"
RUSTUP_HOME = "/home/alice/.rustup"
```

如果使用 Python、Conda、Node 或 Codex CLI，也把需要的 `PATH` 条目和环境变量写入对应 shell profile。修改 `agent.toml` 后需要重启 agent。

### 3.4 安装并启动 agent service

```bash
sudo webcodex-cli agent install-service \
  --profile alice-laptop \
  --bin "$(command -v webcodex-agent)"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-alice-laptop

webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example

webcodex-cli doctor --strict \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

只有替换已有 unit 时才给 `agent install-service` 加 `--overwrite`。

## 4. 第一次客户端部署：不使用 service，前台或后台模式

这个模式适合快速测试、容器、临时 client，或不能使用 systemd 的主机。长期生产 agent 更建议用 service 模式；no-service 模式更方便观察日志和手动停止。

### 4.1 Enroll 到用户配置目录

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir "$HOME/.config/webcodex/clients/alice-laptop" \
  --agent-config "$HOME/.config/webcodex/clients/alice-laptop/agent.toml" \
  --projects-dir "$HOME/.config/webcodex/clients/alice-laptop/projects.d" \
  --allowed-root "$HOME/git"
```

创建项目文件：

```bash
mkdir -p "$HOME/.config/webcodex/clients/alice-laptop/projects.d"
cat > "$HOME/.config/webcodex/clients/alice-laptop/projects.d/my-repo.toml" <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"
EOF
```

把 `path` 改成真实用户和仓库路径。

### 4.2 在 agent.toml 中添加 shell profile

在 `$HOME/.config/webcodex/clients/alice-laptop/agent.toml` 中添加或调整：

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
description = "Rust development tools"
program = "bash"
args = ["-lc"]

[shell.profiles.rust.env]
PATH = "/home/alice/.cargo/bin:/home/alice/.local/bin:/usr/local/bin:/usr/bin:/bin"
CARGO_HOME = "/home/alice/.cargo"
RUSTUP_HOME = "/home/alice/.rustup"
```

`agent.toml` 中建议使用绝对路径，不要依赖 TOML 字符串中的 `$HOME` 展开。项目命令环境应配置在 `[shell.profiles.*]` 中，而不是依赖交互 shell 的 `.bashrc`。

### 4.3 前台启动，方便检查

前台模式是最简单的 no-service 模式。它会直接打印日志，按 `Ctrl-C` 即可退出：

```bash
webcodex-agent --profile alice-laptop
```

另开一个终端检查状态：

```bash
webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

### 4.4 或使用 nohup 后台启动

前台运行确认没问题后，如果希望关闭终端后 agent 继续运行，可以用：

```bash
mkdir -p "$HOME/.local/state/webcodex"
nohup webcodex-agent --profile alice-laptop \
  >> "$HOME/.local/state/webcodex/agent.log" 2>&1 &

echo $! > "$HOME/.local/state/webcodex/agent.pid"
```

查看日志和状态：

```bash
tail -f "$HOME/.local/state/webcodex/agent.log"

webcodex-cli agent status \
  --profile alice-laptop \
  --server-url https://your-domain.example
```

停止后台 agent：

```bash
kill "$(cat "$HOME/.local/state/webcodex/agent.pid")"
```

## 5. 从 server/API 侧测试

agent 在线后，runtime 调用使用 user PAT，不使用 `WEBCODEX_TOKEN`：

```bash
export WEBCODEX_PAT="$(cat /etc/webcodex/clients/alice-laptop/webcodex-user-token 2>/dev/null || cat "$HOME/.config/webcodex/clients/alice-laptop/webcodex-user-token")"

curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

然后在 GPT Actions 或 MCP 中使用同一个 `wc_pat_xxx` token。

## 6. Runtime 注意点

- 使用 project-scoped current session 时，请在 `start_session` 时传入 `project`。`project = null` 创建的 session 不能后续绑定到某个具体 project；返回 `session_project_mismatch` 是预期审计语义，不代表 runtime 故障。
- 项目由 agent 注册，不再通过 server 侧 projects.toml 暴露给 runtime。用 `list_projects` 查看当前 runtime surface。
- 推荐的模型工作流是结构化编辑工具（`replace_line_range`、`insert_at_line`、`delete_line_range`）、patch validation、受控 shell/job validation、`show_changes` 以及 sessions/handoff。
- 使用 `transport = "auto"` 时，agent 只有在配置了 `[quic]` section 后才会先尝试 QUIC，然后 fallback 到 WebSocket，再 fallback 到 polling。没有 `[quic]` 时，`auto` 会从 WebSocket 开始。

## 7. 应该选择哪种模式？

| 模式 | 适用场景 | 说明 |
| --- | --- | --- |
| Server systemd service | 生产服务器 | 推荐。重启后自动恢复。 |
| Agent systemd service | 长期运行的可信 client 或服务器侧 worker | 推荐用于稳定机器。注意配置 shell profiles，因为 systemd 不读 `.bashrc`。 |
| Agent no-service 前台/后台模式 | 临时 client、容器、smoke test 或无 systemd 主机 | 先用前台模式观察日志；确认后可用 `nohup` 让它在终端关闭后继续运行。 |

## 8. 下一步文档

- 概念与术语：[CONCEPTS.zh-CN.md](CONCEPTS.zh-CN.md)
- 完整部署细节：[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)
- GPT Actions 设置：[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)
- MCP 设置：[MCP.zh-CN.md](MCP.zh-CN.md)
- OAuth2 smoke test：[OAUTH2_SMOKE_TEST.md](OAUTH2_SMOKE_TEST.md)
- Agent projects：[AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md)
- Shell profiles 和 PATH 处理：[SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md)
- Transport 细节与 QUIC 验证：[AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)
- Testing 和 validation：[TESTING.md](TESTING.md)、[E2E_VALIDATION.zh-CN.md](E2E_VALIDATION.zh-CN.md)
- 排障：[TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md)
