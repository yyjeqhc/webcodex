# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

**WebCodex 让 ChatGPT、Claude 和其他 AI Agent 直接使用你自己机器上的代码仓库和开发工具。**

你可以直接让 AI 理解项目、修改代码、运行测试、检查 Git 或排查问题。仓库仍然留在原来的机器上，不需要为了使用 WebCodex 把整个项目搬到托管环境里。

## 快速开始

Linux、macOS 或 Windows x64 上准备好 Node.js 18+ 和 Git，然后进入一个仓库：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

看到 **WebCodex ready** 后保持终端运行。Linux 与 macOS 通常会自动复制 **MCP URL**，并可在交互终端按 **Enter** 打开 ChatGPT App 设置；Windows 请手动复制终端中打印的 **MCP URL**，并进入 **Settings -> Apps -> Create**。然后：

1. 如有需要，开启 **Developer Mode** 并选择 **Create**。
2. 粘贴已经复制的 **MCP URL**；也可以使用 WebCodex 终端中打印的地址。
3. 认证选择 **Access token / API key**（或等价的 Bearer 令牌选项），再填入输出的临时 **Credential**。
4. 点击 **Scan Tools**。

Developer Mode、自定义 MCP App 和写入/修改操作是否可用取决于 ChatGPT 套餐、workspace 与管理员策略；WebCodex 无法启用客户端未授予的权限。

第一条消息可以先只读检查：

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

如果 ChatGPT 没有显示 Bearer/访问令牌选项，或者自动尝试 OAuth 后出现 **does not implement OAuth**，直接运行：

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

把输出的完整 `/mcp?token=...` 地址粘贴到 ChatGPT，并选择 **No authentication**。这个 fallback 需要 WebCodex 0.3.9 或更新版本。这条地址包含本次临时密钥，不要公开、转发或记录到日志中。如果已经全局安装 WebCodex，也可以使用等价命令 `webcodex share --auth query-token`。更多客户端配置见[快速开始](docs/QUICK_START.zh-CN.md)和 [MCP 接入指南](docs/MCP.zh-CN.md)。

默认的一键流程会创建一个由本次临时凭据保护的公网 HTTPS MCP 地址；关闭命令后，该地址和凭据都会失效。代理网络、OAuth、私有隧道、自托管等高级配置都放在单独的文档中，不是第一次使用的前置条件。

## 能做什么？

- **理解和修改代码** —— 读取、搜索、分析项目，并在配置好的项目范围内进行受保护的修改。
- **使用真实开发环境** —— 在仓库所在机器上运行命令、测试、格式化、编译器和项目自己的工具。
- **检查 Git** —— 查看状态和差异，让代码变化保持可见、可审查。
- **处理长时间任务** —— 任务可以持续运行并保持可观察，不需要一次模型回复一直等待到底。
- **保留人工审查** —— 可以通过运行时控制台和任务流程进行指导、取消、接受或拒绝。

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

## 平台支持

- **Linux x64/arm64** —— 支持本机 `share`、Server 和 Runner 工作流。
- **macOS x64/arm64** —— 支持本机 `share` 和 Runner 工作流。
- **Windows x64/arm64** —— 支持 CLI、Runner、本地前台 Server，以及显式 `webcodex share --tunnel cloudflare|openai|none`。Windows x64 可自动管理固定版本的 Cloudflare Quick Tunnel；OpenAI `tunnel-client` 在 x64/arm64 都支持 managed 获取。固定版本 Cloudflare 没有官方 Windows ARM64 artifact，因此 Windows ARM64 使用 Cloudflare 时需要提供受信任的显式/`PATH` `cloudflared`。WebCodex 托管的 Windows Server service 仍不支持。

Windows 接入和长期部署见[部署指南](docs/DEPLOYMENT.zh-CN.md)与 [MCP](docs/MCP.zh-CN.md)。

## 长期使用与高级配置

希望长期保留命令行工具时可以全局安装：

```bash
npm install -g @yyjeqhc/webcodex
```

对于 operator 提供 shared key 的已有 hosted WebCodex Server，使用 `webcodex connect <server-url>`；新部署的自托管 Server 应把 bootstrap administrator token 留在 Server 侧，并按[部署指南](docs/DEPLOYMENT.zh-CN.md)使用短期 pairing code + `webcodex login` 完成 enrollment。

生产环境、自托管、Windows 接入、OAuth、私有隧道、代理/私有 CA 等运维配置请直接查看下面的专题文档，不需要在第一次体验前理解这些概念。

## 文档

- [快速开始](docs/QUICK_START.zh-CN.md) —— 从零到第一次成功连接 AI
- [MCP](docs/MCP.zh-CN.md) —— ChatGPT、Claude、认证方式和 MCP 参考
- [部署指南](docs/DEPLOYMENT.zh-CN.md) —— 自托管、长期 Server 和机器接入
- [故障排查](docs/TROUBLESHOOTING.zh-CN.md) —— 连接和运行问题
- [CLI](docs/CLI.zh-CN.md) —— 命令与凭据参考
- [AI 辅助接入](docs/AI_ONBOARDING.zh-CN.md) —— 让 AI 帮你配置 WebCodex
- [安全说明](SECURITY.md) —— 安全模型与使用建议
- [文档索引](docs/INDEX.zh-CN.md) —— 全部用户和贡献者文档

## 安全

WebCodex 能在配置的项目范围内读取和修改文件、执行命令。建议使用版本控制，不要把凭据写进提示词、日志或 Git，只注册确实希望 AI 访问的项目目录。完整安全模型见 [SECURITY.md](SECURITY.md)。

## 从源码构建

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## 参与贡献

欢迎提交贡献，也欢迎使用 WebCodex 或其他 coding agent 辅助开发。Bug 报告、开发流程与 PR 说明见 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。

## 致谢

感谢 [LINUX DO](https://linux.do/) 社区提供友好的技术交流与开源分享环境。

## 许可证

使用 Apache License 2.0，见 [LICENSE](LICENSE)。
