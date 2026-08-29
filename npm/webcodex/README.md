# @yyjeqhc/webcodex

[English](#english) | [简体中文](#简体中文)

## English

**Connect ChatGPT, Claude, and other AI agents to repositories and developer tools on your own machines.**

WebCodex lets an AI inspect and edit code, run tests and commands, use Git, and work with the development environment that already exists beside your repository.

### Recommended: full everyday setup

For normal daily use, install WebCodex and run a regular Server + Runner. This gives ChatGPT the full coding runtime against your real projects; public HTTPS, Cloudflare, and OpenAI Tunnel are reachability choices rather than separate permission modes.

```bash
npm install -g @yyjeqhc/webcodex
```

Follow the [Full Setup guide](https://github.com/yyjeqhc/webcodex/blob/main/docs/PERSONAL_SETUP.md) for Server startup, one-time login, project selection, Runner startup, and ChatGPT connection.

### Quick temporary trial

If you only want to try one repository for a few minutes, use:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` is a temporary, single-project, more restricted experience. The endpoint and temporary credential stop working when the command exits. See the [Quick Trial](https://github.com/yyjeqhc/webcodex/blob/main/docs/QUICK_START.md) for the ChatGPT steps and authentication fallback.

### Platforms

- Linux x64/arm64: local one-command `share`, Server, and Runner workflows.
- macOS x64/arm64: local one-command `share` and Runner workflows.
- Windows x64/arm64: CLI + Runner, foreground Server, and explicit local `share` with `cloudflare`, `openai`, or `none`. Windows x64 auto-manages the pinned Cloudflare binary; managed OpenAI `tunnel-client` supports x64 and arm64. The pinned Cloudflare release has no official Windows ARM64 artifact, so ARM64 Cloudflare use requires a trusted explicit/PATH binary. Managed Windows Server services remain unsupported.

For normal Windows/Linux use, start with the [Full Setup guide](https://github.com/yyjeqhc/webcodex/blob/main/docs/PERSONAL_SETUP.md). Production hosting, OAuth, private-network details, proxy settings, and troubleshooting are available in the [WebCodex documentation](https://github.com/yyjeqhc/webcodex/tree/main/docs).

### Install globally

```bash
npm install -g @yyjeqhc/webcodex
```

The package exposes the `webcodex` command. WebCodex downloads and verifies the matching native release artifacts for supported platforms and replaces the installed binary set atomically; a failed download, extraction, checksum, or identity check does not replace the previous complete installation.

### Security

WebCodex can read and modify files and execute commands inside configured project boundaries. Use version control and keep credentials out of prompts, logs, and Git. See the repository [security guidance](https://github.com/yyjeqhc/webcodex/blob/main/SECURITY.md).

## 简体中文

**把 ChatGPT、Claude 和其他 AI Agent 连接到你自己机器上的代码仓库和开发工具。**

WebCodex 可以让 AI 理解和修改代码、运行测试与命令、检查 Git，并直接使用仓库旁边已经存在的真实开发环境。

### 推荐：日常完整使用

如果准备正常长期使用 WebCodex，推荐安装后运行普通 Server + Runner。这样 ChatGPT 可以使用真实项目上的完整 coding runtime；公网 HTTPS、Cloudflare、OpenAI Tunnel 都只是网络连接方式，不是不同的权限模式。

```bash
npm install -g @yyjeqhc/webcodex
```

按照[完整使用指南](https://github.com/yyjeqhc/webcodex/blob/main/docs/PERSONAL_SETUP.zh-CN.md)完成 Server 启动、一次性登录、项目选择、Runner 启动和 ChatGPT 连接。

### 快速临时试用

如果只想用几分钟体验一个仓库，可以运行：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

`share` 是临时、单项目、能力更受限的体验；命令退出后地址和临时凭据都会失效。ChatGPT 配置和认证 fallback 见[快速试用](https://github.com/yyjeqhc/webcodex/blob/main/docs/QUICK_START.zh-CN.md)。

### 平台支持

- Linux x64/arm64：支持本机一键 `share`、Server 和 Runner 工作流。
- macOS x64/arm64：支持本机一键 `share` 和 Runner 工作流。
- Windows x64/arm64：支持 CLI、Runner、前台 Server，以及显式本机 `share --tunnel cloudflare|openai|none`。Windows x64 可自动管理固定版本 Cloudflare；OpenAI `tunnel-client` 的 managed 获取支持 x64/arm64。固定版本 Cloudflare 没有官方 Windows ARM64 artifact，因此 ARM64 使用 Cloudflare 时需要受信任的显式/`PATH` binary。WebCodex 托管的 Windows Server service 仍不支持。

Windows/Linux 普通使用先看[完整使用指南](https://github.com/yyjeqhc/webcodex/blob/main/docs/PERSONAL_SETUP.zh-CN.md)。生产部署、OAuth、私有网络细节、代理配置和故障排查再查看 [WebCodex 文档](https://github.com/yyjeqhc/webcodex/tree/main/docs)。

### 全局安装

```bash
npm install -g @yyjeqhc/webcodex
```

npm 包对外提供 `webcodex` 命令。安装时会下载并校验当前平台对应的原生发布产物，再原子替换完整的程序文件；下载、解压、校验或版本一致性检查失败时，不会破坏上一份完整安装。

### 安全

WebCodex 能在配置的项目范围内读取和修改文件、执行命令。建议使用版本控制，不要把凭据写进提示词、日志或 Git。完整说明见仓库的[安全文档](https://github.com/yyjeqhc/webcodex/blob/main/SECURITY.md)。

## Development verification / 开发验证

```bash
npm --prefix npm/webcodex test
```

## License

Apache-2.0. See the repository `LICENSE` file.
