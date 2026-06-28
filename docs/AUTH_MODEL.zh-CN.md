# 认证和凭据模型

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex 把 bootstrap administration、account onboarding、runtime API access 和 agent connectivity 分开。不要在所有 surface 上复用同一种凭据。

## 凭据摘要

| Credential | 使用方 | 用途 | 不要用于 |
| --- | --- | --- | --- |
| `WEBCODEX_TOKEN` | server admin | bootstrap/root admin | GPT/MCP/agent 日常使用 |
| `wc_acct_xxx` | user CLI | 创建本地 PAT/agent token | GPT/MCP/agent |
| `wc_pat_xxx` | GPT Action/MCP/API | runtime tools | agent connection |
| `wc_agent_xxx` | `webcodex-agent` | 连接 agent 到 server | GPT/MCP/runtime API |

## `WEBCODEX_TOKEN`

`WEBCODEX_TOKEN` 是 server bootstrap/root/admin credential。它配置在 server environment 中，用于 first-user creation 和 emergency administration。

不要把 `WEBCODEX_TOKEN` 放进 GPT Actions、MCP clients 或日常 agent configs。

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
path = "/root/git/private-drop"
```

不要在 agent `projects.d/*.toml` 文件中使用 server-side `[projects.<id>]` 语法。

## Hash storage

对于用户创建的 PAT 和 agent token，server 保存 token hash，不保存 plaintext `wc_pat_xxx` 或 `wc_agent_xxx`。明文 token 只在创建时显示一次，必须由用户或 agent host 自行保存。
