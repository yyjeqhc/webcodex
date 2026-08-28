# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

这里整理 WebCodex 部署中常见问题的实用检查。排障时不要粘贴或分享真实 tokens、env files、`Authorization` headers 或完整 `agent.toml` files。

## 运维检查清单

Server：

- `webcodex --version` 能打印版本。
- `webcodex server status --env-file /etc/webcodex/webcodex.env` 报告本地 server reachable。
- 在 server host 上，`curl http://127.0.0.1:8080/openapi.json` 返回 OpenAPI JSON。
- 如果使用 nginx 或其他 reverse proxy，public HTTPS 可访问。

Client：

- `webcodex-runner --version` 能打印版本。
- Hosted quick-start 使用
  `webcodex runner status --profile <connect 输出的 profile>`，应显示
  `runner mode: hosted local process`、`runner active: true` 和
  `client online: yes`。
- `webcodex runner status --profile workstation` 能读取本地 Runner config（`agent.toml`）。
- canonical project 的 `webcodex doctor` 通过；managed deployment 则使用
  `webcodex ops status --strict --server-url https://your-domain.example`。
- `listAgents` / `runtime_status` 显示 agent online。

## 常见问题

### `webcodex connect` 无法完成

`connect` 最多等待 15 秒来确认完整链路：Server 可访问、同 key 能看到 Runner、
同 key 能看到目标项目。错误信息会给出该 profile 的 Runner 日志路径。先检查：

```bash
webcodex runner status --profile <connect 输出的 profile>
webcodex runner logs --profile <connect 输出的 profile> --lines 100
```

确认 Server URL 指向 origin 根路径、Server 已启用 shared-key，并且 MCP 与 Runner
使用 trim 后完全相同的 key。不同 key 按设计看不到该 Runner 和项目。如果本次
命令启动的 Runner 无法注册，`connect` 会停止它；本地配置会保留，修复后可重试。

Hosted 日志位于
`$XDG_STATE_HOME/webcodex/clients/<profile>/runner.log`（或
`~/.local/state` 下的对应默认路径）。Runner 运行期间会轮转日志，只保留当前文件和
`.1`、`.2`，每个约 10 MiB。`agent logs --lines` 只做有界尾部读取，并可按需跨这些
归档补齐；`--follow` 会在轮转后跟随新的当前文件。不要通过编辑内部 Runner state
来修复注册问题。

默认每个 shared-key group 最多注册 16 个 Runner，单个 Server 进程全局最多保留
1,024 个 shared-key Runner。离线 shared-key Runner 记录会在 24 小时后清理。
Managed Agent Token 不受这些 shared-key 数量和保留期限限制；所有 Runner 注册都有
64 个项目的输入安全上限。

### `connect` 拒绝 `wc_*`

这是预期边界。`wc_pat_*`、`wc_agent_*`、`wc_acct_*` 和其他 `wc_*` 都是 managed
credentials，绝不会 fallback 成 shared key。Hosted shared-key 流程请使用另一个
随机 key；需要 managed identity 时使用 `webcodex login`。

`webcodex token generate` 只进行离线材料生成，不会向远程 Server 注册生成的
credential，因此不要把其输出当成 hosted shared key。

### Hosted Runner 已退出或 PID state 过期

重新执行相同的 `webcodex connect`。Profile lock 会阻止并发启动重复进程；配置
相同且仍存活的 Runner 会被复用，stale PID 或指向非 Runner 的 PID state 会先被
丢弃，再启动唯一的替代 Runner。显式停止：

```bash
webcodex runner stop --profile <connect 输出的 profile>
```

Key 只保存在受保护的 profile 配置中，status 不会打印，也不会写入项目 checkout。

### `webcodex server install` 提示 service already exists

只有在明确要替换现有 unit 时才使用 `--overwrite`：

