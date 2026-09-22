# Troubleshooting

[English](TROUBLESHOOTING.md) | [简体中文](TROUBLESHOOTING.zh-CN.md)

这里整理 WebCodex 部署中常见问题的实用检查。排障时不要粘贴或分享真实 tokens、env files、`Authorization` headers 或完整 `runner.toml` files。

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
- `webcodex runner status --profile workstation` 能读取本地 Runner config（`runner.toml`）。
- canonical project 的 `webcodex doctor` 通过；managed deployment 则使用
  `webcodex ops status --strict --server-url https://your-domain.example`。
- `list_runners` / `runtime_status` 显示 Runner online。

## 先判断故障发生在哪一层

当 ChatGPT 提示 app、插件或工具被 block 时，不要第一步就重启 Runner。先判断这次
请求是否真正到达 WebCodex。

| 现象 | 更可能的层 | 下一步 |
| --- | --- | --- |
| ChatGPT 返回 `FORBIDDEN: This conversation does not support developer MCPs`，或提示当前会话已禁用 developer MCP server；同时 WebCodex 没有观察到对应请求 | ChatGPT Host / conversation 的 MCP admission | 从 operator/Runner 主机独立验证 WebCodex，再单独排查 Host 连接 |
| WebCodex 返回 HTTP 401/403、MCP authentication error，或正常 structured ToolResult failure | Server auth / authorization / ToolRuntime | 检查 user/API credential、OAuth scope、Server 日志与精确 WebCodex error |
| `runtime_status` 能成功执行，但显示 Runner offline 或 project missing | Runner / project registration | 在 Runner 主机执行 `webcodex runner status` 并查看有界日志 |
| `plugin_tool` 已到达 WebCodex，并返回 `ready=false`、`plugin_check_busy`、`plugin_reload_busy` 等 Plugin diagnostic | WebCodex Native Tool Plugin runtime | 使用 `webcodex plugin check/list/describe/reload`，并查看 [Native Tool Plugin 文档](PLUGINS.zh-CN.md) |

第一行尤其重要：如果 ChatGPT Host 根本没有 dispatch `runtime_status`，界面显示的
`FORBIDDEN` **不是** WebCodex 的 `runtime_status` 返回值。一个没有到达 Server 的
请求，无法通过重启或重配 Runner 来修复。

### ChatGPT 提示 developer MCP 被禁用或当前会话不支持

