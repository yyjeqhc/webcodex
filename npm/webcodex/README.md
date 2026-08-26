# @yyjeqhc/webcodex

[English](#english) | [简体中文](#简体中文)

## English

**Connect ChatGPT, Claude, and other AI agents to repositories and developer tools on your own machines.**

WebCodex lets an AI inspect and edit code, run tests and commands, use Git, and work with the development environment that already exists beside your repository.

### Fastest first connection

Requires Node.js 18+ and Git. On Linux or macOS:

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

When it reports **WebCodex ready**, keep the terminal open and add it to ChatGPT:

1. Enable **Developer Mode** and open **Settings -> Apps -> Create**.
2. Paste the printed **MCP URL**.
3. Choose **Access token / API key** (Bearer token) and paste the printed **Credential**.
4. Run **Scan Tools**.

Try:

```text
Inspect this repository and summarize its structure. Do not make changes.
```

If ChatGPT does not show a Bearer/access-token option, or reports **does not implement OAuth**, run:

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

Paste the complete `/mcp?token=...` URL and choose **No authentication**. Treat the complete URL as a temporary secret. If the package is installed globally, `webcodex share --auth query-token` is equivalent.

### Platforms

- Linux x64/arm64: local one-command `share`, Server, and Runner workflows.
- macOS arm64: local one-command `share` and Runner workflows.
- Windows x64/arm64: CLI + Runner against a remote Linux Server; local `share` is not supported in this release.

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

需要 Node.js 18+ 和 Git。Linux 或 macOS 上进入仓库：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

看到 **WebCodex ready** 后保持终端运行，然后在 ChatGPT 中：

1. 开启 **Developer Mode**，进入 **Settings -> Apps -> Create**。
2. 粘贴输出的 **MCP URL**。
3. 认证选择 **Access token / API key**（Bearer 令牌），并填入输出的临时 **Credential**。
4. 点击 **Scan Tools**。

第一条可以先只读检查：

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

如果 ChatGPT 没有 Bearer/访问令牌选项，或者出现 **does not implement OAuth**，运行：

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

粘贴完整的 `/mcp?token=...` 地址并选择 **No authentication**。完整地址包含本次临时密钥，请按敏感信息处理。如果已经全局安装，也可以使用 `webcodex share --auth query-token`。

### 平台支持

- Linux x64/arm64：支持本机一键 `share`、Server 和 Runner 工作流。
- macOS arm64：支持本机一键 `share` 和 Runner 工作流。
- Windows x64/arm64：支持 CLI 和 Runner，连接远程 Linux Server；本版本不支持 Windows 本机 `share`。

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
