# WebCodex Desktop 使用指南

[English](desktop-guide.md) | [简体中文](desktop-guide.zh-CN.md)

Desktop 负责启动和维护本机 WebCodex Runtime；用户只需要在 ChatGPT 等 AI 客户端中描述要做的工作。本机使用不需要在 Desktop 里先选择、添加、激活或取消注册 Project。首次安装、Tunnel 配置和系统权限请看[安装与连接指南](desktop-install.zh-CN.md)。

贡献者如果要做 frontend/Tauri 开发、从源码加载 runtime、构建 NSIS/DMG 或运行原生安装 smoke，请看 [Desktop 开发与打包](DESKTOP_DEVELOPMENT.zh-CN.md)。

## Runtime 项目清单

**项目**页是当前 Runner 已观察到的 Runtime Project 只读清单，显示 Git 分支、活跃 Session 和最近活动。Runtime Project identity 仍然承担授权、路由、持久化、审计和 Session 边界，但它不是用户需要在 Desktop 中维护的前置资源。

本机 Full Runtime 中，ChatGPT / 模型调用直接提供具体工作目录。`work_on_project(path)` 会复用已经注册的精确 Project；如果尚未注册且 Runner policy 允许，则按需自动注册。多个 Project 可以并发存在，打开一个不会撤销另一个。

Runner 文件系统 policy 仍然是权限边界。全新 Desktop 的本机 Runner 使用 Runner 的正常默认策略；`allowed_roots` 为空时，其有效范围默认是当前用户的 home 目录。显式 Runner policy 可以进一步收窄范围。这个范围是 Runner 级授权，与 Runtime Project identity 分开：例如 E 盘中的 Project 不会因此获得 F 盘访问权，只有 F 盘目标路径也落在 `allowed_roots` 内时，Runtime 才能在那里解析或注册 Project。Project 注册本身不会扩大这个权限范围。

## 第一次使用

1. 启动 WebCodex Desktop，首次选择本机或远程环境。本机配置默认勾选**允许 AI 在此电脑上工作**，不需要默认 Project。确认配置后才会安装持久服务、请求系统授权；仅打开窗口不会抢先把新机器绑定到本地 Server。
2. 直接在 ChatGPT 中描述工作。请求给出工作目录后，Runtime 会在 Runner 允许的范围内自动解析或注册对应 Project。后续打开 Desktop 只观察已保存的持久服务，不会自动重启被主动停止的组件。
3. **项目**页仅用于观察已经出现的 Runtime Project；正常 Desktop UI 不再提供添加、激活、重新激活或取消注册 Project 的流程。
4. 只有需要让 ChatGPT 从外部访问本机时，才配置 **OpenAI Secure Tunnel**。Tunnel 负责连接，不定义 Project 权限。
5. 观察到真实项目调用只能证明此前发生过客户端使用，不能证明宿主此刻在线。

如果已经有远程 Server，选择 **连接现有 Server** 并填写地址。勾选允许本机工作、输入一次性配对码即可接入本机 Runner，不需要选项目；取消勾选则通过用户 API 凭据作为 viewer 加入。已有连接会复用原身份。远程 Runner policy 和外部连接由其管理者负责。

如果只想临时共享一个项目，选择 **快速共享项目** 和连接提供方。Quick Share 仍保留显式项目选择和独立的临时生命周期。

## 每天从首页开始

首页仅显示 Server、Runner、连接状态、ChatGPT 工作入口，以及项目、活动与扩展的快捷入口。项目列表和调用记录分别在项目页、活动页查看。健康状态不再重复展示就绪信息；需要处理的故障仍显示恢复入口。本机 Full Runtime 即使没有默认 Project 也属于正常可用状态；Desktop 启动不以 Project readiness 为前置条件。

- **项目**：只显示已观察到的 Runtime Project。Project 生命周期由模型 / Runtime 的路径解析驱动，而不是 Desktop 按钮。
- **活动** 与 **扩展**：围绕所选 Session 或已观察上下文关联的 Runtime Project 工作。
- **连接**：独立管理外部可达性，不改变 Project 权限。
- **设置**：提供 Runner 配置、诊断和显式运维控制。

持久环境使用[统一安装指南](unified-installation.zh-CN.md)中的共享配置和服务生命周期。Desktop 还显示 Server 授权的机器与项目清单；远程路径在此电脑仅作展示，不会作为本机目录使用。viewer 没有本机 Runner。[原生平台验收记录](unified-deployment-validation.md)明确区分自动检查与真实安装、迁移验收。

