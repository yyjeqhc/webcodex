# 快速开始

[English](QUICK_START.md) | [简体中文](QUICK_START.zh-CN.md)

本指南用尽可能少的概念，把一个本地 Git 仓库通过 MCP 接入 ChatGPT。在 Linux/macOS
的交互式 Git checkout 中，裸 `webcodex` 是最短 first-run 入口，并会进入正常的
`webcodex share` 工作流。Windows 不支持 `share` 所需的本地
Server runtime；Windows 用户需要已有的远程 Linux Server，并改用
`webcodex connect <server-url>`。如果还没有 Server，请先看[部署指南](DEPLOYMENT.zh-CN.md)。

## 前置条件

- npm installer 需要 Node.js 18+。
- `PATH` 中有 Git，并准备一个可以安全查看/编辑的 Git 仓库。
- Linux/macOS 默认临时公网 HTTPS 分享不再要求单独安装 `cloudflared`。WebCodex 会优先
  复用显式指定或 `PATH` 中的 binary；没有时自动下载固定版本并校验后使用。
  通过 npm wrapper 启动时，managed 下载还会继承 npm proxy、`noproxy`、CA 与
  `strict-ssl` 配置；没有 npm-specific 配置时继续使用标准 proxy/系统信任路径。

不全局安装也可以直接试用：

```bash
cd /path/to/your/repository
npx --yes @yyjeqhc/webcodex
```

也可以先全局安装一次：

```bash
npm install -g @yyjeqhc/webcodex
```

npm wrapper 在第一次执行时可以 lazy bootstrap 经过校验的 native binary，因此 npx 路径
不依赖 npm 是否保留 postinstall 产生的文件。在 Linux/macOS 上只做本地 MCP 调试时，
`webcodex share --tunnel none` 不需要
`cloudflared`。

## 1. 分享当前仓库

前面的 `npx --yes @yyjeqhc/webcodex` 已经会直接进入这条 first-run 路径。如果选择了
全局安装，再运行：

```bash
cd /path/to/your/repository
webcodex
# 显式/脚本友好的等价入口：
# webcodex share
```

裸 `webcodex` 只会在 Linux/macOS + 交互式终端 + Git checkout 中这样自动分发；脚本、
非交互调用和 repo 外目录仍显示普通 CLI help。需要确定性行为时继续显式使用 `share`。

这一条 first-run 路径会完成项目设置、启动本地 Server + Runner、创建临时 Connector
credential、打开 Cloudflare Quick Tunnel，并等待 MCP endpoint 可用。除非你明确需要后文的手动/本地
工作流，否则不要先跑 `setup`、`doctor` 或 `run`。

默认 share 是临时的。保持终端运行；Ctrl-C 会停止本地 runtime 与 tunnel，同时使 URL 和
临时 credential 失效。

如果缺少 `cloudflared`，WebCodex 会在创建项目 setup/state 之前把固定版本下载到私有用户
状态目录，校验 artifact 与最终 binary 后继续。可用 `WEBCODEX_CLOUDFLARED_BIN` 强制指定
可信 binary；仅本地调试时也可以使用 `--tunnel none`。

## 2. 在 ChatGPT 中添加 WebCodex

终端出现 **WebCodex ready** 后，终端中打印的值仍然是 source of truth。公网 share 会
best-effort 把 MCP URL 复制到剪贴板，但 credential 永远不会自动复制；交互式终端可按
Enter 打开 ChatGPT App 设置。然后：

1. 在 ChatGPT 开启 **Developer Mode**，进入 **Settings -> Apps -> Create**。
2. 粘贴已复制的 **MCP URL**；复制失败时使用输出的 `https://.../mcp`。
3. 默认 share 的认证选择 **Access token / API key**（Bearer token）。
4. 粘贴输出的 **Credential (this share only)**。
5. 点击 **Scan Tools**。

如不希望访问剪贴板，使用 `webcodex share --no-copy-url`。

Console 故意不显示 credential。以后即使打开 `/console`，认证值也应来自成功的 CLI 首次
输出，而不是浏览器页面。ChatGPT Developer Mode、custom MCP app 与 write/modify action
还分别受 ChatGPT 套餐、workspace 和管理员设置控制；客户端 workspace 没有允许的 action，
WebCodex 不能自行把它开启。

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

自托管和 managed identity 属于独立 operator/高级工作流。如果还需要先准备 Server，推荐的
Docker Server 路径无需 clone 仓库，只需三条 shell 命令；见[部署指南](DEPLOYMENT.zh-CN.md#docker仅-server)。

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
| WebCodex-managed `cloudflared` 获取失败 | 检查网络/代理后重试；也可用 `WEBCODEX_CLOUDFLARED_BIN` 指定可信 binary，或仅本地调试时用 `share --tunnel none` |
| loopback 端口已被占用 | 停掉冲突进程后重试 |
| 本地/手动 runtime 未运行 | `webcodex run` |
| 已有 hosted profile 的 Runner 不可用 | 重跑 `connect` 或检查 `webcodex agent status --profile <profile>` |
| workspace 不可用 | 恢复 Git 仓库/路径 |

完整检查清单见[故障排查](TROUBLESHOOTING.zh-CN.md)。
