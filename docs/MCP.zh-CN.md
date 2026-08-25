# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex 通过 MCP endpoint，让 ChatGPT、Claude 与其他 MCP client 使用持有仓库机器上的
Runner。第一次接入先完成下面的客户端配置；protocol surface 与 scope ceiling 属于
reference，不是 onboarding 前置知识。

## ChatGPT Developer Mode：第一次接入

下面的 self-contained `share` 路径需要本地 WebCodex Server，只支持 Linux/macOS。
Windows 用户应改用 `webcodex connect <server-url>`，把 Runner 连接到已有的远程
Linux Server。

默认临时公网路径先安装
[`cloudflared`](https://developers.cloudflare.com/tunnel/downloads/)，然后执行：

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

CLI 显示 **WebCodex ready** 后：

1. 在 ChatGPT Developer Mode 创建基于 MCP 的 custom app。
2. 填入输出的 **MCP URL**。
3. 默认 share 选择 **Access token / API key**（Bearer token）。
4. 填入输出的临时 **Credential**。
5. 点击 **Scan Tools**。
6. 第一条可先说：`检查这个仓库并总结它的结构。先不要做任何修改。`

`share` 自己会完成 project setup。Hosted ChatGPT 无法访问 loopback-only 的
`webcodex run`，因此 `setup`、`doctor`、`run` 都不是 `share` 之前的必经步骤。ChatGPT
UI 文案可能随 rollout 变化；URL 与认证以 CLI 输出为准。Developer Mode、custom MCP app
和 write/modify action 是否可用，还分别受 ChatGPT 套餐、workspace 与管理员设置控制；
WebCodex scope 不会扩大这些客户端侧权限。

## Claude 与其他 MCP client

使用同一份输出的 `/mcp` URL 与认证值。Claude 中添加 custom connector 并粘贴 MCP URL；
其他 MCP client 同样使用 CLI 报告的 endpoint 与认证方式。仅本地 client 可用
`webcodex share --tunnel none`，不需要 `cloudflared`。

## 已有 Server

对于已经明确配置为 shared-key client 接入的 hosted Server，使用 operator 提供的 credential
走长期 shared-key 路径：

```bash
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` 会启动/复用本地 Runner，并在连接验证完成后输出 MCP URL 与 credential source。
这与刚完成 Docker bootstrap 的自托管 Server enrollment 不同：bootstrap administrator token
保留在 Server 机器上，由 Server 创建短期 pairing code，再在仓库机器上执行 `webcodex login`。
自托管见[部署指南](DEPLOYMENT.zh-CN.md)。

Bearer/shared-key 是最简单的路径。客户端要求 OAuth 时，使用 `share --auth oauth` 或
`connect --auth oauth` 并传入该客户端的精确 callback URL，然后按 CLI 输出配置。managed-user
OAuth 仍是独立的高级身份路径。

## Advanced / reference

### MCP model surface

project-first 的 `webcodex run` / `webcodex share` 使用 project-bound
`canonical_connector` surface。`webcodex connect <server>` 使用已有 Server 选择的 MCP
surface；没有 Connector 配置时默认是更宽的 `local_coding`，operator 还可显式选择
`full_operator_runtime`。这些名称描述 protocol/tool contract；第一次用户不需要先做选择。

Hosted client 需要公网 HTTPS：`share` 默认提供临时 Cloudflare Quick Tunnel，`connect` 使用
已有 hosted Server，自托管部署则提供自己的稳定 HTTPS origin。不要把 bootstrap/admin token、
Runner token 或持久 project-first Connector credential 当作公网 share secret。

### 内建 local MCP gateway

hosted WebCodex Server 可以继续通过同一个稳定 `/mcp` 暴露 Runner-owned 本地 stdio MCP provider。顶层 catalog 保持固定：有权限的 caller 只看到一个 `mcp_tool` meta-tool，而不是把每个 upstream tool 动态摊平到顶层。`mcp_tool` 支持 `list`、`describe`、`call`。provider id 与 upstream tool name 是逻辑 identity；Runner/process/provider-instance identity 与 schema revision token 都只在内部使用。成功 `describe` 会在 Server 记录有界 schema observation；`call` 只解析一次当前 exact Runner/provider instance，并在同一个 persistent provider session 上重新检查当前 tool schema。schema 已变化时不会发送 effectful call；provider replacement 则作为不同错误返回，也不会静默 retarget 或 replay。

不带参数 `server` 的 `mcp_tool(action=list)` 只报告已注册逻辑 provider id 是否可唯一路由（`resolvable` 或 `ambiguous`），不是 provider health check；`list(server=...)` 与 `describe` 才会真实与 provider 交互。对外 `/mcp` 的 2025/2026 protocol support 与 Runner-to-provider gateway V1 compatibility 是两层不同 contract：configured local provider 当前使用 [Runner 文档](RUNNER.zh-CN.md#provider-side-gateway-v1-compatibility)所述 bounded 2025-06-18 stdio tool subset，不承诺任意/最新 MCP server 的无损透明桥接。outer caller request `_meta` 不会转发给 local provider。

Runner 在 `[mcp]` 中配置 provider；不需要额外 daemon/sidecar、第二个公网 resource URL 或 per-provider ChatGPT App。local MCP 使用显式 `mcp:local` scope；direct shared-key、project、open-anonymous 与 legacy OAuth 默认权限都不会自动获得它。普通 hosted shared-key OAuth 必须使用 `webcodex connect ... --auth oauth --oauth-local-mcp` 显式加入“同一 shared-key Runner group 内当前及未来本地 MCP provider”的 class-level authority。ceiling 真正变化会撤销旧 grant 并要求重新走浏览器授权。因此以后新增/替换 provider 不需要新建 OAuth client/App，但只有此前明确 opt in `mcp:local` 的 credential 才能访问。

### OAuth2

当 managed/自托管 Server 启用 OAuth，或使用 `webcodex share --auth oauth` 时，MCP 客户端可以使用 authorization-code
流程而非静态 token。把精确的 ChatGPT callback URL 注册为 OAuth client redirect
URI；宿主提供 `offline_access` 时保持勾选（它是协议级 refresh-token scope，不授予
额外权限）。服务端 OAuth 设置见[部署指南](DEPLOYMENT.zh-CN.md#oauth2)。

对于 project-first share，授权页要求输入本次临时 Project share credential，并签发只带 `runtime:read`、`project:read`、`project:write`、`job:run` 的 `oauth2_project` 身份；它不会创建 managed user，OAuth token 也不能用于 Runner transport。Quick Tunnel 的 issuer URL 每次运行都会变化；如果 OAuth issuer 必须稳定，请使用 `--tunnel none --public-url https://...` 并在外部配置稳定 HTTPS proxy/tunnel。

对于已有 hosted Server，普通 `connect --auth oauth` 使用 shared-key OAuth bridge。OAuth client 以及 code/access/refresh grant 都绑定到 direct shared-key Runner/projects/jobs 所使用的同一个 `shared_key_hash`。direct shared-key bearer authority 始终固定为 `runtime:read`、`project:read`、`project:write`、`job:run`、`computer:read`、`computer:control`。fresh OAuth client 从完整 baseline 开始，但已有受保护 client 可以保留合法的窄 baseline subset。`--oauth-computer-permissions` 只在该现有 baseline subset 上追加 `computer:launch`、`computer:display_read`、`computer:pointer_control`、`computer:clipboard_read`、`computer:clipboard_write`，不会恢复缺失的 baseline scope。浏览器授权页仍默认全部未勾选，并且只能 grant 本次 OAuth request 实际请求且用户选择的 permission。Launch consent 要求 `computer:read` + `computer:launch`；display 要求 `computer:read` + `computer:display_read`；pointer 要求 `computer:read`、`computer:control`、`computer:display_read`、`computer:pointer_control`；clipboard read/write 也分别要求 baseline read/control prerequisite 与对应 optional scope。缺失的 request prerequisite 会 unavailable，而不是自动补齐，因此 consent、token projection 与 runtime scope gate 静态一致。ceiling 真正变化会撤销旧 grant。`account:manage`、`admin`、`job:detach`、任何 `agent:*` 与未来 scope 始终在 bridge 之外；`offline_access` 仍只是协议 scope。授权页按同一个在线 Runner 判断 capability，并在 POST 重新计算；这只代表 backend 当前可用，不保证 OS/native permission 或调用一定成功。runtime 中 OAuth `tools/list` 会隐藏 token scope 不足的工具，直接 `tools/call` scope gate 与 Runner/native 实时检查仍是最终 authority。managed-user identity 仍单独使用 `connect --auth managed-oauth`。

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
session 或 runtime 工具，也不要在 prompt 里放 runtime project id。在 Stateless
MCP 2026 中，每次 `tools/call` 对聊天/窗口连续性而言都是应用层无状态请求：
`task_start` 会返回 durable `task_id`，后续再次 `task_start` 会开始独立工作，即使
客户端仍发送旧的 `Mcp-Session-Id` 也不能形成隐藏连续性。要继续现有工作，必须显式
调用 `task_resume(task_id)`；需要恢复 task identity 时可使用 `task_list`。不要从同一
聊天、连接、credential、project 或 transport header 推断连续性。旧的 stateful
adapter 契约可以显式提供 stable `ClientWindow`，但这不是 MCP 的普遍属性，也不是
Workflow Session 或 model-context identity。

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
`job_log` / `observe_jobs` 返回的 observation token 必须原样回传：首次调用返回
有界 baseline；后续 cursor-aware 调用只返回新增日志；如果无法证明连续性（包括
Server 重启），`reset` 会返回有界 recovery tail。该 token 只是 observation
state，绝不是 Job identity、retry authority 或 execution identity。

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

只有已识别的 backend 明确报告搜索正常完成且无匹配时，空搜索结果才是肯定的
“无匹配”证据。backend 标识缺失或畸形、完成状态缺失、状态与输出不一致、
backend 失败、Agent 失败、超时、请求丢失及 provider 失败都会返回失败。
搜索失败保留兼容的 `code`，并增加有界的 `failure_stage` 与具体
`reason_code`。批量失败条目保留宽泛的 `reason_code`，同时通过
`failure_stage` 和 `detail_code` 保留单项搜索 provenance；成功条目仍保持
稀疏投影。

失败返回只含项目相对路径的小型结构化错误——绝不包含绝对路径、命令或 Runner
stderr、provider stderr 或任意 provider prose。

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

更宽的 model coding surface 暴露 `work_on_project` 时，请阅读
[Coding 工作流](CODING_WORKFLOW.zh-CN.md)，使用 canonical bootstrap / behavioral role
心智模型，并遵循其中的 validation/closeout guidance。`start_coding_task` 保留为 advanced
direct/API compatibility 入口，不再作为平级 model bootstrap，也不会由 MCP discovery 或
GPT Actions generic model schema 宣传。运维工具见
[架构](ARCHITECTURE.md)与 `webcodex` CLI。