项目页先展示工作目录，再展示**执行任务的设备**。Runner 是在这些设备上读取文件和执行命令的服务；“已连接 Server”表示该服务在线。**暂时无法接入桌面**仅说明截图、窗口及鼠标键盘操作所用的桌面会话不可用，文件与命令操作不依赖此状态。未上报或过期的桌面状态会单独标明，不会被当成不可用；桌面会话已连接也不代表系统权限已经授予。本机的检查按钮打开设置中的文件访问与权限，不会启动服务或自动申请权限。

活动页分为**工具调用**、**工作会话**和**服务事件**。工具调用按来源展示最近一次调用，点击可查看完整记录；“调用完成”只描述工具执行结果，不确认 ChatGPT 已收到响应。工作会话展示进度、正在执行的任务和待确认事项；“未结束”不表示正在运行。检查结果使用明确的状态，重复任务计数与空的关注项不再占据列表，HTTP 传输与内部标识保留在详情中。

ChatGPT 在不同工作目录之间切换时，不需要先停止 Runtime 或 OpenAI Secure Tunnel。Runtime 会根据当前 Runner policy 解析兼容路径，并按需注册 Project。
## 连接与故障恢复

连接卡片直接显示观察到的隧道与本地 MCP 可达状态。故障会显示具体原因，并提供代理设置、Runtime 或故障诊断入口。页面显示当前网络路径；故障建议使用失败时的代理证据，不会自动更改网络路径。

通过 **添加连接** 或连接卡片的 **编辑** 按钮设置 Tunnel ID 与只写 API key。普通 Tunnel 运行中或停止后均可编辑。保存后，仅替换本应用正在管理的普通 Tunnel，Server 和 Runner 保持运行；原先停止的 Tunnel 不会因保存而自动启动。API key 留空保留已保存密钥，密钥不会回传到界面。

保存和重新连接是两个结果。替换失败时明确提示 **配置已保存，但 Tunnel 尚未恢复**：应重试连接，无需重复输入已保存密钥。旧进程清理未确认时保留所有权及重试能力，不会虚报已停止。Quick Share 仍是独立的临时连接，新凭据在其下次启动时使用。

观察到真实项目调用，只能验证曾经使用过，不能证明宿主此刻在线。尚未观察到调用或无法观测时显示“未验证”，不据此断言 ChatGPT 已断开。

| 看到的情况 | 下一步 |
| --- | --- |
| 本机 Server 或 Runner 已停止 | Core 管理的持久环境可在诊断页明确启动相应服务；重新打开 Desktop 不会启动被主动停止的持久服务 |
| 请求的工作目录超出 Runner policy | 只为目标工作区调整 Runner 访问范围，然后重新发送自然语言请求 |
| Tunnel ID 或 API key 未检测到 | 在配置区填写并保存 Tunnel ID 和 API key；仅使用环境变量时才需退出重开 |
| 启动失败且没有活动隧道 | 修复配置或网络后再次点击启动 |
| 已有隧道报错 | 点击停止，成功后重新启动；停止失败会保留错误和重试入口 |
| 隧道就绪但剪贴板交接失败 | 在连接页重试“复制 Tunnel ID”，或选中 ID 手动复制，无需重启 |
| 隧道就绪，等待 ChatGPT | 在 ChatGPT 配置 Tunnel，然后直接描述要处理的目标工作目录 |
**设置 → 网络** 管理自动、直连和自定义 HTTP 代理。更改运行中隧道的代理前先停止隧道，保存后再启动。

## 指令、Skills 与原生 Tool Plugins

路径按条目显示，可以选择文件或文件夹，也可以直接输入完整路径。添加、编辑和移除先保存在草稿中，点击**保存**后应用；**取消**恢复当前分类的已保存路径。项目预览中没有可用 Skill 时，可使用上方的**添加 Skill 文件夹**。读取 Runner 设置失败时会显示刷新提示，并保留已有路径草稿。

**扩展**按编程代理、SSH 资源、MCP Providers、Skills 和指令分类。无需选择项目即可配置共享的本机 Runner 路径；下方项目选择器仅选择只读的扩展预览，全局指令编辑器独立于项目预览。扩展使用分类导航；指令页和 Skills 页仅显示各自的路径配置。切换分类或项目预览保留草稿，保存一类路径不会应用另一类未保存的修改。全局指令文件和 Skill 根目录各支持最多 16 个绝对路径。保存绑定到读到的精确本机 Runner 配置，拒绝过期编辑，保留其他配置、注释、凭据及已有插件设置。viewer 不能通过此页面配置远程 Runner 的本机文件。

**添加原生 Tool Plugin** 可填写新插件 ID、名称、可执行程序、字符串数组形式的参数，以及可选的绝对工作目录。不会覆盖已有 ID；参数仅写入，提交后清空。凭据请使用配置中的 profile。注册列表不代表实时连接状态；已有插件的编辑、移除及高级字段仍通过页面展示的 Runner 配置文件管理。

