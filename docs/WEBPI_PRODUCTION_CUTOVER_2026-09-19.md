# WebPi 生产切换记录 — 2026-09-19

## 结论

本机生产切换已完成；公网端到端验收尚未完成。以下为本轮工具实测，不把上一轮临时实例测试当成生产证据。

目标为 `E:\WebPi\webpi-core`、`127.0.0.1:56542` 和 `https://webpi.piforme.vip`。没有操作 WebCodex Desktop 的进程、安装状态或凭据。工作树保留原有未提交改动，未提交、推送、发布或打标签。

## 已完成的生产操作

1. 核验三个新二进制的 SHA-256 与 `WEBPI_BUILD_ACCEPTANCE_2026-09-19.md` 的验收字节完全一致。
2. 使用现有生产 Action PAT 确认旧实例及 Runner 没有活动 Job 或未完成请求。
3. 创建回滚备份，保留旧三个二进制、配置和启动脚本。按原 DACL 保护文件，在写入秘密字节之前设置权限。
4. 按完整可执行路径、PID 和原生创建时间核对旧 Runner 后停止它，让原协调进程完成其他旧组件清理；确认旧四个进程均已退出、56542 已关闭。
5. 停机后复制独立 WebPi 状态目录的 16 个文件；逐文件比对字节摘要；备份数据库 `PRAGMA quick_check` 通过。
6. 执行 `webpi.cmd migrate-config`，仅迁移本安装的配置键；随后配置公网 origin。核对 bootstrap、Action PAT、Runner 配置和数据目录均未更换。
7. 建立并启动 Windows 计划任务 `\WebPi-Standalone`。任务执行当前根目录的 `standalone.py run`，身份为当前用户、`Limited`、`Interactive`；登录触发，重复实例策略 IgnoreNew，运行时限 `PT0S`。它不是受工具一小时生命周期限制的临时 Job，也不是管理员/LocalSystem 服务。

## 最后成功观察到的本机进程

| 组件 | PID | 可执行路径 |
| --- | ---: | --- |
| CLI | 26744 | `E:\WebPi\webpi-core\target\dogfood\webpi.exe` |
| Server | 22904 | `E:\WebPi\webpi-core\target\dogfood\webpi-server.exe` |
| Runner | 24292 | `E:\WebPi\webpi-core\target\dogfood\webpi-runner.exe` |

Server 于 `2026-09-19T13:44:19.2484280Z` 启动。端口最后观察为 `127.0.0.1:56542`，由上述 Server 持有。PID 仅是本轮观察记录，后续管理必须重新核对身份，不能照搬 PID。

## 已通过的生产验收

| 检查 | 实际结果 |
| --- | --- |
| 原验收二进制一致性 | 三个 SHA-256 全部匹配 |
| 配置迁移完整性 | 9 项检查通过：旧配置键清除、bootstrap 保留、数据路径保留、loopback、正确 origin、宽松模式关闭、Action PAT 保留、Runner 配置保留、项目根一致 |
| 本机 HTTP 鉴权与 Schema | 15 项检查通过 |
| 三条受保护路径的无效凭据 | `/api/actions/runtime_status`、`/api/tools/call`、`/mcp`，缺失 Bearer、伪造普通 Bearer、伪造托管 Bearer、错误认证方案共 12 例全部 401 |
| 本机公开 OpenAPI | 真正的 WebPi Schema，公网 origin 正确 |
| 真实生产 Action PAT | 200 且成功读取启用认证的 WebPi 状态 |
| WebPi 项目 | 实际返回 `agent:webpi-local:webpi-core`，Runner 在线 |
| Pi 插件目录 | 当前在线目录 ready，23 个工具，不再是旧的 16 个 |
| 真实 Action PAT 调用 Pi | 描述绑定后调用 `pi_read` 读取本项目 `README.md`，200、非应用错误、`engine=pi`，读到 WebPi 内容 |
| 真实 Action PAT 的项目写入/读取 | 通过 canonical 工具创建并读取专用验收文件；未修改源码 |
| 验收文件清理 | 主操作通道以精确 SHA 保护删除本轮创建的文件；最终观察文件不存在 |

