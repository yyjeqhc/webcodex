# WebCodex CLI

`webcodex` 是统一的操作与开发命令行。它覆盖项目设置、Server 与 Runner 生命周期、
设备接入、令牌管理和只读运维检查。

远程操作（用户、令牌、pairing、运维检查）走 Server HTTP API，CLI 是它们的便捷
客户端；本地操作（项目设置、服务管理、task 审查决策）直接运行在主机上，无法通过
Server API 完成。

从源码构建会产生三个二进制：

- `webcodex` —— 本文档介绍的统一命令。
- `webcodex-server` —— Server 进程（用 `webcodex server ...` 启动与托管）。
- `webcodex-runner` —— 实际执行项目工作的 Runner 进程（用
  `webcodex runner ...` 启动与托管）。

`webcodex --help` 会列出顶层命名空间。下面按命名空间说明各自的用途。本文是完整 CLI reference，不要求普通用户在第一次成功使用前理解所有命令、凭据或内部配置字段。

日常使用推荐按[完整使用指南](PERSONAL_SETUP.zh-CN.md)建立普通 Server + Runner。`webcodex share` 则是 Linux、macOS、Windows 上用于**临时试用/分享一个项目**的显式入口；它会为本次前台运行准备临时项目环境、Server、Runner 和可选 Tunnel，退出即结束。Linux/macOS 在交互式 Git checkout 中运行裸 `webcodex` 自动进入 `share`，也只是这个临时试用的 convenience shortcut。

## 命令总览

### 环境配置

个人新安装可执行 `webcodex environment configure --create --runner --scope user`，
再运行 `webcodex environment configure-tunnel default --host embedded` 和
`webcodex environment status`。持久环境推荐由 Server 托管：Server 只在启动时装载 profile，
配置过程不会安装 standalone Tunnel 服务，也不会暗中重启正在工作的 Server。同机用户/Runner
凭据由内部生成，无需手动 pairing。Linux 退出登录后的持续运行取决于 linger；macOS
LaunchAgent 与 Windows 用户计划任务需要该用户保持登录。旧服务不会被自动接管。
[生命周期与兼容边界](implementation/personal-environment-lifecycle.md)。

`webcodex environment` 与 Desktop 调用同一配置核心。该命名空间配置本机持久环境；原有项目级 `webcodex setup` 含义保持不变。安装包供应与原生验收状态见[统一安装指南](unified-installation.zh-CN.md)和[部署验收清单](unified-deployment-validation.md)。

| 命令 | 用途 |
| --- | --- |
| `webcodex environment configure [--scope user\|system]` | 选择创建/加入与本机 Runner/查看端。新环境默认用户级服务，已有环境保留原作用域；system 明确选择机器启动服务并请求系统授权。 |
| `configure --create --project PATH` / `configure --create --no-project` | 创建或继续配置本机 Server + Runner / 仅 Server 环境。 |
| `configure --join URL --project PATH --code-stdin` | 带项目加入；从 stdin 读取一次性 Runner 配对码，完成凭据保存、项目注册、服务安装和就绪检查。 |
| `configure --join URL --no-project --token-file PATH` | 从受保护文件读取用户 API 凭据，仅作为查看端加入；不创建本机 Runner 身份或服务。省略文件选项时使用隐藏终端输入。 |
| `resume` | 核实已保存进度，只补做缺失步骤，不静默改绑环境。 |
| `invite` | 在环境的本机 Server 创建 Runner 短期邀请；显示的 code 属于敏感信息。 |
| `add-project PATH` | 复用已有 Runner 身份；查看端需先完成 Runner 接入。 |
| `status --json` / `doctor --json` | 查看已保存配置、Server 连通性、Runner/项目就绪状态和结构化诊断。 |
| `start COMPONENT` / `stop COMPONENT` / `restart COMPONENT` | 显式控制环境所拥有的 `server`、`runner` 或 standalone `tunnel`；命名 standalone Tunnel 使用 `--profile NAME`。 |
| `configure-tunnel [PROFILE] --host embedded\|standalone [--credentials-file PATH]` | 创建时直接选择由 Server 托管或独立服务托管。凭据只允许隐藏输入或受保护文件。 |
| `tunnel-status [PROFILE]` | 查看精确 profile 身份、owner、就绪状态，以及是否需要显式重启 Server。 |
| `tunnel-host PROFILE --host embedded\|standalone` | 仅在旧 owner 已干净停止、standalone 服务已卸载后，显式转移已有 profile 的 owner。 |
| `repair-user-credential [--token-file PATH]` | 核实并替换已保存用户凭据，不配对 Runner 或改变服务状态。 |
| `repair-credential runner` | 通过隐藏输入修复 Windows SCM 账户凭据。 |

完整命名空间（含 Tunnel profile 和安装器升级/恢复）见 `webcodex environment --help`。早于 v0.4.6 的安装须先经过[兼容策略](compatibility-policy.md)规定的 v0.4.6 桥接；v0.5 不提供已移除的 pre-Environment Linux 迁移命令。公开环境命令支持 `--json` 和 `--environment-dir PATH`。不要将 token、配对码或服务密码放入命令行参数。配对兑换结果不确定时，先阅读恢复诊断，再显式通过 `resume --new-pairing-code --code-stdin` 提供替换码，不自动重放旧码。

