# Build and Install Quick Reference

[English](BUILD_INSTALL.md) | [简体中文](BUILD_INSTALL.zh-CN.md)

这是构建和安装的快速参考。生产部署细节见 [DEPLOYMENT.md](DEPLOYMENT.md) / [DEPLOYMENT.zh-CN.md](DEPLOYMENT.zh-CN.md)。

## 构建 binaries

为当前 host 构建三个 binaries：

```text
webcodex
webcodex-agent
webcodex-cli
```

不要运行 unauthenticated production deployments。

## 已按二进制 help 校验的命令形态

本文档中的示例已对照当前 `webcodex-cli -h`、`webcodex-agent -h` 和 `webcodex -h` 的输出检查。需要特别注意这些 flag 差异：

| 任务 | 推荐命令形态 |
| --- | --- |
| 初始化服务器 env | `webcodex-cli server init --listen ... --data-dir ... --env-file ...` |
| 安装服务器 systemd unit | `webcodex-cli server install-service --env-file ... --bin ...` |
| 检查服务器状态 | `webcodex-cli server status --env-file ...` |
| 管理员创建账户凭据 | `webcodex-cli users create --server-url ... --token ... --username ... --issue-credential` |
| 用户创建 PAT | `webcodex-cli token create-local --server ... --user ... --credential ... --scopes ...` |
| 用户创建 agent token | `webcodex-cli agent-token create-local --server ... --user ... --credential ... --client-id ...` |
| 创建 pairing code | `webcodex-cli pairing create --server-url ... --username ... --client-id ...` |
| 客户端 enrollment | `webcodex-cli client enroll --server-url ... --pairing-code ... --client-id ...` |
| 前台运行 agent | `webcodex-agent --profile ...` |
| 安装 agent service | `webcodex-cli agent install-service --profile ... --bin ...` |
| Doctor 检查 | `webcodex-cli doctor --server-url ... --user-token-file ... --strict` |

账户管理命令使用 `users create` 和 `--server-url`；本地 token 创建命令使用 `--server`。这是当前 CLI surface 的实际差异，示例中会按这个差异书写。

## 安装 packages

推荐分发路径是 npm thin installer/wrapper：

```bash
npm install -g @yyjeqhc/webcodex
```

计划发布的 v0.2.0 GitHub release artifacts 包含 `linux-x64`、`linux-arm64` 和 `darwin-arm64`。除非后续 release 增加 artifacts，否则 v0.2.0 暂不计划包含 `darwin-x64`、Windows 和其他 targets。npm wrapper 当前安装的是 v0.1.0 二进制文件；v0.2.0 用户应直接下载 GitHub release 二进制文件。

npm package 是 native release artifacts 的 thin wrapper。安装时会下载匹配的 GitHub Release artifact，并使用 manifest 中的 SHA-256 checksum 验证。

## 示例文件

`deploy/` 目录包含可改造的短示例：

- `deploy/webcodex.env.example`
- `deploy/webcodex.service.example`
- `deploy/webcodex-agent.toml.example`
- `deploy/webcodex-agent.service.example`
- `deploy/nginx.webcodex.example.conf`

nginx 文件只是示例。WebCodex CLI 不会自动配置 reverse proxy。

## Binary deployment flow

Server：

1. 安装 `webcodex` 和 `webcodex-cli` binaries。
2. 初始化 server env file：

```bash
sudo webcodex-cli server init \
  --listen 127.0.0.1:8080 \
  --data-dir /var/lib/webcodex \
  --env-file /etc/webcodex/webcodex.env
```

这只会在 `/etc/webcodex/webcodex.env` 中创建 server bootstrap/admin `WEBCODEX_TOKEN`。该文件只属于 server-side，不会创建 user API tokens 或 agent tokens。

3. 安装 server service。只有替换旧 unit 时才使用 `--overwrite`。

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex
```

4. Reload systemd，启动 service 并检查状态：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex
webcodex-cli server status --env-file /etc/webcodex/webcodex.env
```

Server/admin：

5. 创建短期一次性 pairing code：

```bash
webcodex-cli pairing create \
  --server-url https://your-domain.example \
  --env-file /etc/webcodex/webcodex.env \
  --username friendname \
  --client-id friend-laptop \
  --display-name "Friend Name" \
  --ttl-secs 600
```

`pairing create` 是 server/admin-side 命令。只复制短期 `wc_pair_*` code 给 client；不要复制 `WEBCODEX_TOKEN`、`wc_pat_*`、`wc_agent_*`、完整 env files 或完整 `agent.toml` files。

Client：

6. 安装 `webcodex-agent` 和 `webcodex-cli` binaries。
7. 通过 HTTPS 交换 pairing code，并写入 client-side credentials/config：

```bash
sudo webcodex-cli client enroll \
  --server-url https://your-domain.example \
  --pairing-code <wc_pair_...> \
  --client-id friend-laptop \
  --profile workstation \
  --allowed-root /home/friend/git
```

`client enroll` 会在本地创建 `wc_pat_*` user token、`wc_agent_*` agent token 和 `/etc/webcodex/clients/special/agent.toml`，Unix 上权限为 `0600`。`/etc/webcodex/webcodex.env` 只属于 server 侧；多用户或多个 client 共用一台机器时，client-side token/config 文件应隔离在 `/etc/webcodex/clients/<profile>/` 下。

8. 安装并启动 agent service，然后验证：

```bash
sudo webcodex-cli agent install-service \
  --profile workstation \
  --bin /opt/webcodex/bin/webcodex-agent \
  --overwrite
sudo systemctl daemon-reload
sudo systemctl enable --now webcodex-agent-special
webcodex-cli agent status \
  --profile workstation \
  --server-url https://your-domain.example
webcodex-cli doctor --strict \
  --profile workstation \
  --server-url https://your-domain.example
```

GPT Actions 应使用生成的 client-side user-token file。GPT Actions 需要 public HTTPS URL；WebCodex CLI 不会自动配置 reverse proxies 或 tunnels。

## Agent config

`client enroll` 会写入 `agent.toml`。systemd service 使用 `webcodex-cli agent install-service`；前台测试可运行：

```bash
webcodex-agent --profile workstation
```

`webcodex-agent init` 仍保留为兼容入口。

## Doctor

运行非破坏性 diagnostics：

```bash
webcodex-cli doctor --strict \
  --server-url https://your-domain.example \
  --user-token-file /etc/webcodex/clients/workstation/webcodex-user-token \
  --agent-token-file /etc/webcodex/clients/workstation/webcodex-agent-token
```

添加 `--agent-config /etc/webcodex/clients/workstation/agent.toml` 可运行本地 shell-profile / project diagnostics。添加 `--project <id>` 可对指定项目运行远程 shell roundtrip。

Doctor 不会打印 `init_script` bodies、env values 或 tokens。Profile 配置和排障见 [SHELL_PROFILES.zh-CN.md](SHELL_PROFILES.zh-CN.md)。

## Auth reminders

REST、polling、MCP 和 GPT Actions 使用：

```text
Authorization: Bearer <token>
```

`?token=` 只允许用于 `/api/agents/ws` WebSocket handshake 兼容场景。

## systemd PATH reminder

systemd services 不读取交互式 shell 启动文件，例如 `~/.bashrc`。如果命令需要 Rust/Cargo、Node 或 Codex CLI，请通过 agent shell profiles 或 service manager environment 暴露它们。

`runCodexTask` 是可选能力，需要 agent 机器上已有 Codex CLI。它不会启动新的 `webcodex-agent`。
