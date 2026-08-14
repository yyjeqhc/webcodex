# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex 把 ChatGPT、Claude 等在线 AI 客户端，连接到运行在你自己的机器上的仓库
与开发工具。通过 WebCodex Server 和持有代码的机器上的 Runner，AI 助手可以直接在
你熟悉的聊天窗口里查看和修改文件、运行命令与测试、操作 Git，并使用项目所在机器上
已经安装的工具链。

参见[最新版本](https://github.com/yyjeqhc/webcodex/releases/latest)和
[文档索引](docs/INDEX.zh-CN.md)。

## 能做什么

- **文件与源码查看** —— 在已注册项目中读取、搜索、列出文件。
- **受保护的编辑** —— 在项目边界内执行结构化文件编辑和受校验的补丁。
- **Git** —— 状态、diff 与聚焦提交的准备。
- **命令与测试** —— 有界的 shell 命令与结构化校验（Rust、Node、Python、Go）。
- **长任务 Job** —— 超出单轮聊天的任务会以可观察、可查询的 Job 继续运行。
- **多台 Runner 机器与多项目** —— 一个 Server 可以把工作路由到多台持有仓库的机器。
- **MCP** —— 从 ChatGPT、Claude 或任意 MCP 客户端接入。
- **可选的 GPT Actions** —— 面向 Custom GPT 的 OpenAPI 集成。

实际可用能力取决于 Server surface、Runner 能力和你配置的权限。

## 工作方式

```text
AI 客户端
   |
   | MCP / HTTPS（或 GPT Actions）
   v
WebCodex Server
   |
   | 已认证的 Runner 连接
   v
webcodex-runner
   |
   仓库 / Git / 工具链
```

Server 负责认证调用方并路由工具请求；真正的文件、Git 和命令操作由持有仓库的
Runner 机器完成。仓库与本地工具链留在 Runner 主机上，连接中只传输当前工具调用
所需的输入与结果。

## 快速开始

三种常见路径：

**1. 把仓库接入已有的 Server**

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

`connect` 把当前目录作为项目，创建本地 profile，启动 detached Runner，并输出
MCP URL 与生成的 key。把 URL 和 key 填入 MCP client 后即可提出真实任务。

**2. 临时分享一个本地项目**

```bash
webcodex share
```

`share` 启动本地 Server + Runner 和 Cloudflare Quick Tunnel，并输出本次会话的
临时 HTTPS `/mcp` URL 与 Bearer credential。它面向开发与测试，不适合生产。

**3. 自托管一个 Server**

在常在线主机上部署 server-only Docker/Compose，在前面配置稳定的 HTTPS 域名，
然后在每台持有仓库的机器上用 `webcodex login` 接入为 Runner。

每条路径的详细步骤见[快速开始](docs/QUICK_START.zh-CN.md)，完整生产部署见
[部署指南](docs/DEPLOYMENT.zh-CN.md)。

## CLI

`webcodex` CLI 覆盖项目设置、Server 与 Runner 生命周期、设备接入和运维检查。
常用命令示例：

```bash
webcodex connect https://your-server.example   # 把仓库接入 hosted Server
webcodex share                                 # 临时分享本地项目
webcodex login https://your-server.example --code <wc_pair_...>
webcodex setup                                 # project-first 本地设置
webcodex doctor                                # 只读就绪检查
webcodex run                                   # 启动 project-bound runtime
webcodex agent status --profile <profile>      # 查看 Runner
webcodex ops status                            # 只读运维检查
webcodex task list                             # 查看任务并在本地决策
```

完整命令、术语与凭据见 [CLI 参考](docs/CLI.zh-CN.md)。

## 让 AI agent 帮你搭建

更愿意让 coding agent 来配置 WebCodex？把本仓库交给它，并请它先阅读
`docs/AI_ONBOARDING.zh-CN.md`。可以直接复制的提示：

```text
阅读 docs/AI_ONBOARDING.zh-CN.md，帮我接入或部署 WebCodex。
先核实当前机器与已有配置。
不要打印或复制任何密钥；需要我输入凭据时告诉我。
```

两份指南用途不同：

- `docs/AI_ONBOARDING.md`（及中文版）面向帮助**用户**安装、接入或部署 WebCodex
  的 AI agent。
- `AGENTS.md` 面向**开发 WebCodex 本身**的 AI coding agent。

## 文档

- [快速开始](docs/QUICK_START.zh-CN.md) —— 最短可用的接入路径
- [AI 接入指南](docs/AI_ONBOARDING.zh-CN.md) —— 供 AI agent 帮你搭建
- [CLI](docs/CLI.zh-CN.md) —— 命令、术语、凭据
- [部署指南](docs/DEPLOYMENT.zh-CN.md) —— 自托管与生产运维
- [认证模型](docs/AUTH_MODEL.zh-CN.md) —— 凭据与令牌
- [Runner](docs/RUNNER.zh-CN.md) —— Runner/agent 是什么，以及如何运维
- [MCP](docs/MCP.zh-CN.md) —— 接入 MCP 客户端
- [Coding 工作流](docs/CODING_WORKFLOW.zh-CN.md) —— bootstrap、behavioral guidance、validation 与 closeout
- [故障排查](docs/TROUBLESHOOTING.zh-CN.md)
- [架构](docs/ARCHITECTURE.md)
- [安全](SECURITY.md)

## 安全

WebCodex 可以修改文件并执行命令，因此应把已连接的客户端视为对已配置机器拥有
真实开发权限的助手。只注册允许助手访问的项目根目录，不要把令牌写进 prompt、
日志或 Git，Runner 优先使用普通 OS 用户。完整说明见 [SECURITY.md](SECURITY.md)。

## 从源码构建

```bash
cargo build --release --workspace --bins
export PATH="$PWD/target/release:$PATH"
```

## 鸣谢

感谢 [LINUX DO](https://linux.do/) 社区提供的交流氛围与开源推广支持。

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