### 项目 / 本地工作流

以下命令作用于当前 Git 项目。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex`（无子命令） | 临时分享的交互式快捷方式 | 仅 Linux/macOS + stdin/stdout 为终端 + 当前目录位于 Git checkout 时自动进入 `share`；否则照常显示 help。 |
| `webcodex share` | 临时把当前项目接入 ChatGPT/MCP | Linux/macOS/Windows 的快速试用/短期分享路径；包含临时 setup、本地 Server + Runner、`cloudflare|openai|none` 和有界前台 cleanup。日常完整使用见 `PERSONAL_SETUP`。 |
| `webcodex connect <server>` | 把当前项目接入已有的 Server | 已经拥有 Server URL 时的长期路径；默认使用 hosted shared-key。 |
| `webcodex status` | 简洁的项目 coding 就绪状态 | 简短状态；`doctor` 提供完整诊断。 |
| `webcodex doctor` | 当前项目的只读就绪检查 | 诊断/手动工作流；输出稳定的 `next action`。 |
| `webcodex setup` | 只配置当前 Git 项目，不启动 runtime | local-only/手动工作流；创建私有状态与 Project Credential。 |
| `webcodex run` | 启动 project-bound loopback Server 与本地 Runner | local-only/手动工作流；前台运行，Ctrl-C 同时停止两者。 |
| `webcodex disconnect [--project PATH] [--profile NAME]` | 移除一个 hosted 项目注册 | 是该仓库 `connect` 的精确逆操作；绝不删除仓库或 `.git`。 |

`webcodex share --auth query-token` 是给无法配置 Bearer header 的 MCP client 使用的显式临时 share 兼容模式。它只在 `/mcp?token=...` 接受当前 share 的精确 Project Credential，输出经过 URL 编码的敏感 MCP URL，并让 client 选择 No authentication。普通 Server/runtime 请求不会因此启用 query auth，PAT/OAuth/shared-key/Runner credential 也不能通过这个 query 路径回退认证；`--tunnel openai` 会拒绝该模式。整条 URL 都必须当作 credential，因为 query 可能被 client、proxy、剪贴板或 access log 留存。默认仍是 `--auth bearer`。

`webcodex share --auth oauth --oauth-redirect-uri <精确回调地址>` 使用 OAuth 2.0 Authorization Code + PKCE S256。OAuth client ID/secret 会按“项目 + 回调地址”保存在受保护的 project state 中；临时 OAuth grant 只在当前 `share` 运行期间有效。重启 `share` 会让旧 OAuth grant 失效，但不会改变项目。OAuth access token 永远不能用于 Runner transport。加 `--oauth-persist-on-restart` 后，同一个 share 的普通重启会保留已校验的 OAuth client 及其未过期、未撤销的 grant：session subject 由持久化的 client ID 派生，而不是每次重新随机生成，因此重启不必重新授权。过期、撤销、scope 与 token replay 规则仍然有效，重启不会重放任何副作用；首次带该标志运行时可能需要一次重新授权，因为上一次的 grant 属于不同的 session subject。不加该标志时行为完全不变。 `--project-roots <目录>`（可重复，最多 8 个）显式授权本 share 的 project-share OAuth 主体按需接入给定规范化父目录下已存在的目录，使新 clone 的仓库和新建的普通目录无需逐项目注册即可通过同一 MCP 入口开始工作。授权在 share 启动时校验并规范化，与 share OAuth client 状态一同持久化，并具备持久更新语义：重复传入相同列表是幂等空操作；列表变化时原子替换旧列表（更新失败绝不破坏旧值）；省略参数恢复最后保存的列表；`--no-project-roots` 删除持久化授权，被移除的根不会在之后的重启中复活。授权仅绑定本身份、本 Runner：授权根之外的路径——包括前缀相似目录、符号链接逃逸与不存在的目录——仍维持 `project_registry_scope_denied` 栅栏，Runner `allowed_roots` 策略仍是真实执行边界。


Cloudflare Quick Tunnel 的公网 origin 仍然是临时的。如需稳定 HTTPS origin，可使用 `--tunnel none --public-url https://share.example`，并由 operator 自己把该 origin 反向代理/隧道到 loopback WebCodex Server；`--public-url` 只声明外部 origin/issuer，不会创建代理或 tunnel。

`webcodex share --tunnel openai` 使用原生 Rust Secure MCP Tunnel client，需要 `CONTROL_PLANE_TUNNEL_ID` 和具有 Tunnels Read + Use 权限的 Restricted `CONTROL_PLANE_API_KEY`，支持 `--auth bearer`。临时本地 Bearer 直接在内存中传递，仅注入固定的本地 MCP 请求；ChatGPT 使用 Connection: Tunnel + No authentication。启动时验证本地 MCP 和成功的控制面 poll，不再下载外部 tunnel-client 或启动 doctor/health 子进程。严格 deadline、有界接收、禁止本地重放及不确定重启标记见 [crate 契约](../crates/webcodex-openai-tunnel/README.md)。

