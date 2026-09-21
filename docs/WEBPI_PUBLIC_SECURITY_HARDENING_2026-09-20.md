# WebPi 公网最小暴露与 Coding 体验安全加固 — 2026-09-20

## 目标

在不降低 WebPi Agent coding 能力、Runner 并发、Pi bridge、插件能力或当前 Action PAT scope 的前提下，缩小 `https://webpi.piforme.vip` 的公网攻击面，并增加不会误伤正常 coding 流量的错误认证限速。

## 生产策略

公网域名仅保留：

- `GET /openapi.json`
- `POST /api/actions/*`

公网隐藏为 404：

- `/mcp`
- `/api/tools/call`
- `/admin` 及其他非 Actions 管理面

Loopback `127.0.0.1:56542` 保持完整能力；`/mcp` 与 `/api/tools/call` 仍可使用合法 PAT 正常调用。

错误认证限速仅作用于公网认证失败：

- 120 次失败 / 60 秒窗口
- 超过后惩罚 60 秒
- 合法 PAT 不进入该计数器，因此不降低正常 coding burst、Runner 4 并发或 Job 吞吐
- 不对源码、patch、脚本 body 做内容型 WAF 拦截

生产配置由 `scripts/webpi/standalone.py::harden_server_env` 固化：

- `WEBPI_PUBLIC_ACTIONS_ONLY=true`
- `WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_ENABLED=true`
- `WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_MAX=120`
- `WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_WINDOW_SECS=60`
- `WEBPI_PUBLIC_INVALID_AUTH_RATE_LIMIT_PENALTY_SECS=60`

原有安全设置保持：

- `WEBPI_SHARED_KEY_ENABLED=false`
- `WEBPI_ALLOW_ANONYMOUS=false`
- `WEBPI_OAUTH2_SHARED_KEY_BRIDGE=false`
- `WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED=false`
- Origin 仅监听 `127.0.0.1:56542`

## TDD 与回归

新增红绿 TDD 覆盖：

- 公网只允许 OpenAPI + Actions，loopback 保留完整面
- Cloudflare `CF-Connecting-IP` 请求被识别为公网
- Host 与配置 public URL 匹配时识别为公网
- 错误认证 limiter 的窗口、惩罚和不同客户端隔离
- 同一公网来源已触发错误认证 429 后，合法 Action PAT 仍立即 200

通过的主要测试：

- WebPi Python tests: 49 / 49
- WebPi CLI library: 348 / 348
- webcodex-core + runner-config + validation: 381 / 381
- tool contracts: 213 / 213
- OpenAPI / Actions regression: 23 / 23
- frontend: 178 / 178
- frontend dist check: 31 files
- Plugin SDK runtime tests: 17 / 17
- Pi bridge: 32 / 32
- Rust formatting and git diff check: passed during acceptance

隔离 HTTP fixture 使用全新状态、全新 managed PAT 和独立 loopback listener，完整通过，并确认 fixture listener 回收，未修改生产服务。

## 生产切换

生产候选：

`target/webpi-public-security-hotfix/dogfood/webpi-server.exe`

SHA-256：

`6c68352cc5316a2b3974ccc67e8590a32714a52ad873fd40f9419663f66a5996`

已安装为：

`target/dogfood/webpi-server.exe`

生产配置精确备份：

`.webpi-state/deployment-backups/public-security-hotfix-f4fb14a0/webpi.env.before`

旧 Server 回滚副本：

`.webpi-state/deployment-backups/public-security-hotfix-f4fb14a0/webpi-server.exe.before`

配置备份保留原私有权限，bootstrap token、public URL、data path 均确认保持不变。

## 生产验收

本机：

- 无凭据、未知普通 Bearer、未知 managed Bearer、错误 scheme 对 Actions / tools / MCP 均按原策略返回 401
- 合法 PAT 调用 `/api/tools/call` = 200
- 合法 PAT 调用 `/mcp` = 200
- OpenAPI = 200
- Action PAT runtime read = 200

公网：

- 非法/缺失 Action Bearer = 401
- `/api/tools/call` = 404
- `/mcp` = 404
- `/admin` = 404
- 上述隐藏接口即使携带合法 PAT 也仍为 404
- `/openapi.json` = 200
- 合法 Action PAT runtime status = 200
- `list_projects` = 200，并解析 `agent:webpi-local:webpi-core`
- `read_files` = 200
- `search_project_texts` = success
- `apply_text_edits(dry_run)` = would_change 且没有创建文件
- `run_process` = success
- Job handoff + `observe_jobs` = terminal completed
- `plugin_tool describe -> call(pi_read)` = `engine=pi` 且内容非空

第一次完整公网 verify 的最后一个合法 PAT 请求出现一次网络级 `unreachable`；Cloudflared、Server 和 listener 均保持 Running。紧接着独立正向链 `runtime_status -> list_projects -> read_files` 全部 200，并以同一 assertion 重新运行完整公网 verify 后通过，因此记录为已复验解决的瞬时网络事件。

生产临时“真实写入后删除” smoke 在执行前被平台安全检查拦截，命令未执行，未修改任何生产文件；没有通过其他通道绕过。写入/删除语义由 Rust/Tool contract/Pi 测试和此前生产写读删验收覆盖。本轮生产采用 `apply_text_edits(dry_run)` 验证公网 body、权限和 schema 链。

## 远程手机使用边界

手机上的私人 WebPi GPT 可以通过 ChatGPT Actions 远程 coding，不需要手机连接家里局域网。实际链路：

`ChatGPT mobile -> OpenAI Actions -> Cloudflare -> webpi.piforme.vip -> 127.0.0.1 WebPi -> Runner/Pi`

必要条件：

- 主机开机且未睡眠/休眠
- Windows 用户会话中的 WebPi 计划任务正在运行
- Cloudflared Windows 服务正在运行
- WebPi Server 与 `webpi-local` Runner 在线
- 主机互联网连接正常
- GPT 保持私人可见，Action PAT 不泄漏

当前 `WebPi-Standalone` 是当前用户的交互式 at-logon 计划任务，不是 LocalSystem always-on 服务。因此机器重启后，在用户尚未登录 Windows 的阶段，只有 Cloudflared 服务运行并不足以让 WebPi Agent 工作；需要用户登录后计划任务启动。锁屏但未注销、且机器不睡眠时，现有进程可以继续工作。
