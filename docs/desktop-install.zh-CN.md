# WebCodex Desktop 快速安装与 ChatGPT 连接

[English](desktop-install.md) | [简体中文](desktop-install.zh-CN.md)

对于普通 Windows / macOS 个人用户，**最推荐的路径是 WebCodex Desktop +
官方 OpenAI Secure Tunnel**。WebCodex Desktop 在本机运行 Server 和 Runner；
真正向 AI 发送编程请求的界面是 **ChatGPT 网页版**。第一次使用不需要配置
公开的 WebCodex 地址、反向代理或 ChatGPT OAuth。

## 完整快速开始

开始前准备好：

- 真正要让 ChatGPT 使用的项目目录，建议项目已经使用版本控制；
- 能看到“开发人员模式”和“插件”页面的 ChatGPT 网页版账户；
- 能够创建 Tunnel 和 API key 的 OpenAI Platform 访问权限。

WebCodex 可以在已注册的项目范围内读取和修改文件、执行命令。只注册确实要
交给 AI 使用的目录，留意工具调用，并且不要把 API key、token 或其他密钥放进
提示词、截图、Git、Issue 或共享日志。

1. **安装并打开 WebCodex Desktop。**从 [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases)
   下载与当前机器架构匹配的 Windows installer 或 DMG。当前 macOS 构建
   未经过 notarization；如果被拦截，进入**系统设置 → 隐私与安全 → 仍要打开**，
   不要全局关闭 Gatekeeper。
2. **选择项目。**选择 **Local Full Runtime / 在此电脑使用 WebCodex**，再选择
   真正的代码仓库目录，等待 **Service**、**Runner** 和 **Project** 全部 Ready。
   WebCodex Desktop 是本机 Runtime 控制器，不是聊天界面。
