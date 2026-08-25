# AI Coding Agent 接入指南

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

本文面向帮助**用户**把仓库接入 WebCodex 或操作已有部署的 AI coding agent。它不是
[`AGENTS.md`](../AGENTS.md)；后者约束的是 WebCodex 自身开发。

默认用户目标应理解为：**“让 ChatGPT 操作这个仓库。”** 除非用户选择的路径确实需要，
不要提前引入 Server 部署、client id、runtime project id、PAT、Runner token 或 OAuth scope
ceiling。

## 选择最小路径

先检查平台再选路径。Windows 不支持 `webcodex share` 所需的本地 Server runtime；
Windows 已有远程 Linux Server 时使用 `connect`，没有 Server 时先引导用户部署 Linux
Server。不要在 Windows 上推荐 `share`。

1. 在 Linux/macOS 上，用户只有一个本地仓库、想马上试 ChatGPT/MCP，并且没有现成
   WebCodex Server URL：使用 **`webcodex share`**。
2. 用户已经有 WebCodex Server URL，需要长期连接：使用
   **`webcodex connect <server>`**。
3. 用户明确需要独立身份、独立撤销、审计或 managed user：使用 managed identity
   （`webcodex login` / managed OAuth）。
4. 用户需要基础设施控制、内网、稳定 HTTPS 或自有身份系统：使用
   [部署指南](DEPLOYMENT.zh-CN.md)。

不要把虚构的 `https://your-server.example` 当作全新用户的前置条件。

## 第一次 ChatGPT 接入：`share`

先确认 Git 和目标仓库；默认公网 share 还要确认 `cloudflared` 已安装。如果缺少它，告诉用户
使用 Cloudflare 官方下载，不要静默增加“自动下载第三方 executable”的行为。

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

`share` 自己会完成项目 setup。不要把
`setup → doctor → run → 停掉 run → share` 教成 onboarding 主流程。

CLI 显示 **WebCodex ready** 后，让人类保持终端运行，并按 **What to do next**。不要在未确认
前承诺 write 能力：ChatGPT Developer Mode、custom MCP app 与 write/modify action 受用户的
ChatGPT 套餐、workspace 和管理员设置控制，这些客户端侧权限与 WebCodex authorization 是
两层独立边界：

- ChatGPT Developer Mode → 创建 MCP custom app；
- 粘贴输出的 MCP URL；
- 选择 CLI 输出的认证类型；
- 由**人类**把输出的 credential 粘贴到 ChatGPT；
- Scan Tools；
- 第一条：`检查这个仓库并总结它的结构。先不要做任何修改。`

仅做本地调试时可用 `webcodex share --tunnel none`，不启动 Cloudflare Quick Tunnel，也不
需要 `cloudflared`。

## 已有 shared-key Server：`connect`

只有 hosted Server 已明确配置 shared-key client，并且 operator 已提供该 client credential
（或本机已有受保护 profile）时才走这条路径：

```bash
cd /path/to/your/repository
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` 创建/复用 profile、启动 detached Runner、等待同一身份能看到 Runner 与项目，
然后先输出 MCP 配置，再输出诊断 Details。不要为了得到 shared key，把自托管 Docker Server
`.env` 中的 bootstrap administrator token 复制到 client。

逆操作：

```bash
webcodex disconnect
```

它只注销 canonical 路径精确匹配的仓库；不要删除 checkout、`.git`、profile credential 或
同 profile 的其他项目注册。

## 自托管 enrollment 与可选 OAuth

对于刚完成 bootstrap 的自托管 Server，managed pairing 是仓库机器的正常 enrollment 路径，
不是把 Server admin token 复用到 client。先在 Server 创建短期 pairing code，再以实际执行项目
命令的普通用户兑换并显式安装 Runner：

```bash
webcodex login https://webcodex.example --code <wc_pair_...> \
  --allowed-root "$HOME/git"
webcodex agent install --scope user \
  --config <login-reported-agent-config>
```

只有 MCP client 要求 OAuth 且精确 callback URL 已知时，才使用 `share --auth oauth` 或
`connect --auth oauth`。不要把 OAuth client secret、shared key、Project Credential、PAT、
Runner token 或 bootstrap administrator token 合并成一个概念。

同一 managed flow 也能在需要时提供 user/device identity、revocation、audit 或组织管理。
它故意把 user/API authority 与 Runner transport authority 分开。reference 见
[认证模型](AUTH_MODEL.zh-CN.md)和 [MCP](MCP.zh-CN.md)。

## AI 可以检查什么

可以安全观察的非 secret 信息包括：

- 仓库路径和 Git 状态；
- Server URL；
- profile/state/log **路径**；
- `webcodex agent status` / `webcodex server status`；
- `webcodex status` 与 `webcodex doctor` 输出；
- token 文件的位置，但不是其内容。

`doctor` 是本地/手动 runtime 诊断。即使它推荐 `webcodex run`，也不要把这解释成 hosted
ChatGPT 的前置条件；第一次 hosted-chat 路径用 `share`。

## Secret 仍由人类控制

**不要**回读、打印、记录、提交或在聊天中回显：

- shared key、PAT、Runner token、account credential、bootstrap token；
- Project Credential；
- OAuth client secret；
- 完整 `agent.toml` 或 Server env 文件；
- `Authorization` header。

需要把 credential 填进 ChatGPT/Claude 时，要准确说明来源并让**人类**复制。首次优先使用
成功的 `share`/`connect` disclosure；若必须恢复已保存的值，只指出精确受保护文件/字段，
不要由 AI 自己回显。status/log 命令故意不显示 secret。

永远不要用 `wc_agent_*` Runner token 替代 MCP token，不要把 bootstrap
`WEBCODEX_TOKEN` 当作 MCP credential，也不要假设 `webcodex token generate` 的离线素材
已经在远程 Server 注册。

## 故障排查

优先使用 CLI 的 actionable error。已有状态时只读取最小必要的非 secret 诊断：

```bash
webcodex status
webcodex doctor
webcodex agent status --profile <profile>
webcodex agent logs --profile <profile> --lines 100
```

稳定错误码与 operator 检查见[故障排查](TROUBLESHOOTING.zh-CN.md)。
