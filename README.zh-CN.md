<p align="right"><a href="README.md">English</a> · <a href="README.zh-CN.md">简体中文</a></p>

<p align="center"><img src="docs/assets/brand/webcodex-app-icon.png" alt="WebCodex 标志" width="96" height="96"></p>

<h1 align="center">WebCodex</h1>

<p align="center"><strong>让云端 AI Agent 使用你自己机器上的真实开发环境。</strong></p>
<p align="center">把 ChatGPT、Claude 等 MCP 客户端连接到你已有的仓库、Git 工作区和开发工具。</p>
<p align="center"><a href="#只想先试几分钟临时分享">快速试用</a> · <a href="#下载发行版">下载</a> · <a href="docs/PERSONAL_SETUP.zh-CN.md">完整配置</a> · <a href="#文档">文档</a> · <a href="https://github.com/yyjeqhc/webcodex/issues">Issues</a> · <a href="CONTRIBUTING.zh-CN.md">参与贡献</a> · <a href="SECURITY.md">安全说明</a></p>

<p align="center">
  <a href="docs/MCP.zh-CN.md"><img src="https://img.shields.io/badge/protocol-MCP-2563EB?labelColor=1E40AF&amp;style=flat-square" alt="MCP 协议"></a>
  <a href="docs/QUICK_START.zh-CN.md#前置条件"><img src="https://img.shields.io/badge/Node.js-18%2B-0D9488?labelColor=0F766E&amp;style=flat-square" alt="需要 Node.js 18 或更新版本"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-2563EB?labelColor=1E40AF&amp;style=flat-square" alt="Apache 2.0 许可证"></a>
</p>

你可以直接让 AI 理解项目、修改代码、运行测试、检查 Git 或排查问题。仓库仍然留在原来的机器上，不需要为了使用 WebCodex 把整个项目搬到托管环境里。

<p align="center">
  <a href="https://trendshift.io/repositories/121867">
    <img src="https://trendshift.io/api/badge/repositories/121867" alt="WebCodex | GitHub Trending" width="250" height="55" />
  </a>
</p>

## 开始使用

### 只想先试几分钟：临时分享

如果你只是想快速看看 WebCodex 是否适合自己，可以在一个仓库里运行：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` 会临时启动仅开放单个项目的 WebCodex Adaptive Runtime，并给出 ChatGPT 连接信息。临时 Project Credential 只允许访问对应的 ProjectGrant；关闭命令后连接和凭据都会失效。它适合试用和临时分享，不是日常完整体验的默认部署方式。详细步骤见[快速试用](docs/QUICK_START.zh-CN.md)。

### 日常使用：完整 WebCodex（推荐）

如果你准备让 ChatGPT 长期使用自己的开发环境，推荐从 **普通 Server + Runner** 开始。这是 WebCodex 的完整开发体验：可以长期连接多个项目，并使用项目探索、编辑、Git、命令、测试、长任务和代码导航能力。公网 HTTPS、Cloudflare Tunnel 或 OpenAI Secure MCP Tunnel 只是 ChatGPT 到 Server 的连接方式，不会把你切换到另一套受限体验。

Windows / macOS 普通用户最推荐 **WebCodex Desktop + 官方 OpenAI Secure Tunnel**，直接按 [Desktop 安装与连接指南](docs/desktop-install.zh-CN.md)操作即可；CLI、已有 Server、自托管或高级配置再看[完整使用指南](docs/PERSONAL_SETUP.zh-CN.md)。

### 下载发行版

下表链接到**上游官方 v0.4.1 Release**。Windows/macOS 普通使用可选择 Desktop 安装包；Server/Runner 工作流可选择对应平台的 CLI 压缩包。[查看发行说明与校验值](https://github.com/yyjeqhc/webcodex/releases/tag/v0.4.1)。

| 平台 | Desktop | CLI / Server / Runner |
| --- | --- | --- |
| Windows x64 | [安装包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-win32-x64-setup.exe) | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-win32-x64.tar.gz) |
| Windows arm64 | — | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-win32-arm64.tar.gz) |
| macOS Apple Silicon | [DMG](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-darwin-arm64.dmg) | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-darwin-arm64.tar.gz) |
| macOS Intel | [DMG](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-desktop-v0.4.1-darwin-x64.dmg) | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-darwin-x64.tar.gz) |
| Linux x64 | — | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-linux-x64.tar.gz) |
| Linux arm64 | — | [压缩包](https://github.com/yyjeqhc/webcodex/releases/download/v0.4.1/webcodex-v0.4.1-linux-arm64.tar.gz) |

也可以通过 npm 安装：`npm install -g @yyjeqhc/webcodex`（需要 Node.js 18+）。

## 能做什么？

- **理解和修改代码** —— 读取、搜索、分析项目，并在配置好的项目范围内进行受保护的修改。
- **使用真实开发环境** —— 在仓库所在机器上运行命令、测试、格式化、编译器和项目自己的工具。
- **检查 Git** —— 查看状态和差异，让代码变化保持可见、可审查。
- **处理长时间任务** —— 任务可以持续运行并保持可观察，不需要一次模型回复一直等待到底。
- **保留人工审查** —— 可以通过运行时控制台、任务状态和 Git 差异查看工作结果。

## 为什么用 WebCodex？

- **代码留在自己的机器上。** 不需要把整个仓库上传到聊天服务。
- **AI 使用的是真实开发环境。** 文件、Git、编译器、测试和已有工具链都可以直接复用。
- **工作不局限于一次请求。** 长时间执行、测试结果和相关证据可以继续观察。
- **既能临时使用，也能长期部署。** 可以一条命令快速分享，也可以连接到自托管服务长期使用。

## 工作方式

```text
AI 客户端
   |
   | MCP / HTTPS
   v
