# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex 可以暴露不止一种 MCP model surface。project-first 的
`webcodex run` / `webcodex share` 会启动带 project-bound
`canonical_connector` surface 的 Server。`webcodex connect <server>` 则连接
已有 hosted Server，并使用该 Server 选择的 MCP surface；没有 Connector 配置时，
默认是更宽的 `local_coding` surface。project-first 流程先看
[快速开始](QUICK_START.zh-CN.md)。

## Endpoint 与认证

本地客户端可用：

```text
http://127.0.0.1:<configured-port>/mcp
```

Hosted 客户端需要 HTTPS。有三种路径：

- **Hosted：** `webcodex connect <server>` 使用已有 hosted Server；本地只运行
  Runner。默认仍使用 shared-key。`webcodex connect <server> --auth oauth
  --oauth-redirect-uri <URL>` 会把同一个 shared-key group 桥接成 ChatGPT OAuth，
  不需要 login、pairing、PAT 或 account identity。Runner 继续使用 direct shared key，
  ChatGPT 只获得 OAuth client credential/token；工具集合先由远端 Server 的 MCP
  model surface 决定，OAuth caller 的 `tools/list` 还会按 access token 的实际 scopes
  进一步投影。
- **本地分享：** `webcodex share` 启动本地 Server + Agent；默认启动 Cloudflare
  Quick Tunnel 并使用独立的临时 Bearer credential。`webcodex share --auth oauth
  --oauth-redirect-uri <URL>` 则暴露 project-bound OAuth 2.0 Authorization Code +
  PKCE。OAuth client 仍绑定同一个 project grant，而 code/access/refresh grant
  都 fenced 到当前 share 进程。`--tunnel none` 可用于本地测试，或配合 operator
  管理的 `--public-url` 使用。
- **自托管：** 使用稳定 HTTPS 域名/tunnel、持久服务管理与 OAuth 或受限凭据进行
  长期运行。

普通 hosted `connect` 直接使用其生成/提供的 shared key，或把同一个身份桥接成 OAuth。managed-user 部署也可使用受限 user API token（`wc_pat_*`）或显式 `--auth managed-oauth` 流程。不要把 bootstrap/admin token、account credential、Runner token 或持久 project-first Connector credential 当作公开分享 secret。`share` 会为该会话创建并打印自己的临时 credential。

在 ChatGPT Developer Mode 中，用输出的 `/mcp` URL 创建自定义 app。如果认证菜单
提供 **访问令牌/API 密钥**，选择它并粘贴 bearer credential，然后执行 **Scan
Tools / 扫描工具**。ChatGPT 的 UI 文案与可用范围可能随 workspace 与 rollout
变化。

### OAuth2

