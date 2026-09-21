# WebPi 安全审查、开发验证与部署交接

日期：2026-09-19。状态：**安全加固与 Pi 桥接开发已在可访问工作树落地并验证；生产部署尚未完成，公网验收未通过。**

## 1. 实际工作目录与限制

本轮工具确认并修改的目录是 `E:\WebCodex Desktop\webpi-core`，分支 `webpi`，初始及当前 HEAD 为 `3cb489dce8557a94a3e56c51ed71c634e3169939`。已有未提交改动被保留，本轮没有提交、推送、发布、reset 或 clean。

文档中的独立部署目标是 `E:\WebPi\webpi-core`。对此目录的工具访问返回 `path_outside_allowed_roots`，因此没有通过 shell、复制或修改 allowed roots 绕过限制，也没有把旧工作树的成功当成新目录已部署。需要用户在本机 Desktop/Runner 中明确选择并登记实际部署目录，之后再检查并合并两个工作树的差异。

运行在 56542 的现有高权限 WebPi 进程无法通过当前权限安全重启；已有停止进程、启动 cloudflared 服务的操作返回 Access denied。运行进程的完整可执行文件路径也未能核实，因此没有根据 PID 猜测目标后强制终止。无关 WebCodex Desktop 服务没有被本轮部署操作修改。

## 2. Bearer 问题的实际结论

| 检查 | 观察结果 | 解释 |
| --- | --- | --- |
| 本机 `/openapi.json` | 200，真实 OpenAPI；servers 仍是 `http://localhost:8080` | Schema 可公开；旧运行实例没有采用目标公网 origin。 |
| 本机受保护 Actions / generic runtime / MCP，无 Bearer | 401 | 无令牌检查有效。 |
| 同样接口，任意非 `wc_` Bearer | 200；runtime 返回 `success:true`，MCP 返回工具目录 | 不是单纯 HTML 200，需要关闭不适合当前公网部署的 shared-key 快速启动模式。 |
| 假 Bearer 查询项目 | success，但项目数 0 | 尚未证明能访问用户项目或在项目内任意执行。不要把租户快速启动误报为已证实的管理员接管。 |
| 公网 `webpi.piforme.vip` | 初次 530；最后一轮全部所测路径为 403 纯文本，包括 OpenAPI | 两者都不能证明已到达正确 WebPi 实例并通过认证。当前公网不可作为 GPT Actions 已就绪的证据。 |

根因对应 `src/auth/shared_key.rs`：上游 shared-key 模式允许非托管 Bearer 作为轻量租户身份。它与托管 PAT 和 bootstrap 管理凭据不是同一合同。当前 WebPi 公网用途要求拒绝这些任意值，因此配置和子进程环境均强制关闭：

```text
WEBCODEX_SHARED_KEY_ENABLED=false
WEBCODEX_ALLOW_ANONYMOUS=false
WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=false
WEBCODEX_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED=false
```

当前可访问工作树的 `webpi.cmd status` 已显示 `public_url=https://webpi.piforme.vip`、`auth_config_hardened=true`，但同时显示 `live_unknown_bearer_status=200`、`live_auth_hardened=false`、`restart_required_for_auth=true`。**配置文件已准备不等于运行进程已切换。**

## 3. 已实施的安全修复与能力补齐

### 文件、搜索和发现

新增 `plugins/pi-bridge/src/project-files.ts`。除字符串路径校验外，还检查真实文件系统路径、祖先组件和打开后的文件身份；拒绝符号链接、junction、硬链接读取、特殊文件、Windows 设备别名、ADS、父目录穿越和敏感部署状态。读取在分配前实施 8 MiB 上限，并比较读取前后的文件状态。

搜索必须使用现有绝对路径 ripgrep，不通过项目 cwd 中的同名程序探测，也不在工具调用中下载程序。固定排除敏感文件和链接，设置超时与输出边界；搜索结果的文本重新通过受保护文件句柄读取，而不是直接发布子进程返回的任意行。文件夹和扩展发现也有扫描上限。图片读取保留原生 Pi 图像内容，并遵守插件结果上限。

