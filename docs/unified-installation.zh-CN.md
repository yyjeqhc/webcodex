# 统一安装指南

[English](unified-installation.md)

本文描述本分支正在开发的统一安装流程，不代表已有新版本发布。Windows NSIS、macOS 安装包和 Debian 12 / Ubuntu 22.04+ `.deb` 计划在 x64 与 arm64 上统一包含 Desktop、CLI、Server 和 Runner。Linux 统一安装包会在原生架构的 Ubuntu 22.04 容器中构建和探测，将 GLIBC 符号版本限制在 2.35 以内，并检查运行时依赖。三种平台的真实机器安装、重启持久性、GUI 会话行为和升级尚未全部验收。具体检查项见[部署验收清单](unified-deployment-validation.md)。

已发布文件请从 [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases)获取。仓库 [`download/`](../download/README.md) 目录仅包含静态页面源文件；生成的 `manifest.json` 不提交到仓库。[下载页 workflow](https://github.com/yyjeqhc/webcodex/actions/workflows/download-page.yml) 会在 Release 发布后构建基于 manifest 的 GitHub Actions artifact，但不会托管或部署网页。安装包发布前如需预览特定源码修订，请看 [Linux 源码预览](DESKTOP_DEVELOPMENT.zh-CN.md#linux-源码预览与已有-server)。

## 一台电脑

以下流程适用于对应平台的安装包已经验收并发布之后；源码预览请使用上文单独列出的开发流程。

1. 安装对应平台的安装包并打开 WebCodex Desktop。
2. 选择**创建主节点**并确认配置。Server 与本机 Runner 为两个独立服务，跳过可选初始项目仍启用 Runner。**高级**保留明确的 Server-only 设置；既有环境保持原角色。
3. 在 Desktop 确认 Server、Runner 和项目状态。Desktop、CLI 与网页使用同一 Server 的授权视图。浏览器打开 `SERVER_URL/runtime`，使用现有用户凭据查看获授权的 Runner、项目和状态。
4. 通过 Server 配置现有 ChatGPT MCP/Tunnel 连接。ChatGPT 始终连接中心 Server。远程项目路径属于 Runner 所在机器，不是 Server 机器上的本地路径。

关闭 Desktop 不会停止持久服务。GUI helper 只会在同一用户已登录且未锁定的会话运行。Linux 不新增 GUI helper backend。

## 多台电脑

先按单机流程创建相同的完整主节点，保留中心 Server 与本机 Runner。其他机器作为追加 Runner 接入；主节点也能操作自己的项目。在每台机器安装适配系统和架构的安装包。高级 Server-only 和 viewer-only 能力仍然保留。

在主节点打开**项目 → 添加设备**，填写其他机器可访问的 Server 地址，再明确点击**创建邀请**。在追加机器安装 WebCodex，选择**加入主节点**，输入直接地址并通过受保护输入提供一次性配对码。初始项目可跳过，Runner 仍保持启用。CLI 仍可使用 `webcodex environment invite`；配对码属于密钥，不要放入命令行参数或日志。

ChatGPT 通过 OpenAI Tunnel 连接中心 Server；追加 Runner 使用独立的直接 Server 地址。环回地址或 OpenAI 地址不能用作额外机器的接入口。无法访问时需检查监听地址、防火墙、域名或私有网络，窗口不会修改这些设置。邀请已创建不等于设备已接入，随后通过现有授权设备和项目清单刷新查看。参见[Desktop 添加设备说明](desktop-guide.zh-CN.md#添加其他设备)。

在中心 Server 机器创建环境：

```text
webcodex environment configure --create --project PATH
```

CLI 无项目主节点使用 `webcodex environment configure --create --runner`；仅使用 `--no-project` 而不指定 `--runner` 会明确保留高级 Server-only 角色。远程机器可用 `webcodex environment configure --join https://server.example --runner --code-stdin`。既有环境保留原角色；首次为 viewer 启用 Runner 时，通过 Desktop 明确勾选允许本机工作，或使用 CLI 的 `add-project` 转换。在每台仓库机器加入 Server：

```text
webcodex environment configure --join https://server.example --project PATH
```

使用 `--no-project` 可仅作为查看端加入，不配置本机 Server 或 Runner。普通 Desktop 创建/加入默认允许本机工作；高级保留明确的角色选项。CLI 的 `--runner` 可启用 Runner 而不选默认项目；兼容的 `--project PATH` 仍表示启用 Runner 并注册该明确项目。随后按所选流程收集 Server 地址、认证信息和必要的系统授权。以 viewer 身份加入时，使用用户个人访问 token，通过隐藏输入或受保护的 `--token-file` 提供；不使用 pairing code。例如，从受保护文件导入已有用户 API 凭据：

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

## 从 v0.4.6 升级

v0.4.6 已经包含 Environment。请保留原 Environment 目录、服务所有者和 scope、
Runtime 选择、私有凭据以及恢复记录；普通升级不需要删除配置或重新配对 Runner。
升级到 v0.5 后，刷新 MCP 连接的工具 schema，并重新打开旧 App 阅读器，以使用新的
规范工具名称。具体保留边界、旧安装迁移，以及“Desktop 已保存但 Environment 尚未
配置”的 Tunnel profile 如何显式补齐，见[兼容性政策](compatibility-policy.md)。
该 CLI 补齐路径会安装并启动 standalone 服务，不是仅保存配置的导入操作。

## 从 v0.4.6 之前的安装升级

v0.5 将正式发布的 v0.4.6 Environment 版本作为最低直接升级来源。pre-Environment Linux 安装以及官方 Windows v0.4.3 包不再由 v0.5 直接迁移。请先升级或迁移到 v0.4.6，确认 Environment 已正确接管原 Server/Runner 身份和服务，再从该 Environment 升级到 v0.5。

这样历史安装形态只需要经过一个已发布的桥版本，不必让后续每个版本继续携带旧 systemd handoff 和 v0.4.3 package classifier。跨桥升级时不要删除或重建凭据；旧身份由 v0.4.6 负责导入，v0.5 只保留并升级生成后的 Environment 数据。

## Tunnel 配置

持久本机 Server 推荐在创建时直接选择 Server owner：`webcodex environment configure-tunnel work --host embedded --credentials-file /secure/work.json`。受保护 JSON 只包含 `tunnel_id` 和 `api_key`，也可使用隐藏终端输入。该命令把精确 profile 与私有 local-MCP binding 写入 `EnvironmentStore`，绝不安装或启动 standalone Tunnel 服务，也不会 hot reload 或暗中重启正在工作的 Server。JSON 结果会返回 `server_restart_required` 和 `next_action`；可连续配置多个 profile，最后按提示只执行一次 `webcodex environment restart server`。Desktop 中对应 **Run with WebCodex Server (recommended)**，并显示同一个显式重启操作。

只有明确需要独立 per-profile 服务时才使用 `--host standalone`；原有 `start|stop|restart tunnel --profile PROFILE` 生命周期保持不变。已有 profile 改 owner 仍必须在干净停止并卸载 standalone 服务后显式执行 `tunnel-host PROFILE --host ...`；配置流程不会自动接管 foreign 或 legacy 服务。使用 `tunnel-status PROFILE` 查看状态，使用 `remove-tunnel PROFILE` 删除 profile。

持久模式下，CLI 与 Desktop 都以 `EnvironmentStore` 为唯一 profile catalog。历史 Desktop `secrets/tunnel-config.json` 只服务 legacy/non-persistent runtime；若它与持久 Environment 同时存在，只作为 fail-closed profile/Tunnel identity 栅栏，不再是第二套可写 catalog。历史 API-key 字节不会覆盖或阻止带 revision 栅栏的 EnvironmentStore key rotation。请妥善保护凭据文件，并在使用后删除。

## 服务与凭据

计划使用的持久服务管理器分别是 Linux systemd、macOS LaunchDaemon 和 Windows Service Control Manager（SCM）。使用 `webcodex environment start server`、`webcodex environment stop runner` 或 `webcodex environment restart tunnel` 管理组件（按需替换操作和组件）。持久服务独立于 Desktop 窗口。

如果 Windows Runner 服务丢失登录凭据，使用 `webcodex environment repair-credential runner`。它要求通过隐藏控制台输入真实 Windows 账户凭据；Windows Hello PIN 不是账户密码。服务以真实用户 SID 运行。服务登录密码仅由 SCM 保存；WebCodex 不会另存密码副本。

Desktop Diagnostics 只为已保存环境所拥有的本机组件提供独立 Start、Stop、Restart 控制。Server-only 环境只有 Server 控制；仅查看端没有本机服务控制。以查看端连接同一台机器的 Server，也不会接管由其他方式管理的服务。Windows 上还提供 Runner 服务原生凭据修复。Desktop 打开已有持久服务的环境时只读取状态并定期刷新，不会自动重启已停止的服务。若要恢复已保存的 Server 用户凭据，可在 Diagnostics 使用 **Restore Server user credential** 并通过受保护输入提供新凭据，或运行 `webcodex environment repair-user-credential [--token-file PATH]`。操作会核对已保存的 Server 和用户名，再原子写入新凭据；不会配对 Runner，也不会更改服务状态。

ChatGPT 使用中心 Server 现有的 MCP/Tunnel 集成。其他机器上的 Runner 需要有一条自己可访问的 Server URL。OpenAI Tunnel 只承载 ChatGPT 到 MCP 的连接，不能作为 Runner 接入地址。配置不会自动开放防火墙端口或修改 Server 监听地址。Tailscale 可作为可选的私有网络连通方式，WebCodex 本身不依赖它。

macOS 的开机恢复以系统和项目所在磁盘已解锁为前提。FileVault 的启动解锁由操作系统负责；WebCodex 不关闭加密或保存磁盘解锁密码。磁盘解锁后，后台项目任务不要求 Desktop 窗口保持打开；GUI 操作仍要求所属用户的活动、未锁定会话。参见 [Apple FileVault 说明](https://support.apple.com/guide/deployment/intro-to-filevault-dep82064ec40/web)。

## 升级

正常升级会根据已发布 HTTPS source manifest 验证候选构建，包括 source SHA、version 和 provenance hash。各身份按 manifest 声明关系校验，不要求来自同一个 CI job 或 timestamp。开发和 CI 构建必须显式使用 `--development-build`，并报告 `provenance_verified: false`；这不属于官方 Release 验证，不能如此宣传。

Linux 和 macOS 的已有安装升级，要求原用户先针对候选包和自己的 EnvironmentStore 运行 Core `upgrade-prepare`。管理员用 `installer-authorize` 授权该 receipt；包钩子会将它与候选包及稳定 runtime 目录核对，不会打开 root 的默认 EnvironmentStore。包完成阶段由 root 下的已安装 CLI 通过窄范围 owner-context broker 运行 `installer-finish`。只有 Core 提交 owner 事务后才报告成功；失败时保留授权 receipt 供恢复。全新安装使用隔离的安装事务目录，不会启动服务。Windows 外层安装器以当前用户身份运行，在调用内层 Tauri 安装器前准备并验证该用户的 Store，完成后运行 `upgrade-finish`。

Windows 会由绑定候选包 manifest 的新 CLI 单独判定安装类型和 Environment 所有权。官方 x64 v0.4.3 通过旧 build identity 及缺少 Environment 数据格式声明识别，不会向旧 CLI 发送 Environment 命令。完整但尚未配置 Environment 的新版安装也走独立的程序包事务。升级前须退出旧 Desktop 并停止其 Server/Runner；程序不可访问或正在运行、安装不完整、用户不匹配、存在无法对应 owner 的 Windows 服务时，会阻止替换。候选 CLI 保存私有备份并生成 `windows-package-prepared.json`，绑定原用户 SID、四个精确文件目标、事务及已发布 manifest。调用内层安装器前核对凭据和旧文件；完成阶段核对四个安装文件的 hash 和 build-info 启动结果；回滚恢复原来的四个程序。此流程不会创建 Environment、注册 Runner、修改 provider 配置或迁移 Server 数据。已有 Environment 保留已安装 CLI 和现有 owner 事务路径。

如果程序回滚失败，请保留所选 Environment 目录内的 `windows-package-upgrade.json` 和 `upgrade-backups/`。停止相关程序后，使用已验证的新 CLI 执行 `environment package-upgrade-rollback --expected-runtime-dir <原安装目录>/webcodex-runtime --environment-dir <原Environment目录> --json`。不要删除恢复文件或修改事务目标。原生 Windows 测试包含兼容 v0.4.3 的 PE 夹具，其 Environment 命令返回 exit 2；真实官方安装器验收仍待执行。

重新安装完全相同且已验证的包会走只读、幂等验证路径，即使尚未配置 Environment 也一样。Unix 上没有 Environment 时，安装器仍会拒绝不同版本的包，需要由 owner 准备恢复记录；同包校验不能用作替换不同文件的许可。已有 Environment 的不同候选包仍需要 owner receipt 流程和已发布 source manifest 验证。

回滚范围严格遵循包所拥有的文件。Linux 恢复受管 Desktop 可执行文件；macOS 恢复完整 `.app` bundle；Windows 只快照并恢复四个精确受管可执行文件：`WebCodex.exe` 和 `webcodex-runtime/` 下的 CLI、Server、Runner。系统菜单项、`.desktop` 文件和卸载元数据由包管理器负责，不属于应用回滚快照。Core 也会快照 runtime 可执行文件和 Environment 数据。已安装 CLI 损坏后的恢复和完整外层安装器回滚仍需原生验证。旧 CLI 服务迁移是上文单独说明的显式 Linux 流程，不会在安装时自动执行。以上升级和迁移流程尚未通过原生 Windows NSIS、macOS package 或 Debian package 验收。

平台验收步骤和仍需真实机器验证的事项见[部署验收清单](unified-deployment-validation.md)。npm/runtime 压缩包和 Docker 高级说明保留在[部署指南](DEPLOYMENT.zh-CN.md)。
