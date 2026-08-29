# @yyjeqhc/webcodex

[English](#english) | [简体中文](#简体中文)

## English

**Connect ChatGPT, Claude, and other AI agents to repositories and developer tools on your own machines.**

WebCodex lets an AI inspect and edit code, run tests and commands, use Git, and work with the development environment that already exists beside your repository.

### Fastest first connection

Requires Node.js 18+ and Git. On Linux, macOS, or Windows x64:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

When WebCodex reports **WebCodex ready**, keep the terminal open. Linux and macOS normally copy the **MCP URL** to your clipboard and can use **Enter** in an interactive terminal to open ChatGPT App settings. On Windows, copy the printed **MCP URL** manually and open **Settings -> Apps -> Create**. Then:

1. Enable **Developer Mode** if needed and choose **Create**.
2. Paste the copied **MCP URL** (or use the URL printed by WebCodex).
3. Choose **Access token / API key** (Bearer token) and paste the printed **Credential**.
4. Run **Scan Tools**.

The default one-command flow creates a temporary public HTTPS MCP endpoint protected by that run's temporary credential; both the endpoint and credential stop working when the command exits. Developer Mode, custom MCP Apps, and write/modify actions depend on your ChatGPT plan, workspace, and administrator policy; WebCodex cannot enable capabilities the client does not grant.

Try:

```text
Inspect this repository and summarize its structure. Do not make changes.
```

If ChatGPT does not show a Bearer/access-token option, or reports **does not implement OAuth**, run:

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

Paste the complete `/mcp?token=...` URL and choose **No authentication**. This fallback requires WebCodex 0.3.9 or later. Treat the complete URL as a temporary secret. If the package is installed globally, `webcodex share --auth query-token` is equivalent.

### Platforms

- Linux x64/arm64: local one-command `share`, Server, and Runner workflows.
- macOS x64/arm64: local one-command `share` and Runner workflows.
- Windows x64/arm64: CLI + Runner, foreground Server, and explicit local `share` with `cloudflare`, `openai`, or `none`. Windows x64 auto-manages the pinned Cloudflare binary; managed OpenAI `tunnel-client` supports x64 and arm64. The pinned Cloudflare release has no official Windows ARM64 artifact, so ARM64 Cloudflare use requires a trusted explicit/PATH binary. Managed Windows Server services remain unsupported.

For Windows, permanent/self-hosted setup, OAuth, private tunnels, proxy settings, and troubleshooting, use the [WebCodex documentation](https://github.com/yyjeqhc/webcodex/tree/main/docs). The [Quick Start](https://github.com/yyjeqhc/webcodex/blob/main/docs/QUICK_START.md) stays focused on the first successful connection.

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

### 最快的第一次连接

需要 Node.js 18+ 和 Git。Linux、macOS 或 Windows x64 上进入仓库：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

看到 **WebCodex ready** 后保持终端运行。Linux 与 macOS 通常会自动复制 **MCP URL**，并可在交互终端按 **Enter** 打开 ChatGPT App 设置；Windows 请手动复制终端中打印的 **MCP URL**，并进入 **Settings -> Apps -> Create**。然后：

1. 如有需要，开启 **Developer Mode** 并选择 **Create**。
2. 粘贴已经复制的 **MCP URL**；也可以使用 WebCodex 终端中打印的地址。
3. 认证选择 **Access token / API key**（Bearer 令牌），并填入输出的临时 **Credential**。
4. 点击 **Scan Tools**。

默认的一键流程会创建一个由本次临时凭据保护的公网 HTTPS MCP 地址；关闭命令后，该地址和凭据都会失效。Developer Mode、自定义 MCP App 和写入/修改操作是否可用取决于 ChatGPT 套餐、workspace 与管理员策略；WebCodex 无法启用客户端未授予的权限。

第一条可以先只读检查：

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

如果 ChatGPT 没有 Bearer/访问令牌选项，或者出现 **does not implement OAuth**，运行：

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

粘贴完整的 `/mcp?token=...` 地址并选择 **No authentication**。这个 fallback 需要 WebCodex 0.3.9 或更新版本。完整地址包含本次临时密钥，请按敏感信息处理。如果已经全局安装，也可以使用 `webcodex share --auth query-token`。

### 平台支持

- Linux x64/arm64：支持本机一键 `share`、Server 和 Runner 工作流。
- macOS x64/arm64：支持本机一键 `share` 和 Runner 工作流。
- Windows x64/arm64：支持 CLI、Runner、前台 Server，以及显式本机 `share --tunnel cloudflare|openai|none`。Windows x64 可自动管理固定版本 Cloudflare；OpenAI `tunnel-client` 的 managed 获取支持 x64/arm64。固定版本 Cloudflare 没有官方 Windows ARM64 artifact，因此 ARM64 使用 Cloudflare 时需要受信任的显式/`PATH` binary。WebCodex 托管的 Windows Server service 仍不支持。

Windows、长期/自托管部署、OAuth、私有隧道、代理配置和故障排查见 [WebCodex 文档](https://github.com/yyjeqhc/webcodex/tree/main/docs)。[快速开始](https://github.com/yyjeqhc/webcodex/blob/main/docs/QUICK_START.zh-CN.md)只保留第一次成功连接所需的步骤。

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