当 managed/自托管 Server 启用 OAuth，或使用 `webcodex share --auth oauth` 时，MCP 客户端可以使用 authorization-code
流程而非静态 token。把精确的 ChatGPT callback URL 注册为 OAuth client redirect
URI；宿主提供 `offline_access` 时保持勾选（它是协议级 refresh-token scope，不授予
额外权限）。服务端 OAuth 设置见[部署指南](DEPLOYMENT.zh-CN.md#oauth2)。

对于 project-first share，授权页要求输入本次临时 Project share credential，并签发只带 `runtime:read`、`project:read`、`project:write`、`job:run` 的 `oauth2_project` 身份；它不会创建 managed user，OAuth token 也不能用于 Agent transport。Quick Tunnel 的 issuer URL 每次运行都会变化；如果 OAuth issuer 必须稳定，请使用 `--tunnel none --public-url https://...` 并在外部配置稳定 HTTPS proxy/tunnel。

对于已有 hosted Server，普通 `connect --auth oauth` 使用 shared-key OAuth bridge。OAuth client 以及 code/access/refresh grant 都绑定到 direct shared-key Runner/projects/jobs 所使用的同一个 `shared_key_hash`。direct shared-key bearer authority 始终固定为 `runtime:read`、`project:read`、`project:write`、`job:run`、`computer:read`、`computer:control`；普通 OAuth 默认也是这个 baseline。只有 connect 显式传 `--oauth-computer-permissions`，OAuth client ceiling 才可增加 `computer:launch`、`computer:display_read`、`computer:pointer_control`、`computer:clipboard_read`、`computer:clipboard_write`，而浏览器授权页仍默认全部未勾选，并且只能 grant 本次 OAuth request 实际请求且用户选择的 permission。pointer consent 还要求 request 已包含 runtime 所需的 `computer:read`、`computer:control`；display/clipboard 同样要求对应 baseline read/control scope，因此 consent、token projection 与 runtime scope gate 静态一致。ceiling 真正变化会撤销旧 grant。`account:manage`、`admin`、`job:detach`、任何 `agent:*` 与未来 scope 始终在 bridge 之外；`offline_access` 仍只是协议 scope。授权页按同一个在线 Runner 判断 capability，并在 POST 重新计算；这只代表 backend 当前可用，不保证 OS/native permission 或调用一定成功。runtime 中 OAuth `tools/list` 会隐藏 token scope 不足的工具，直接 `tools/call` scope gate 与 Runner/native 实时检查仍是最终 authority。managed-user identity 仍单独使用 `connect --auth managed-oauth`。

### Grok Custom Connector（OAuth）

Grok 支持自定义 MCP Connector，并可完成 MCP Server 要求的 OAuth 流程。对于
自托管 WebCodex Server，先通过公网 HTTPS 暴露
`https://your-domain.example/mcp`，并启用 OAuth：

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

当前 Grok Web Connector 流程（2026 年 8 月已验证）使用以下 redirect URI，注册时
必须精确匹配：

```text
https://grok.com/connectors-oauth-exchange-code/
```

如果后续 Grok 展示或实际使用了不同 callback，应改为注册 Grok 当时提供的精确值。
为 Grok 单独创建 OAuth client；`client_secret` 只会返回一次：

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"Grok MCP","redirect_uris":["https://grok.com/connectors-oauth-exchange-code/"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

在 Grok 的 **Custom Connector** 表单中填写：

| 字段 | 值 |
| --- | --- |
| MCP server URL | `https://your-domain.example/mcp` |
| Client ID | 创建 client 时返回的 `wc_client_*` |
| Client Secret | 只返回一次的 `wc_csec_*` |
| Authorization Endpoint | `https://your-domain.example/oauth/authorize` |
| Token Endpoint | `https://your-domain.example/oauth/token` |
| Scopes | `runtime:read`、`project:read`、`project:write`、`job:run`、`offline_access` |
| Token Auth Method | `client_secret_post` |

WebCodex 会公布 PKCE `S256`；Grok 可以同时使用 PKCE 与
`client_secret_post`。对于已经注册 client secret 的 WebCodex OAuth client，不要选
`none (PKCE only)`。`offline_access` 是用于 refresh token 的协议级 scope，不会写进
OAuth client 的 `allowed_scopes` 权限列表。MCP Protected Resource Metadata 会省略
`scopes_supported`，因为不同的预注册 client 可能有不同的 scope 上限。通用 MCP 客户端
因此可以省略 `scope`，由 WebCodex 把授权请求默认到该 client 已注册的
`allowed_scopes`。

打开 WebCodex Authorization 页面后，用希望 Grok 代表的用户当前有效 PAT
（`wc_pat_*`）登录。Runner token（`wc_agent_*`）不是用户登录 token。最终签发的
OAuth access token 会绑定到该用户，同时继续受 client 注册权限和本次请求 scopes
约束。

常见错误：

- **Save & Connect 为灰色：** Grok 在启动 OAuth 前要求 Client ID 已填写。
- **`invalid token`：** PAT 必须能在当前这台 WebCodex Server 的数据库中通过认证；
  不要使用 Runner token，也不要使用旧 Server/旧数据库遗留的 stale PAT。
- **`invalid scope`：** 每个 WebCodex permission scope 都必须包含在该 OAuth client
  的 `allowed_scopes` 中。普通 Grok MCP 接入不需要 `account:manage`；
  `offline_access` 作为协议级 scope 单独接受。
- **redirect mismatch：** redirect URI 必须与注册值逐字一致，包括路径和末尾 `/`。

Grok Custom MCP UI 与可用范围以 xAI 的
[Connector 文档](https://docs.x.ai/grok/connectors)为准。

## project-bound surface

Server 以 project-first Connector 配置（`canonical_connector`）启动时，MCP
`tools/list` 恰好包含以下十四个操作。这是 `webcodex run` 与 `webcodex share`
使用的 surface；没有 Connector context 的普通 hosted/self-hosted Server 默认暴露
`local_coding`（或显式的 `full_operator_runtime`）而不是这十四个操作：

```text
task_start
task_list
task_resume
files_list
files_read
files_search
code_navigate
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
code_impact
```

Connector context 已绑定配置的仓库。用 `task_start` 开始；不要调用项目发现、
session 或 runtime 工具，也不要在 prompt 里放 runtime project id。同一个聊天窗口
会自动延续当前仓库的工作。`task_list` 与 `task_resume` 是客户端无法再提供传输
窗口身份时的显式恢复工具。

## 黄金 coding 循环

```text
task_start
→ files_list
→ files_read / files_search / code_navigate / code_impact
→ edits_apply
→ checks_run
→ task_finish
→ task_review
```

- `files_list` 从 Git index 回答"项目里有什么"，因此被忽略的目录不会出现。猜测
  路径前先调用它。
- `code_navigate` 提供只读的语言服务器状态、document/workspace symbols、
  definition、references、diagnostics 与 hover。它只接受项目相对路径和从 1 开始的
  Unicode scalar 位置；Connector 负责选择已绑定的 executor project。参数按
  operation 严格区分：`status` 不带额外字段；document symbols 与 diagnostics
  使用 `path`；workspace symbols 使用 `query`；definition、references 与 hover
  使用 `path` + `line` + `column`。无意义的字段会被拒绝。normal、inspect 和
  read-only task 均可调用。
- `code_impact` 从项目相对源码位置执行一次有界 call hierarchy 操作。它支持
  `incoming`、`outgoing`、`both`，广度优先深度为 1 或 2，全局 edge 上限为
  1..100；只返回规范化的项目内 root、edge 和有界 call-site range。语言服务器
  不支持时会显式失败，不回退到 grep 或 AST。normal、inspect、read-only task
  均可调用。
- `edits_apply` 是受保护的编辑工具；`commands_run` 是需要 shell 的命令的有界
  逃生口。
- `checks_run` 做校验。使用稳定 `operation_id`，让精确重试复用同一操作。
- `task_finish` 生成稳定结果；由人工在本地用 `webcodex task accept <id>` /
  `webcodex task reject <id>` 接受或拒绝。模型永远不能接受自己的工作。

### 校验 recipe

`checks_run` 支持 `format`、`check`、`test` 与可选 `recipe` 枚举（`rust`、
`node`、`python`、`go`）。省略 `recipe` 时，从任务 `cwd` 相对位置最近的
`Cargo.toml`、`package.json`、`pyproject.toml` 或 `go.mod` 自动解析。Recipe 不
安装依赖、不修改 lockfile、不使用网络。缺少工具是 executor 失败；已启动的
validator 返回非零是断言失败。

| Recipe | 标记 | `format` | `check` | `test` |
| --- | --- | --- | --- | --- |
| Rust | `Cargo.toml` | `cargo fmt -- --check` | `cargo check --all-targets` | `cargo test` |
| Node | `package.json` | `format:check`/`format-check`/`check:format` 第一个 | `check`/`typecheck`/`lint` 第一个 | 精确 `test` |
| Python | `pyproject.toml` | 配置的 Ruff/Black | 配置的 Ruff/Mypy | 配置的 pytest |
| Go | `go.mod` | 不可用 | `go vet ./...` | `go test -json ./...` |

### 长校验会持久继续

`checks_run` 与 `commands_run` 使用 durable execution；工作仍在继续时，调用大约
8 秒后可能 quick-yield。在十四工具 Connector surface 上，用 `task_review` 的
`after_cursor` / `wait_ms`（需要输出时再加 `include_output_tail=true`）观察进度，
直到 execution 进入 terminal；需要停止时调用 `task_cancel`。不要为了轮询而重新
执行同一个操作。

更宽的 `local_coding` 与 `full_operator_runtime` MCP surface 才暴露
`job_status`、`job_log`、`validation_summary`、`stop_job` 等原始 Job 工具；这些
工具不属于十四个 Connector capability。

## 第一个安全 prompt

```text
Use the configured WebCodex project. Start a read-only task, read README.md,
summarize the project, review the result, and finish. Do not edit files.
```

这个 prompt 里不需要项目发现或 runtime 标识符。

## 读取与搜索边界

- `read_file` 是有界流式范围读取：`start_line`（默认 1）、`limit`（默认 2000，
  最大 2000），返回范围加上完整文件 SHA-256 与行元数据，以及用于继续的
  `next_start_line`。
- `read_files` 批量执行最多 8 次单文件读取，条目结果相互独立。
- `search_project_text` 是默认搜索工具（优先 ripgrep，工作量与字节均有界）；
  `search_project_texts` 批量执行最多 8 个查询。

失败返回只含项目相对路径的小型结构化错误——绝不包含绝对路径、命令或 Runner
stderr。

## 常见错误

| 错误码 | 含义 | 处理 |
| --- | --- | --- |
| `project_not_configured` | 没有 canonical setup | 运行 `webcodex setup` |
| `project_credential_invalid` | 私有 Project Credential 缺失或不匹配 | 恢复两个匹配的私有文件或重建 profile |
| `project_credential_rejected` | 可达 server 拒绝了该凭据 | 恢复与 server 匹配的凭据 |
| `workspace_unavailable` | 配置的 Git 工作区不可用 | 恢复工作区，再运行 doctor |
| `server_unreachable` / `agent_offline` | 项目 runtime 或 Agent 不可用 | 运行 `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | Agent 缺少 coding 能力 | 升级所有二进制 |
| `task_not_active` | 任务无法再变更或执行 | 开始新任务 |
| `execution_not_terminal` | Finish 被活跃/未知工作阻塞 | 审查/等待/取消 |
| `checks_required` | 普通任务尚未运行检查 | 调用 `checks_run` |
| `checks_stale` | 上次检查后工作区已变化 | 运行一次新检查 |

## 高级 runtime surface

在 project-bound Connector 之外，WebCodex 还可以作为多项目管理 ToolRuntime 运行，
提供 discovery、session、LSP、raw job 与 artifact 工具。那是面向运维者的高级
surface，不是 canonical project Connector，也不是普通 coding 的前提。

### ChatGPT 文件桥接

在暴露 artifact 工具的更宽 MCP operator surface 上，WebCodex 支持双向的
host-native 文件传输，不需要把完整二进制经由模型文本搬运：

- `import_conversation_files_to_project` 通过 ChatGPT host 的
  `openai/fileParams` 导入 1..10 个文件。它既适用于用户选择的当前会话附件，也
  适用于 host 能绑定为 file parameter 的本轮新生成文件。Control 端负责下载原始
  bytes，并通过现有有界 artifact write 路径提交；调用方不应自行构造下载 URL，
  也不应手工 Base64 转运这些文件。
- `export_project_artifact` 为一个有界 project artifact 创建短期、受认证的 MCP
  `ResourceLink` 并返回 metadata。`tools/call` 不包含完整二进制；host 通过
  `resources/read` 取得 binary resource。读取时会再次检查认证与当前
  project-read authority，并在返回 bytes 前重新验证 artifact metadata。
- resource URI 本身不是独立 bearer authority。Export handle 只是短期、
  process-local 的 presentation state；现有 project artifact 的大小、MIME、路径与
  authorization 边界继续生效。

`read_project_artifact` 仍然只是有界 chunk inspection API，不承担大文件下载。
DOCX/PPTX/XLSX 等 Office artifact 与 PDF 复用同一 artifact transport，因此在
支持这些 host 能力的 ChatGPT 中，可以在 project 与 host 之间直接传递，而不需要
模型手工搬运 Base64。

更宽的 coding surface 暴露 `work_on_project` 或 `start_coding_task` 时，请阅读
[Coding 工作流](CODING_WORKFLOW.zh-CN.md)，按 canonical mental model 区分 bootstrap
与 behavioral role，并遵循其中的 validation/closeout guidance。运维工具见
[架构](ARCHITECTURE.md)与 `webcodex` CLI。
