# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

本指南用尽可能少的概念，把一个本地 Git 仓库通过 MCP 接入 ChatGPT。在 Linux/macOS
上，普通第一次使用的主命令是 `webcodex share`。Windows 不支持 `share` 所需的本地
Server runtime；Windows 用户需要已有的远程 Linux Server，并改用
`webcodex connect <server-url>`。如果还没有 Server，请先看[部署指南](DEPLOYMENT.zh-CN.md)。

## 前置条件

- npm installer 需要 Node.js 18+。
- `PATH` 中有 Git，并准备一个可以安全查看/编辑的 Git 仓库。
- Linux/macOS 的默认临时公网 HTTPS 分享需要把
  [`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/) 安装到 `PATH`。

安装 WebCodex：

```bash
npm install -g @yyjeqhc/webcodex
```

在 Linux/macOS 上只做本地 MCP 调试时，`webcodex share --tunnel none` 不需要
`cloudflared`。

## 1. 分享当前仓库

```bash
cd /path/to/your/repository
webcodex share
```

这一条命令会完成项目设置、启动本地 Server + Runner、创建临时 Connector credential、
打开 Cloudflare Quick Tunnel，并等待 MCP endpoint 可用。除非你明确需要后文的手动/本地
工作流，否则不要先跑 `setup`、`doctor` 或 `run`。

默认 share 是临时的。保持终端运行；Ctrl-C 会停止本地 runtime 与 tunnel，同时使 URL 和
临时 credential 失效。

如果缺少 `cloudflared`，WebCodex 会在创建项目 setup/state 之前失败，并给出官方安装地址。
安装后重试；或者仅本地调试时使用 `--tunnel none`。

## 2. 在 ChatGPT 中添加 WebCodex

终端出现 **WebCodex ready** 后，直接按 **What to do next** 中的值填写：

1. 在 ChatGPT 开启 **Developer Mode**，创建基于 MCP 的 **custom app**。
2. **MCP URL** 填输出的 `https://.../mcp`。
3. 默认 share 的认证选择 **Access token / API key**（Bearer token）。
4. 粘贴输出的 **Credential (this share only)**。
5. 点击 **Scan Tools**。

Console 故意不显示 credential。以后即使打开 `/console`，认证值也应来自成功的 CLI 首次
输出，而不是浏览器页面。

## 3. 发送第一条安全请求

先确认客户端连接的是正确仓库，并且不产生修改：

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

确认成功后，再让它完成一个小且可回退的改动。project-bound coding surface 会在内部处理
项目身份；普通用户不需要在 prompt 中提供 runtime project id 或 operation id。

## 4. 审查结果

浏览器 `/console` 可以查看 readiness、工作队列、task guidance、审批与 review action。
稳定结果准备好后，人类可以在 Console 或 CLI 中 Accept/Reject：

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
# 或：webcodex task reject <task-id>
```

在线模型不能接受自己的结果。

## 已有 Server：长期连接

只有已经拥有 WebCodex Server URL 时，才用 `connect` 替代临时 `share`：

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example
```

`connect` 创建/复用本地 profile、启动 detached Runner、等待 Server 看见 Runner 与项目，
然后输出 MCP URL、认证类型、credential 来源、ChatGPT 提示和诊断 Details。若它自动生成
shared key，完整值只会在允许的首次 disclosure 中出现；status/log 不会泄露它。

以后只注销当前仓库：

```bash
webcodex disconnect
```

自托管和 managed identity 属于独立 operator/高级工作流，见[部署指南](DEPLOYMENT.zh-CN.md)。

## 可选 OAuth

Bearer 是最简单的试用路径。如果 MCP client 要求 OAuth，请提供它的精确 callback URL。

临时 project-bound share：

```bash
webcodex share --auth oauth \
  --oauth-redirect-uri https://client.example/callback
```

已有 hosted Server：

```bash
webcodex connect https://webcodex.example --auth oauth \
  --oauth-redirect-uri https://client.example/callback
```

CLI 会明确哪些值填进 MCP client，哪个临时 project credential 只能填在 WebCodex 授权页。
高级 OAuth scope ceiling、optional Computer permission、managed-user OAuth 与协议 contract
见 [MCP](MCP.zh-CN.md) 和[认证模型](AUTH_MODEL.zh-CN.md)。

## 仅本地 / 手动工作流

这些命令仍适合开发与诊断，但不是 hosted ChatGPT 接入的前置条件：

```bash
cd /path/to/your/repository
webcodex setup     # 只配置私有项目状态
webcodex doctor    # 只读本地 readiness 诊断
webcodex run       # 前台 loopback Server + Runner
# 另一个终端：
webcodex status
```

`doctor` 描述的是本地/手动 runtime；loopback runtime 停止时，它仍可能推荐
`webcodex run`。Hosted client 无法访问 loopback，因此普通 ChatGPT onboarding 从
`share` 开始，而不是从 `doctor` 开始。

## 故障排查

优先看 `share` 或 `connect` 返回的精确错误。已有本地状态时也可运行：

```bash
webcodex status
webcodex doctor
```

| 现象 | 下一步 |
| --- | --- |
| 缺少 `cloudflared` | 从 Cloudflare 官方下载并安装后重试；仅本地调试可用 `share --tunnel none` |
| loopback 端口已被占用 | 停掉冲突进程后重试 |
| 本地/手动 runtime 未运行 | `webcodex run` |
| 已有 hosted profile 的 Runner 不可用 | 重跑 `connect` 或检查 `webcodex agent status --profile <profile>` |
| workspace 不可用 | 恢复 Git 仓库/路径 |

完整检查清单见[故障排查](TROUBLESHOOTING.zh-CN.md)。