3. **创建 OpenAI Tunnel 凭据。**在 [OpenAI Tunnels 页面](https://platform.openai.com/settings/organization/tunnels)
   创建 Tunnel，记录完整且准确的 Tunnel ID；再到 [API keys 页面](https://platform.openai.com/settings/organization/api-keys)
   创建 API key。建议使用 Restricted key，只授予 Tunnel 所需权限，包括
   **Tunnels: Read** 和 **Tunnels: Use**。
4. **在 WebCodex Desktop 保存凭据。**进入**连接 → Tunnel 连接配置**，填写
   Tunnel ID 和 API key，然后点击**保存配置**。API key 会以未加密形式保存在
   当前用户的本机应用数据目录，不要分享或提交对应文件。
5. **只在有需要时配置网络。**一般保持**设置 → OpenAI Tunnel 网络 → 自动**；
   只有环境需要时才选择**直接连接**或**自定义 HTTP 代理**。修改后停止并重新
   启动 Tunnel 即可，不需要重启 WebCodex Desktop。
6. **启动 Tunnel。**进入**连接 → OpenAI Secure Tunnel**，点击**启动安全隧道**。
   看到“**OpenAI Secure Tunnel 已就绪，等待 ChatGPT 连接**”后再继续。
7. **在 ChatGPT 网页版添加插件。**打开[账户安全设置](https://chatgpt.com/#settings/Security)，
   开启**开发人员模式**，再打开[插件页面](https://chatgpt.com/plugins)。创建插件，
   连接方式选择 **Tunnel**；选择正确的 Available Tunnel，或点击 **Use tunnel ID
   instead** 填写完整 ID；Authentication 选择 **No Auth**；仅在信任当前安装时
   接受自定义 MCP 风险提示；然后依次点击 **Create** 和 **Connect**。不要把
   Tunnel API key 填入 ChatGPT 网页版。如果 Tunnel 列表可能没有刷新，请和
   OpenAI Tunnels 页面中的完整 ID 对照。
8. **从 ChatGPT 网页版验证完整链路。**发送：“列出 WebCodex 项目，然后列出
   我刚才选择的项目的顶层文件；如果目录为空，请明确报告为空。”这次真实项目
   读取才是最终验收：仅仅看到本机状态全绿，并不能证明 ChatGPT 网页版、Tunnel、
   Server、Runner 和项目权限已经全部连通。

“OpenAI Secure Tunnel 已就绪”只证明本机 Tunnel 已经可以接受 ChatGPT 连接。
插件保存后，WebCodex Desktop 仍可能保守显示“等待 ChatGPT”；以第 8 步的真实
项目读取为准。任一步骤失败时，查看下方对应的详细章节；这些章节保留了操作和
恢复说明，但不是要求用户重新按章节编号执行一次的第二套安装流程。

配置完成后的日常操作请看 [Desktop 使用指南](desktop-guide.zh-CN.md)。CLI、已有
远程 Server、生产部署或高级网络配置请看[完整使用指南](PERSONAL_SETUP.zh-CN.md)
和[部署指南](DEPLOYMENT.zh-CN.md)。

如果你是贡献者，希望修改 Desktop 或自己构建 Windows/macOS 安装包，请看 [Desktop 开发与本地打包](DESKTOP_DEVELOPMENT.zh-CN.md)；下面的安装指南默认你已经拿到一个完整 Release artifact。

## 1. 安装 WebCodex Desktop

从 [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases) 下载对应安装包：

- **Windows：**按主机架构选择 x64 或 ARM64 installer；Windows ARM64 Desktop 从 v0.4.2+ release build path 开始提供。
- **macOS：**按 Mac 架构选择 Intel 或 Apple Silicon DMG。

当前 macOS 构建使用 ad-hoc 签名且没有 notarization。如果 Gatekeeper 拦截新下载构建的首次启动，进入**系统设置 → 隐私与安全 → 仍要打开**，再确认**打开**；不要全局关闭 Gatekeeper。

安装完成后启动 WebCodex Desktop。

**成功时你应该看到：**WebCodex 主窗口能够打开，首页没有安装包/运行时缺失错误。

**失败时：**macOS 被 Gatekeeper 拦截就按上面的“仍要打开”处理；安装包或 bundled runtime 缺失则重新安装同一版本，不要手工拼装内部二进制。

**下一步：**如果按上面的完整快速开始操作，请先选择真实项目，再启动 Tunnel。
以下章节用于详细解释各个配置区域。

### 可选：后台运行与登录时启动

关闭主窗口只会隐藏 WebCodex Desktop，**不会**退出应用，也不会停止本机 Runtime
和 Tunnel：

- **macOS：**通过菜单栏中的 WebCodex 图标重新打开窗口。
- **Windows：**通过系统托盘中的 WebCodex 图标重新打开窗口。
- 要停止 Runtime，请使用**停止 Desktop 管理的运行环境**；要退出应用并停止 Desktop 管理的
  进程，请使用**退出 WebCodex**。
- 如果希望登录系统后在后台启动 WebCodex，可开启**设置 → 后台与启动 → 登录时
  启动 WebCodex**。

安装完成后的导航、快捷键、活动筛选等操作请看
[Desktop 使用指南](desktop-guide.zh-CN.md)。

## 2. 准备 OpenAI Tunnel

在 OpenAI 平台创建一个 Tunnel，并准备一个可用于该 Tunnel 的 API key：

- [Tunnels - OpenAI API](https://platform.openai.com/settings/organization/tunnels)
- [API keys - OpenAI API](https://platform.openai.com/settings/organization/api-keys)

Tunnel 名称可以自定义；记录自己的 Tunnel ID。API key 建议使用 Restricted key，只授予 Tunnels 所需的 **Read + Use** 权限。

![OpenAI Tunnels 页面](desktop-install/image-20260906171606559.png)

![OpenAI API Keys 页面](desktop-install/image-20260906171633208.png)

不要把真实 API key、WebCodex token 或 authorization 内容提交到 Git、issue、截图或聊天记录中。

## 3. 在 Desktop 内保存 Tunnel 配置（推荐）

打开 **连接 → Tunnel 连接配置**。普通 Tunnel 运行中或停止后，编辑区始终可见：

1. 在 **Tunnel ID** 输入框填写自己的 Tunnel ID。
2. 在 **Tunnel API key** 密码输入框填写可用于该 Tunnel 的 API key。
3. 点击 **保存配置**。看到“当前来源：本机配置文件（优先）”后即可启动连接，**不需要重启 Desktop**。

Desktop 会把 API key **以未加密形式**保存在当前用户的本机应用数据目录中。
不要把它放入项目、Git、工单、截图或共享备份。

**成功时：**配置来源显示为本机文件，两项检测都通过。接下来选择真正要给
ChatGPT 使用的项目。

### 可选：已保存配置的行为与存储位置

这两个字段也可以在首次本机配置的可选 Tunnel 区域填写。已有保存的密钥时，API key 留空表示保留原密钥；界面不会取回密钥值，提交后输入框会清空。保存失败时会保留 Tunnel ID，并要求重新输入尚未保存的密钥。

**优先级：完整的已保存配置 → Desktop 进程继承的环境变量。** 不会混用文件中的 Tunnel ID 和环境中的 API key。保存不会修改系统环境。本应用管理的普通 Tunnel 正在运行时，会使用新配置替换该 Tunnel，不重启 Server 或 Runner；原先停止的 Tunnel 保持停止。OpenAI Quick Share 在下次启动时使用新值。保存成功和连接恢复是两个结果：替换失败时保留新配置，并明确提示重试连接。

配置保存在 Desktop 的本机应用数据目录中，相对路径为 `secrets/tunnel-config.json`：

- macOS：`~/Library/Application Support/dev.webcodex.desktop/secrets/tunnel-config.json`。
- Windows：`%LOCALAPPDATA%\dev.webcodex.desktop\secrets\tunnel-config.json`。

macOS/Unix 写入权限为当前用户读写（`0600`）；Windows 继承本机用户应用数据目录的访问权限。保存采用原子替换，不会为密钥文件保留旧值备份。`secrets` 目录受 WebCodex 现有敏感路径策略保护。普通 `desktop-state.json` 仍只保存非密钥运行状态。

点击 **清除已保存配置，改用环境变量** 会清除保存的一组值，恢复环境变量回退；文件中记录为 `null`。已有文件无效或无法读取时不会自动改用环境变量，请在界面重新保存，或者清除配置。手工编辑文件后需重新启动 Desktop；界面保存无需重启。

### 可选：继续使用环境变量

没有保存配置时，Desktop 使用当前进程继承的：

```text
CONTROL_PLANE_TUNNEL_ID
CONTROL_PLANE_API_KEY
```

无需额外设置 `OPENAI_ADMIN_KEY` 或 `OPENAI_API_KEY`。首次启动 OpenAI Secure Tunnel 时，WebCodex 会自动下载并校验固定版本的 `tunnel-client`；通常不用手动安装。下载失败时检查网络或代理，高级用户可指定 `WEBCODEX_TUNNEL_CLIENT_BIN`。

Windows 用户可以设置当前用户的持久环境变量。macOS 从 Finder / Dock 启动不会读取 `~/.zshrc`；需要从已加载变量的 Terminal 启动应用，或者配置登录会话环境。如果选择这种高级方式，修改变量后须通过托盘 **退出 WebCodex**，再重新启动。关闭窗口只是隐藏，不会更新进程环境。**重新检测配置** 不会执行 shell 启动脚本，也不会读取手工修改的配置文件。

### 可选：macOS 的 Computer Use 权限

首次前台启动且 Desktop 权限不全时会显示应用内说明。请求按钮调用原生 macOS 授权 API；选择稍后继续不会更改权限。后台登录启动不抢焦点。**设置 → Computer Use 权限** 显示实际观测到的 Desktop 权限，支持重新检测和打开系统设置，不根据 Desktop 状态推断实际 Runner 已授权。

如果需要截图、窗口观察、键盘鼠标等能力，请在 **系统设置 → 隐私与安全性** 为实际运行 WebCodex Runner / Desktop 的进程授予相应权限：包括 **屏幕与系统音频录制**，界面控制还需要 **辅助功能**。授权后按系统要求重启相关进程。

## 4. 启动本机运行环境并添加项目

首次启动后，选择 **Local Full Runtime / 在此电脑使用 WebCodex**，再选择真正
要让 ChatGPT 使用的代码仓库目录。WebCodex 只会访问你明确添加的项目，不会
自动获得其他目录或整块磁盘的访问权限。

在首页展开**查看运行诊断**，确认：

- Service：运行中 / Ready；
- Runner：已连接 / Ready；
- Project：Ready，而且显示的是你刚选择的目录。

以后要使用其他代码仓库，可进入**项目**页面，点击**选择其他项目**或
**添加项目**。

**成功时你应该看到：**Service、Runner、Project 同时 Ready，Project 路径与实际目录一致。

**失败时：**点击**重新激活项目**；仍然失败时再查看错误和**活动**详情。不要
通过扩大项目访问范围来绕过错误。

**下一步：**只有这三项都 Ready 后才启动 OpenAI Secure Tunnel。

## 5. 可选：配置 Tunnel 网络

大多数用户保持**设置 → OpenAI Tunnel 网络 → 自动**即可，直接继续第 6 步。
只有当前网络需要代理，或者 Tunnel 启动提示网络错误时才需要修改：

- **自动（推荐）**：优先使用 Desktop 进程继承的代理；Windows 还会检测系统代理。
- **直接连接**：不使用代理。
- **自定义 HTTP 代理**：例如 `http://127.0.0.1:7890`。

如果 Tunnel 已在运行，先停止 Tunnel，修改并保存代理设置，再重新启动 Tunnel。**不需要重启 Desktop**；每次启动 Tunnel 都会重新读取最新代理设置。

如果仍然失败，再查看 Tunnel 错误和**活动**详情。不要为了绕过网络错误而修改
项目访问范围或 Runner 配置。

保存需要的调整后，继续第 6 步。

## 6. 启动官方 OpenAI Secure Tunnel

进入 **连接**，选择 **OpenAI Secure Tunnel**，再点击 **启动安全隧道**。仅选择连接方式不会启动或停止进程。已有隧道报错时，先点击停止，再重新启动；失败后页面会保留错误和重试入口。运行成功后，Desktop 会显示类似：

> OpenAI Secure Tunnel 已就绪，等待 ChatGPT 连接

系统允许时，Desktop 会把 Tunnel ID 复制到剪贴板。

这里**不应该**因为 daemon ready 或 Tunnel ID 已复制就显示“ChatGPT 已连接”或“可以使用”。这些证据只证明 Tunnel **ready for ChatGPT**。

**成功时你应该看到：**Tunnel 本地就绪，同时整体状态仍明确提示等待 ChatGPT/需要最终验证。

**失败时：**先确认第 3 步两项配置都“已检测”，再检查第 5 步代理；按页面动作重新启动安全隧道。

**下一步：**把 Tunnel ID 填到 ChatGPT 网页版。

## 7. 在 ChatGPT 网页版添加连接器

本节中的所有 ChatGPT 配置都在 **ChatGPT 网页版**中完成。本文中的
“Desktop”始终指 **WebCodex Desktop**，不指 ChatGPT 桌面应用。

1. 打开 [ChatGPT 网页版的账户安全设置](https://chatgpt.com/#settings/Security)。
2. 在**账户安全与登录**中开启**开发人员模式**。开启前请阅读 ChatGPT
   显示的风险提示；开发人员模式允许添加可能永久修改或删除数据的连接器。

![在 ChatGPT 网页版开启开发人员模式](desktop-install/chatgpt-enable-developer-mode.zh-CN.png)

3. 打开 [ChatGPT 插件页面](https://chatgpt.com/plugins)。
4. 创建新插件，连接方式选择 **Tunnel**。
5. 在 **Available tunnels** 中选择为 WebCodex 配置的 Tunnel；需要指定 ID
   时，点击 **Use tunnel ID instead**，填入 WebCodex Desktop / OpenAI 中当前的
   Tunnel ID。如果列表可能没有刷新，或者存在名称相近的 Tunnel，请核对完整 ID。
6. **Authentication 选择 No Auth / None / No authentication**。
7. 阅读自定义 MCP Server 的风险提示；只有信任当前 WebCodex 安装时，才勾选
   **I understand and want to continue**。
8. 点击 **Create**。
9. 在 **Add … to ChatGPT** 确认页面点击 **Connect**。

这里不需要 OAuth。WebCodex 会在本机保存 MCP authorization credential，并由 Tunnel client 注入；ChatGPT 网页版不需要看到这份本机凭据。

在 ChatGPT 网页版保存连接器后，回到 WebCodex Desktop。仅仅保存 ChatGPT
网页版的连接器并不会自动把 WebCodex Desktop 的本地 Tunnel 证据升级成
“已连接”；如果当前版本没有稳定的外部 MCP 客户端观测信号，WebCodex
Desktop 会继续保守显示“等待 ChatGPT”。

**成功时你应该看到：**ChatGPT 网页版的连接器保存成功；WebCodex Desktop Tunnel 继续运行。

**失败时：**确认选择或填入的是当前 Tunnel ID，而不是 API key；Authentication
使用 No Auth / None / No authentication。如果 Tunnel 不显示或被拒绝，请确认它已经关联到
目标 ChatGPT workspace，并确认当前 OpenAI Platform 身份对该 Tunnel 具有 **Tunnels: Read + Use**；
Developer Mode 与 Tunnel 权限是两套独立前提。如果只是 Available tunnels 列表可能没有刷新，
请与 OpenAI Tunnels 页面中的完整 ID 对照，或使用 **Use tunnel ID instead**。API key 不应复制到 ChatGPT 网页版。

**下一步：**立即做一次真实项目读取。

![ChatGPT 网页版添加连接器示例](desktop-install/image-20260906174157920.png)

![Tunnel 配置示例](desktop-install/image-20260906174207352.png)

![连接完成示例](desktop-install/image-20260906174215647.png)

## 8. 最小验收

连接后先做**一个最小、真实的项目读取**，例如：“列出 WebCodex 项目，然后列出
我刚选择项目的顶层文件；空目录请报告为空”。这一步成功，就证明基本的
ChatGPT 网页版 → Tunnel → Server → Runner → Project 链路已经打通。

以下检查用于验证更多能力，均为可选：

- 列出 WebCodex 项目。
- 读取一个文件。
- 在明确注册的项目中创建并再读取一个临时文件，然后删除。
- 执行 `git status`、`uname -a` / `ver` 等只读命令。
- 需要 Computer Use 时，尝试列出窗口或截取浏览器窗口。

只执行你确实需要并且理解其影响的检查，特别是会修改文件或使用 Computer Use
的项目。

如果 Desktop 看起来 Service / Runner / Project / Tunnel 都正常，但 ChatGPT 仍无法列项目或读取文件，**不要把本地全绿当作外部连接成功证据**。先检查 ChatGPT 中的 Tunnel ID 和连接配置，再回到 Desktop 的 Connection / Activity 查看最新状态。

## 常见问题

**OpenAI Secure Tunnel 按钮不可用**：查看 Desktop 的“OpenAI Tunnel 配置检测”。缺哪一项会明确显示；点“重新检测配置”只观察当前进程。如果变量刚设置，必须**完全退出 WebCodex 后重新启动**，关闭窗口不算退出。

**macOS `.zshrc` 已配置但 Desktop 仍检测不到**：这是正常的进程环境语义。Finder / Dock 不 source `~/.zshrc`；按上面的 Terminal 或 `launchctl setenv` 方式处理。Desktop 的“重新检测配置”不会执行 shell startup script。

**Tunnel 启动失败或连接 ChatGPT 超时**：优先检查“设置 → OpenAI Tunnel 网络”的代理；修改后停止并重新启动 Tunnel 即可。

**项目目录无法访问**：到“项目”页面显式添加对应目录，不要通过扩大默认安装目录权限来绕过项目边界。

**出现 `project_not_loaded` / 项目尚未就绪**：点击“重新激活项目”。Desktop 会重试同一个项目并只管理自己拥有的 Runner；普通用户不需要理解或手工修改内部 project registry。

**我关了窗口再打开，为什么新环境变量还是识别不到**：关闭窗口默认只是隐藏到菜单栏/托盘，进程并未退出。使用菜单栏/托盘中的**退出 WebCodex**，再重新启动新进程。

## 保留 Shell，单独更新 Runtime

在“设置 → Runtime”检查官方解压目录或原生源码构建，然后明确激活，无需修改应用包。详见 [Runtime 兼容与诊断](DESKTOP_RUNTIME_COMPATIBILITY.zh-CN.md)。所选 Custom 文件缺失时不会悄悄改用内置文件。