[Issue #500](https://github.com/yyjeqhc/webcodex/issues/500) 已出现一组很有代表性的
对照：同一个会话此前已经正常使用 WebCodex，随后连续得到：

```text
FORBIDDEN: This conversation does not support developer MCPs
```

当时同一 Server、Runner、project 与本地 workspace 通过独立路径仍然可用；之后没有
修改 WebCodex 配置，同一个 ChatGPT 会话又自行恢复。这更符合 Host/conversation 级
developer-MCP routing / permission state 的间歇性异常，而不是持久的 Runner 故障。

推荐按下面顺序排查：

1. 记录完整错误文本、发生时间、时区，以及使用的 ChatGPT surface
   （例如 web/desktop/mobile）。
2. 脱离这个会话，独立验证 WebCodex。Hosted profile 可执行：

   ```bash
   webcodex --version
   webcodex-runner --version
   webcodex runner status --profile <connect 输出的 profile>
   webcodex runner logs --profile <connect 输出的 profile> --lines 100
   ```

   Managed deployment 还可以使用只读 operator 检查：

   ```bash
   webcodex ops status --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE" --strict
   webcodex ops runners --server-url "$SERVER_URL" --token-file "$USER_TOKEN_FILE"
   ```

   systemd Runner 要使用安装时相同的 `--scope user|system`。
3. 判断失败的 ChatGPT 调用有没有到达 Server。普通日志不够时，使用下面
   [捕获一次失败的 tool call](#捕获一次失败的-tool-call) 的单次有界 trace 流程。
   如果 Server/Runner 独立检查正常，而且精确复现时间没有对应 inbound request，这是
   “故障发生在 WebCodex 之前”的强证据。若 trace 文件缺失，仍应先检查 trace-capture
   warning，不能单凭“没有文件”下结论。
4. 在 ChatGPT 中确认 Developer Mode / developer MCP app 对当前 conversation/workspace
   仍然可用；Host UI 提供时可以重新连接或重新启用同一个 MCP。使用相同 endpoint 在
   新会话中测试也是很有价值的隔离手段：如果新会话正常、旧会话异常，不要为了旧会话
   去改 Runner/project 配置。
5. 只有独立检查确实发现 WebCodex 问题时，才修改 WebCodex：例如 Server 不可达/auth
   失败、Runner offline、project missing，或真正的 `plugin_tool` diagnostic。

复制一个新 MCP、只改 display name 有时可能让 Host 建立新的 mount，但它不是可靠的
WebCodex 修复，也不能解释原来的 Host state。对于上面的精确 Host-level error，也不要
仅因为它就旋转 token、改写 `runner.toml`、重新注册 project，或反复重启本来健康的
Runner。

反馈此类问题时，建议只提供安全 evidence：

- 精确 Host error、复现/恢复时间与时区；
- ChatGPT surface，以及相同 MCP 在新会话中是否可用；
- `webcodex --version` 与 `webcodex-runner --version`；
- 已脱敏的 `webcodex runner status` / `webcodex ops status`；
- 如果另一个会话/client 仍能调用 `runtime_status`，提供其已脱敏的 build / connection-layer summary；
- 故障时间点 Server 是否观察到对应 request/trace。

不要公开 access token、OAuth secret、`Authorization` header、完整 env file、完整
`runner.toml`，也不要未经检查/脱敏直接贴 full raw trace。

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
`.1`、`.2`，每个约 10 MiB。`runner logs --lines` 只做有界尾部读取，并可按需跨这些
归档补齐；`--follow` 会在轮转后跟随新的当前文件。不要通过编辑内部 Runner state
来修复注册问题。

默认每个 shared-key group 最多注册 16 个 Runner，单个 Server 进程全局最多保留
1,024 个 shared-key Runner。离线 shared-key Runner 记录会在 24 小时后清理。
Managed Runner Token 不受这些 shared-key 数量和保留期限限制；所有 Runner 注册都有
64 个项目的输入安全上限。

### `connect` 拒绝 `wc_*`

这是预期边界。`wc_pat_*`、`wc_agent_*`、`wc_acct_*` 和其他 `wc_*` 都是 managed
credentials，绝不会 fallback 成 shared key。Hosted shared-key 流程请使用另一个
随机 key；需要 managed identity 时使用 `webcodex login`。

`webcodex tokens generate` 只进行离线材料生成，不会向远程 Server 注册生成的
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

### 捕获一次失败的 tool call

自托管 Server 的 status/log 仍不足以定位问题时，可以临时开启 full tool-request
trace，并且只复现**一次**目标调用：

```text
WEBCODEX_TOOL_REQUEST_TRACE=full
WEBCODEX_TOOL_REQUEST_TRACE_DIR=/var/lib/webcodex/tool-request-traces
```

按当前部署方式让新的 Server 环境生效，记录本次复现的准确时间、tool 和 error，然后
检查配置 trace 目录中最新生成的记录。完成捕获后应重新关闭 `full` trace。

Full trace 可能包含源码、patch、script/stdin、命令输出、user message，以及本身就
出现在 tool payload 中的 secret。未经检查/脱敏，不要把 raw trace 公开到 issue 或聊天。
如果没有生成新记录，应先查看 Server journal 中的 trace-capture warning，不要把“没有
capture”解释成“请求为空”。

内部 trace layout、request/Runner correlation、payload layer 与 capture omission 语义见
maintainer-only 的 [Tool Request Tracing](agent/tool-request-tracing.md) contract。

### Client 显示 `webcodex: command not found`

把 CLI 安装或 symlink 到 client 的 `PATH`，例如：

```bash
sudo ln -s /opt/webcodex/bin/webcodex /usr/local/bin/webcodex
```

请使用你主机上的实际安装路径。

### Client 误运行 `pairing create`，且 `/etc/webcodex/webcodex.env` 缺失

`webcodex pairing create` 是 server/admin-side 命令，需要 server bootstrap env file。朋友或 client 机器应运行 `webcodex login <server-url> --code <wc_pair_...>`，并使用 server owner 发来的短期 `wc_pair_*` code。

机器之间只复制 `wc_pair_*` code。不要复制 `WEBCODEX_TOKEN`、user API tokens、Runner tokens、env files 或完整 `runner.toml` files。

### Client 上 doctor 警告 `binary webcodex not found in PATH`

这在 Runner-only client machines 上可能是正常的。Runner-only client 需要公开 `webcodex` CLI 和 `webcodex-runner`；`webcodex-server` 只在 server host 上需要。

### `client online: no`

Hosted `connect` profile 使用上面的 profile-specific status 和日志路径。
systemd-managed deployment 则检查 Runner service 和连接详情：

使用安装 service 时选择的同一 scope：

```bash
# 普通 user service
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100

# 管理员管理的 system service
sudo webcodex runner status --scope system
sudo webcodex runner logs --scope system --lines 100
```

同时确认 server URL、本地 token files 和 Runner `allowed_roots`。缺失或为空的 `allowed_roots` 默认使用 `$HOME`；显式 `allowed_roots` 会覆盖该默认值。

### `tool_manifest` discovery 范围过大

GPT Actions 应直接调用 canonical `tool_manifest` operation，并优先传 exact
`tool_name`，或使用 `category` / `intent` filter 来保持 discovery 紧凑。generic
Actions surface 已不再暴露退休的 `listRuntimeTools` facade。

### GPT Action 仍在使用旧 schema

从已部署的 `/openapi.json` 重新导入 OpenAPI schema，然后检查 operation count。
该数量由当前 Adaptive Direct projection 加 `call_runtime_tool` 动态派生，不应再和
固定“推荐数量”比较。生成 surface 必须保持在 GPT Actions 的 30-operation ceiling
以下；如果达到 ceiling，应调整 canonical Adaptive projection 或真实的 protocol
exception，而不是静默截断 schema。

### MCP tool list 看起来是旧的

重连或重启 MCP client，让它重新执行 `initialize` 和 `tools/list`。如果 server
刚升级，确认 public HTTPS 已指向新 service，并检查 `journalctl -u webcodex`
中是否有 startup 或 auth errors。

### Runner offline

先运行 `runtime_status` 或 `list_runners`，再在 Runner host 上检查：

```bash
webcodex runner status --scope user
webcodex runner logs --scope user --lines 100
# 管理员管理的 system service 使用 `sudo ... --scope system`。
```

确认 Runner server URL、token file、service user 和 `allowed_roots`。

### Token type 错误

Hosted quick-start 中，MCP 与 Runner 使用同一个非 `wc_` shared key。Managed
mode 中，GPT Actions、MCP 和普通 REST/project API 使用
`webcodex-user-token`（`wc_pat_*`）；Runner 令牌（`wc_agent_*`）只给
Runner transport 使用——`webcodex login` 之后它内联在 `runner.toml` 中，
没有单独的 `webcodex-runner-token` 文件。把 `wc_agent_*` 放入
`--token` 或 `--token-file` 后得到 403，正是预期安全边界；应改用生成的
`webcodex-user-token`。新版 CLI 也会在不打印完整 token 的前提下诊断这个错误。
`WEBCODEX_TOKEN` 面向 bootstrap/admin，
不应复制到 GPT Actions、MCP 或 Runner config。

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
Runner-backed git project。

### `operation_count` 超过 30

生成的 GPT Actions surface 必须保持在 30 operations 以下。long-tail runtime
tools（包括 chunked artifact upload tools）继续通过 `call_runtime_tool` 调用；direct
operations 从 canonical Adaptive Direct surface 派生，不维护单独的 Actions allowlist。

### `artifact_upload_chunk` 报 `path` 缺失

`artifact_upload_chunk`、`artifact_upload_finish` 和 `artifact_upload_abort`
必须重复 `artifact_upload_begin` 使用的完全相同 `path`。这是为了把 opaque
`upload_id` 绑定到请求的目标 artifact path。

### `application/octet-stream` 因 unsafe extension 被拒绝

使用安全的 project-relative artifact path，并让 MIME type 与文件扩展名匹配。
Smoke tests 建议使用简单 `.txt` 路径和 `text/plain`。避免 secret-like paths、
绝对路径、`.env*`、`.git`、token/credential paths，以及不安全的二进制扩展名。