```bash
sudo webcodex server install \
  --env-file /etc/webcodex/webcodex.env \
  --bin /usr/local/bin/webcodex-server \
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

### 端到端追踪一次 tool invocation

自托管 forensic 排障时，在 Server 设置 `WEBCODEX_TOOL_REQUEST_TRACE=full`，只复现
一次目标调用。在 Server journal 找到 `server_trace_id`，然后检查：

```bash
TRACE_ROOT=/var/lib/webcodex/tool-request-traces
find "$TRACE_ROOT/<server_trace_id>" -maxdepth 2 -type f -print
cat "$TRACE_ROOT/<server_trace_id>/events.jsonl"
zstd -dc "$TRACE_ROOT/<server_trace_id>/payloads/<payload>.json.zst" | jq .
```

对于 `tools/call`，`raw_request_body`（API）或 `raw_arguments`（MCP）表示 WebCodex
进行 wrapper/session normalization 之前 Server 实际收到的内容；
`effective_arguments` 是真正送入 runtime/connector dispatch 的 semantic arguments。
排查客户端声称已发送但 Server 最后看不到的 optional field（例如
`ack_session_context_revision`）时，先直接对比这两层。`final_response` 保存 Server
产生的有界 JSON response。

如果调用到达 Runner，`events.jsonl` 会记录 `runner_request_id`、Runner
client/instance、transport 与注册时报告的 build version/commit。`runner_request`
payload 是 Server 实际 enqueue 给该 Runner 的 exact typed request；如果字段疑似在
派发构造阶段消失，可以直接与 `effective_arguments` 对比。Runner 也启用 tool request
trace 时，可直接在 Runner journal 搜索同一个 `runner_request_id`。后续 raw Runner
result 与 Job update 会通过 Server 的有界 correlation index 回写到原始 Server trace。

trace code 不会读取 WebCodex ingress HTTP `Authorization` header，但 `full` 本来就是
raw forensic 模式：如果 credential-like value 被放进 tool argument、script/stdin、
Runner request 或 Runner response，它会作为 payload data 被完整记录。

没有 payload 文件不代表 payload 是空的，也不代表内容被截断。检查 Server journal
中的 `tool_trace_capture_omitted` / `trace_disk_budget_exceeded` 或
`tool_trace_capture_failed`。这些 trace 失败只影响诊断，不会让底层 tool 失败。如果
WebCodex 已记录 `tool_handler_returned` 而 client 没收到 response，还要按同一请求时间
关联 reverse proxy access log，因为 handler return 并不证明 client delivery。

### Client 显示 `webcodex: command not found`

把 CLI 安装或 symlink 到 client 的 `PATH`，例如：

```bash
sudo ln -s /opt/webcodex/bin/webcodex /usr/local/bin/webcodex
```

请使用你主机上的实际安装路径。

### Client 误运行 `pairing create`，且 `/etc/webcodex/webcodex.env` 缺失

`webcodex pairing create` 是 server/admin-side 命令，需要 server bootstrap env file。朋友或 client 机器应运行 `webcodex login <server-url> --code <wc_pair_...>`（高级替代：`webcodex client enroll`），并使用 server owner 发来的短期 `wc_pair_*` code。

机器之间只复制 `wc_pair_*` code。不要复制 `WEBCODEX_TOKEN`、user API tokens、agent tokens、env files 或完整 `agent.toml` files。

### Client 上 doctor 警告 `binary webcodex not found in PATH`

这在 agent-only client machines 上可能是正常的。Agent-only client 需要公开 `webcodex` CLI 和 `webcodex-runner`；`webcodex-server` 只在 server host 上需要。

### `client online: no`

Hosted `connect` profile 使用上面的 profile-specific status 和日志路径。
systemd-managed deployment 则检查 agent service 和连接详情：

使用安装 service 时选择的同一 scope：

```bash
# 普通 user service
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100

# 管理员管理的 system service
sudo webcodex runner status --scope system
sudo webcodex runner logs --scope system --lines 100
```

同时确认 server URL、本地 token files 和 agent `allowed_roots`。缺失或为空的 `allowed_roots` 默认使用 `$HOME`；显式 `allowed_roots` 会覆盖该默认值。

### `listRuntimeTools` full response 过大

完整 `listRuntimeTools` 会包含展开后的 schemas 和 metadata。GPT Actions 的日常
discovery 应优先使用 `callRuntimeTool` 且 `tool="tool_manifest"`。需要聚焦
schema/debug 时，再调用 `listRuntimeTools`，并传
`summary_only=true` 加 `category`、`features` 或 `limit`。

### GPT Action 仍在使用旧 schema

从已部署的 `/openapi.json` 重新导入 OpenAPI schema，然后检查 operation count。
当前推荐值是 25，GPT Actions 上限是 30。如果 count 超过 30，不要直接部署该
schema；artifact upload tools 应继续作为 runtime-only tools 通过
`callRuntimeTool` 使用，不要新增 dedicated Actions。兼容编辑工具也应继续通过
`callRuntimeTool` 使用。

### MCP tool list 看起来是旧的

重连或重启 MCP client，让它重新执行 `initialize` 和 `tools/list`。如果 server
刚升级，确认 public HTTPS 已指向新 service，并检查 `journalctl -u webcodex`
中是否有 startup 或 auth errors。

### Agent offline

先运行 `runtime_status` 或 `listAgents`，再在 agent host 上检查：

```bash
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100
# 管理员管理的 system service 使用 `sudo ... --scope system`。
```

确认 agent server URL、token file、service user 和 `allowed_roots`。

### Token type 错误

Hosted quick-start 中，MCP 与 Runner 使用同一个非 `wc_` shared key。Managed
mode 中，GPT Actions、MCP 和普通 REST/project API 使用
`webcodex-user-token`（`wc_pat_*`）；Runner 令牌（`wc_agent_*`）只给
Runner transport 使用——`webcodex login` 之后它内联在 `agent.toml` 中，
没有单独的 `webcodex-runner-token` 文件（高级的 `webcodex client enroll`
流程会写入一个）。把 `wc_agent_*` 放入
`--token` 或 `--token-file` 后得到 403，正是预期安全边界；应改用生成的
`webcodex-user-token`。新版 CLI 也会在不打印完整 token 的前提下诊断这个错误。
`WEBCODEX_TOKEN` 面向 bootstrap/admin，
不应复制到 GPT Actions、MCP 或 agent config。

### 一条命令能看到 Runner service，另一条却看不到

install、status、start、stop、restart、logs 和 uninstall 必须传入相同的
`--scope`。user scope 调用 `systemctl --user` / `journalctl --user`，并使用
`$XDG_CONFIG_HOME/systemd/user`（未设置时为 `$HOME/.config/systemd/user`）；
system scope 调用 system manager，并使用 `/etc/systemd/system`。

非 root 调用者默认使用 user scope。root 调用者默认使用 system scope，但安装时
仍需提供非 root `--user`；有意使用 root Runner 还必须传
`--allow-root-runner`，且不推荐这样做。install 时若使用了自定义
`--service-file`，后续命令也要传同一 absolute path 与 scope。WebCodex 不会静默
迁移或覆盖另一 scope 的 unit。

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