保存设置不会启动插件或偷偷重启 Runner。**重启本应用管理的 Runner** 是显式操作，会中断该 Runner 当前工作，但保留 Server 和普通 Tunnel。独立运行的 Runner 必须由其实际管理者重启。

## macOS 权限引导

首次前台启动且 Desktop 权限不全时，会显示权限说明，提供请求辅助功能、请求屏幕录制和稍后继续入口。后台登录启动不会弹出对话框抢占其他应用焦点。系统权限请求必须由按钮触发；拒绝或跳过不影响普通项目操作。授权后可在设置页重新检测。

Desktop 的授权结果不等于独立 Runner 已获授权。持久 Runner 的 GUI 操作由同用户交互会话中的 helper 执行，要求活动且未锁定的会话；GUI 不可用时，项目服务仍可在线。应为 macOS 显示的实际 helper 进程授权，并按系统提示重启。旧前台 Runner 仍使用自身的 Computer backend。未观测到的 Runner 授权状态不会被显示为已授权。原生 GUI helper 与会话行为仍需完成[部署验收清单](unified-deployment-validation.md)中的平台检查。Windows 不显示 macOS 专属权限按钮。

## 活动、设置和后台运行

本机 Server 在启动期间退出时，可展开错误提示中的 **详情**。Desktop 会显示可用的退出码；识别到 HTTP 监听绑定失败时，还会显示监听地址和系统错误码，例如 `127.0.0.1:54611` 和 `os error 10013`。诊断从有大小上限的进程输出中提取，不展示任意原始日志，也不改变端口选择或恢复行为。

**活动** 按最新在前显示操作记录。可搜索内容或来源，也可选择 **只看警告和错误**；筛选不会删除记录。

**设置** 按通用、文件访问与权限、网络、Runtime、故障排查、关于与更新分区。选中分区后直接显示配置，无需展开折叠项；在分区之间切换会保留网络和追踪设置草稿，不会自动保存或重启服务。通用包含语言、外观和登录时启动；文件访问与权限包含 Runner 允许目录和 macOS Computer Use 权限。语言也可以在左下角直接切换。支持简体中文、English、日本語、한국어、Deutsch、Français；选择会保存，重开后恢复，活动时间按所选语言格式显示。系统托盘菜单和原始后端诊断仍使用英文，系统文件选择器的语言由操作系统决定。

关闭窗口只会把 Desktop 隐藏到菜单栏或系统托盘。持久的 Server、Runner 和 Tunnel 服务独立于窗口运行，退出 Desktop 后也会继续运行。托盘中的 **退出 WebCodex** 会结束 Desktop 及其直接管理的进程，例如临时 Quick Share、旧版由 Desktop 管理的 Runtime 或普通 Tunnel。Core 管理的本机服务需要使用诊断页的明确控制；仅此电脑拥有的组件才显示对应控制。viewer 没有本机 Runner 控制，只有 Server 的电脑也没有 Runner 控制。Desktop 可以通过 Server 连接观察自定义旧服务，但不会接管其生命周期。首页用于旧版 Desktop Runtime 的停止操作还会更新已保存的启动偏好，不会停止独立服务。

macOS 使用 **⌘ + 1–6**，Windows 使用 **Ctrl + 1–6**，依次切换首页、项目、活动、连接、扩展和设置。这些导航快捷键在输入框和语言选择框内同样生效，普通输入及复制、粘贴等文本编辑快捷键不受影响。可用 Tab 聚焦按钮，用 Enter 激活；设置分区支持上下方向键及 Home / End 切换。

Runtime 和故障排查拥有独立设置分区，常用操作直接显示。主动停止后显示“已停止”，点击“启动”继续。活动页优先显示操作结果，勾选“显示进程详情”查看常规进程事件。Tunnel ID 和密钥在连接页配置；API key 不回显。

### 无默认 Project 的 Runtime

Full Runtime 由 Server、Runner 和 Runner 已观察到的 Project 清单组成。Desktop 默认展示 Project 是可选状态；全新的本机 Desktop 会刻意在没有默认 Project 的情况下启动。持久环境也可以只有 Server，或仅作为 viewer。重新打开 Desktop 只观察持久服务，不会启动已主动停止的组件。
Desktop 重启会恢复已保存的 Server/Runner 身份和 Connections，不会预先注册 Project。只有模型驱动的工作解析了具体目录时，才会出现对应 Runtime Project。在线且完整的 Runner inventory 是这些注册状态的权威来源；Desktop 可以清理过期的展示历史，但正常 Desktop UI 不负责修改 Project 注册状态。

项目目录、Git 文件和 Runner policy 是彼此独立的边界：按需注册 Project 只是为已经获得授权的路径创建或复用 runtime identity，不会扩大 `allowed_roots`。
