# AI Coding Agent 接入指南

[English](AI_ONBOARDING.md) | [简体中文](AI_ONBOARDING.zh-CN.md)

本文面向帮助**用户**把仓库接入 WebCodex 或操作已有部署的 AI coding agent。它不是
[`AGENTS.md`](../AGENTS.md)；后者约束的是 WebCodex 自身开发。

默认用户目标应理解为：**“让 ChatGPT 正常使用我的开发环境。”** 除非用户明确说只想临时试用，否则优先帮助他获得普通 Server + Runner 的完整 coding 体验，而不是默认把 `share` 当成 WebCodex 本体。

第一次成功前，把用户需要理解的词汇压缩到：**WebCodex 服务、Runner（运行代码的机器）、Project（项目）、一次性登录码、ChatGPT 连接**。不要提前解释 `client_id`、runtime project id、`projects_dir`、PAT、Runner token、scope ceiling、Connector surface 等内部术语。

## 先判断用户要哪种体验

1. 用户想正常/长期/日常使用 WebCodex，或没有明确要求“只临时试一下”：使用[完整使用指南](PERSONAL_SETUP.zh-CN.md)，建立普通 Server + Runner。
2. 用户明确只想几分钟体验一个仓库、不想配置长期 runtime：使用 **`webcodex share`**。这是临时、单项目、受限体验。
3. 用户已经有 WebCodex Server URL：优先使用该 Server 已提供的正常 enrollment 方式；自托管 managed Server 用 pairing + `webcodex login`，只有 operator 明确提供 shared-key credential 时才使用 `webcodex connect <server>`。
4. 用户需要生产托管、多用户、OAuth、systemd/Docker、私有 CA 等运维能力：再进入[部署指南](DEPLOYMENT.zh-CN.md)。

Windows 可以直接运行前台 Server 和 Runner；不要因为平台是 Windows 就把用户推到远程 Linux，也不要因为需要 Tunnel 就切换成 `share`。**Tunnel 解决网络可达性，不决定 WebCodex 的 coding 权限模式。**

## 默认：完整使用

完整路径的用户说明以[完整使用指南](PERSONAL_SETUP.zh-CN.md)为准。AI 应优先帮助用户完成这些可观察步骤：

- 安装 WebCodex；
- 启动普通 Server；
- 选择公网 HTTPS / Cloudflare / OpenAI Tunnel 等一种到 Server 的连接方式；
- 创建一次性登录码；
- `webcodex login ... --project <repo>`；
- 启动 Runner；
- 按 `--print-mcp-config` 或 Tunnel 指南把连接加入 ChatGPT；
- 用一次读取和一次小修改验证完整 coding 体验。

不要把 Deployment reference 中的 systemd socket、OAuth scope、token 类型、registry 路径等内容提前搬进这条普通用户流程。

## 临时试用：`share`

只有用户明确选择快速临时体验时，才把 `share` 作为主路径：

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

`share` 自己完成临时项目 setup，并启动本次会话拥有的 Server/Runner/Tunnel。CLI 显示 **WebCodex ready** 后，让人类按输出配置 ChatGPT。Developer Mode、custom MCP app 与 write/modify action 仍受 ChatGPT 套餐、workspace 和管理员设置控制。

仅本地调试可用 `webcodex share --tunnel none`；明确使用 OpenAI Secure MCP Tunnel 的临时分享可用 `webcodex share --tunnel openai`。这些都是 `share` 的临时模式，不要据此推断普通 Server + Runner 也受同样的临时限制。

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
  --allowed-root "$HOME/git" \
  --project "$HOME/git/my-repo"
webcodex runner install --scope user \
  --config <login-reported-runner-config>
```

面向普通用户只解释：`--project` 是实际项目，`--allowed-root` 是以后允许继续添加项目的父目录。不要要求用户手工编辑 `runner.toml` 或 `projects.d`；只有排障或 reference 场景才解释 registry/authority 的内部表示。如果 login 时没有传 `--project`，再用 `webcodex project register --config <login-reported-runner-config> /path/to/repo` 添加项目。

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
- `webcodex runner status` / `webcodex server status`；
- `webcodex status` 与 `webcodex doctor` 输出；
- token 文件的位置，但不是其内容。

`doctor` 是本地/手动 runtime 诊断，不是普通 ChatGPT 接入的必经仪式。根据用户目标选择完整 Server + Runner 或临时 `share`，不要机械地把诊断命令堆进 onboarding。

## Secret 仍由人类控制

**不要**回读、打印、记录、提交或在聊天中回显：

- shared key、PAT、Runner token、account credential、bootstrap token；
- Project Credential；
- OAuth client secret；
- 完整 `runner.toml` 或 Server env 文件；
- `Authorization` header。

需要把 credential 填进 ChatGPT/Claude 时，要准确说明来源并让**人类**复制。优先使用当前流程显式产生的连接信息，例如完整使用中的 `login --print-mcp-config`，或临时 `share` / 已有 Server `connect` 的成功 disclosure。若必须恢复已保存的值，只指出精确受保护文件/字段，不要由 AI 自己回显。status/log 命令故意不显示 secret。

永远不要用 `wc_agent_*` Runner token 替代 MCP token，不要把 bootstrap
`WEBCODEX_TOKEN` 当作 MCP credential，也不要假设 `webcodex tokens generate` 的离线素材
已经在远程 Server 注册。

## 故障排查

优先使用 CLI 的 actionable error。已有状态时只读取最小必要的非 secret 诊断：

```bash
webcodex status
webcodex doctor
webcodex runner status --profile <profile>
webcodex runner logs --profile <profile> --lines 100
```

稳定错误码与 operator 检查见[故障排查](TROUBLESHOOTING.zh-CN.md)。
