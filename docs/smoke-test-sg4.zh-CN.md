# sg4 手工 smoke test：account credential、PAT、agent token 和 project call

[English](smoke-test-sg4.md) | [简体中文](smoke-test-sg4.zh-CN.md)

本文档记录 `https://sg4.yyjeqhc.cn` 上手工验证过的 onboarding loop。其他部署请替换 domain、username、`client_id`、project id 和 paths。

> 这是环境特定验证记录，不建议作为通用入门文档放在 README 主路径。它适合保留在高级/验证资料中，或以后移动到 `docs/validation/`。

## 已验证示例值

```text
username = alice
client_id = ubuntu-client
agent project id = webcodex
runtime project id = agent:ubuntu-client:webcodex
project path = /root/git/private-drop
```

## Credential model

- `WEBCODEX_TOKEN`：server bootstrap/root/admin credential。用于 first user creation 和 emergency management。不要用于 GPT Actions、MCP 或日常 `webcodex-agent`。
- `wc_acct_xxx`：account credential。管理员创建用户时签发一次；用户用它在本地创建 `wc_pat_xxx` 和 `wc_agent_xxx`。不要放进 GPT Actions/MCP，也不要给 `webcodex-agent`。
- `wc_pat_xxx`：personal API token。本地生成，server 只保存 hash。用于 GPT Actions、MCP、`/api/tools/list` 和 `/api/tools/call`。
- `wc_agent_xxx`：agent token。本地生成，server 只保存 hash，并绑定 `allowed_client_id`。只用于 `webcodex-agent`。
- `client_id`：agent client instance id，例如 `ubuntu-client` 或 `alice-macbook`。Runtime project ids 形如 `agent:<client_id>:<project_id>`。

## 测试流程摘要

### 1. Server env

在 server 上创建 env file，设置 `WEBCODEX_ADDR`、`WEBCODEX_DATA` 和 `WEBCODEX_TOKEN`，并启动 `webcodex`。

保持 `WEBCODEX_TOKEN` 只在 server side。它不是 GPT Action/MCP token，也不是 agent token。

### 2. 创建用户并签发 account credential

管理员运行：

```bash
webcodex-cli users create \
  --server-url https://sg4.yyjeqhc.cn \
  --token "$WEBCODEX_TOKEN" \
  --username alice \
  --display-name "Alice" \
  --role user \
  --issue-credential
```

保存返回的 `wc_acct_xxx`，示例中通过 `WEBCODEX_ACCOUNT_CREDENTIAL` 传递。

### 3. 用户本地创建 PAT

```bash
webcodex-cli token create-local \
  --server https://sg4.yyjeqhc.cn \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

输出包含 `wc_pat_xxx`。将其保存为 `WEBCODEX_PAT`，用于 GPT Actions/MCP/runtime API calls。

### 4. 用户本地创建 agent token

```bash
webcodex-cli agent-token create-local \
  --server https://sg4.yyjeqhc.cn \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --client-id ubuntu-client \
  --name ubuntu-client
```

输出包含 `wc_agent_xxx`，保存为 agent host 上的 `WEBCODEX_AGENT_TOKEN`。

### 5. 初始化 agent

使用 `webcodex-agent init` 设置 server URL、agent token、`client_id`、owner、display name、transport、projects dir、allowed root 和 output config。

### 6. Agent project config

Agent project file 使用顶层 `id` 和 `path`：

```toml
id = "webcodex"
path = "/root/git/private-drop"
name = "WebCodex"
kind = "repo"
allow_patch = true
```

不要使用 server-side nested `[projects.webcodex]` 格式。

### 7. 启动 agent

```bash
webcodex-agent --config /opt/webcodex/agent.toml
```

### 8. runtime_status check

调用 `/api/runtime/status`，确认 `ubuntu-client` online，且 projects_count 至少为 1。

### 9. projects/list check

调用 `/api/projects/list`，确认 `agent:ubuntu-client:webcodex` 出现在结果中。

### 10. tools/call git_status check

通过 `/api/tools/call` 调用 `git_status`，确认 project tool routing 可用。

### 11. GPT Action smoke

在 GPT Action 中使用同一个 `wc_pat_xxx`，先测试 `getRuntimeStatus`、`listProjects`、`getProjectGitStatus` 等只读 tools。写入测试应限制在 disposable smoke project。

## 清理建议

如果这个记录不再需要出现在公开文档索引中，可以移动到 `docs/validation/` 或 release notes。不要在本文档中保存真实 token、完整 env file 或完整 `agent.toml`。
