# Runtime 安装与 CLI 加入主节点

[English](runtime-installation.md)

Linux **Runtime** 软件包包含 CLI、Server 和 Runner；**Full** 还包含 Desktop。软件包内容与电脑角色是两回事：安装 Runtime 不会创建环境或启动 Server，加入主节点只会启动本机 Runner。Runtime 软件包及此更新路径仍在开发中；此处的源代码与聚焦测试不能证明原生安装、重启或回滚已通过验收。使用软件包流程前，必须已有对应系统和架构的已发布文件。参见[统一安装](unified-installation.zh-CN.md)与[部署验收](unified-deployment-validation.md)。

## 加入已有主节点

安装匹配本机 Linux 架构及软件包管理器的官方 Runtime 软件包。以项目原有用户运行 CLI，SSH 会话也应使用该用户；不要在配置命令前加 `sudo`。选择系统服务时，流程会单独请求系统授权，同时保留原用户身份。主节点所有者提供此电脑能直接访问的 Server URL，并通过 Desktop 的“添加设备”或 `webcodex environment invite` 创建短期邀请码。Server 的引导凭据和 Tunnel/API Key 应留在其所属电脑上。

在终端中加入，并启用无初始项目的 Runner：

```text
webcodex environment configure --join https://server.example --runner --no-project --scope user
```

显式配置时可加上 `--runner-name "SSH worker"`，沿用现有 Runner 显示名称（最多 200 字符，不含 NUL），不改变 Runner 或项目身份。已保存的配置和恢复流程保留原名称。

在隐藏输入提示中输入一次性邀请码。不要把它写进命令行参数、shell 历史、日志或命令替换。需要受保护的非交互输入流时，添加 `--code-stdin`；CLI 只通过 stdin 消费一次邀请码。使用不会把秘密放进 argv 的受保护秘密来源提供输入。存在 `--runner` 时，`--no-project` 不会关闭 Runner；没有 `--runner` 的 `--no-project` 则保留原有查看者角色。

用户服务遵循现有系统管理器契约。Linux 退出登录后能否继续运行，取决于用户的 systemd 管理器与 linger 配置；设置不会自动改用系统服务。若要启动时运行的系统服务，请明确选择 `--scope system` 并完成系统授权。后续命令仍以同一原用户执行；已保存环境保留其服务范围和身份。

若加入时注册目录，可将 `--no-project` 换成 `--project /home/alice/src/repo`。也可稍后添加：

```text
webcodex environment add-project /home/alice/src/repo
webcodex environment status --json
webcodex environment doctor --json
```

此操作复用同一 Runner 身份。加入不会创建、安装或启动中央 Server；Runtime 内含 `webcodex-server` 不等于授权启用它。直连 URL 与 ChatGPT 的 Tunnel/MCP 连接是不同路径，应填写主节点的 Server 地址，不是 OpenAI 或回环地址。CLI 不会修改监听地址、防火墙或 DNS。ChatGPT 连接由主节点所有者管理。

若设置中断，以同一用户执行 `webcodex environment resume`。需要邀请码时可提供 `--code-stdin`。只有明确进入替换邀请码的恢复路径才使用 `--new-pairing-code`；不要重新创建其他环境、更换已保存的 Server 地址，也不要假设已消费的一次性邀请码能够重放。没有新增 Runtime 专属的凭据或身份存储。

## 查看与更新已安装的软件包

使用软件包自带的 CLI，以配置该 Environment 的同一原用户运行：

```text
webcodex environment update status --json
webcodex environment update check --json
webcodex environment update download --version VERSION --json
```

将 `VERSION` 换成已发布的稳定版本。`status` 读取本地状态，`check` 查询发行元数据，`download` 校验已发布的哈希与来源并保存候选安装程序。这些命令不替换程序、不重启服务，也不要求交互终端。下载候选包不等于已经更新。

有界的更新 JSON 保留已有 schema，只有验证且符合条件的已安装软件包才在规范字段 `view.installed_target` 中显示目标。Runtime 带有 `flavor: "runtime"`，Full 为兼容旧目标格式省略该字段。已安装目标缺失表示尚未验证，不能因缺少 Desktop 推断为 Runtime。已验证 Runtime 显示 CLI、Server、Runner 三个组件；Full 还显示 Desktop。候选身份与组件独立描述已下载的软件包。本机 Server/Runner 角色描述已保存环境，不表示软件包内含程序的运行状态，也不表示远端电脑在线。

托管更新保留已安装的类型、架构和软件包格式。Runtime 下载选择已验证的 Runtime 目标，Full 选择 Full。不匹配的候选包会在任务检查、sudo 验证和服务准备前被拒绝。手动下载 Full 或缺少 Desktop 都不能授权将安装转换为 Runtime。npm、源代码、裸 Runner、自定义及不支持的安装使用[手动部署/发行路径](DEPLOYMENT.md)，保留其原有配置与所有者。通过 `status` 查看限制；存在新发行版不意味着手动安装具备自动替换资格。

Linux 上符合条件的软件包，可在真实终端中应用，例如分配了 TTY 的 SSH 会话：

```text
webcodex environment update apply --version VERSION --yes
```

原用户保持非特权身份；已有受信安装辅助程序获得系统 sudo 授权。`--yes` 确认此次具体应用，但不能取消 TTY 或系统授权要求。非交互 `apply`、`resume`、`rollback` 会在打开更新存储或准备服务之前失败。此无界面入口不支持 macOS 和 Windows 的应用更新。Runtime 目前面向 Linux；其他平台使用已有原生或手动路径。

运行中的工作、无法确认任务状态、缺少软件包工具、不支持的安装、未经验证的候选包或未结束的操作，都可能阻止应用。辅助程序的启动确认不等于安装成功。请查看规范结果：

```text
webcodex environment update status --json
webcodex environment update resume --operation-id OPERATION_ID --yes
webcodex environment update rollback --operation-id OPERATION_ID --yes
```

恢复时使用 `view.upgrade.operation_id` 报告的准确已保存 UUID，在 Linux TTY 中以原用户操作其环境。这些命令复用已有 Environment 完成/回滚保护，不会启动第二个安装程序，也不会推断不确定的替换可以安全重试。需要恢复时保留日志与私有备份。更新只影响本机已安装软件包及其拥有的环境，不会通过连接升级远端中央 Server 或其他 Runner。原生安装与恢复仍需完成上方链接中的平台验收。
