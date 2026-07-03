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
- `webcodex-cli agent status --profile workstation` 能读取本地 agent config。
- `webcodex-cli doctor --strict --profile workstation --server-url https://your-domain.example` 通过。
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

### `listRuntimeTools` full response 过大

完整 `listRuntimeTools` 会包含展开后的 schemas 和 metadata。GPT Actions 的日常
discovery 应优先使用 `callRuntimeTool` 且 `tool="tool_manifest"`。需要聚焦
schema/debug 时，再调用 `listRuntimeTools`，并传
`summary_only=true` 加 `category`、`features` 或 `limit`。

### GPT Action 仍在使用旧 schema

从已部署的 `/openapi.json` 重新导入 OpenAPI schema，然后检查 operation count。
当前推荐值是 27，GPT Actions 上限是 30。如果 count 超过 30，不要直接部署该
schema；artifact upload tools 应继续作为 runtime-only tools 通过
`callRuntimeTool` 使用，不要新增 dedicated Actions。

### MCP tool list 看起来是旧的

重连或重启 MCP client，让它重新执行 `initialize` 和 `tools/list`。如果 server
刚升级，确认 public HTTPS 已指向新 service，并检查 `journalctl -u webcodex`
中是否有 startup 或 auth errors。

### Agent offline

先运行 `runtime_status` 或 `listAgents`，再在 agent host 上检查：

```bash
systemctl status webcodex-agent
journalctl -u webcodex-agent
```

确认 agent server URL、token file、service user 和 `allowed_roots`。

### Token type 错误

GPT Actions 和 MCP 应使用 managed `wc_pat_*` token，或部署允许的 shared key。
`wc_agent_*` 只给 `webcodex-agent` 使用。`WEBCODEX_TOKEN` 面向 bootstrap/admin，
不应复制到 GPT Actions、MCP 或 agent config。

### 非 git smoke workspace 不能运行 `git_status`

`git_status` 需要 git repository，部署 smoke 才能得到 clean 结果。为 disposable
smoke project 初始化 git 并创建初始 commit，或把 smoke 指向另一个安全的
agent-backed git project。

### `operation_count` 超过 30

GPT Actions surface 必须保持在 30 operations 以内。runtime-only tools，包括
chunked artifact upload tools，应继续放在 `callRuntimeTool` 后面，除非有明确的
产品决策和 operation budget 来新增 dedicated Action。

### `artifact_upload_chunk` 报 `path` 缺失

`artifact_upload_chunk`、`artifact_upload_finish` 和 `artifact_upload_abort`
必须重复 `artifact_upload_begin` 使用的完全相同 `path`。这是为了把 opaque
`upload_id` 绑定到请求的目标 artifact path。

### `application/octet-stream` 因 unsafe extension 被拒绝

使用安全的 project-relative artifact path，并让 MIME type 与文件扩展名匹配。
Smoke tests 建议使用简单 `.txt` 路径和 `text/plain`。避免 secret-like paths、
绝对路径、`.env*`、`.git`、token/credential paths，以及不安全的二进制扩展名。