最后一次 `webpi.cmd status` 报告：

```text
server_online=true
config_migration_required=false
native_binaries_present=true
live_unknown_bearer_status=401
live_auth_hardened=true
auth_hardened=true
restart_required_for_auth=false
```

其中 `tunnel_configured=false` 是 WebPi 的 OpenAI Secure MCP Tunnel 配置项，不是对 Cloudflare Windows 服务状态的判断；当前采用 Cloudflare HTTPS/Actions 路线，不需要借用 OpenAI Tunnel 配置。

## 未完成与明确限制

- `Cloudflared` Windows 服务仍为 Stopped。检查服务控制权限得到 Win32 错误 5；随后一次正常 `Start-Service Cloudflared` 尝试也失败，返回 `CouldNotStartService`。未提升权限、未修改服务 ACL、未读取其 token 内容，也未启动替代 cloudflared 进程绕过服务权限。
- 最后公网检查为 14 项失败：13 个实际 HTTP 请求均返回 530，Schema origin 校验也失败。没有把公网错误页当成成功，没有向这一轮公网探测发送真实 Action PAT。
- 一批 Pi 敏感路径负向探测、一批 guarded edit/进程执行验收，以及后续一批追加 MCP 正向检查被工具安全检查拦截。未通过其他接口重复这些受拦探测。普通 README 的 Pi 读取是独立的非敏感成功检查。生产环境中的这些被拦项目不能记为通过；此前隔离测试中的相应结果也不能冒充本轮线上验收。
- 当前未证明网页 GPT 的完整调用链成功，未执行生产扩展安装、信任提升或自动审批。
- 计划任务按当前用户登录启动；注销、重启后的自动恢复和无人登录时持续服务没有现场验收。不宣称等同于开机即运行的系统级服务，也不宣称同用户扩展彼此拥有 OS 沙箱。
- 曾发生一次 WEBCODEX 控制隧道瞬断，随后只读状态查询恢复在线；没有重复迁移或重复启动。它与 Cloudflare 公网 Tunnel 是不同通道。
- 最初停止检查因 CIM 微秒时间与原生 FILETIME 精度差异拒绝执行；重新读取精确原生时间后按同一原生时间身份校验完成停止，没有移除身份检查。

## 回滚与证据位置

```text
E:\WebPi\webpi-core\.webpi-state\deployment-backups\cutover-b5347c3c\
```

包含 `legacy-binaries`、`preflight-state`、`state-before`、`launch-scripts-before`、`receipt.json`、`acceptance-context.json`、`acceptance-file-read.json`、`pi-read-acceptance.json`、`processes-after.json`、`webpi-task.xml`。其中有配置和凭据备份，不应上传到聊天或公开分享。

配置迁移还创建了精确原文件备份：

```text
.webpi-state/server/webpi.env.pre-webpi-namespace-2a6e5de1a89d4b6cb1cbbee69babf6b7.bak
```

旧配置存在宽松认证行为，回滚必须保持公网关闭，并协调匹配的旧程序和启动脚本；不能将备份直接覆盖到在线新实例后公开。当前新实例正常，不建议无故回滚。

## 必须由本机操作员完成的下一步

在管理员 PowerShell 中，仅启动已经存在的 Cloudflared 服务：

```powershell
Start-Service -Name Cloudflared
Get-Service -Name Cloudflared
```

不要重跑迁移，不要再启动第二份 `webpi.cmd run`，也不要把整个网页编码运行时提升为管理员。当前 WebPi 已由普通用户计划任务运行。

服务启动后，先在普通 PowerShell 检查公开 Schema 与无效认证：

```powershell
Set-Location 'E:\WebPi\webpi-core'
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip
```

只有以上检查通过，再执行真实 Action PAT 的公网正向验收：

```powershell
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip --with-action-token
```

最终还需要网页 GPT 重新导入 WebPi OpenAPI 并实际调用。不得将 530、HTML、全路径 403 或只成功取得静态 Schema 当作整链验收通过。