WebCodex
   |
   v
你的机器
   |
   +-- 代码仓库
   +-- Git
   +-- 编译器 / 测试 / 开发工具
```

如果需要了解内部的 Server/Runner 架构、协议接口和权限边界，再阅读[架构说明](docs/ARCHITECTURE.md)、[MCP](docs/MCP.zh-CN.md)和[认证模型](docs/AUTH_MODEL.zh-CN.md)。

## Star History

下图展示上游仓库 [yyjeqhc/webcodex](https://github.com/yyjeqhc/webcodex) 的 Star 历史。

<a href="https://www.star-history.com/yyjeqhc/webcodex">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=yyjeqhc/webcodex&amp;type=Date&amp;theme=dark">
    <img alt="yyjeqhc/webcodex 的 Star 历史" src="https://api.star-history.com/svg?repos=yyjeqhc/webcodex&amp;type=Date">
  </picture>
</a>

## 平台支持

以下平台能力以链接的上游官方版本为准。

- **Linux x64/arm64** —— 支持本机 `share`、Server 和 Runner 工作流。
- **macOS x64/arm64** —— 支持 Desktop 本机 Server + Runner、OpenAI Secure Tunnel、本机 `share` 和独立 Runner 工作流。
- **Windows x64** —— 推荐 Desktop 本机 Server + Runner + 官方 OpenAI Secure Tunnel；同时支持 CLI + Runner、本地前台 Server，以及显式 `webcodex share --tunnel cloudflare|openai|none`。
- **Windows arm64** —— native ARM64 构建路径支持 Desktop 本机 Server + Runner 与官方 OpenAI Secure Tunnel，同时也支持 CLI + Runner、本地前台 Server 与 `share`；从 v0.4.2+ 的 Release 构建开始提供 Windows ARM64 Desktop installer。固定版本 Cloudflare 没有官方 Windows ARM64 artifact，因此使用 Cloudflare 时仍需要受信任的显式/`PATH` `cloudflared`。除 Desktop 自己托管的前台 runtime 外，WebCodex-managed Windows Server service 仍不支持。

Windows 接入和长期部署见[部署指南](docs/DEPLOYMENT.zh-CN.md)与 [MCP](docs/MCP.zh-CN.md)。

## 已有 Server 与高级配置

如果已经有人为你提供 WebCodex Server 和接入凭据，直接使用已有 Server 并看[完整使用指南](docs/PERSONAL_SETUP.zh-CN.md)。普通 Windows / macOS 个人安装使用 [Desktop 指南](docs/desktop-install.zh-CN.md)。生产环境、多用户、systemd/Docker、OAuth、代理/私有 CA 等运维内容再查看[部署指南](docs/DEPLOYMENT.zh-CN.md)。

这些是后续配置，不应该成为第一次使用 WebCodex 的概念负担。

## 文档

- [Desktop 安装与连接](docs/desktop-install.zh-CN.md) —— Windows / macOS 推荐路径：Desktop + 官方 OpenAI Secure Tunnel
- [Desktop 日常使用](docs/desktop-guide.zh-CN.md) —— 项目、连接、活动与后台运行
- [Desktop 开发与打包](docs/DESKTOP_DEVELOPMENT.zh-CN.md) —— 从源码运行并在 Windows/macOS 本地构建安装包
- [完整使用指南](docs/PERSONAL_SETUP.zh-CN.md) —— CLI、已有 Server、Linux 与高级普通 Server + Runner 配置
- [快速试用](docs/QUICK_START.zh-CN.md) —— 用 `share` 临时体验一个仓库
- [MCP](docs/MCP.zh-CN.md) —— ChatGPT、Claude、认证方式和 MCP 参考
- [部署指南](docs/DEPLOYMENT.zh-CN.md) —— 生产、自托管和高级运维
- [故障排查](docs/TROUBLESHOOTING.zh-CN.md) —— ChatGPT/MCP Host、连接和运行问题
- [CLI](docs/CLI.zh-CN.md) —— 命令与凭据参考
- [AI 辅助接入](docs/AI_ONBOARDING.zh-CN.md) —— 让 AI 帮你配置 WebCodex
- [安全说明](SECURITY.md) —— 安全模型与使用建议
- [文档索引](docs/INDEX.zh-CN.md) —— 全部用户和贡献者文档

## 安全

WebCodex 能在配置的项目范围内读取和修改文件、执行命令。建议使用版本控制，不要把凭据写进提示词、日志或 Git，只注册确实希望 AI 访问的项目目录。工具返回的结果（包括按请求读取的文件片段）可能传给 AI 客户端。完整安全模型见 [SECURITY.md](SECURITY.md)。

## 从源码构建

如果已发布版本遇到问题，不必只能等待维护者发布新版本。欢迎先在最新 `main`
复现问题，在本地构建并验证 focused fix，然后直接提交 pull request。

CLI / Server / Runner 开发需要先安装 [Git](https://git-scm.com/) 和通过
[rustup](https://rustup.rs/) 安装的 stable Rust toolchain，然后使用日常 dogfood
profile 构建：

```bash
cargo build --locked --profile dogfood --workspace --bins
```

产物位于 `target/dogfood/`。如果修改 Desktop/frontend，还需要 Node.js 22 + npm
以及对应平台的 native toolchain。Windows Desktop 开发需要 MSVC / Windows SDK
环境；macOS 需要 Xcode Command Line Tools。完整 prerequisites、源码运行和打包流程见
[Desktop 开发与打包](docs/DESKTOP_DEVELOPMENT.zh-CN.md)。

本地验证 Desktop installer 时请使用仓库 helper，而不是直接执行 raw Tauri bundle：

```powershell
# Windows：clean 且已提交的源码
.\scripts\build_desktop_windows_local.ps1