公网 `share` 会 best-effort 复制 MCP URL；默认 Bearer/OAuth 模式仍不会自动把临时 credential 复制进剪贴板。显式 `--auth query-token` 模式则按设计复制含临时 credential 的敏感 URL，并在状态输出中明确提示。Linux/macOS 交互式终端还会提供按 Enter 打开 ChatGPT App 设置的快捷入口。剪贴板/浏览器集成都只是 convenience，失败不会影响已经 ready 的 runtime。使用 `--no-copy-url` 可关闭剪贴板访问。

面向受监督的 machine integration，可使用 `webcodex share --json --stop-on-stdin-eof`。它仍保持原有前台生命周期，但会把 supervising parent 关闭 stdin 视为停止请求，使 Desktop 或其他 structured process owner 可以让 `share` 自己清理临时 Server、Runner 与 Tunnel，而不需要拼 shell signal 命令。该 flag 在非 `--json` 模式下会被拒绝。

`webcodex connect <server> --auth oauth --oauth-redirect-uri <精确回调地址>` 是普通 hosted OAuth 路径。Runner 保持原有 hosted credential，MCP client 使用 OAuth。只有真正需要额外能力时才增加 `--oauth-computer-permissions`、`--oauth-local-mcp` 或 `--oauth-local-ssh`；它们属于显式权限变更，可能要求重新授权。`--oauth-local-ssh` 会授予 MCP client 使用模型侧 `manage_ssh_resource` 接入工具所需的可选 `ssh:local` authority；它不会暴露 SSH credential，也不会绕过工具返回的 Runner restart requirement 让新资源立即生效。Client 设置见 [MCP](MCP.zh-CN.md#oauth2)，安全模型见[认证](AUTH_MODEL.zh-CN.md#oauth2)。

Browser Use 的 shared-key OAuth delegation 需要在 `connect --auth oauth` 时显式指定 `--oauth-browser-permissions`，仅追加 `browser:read`、`browser:control`、`browser:launch`。默认 baseline 不包含 Browser scope；Browser 与 `--oauth-computer-permissions` 相互独立，也不使用 Computer consent checkbox。已有 client 不会自动扩权，历史窄权限仅追加显式选择的类别。scope ceiling 变化会撤销旧 grants 并要求重新授权；复用已启用 Browser 的 profile 时必须继续携带该 flag。

Managed OAuth client 的创建、列举、修改和撤销要求 PAT 持有 `account:manage`（或 super-scope `admin`）；普通 account manager 仍只能管理自己的 client。OAuth access token 不能执行这些 first-party 管理操作。授权范围不得超过 PAT authority；`offline_access` 是 refresh capability，不是 runtime permission。需要 admin diagnostics 时，使用 admin PAT 授权自己拥有的 managed client。升级前的 connection 需要重新授权一次，此后 refresh 保留 grant authority。shared-key `connect --auth oauth` 不授予 admin。参见[认证模型](AUTH_MODEL.zh-CN.md#oauth2)。

高级 managed identity 流程仍保留为 `--auth managed-oauth --oauth-redirect-uri <精确回调地址>`，它才要求先 `webcodex login`；`--user` 也只用于该模式。

`disconnect` 按 canonical 仓库路径匹配，不根据 basename 或 project id 猜测。如果同一仓库
注册在多个 hosted profile 中，必须显式指定 `--profile`。managed Runner 在线时，它先执行
structured unregister，再删除本地 registration；Runner 已停止时，只删除精确
匹配的本地项目 registration。其他项目、profile credential 和 `runner.toml` 都会保留。

接入 MCP coding client 后，可阅读 [Coding 工作流](CODING_WORKFLOW.zh-CN.md)，了解 canonical
`work_on_project` model bootstrap、behavioral guidance、validation 与 closeout evidence。

### 设备接入

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex login <server-url> --code <wc_pair_...> [--project PATH]` | 用一次性登录码把本机接入 Server | 普通 managed 接入入口；`--project` 选择实际项目，`--allowed-root` 指定以后允许添加项目的父目录；`--print-mcp-config` 可显式打印敏感的 ChatGPT MCP 连接信息。 |
| `webcodex project register --config PATH <PROJECT>` | 给现有 Runner 再添加一个项目 | 写入该 Runner 的项目配置；不要求 Server 在线即可持久化，运行中的 Runner 是否需 reload 以命令输出为准。 |
| `webcodex pairing create` | Server/admin 侧：创建短期 pairing code | 需要 server bootstrap/admin 认证。 |
| `webcodex logout <server-url> [--user USER|--all]` | 移除本机对某 Server 的凭据 | 只有一个 saved user 时自动选择；多个 saved user 时必须用 `--user USER` 选择一个，或显式用 `--all` 选择全部；真正删除仍遵守现有 confirmation/`--yes` 流程。 |

本地 `login --project`、`project register` 与 Desktop picker 显式选择现有项目时，
会自动持久化 exact canonical root 授权，不扩大到父目录或整个共享目录。
需要新授权的 symlink 路径须显式选择 canonical 目标；危险 Windows namespace 与
`..` traversal 仍拒绝。模型注册入口仍受既有 Runner policy 约束，UNC 仍先检查网络授权。
已运行 Runner 若需要 reload，按命令输出处理。

在 Windows 上，后台项目列表从配置读取 `\\wsl.localhost` 和 `\\wsl$` 下的注册记录。
为避免唤醒发行版，列表会省略这些项目的 Git 状态和真实根目录指纹。显式注册项目或运行项目任务时，
Runner 可以启动目标发行版并检查实际路径。取消注册会保留文件访问授权，可在 Desktop 中单独撤销。
Server 重启后，WSL 即使处于停止状态，原 Project Ref 仍可列出调用者已绑定的 Session 历史。
如果先升级 Runner，它会继续向旧 Server 提供实际根目录指纹，已有 Session 可以原样续接。
双方都支持延迟探测后，后台列表就会跳过根目录检查。

root login 会给出可直接运行的 foreground 命令，并提示项目命令拥有 root 权限；
Linux system service 安装命令明确包含 `--allow-root-runner`，无需重新 login。

### Runner 生命周期

Runner 可执行文件是 `webcodex-runner`。其规范 CLI 生命周期命名空间是 `runner`：
`webcodex runner ...` 管理 `webcodex-runner` 进程与服务。`webcodex` 与
`webcodex-runner` 是两个独立可执行文件。

| 命令 | 用途 |
| --- | --- |
| `webcodex runner init` | 手动生成 `runner.toml` 配置 |
| `webcodex runner install` | 安装、启用并启动 Runner 服务 |
| `webcodex runner run` | 前台运行 `webcodex-runner`。省略 `--profile` 和 `--config` 时，由 `webcodex-runner` 自行解析配置。 |
| `webcodex runner start` | 启动 hosted 后台 Runner 或已安装的 profile 服务 |
| `webcodex runner stop` | 停止 |
| `webcodex runner restart` | 重启 |
| `webcodex runner status` | 检查 Runner 生命周期、配置与连通性 |
| `webcodex runner logs` | 读取 Runner 日志（有界） |
| `webcodex runner uninstall` | 移除服务单元（需要 `--confirm`） |

服务命令接受 `--scope user|system`。非 root 用户默认 user scope；root 默认
system scope。`webcodex connect` 创建的 profile 在不传 `--scope` 时保持其
detached-process 行为。

### Server

| 命令 | 用途 |
| --- | --- |
| `webcodex server init` | 初始化/更新 Server env 文件与所选 data directory（创建 bootstrap token） |
| `webcodex server install` | 安装 Linux systemd `webcodex.socket` + `webcodex.service` pair；默认 WorkingDirectory 跟随所选 env 的 `WEBCODEX_DATA` |
| `webcodex server run [--env-file PATH]` | 前台运行 `webcodex-server`（direct bind）；`--env-file` 通过 `WEBCODEX_ENV_FILE` 精确传递路径 |
| `webcodex server start` / `stop` | 一致地启动或停止 socket activation 与 Server process |
| `webcodex server restart` | 只 restart Server process，保持受管 listener socket active |
| `webcodex server status` | 检查 authoritative socket/service 状态、HTTP 可达性与构建版本 |
| `webcodex server logs` | 读取 Server service journal |
| `webcodex server uninstall` | stop/disable/remove 受管 socket/service pair |

## Controller（CLI 管理工具）

webcodex controller 是 WSL/Linux/Unix 场景下的 CLI 管理工具，提供命令行和只读 TUI 概览入口，沿用 Server、Runner 和 OpenAI Tunnel 的现有运行契约。Server 可配置为本地托管或远程观察；Runner 仍在本机由 Controller 托管；OpenAI Tunnel 只适用于本地 Server。

    webcodex controller init
    webcodex controller doctor
    webcodex controller install

默认配置位于 ~/.config/webcodex/controller.toml。运行中的 Controller 通过 $XDG_RUNTIME_DIR/webcodex/controller.sock（未设置 XDG_RUNTIME_DIR 时使用当前用户专属的 /tmp runtime 目录）提供本地 Unix Socket 控制接口。

常用操作：

    webcodex controller start
    webcodex controller status
    webcodex controller restart
    webcodex controller restart server
    webcodex controller restart runner
    webcodex controller restart tunnel
    webcodex controller logs --lines 100
    webcodex controller logs -f
    webcodex controller stop
    webcodex controller uninstall --confirm

只读终端概览：

    webcodex controller tui
    webcodex controller tui --config /path/to/controller.toml --service-file /path/to/controller.service --user-token-file /path/to/user-token

界面展示 Controller 来源与运行状态、systemd 服务状态、Server/Runner/Tunnel 的状态、PID 和重启次数，项目 ID、路径、库存同步状态和截断提示，下方持续显示实时日志。状态每 2 秒刷新，项目每 10 秒刷新；不可访问时显示具体错误，并将保留的旧数据标记为过期。项目查询沿用已有凭据选择及在线 Runner 检查。TUI 要求交互式 stdin/stdout 和至少 80×24 的终端，仅提供观察功能，进入或退出均不改变 Controller、服务或项目状态。

Tab 切换项目/日志焦点，方向键及 PageUp/PageDown 滚动，Space 暂停/恢复日志自动滚动，`r` 刷新状态和项目，`q` 或 Ctrl+C 退出。日志首次读取 100 行，缓存最多 500 行和 1 MiB；缓存淘汰、连接断开及输出超限均明确提示。断开后每 2 秒重连；运行中的旧 Controller 不支持日志跟随时提示更新并停止重连。退出会清理 TUI 的查询与日志观察进程并恢复终端。

项目管理：

    webcodex controller project list
    webcodex controller project register /path/to/project
    webcodex controller project remove <project-id-or-path>

Controller 的 [server] 支持 mode = "local"、mode = "remote" 与 mode = "wsl2_nat"。local 模式要求 env_file，Controller 会启动并监督 webcodex-server；remote 模式要求 url，Controller 只探测远程 Server，不启动本地 Server，也禁止本地 regular Tunnel。local/remote 模式下 Runner 都使用本机 runner.toml，且其中的 server_url 必须与 Controller 配置的 Server 一致。Controller 不接管已经由 webcodex.service / webcodex.socket / webcodex-runner.service 管理的同类本地实例。

三种模式均由 Controller 守护进程使用用户 API 凭据持续查询指定 Runner 的在线状态：启动后立即查询，之后每 10 秒查询一次，单次超时 3 秒，响应上限 1 MiB。首次观测前为 `starting`；Server 确认在线后为 `ready`；明确非在线且进程存活时为 `running_offline`；凭据不可用、无权访问、不可见或查询失败时为 `running_unverified`。失败会立即撤销旧在线结论，超过 30 秒的观测也不再有效。观测不阻塞 IPC，不触发重启或 WSL 网关发现。停止或更换 Runner 进程会清空旧观测。

TUI、`status` 和在线 `doctor` 共用守护进程状态；JSON 的 Runner 快照提供可选 `observed_at` 和 `observation_error`，文本和 TUI 同时展示观测时间及错误分类。`doctor` 将配置检查与运行状态分开；守护进程不可用时，Runner 在线状态为未知。项目命令仍自行检查在线状态，不使用展示缓存作为执行依据。

`wsl2_nat` 专用于 WSL2 NAT 内的 Runner 自动连接 Windows 宿主机上的 Desktop Server，不考虑其他虚拟机或网络拓扑兼容。Controller 配置如下：

```toml
[server]
mode = "wsl2_nat"
interface = "eth0" # 可省略，默认 eth0
scheme = "http"   # 可省略，默认 http
port = 8080       # 必填

[runner]
enabled = true
config = "/home/user/.config/webcodex/clients/wsl2-runner/runner.toml"
# 可选：显式指定用户 API 凭据；省略时自动查找
# user_token_file = "/home/user/.config/webcodex/webcodex-user-token"

[tunnel]
enabled = false
```

该模式固定通过 Rust Netlink 读取指定接口 IPv4 主路由表的默认网关。省略 interface/scheme 时分别使用 `eth0`/`http`，显式空值或非法值报错；禁止配置 `server.url` 和 `server.env_file`。Windows Desktop Server 必须允许 WSL NAT 接口连接，防火墙应仅放行受信 WSL 来源。Runner 须预先在该 Server 完成配对；发现地址不代表 Server 身份可信，也不会迁移凭据。HTTP 仅用于受信宿主机/WSL 私有链路，HTTPS 保留证书验证。使用默认 WebSocket、polling 或不含 QUIC 配置段的 auto；该模式拒绝固定 QUIC 端点。

地址发现和探测只在新一轮 Runner 启动尝试前执行。缺少路由或 Desktop Server 不可达时，Controller 保持 IPC 可用，状态为 `waiting_for_desktop_server` / `waiting_for_server`，以可取消、最多 30 秒的退避等待重试，不消耗子进程崩溃重启次数。实际重新启动 Runner 前会重新解析；存活的 Runner 不触发路由轮询、自动改绑或网络驱动重启，NAT 地址改变后可显式执行 `controller restart runner`。认证/注册失败会停止自动恢复并报告 `runner_auth_failed`。该模式不能重启 Windows Server 或管理其 Tunnel。

Controller 通过 `webcodex-runner --config PATH --server-url URL --stop-on-stdin-eof` 启动 Runner。独立 Runner 的 `--server-url` 与 `runner.toml` 的 `server_url` 含义和校验相同，命令行值在本次进程及热重载时始终优先；显式无效值直接报错，不回退配置值。配置文件、身份、凭据、项目注册目录及策略保持不变。Controller 使用子进程局部代理排除项，确保网关请求不经代理，不修改全局环境变量。

`status --json` 的 `wsl2_nat_target` 报告当前目标、代次、配置绑定、采样时间及解析/探测错误。`doctor --json` 在线时使用活动快照，离线发现明确标记为 `candidate`。网络可达（包括 HTTP 401/403）不代表 Runner 已认证；Runner 在线状态由上述授权查询持续维护。

`controller install` 默认安装 user service 到 `~/.config/systemd/user/webcodex-controller.service`，并通过 `systemctl --user` 管理 Controller 生命周期。`status` 会优先读取正在运行的 Controller Unix Socket，并同时显示 systemd 状态；Socket 不可用时仍可显示已安装 service 状态。`logs` 优先读取 Controller 内存中的组件日志，Controller 不可达时回退到 user journal。添加 `-f` 或 `--follow` 可先输出最近日志，再持续读取新增日志，按 Ctrl+C 退出；`--lines N` 控制首次读取的行数。内存日志跟随连接断开或读取速度落后时会明确报错，可重新运行命令读取最新日志。`stop` 会自动识别前台 Controller 与 systemd service；无组件参数的 `restart` 优先重启已安装 service，否则重启前台 runtime。`restart server|runner|tunnel` 始终通过 Controller IPC 操作组件。`uninstall --confirm` 只删除 Controller unit，不删除 controller.toml 或 controller.env。

所有 `controller project` 命令均要求 `runner.enabled=true`，且指定 Runner 在 Server 上在线并对当前凭据可见。local/remote 模式下 Controller 守护进程本身不必运行；`wsl2_nat` 模式要求守护进程在线，并具有与当前配置匹配的活动目标。项目操作使用固定的有效端点，每次请求前检查代次；Controller 不可用、目标未解析或代次改变时明确报错。Runner 离线、目标不可访问或版本不支持时明确报错，不回退本地 registry，也不自动启动 Runner。

三个命令及 TUI 的用户凭据优先级为：命令行 `--user-token-file PATH` → `controller.toml` 的 `runner.user_token_file` → 默认连接扫描。扫描按原始 Server URL 和 Runner ID 匹配，优先选择包含当前 Runner 配置文件的连接；仅在零匹配时，兜底读取所使用的 `runner.toml` 路径同级的 `webcodex-user-token`，不递归扫描，也不要求 `server.toml`。连接歧义、选中连接的凭据不可用、显式文件无效均报错，不继续回退。Runner transport 和 Tunnel 凭据不能替代用户凭据。守护进程使用相同规则，但没有命令行凭据覆盖；只给 TUI 传入凭据不会配置守护进程。每轮观测重新读取凭据文件，支持原路径轮换；修改 Controller 配置需要重启 Controller。

`wsl2_nat` 模式的默认凭据仍绑定原始 Runner 配置及已配对的 Server 连接，不会依据发现的网关地址猜测新的凭据目录。

Runner 超过 100 个项目时，可使用 Server 返回的完整项目 ID（例如 `agent:runner-a:demo`）删除，命令会在 Server 侧精确筛选库存。短 ID 和路径匹配要求库存未截断，以便可靠拒绝歧义目标。

`project list` 调用 `list_projects`，仅列出指定 Runner 的可见项目，并显示库存同步及截断状态（最多 100 项）。`project register PATH` 复用在线按路径解析或注册 API，只使用 Runner 当前已有路径权限，不扩展 `[policy].allowed_roots`。`project remove ID-OR-PATH` 从完整库存中解析唯一项目，携带 revision 调用 `unregister_project`；只注销项目，不删除工作目录、不收缩 allowed_roots、不停止 Runner。操作在线生效，无需重启 Runner。revision 冲突直接报错；变更响应丢失时报告结果未知，不自动重试或补删本地文件。使用 `--json` 时，成功 API 结果输出到 stdout，命令失败以 JSON 输出到 stderr 并返回非零退出码。

Windows 支持 `server init`、前台 `server run` 与显式 `share`。受管 service 生命周期（`install`、`start`、`stop`、`restart`、`logs`、`uninstall`）仍只支持 Linux。

使用 `webcodex server install --service-file /path/name.service` 时，会派生同目录的
`/path/name.socket`。后续 `start`、`stop`、`restart`、`status`、`logs` 和 `uninstall`
应传入相同的 `--service-file` 来管理或检查该自定义 pair；省略时仍操作默认的
`webcodex.service` / `webcodex.socket` pair。

Runner 配置术语中，`project_registry_dir` 是 Project registry TOML 文件目录，不是 workspace root；`[policy].allowed_roots` 只限制哪些文件系统路径可以注册，Project record 才指向实际 workspace。

### 运维（只读操作检查）

| 命令 | 用途 |
| --- | --- |
| `webcodex ops status` | 汇总 runtime、工具、任务、Runner 与项目 |
| `webcodex ops runners` | 简洁的 Runner fleet 状态 |
| `webcodex ops runner --client-id <id>` | 精确读取单个 Runner 的注册与构建状态 |
| `webcodex ops projects` | 项目清单与 smoke 适用性 |
| `webcodex ops smoke-preflight --project <id>` | 为某项目做 deploy smoke 预检 |

`ops` 命令只读。支持 `--server-url`、`--token-file`、`--env-file`、`--token`、
`--json` 与 `--strict`。优先使用 `--token-file`；`--token` 容易泄露到 shell
历史或进程列表。`--strict` 会让 FAIL 报告以状态码 2 退出。

### 审查与 runtime activity

旧 `webcodex task` namespace 已随独立 Connector Task/Result/Approval lifecycle 删除。`webcodex run` 会输出 Runtime Console 地址（`/runtime`）。现在的 review 使用 canonical Workflow Session、Job、Git/diff、`read_workspace_changes` 与 `finish_coding_task`，不再有 host-side result accept/reject queue。

### 凭据与账号

Admin 用户/令牌操作由 Server API 支撑；`auth status` 读取本机连接状态，而
`create-local` 命令在本地生成凭据，只向 Server 注册其 hash。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex auth status` | 显示本机已登录哪些 Server | 只读；支持 `--dir` 与 `--json`。 |
| `webcodex users create` | 创建用户；`--issue-credential` 返回一次性 account credential | Server/admin 侧；使用 `--server-url`。 |
| `webcodex users list` | 列出用户 | |
| `webcodex tokens create-local` | 本地生成 `wc_pat_*` 个人 API 令牌并注册其 hash | 使用 `--server-url`、`--username` 与 account credential。 |
| `webcodex tokens create` | Admin：在服务端创建 PAT | 使用 `--server-url`。 |
| `webcodex tokens generate` | 离线生成令牌素材 | **不会**在 Server 注册。 |
| `webcodex tokens list` / `revoke` / `register-hash` | 列出或撤销 PAT；注册外部计算的 hash | Admin 侧；使用 `--server-url`。 |
| `webcodex runner-tokens create-local` | 本地生成 `wc_agent_*` Runner 令牌并注册其 hash | 使用 `--server-url` 并绑定 `--client-id`。 |
| `webcodex runner-tokens create` / `list` / `revoke` / `register-hash` | Admin 变体 | |

所有面向 Server 的 credential 命令统一使用 canonical `--server-url`。
本地 `tokens create-local` / `runner-tokens create-local` 使用 `--username` 与 account
credential；admin token management 也使用相同的 plural namespace。

### 高级与兼容命令

以下命令覆盖不常见场景；上面的推荐路径才是常规入口。

| 命令 | 用途 | 说明 |
| --- | --- | --- |
| `webcodex pairing create` | Server/admin 侧：创建短期 pairing code | 需要 server bootstrap/admin 认证。 |
| `webcodex tokens generate` | 离线生成令牌素材 | 不注册任何东西；若需要服务端注册 hash，把输出配 `tokens register-hash` 使用。 |
| `webcodex tokens register-hash` | Admin：注册外部计算的 PAT hash | 使用 `--server-url`；用于离线生成的素材。 |
| `webcodex runner-tokens register-hash` | Admin：注册外部计算的 Runner 令牌 hash | 使用 `--server-url`；用于离线生成的素材。 |

## 术语

- **Server** —— 认证调用方、保存共享 runtime 状态并路由工作。
- **Runner** —— 在持有代码的机器上执行仓库工作。
- **Project** —— 由 Runner 注册的一个仓库/工作区。
- **Job** —— 发起调用返回后仍继续运行的命令或 validation。
- **Workflow Session** —— runtime 用于 coding evidence/continuity 的有界状态。普通用户通常不需要管理其内部协议字段。

部分兼容名称仍保留 `agent`，主要是 `wc_agent_*` 与 `agent:<client_id>:<project_id>`。它们属于 Runner 时代的兼容名称，不是独立 Durable Agent domain；其它 process/protocol identifier 继续留在内部。新文档除引用这些公开名称外应统一写 **Runner**。

## 凭据：我到底需要哪个令牌？

WebCodex 把 bootstrap 管理、账号接入、runtime API 访问与 Runner 连接分开。
不要跨 surface 复用同一凭据。完整模型见
[AUTH_MODEL.md](AUTH_MODEL.zh-CN.md)；下表是快速答案。

| 凭据 | 前缀 | 由谁创建 | 用途 | 不要用于 |
| --- | --- | --- | --- | --- |
| Server bootstrap token | （env `WEBCODEX_TOKEN`） | `webcodex server init` | server/admin 设置、建用户、pairing | MCP、Runner、日常使用 |
| 共享 key | `wck_...` | `webcodex connect`（一次性生成） | hosted shared-key 的 MCP + Runner | 生产 IAM |
| Project Credential | （私有文件） | `webcodex setup` | 一个 ProjectGrant 的普通 runtime API/MCP 访问 | 其它 ProjectGrant、admin、Runner transport |
| Account credential | `wc_acct_...` | `webcodex users create --issue-credential` | 本地创建令牌 | MCP、Runner |
| 个人 API 令牌（PAT） | `wc_pat_...` | `webcodex tokens create-local` | MCP、REST API | Runner 连接 |
| Runner 令牌 | `wc_agent_...` | `webcodex runner-tokens create-local` | 仅 `webcodex-runner` 传输 | MCP、REST |
| OAuth 访问令牌 | `wc_oat_...` | OAuth2 授权流程 | 启用 OAuth 时的 MCP | — |

### 实际使用规则

- 普通 managed setup：`webcodex login` 会创建本地 user/API 与 Runner 凭据；直接使用它报告的路径和 MCP 连接值。
- 已有 shared-key Server：使用 operator 提供的 `wck_...` 配合 `webcodex connect`。
- Project-first/manual setup：Project Credential 只留在受保护的项目私有状态里，不要当作通用 user/admin token。
- `WEBCODEX_TOKEN` 只留在 Server；它不是 MCP 或 Runner 凭据。
- `wc_agent_*` 只用于 Runner transport；`wc_pat_*` 才是普通 managed user API token。
- 优先使用 `--token-file`，不要把完整配置文件粘贴进聊天。
- OAuth client 应走 OAuth flow，而不是人工复制 access token。见[认证](AUTH_MODEL.zh-CN.md#oauth2)与 [MCP](MCP.zh-CN.md#oauth2)。

## 常用示例

日常完整使用：先按[完整使用指南](PERSONAL_SETUP.zh-CN.md)启动普通 Server，再在项目机器上完成一次性登录并启动 Runner：

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo" \
  --print-mcp-config
webcodex runner run --config <login-reported-runner-config>
```

只想临时试用一个仓库：

```bash
cd /path/to/your/repository
webcodex share
```

Local/manual project-bound 工作流（高级/诊断）：

```bash
webcodex setup
webcodex doctor
webcodex run          # 保持该终端打开；输出会指向 /runtime
webcodex status       # 在另一个终端
```

已有 hosted Server：

```bash
webcodex connect https://your-server.example
webcodex runner status --profile <profile>
webcodex runner logs --profile <profile> --lines 100
```

Linux 上把已经验证过的 Runner 改为 user service：

```bash
webcodex runner install --scope user --config <login-reported-runner-config>
webcodex runner status --scope user --config <login-reported-runner-config>
webcodex ops status --server-url https://your-server.example \
  --token-file <login-reported-webcodex-user-token> --strict
```

## 代理与网络

CLI 请求默认遵循标准代理环境变量（`HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`、
`NO_PROXY`）。用 `--proxy http://HOST:PORT` 为单次调用覆盖，或用
`--no-system-proxy` 忽略代理环境直连。这些 flag 只影响 CLI 自身的 HTTP 请求；
`webcodex connect` 不会把它们持久化或注入 Runner 配置。

### Tunnel 离线诊断

`webcodex environment tunnel-diagnose primary --environment-dir /absolute/environment --json`
只读取指定 Profile 的 catalog、私有 binding 和 readiness；不会伪造 Environment、启动服务或清除保护标记。
结果包含已观察状态、安全失败码及两项 readiness，并区分缺失/无效、过期、配置 revision 或 owner PID 不匹配。
只有 catalog 的配置返回 `owner_unverified`，历史 heartbeat 不证明当前 ready。
Linux/macOS 托管服务需要匹配服务管理器的当前 PID；暂不能验证精确 PID 的其他管理器保守返回未就绪。
`tunnel_restart_uncertain` 需要先核对历史执行效果；诊断不会重放写操作。
readiness v1 增加可选的 `state`、`diagnostic` 字段，旧文件仍可读取，缺失状态保持未知。

### 单 Profile 受控恢复

`webcodex environment recover-tunnel primary --environment-dir /absolute/environment --json`
默认只诊断，返回 run ID、Profile revision、Environment ID 和 owner/standalone 服务观察。
核对历史执行结果并通过原有服务生命周期停止 owner 后，才可使用诊断返回的精确标识：

```
webcodex environment recover-tunnel primary --environment-dir /absolute/environment \
  --apply --accept-uncertain-effects --expected-environment-id ENVIRONMENT_ID \
  --expected-revision REVISION --expected-run-id RUN_ID
```

执行持有 Environment 配置锁和 Tunnel 身份锁；活跃/复用 PID、其他 owner、旧 revision 或 run ID 均拒绝。
只原子归档精确标记并保留原内容，随后使用现有 Service Manager 启动已停止的 owner，验证当前 PID、revision、
Tunnel 和本地 MCP readiness。重复旧请求不会再次归档新 run 或重放写操作。Server 按原有生命周期载入其配置，
其他 Profile 的 fence 和配置不修改；没有第二套 supervisor 或临时 launchctl 任务。

当前自动恢复限于完整 Environment 中的 Unix 用户托管 OpenAI Tunnel；catalog-only 配置仅诊断。
新 run 使用含 PID 和 run ID 的 v2 标记；旧二进制仍受同名 `.active` 文件阻挡。
旧 v1 标记没有 owner 证据，保留原样并返回 `tunnel_legacy_owner_unverifiable`。即使确认风险，也不能证明旧执行者
已经结束，因此仍需单独人工核对，命令不会自动迁移或删除。没有新增凭据或模型执行权限。
