# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

这里整理 WebCodex 部署中常见问题的实用检查。排障时不要粘贴或分享真实 tokens、env files、`Authorization` headers 或完整 `agent.toml` files。

## 运维检查清单

Server：

- `webcodex --version` 能打印版本。
- `webcodex-cli server status --env-file /etc/webcodex/webcodex.env` 报告本地 server reachable。
- 在 server host 上，`curl http://127.0.0.1:8080/openapi.json` 返回 OpenAPI JSON。
- 如果使用 nginx 或其他 reverse proxy，public HTTPS 可访问。

Client：

- `webcodex-agent --version` 能打印版本。
- `webcodex-cli agent status --config /etc/webcodex/agent.toml` 能读取本地 agent config。
- `webcodex-cli doctor --strict --server-url https://your-domain.example --user-token-file /etc/webcodex/webcodex-user-token --agent-token-file /etc/webcodex/webcodex-agent-token` 通过。
- `listAgents` / `runtime_status` 显示 agent online。

## 常见问题

### `webcodex-cli server install-service` 提示 service already exists

只有在明确要替换现有 unit 时才使用 `--overwrite`：

```bash
sudo webcodex-cli server install-service \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex \
  --overwrite
sudo systemctl daemon-reload
```

然后按你的正常部署流程 restart 或 start service。

### `server status` 显示 `HTTP reachable: no`

先检查本地 service，再检查 reverse proxy：

```bash
systemctl status webcodex
journalctl -u webcodex
curl http://127.0.0.1:8080/openapi.json
```

如果本地 HTTP 正常但 public HTTPS 不通，检查 nginx upstream host/port 和 TLS 配置。WebCodex CLI 不会自动配置 reverse proxy。

### Client 显示 `webcodex-cli: command not found`

把 CLI 安装或 symlink 到 client 的 `PATH`，例如：

```bash
sudo ln -s /opt/webcodex/bin/webcodex-cli /usr/local/bin/webcodex-cli
```

请使用你主机上的实际安装路径。

### Client 误运行 `pairing create`，且 `/etc/webcodex/webcodex.env` 缺失

`webcodex-cli pairing create` 是 server/admin-side 命令，需要 server bootstrap env file。朋友或 client 机器应运行 `webcodex-cli client enroll`，并使用 server owner 发来的短期 `wc_pair_*` code。

机器之间只复制 `wc_pair_*` code。不要复制 `WEBCODEX_TOKEN`、user API tokens、agent tokens、env files 或完整 `agent.toml` files。

### Client 上 doctor 警告 `binary webcodex not found in PATH`

这在 agent-only client machines 上可能是正常的。Agent-only client 需要 `webcodex-agent` 和 `webcodex-cli`；server binary `webcodex` 只在 server host 上需要。

### `client online: no`

检查 agent service 和连接详情：

```bash
systemctl status webcodex-agent
journalctl -u webcodex-agent
```

同时确认 server URL、本地 token files 和 agent `allowed_roots`。缺失或为空的 `allowed_roots` 默认使用 `$HOME`；显式 `allowed_roots` 会覆盖该默认值。