# Windows：显式把未提交修改打成 dirty dogfood installer
.\scripts\build_desktop_windows_local.ps1 -AllowDirty
```

```bash
# macOS
bash scripts/build_desktop_macos_local.sh
```

这些属于开发/dogfood 构建，不是正式 Release artifact。

## 参与贡献

欢迎提交 Issue，也非常欢迎 focused pull request。维护者响应时间可能有所变化，因此如果
能够在最新 `main` 复现问题，尤其欢迎直接排查、在本地构建验证修复并提交 PR，而不必
等待维护者先实现。可以使用 WebCodex 本身、Codex、ChatGPT、Claude 或其他 coding
agent 辅助阅读、修改和验证仓库。

Bug 报告需要提供哪些信息、自助修复流程、验证要求与 PR 说明见
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。

## 致谢

感谢 [LINUX DO](https://linux.do/) 社区提供友好的技术交流与开源分享环境。

## 许可证

使用 Apache License 2.0，见 [LICENSE](LICENSE)。

## Desktop Shell 与 Runtime 升级

Desktop 可以保留当前 Shell，并使用单独选择的兼容 Runtime。构建修订与软件版本用于诊断，不是兼容性开关。自行构建、切换与恢复、追踪、诊断报告与更新提示见 [Desktop Runtime 兼容说明](docs/DESKTOP_RUNTIME_COMPATIBILITY.zh-CN.md)。
