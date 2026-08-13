# Runner

Runner 是真正执行工作的组件。可执行文件是 `webcodex-runner`；管理它的 CLI 命名
空间是 `webcodex agent ...`。"Agent" 与 "Runner" 指同一个执行组件，但它们不是
同一个程序：`webcodex`（包含 `agent` 命名空间）与 `webcodex-runner` 是两个独立
可执行文件——"agent" 是历史遗留的 CLI 名称。本页说明 Runner 做什么、如何连接、
如何注册项目、如何以服务方式运维，以及它的主要运行时概念。

安装与服务设置见[部署指南](DEPLOYMENT.zh-CN.md)；管理 Runner 的命令见
[CLI](CLI.zh-CN.md#runneragent-命名空间)。

## Runner 做什么

Runner 运行在持有仓库的机器上。它主动连接 WebCodex Server，注册它允许服务的
项目，并在这些项目边界内执行有界操作——文件读写、Git 检查、结构化校验、shell
命令与长任务 Job。

Runner 是最接近你仓库的信任边界。请用窄的 allowed roots 与显式 shell profile
配置它，而不是继承宽泛的交互式 shell 状态。

## Server、CLI、Runner、Agent

| 术语 | 含义 |
| --- | --- |
| **Server** | `webcodex-server` 进程：认证、路由、持久化。 |
| **CLI** | 运维与开发者使用的 `webcodex` 命令。 |
| **Runner** | 在仓库机器上执行工作的 `webcodex-runner` 进程。 |
| **Agent / agent CLI 命名空间** | 管理 Runner 的 CLI 命名空间 `webcodex agent ...`。与 Runner 是同一执行组件，但是独立的可执行文件。 |
| **profile** | 一个命名的本地客户端配置（`agent.toml`、令牌、路径）。 |
| **client_id** | 一个 Runner/设备的稳定逻辑标识。 |
| **agent_instance_id** | `webcodex-runner` 启动时生成的进程级身份；Server 把它当作活跃租约身份（见「重连与恢复」）。 |
| **project_id** | Runner 在其项目注册表中注册的项目 id。 |
| **runtime project id** | `agent:<client_id>:<project_id>` —— Server 定位已注册项目的方式。 |
| **Connector** | 已配置本地项目的 project-bound coding surface；把一个项目绑定到其执行器。 |

## 连接 Server

Runner 主动向外连接 Server，使用四种传输之一，由 `agent.toml` 中的 `transport`
设置选择：

| 传输 | 配置值 | 用途 |
| --- | --- | --- |
| Auto | `auto` | 生产环境推荐（配置 `[quic]` 时）：先 QUIC，再 WebSocket，最后 polling。 |
| QUIC | `quic` | 仅 QUIC。Server 端独立 UDP listener。 |
| WebSocket | `websocket` | 无 UDP 场景的稳定 fallback。 |
| Polling | `polling` | 受限网络的最后手段。 |

Runner 使用 agent token（或在 hosted quick-start 模式中使用直接共享 key）认证。
该令牌只是 Runner 传输凭据，不用于 MCP、REST 或 GPT Actions。

WebSocket 使用 `Authorization: Bearer <token>`（`?token=` 仅兼容
`/api/agents/ws` 握手）。Polling 每次请求都带 bearer 头。QUIC 在注册信封内发送
token。

## 注册项目

项目存放在 Runner 机器上。Runner 把允许的目录注册给 Server；Server 不扫描文件
系统，也不自行发现项目路径。

每个注册项目是 Runner `projects_dir`（默认 `projects.d`）下的一个一文件一项目
TOML：

```toml
id = "webcodex"
path = "/srv/webcodex/projects/webcodex"
name = "WebCodex"
kind = "repo"
allow_patch = true
```

顶层 `id` 与 `path` 是必需的。旧的 `[projects.<id>]` 服务端嵌套格式不能用于
`projects.d`。

Runtime project id 形如 `agent:<client_id>:<project_id>`，例如
`agent:workstation:my-repo`。project-bound Connector 会在内部解析它；普通用户
不需要输入。

### 允许根目录

Runner policy 中的 `allowed_roots` 控制项目可以在哪里注册或创建：

- `allowed_roots` 缺失或为空时默认 `$HOME`。
- 显式 `allowed_roots` 覆盖该默认值。
- 用显式 roots 把 Runner 收窄到某个工作区，例如：

```toml
[policy]
allow_cwd_anywhere = false
allowed_roots = ["/root/git"]
```

### 临时项目

在 `agent.toml` 中设置 `temporary_projects_root`，可让 `start_coding_task`
通过 `client_id` 而非已有 `project` 来创建项目。Runner 只在该根目录下创建新的
直接子目录，并写入一条 `kind = "managed_temporary"` 的 `projects.d` 记录。
该根目录必须已存在且被 policy 允许。没有自动过期机制。

### 运行时注册项目

runtime 工具 `register_project` 与 `create_project` 让客户端在在线 Runner 上
注册已有目录或创建新目录，受 Runner 的 `allowed_roots` policy 约束。

## Shell profile

默认情况下 `run_shell` 与 `run_job` 不保留持久 shell 会话。它们会为每个
项目/profile 对准备一次环境快照，然后让每条命令以应用了该快照的独立进程运行。
快照的生成方式是：以清空的环境启动 profile 程序，应用 profile 的 `env`，执行
profile 的 `init_script`（如果有），再捕获最终环境。

WebCodex 默认不 source `~/.bashrc` 或 `~/.profile`：它们可能很慢、面向交互、
污染环境且不可复现。请改用显式 profile。

`agent.toml` 中的 Rust/Cargo 示例：

```toml
[shell]
default_profile = "rust"

[shell.profiles.rust]
program = "sh"
args = ["-c"]

[shell.profiles.rust.env]
PATH = "/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
CARGO_HOME = "/root/.cargo"
RUSTUP_HOME = "/root/.rustup"
```

Python venv 示例：

```toml
[shell.profiles.py-venv]
program = "bash"
args = ["-lc"]
init_script = '''
source .venv/bin/activate
'''
```

`init_script` 是项目相对路径：从项目根目录解析，因此每个项目激活自己的 venv。
项目可以固定 profile：

```toml
id = "paper-exp"
path = "/root/git/paper-exp"
shell_profile = "conda-ml"
```

解析顺序：`project.shell_profile`，再 `shell.default_profile`，最后是普通 shell
配置（不生成快照）。

Profile 的安全要点：

- 不要把令牌放进 `init_script`，也不要在其中 `echo` 密钥——脚本 stdout 会被
  当作快照的一部分解析。
- status 与 runtime API 只暴露脱敏的 profile 元数据（名称、`has_init_script`、
  env 键数量、program、dialect）——绝不暴露 `init_script` 正文或环境值。
- Profile 以清空环境 + 显式白名单运行；请声明所需 env。

## Job 与并发

Job 是在发起调用返回后仍继续运行的长命令或校验。Job 有稳定的 `job_id`、有界的
stdout/stderr 尾部，并且可以停止。结构化执行（`run_process`、`run_script`）与
校验 Job 会在单个执行超过同步宽限期时把同一执行交给 Job 延续；同一个进程继续
运行——绝不会被重启。

Runner 同时最多执行 `max_concurrent_jobs` 个 Job（默认 4，有效范围 1..64）。
这是运维调优参数，不是安全边界，修改需要重启 Runner：

```toml
max_concurrent_jobs = 4
```

当所有槽位都被占用时，已接受的 Job 仍是同一个可查询 Job（同一 `job_id`），并
报告 `agent_queued`。

## 传输细节

### Server 的 QUIC 要求

在 Server 上启用 QUIC listener 并打开所选 UDP 端口：

```sh
WEBCODEX_QUIC_ENABLED=true
WEBCODEX_QUIC_LISTEN=0.0.0.0:8443
WEBCODEX_QUIC_CERT=/etc/letsencrypt/live/<host>/fullchain.pem
WEBCODEX_QUIC_KEY=/etc/letsencrypt/live/<host>/privkey.pem
WEBCODEX_QUIC_ALPN=webcodex-runner/1
```

证书 SAN 必须匹配 Runner 上配置的 `server_name`。配置了 `[quic]` 时 `auto` 先试
QUIC，再 WebSocket，最后 polling。

### Runner 的出站代理

如果 Runner 主机需要出站 HTTP 代理，请把代理变量放进 Runner 的服务环境，而不仅
是交互 shell。WebSocket 遵循 `HTTPS_PROXY`/`https_proxy`、再
`HTTP_PROXY`/`http_proxy`、再 `ALL_PROXY`/`all_proxy`；`NO_PROXY`/`no_proxy`
可绕过匹配主机。当前支持的代理传输是 `http://host:port` 的 HTTP `CONNECT`。
QUIC 不使用代理设置。

## 重连与恢复

Runner 断连是 liveness 事实，不等于工作丢失。已接受的活跃 Job 会进入有界的
`recovering` 状态（默认宽限 120 秒），并在同一 Runner 实例重连后从其 inventory
恢复。替换的 Runner 实例不会继承旧实例的 Job；它们会变成 `lost`。Runner 进程重启
无法恢复其旧的子进程。

每个 Runner 进程携带自己的 `agent_instance_id`（启动时生成），把它与设备跨重启
保留的稳定 `client_id` 区分开。Server 把 `agent_instance_id` 当作活跃租约身份：
同 `client_id` 但不同 `agent_instance_id` 的第二个进程在第一个在线时会被拒绝，
过期/被替换的实例不能再 poll 或提交结果。

重连以短延迟自动进行。认证失败等致命错误会停止 Runner，而不是无限重试。

## 关停与重启

`webcodex-runner` 在 `SIGINT`/`SIGTERM` 时干净关停。它不会自行 daemonize。
托管部署请用 `webcodex agent install --scope user|system` 把它安装并托管为
user 或 system 服务，并把令牌放进服务环境。

机器重启后，hosted `connect` profile 通过重新运行 `webcodex connect` 或
`webcodex agent start --profile <profile>` 来恢复。hosted profile 暂不支持开机自动
启动。

## SSH 会话资源（高级）

能够调用本地 OpenSSH 客户端的 Runner 会声明 `ssh_shell` capability。Workflow
Session 可以选择命名的 SSH 资源，使 `run_shell` 与 `run_job` 通过 Runner 自己的
OpenSSH 客户端在远程主机上执行：

```toml
[ssh.resources.tmp]
host = "tmp"
default_cwd = "/opt/webcodex-edge"
```

`host` 值会传给 Runner 机器的 OpenSSH 客户端，因此 `~/.ssh/config`、密钥、
`ssh-agent`、`ProxyJump` 等配置都留在该机器上。不要把凭据、私钥或完整 SSH 配置
放进 Session 数据、Server 存储或工具输入。Session 的 `execution_context.resource`
只改变 `run_shell` 与 `run_job`；文件、Git、LSP 工具仍在本地。

## LSP 导航（只读）

Runner 可以通过在仓库机器上运行的语言服务器提供只读语义导航：

| 语言 | 服务器 | 标记 |
| --- | --- | --- |
| Rust | `rust-analyzer` | `Cargo.toml` |
| Python | `pyright` | `pyproject.toml`、`setup.py`、`requirements.txt`、… |
| TypeScript / JavaScript | `typescript-language-server` | `tsconfig.json`、`package.json`、… |

工具包括 `lsp_status`、`document_symbols`、`goto_definition`、
`find_references`、`document_diagnostics`、`hover` 与 `workspace_symbols`。
独立的 `call_hierarchy` 操作在 Runner 内完成 prepare 以及有界的
incoming/outgoing 广度优先遍历；canonical Connector 将其投影为 `code_impact`，
不会暴露原始协议方法或不透明 LSP item data。
它们只读、project-bound，并且被约束为启动语言服务器绝不执行仓库代码或拉取依赖。
路径是项目相对路径；外部/依赖位置会被省略。语言服务器必须安装在 Runner 机器上，
或通过 `WEBCODEX_RUST_ANALYZER` 等 env override 指定。

Call hierarchy 必须由独立的 `lsp_call_hierarchy` capability 声明，且所选语言服务器
必须提供 `callHierarchyProvider`。缺少支持会显式失败，不回退到 grep、AST、shell
或 references。

## 运维 Runner

最简命令：

```bash
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
webcodex agent restart --profile <profile>
```

用户服务：

```bash
webcodex agent install --scope user --config <login-reported-agent-config>
webcodex agent status --scope user --config <login-reported-agent-config>
```

管理员管理的系统服务：

```bash
sudo webcodex agent install --scope system --profile <profile> \
  --user <runner-user> --working-directory /home/<runner-user>
sudo webcodex agent status --scope system --profile <profile>
```

install、status、start、stop、restart、logs、uninstall 请使用相同的 `--scope`。
User scope 使用 `systemctl --user`；system scope 使用 `/etc/systemd/system`。

编辑 `agent.toml` 后，reload 对应服务以应用 policy、shell 与 SSH 资源变更。
身份、server/auth、传输与并发变更仍需要重启。无效 reload 会保留当前生效的
generation。