### 扩展审批和生命周期

`approval-store.ts` 拒绝带链接的候选根、祖先和目录成员，限制候选体积、条目数、深度和审批文件大小；审批内容必须是合法绝对路径与 SHA-256。写入采用独占临时文件、同步落盘、原子替换，并使用排他锁避免并发更新覆盖撤销结果。

`pi-runtime-host.ts` 在后续工具/命令执行前重新核验已加载候选的批准内容；变更或撤销后拒绝旧调用。失败 reload 和 shutdown 不再留下仍可调用的旧 runner。补齐被阻止工具的结束事件，修复正常扩展文本被固定截成 4000 字符的问题。

新增 `pi_extension_tool_describe`，返回真实 TypeBox/JSON Schema、active 状态与 generation。网页 GPT 可按 `list → describe → call` 调用，而不是猜参数。保留并验证原有 native package install/update/remove、精确候选审批、撤销、技能、提示模板、命令、资源生命周期、图片及状态持久化链路。

### 认证与部署验证

子进程环境按不区分大小写的键清除继承的 WebCodex/OpenAI/Cloudflare 凭据，强制认证标志。缺少 WebPi 自己的 bootstrap 凭据时拒绝启动。公网 URL 拒绝控制字符和注入，env 更新有原子替换与并发内容检查。

新增 `security_smoke.py`，对三个受保护接口各测试四种无效凭据，另检查公开 Schema、公网 origin 和真实托管 PAT。拒绝把 HTML、530、403、重定向或错误包裹在 HTTP 200 中的结果当作通过；不记录凭据或响应正文，不向重定向地址转发 Authorization。

新增 `http_auth_fixture.py`：创建全新临时 loopback 实例、配置、数据和凭据，经真实 CLI pairing/login 获得托管 PAT，验证后停止自己的进程并确认监听器关闭。它不重启生产服务。

## 4. 验证证据

| 验证 | 本轮结果 | 范围 |
| --- | --- | --- |
| Pi bridge TypeScript 编译 | 通过 | 当前桥接源码 |
| Pi bridge / admin / 新安全回归 | 32 passed，0 failed，0 skipped | 原生资源与扩展、审批撤销、链接、搜索、图片、生命周期、状态、Schema 描述 |
| WebPi Python 单元测试 | 34 passed | 认证配置、隔离、公共 URL、验证器、隧道客户端、启动入口 |
| Plugin SDK 编译与测试 | 17 passed | stdio、Schema、序列化、异常与混合图片/文本 |
| Rust `webcodex-core` 插件定向测试 | 10 passed | 工具、Schema、协议内容与图片边界；非完整 workspace 测试 |
| 全新临时实例真实 HTTP 验收 | 14 checks passed | 12 个无效凭据案例全部 401、公开 Schema 200、真实 PAT 200成功 |
| Pi bridge 生产依赖 `npm audit --omit=dev` | 0 已知漏洞报告 | 执行时注册表已知公告；不覆盖未知漏洞、整个 Rust 依赖树或未审查第三方扩展 |
| 现有服务及公网 | 未通过 | 本机仍接受任意非托管 Bearer；公网最后为全路径 403 |

临时 HTTP fixture 使用的既有 CLI 二进制 SHA-256：`c6da6bd8f7b26cdec5ecbb5685c63ee54220fd456a7c996c6ed8f3d5fd386557`。此结果验证真实 HTTP 配置合同，不意味着已完整重建并发布全部 Rust 运行时。

