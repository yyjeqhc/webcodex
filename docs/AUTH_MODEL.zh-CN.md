# 认证与凭据

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex 之所以有多种认证方式，是因为 Server 管理、模型/API 访问和 Runner 连接属于不同的信任边界。普通用户**不需要**先学习 WebCodex 的内部标识符体系，才能安全使用它。

## 最短答案

日常完整使用请直接按[完整使用指南](PERSONAL_SETUP.zh-CN.md)：用一次性登录 code 完成 `webcodex login`，让 CLI 生成本地用户凭据和 Runner 凭据，再使用它输出的连接信息接入 ChatGPT。

如果你连接的是已有 hosted shared-key Server，就使用 operator 提供的 shared key 配合 `webcodex connect`。如果只是临时试用一个仓库，就使用本次 `webcodex share` 输出的临时凭据。

不要把 Server bootstrap token 复制到客户端，也不要把 Runner token 当作 MCP/API token。

## 你实际可能接触到的凭据

| 凭据 | 常见形式 | 用途 |
| --- | --- | --- |
| Server bootstrap token | Server env 中的 `WEBCODEX_TOKEN` | 初始管理与紧急恢复 |
| Pairing code | `wc_pair_...` | 一次性设备/用户接入 |
| 个人 API 令牌（PAT） | `wc_pat_...` | managed user 的 MCP、GPT Actions 与 runtime API |
| Runner token | `wc_agent_...` | 仅 `webcodex-runner` 传输 |
| Shared key | `wck_...` | hosted shared-key MCP/runtime 与对应 Runner group |
| Project Credential | 受保护的项目私有文件 | 单个 project-first Connector/share 环境 |
| OAuth access token | `wc_oat_...` | 启用 OAuth 后的委派 MCP/GPT 访问 |
| Account credential | `wc_acct_...` | 高级 managed-account 本地令牌创建 |

这些前缀有助于诊断“把哪类凭据放错地方”的问题，但不意味着用户需要学习所有内部 ID。

## 凭据不等于各种 ID

WebCodex 内部还有一些非 secret 的 ID 与 opaque tool state。普通用户通常不需要学习它们的格式。

| 类别 | 示例 | 含义 |
| --- | --- | --- |
| Credential | PAT、Runner token、shared key、OAuth token | 用来认证调用方 |
| Resource ID | Project、Job、Workflow Session、task | 标识一个对象；不会授予该对象权限 |
| Opaque tool state | 工具返回的 continuation/recovery value | 用来延续或安全重试某个具体 workflow；不是认证凭据 |

核心规则只有一句：**知道一个 ID 或 opaque tool value，永远不能替代认证与授权。** 精确的内部 identity/continuity 格式属于 maintainer contract 和代码，不属于普通用户指南。

## Secret 处理

- 不要打印、记录、提交或把完整 credential 文件粘贴进聊天。
- CLI 优先使用 `--token-file <path>`，不要把明文 token 放在命令行参数里。
- AI agent 可以指出具体的受保护文件/字段；真正需要复制 secret 时由人类完成。
- status、diagnostic 与普通 API response 会尽量避免返回明文凭据。
- 凭据可能泄露时，应按照创建它的那条流程进行轮换或替换。

## Server bootstrap token

`WEBCODEX_TOKEN` 是 Server bootstrap/admin 凭据。`webcodex server init` 会把它保存到 Server 环境中。它只用于初始管理、建用户/pairing 与紧急恢复，不用于 MCP、GPT Actions、Runner 连接或日常开发。

## Pairing 与 managed login

`webcodex pairing create` 会生成短期 `wc_pair_*` code。仓库机器通过 `webcodex login <server-url> --code <code>` 兑换它，随后 CLI 会创建普通用户/API 访问和 Runner 连接所需的本地文件。

Pairing code 是一次性的，不是长期 API 凭据。

## 个人 API 令牌（`wc_pat_*`）

PAT 在 MCP、GPT Actions 与 runtime API 上代表一个 managed user。Server 只保存其 hash。`webcodex login` 通常会把用户 token 写到该 Server/user 本地配置目录中的 `webcodex-user-token`。

按工作流只授予需要的最小 scopes。普通 MCP coding client 只需要与其实际读写/执行能力对应的 runtime/project 权限；account-management authority 与普通 coding client 分开。

## Runner token（`wc_agent_*`）

Runner token 用于认证 `webcodex-runner`，并绑定到配置的 Runner `client_id`。它会被 MCP/runtime/account surface 拒绝。

`wc_agent_*` 是兼容性保留的历史前缀。当前产品术语中它是 **Runner token**，不是 Durable Agent identity。其它保留的 `agent_*` wire/storage 名称也遵循同样原则：不要从兼容名称推导 Durable Agent 语义。

## Shared key（`wck_...`）

