# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这份文档给出第一次部署 WebCodex server 和第一次部署 client agent 的最短可用路径。它比 README 更偏操作，比 [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md) 更短。

下面的命令形态已对照当前 `webcodex-cli`、`webcodex-agent` 和 `webcodex` 的二进制 help 输出检查。

## 0. 安装 binaries

在每台要运行 server 或 agent 的机器上安装：

```bash
npm install -g @yyjeqhc/webcodex

webcodex -h
webcodex-cli -h
webcodex-agent -h
```

v0.1.0 release artifacts 当前包含 `linux-x64`、`linux-arm64` 和 `darwin-arm64`。v0.1.0 暂不包含 Windows 和 `darwin-x64` artifacts。

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

这只会写入 bootstrap/admin `WEBCODEX_TOKEN`，不会创建 `wc_pat_xxx` user token 或 `wc_agent_xxx` agent token。

### 1.2 放到 HTTPS 后面

配置反向代理，让公网 URL 转发到本地 HTTP server：

```text
https://your-domain.example  ->  http://127.0.0.1:8080
```

GPT Actions 和 MCP 需要公网 HTTPS URL。WebCodex CLI 不会自动配置 DNS、TLS、反向代理或 tunnel。

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

如果暂时不启用 QUIC，可以先用 WebSocket。agent 可使用 `--transport websocket` 或 `transport = "websocket"`。

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
  --output-dir /etc/webcodex \
  --agent-config /etc/webcodex/agent.toml \
  --projects-dir /etc/webcodex/projects.d \
  --allowed-root /home/alice/git
```

`client enroll` 会通过 HTTPS 接收 `wc_pat_xxx` 和 `wc_agent_xxx`，并以受限权限写到本机。

如果 server 已启用 QUIC listener，在 `/etc/webcodex/agent.toml` 中添加 `[quic]`：

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
sudo mkdir -p /etc/webcodex/projects.d
sudo tee /etc/webcodex/projects.d/my-repo.toml >/dev/null <<'EOF'
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

在 `/etc/webcodex/agent.toml` 中添加或调整：

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
  --config /etc/webcodex/agent.toml \
  --bin "$(command -v webcodex-agent)"

sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent

webcodex-cli agent status \
  --config /etc/webcodex/agent.toml \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token

webcodex-cli doctor --strict \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/webcodex-user-token \
  --agent-token-file /etc/webcodex/webcodex-agent-token \
  --agent-config /etc/webcodex/agent.toml
```

只有替换已有 unit 时才给 `agent install-service` 加 `--overwrite`。

## 4. 第一次客户端部署：不使用 service，后台进程模式

这个模式适合快速测试、容器、临时 client，或不能使用 systemd 的主机。长期生产 agent 更建议用 service 模式。

### 4.1 Enroll 到用户配置目录

```bash
webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id alice-laptop \
  --display-name "Alice Laptop" \
  --transport auto \
  --output-dir "$HOME/.config/webcodex" \
  --agent-config "$HOME/.config/webcodex/agent.toml" \
  --projects-dir "$HOME/.config/webcodex/projects.d" \
  --allowed-root "$HOME/git"
```

创建项目文件：

```bash
mkdir -p "$HOME/.config/webcodex/projects.d"
cat > "$HOME/.config/webcodex/projects.d/my-repo.toml" <<'EOF'
id = "my-repo"
path = "/home/alice/git/my-repo"
name = "My Repo"
kind = "repo"
allow_patch = true
shell_profile = "rust"
EOF
```

把 `path` 改成真实用户和仓库路径。

### 4.2 配置后台启动环境

为后台进程创建一个小的环境文件：

```bash
mkdir -p "$HOME/.config/webcodex" "$HOME/.local/state/webcodex"
cat > "$HOME/.config/webcodex/agent.env" <<'EOF'
WEBCODEX_AGENT_CONFIG=$HOME/.config/webcodex/agent.toml
PATH=$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin
EOF
chmod 600 "$HOME/.config/webcodex/agent.env"
```

这个环境只控制 `webcodex-agent` 后台进程如何启动。项目命令环境仍应配置在 `agent.toml` 的 `[shell.profiles.*]` 中。

### 4.3 在 agent.toml 中添加 shell profile

在 `$HOME/.config/webcodex/agent.toml` 中添加或调整：

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

`agent.toml` 中建议使用绝对路径，不要依赖 TOML 字符串中的 `$HOME` 展开。

### 4.4 后台启动

```bash
set -a
. "$HOME/.config/webcodex/agent.env"
set +a

nohup webcodex-agent --config "$HOME/.config/webcodex/agent.toml" \
  >> "$HOME/.local/state/webcodex/agent.log" 2>&1 &

echo $! > "$HOME/.local/state/webcodex/agent.pid"
```

查看日志和状态：

```bash
tail -f "$HOME/.local/state/webcodex/agent.log"

webcodex-cli agent status \
  --config "$HOME/.config/webcodex/agent.toml" \
  --server-url https://your-domain.example \
  --user-token-file "$HOME/.config/webcodex/webcodex-user-token" \
  --agent-token-file "$HOME/.config/webcodex/webcodex-agent-token"
```

停止后台 agent：

```bash
kill "$(cat "$HOME/.local/state/webcodex/agent.pid")"
```

## 5. 从 server/API 侧测试

agent 在线后，runtime 调用使用 user PAT，不使用 `WEBCODEX_TOKEN`：

```bash
export WEBCODEX_PAT="$(cat /etc/webcodex/webcodex-user-token 2>/dev/null || cat "$HOME/.config/webcodex/webcodex-user-token")"

curl -sS --oauth2-bearer "$WEBCODEX_PAT" \
  -H 'Content-Type: application/json' \
  https://your-domain.example/api/tools/list \
  -d '{}'
```

然后在 GPT Actions 或 MCP 中使用同一个 `wc_pat_xxx` token。

## 6. 应该选择哪种模式？

| 模式 | 适用场景 | 说明 |
| --- | --- | --- |
| Server systemd service | 生产服务器 | 推荐。重启后自动恢复。 |
| Agent systemd service | 长期运行的可信 client 或服务器侧 worker | 推荐用于稳定机器。注意配置 shell profiles，因为 systemd 不读 `.bashrc`。 |
| Agent no-service background process | 临时 client、容器、smoke test 或无 systemd 主机 | 用 `nohup` 启动；保留 log 和 pid 文件；重启后需手动恢复。 |

## 7. 下一步文档

- 完整部署细节：[DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)
- Agent projects：[AGENT_PROJECTS.zh-CN.md](AGENT_PROJECTS.zh-CN.md)
- Shell profiles 和 PATH 处理：[SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md)
- Transport 细节与 QUIC 验证：[AGENT_TRANSPORTS.zh-CN.md](AGENT_TRANSPORTS.zh-CN.md)
- 排障：[TROUBLESHOOTING.zh-CN.md](TROUBLESHOOTING.zh-CN.md)
