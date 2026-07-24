# WebCodex

[English](README.md) | [简体中文](README.zh-CN.md)

WebCodex 让 coding client 通过项目级 server 和本地 Agent 操作私有代码。源码、
Git、修改和验证仍留在拥有仓库的机器上。

## 安装

支持的 Linux x64 环境可以直接安装：

```bash
npm install -g @yyjeqhc/webcodex
```

也可以从源码构建全部 binaries：

```bash
cargo build --release --bins
export PATH="$PWD/target/release:$PATH"
```

安装细节见 [docs/BUILD_INSTALL.zh-CN.md](docs/BUILD_INSTALL.zh-CN.md)。

## 一个项目，一个入口

在希望开放的 Git 项目中运行：

```bash
webcodex setup
webcodex doctor
webcodex agent start
```

`setup` 在 checkout 外创建最小私有状态。它不会修改 Git 内容、启动后台服务、
开放端口、修改 shell 启动文件或发送项目文件。再次运行是安全的：已有有效配置
保持不变，只修复缺失部分；冲突状态会 fail closed。私有状态包含一个由本项目
Connector 与 Agent 共用的精确 Project Credential；默认输出不会打印它，任意
Bearer token 不能替代它。

`doctor` 只读。刚完成 setup 时，它通常会报告本地 Agent 尚未启动，并给出唯一
下一条命令。

`agent start` 是显式 foreground action，会启动绑定当前项目的 loopback runtime
和 Agent。保持该终端运行，在另一个终端执行：

```bash
webcodex status
```

默认输出只使用 Project、Connection、Agent、Capabilities、readiness 和 next
action 等产品概念，不打印 credentials、client ID、runtime project ID、executor
reference、workflow session 或 transport 细节。

完整步骤见 [docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)。

## Canonical coding path

配置完成的 MCP/OpenAPI Connector 只暴露九项项目级能力：

```text
task_start
→ files_read / files_search
→ edits_apply
→ checks_run
→ task_finish
→ task_review
→ task_cancel（需要时）
```

Connector context 确定性解析项目。普通 coding 不需要先调用 `list_projects`、
`runtime_status`、`tool_manifest`、`start_session`、`current_session`、Agent listing
或 project registration tools。

普通可写任务必须在 `task_finish` 前运行 structured checks。结果保持隔离，直到
人类在本机 review 并接受：

```bash
webcodex task list
webcodex task show <task-id>
webcodex task accept <task-id>
```

task、operation、execution 和 result ID 会保留，因为它们用于精确 retry、进度、
review 和 accept；executor routing 和 queue ID 保持内部实现。

## Readiness

`webcodex status` 快速回答“当前项目现在能不能工作”；`webcodex doctor` 提供
结构化、可执行的诊断，覆盖本地配置、认证材料是否存在、项目注册、Git/workspace、
Agent runtime、server reachability、Agent registration、必要 capability 和
structured validation。

Browser `/console` 只是同一组 readiness facts 的最小只读投影，不是第二套 status
逻辑，也不是 Browser IDE。

## Client 接入

canonical setup 默认只启动 loopback，不创建公网 ingress。本地 client 可以使用
经过批准的本地连接配置及其精确 Project Credential 访问 project-bound
Connector。Loopback 是网络边界，不是认证豁免；未知 credential 会在 readiness、
Task state 或 Agent dispatch 之前被拒绝。ChatGPT 托管 client 需要 operator
管理的 HTTPS endpoint 和认证；setup 不创建 tunnel、不开放公网端口，也不重设
production auth。详见 [docs/DEPLOYMENT.zh-CN.md](docs/DEPLOYMENT.zh-CN.md)、
[docs/MCP.zh-CN.md](docs/MCP.zh-CN.md) 和
[docs/GPT_ACTIONS.zh-CN.md](docs/GPT_ACTIONS.zh-CN.md)。

legacy ToolRuntime discovery/operations tools 继续供管理和诊断使用，但不再是普通
项目 coding path 的前置步骤。

## 安全边界

- setup 只注册 Git 明确解析出的 root，不根据目录同名或最近使用记录猜测。
- project setup 使用精确 credential verifier，不进入普通 arbitrary-key
  quick-start fallback；Connector 与 Agent 必须映射到同一个非秘密 project grant。
- 显式 project binding 按 principal 隔离，并在协议需要时按 transport 隔离；
  ambiguity 会 fail closed。
- read-only task 拒绝 mutation、shell 和 job-like action。
- 优先使用结构化 edit 和 validation，而不是 raw shell。
- validation command 无法 spawn 时属于 executor failure，不是项目 assertion
  failure。
- token、Authorization header、hash、private key 和 secret path 不得出现在
  prompt、日志、示例或提交的配置中。

完整边界见 [SECURITY.md](SECURITY.md) 和
[docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)。

## 范围

WebCodex 是 self-hosted infrastructure，不是 hosted SaaS 或完整 Browser IDE。
高级 multi-client enrollment、production OAuth、remote deployment、QUIC、shell
profile 和 operator observability 继续通过管理文档和 `webcodex-cli` 提供，但不会
改变上面的普通项目入口。

## 文档

- 快速开始：[docs/QUICK_START.zh-CN.md](docs/QUICK_START.zh-CN.md)
- 构建安装：[docs/BUILD_INSTALL.zh-CN.md](docs/BUILD_INSTALL.zh-CN.md)
- 概念：[docs/CONCEPTS.zh-CN.md](docs/CONCEPTS.zh-CN.md)
- MCP：[docs/MCP.zh-CN.md](docs/MCP.zh-CN.md)
- GPT Actions：[docs/GPT_ACTIONS.zh-CN.md](docs/GPT_ACTIONS.zh-CN.md)
- 部署：[docs/DEPLOYMENT.zh-CN.md](docs/DEPLOYMENT.zh-CN.md)
- Roadmap：[docs/ROADMAP.zh-CN.md](docs/ROADMAP.zh-CN.md)

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
