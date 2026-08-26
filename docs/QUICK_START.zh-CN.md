# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

这个页面只做一件事：让一个本地代码仓库尽快完成第一次 AI 开发请求。开始之前不需要理解 WebCodex 的内部架构。

## 前置条件

- Node.js 18 或更新版本。
- Git，以及一个可以让 AI 安全查看的代码仓库。
- 一键本机 `share` 流程需要 Linux 或 macOS。

Windows 需要连接远程 Linux WebCodex Server，不在本机运行 `share`；请从 [MCP 接入](MCP.zh-CN.md)或[部署指南](DEPLOYMENT.zh-CN.md)开始。

## 1. 运行 WebCodex

进入希望 AI 使用的仓库：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

第一次使用不需要提前运行 `setup`、`doctor` 或 `run`。WebCodex 会自动准备本次临时连接。

## 2. 等待 `WebCodex ready`

看到 **WebCodex ready** 后保持终端运行。WebCodex 会打印 MCP 客户端需要的配置，通常也会把 MCP URL 复制到剪贴板。可以直接在终端按 **Enter** 打开 ChatGPT App 设置，也可以手动进入 **Settings -> Apps -> Create**。

## 3. 在 ChatGPT 中添加 WebCodex

1. 如有需要，在 ChatGPT App 设置中开启 **Developer Mode** 并选择 **Create**。
2. 粘贴已经复制的 **MCP URL**；也可以使用 WebCodex 终端中打印的地址。
3. 默认分享方式选择 **Access token / API key** 或等价的 Bearer 令牌选项。
4. 填入输出的临时 **Credential**。
5. 点击 **Scan Tools**。

不同账号或工作区看到的 ChatGPT 文案可能略有区别，具体 URL 和认证值以 WebCodex 终端输出为准。

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

第一次连接已经成功。使用这次临时分享时保持 WebCodex 终端运行；按 Ctrl-C 会结束本次连接。

接下来按需要阅读：

- [ChatGPT、Claude 与认证方式](MCP.zh-CN.md)
- [Windows 与长期/自托管部署](DEPLOYMENT.zh-CN.md)
- [CLI 参考](CLI.zh-CN.md)
- [故障排查](TROUBLESHOOTING.zh-CN.md)
- [安全说明](../SECURITY.md)
- [完整文档索引](INDEX.zh-CN.md)
