# E2E Validation

[English](E2E_VALIDATION.md) | [简体中文](E2E_VALIDATION.zh-CN.md)

E2E validation script 用于在不依赖 ChatGPT UI 的情况下，本地验证 WebCodex server、agent、GPT Actions schema 和 MCP endpoint。

## 检查内容

典型 validation 应确认：

- `/openapi.json` 有效，并包含预期 GPT Actions operations。
- `runtime_status` 返回 `service=webcodex`。
- `listAgents` 显示 online agent 和 redacted policy summary。
- `listProjects` 显示 agent-backed project ids。
- 只读 project tools 可用。
- Mutation tools 只对 disposable projects 生效。
- MCP tools/list 与预期 runtime tool surface 匹配。

## Authentication

REST、polling、MCP 和 GPT Actions calls 必须使用：

```text
Authorization: Bearer <token>
```

`?token=` 只用于 `/api/agents/ws` WebSocket handshake 兼容场景。

## Codex CLI

Codex validation 是可选的。`runCodexTask` 需要 agent host 上有 Codex CLI。Local E2E tests 可以使用 stub Codex binary；这不会启动单独的 `webcodex-agent`。

## Management setup

优先使用：

```bash
webcodex-cli server init
webcodex-cli server install-service
webcodex-cli server status
webcodex-cli pairing create --server-url URL --username alice --client-id alice-laptop
webcodex-cli client enroll --server-url URL --pairing-code CODE --client-id alice-laptop
webcodex-cli agent install-service --profile special --bin /opt/webcodex/bin/webcodex-agent
webcodex-cli agent status --profile special --server-url URL
webcodex-cli doctor --strict --profile special --server-url URL
```

`pairing create` 是 server/admin-side。`client enroll`、`agent install-service` 和 `agent status` 是运行 `webcodex-agent` 的 client-side 操作。不要把 server tokens 复制到 client；只复制短期 pairing code。
## 二进制 help 校验

发布前或大规模修改文档后，应对照实际二进制 help 检查命令示例：

```bash
webcodex-cli -h
webcodex-cli server init -h
webcodex-cli server install-service -h
webcodex-cli server status -h
webcodex-cli pairing create -h
webcodex-cli client enroll -h
webcodex-cli agent install-service -h
webcodex-cli agent status -h
webcodex-cli doctor -h
webcodex-agent -h
webcodex -h
```

重点检查管理员创建账户凭据使用 `users create --server-url ...`，而本地 token 创建使用 `token create-local --server ...` 和 `agent-token create-local --server ...`。


兼容入口仍然存在，但新的 validation docs 应使用 `webcodex-cli`。

## Documentation scans

Docs polish 或 release validation 期间，应运行标准 documentation scans，检查三类错误：

1. 指向已删除 historical planning documents 的引用；
2. 旧产品名或 legacy environment/key names；
3. 用户文档和示例中明显真实的 token-looking values。

预期结果：没有 stale deleted-doc references，没有旧产品名，没有真实 token-looking values。`<token>` 和 `<wc_pair_...>` 这类 placeholder 是可以的。