早期无效 PNG 测试素材导致的图像用例失败，以及 Windows `\\?\` 与普通盘符路径比较导致的 fixture 误报，均在修正原因后通过复测；未删除测试或降低校验标准。

## 5. 不能宣称已经解决的边界

完整原生 Pi 还包含 TUI、模型/provider 控制以及 agent/turn/input 等模型循环事件。当前设计坚持网页 GPT 是唯一主推理循环，因此这些能力被如实标记为不适用或有限，不能宣称所有现有 Pi 扩展无需适配就能运行。

原生扩展和 npm/git 安装脚本执行主机代码。审批哈希不是 OS 沙箱，不覆盖任意动态导入依赖闭包；撤销不能自动回滚文件、网络请求或已启动的进程。同用户权限的恶意进程也不是这些 JavaScript 路径检查能够完全隔离的对手。已授权的 canonical shell 仍具有主机用户权限。运行不可信扩展需要另设低权限账户或受限 VM/容器、网络和目录权限。

本轮没有对整个 WebCodex 历史代码、所有传递依赖、所有扩展和无法访问的部署目录完成穷尽审计。可以报告已发现问题的具体修复与测试，不能保证“修复全部未知漏洞”或“零风险”。

## 6. 剩余步骤：先恢复可验证的部署目标，再启动公网

**首先需要用户在本机完成：** 在 Desktop/Runner 中选择、登记实际的 `E:\WebPi\webpi-core`；不要单纯复制旧目录覆盖它，也不要复制 `.webpi-state` 凭据。比较两个目录的代码、未提交改动、服务配置与启动目标后，按差异合并本轮修复并重新构建验证。当前工具无法越过 allowed-roots 拒绝代做这一步。

确认目标目录代码已更新后，以下命令才适用于该目标目录，而不是证明已执行的日志：

```powershell
.\webpi.cmd cloudflare-config https://webpi.piforme.vip
.\webpi.cmd status
```

用户需通过现有服务管理方式停止并重启**确认身份的 WebPi 实例**。已有实例占用 56542 时不要再启动第二份。普通前台部署使用：

```powershell
.\webpi.cmd run
```

在另一个本机终端先运行：

```powershell
.\webpi.cmd verify --with-action-token
```

只有本机认证负向与正向测试均通过，才启用/恢复 Cloudflare Tunnel。Cloudflare Public Hostname 应为 `webpi.piforme.vip`，后端为 `http://127.0.0.1:56542`，不是无关 WebCodex 服务端口。若 cloudflared 是 Windows 服务，由有权限的用户在服务管理器中操作；不要把凭据写进聊天或命令行。

随后在已配置的目标目录执行：

```powershell
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip --with-action-token
```

403 覆盖 OpenAPI、530、Challenge/HTML 或重定向都必须先查明边缘策略、路由和源站情况，不能以关闭 WebPi 认证来消除错误。Cloudflare 是传输入口，不代替 WebPi 的应用凭据校验。

最后由用户在 Custom GPT 编辑器导入：

```text
https://webpi.piforme.vip/openapi.json
```

认证选择 API Key / Bearer，密钥使用**实际 WebPi 部署自己签发的 Action PAT**，不是任意字符串、OpenAI API key、Cloudflare token 或 WebCodex 的凭据。保留 GPT 为仅自己可用，避免共享同一高权限凭据。将 `docs/WEBPI_GPT_INSTRUCTIONS.md` 配置为工作指令，并执行真实 `work_on_project → Pi capability/inventory → describe → call` 冒烟后，才开始受控自扩展。

Cloudflare/GPT Actions 路线不需要填写 OpenAI Secure MCP Tunnel ID；`run-web` 是另一条 OpenAI Secure MCP Tunnel 入口，不要混用。

## 7. 参考与关联文档

本地事实来自本次 WEBCODEX 源码检查、结构化命令输出与 Job 验证记录。关联文档：`WEBPI_ASSET_MAP.md`、`WEBPI_AUTH_BOUNDARIES.md`、`WEBPI_PI_PARITY.md`、`WEBPI_GPT_INSTRUCTIONS.md`。

公开合同参照官方文档：

- OpenAI GPT Action authentication: https://developers.openai.com/api/docs/actions/authentication
- Cloudflare Tunnel: https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/
- Pi native capability overview: https://pi.dev/
