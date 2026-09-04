# 快速试用

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这个页面只做一件事：用 `webcodex share` 临时分享一个本地仓库，让你几分钟内判断 WebCodex 是否适合自己。这个模式是一条命令的**临时、单项目、受限体验**；关闭命令后连接会结束。

如果你准备日常使用 WebCodex，并希望获得普通 Server + Runner 的完整 coding 能力，请直接使用[完整使用指南](PERSONAL_SETUP.zh-CN.md)，不要把 `share` 当作长期默认部署。

## 前置条件

- Node.js 18 或更新版本。
- Git，以及一个可以让 AI 安全查看的代码仓库。
- Linux、macOS 或 Windows x64 可直接使用完整 managed Cloudflare 本机 `share` 流程。

Windows 已支持显式本机 `webcodex share`。固定版本 Cloudflare 没有官方 Windows ARM64 binary，因此 Windows ARM64 使用 `--tunnel cloudflare` 时需要通过 `WEBCODEX_CLOUDFLARED_BIN`/`PATH` 提供受信任 binary；managed OpenAI `tunnel-client` 与 `--tunnel none` 仍可用。

## 1. 运行 WebCodex

进入希望 AI 使用的仓库：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex share
```

临时试用不需要提前运行 `setup`、`doctor` 或 `run`。这一键流程会创建一个由本次临时凭据保护的公网 HTTPS MCP 地址；关闭命令后，该地址和凭据都会失效。

## 2. 等待 `WebCodex ready`

看到 **WebCodex ready** 后保持终端运行。WebCodex 会打印 MCP 客户端需要的配置。Linux 与 macOS 通常会自动复制 MCP URL，并可在交互终端按 **Enter** 打开 ChatGPT App 设置；Windows 请手动复制终端中打印的 MCP URL，并进入 **Settings -> Apps -> Create**。

## 3. 在 ChatGPT 中添加 WebCodex

1. 如有需要，在 ChatGPT App 设置中开启 **Developer Mode** 并选择 **Create**。
2. 粘贴已经复制的 **MCP URL**；也可以使用 WebCodex 终端中打印的地址。
3. 默认分享方式选择 **Access token / API key** 或等价的 Bearer 令牌选项。
4. 填入输出的临时 **Credential**。
5. 点击 **Scan Tools**。

不同账号或工作区看到的 ChatGPT 文案可能略有区别。Developer Mode、自定义 MCP App 和写入/修改操作是否可用也取决于 ChatGPT 套餐、workspace 与管理员策略；WebCodex 无法启用客户端未授予的权限。具体 URL 和认证值以 WebCodex 终端输出为准。

### 如果找不到 Bearer/访问令牌选项

如果 ChatGPT 自动尝试 OAuth 并出现 **does not implement OAuth**，或者当前客户端根本没有 Bearer 令牌输入框，先结束当前分享，再运行：

```bash
npx --yes @yyjeqhc/webcodex share --auth query-token
```

把输出的完整 `/mcp?token=...` 地址粘贴进去，认证选择 **No authentication**，然后再次点击 **Scan Tools**。完整地址包含本次临时密钥，不要公开或写入日志。如果已经全局安装 WebCodex，等价命令是 `webcodex share --auth query-token`。

## 4. 先试一个只读请求

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

能正常得到回答，就说明 ChatGPT 已经能够通过 WebCodex 访问目标仓库。

## 5. 再做一个小修改

确认只读请求正常后，可以尝试一个容易审查的小任务：

```text
修复这个仓库里的一个小问题，并运行相关测试。告诉我具体改了什么。
```

接受结果之前，用 Git 或 WebCodex 的审查界面检查实际改动。

## 完成

临时试用已经成功。使用这次分享时保持 WebCodex 终端运行；按 Ctrl-C 会结束本次连接。

如果准备继续日常使用，下一步推荐切换到[完整使用指南](PERSONAL_SETUP.zh-CN.md)，获得普通 Server + Runner 的完整 coding 体验。其他参考：

- [ChatGPT、Claude 与认证方式](MCP.zh-CN.md)
- [生产与高级部署](DEPLOYMENT.zh-CN.md)
- [CLI 参考](CLI.zh-CN.md)
- [故障排查](TROUBLESHOOTING.zh-CN.md)
- [安全说明](../SECURITY.md)
- [完整文档索引](INDEX.zh-CN.md)
