# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex 让 ChatGPT、Claude 和其他 MCP 客户端直接使用你自己机器上的仓库与开发工具。
Runner 在仓库所在机器上执行文件、Git、命令与测试操作；仓库本身不需要搬到聊天服务端。

## 用一个仓库快速试起来

平台说明：`webcodex share` 会启动本地 WebCodex Server，目前只支持 Linux 和 macOS。
Windows 版本支持 CLI + Runner 连接远程 Linux Server；Windows 上请使用
`webcodex connect <server-url>`。如果还没有 Server，需要先在 Linux 上部署一个。

默认临时公网分享会优先复用 `WEBCODEX_CLOUDFLARED_BIN` 或 `PATH` 中已有的
`cloudflared`；如果没有，在支持的 Linux/macOS 平台上 WebCodex 会自动下载并校验自己管理的
副本。然后执行：

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

`share` 是完整入口：需要时它会先准备 tunnel 依赖，然后配置当前 Git 项目、启动本地
WebCodex Server + Runner、创建临时 Connector credential，并打开 Cloudflare Quick Tunnel。
第一次使用**不需要**先运行 `setup`、`doctor` 或 `run`。

命令显示 **WebCodex ready** 后保持终端运行，并按它输出的值配置 ChatGPT：

1. 在 ChatGPT 中启用 **Developer Mode**，创建基于 MCP 的 **custom app**。
2. 填入输出的 **MCP URL**。
3. 认证选择 **Access token / API key** 或等价的 Bearer token 选项。
4. 填入输出的临时 **Credential**。
5. 点击 **Scan Tools**。
6. 第一条消息先用只读、安全的请求，例如：

```text
检查这个仓库并总结它的结构。先不要做任何修改。
```

ChatGPT 的 UI 文案可能随 workspace 与 rollout 改变。Developer Mode、custom MCP app 以及
write/modify action 是否可用，由 ChatGPT 套餐、workspace 与管理员设置控制；WebCodex
不能扩大客户端侧 app 权限。当前这次运行到底该填哪个 WebCodex URL、认证类型和
credential，以 CLI 成功输出为准。

默认 `share` 的 URL 与 credential 都是临时的，命令退出后失效。仅做本地 MCP 调试时可用
`webcodex share --tunnel none`，此模式不需要 `cloudflared`。

## 第一次连接以后

WebCodex 可以读取/搜索文件、准备受保护的修改、运行命令与聚焦校验、查看 Git，并让长时间
Job 保持可观察。底层权限边界不会因为 onboarding 简化而改变。打开 `/console` 可以查看
项目就绪状态与工作队列；Console 在相应状态下可以 Guide、Cancel、Accept 或 Reject，
但不会显示 credential。

## 已有 Server 与长期部署

只有在你**已经拥有** WebCodex Server URL 时，才使用长期 `connect` 路径：

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example
```

`connect` 会创建可复用的本地 profile、启动 Runner、等待项目在该 Server 上可见，并输出
MCP 配置值。它是已有 Server 的长期路径，不是本地试用 WebCodex 的前置条件。

`webcodex login` 与 managed OAuth 是需要独立用户身份、撤销、审计或组织级控制时的高级
身份路径。自托管属于 operator 工作流，见[部署指南](docs/DEPLOYMENT.zh-CN.md)。

## 工作方式

```text
ChatGPT / Claude / MCP client
            |
            | MCP / HTTPS
            v
      WebCodex Server
            |
            | 已认证 Runner 连接
            v
     webcodex-runner
            |
      仓库 / Git / 工具链
```

Server 负责认证与路由；真正的工作由仓库所在机器上的 Runner 执行。连接中只传输当前工具
调用需要的输入与结果。

## CLI

普通用户优先需要这些命令：

```bash
webcodex share                     # 最快的临时 ChatGPT/MCP 接入
webcodex connect <server-url>      # 接入已有 Server
webcodex status                    # 简洁项目就绪状态
webcodex doctor                    # 更完整的本地诊断
webcodex setup                     # 手动/本地项目设置
webcodex run                       # 手动启动本地 Server + Runner
webcodex task list                 # 审查本地任务
```

Runner 是执行组件。为了兼容现有 CLI，Runner 服务管理仍使用历史命名空间
`webcodex agent ...`。operator 命令与 credential reference 见 [CLI](docs/CLI.zh-CN.md)。

## 文档

- [快速开始](docs/QUICK_START.zh-CN.md) —— 第一次 ChatGPT/MCP 接入
- [MCP](docs/MCP.zh-CN.md) —— 先讲 ChatGPT/Claude 与认证，再进入协议参考
- [AI 接入指南](docs/AI_ONBOARDING.zh-CN.md) —— 供 AI 帮用户配置 WebCodex
- [CLI](docs/CLI.zh-CN.md) —— 命令、兼容说明与凭据
- [部署指南](docs/DEPLOYMENT.zh-CN.md) —— 自托管与生产运维
- [认证模型](docs/AUTH_MODEL.zh-CN.md) —— credential 与 authority 模型
- [Runner](docs/RUNNER.zh-CN.md) —— Runner 运维
- [Coding 工作流](docs/CODING_WORKFLOW.zh-CN.md)
- [故障排查](docs/TROUBLESHOOTING.zh-CN.md)
- [文档索引](docs/INDEX.zh-CN.md)
- [安全](SECURITY.md)

## 安全

WebCodex 可以修改文件并执行命令。只注册允许助手访问的项目根目录，不要把 credential 写进
prompt、日志或 Git，并优先让 Runner 使用普通 OS 用户。onboarding 的简化不会合并底层真实
不同的 credential/authority。完整模型见 [SECURITY.md](SECURITY.md)。

## 从源码构建

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## 鸣谢

感谢 [LINUX DO](https://linux.do/) 社区提供的交流氛围与开源推广支持。

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