Shared key 是 `webcodex connect` 使用的简单 hosted connection credential。同一把 key 可以认证 MCP/runtime client 与对应 Runner group；不同 key 彼此隔离。

创建后 protected profile 会保存这把 key，重复 `connect` 会复用它而不是再次打印。Shared-key 模式适合简单、受信的部署，不应被理解成 managed multi-user IAM 的替代品。

## Project Credential

`webcodex setup` 与临时 project-first 流程使用一把受保护的 Project Credential，只属于一个项目环境。它不是通用 user/admin token，也不应跨项目复用。

WebCodex 验证该 credential 后只在内部保留所需的非 secret authorization metadata；它不是另一把需要用户复制或管理的 credential。

## OAuth2

OAuth 允许 MCP/GPT client 使用 authorization-code flow，而不是在 client 中长期保存 PAT。注册 client 实际要求的精确 callback URL，并按 `webcodex share --auth oauth` 或 `webcodex connect --auth oauth` 的输出完成连接。

用户需要理解的 OAuth 概念只有：

- **client id** —— OAuth client 的公开标识；
- **client secret** —— 创建 client 时返回的 secret；
- **access token** —— client 实际使用的委派凭据；
- **refresh token** —— 用于刷新 access token；`offline_access` 本身不增加 WebCodex 权限；
- **allowed scopes** —— 这个 client 最多可以请求哪些 WebCodex 权限。

WebCodex 新增 scope 时不会静默扩大已有 OAuth client 的权限上限。修改 allow-list 是显式管理操作，并会让旧 grant 失效，要求 client 重新授权。

普通 hosted shared-key OAuth 使用 `webcodex connect ... --auth oauth`：Runner 继续使用 shared key，而 MCP client 获得独立 OAuth credential。`--oauth-computer-permissions` 显式开放该流程中的额外 Computer 权限；`--oauth-local-mcp` 显式开放 Runner-owned local MCP provider。Managed-user OAuth 仍是另一条高级流程（`--auth managed-oauth`）。

Server 侧配置见[部署指南](DEPLOYMENT.zh-CN.md#oauth2)；MCP client 设置见 [MCP](MCP.zh-CN.md#oauth2)。

## Scope 与 authority

Authentication 回答“**调用方是谁**”；scope 回答“**该调用方可以请求哪一类操作**”。Project/path 检查、Session guard、Runner capability 与 Server authority mode 都是独立的后续检查。

一个 token 拥有某个 scope，并不能绕过 project boundary 或 native safety check；反过来，知道 Project/Session/Job ID 也不会凭空获得缺失的 scope。

`WEBCODEX_AUTHORITY_MODE` 同样与 authentication 分离。它决定有后果的操作在硬安全检查之后是自动执行还是走配置好的人工授权路径，但不会改变 credential identity 或 scope。Maintainer 级细节见 [Authority model](agent/permission-model.md)。

## Computer observation and control authorization

Computer Use 权限有意与普通 project/runtime 权限分开。只读 Computer observation 需要 `computer:read`，会产生 UI effect 的控制需要 `computer:control`，启动应用使用 `computer:launch`。全屏 display、pointer 与全局 clipboard 还拥有额外显式 scope，不会因为旧 credential 已经有普通 Computer 权限就自动继承。

OAuth client 只有通过明确的 operator/user opt-in 才会得到这些 optional permission。真正调用时，runtime 仍会重新检查当前 Runner capability 与操作系统原生权限。产品层说明见 [Computer Use](COMPUTER_USE.md)。

## 仍可能看到的兼容名称

有些 compatibility-facing 名称为了配置/存储/wire 兼容仍然存在：

- `wc_agent_*` —— Runner token；
- `agent:<client_id>:<project_id>` —— runtime Project address；

它们都**不是** WebCodex 独立的 Durable Agent / Conversation / Agent Task domain。其它 process/protocol compatibility field 继续留在 implementation detail 中；新文档除引用上面的公开兼容名称外应统一使用 **Runner**。

## 凭据通常存在哪里

| 凭据 | 常见位置 |
| --- | --- |
| `WEBCODEX_TOKEN` | Server env 文件，常见为 `/etc/webcodex/webcodex.env` |
| Managed user PAT | `~/.config/webcodex/<server-slug>/<user>/webcodex-user-token` |
| Managed Runner token | 对应 `runner.toml` 内联字段 |
| Hosted shared key | 受保护 hosted profile 的 `runner.toml` |
| Project Credential | 受保护的 project-private state |
| OAuth client secret | client 创建时返回；保存在 client/operator 的 secret store |

具体命令和恢复路径见 [CLI](CLI.zh-CN.md) 与[故障排查](TROUBLESHOOTING.zh-CN.md)。内部 identity/continuity 格式有意不放在这份 user-facing reference 中。
