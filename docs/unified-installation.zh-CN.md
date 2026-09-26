# 统一安装指南

[English](unified-installation.md)

本文描述本分支正在开发的统一安装流程，不代表已有新版本发布。Windows NSIS、macOS 安装包和 Debian 12 / Ubuntu 22.04+ `.deb` 计划在 x64 与 arm64 上统一包含 Desktop、CLI、Server 和 Runner。Linux 统一安装包会在原生架构的 Ubuntu 22.04 容器中构建和探测，将 GLIBC 符号版本限制在 2.35 以内，并检查运行时依赖。三种平台的真实机器安装、重启持久性、GUI 会话行为和升级尚未全部验收。具体检查项见[部署验收清单](unified-deployment-validation.md)。

已发布文件请从 [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases)获取。仓库 [`download/`](../download/README.md) 目录仅包含静态页面源文件；生成的 `manifest.json` 不提交到仓库。[下载页 workflow](https://github.com/yyjeqhc/webcodex/actions/workflows/download-page.yml) 会在 Release 发布后构建基于 manifest 的 GitHub Actions artifact，但不会托管或部署网页。安装包发布前如需预览特定源码修订，请看 [Linux 源码预览](DESKTOP_DEVELOPMENT.zh-CN.md#linux-源码预览与已有-server)。

## 一台电脑

以下流程适用于对应平台的安装包已经验收并发布之后；源码预览请使用上文单独列出的开发流程。

1. 安装对应平台的安装包并打开 WebCodex Desktop。
2. 选择**创建环境**并选择项目目录，也可以暂时不选项目。
3. 在 Desktop 确认 Server、Runner 和项目状态。Desktop、CLI 与网页使用同一 Server 的授权视图。浏览器打开 `SERVER_URL/runtime`，使用现有用户凭据查看获授权的 Runner、项目和状态。
4. 通过 Server 配置现有 ChatGPT MCP/Tunnel 连接。ChatGPT 始终连接中心 Server。远程项目路径属于 Runner 所在机器，不是 Server 机器上的本地路径。

关闭 Desktop 不会停止持久服务。GUI helper 只会在同一用户已登录且未锁定的会话运行。Linux 不新增 GUI helper backend。

## 多台电脑

在每台电脑安装相同平台安装包。先确定哪台机器承载中心 Server，哪些机器持有代码仓库。只运行 Server 的中心机器也可以显示远程 B、C 机器上的项目，只要这些机器的 Runner 已连接。

在中心 Server 上使用 `webcodex environment invite` 创建 Runner 短期邀请 code。显示出的 code 属于密钥，只交给目标机器，不要放入命令行参数或日志。

在中心 Server 机器创建环境：

```text
webcodex environment configure --create --project PATH
```

如果该机器没有仓库，使用 `--no-project`。在每台仓库机器加入 Server：

```text
webcodex environment configure --join https://server.example --project PATH
```

使用 `--no-project` 可仅作为查看端加入，不配置本机 Server 或 Runner。两项业务选择是创建/加入和项目/跳过；随后按所选流程收集 Server 地址、认证信息和必要的系统授权。以 viewer 身份加入时，使用用户个人访问 token，通过隐藏输入或受保护的 `--token-file` 提供；不使用 pairing code。例如，从受保护文件导入已有用户 API 凭据：

```text
webcodex environment configure --join https://server.example --no-project --token-file PATH
```

仅查看配置不会兑换 Runner 配对码，也不会生成 Runner token、`client_id` 或 `runner.toml`。Desktop 显示“本地 Runner · 未配置”，项目列表仍可展示其他 Runner 上获授权的项目。

Runner 接入时，通过 stdin 一次性提供短期 pairing code：

```text
webcodex environment configure --join https://server.example --project PATH --code-stdin
```

pairing code 从 stdin 读取，不能放进命令行参数。由中心 Server 的管理员创建短期 code。用户 token 和 Server bootstrap credential 应保留在各自授权的机器上。

已配置 Runner 的机器可用 `webcodex environment add-project PATH` 添加仓库；它会复用本机身份。viewer 机器首次添加项目时会提示转为 Runner，并要求一次性 pairing code。

使用 `webcodex environment status --json` 或 `webcodex environment doctor --json`检查配置。通过 `webcodex environment resume`恢复中断的设置；只有无法确定之前的一次性 code 是否已兑换时，才用 `--new-pairing-code` 请求新 code。不要例行更换 code。

普通 `resume` 使用 `--token-file` 是为了继续配置，不会轮换已保存的用户凭据。若已保存的 Server 用户凭据丢失或失效，请运行 `webcodex environment repair-user-credential [--token-file PATH]`；不提供 `--token-file` 时，CLI 会通过隐藏终端输入安全读取凭据。

## 迁移现有 Linux CLI 服务

Linux 为受支持的旧 CLI 服务提供显式迁移命令。迁移会保留既有身份，不会签发新的配对码。源码审查已完成，但这些命令尚未通过原生安装包/服务验收；不要将这些路径视为已完成 M2f 原生验收。

对于用户所有的旧 Runner，请由原用户运行，并提供该用户私有的 user credential 文件：

```bash
webcodex environment migrate-legacy-runner \
  --join https://server.example \
  --project /home/alice/src/repo \
  --token-file /home/alice/.config/webcodex/webcodex-user-token
```

只有命名 profile 才添加 `--profile NAME`。迁移只接受已知、带 owner 的 Runner 配置及其生成的用户 unit，复用现有 Runner 身份、项目 ID 和用户凭据，不会重新配对。不会推测无 owner 的 shared-key 配置或自定义 unit。

对于 root 所有的固定 system Server unit/socket，请用原 Server 用户名、该用户私有的 API credential 文件和旧监听地址执行显式迁移：

```bash
webcodex environment migrate-legacy-server \
  --user alice \
  --token-file /home/alice/.config/webcodex/webcodex-user-token \
  --listen 0.0.0.0:8080 \
  --server-url http://127.0.0.1:8080
```

`--server-url` 可选；省略时 CLI 会从原监听地址推导本机 URL。此命令只迁移 Server，目标仅限已知的 systemd unit/socket 以及固定的环境和数据目录。独立配置的旧 Tunnel 仍归原 owner/profile 管理，不会由此迁入 Core。命令不会猜测自定义 unit、路径或 owner。妥善保护 credential 文件，不要用 Server bootstrap token 替代原用户的 API credential。

## Tunnel 配置

使用 `webcodex environment configure-tunnel PROFILE --credentials-file PATH` 配置命名 Tunnel profile。受保护的 JSON 文件包含 `tunnel_id` 和 `api_key`；CLI 也支持通过隐藏终端输入提供凭据。使用 `webcodex environment tunnel-status PROFILE` 查看状态，或使用 `webcodex environment remove-tunnel PROFILE` 删除 profile。通过 `webcodex environment start tunnel --profile PROFILE`、`stop tunnel --profile PROFILE` 或 `restart tunnel --profile PROFILE` 管理指定 profile。请妥善保护凭据文件，并在使用后删除。

## 服务与凭据

计划使用的持久服务管理器分别是 Linux systemd、macOS LaunchDaemon 和 Windows Service Control Manager（SCM）。使用 `webcodex environment start server`、`webcodex environment stop runner` 或 `webcodex environment restart tunnel` 管理组件（按需替换操作和组件）。持久服务独立于 Desktop 窗口。

如果 Windows Runner 服务丢失登录凭据，使用 `webcodex environment repair-credential runner`。它要求通过隐藏控制台输入真实 Windows 账户凭据；Windows Hello PIN 不是账户密码。服务以真实用户 SID 运行。服务登录密码仅由 SCM 保存；WebCodex 不会另存密码副本。

Desktop Diagnostics 只为已保存环境所拥有的本机组件提供独立 Start、Stop、Restart 控制。Server-only 环境只有 Server 控制；仅查看端没有本机服务控制。以查看端连接同一台机器的 Server，也不会接管由其他方式管理的服务。Windows 上还提供 Runner 服务原生凭据修复。Desktop 打开已有持久服务的环境时只读取状态并定期刷新，不会自动重启已停止的服务。若要恢复已保存的 Server 用户凭据，可在 Diagnostics 使用 **Restore Server user credential** 并通过受保护输入提供新凭据，或运行 `webcodex environment repair-user-credential [--token-file PATH]`。操作会核对已保存的 Server 和用户名，再原子写入新凭据；不会配对 Runner，也不会更改服务状态。

ChatGPT 使用中心 Server 现有的 MCP/Tunnel 集成。其他机器上的 Runner 需要有一条自己可访问的 Server URL。OpenAI Tunnel 只承载 ChatGPT 到 MCP 的连接，不能作为 Runner 接入地址。配置不会自动开放防火墙端口或修改 Server 监听地址。Tailscale 可作为可选的私有网络连通方式，WebCodex 本身不依赖它。

macOS 的开机恢复以系统和项目所在磁盘已解锁为前提。FileVault 的启动解锁由操作系统负责；WebCodex 不关闭加密或保存磁盘解锁密码。磁盘解锁后，后台项目任务不要求 Desktop 窗口保持打开；GUI 操作仍要求所属用户的活动、未锁定会话。参见 [Apple FileVault 说明](https://support.apple.com/guide/deployment/intro-to-filevault-dep82064ec40/web)。

## 升级

正常升级会根据已发布 HTTPS source manifest 验证候选构建，包括 source SHA、version 和 provenance hash。各身份按 manifest 声明关系校验，不要求来自同一个 CI job 或 timestamp。开发和 CI 构建必须显式使用 `--development-build`，并报告 `provenance_verified: false`；这不属于官方 Release 验证，不能如此宣传。

Linux 和 macOS 的已有安装升级，要求原用户先针对候选包和自己的 EnvironmentStore 运行 Core `upgrade-prepare`。管理员用 `installer-authorize` 授权该 receipt；包钩子会将它与候选包及稳定 runtime 目录核对，不会打开 root 的默认 EnvironmentStore。包完成阶段由 root 下的已安装 CLI 通过窄范围 owner-context broker 运行 `installer-finish`。只有 Core 提交 owner 事务后才报告成功；失败时保留授权 receipt 供恢复。全新安装使用隔离的安装事务目录，不会启动服务。Windows 外层安装器以当前用户身份运行，在调用内层 Tauri 安装器前准备并验证该用户的 Store，完成后运行 `upgrade-finish`。

重新安装完全相同且已验证的包会走只读、幂等验证路径，即使尚未配置 Environment 也一样。Unix 上没有 Environment 时，安装器仍会拒绝不同版本的包，需要由 owner 准备恢复记录；同包校验不能用作替换不同文件的许可。已有 Environment 的不同候选包仍需要 owner receipt 流程和已发布 source manifest 验证。

回滚范围严格遵循包所拥有的文件。Linux 恢复受管 Desktop 可执行文件；macOS 恢复完整 `.app` bundle；Windows 只快照并恢复四个精确受管可执行文件：`WebCodex.exe` 和 `webcodex-runtime/` 下的 CLI、Server、Runner。系统菜单项、`.desktop` 文件和卸载元数据由包管理器负责，不属于应用回滚快照。Core 也会快照 runtime 可执行文件和 Environment 数据。已安装 CLI 损坏后的恢复和完整外层安装器回滚仍需原生验证。旧 CLI 服务迁移是上文单独说明的显式 Linux 流程，不会在安装时自动执行。以上升级和迁移流程尚未通过原生 Windows NSIS、macOS package 或 Debian package 验收。

平台验收步骤和仍需真实机器验证的事项见[部署验收清单](unified-deployment-validation.md)。npm/runtime 压缩包和 Docker 高级说明保留在[部署指南](DEPLOYMENT.zh-CN.md)。
