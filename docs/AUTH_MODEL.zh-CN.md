# 认证和凭据模型

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex 把 bootstrap administration、account onboarding、runtime API access 和 agent connectivity 分开。不要在所有 surface 上复用同一种凭据。

## 凭据摘要

| Credential | 使用方 | 用途 | 不要用于 |
| --- | --- | --- | --- |
| `WEBCODEX_TOKEN` | server admin | bootstrap/root admin | GPT/MCP/agent 日常使用 |
| Project Credential | setup 生成的 Connector + Agent | 精确访问一个 private project grant | 其他项目/admin/普通 quick start |
| shared key | agent + GPT/MCP quick start | shared-key group onboarding | production IAM/admin |
| `wc_acct_xxx` | user CLI | 创建本地 PAT/agent token | GPT/MCP/agent |
| `wc_pat_xxx` | GPT Action/MCP/API | runtime tools | agent connection |
| `wc_agent_xxx` | `webcodex-agent` | 连接 agent 到 server | GPT/MCP/runtime API |

## `WEBCODEX_TOKEN`

`WEBCODEX_TOKEN` 是 server bootstrap/root/admin credential。它配置在 server environment 中，用于 first-user creation 和 emergency administration。

不要把 `WEBCODEX_TOKEN` 放进 GPT Actions、MCP clients 或日常 agent configs。

## Project Credential

`webcodex setup` 会为选定 Git root、profile 和 private state directory 创建一个
Project Credential。Iteration 8.0 中，Connector credential file 与生成的 Agent
config 携带同一个 secret；精确 verifier 会把两类 caller 映射到同一个稳定、非秘密
的 `project_grant_id`。Agent registry access、readiness、file operation、job、log
与 cancel 都要求该 grant。

secret 只存在于 owner-protected private file；不会写入数据库，也不会出现在
readiness、Browser JSON、日志或错误中。runtime 只保留 SHA-256 verifier value，
candidate hash 使用 constant-time comparison。Agent client ID 还包含非秘密 grant
suffix；跨 grant registration 不能替换已有 lease。

Project mode 不是 shared-key quick start。它显式关闭 direct unknown-token fallback，
请求只有通过精确 credential verifier 才能进入 Connector runtime state。因此任意
非空 Bearer token 会得到 `401`，不能创建 Task、Execution、binding 或 Agent
request。Loopback 同样不免除认证，本机进程仍是不同 trust subject。

Setup 不会静默轮换仍存在的 Project Credential。可恢复的丢失应恢复 Connector 与
Agent 两份匹配 private file。若 secret 无法恢复，停止 runtime，并明确退役整个
private project-state profile 后重新 setup；这会生成新 secret，也会退役该 profile
中的本地 Task/Execution history。Iteration 8.0 没有 in-place rotate command。

## Shared key quick start

shared key 是 quick-start secret：agent 通过 `connect --key <KEY>` 使用它；GPT Actions 或 MCP 只有在 Host 支持静态 Bearer/API-key 认证时才使用它。请求形态是：

```text
Authorization: Bearer <KEY>
```

当 `WEBCODEX_SHARED_KEY_ENABLED=true` 时，未知且非 `wc_` 开头的 Bearer 值会被接受为 shared-key principal。该明文值不会作为 server-side allowlist entry 预先登记；WebCodex 按 `shared_key_hash` 对调用者分组。不同值会形成不同的轻量 group。

这个 fallback 只属于显式配置的普通 server quick-start。project-bound setup 会把
它设为 false，并使用上面的精确 Project Credential verifier；两条路径不会互相
fallback。

shared key 不是 admin credential，不是 managed user identity，也不是 production IAM。

静态 Bearer/API-key 认证既可以承载 shared key，也可以承载 managed mode 的 `wc_pat_xxx`。OAuth 是独立 flow；OAuth client 字段留空不会变成 no-auth，也不会变成静态 Bearer。

## `wc_acct_xxx`

`wc_acct_xxx` 是管理员使用 `--issue-credential` 创建用户时签发的一次性 account credential。

用户在本地用它执行：

```bash
webcodex-cli token create-local
webcodex-cli agent-token create-local
```

这些命令在本地生成 plaintext tokens，并只把 token hashes 注册到 server。

不要把 `wc_acct_xxx` 用作 GPT Action token、MCP token、runtime API token 或 agent connection token。

## `wc_pat_xxx`

`wc_pat_xxx` 是用户本地生成的 personal API token。server 只保存它的 hash。

`wc_pat_xxx` 用于：

- GPT Actions
- MCP
- Runtime API calls
- `/api/tools/list` 和 `/api/tools/call` 等 tool calls

应按 workflow 收窄 PAT scope。例如，一个会检查和编辑项目的 GPT Action 可能需要 runtime、project 和 job scopes。

## `wc_agent_xxx`

`wc_agent_xxx` 是用户本地生成的 agent token。server 只保存它的 hash，并把 token 绑定到 `allowed_client_id`。

`wc_agent_xxx` 只能用于 `webcodex-agent` connectivity。它不能调用 runtime、project、tool、MCP 或 account endpoints。

## `client_id`

`client_id` 标识一个 agent client instance，例如：

```text
ubuntu-client
alice-macbook
ci-runner-1
```

Agent token 绑定到允许的 `client_id`，防止为一个 client 创建的 agent token 被拿去注册成另一个 client。

## Runtime project ids

Agent-backed runtime project ids 使用这种格式：

```text
agent:<client_id>:<project_id>
```

示例：

```text
agent:ubuntu-client:webcodex
agent:alice-macbook:my-repo
```

`<project_id>` 来自 agent `projects.d/*.toml` 文件中的顶层 `id` 字段：

```toml
id = "webcodex"
path = "/srv/webcodex/projects/webcodex"
```

不要在 agent `projects.d/*.toml` 文件中使用 server-side `[projects.<id>]` 语法。

## Hash storage

对于用户创建的 PAT 和 agent token，server 保存 token hash，不保存 plaintext `wc_pat_xxx` 或 `wc_agent_xxx`。明文 token 只在创建时显示一次，必须由用户或 agent host 自行保存。
