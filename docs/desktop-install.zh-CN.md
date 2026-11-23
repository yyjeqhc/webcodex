# WebCodex Desktop 快速安装与 ChatGPT 连接

[English](desktop-install.md) | [简体中文](desktop-install.zh-CN.md)

Desktop 界面支持简体中文、繁體中文（`zh-TW`）、English、日本語、한국어、Deutsch 和 Français。可在设置中切换，选择会保存，活动时间也按所选语言格式显示。繁体中文使用独立翻译；系统托盘菜单跟随所选语言，后台启动时也会恢复上次选择；原始后端诊断仍为英文，文件选择器跟随操作系统语言。

对于普通 Windows / macOS **个人、同一所有者**使用，**最推荐的路径是 WebCodex
Desktop + 官方 OpenAI Secure Tunnel**。WebCodex Desktop 在本机运行 Server 和
Runner；真正向 AI 发送编程请求的界面是 **ChatGPT 网页版**。第一次使用不需要
配置公开的 WebCodex 地址、反向代理或 ChatGPT OAuth。这种个人 Tunnel **不是**
推荐的多人共享部署方式；团队使用请参阅[部署指南](DEPLOYMENT.zh-CN.md)与
[认证与凭据](AUTH_MODEL.zh-CN.md)。

本文覆盖已发布 Desktop 产物路径，包括 v0.4.6 的 Windows x64/arm64 setup `.exe`
与 macOS Intel/Apple Silicon `.dmg`。版本专属命令和 UI 请使用
[v0.4.6 指南](https://github.com/yyjeqhc/webcodex/blob/v0.4.6/docs/desktop-install.zh-CN.md)。
当前开发版的统一 NSIS/`.pkg`/`.deb` 流程见[统一安装](unified-installation.zh-CN.md)
与[部署验收](unified-deployment-validation.md)；其待完成的原生验收与已发布 Desktop
产物是独立边界。

## 完整快速开始

新建**个人、同一所有者**的 WebCodex Desktop / Tunnel 接入前，请准备好：

- 真正要让 ChatGPT 使用的项目目录，建议项目已经使用版本控制；
- 能访问目标 ChatGPT 工作空间的 **Plugins（插件）** 页面；Business 工作空间
  只有 Owner/Admin 可以创建和发布自定义 MCP 插件；
- 能够创建 Tunnel 和 API key 的 OpenAI Platform 访问权限。

**已经有团队集中管理的 Business 工作空间插件？**如果管理员已提供获批插件，
其后端是集中管理的 WebCodex Server，并配置了适当的按用户授权，
**无需执行下面个人 Desktop/Tunnel 快速开始的第 1–7 步**。直接按
[连接已批准的 Business 工作空间插件](#连接已批准的-business-工作空间插件)操作，
再按[最小验收](#8-最小验收)读取有权限的项目。
加入工作空间不会自动连接到你自己的 Desktop，也不会创建独立的 WebCodex 身份。

WebCodex 可以在已注册的项目范围内读取和修改文件、执行命令。只注册确实要
交给 AI 使用的目录，留意工具调用，并且不要把 API key、token 或其他密钥放进
提示词、截图、Git、Issue 或共享日志。

1. **安装并打开 WebCodex Desktop。**从 [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases)
   下载与当前机器架构匹配的 Windows installer 或 DMG。当前 macOS 构建
   未经过 notarization；如果被拦截，进入**系统设置 → 隐私与安全 → 仍要打开**，
   不要全局关闭 Gatekeeper。
2. 全新安装后点击**开始使用**。Desktop 在当前用户下准备本机服务，观察到两个服务就绪后打开连接页。稍后可在**项目**中添加文件夹。其他角色、自定义程序和服务控制位于**设置 → Runtime 与服务**。
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
6. **启动 Tunnel。**进入**连接 → OpenAI Secure Tunnel**，点击**启动**。
   看到“**OpenAI Secure Tunnel 已就绪，等待 ChatGPT 连接**”后再继续。
7. **在 ChatGPT 网页版连接个人 Desktop Tunnel。**先切换到正确的工作空间，打开
   [Plugins（插件）页面](https://chatgpt.com/plugins)。如果你有创建权限（Business 工作空间须为 **Owner/Admin**），点击 **+ → Add custom MCP server**，
   连接方式选 **Tunnel**，选择正确的 Tunnel 或点击 **Use tunnel ID instead**
   输入完整 ID；**Authentication** 选 **No Auth**；仅在信任当前安装时阅读并
   接受自定义 MCP 风险提示，再点击 **Create as a plugin**（旧界面可能是
   **Create**）、审查工具，并按提示连接/安装。当前 Plugins 流程**不要求先开启
   Developer Mode**；只有旧界面明确提示时才按提示操作。Business **普通成员**
   应跳过个人 Tunnel 步骤，改为[连接已批准的团队插件](#连接已批准的-business-工作空间插件)。
   不要把 Tunnel API key 填入 ChatGPT 网页版。如果 Tunnel 列表没有刷新，请核对
   OpenAI Tunnels 页面中的完整 ID。
8. **从 ChatGPT 网页版验证完整链路。**发送：“列出 WebCodex 项目，然后列出
   我有权访问的某个项目的顶层文件；如果目录为空，请明确报告为空。”这次真实项目
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

macOS 签名明确分为三种模式：`self-signed` 是当前 public release 和长期 dogfood 的持久 fallback；Apple 凭据可用时可显式选择 `developer-id`，使用 Developer ID Application 并完成 notarization/stapling；`adhoc` 仅用于一次性 CI/本地验证，不承诺 TCC 升级连续性。必须跨 build/upgrade 长期保留**同一张 self-signed certificate 及其 private key**；同名重建证书也会改变身份。Desktop（`dev.webcodex.desktop`）和 bundled Runner（`dev.webcodex.runner`）都使用不含 binary cdhash 的证书锚定 designated requirement，并在 Tauri nested signing 后验证最终 app/DMG。历史 ad-hoc 授权不会因此自动迁移，首次改用持久身份时可能需要重新授权。Self-signed 不等同于 Apple notarization；如 Gatekeeper 要求，请使用**系统设置 → 隐私与安全 → 仍要打开**，不要全局关闭 Gatekeeper。

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

打开 **连接** 页面并新增 ChatGPT connection。对于持久本机 Environment：

1. 填写 profile 名称和精确的 **Tunnel ID**。
2. 在 write-only 密码输入框填写已授权 API key。
3. 推荐选择 **Run with WebCodex Server (recommended)**；只有明确需要独立服务生命周期时才选择 **Separate Tunnel service (advanced)**。
4. 保存 profile。保存绝不会暗中重启正在工作的 Server。可以继续添加其余 profile，最后仅在 Desktop 提示尚未应用运行时状态时，执行一次显式 **Restart Server**。

Server-owned profile 写入当前 `EnvironmentStore`，只在 Server 启动时装载，绝不会创建 standalone Tunnel 服务。Separate-service profile 保留逐 profile 的 Start、Stop、Restart 控件。普通编辑器不能改变已有 owner；必须先干净停止并卸载旧 owner，再使用显式 ownership-transfer 流程。

Desktop 不会把 API key 读回表单。编辑时留空表示保留该 profile 已保存的 key；替换 key 必须通过当前 profile revision 栅栏。密钥仍以未加密形式保存在 owner-private binding 中，因此必须妥善保护 Environment 数据、支持包、工单、截图和备份。

**成功时：**每张卡片独立显示名称、Tunnel ID、owner 和 readiness；Server-owned profile 另外显示自己的随 Server 启动选择。Server-owned profile 在完成一次显式重启前可能显示 **Restart Server**。保存配置与观察到 Tunnel 已连接是两个不同结果。

### Profile authority 与 legacy 存储

持久 Environment 下，CLI 与 Desktop 都以 `EnvironmentStore` 为 canonical authority：`tunnel.json` 保存 catalog，`server/tunnels/<profile>/webcodex.env` 保存私有 binding。CLI 新增的 profile 会出现在 Desktop Connections 页面；Desktop 保存后，CLI `tunnel-status` 也会看到同一份状态。

历史 Desktop `secrets/tunnel-config.json` 在持久模式下不再是第二套可写 catalog，只作为 fail-closed identity reconciliation 栅栏：profile/Tunnel identity 一致即可共存；identity 缺失或不同会阻止 mutation，而不会静默覆盖、导入或删除任意一侧。authority 转移后不再比较历史 API-key 字节，因为 key rotation 只属于带 revision 栅栏的 EnvironmentStore binding。

Legacy/non-persistent Desktop runtime 继续使用本机应用数据目录内的 `secrets/tunnel-config.json`：

- macOS：`~/Library/Application Support/dev.webcodex.desktop/secrets/tunnel-config.json`。
- Windows：`%LOCALAPPDATA%\dev.webcodex.desktop\secrets\tunnel-config.json`。

macOS/Unix 写入权限为 owner-only（`0600`）；Windows 继承当前用户应用数据目录权限。保存使用带原内容检查的原子替换，不保留旧 secret 备份；无效、不可读、symlink 或超限文件都会 fail closed。

### 可选：仅在 legacy 模式继续使用环境变量

没有保存 legacy/non-persistent 配置时，Desktop 可使用当前进程继承的：

```text
CONTROL_PLANE_TUNNEL_ID
CONTROL_PLANE_API_KEY
```

无需额外设置 `OPENAI_ADMIN_KEY` 或 `OPENAI_API_KEY`。环境变量不是持久 multi-profile 格式，也不会与已保存数据按字段混用。OpenAI Secure Tunnel 使用内置 Rust client，无需下载或安装额外 tunnel-client。启动失败时请检查连接健康状态和 control-plane 路由；若 WebCodex 提示上次工作未确认，先处理该状态，再恢复同一个 Tunnel identity。

Windows 用户可以设置当前用户的持久环境变量。macOS 从 Finder / Dock 启动不会读取 `~/.zshrc`；需要从已加载变量的 Terminal 启动应用，或者配置登录会话环境。如果选择这种高级方式，修改变量后须通过托盘 **退出 WebCodex**，再重新启动。关闭窗口只是隐藏，不会更新进程环境。**重新检测配置** 不会执行 shell 启动脚本，也不会读取手工修改的配置文件。

### 可选：macOS 的 Computer Use 权限

首次前台启动且 Desktop 权限不全时会显示应用内说明。请求按钮调用原生 macOS 授权 API；选择稍后继续不会更改权限。后台登录启动不抢焦点。**设置 → Computer Use 权限** 显示实际观测到的 Desktop 权限，支持重新检测和打开系统设置，不根据 Desktop 状态推断实际 Runner 已授权。

如果需要截图、窗口观察、键盘鼠标等能力，请在 **系统设置 → 隐私与安全性** 为实际运行 WebCodex Runner / Desktop 的进程授予相应权限：包括 **屏幕与系统音频录制**，界面控制还需要 **辅助功能**。授权后按系统要求重启相关进程。

## 4. 启动本机运行环境并添加项目

全新安装后点击**开始使用**。Desktop 在当前用户下准备本机服务，观察到两个服务就绪后打开连接页。稍后可在**项目**中添加文件夹。其他角色、自定义程序和服务控制位于**设置 → Runtime 与服务**。 [Windows 实机验收清单](desktop-windows-acceptance.zh-CN.md)

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

连接配置中的**自动启动**控制重开 Desktop 时是否恢复该连接。使用桌面管理的本机 Runtime 时，Runtime 就绪后会恢复已启用且允许自动启动的连接；这也适用于 Runtime 本身关闭了自动启动、但已经在运行的情况。点击**停止**会停用该连接，重开不会再次启动它。持久环境中的 Tunnel 由系统服务管理，打开 Desktop 会读取实际服务状态，不会重启已停止的服务。

进入 **连接**，选择 **OpenAI Secure Tunnel**，再点击 **启动**。仅选择连接方式不会启动或停止进程。已有隧道报错时，先点击停止，再重新启动；失败后页面会保留错误和重试入口。运行成功后，Desktop 会显示类似：

> OpenAI Secure Tunnel 已就绪，等待 ChatGPT 连接

系统允许时，Desktop 会把 Tunnel ID 复制到剪贴板。

这里**不应该**因为 daemon ready 或 Tunnel ID 已复制就显示“ChatGPT 已连接”或“可以使用”。这些证据只证明 Tunnel **ready for ChatGPT**。

**成功时你应该看到：**Tunnel 本地就绪，同时整体状态仍明确提示等待 ChatGPT/需要最终验证。

**失败时：**先确认第 3 步两项配置都“已检测”，再检查第 5 步代理；按页面动作重新启动安全隧道。

**下一步：**把 Tunnel ID 填到 ChatGPT 网页版。

## 7. 在 ChatGPT 网页版添加连接器

本节中的所有 ChatGPT 配置都在 **ChatGPT 网页版**中完成。本文中的
“Desktop”始终指 **WebCodex Desktop**，不指 ChatGPT 桌面应用。

### ChatGPT Business（原 Team）工作空间接入前提

ChatGPT Team 已于 2025 年 8 月 29 日更名为 **ChatGPT Business**，原有 Team
工作空间无需迁移。先在 ChatGPT 网页版切换到要使用的 **Business 工作空间**，
不要误用个人工作空间。Business 订阅**不包含** OpenAI API Platform 用量，也不会
自动提供 Tunnel 凭据；第 2–3 节使用的 Tunnel ID 和 API key 需要在独立的
OpenAI Platform 环境中配置。参见 OpenAI 官方[更名说明](https://help.openai.com/en/articles/12111915-chatgpt-team-is-now-chatgpt-business)
和 [API 计费说明](https://help.openai.com/zh-hans-cn/articles/9039756-managing-billing-for-chatgpt-and-the-api-platform)。

**在开放插件给同事前，先区分 WebCodex 部署方式：**

- **个人 Desktop + OpenAI Secure Tunnel（同一所有者）：**Tunnel 使用本机
  Server 的 `WEBCODEX_TOKEN` bootstrap 凭据转发 MCP 请求，该凭据具有
  **admin 管理员权限**。ChatGPT 选择 **No Auth** 只表示 ChatGPT 不另行提供
  WebCodex 凭据，并不代表匿名调用或每位成员独立认证。如果把这个插件发布给
  其他 Business 成员，获准使用插件的人会以**同一个 WebCodex 身份**访问该
  Server 及其配置的 Runners、已注册项目和工具权限（仍受 Runner/项目边界及
  其他运行时检查约束）。成员不会因此连接各自的 Desktop，也不会获得独立的
  WebCodex scopes。**不要把个人 Desktop Tunnel 当作常规团队共享方案发布。**
- **团队/多人部署：**单独部署**集中管理的 Server 及其 Runners**，为各成员配置
  **适当权限范围的独立或委托凭据**（例如所选 MCP 连接方式支持的 managed-user
  OAuth 或 PAT），并选择兼容的 ChatGPT 认证方式。仅发布 **Tunnel + No Auth**
  插件不会自动实现 WebCodex 按用户授权。不得向成员分发 Server bootstrap 凭据。
  详见[认证与凭据](AUTH_MODEL.zh-CN.md)和[部署指南](DEPLOYMENT.zh-CN.md)。

**ChatGPT Business 工作空间角色：**

- **工作空间所有者/管理员：**Business 工作空间仅 Owner/Admin 能创建和发布
  自定义 MCP 插件。先在**工作空间设置 → 成员**确认角色。个人使用时应将 Desktop
  Tunnel 保持为仅自己使用；团队使用时先部署集中管理的 Server 并设置按用户授权，
  审查其工具（尤其是写入操作），再通过工作空间管理设置批准/发布**团队插件**。
- **普通成员：**请管理员提供获批的**集中管理团队插件**。切换到 Business
  工作空间，在 **Plugins（插件）**中找到并按管理员配置的认证方式连接/安装。
  成员不能自行创建、发布 Business 工作空间插件或绕过管理员限制。不要索取
  或分享所有者的 Tunnel API key、Server bootstrap token 或本地凭据。

Business 的应用可用性主要在工作空间级管理；Enterprise/Edu 另有按角色控制
应用访问的功能。ChatGPT 侧插件是否可见或可用，**不等于** WebCodex 后端的
按用户授权。当前创建/发布规则请以 OpenAI 的
[自定义 MCP 服务器说明](https://help.openai.com/zh-hans-cn/articles/12584461-connecting-custom-mcp-servers-in-chatgpt)
为准。

### 连接已批准的 Business 工作空间插件

如果管理员已发布由独立 Server 支撑的团队 WebCodex 插件，
**无需执行前面的个人 Desktop/Tunnel 设置步骤**：

1. 在 ChatGPT 网页版切换到目标 **Business 工作空间**，打开
   [Plugins（插件）页面](https://chatgpt.com/plugins)。
2. 找到工作空间管理员发布/批准的 WebCodex 插件。如果找不到或被禁用，
   请联系管理员开放权限；不要用别人的 Desktop Tunnel 绕过限制。
3. 按页面提示选择 **Connect/Install**，完成团队配置的认证（例如支持时使用
   managed-user OAuth 或获授权的 PAT）。不得输入 Server bootstrap 凭据或借用他人账号。
4. 按[最小验收](#8-最小验收)读取自己在 WebCodex 中有权访问的项目。

此路径使用团队的 Server 和 Runners；ChatGPT 工作空间允许使用插件，
不等于已获得 WebCodex 项目权限。

### 创建个人 Desktop Tunnel 插件

只有完成个人 Desktop/Tunnel 的第 1–6 步且拥有插件创建权限的用户，
才执行下面的 **Tunnel + No Auth** 操作；这**不会**为团队成员建立独立授权。

1. 在正确的工作空间打开 [ChatGPT 插件页面](https://chatgpt.com/plugins)。
2. 点击 **+ → Add custom MCP server**（旧版界面可能显示 **Create a new plugin**）。
   当前 Plugins 流程**无需事先开启 Developer Mode**；如果旧界面明确要求，
   请按页面提示阅读风险说明后操作。Developer Mode 不会赋予普通成员管理员权限。
3. 连接方式选择 **Tunnel**。
4. 在 **Available tunnels** 中选择为 WebCodex 配置的 Tunnel；需要指定 ID
   时，点击 **Use tunnel ID instead**，填入 WebCodex Desktop / OpenAI 中当前的
   Tunnel ID。如果列表可能没有刷新，或者存在名称相近的 Tunnel，请核对完整 ID。
5. **Authentication 选择 No Auth / None / No authentication**。
6. 阅读自定义 MCP Server 的风险提示；只有信任当前 WebCodex 安装时，才勾选
   **I understand and want to continue**。
7. 点击 **Create as a plugin**（旧版界面可能是 **Create**），并审查发现的工具。
8. 如果出现 **Add … to ChatGPT** 确认页面，点击 **Connect**。
9. **个人 Desktop Tunnel 插件仅供其所有者使用。**不要把它直接发布为 Business
   工作空间共享插件。如需团队使用，应先配置独立的**集中管理团队连接**和上述
   委托认证，再发布面向团队的插件。

**仅对于这个个人 Tunnel 接入方式**，ChatGPT 侧无需配置 OAuth：WebCodex 将 bootstrap MCP 凭据保存在本机并由 Tunnel client 注入。ChatGPT 网页版不需要看到这份本机凭据，但这**不会**为其他成员创建不同的 WebCodex 身份。团队连接必须另行采用合适的认证和权限模型。

在 ChatGPT 网页版保存连接器后，回到 WebCodex Desktop。仅仅保存 ChatGPT
网页版的连接器并不会自动把 WebCodex Desktop 的本地 Tunnel 证据升级成
“已连接”；如果当前版本没有稳定的外部 MCP 客户端观测信号，WebCodex
Desktop 会继续保守显示“等待 ChatGPT”。

**成功时你应该看到：**ChatGPT 网页版的连接器保存成功；WebCodex Desktop Tunnel 继续运行。

**失败时：**确认选择或填入的是当前 Tunnel ID，而不是 API key；Authentication
使用 No Auth / None / No authentication。如果 Tunnel 不显示或被拒绝，请确认它已经关联到
目标 ChatGPT workspace，并确认当前 OpenAI Platform 身份对该 Tunnel 具有 **Tunnels: Read + Use**；
Business 工作空间的创建权限与 Tunnel 权限是两个独立前提。如果只是 Available tunnels 列表可能没有刷新，
请与 OpenAI Tunnels 页面中的完整 ID 对照，或使用 **Use tunnel ID instead**。API key 不应复制到 ChatGPT 网页版。

**下一步：**立即做一次真实项目读取。

![ChatGPT 网页版添加连接器示例](desktop-install/image-20260906174157920.png)

![Tunnel 配置示例](desktop-install/image-20260906174207352.png)

![连接完成示例](desktop-install/image-20260906174215647.png)

## 8. 最小验收

连接后先做**一个最小、真实的项目读取**，例如：“列出 WebCodex 项目，然后列出
我有权访问的某个项目的顶层文件；空目录请报告为空”。这一步成功，说明当前调用者的
ChatGPT 网页版 → 已选 MCP 连接（个人 Tunnel 或团队连接）→ Server → Runner →
Project 权限链路已经打通，并不代表其他工作空间成员也能访问。

以下检查用于验证更多能力，均为可选：

- 列出 WebCodex 项目。
- 读取一个文件。
- 在明确注册的项目中创建并再读取一个临时文件，然后删除。
- 执行 `git status`、`uname -a` / `ver` 等只读命令。
- 需要 Computer Use 时，尝试列出窗口或截取浏览器窗口。

只执行你确实需要并且理解其影响的检查，特别是会修改文件或使用 Computer Use
的项目。

个人 Desktop Tunnel 场景下，如果 Desktop 看起来 Service / Runner / Project / Tunnel 都正常，但 ChatGPT 仍无法列项目或读取文件，**不要把本地全绿当作外部连接成功证据**。先检查 ChatGPT 中的 Tunnel ID 和连接配置，再回到 Desktop 的 Connection / Activity 查看最新状态。

## 常见问题

**Business 工作空间无法创建自定义 MCP 插件**：先确认已切换到正确的 Business 工作空间，且当前账号是 Owner/Admin。普通成员须由所有者/管理员创建并发布插件；开启 Developer Mode 不能绕过角色权限。

**Business 成员找不到或无法使用 WebCodex**：请管理员检查集中管理的团队插件是否已在工作空间发布/批准、是否在 **Plugins（插件）**中可用，以及成员是否具有所需的 WebCodex 后端授权。看到 **Disabled by admin（已被管理员禁用）**时应联系管理员；不要借用他人的个人 Desktop Tunnel 或共享凭据绕过限制。

**共享插件意外暴露了某个人 Desktop 上的项目**：个人 Tunnel 对转发请求使用所有者的 bootstrap/admin 身份。应停止共享该连接，检查 Server/Runner 的项目访问范围，并改用带独立委托凭据的团队 Server；仅调整 ChatGPT 插件可用性不能隔离 WebCodex 权限。

**OpenAI Secure Tunnel 按钮不可用**：查看 Desktop 的“OpenAI Tunnel 配置检测”。缺哪一项会明确显示；点“重新检测配置”只观察当前进程。如果变量刚设置，必须**完全退出 WebCodex 后重新启动**，关闭窗口不算退出。

**macOS `.zshrc` 已配置但 Desktop 仍检测不到**：这是正常的进程环境语义。Finder / Dock 不 source `~/.zshrc`；按上面的 Terminal 或 `launchctl setenv` 方式处理。Desktop 的“重新检测配置”不会执行 shell startup script。

**Tunnel 启动失败或连接 ChatGPT 超时**：优先检查“设置 → OpenAI Tunnel 网络”的代理；修改后停止并重新启动 Tunnel 即可。

**项目目录无法访问**：到“项目”页面显式添加对应目录，不要通过扩大默认安装目录权限来绕过项目边界。

**出现 `project_not_loaded` / 项目尚未就绪**：点击“重新激活项目”。Desktop 会重试同一个项目并只管理自己拥有的 Runner；普通用户不需要理解或手工修改内部 project registry。

**我关了窗口再打开，为什么新环境变量还是识别不到**：关闭窗口默认只是隐藏到菜单栏/托盘，进程并未退出。使用菜单栏/托盘中的**退出 WebCodex**，再重新启动新进程。

## 保留 Shell，单独更新 Runtime

在“设置 → Runtime”检查官方解压目录或原生源码构建，然后明确激活，无需修改应用包。详见 [Runtime 兼容与诊断](DESKTOP_RUNTIME_COMPATIBILITY.zh-CN.md)。所选 Custom 文件缺失时不会悄悄改用内置文件。

> 本文详细说明现有 Release 的 Desktop + OpenAI Tunnel 流程。统一安装包仍在开发中，请看[统一安装指南](unified-installation.zh-CN.md)；新包尚未发布，也未完成所有平台的真实机器验收。
