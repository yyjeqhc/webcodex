# AI Coding Agent 接入指南

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

本文面向帮助**用户**把本地仓库接入 WebCodex 或部署 WebCodex Server 的 AI coding
agent。它与 [`AGENTS.md`](../AGENTS.md) 不同——后者面向**开发 WebCodex 本身**的
AI coding agent。

执行命令前先选择一条路径，并在改动任何配置前核实当前机器与已有配置。

## 决策树

1. 用户只想最快地把一个或多个仓库接入，且不需要自己运维 Server？
   - 是：使用 **Hosted shared key** 与 `webcodex connect`。
2. 用户需要独立账号、设备级授权、独立令牌撤销、身份审计或组织管理？
   - 是：使用 **Managed flow** 与 `webcodex login`。
3. 用户需要完整基础设施控制、内网、自有 HTTPS 或身份系统，或不依赖官方
   Server？
   - 是：使用 **Full self-hosting**，阅读[部署指南](DEPLOYMENT.zh-CN.md)。

hosted 路径不需要部署 WebCodex Server。

## 最快接入：hosted shared key

在持有仓库的机器上执行：

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex connect https://your-server.example
```

当前目录是默认项目。除非用户显式提供 `--key-file` 或 `--key`，命令会生成共享
key。连接检查通过后，用输出的值配置 MCP client：

```text
MCP URL: https://your-server.example/mcp
Authentication: Bearer token
Bearer token: 生成的 key
```

`connect` 完成完整本地设置：创建或复用 profile、生成唯一 client ID、注册本地
项目、写入 `0600` Runner 配置、启动一个 detached Runner，并等待同一 key 能看到
Runner 与目标项目。再次运行同一命令会复用 profile 与存活 Runner。

自动化场景优先用 `--key-file <path>`，避免 key 进入 shell 历史。不要同时传
`--key` 与 `--key-file`。

## 自动 key

省略 `--key`/`--key-file` 时，`connect` 会生成超过 256 位随机性的 `wck_...`
key，保存在受保护 profile 中，只在首次创建时完整打印。请让用户立刻把它复制到
MCP client。重复连接会恢复本地 profile 且不再打印 key。

key 存放在 `~/.config/webcodex/clients/<profile>/agent.toml`（或
`$XDG_CONFIG_HOME/webcodex/clients/<profile>/agent.toml`）的顶层 `token`
字段中。如果用户丢失了打印出来的值，请让人类从该字段复制；作为 AI agent，
请定位文件并把位置告诉用户——不要把值回显到聊天里。

detached Runner 能存活到终端关闭，但不能跨越机器重启。重启后，重新运行同一条
`connect` 或使用 `webcodex agent start --profile <profile>`。

## Managed flow

用户需要独立身份、令牌撤销、设备级授权、身份审计或组织管理时使用：

```bash
webcodex login https://your-server.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
```

managed flow 使用 pairing/account 凭据、用于 MCP/API 的 PAT，以及独立绑定的
Runner 令牌。不要用共享 key 替代这套拆分。

## 完全自托管

当用户需要完整的 Server 与数据控制、内网部署、自有 HTTPS endpoint、自有身份
系统或不依赖官方 Server 时，使用[部署指南](DEPLOYMENT.zh-CN.md)。该路径包含
Server、数据库/状态、反向代理、TLS、服务与凭据操作；这些都不是 hosted
`webcodex connect` 的前提。

## AI agent 可以检查什么

你可以安全地定位并读取**非敏感**配置：

- profile 路径：`~/.config/webcodex/clients/<profile>/`（或
  `$XDG_CONFIG_HOME/webcodex/clients/<profile>/`）
- Runner 状态与日志路径：
  `~/.local/state/webcodex/clients/<profile>/`（或
  `$XDG_STATE_HOME/webcodex/clients/<profile>/`）
- 令牌**文件路径**（不是内容）：如 `webcodex-user-token`
- env 文件**路径**与变量**名**（例如 server env 文件里的 `WEBCODEX_TOKEN`）——
  不是值
- server URL
- 服务状态：`webcodex agent status` / `webcodex server status`
- `webcodex doctor` 输出

有用的非敏感命令：

```bash
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
webcodex status
webcodex doctor
webcodex ops status --server-url <url> --token-file <path> --strict
```

## 必须由人类复制或粘贴的内容

**不要**回读、打印、记录、提交或在聊天里回显：

- 完整令牌值（共享 key、PAT、Runner token、account credential、bootstrap
  token、OAuth secret）
- 完整 `agent.toml` 内容
- server env 文件
- `Authorization` 头
- OAuth client secret

当某个 secret 必须填入 ChatGPT/Claude 或其他客户端时，请告诉**人类**具体该复制
哪个本地文件/值。例如："请把 `~/.config/webcodex/<server>/<user>/webcodex-user-token`
里的值复制到 Bearer 字段。" 不要自己读取文件并把内容粘贴进聊天。

### 面向 AI agent 的凭据规则

- 永远不要运行 `webcodex token generate` 并假设远程 Server 会接受其输出。它只
  生成离线素材，不会注册。
- 永远不要用 `wc_*` 值作为 hosted 共享 key。未知或已撤销的 managed 凭据不会
  回退到 shared-key 认证。
- 永远不要用 `wc_agent_*` 替代 MCP 令牌。
- 永远不要把 bootstrap `WEBCODEX_TOKEN` 粘贴进 MCP 或本地 hosted profile。
- 永远不要打印、记录、提交或复制完整 `agent.toml`。
- 配置 MCP 前先运行 `connect`，确保完整 Runner/project 路径先被验证。

## 本地状态与故障排查

普通非 root 用户：profile 配置默认在 `~/.config/webcodex/clients/<profile>/`；
Runner 状态与日志在 `~/.local/state/webcodex/clients/<profile>/`。hosted Runner
会写 `runner.log`，运行期间约 10 MiB 轮转，只保留 `runner.log`、
`runner.log.1`、`runner.log.2`（Unix 上均为 `0600`）。`agent logs --lines` 读取
有界尾部；`--follow` 会在轮转后重新打开 `runner.log`。

连接失败时，使用 `connect` 打印的 profile 与日志路径。检查 Server 可达性、
shared-key 是否启用、key 是否完全一致、client ID 冲突与项目路径有效性。status
与 logs 不会打印 key。稳定的失败指引见[故障排查](TROUBLESHOOTING.zh-CN.md)。
