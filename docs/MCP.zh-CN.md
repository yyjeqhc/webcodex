# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex 通过 MCP endpoint，让 ChatGPT、Claude 与其他 MCP client 使用持有仓库机器上的 Runner。普通用户先选择**完整使用**还是**临时试用**即可；protocol surface、scope、credential taxonomy 都属于 reference，不是 onboarding 前置知识。

## ChatGPT：推荐的完整使用方式

日常使用推荐普通 Server + Runner。按照[完整使用指南](PERSONAL_SETUP.zh-CN.md)完成一次性登录和项目注册，并在同一次 `webcodex login` 中使用 `--print-mcp-config` 获取普通 HTTPS MCP 连接信息。公网 HTTPS、Cloudflare Tunnel 或 OpenAI Secure MCP Tunnel 只负责到达 Server，不改变这条完整开发路径本身的能力。

如果只是临时试用一个仓库，再使用下面的 `share` 路径。

## ChatGPT Host 侧的 Developer MCP 错误

如果 ChatGPT 返回：

```text
FORBIDDEN: This conversation does not support developer MCPs
```

该拒绝来自 ChatGPT Host 或会话级 Developer MCP 准入与路由层。它本身并不表示 WebCodex 永久关闭了开发者访问权限，也不能证明 Runner 已离线或项目注册已失效。

请独立检查 Runner 与项目状态。如果两者仍然可用，而且请求没有到达 WebCodex，请在 ChatGPT 允许 Developer MCP 的会话中重试。不要仅因出现这个 Host 侧错误就修改 WebCodex 配置。

## ChatGPT：临时 `share`

显式 `share` 支持 Linux、macOS 与 Windows，并由当前前台进程持有临时单项目环境。Windows x64 可直接使用 managed 默认 Cloudflare Quick Tunnel；固定版本 Cloudflare 没有官方 Windows ARM64 artifact，因此 ARM64 需要受信任的显式/`PATH` `cloudflared`。原生 OpenAI Tunnel 支持 Windows x64/arm64。

默认临时公网路径会复用显式指定/`PATH` 中的 `cloudflared`，否则由 WebCodex 自动下载并校验固定的 managed 副本，然后执行：

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

如果 ChatGPT 自身返回 `FORBIDDEN: This conversation does not support developer MCPs`
（或提示当前会话已禁用 developer MCP server），在有相反证据之前应先按 Host/conversation
admission 问题处理。如果 Host 根本没有 dispatch `get_runtime_status`，这段文本并不是
WebCodex tool result。修改 credential 或 Runner 配置前，先从独立路径确认 Server/Runner；
完整流程见[故障排查](TROUBLESHOOTING.zh-CN.md)。

## 同一 Server 的客户端策略

普通 MCP 与 Host 原生编排共用工具。客户端可在连接设置中为每个请求附加以下
HTTP header，也可由专用反向代理路由注入；不需要新增模型工具参数：

```http
X-WebCodex-MCP-Profile: direct
X-WebCodex-MCP-Budget-Secs: 20
```

Profile 只接受 `direct` / `host_code_mode`；省略时使用服务端
`WEBCODEX_MCP_HOST_PROFILE`，其默认是 `direct`。不会按客户端品牌、Window、
Session 或历史调用猜测，也不证明模型实际使用了程序编排。必须每次请求携带，
不是在 initialize / work_on_project 设置一次。如果服务端默认 host_code_mode，
普通客户端需显式发送 direct；无法设置 header 时可使用注入 header 的代理路由。

可选的正整数 Budget 只能缩短服务端已解析的 Host budget，不能放大服务端上限。
继续保留 5 秒返回余量与极小预算下已有的 1 秒等待下限。例如服务端 budget=55，
Direct 默认执行 handoff=10 秒、同步/观察上限=50 秒；Host Code Mode 保留
5 秒 handoff/观察切片。readiness 最多等待 45 秒，同时受 budget 减去返回余量约束，
不会被通用 5 秒 handoff 切片误缩短。空值、重复或畸形 header 在 dispatch 前拒绝，
不回显其原文；超大的合法整数 budget 会被截到服务端上限。

该预算只约束执行交接与 Job 观察/readiness 等待，不是所有 RPC 的统一 timeout。
命令 `timeout_secs`、Job 身份、权限、执行生命周期及内部编排上限不变。
`work_on_project.guidance_profile` 仍只覆盖当次指导文本；后续 context refresh
按其自身请求策略处理。API transport、tools/list、App admission、错误语义及
`WEBCODEX_MCP_TEXT_JSON_COMPAT` 均不改变。get_runtime_status 中的 mcp_host 继续
表示部署默认值，而非所有客户端当前请求的策略。

默认在当前 turn 完成工作：先做独立工作，依赖 Job 时进行一次有界
wait_for_job_readiness，必要时 observe_jobs 获取结果。deadline 后重新判断工作与
依赖，不机械续等、不重新派发执行、不假设自动开启下一 turn；无法完成时保留
pending Job 的精确身份。wait_for_job_terminal 仅用于显式建立的可选 continuation
workflow，不是阻塞等待，也不是普通 MCP 的前置要求。

## Claude 与其他 MCP client

使用同一份输出的 `/mcp` URL 与认证值。Claude 中添加 custom connector 并粘贴 MCP URL；
其他 MCP client 同样使用 CLI 报告的 endpoint 与认证方式。仅本地 client 可用
`webcodex share --tunnel none`，不需要 `cloudflared`。

如果 client 无法设置 Bearer header，可显式使用 `webcodex share --auth query-token`：
粘贴输出的敏感 `/mcp?token=...` URL，并选择 No authentication。这个 query 只接受当前
share 的 Project Credential，并不是 PAT/OAuth/shared-key 的通用 query auth。整条 URL
都必须当作 secret，因为 URL query 可能进入日志。

如果只需要 OpenAI 产品的私有 transport，创建/选择 Secure MCP Tunnel，导出
`CONTROL_PLANE_TUNNEL_ID` 与只授予 Tunnels Read + Use 的 Restricted
`CONTROL_PLANE_API_KEY`，然后运行 `webcodex share --tunnel openai`。ChatGPT 使用
Connection: Tunnel + No authentication；临时 WebCodex Bearer 留在本机，由
原生 Rust Tunnel client 在内存中注入。

对于通过 OpenAI Secure Tunnel 访问的长期 **loopback-only** Server，可以设置
`WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true`，从而信任由明确允许的本地 tunnel
credential 认证的 ChatGPT host-file rewrite。WebCodex Desktop 自 v0.4.2 起会为它自己管理的
本机 loopback Server 默认写入该值；已有显式配置不会被覆盖。该例外仅在 `WEBCODEX_ADDR`
解析为 loopback，且当前 credential 是普通 user API token，或 Desktop regular Tunnel 使用的
已配置 Server bootstrap credential 时生效。regular Tunnel 从本机 `WEBCODEX_TOKEN` 配置派生
该 credential，并只把它注入原生 Tunnel 的固定本地 Authorization；用户不应复制或暴露该
credential。独立/network-accessible Server 仍默认关闭，不应使用它替代 OAuth。

如果在 Windows 上使用普通独立 Server + Runner 并通过 OpenAI Tunnel 接入，或排查“本地 MCP 正常但 ChatGPT Connector 创建失败”的情况，见 [Windows + OpenAI Secure MCP Tunnel 深入实操](WINDOWS_OPENAI_TUNNEL.zh-CN.md)。它是深入配置/排障文档，不是普通用户第一次必须阅读的教程。

## 对话侧边栏中的 Work Result

`present_work_result` 在首次成功的 Project 操作后展示一次当前 Window 的工作结果。
支持对话侧边栏的 Host 可以用空参数对象调用公开的界面展示
`work_result_thread_panel` 入口。入口只恢复同一认证主体、同一 Host Window 中最近一次
成功展示的 Project，以及该次展示显式传入的业务 Session。其他操作或失败的展示不能
重新指定侧边栏目标。

侧边栏默认打开 Review，依次展示 Changed files / 按需 Diff、Session 检查结果和
sealed Final Changes。Activity / Collaboration 保留在次级页签；Project / Window /
Session 标识收进默认折叠的 Diagnostics。inline card 继续采用 Activity 优先的布局，
打开 Review 不会提前加载所有 Diff。

已打开的侧边栏在刷新时保留原 Project 和显式 Session 选择；Window 关联的 Session
证据不会成为刷新授权依据。重新打开才选择更新的成功展示记录。缺少稳定 Window
或展示绑定时拒绝打开，每次读取仍校验当前授权和快照边界。

Changed files 和 Final Changes 仅允许对已列出的路径按需查看 Full text。
当前文件来自固定的工作树快照，最终文件来自 sealed final tree，后续修改不会漂移。
每次显式加载最多 32 KiB，每文件累计最多 256 KiB；未完整或达到上限会明确标记。
已删除文件没有最终版本；二进制、非 UTF-8、符号链接与 submodule 不提供文本预览。
读取失败或快照过期不会转向实时路径。`.md` / `.markdown` 完整读取后才启用
Markdown：内嵌 markdown-it 支持标准 Markdown、表格、删除线，不承诺完整 GFM。
DOM 节点和属性采用允许列表，原生 HTML 作为文本，拒绝不安全 URL；链接不打开，
外部图片仅显示未加载说明，不自动请求资源。行或选区回传对话留待后续版本。

Markdown bundle 已提交在单 script App 资源内，纯 Rust 编译不需要 npm。
`npm ci --prefix frontend` 后用 `npm --prefix frontend run build:work-result`
重新生成；`node frontend/scripts/build-work-result-markdown.mjs --check` 比对确定性产物，
该检查同时接入 `check:dist`。

展示调用会保存这条窗口绑定，但不会启动 live Window activity；侧边栏入口和 App
刷新调用也不会启动 live Window activity。当前界面资源是
`ui://webcodex/work-result/v17`，旧版资源 URI 不再提供模板，避免缓存界面调用已退役的工具名。

界面展示入口保留默认的 model/App 可见性；App-only 桥接工具只返回数据，不声明
`ui.resourceUri`。ChatGPT 刷新工具时会拒绝声明界面资源的私有工具。更新 Server
的工具描述后，应先在 ChatGPT 插件设置中刷新已有 App 的工具，再验证新入口。

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

### Adaptive Runtime routing

WebCodex 只有一个 model-facing MCP runtime contract：**Adaptive Runtime**。Canonical `ToolDefinition` rank 决定 direct tools；普通 model-visible long-tail tools 通过 `call_runtime_tool` 调用；server-owned protocol capability 与 MCP App admission 可以为对应请求加入 hidden extension。启动时不再选择 model surface。`read_tool_manifest(tool_name=...)` 只负责 discovery，不会动态向 Host 注册一个新 tool。exact manifest 的 `route.primary` 给出首选 callable；普通 direct tool 还会给出经 `call_runtime_tool` 的 `route.fallback`，用于 Host 当前没有该 direct callable 的情况；显式 MCP App presentation tool 会标明 Apps enabled 时该 fallback 被禁止。direct/gateway 只改变 presentation，不会绕过目标工具的 authentication、Project authority、permission、Runner capability、Session 或 safety checks。

### Tool result framing

MCP tool 的 machine-readable 结果位于 `structuredContent`；`content` 只保留简短的人类可读或 protocol-native fallback。需要结构化字段的 client 应读取 `structuredContent`，不要解析文本。

普通 client 保持标准 MCP `isError` 语义。对于 `_meta["io.modelcontextprotocol/clientInfo"].name` 精确等于 `openai-mcp`（不限版本）的请求，MCP adapter 会对 WebCodex-owned canonical `ToolResult` failure 应用 **OpenAI structured-failure compatibility projection**：presentation 使用 `isError=false`，但 `structuredContent.success=false` 仍是业务结果的 authoritative truth，完整 output/error 也继续保留。当前 OpenAI Host 会把 `isError=true` 提升成异常而不暴露 `structuredContent`；该兼容层用于保留 machine-actionable failure/recovery data，Host 行为修复后即可移除。JSON-RPC/protocol error 仍然是真错误，第三方 MCP/Plugin passthrough result 也继续保留 provider 自己的语义。

部分 MCP host 不会把 `structuredContent` 暴露给模型；Claude Custom Connector 已观察到这种情况，即使 WebCodex 已成功执行工具并返回完整结构化结果。为这类 host 提供服务的 operator 可以显式设置 `WEBCODEX_MCP_TEXT_JSON_COMPAT=true`。开启后，普通 runtime tool result 仍以 `structuredContent` 为 canonical，同时把同一 JSON 值序列化到 `content[0].text`。该选项默认关闭，因为重复表示会增加 response/model-context 大小；protocol-native image/resource framing 与现有 App-only compatibility path 不受影响。

Result 中的 recovery 字段只描述下一次**显式**调用的安全建议，不授予 authority，也不会触发 hidden retry。尤其是 uncertain outcome，必须先 reconcile，再决定是否重复 effect。

### 内建 local MCP gateway

hosted Server 可以通过同一个 `/mcp` 暴露 Runner-owned 本地 stdio MCP provider。有权限的 caller 使用单一 `mcp_tool` 来 list、describe、call 已配置 provider；provider process/instance identity 与 schema-revision state 都留在内部。

在 Runner 的 `[mcp]` 中配置 provider。访问需要显式 `mcp:local` permission；hosted OAuth client 通过 `webcodex connect ... --oauth-local-mcp` opt in。Provider compatibility 细节见 [Runner](RUNNER.zh-CN.md#provider-side-gateway-v1-compatibility)。

### Managed SSH resource 接入

`manage_ssh_resource` 工具提供一条窄化的 Runner-local 命名 SSH resource 接入路径。`list`
观察安全逻辑名称并返回 opaque exact-Runner/revision binding；`register` 与 `remove`
消费该 binding，只修改 durable desired state。它们不会把旧 mutation 静默重定向到
replacement Runner，也不会在 uncertain outcome 后自动 replay。Raw SSH target 不会出现在
工具结果或 normal/full trace body 中。返回 `restart_required=true` 时，应先重启 Runner，
再重新 `list`，之后才能在 Workflow Session 中选择该资源。

该 surface 需要可选 `ssh:local` permission，不属于普通 hosted OAuth baseline；通过
`webcodex connect ... --oauth-local-ssh` 显式 opt in。Static/managed 语义与 PersistentShell
边界见 [Runner](RUNNER.zh-CN.md#ssh-会话资源高级)。

### OAuth2

启用 OAuth 后，MCP client 可以使用 authorization-code flow，而不是静态 token。注册 client 实际要求的精确 callback URL；host 要求 refresh-token support 时保留 `offline_access`；连接参数以 `share --auth oauth` 或 `connect --auth oauth` 的输出为准。Server 配置见[部署指南](DEPLOYMENT.zh-CN.md#oauth2)。

普通 hosted `connect --auth oauth` 中，Runner 保持原 hosted credential，MCP client 获得独立 OAuth credential。只有真正需要额外能力时才增加 `--oauth-computer-permissions`、`--oauth-local-mcp` 或 `--oauth-local-ssh`。已有 client 不会被静默扩权；真实权限变化要求重新授权。

Browser Use 的 shared-key OAuth delegation 需要在 `connect --auth oauth` 时显式指定 `--oauth-browser-permissions`，仅追加 `browser:read`、`browser:control`、`browser:launch`。默认 baseline 不包含 Browser scope；Browser 与 `--oauth-computer-permissions` 相互独立，也不使用 Computer consent checkbox。已有 client 不会自动扩权，历史窄权限仅追加显式选择的类别。scope ceiling 变化会撤销旧 grants 并要求重新授权；复用已启用 Browser 的 profile 时必须继续携带该 flag。

`read_tool_trace` 要求 credential-level admin authority 和 Stateless MCP 2026 trace-diagnostics capability。admin PAT 授权自己拥有的 managed-user client 后，OAuth connection 获得内部 admin authority；tools/list、manifest discovery、direct call 与 `call_runtime_tool` 均走同一 admin scope 检查。`admin` 仍不进入 requestable scopes 或公开 OAuth response，普通 OAuth 拒绝也不会提示申请 `admin` scope。shared-key/project-share OAuth 不具备该 authority。升级前的 ChatGPT/NewWebCodex connection 需要这样重新授权一次，此后 refresh rotation 保留 authority。参见[认证模型](AUTH_MODEL.zh-CN.md#oauth2)。

Project-first `share --auth oauth` 仍绑定本次临时 share 环境。Managed-user OAuth 是另一条高级流程（`connect --auth managed-oauth`）。OAuth credential 永远不能用于 Runner transport。

Credential 与 scope 模型见[认证](AUTH_MODEL.zh-CN.md#oauth2)。

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

## Project-scoped ordinary runtime

`webcodex run` 与 `webcodex share` 绑定一个已配置仓库，启动本地 Server + Runner，并暴露普通 Adaptive Runtime。临时或持久的 Project Credential 是 authentication / ProjectGrant 边界，不会选择第二套 capability surface。

典型 coding 流程是：

```text
work_on_project
→ read_files / search_project_texts / 按需语义导航
→ edit_project_files 或其它 canonical edit 工具
→ substantial work 进入真实状态后调用一次 present_work_result
→ 按需 run_process / run_shell / focused validation
→ read_workspace_changes
→ finish_coding_task
```

`work_on_project` 在普通 registered Project 上启动或精确恢复 Workflow Session。用户要求隔离且已经有 registered Project 时，使用 `work_on_project(project=..., mode=worktree)`：Server 先重新授权 source Project，Runner 再派生内部 managed placement，并把生成的 worktree 注册为另一个普通 Project。模型不需要重新推导 Runner path，也不能选择 managed destination。`client_id + path + mode=worktree` 只保留为 compatibility/bootstrap 入口，并继续接受普通 path authority 检查；没有隔离要求时，本地 `share` / `run` 直接使用 setup 已注册的 Project。

`present_work_result` 是 substantial coding 的一次性可视化层，不是 correctness primitive。挂载后，卡片通过 App-only state read 持续显示 Progress、Workspace、Validation 与 Review，无需模型轮询。`finish_coding_task` 在 non-blocking closeout 时把 eligible final changes seal 到 presentation cache，同一张卡随后发现这份 immutable snapshot，并按文件 lazy 展开 diff。tiny/read-only 工作应跳过这张卡，同一 Session 不应重复 presentation。

普通的有界 Project validation 优先使用 `project_validate`；项目自定义脚本与测试仍可能修改文件或访问网络。它只接受封闭的 `format_check` / `check` / `test` intent，以及可选的 `auto` / `rust` / `go` / `python` / `node` adapter hint；Runner 在自己注册的真实文件系统上解析最近且无歧义的 recipe，然后进入现有 structured validation Job。Rust 分别映射到 `cargo fmt -- --check`、`cargo check --all-targets`、`cargo test`；Go 映射到 `go vet ./...` 或 `go test -json ./...`。Go project validation 由 Runner 固定为 single-module 模式（`GO111MODULE=on`、`GOWORK=off`），因此 ambient module mode 或父目录 `go.work` 选择不会静默改变 gateway 的 workspace 语义；其 validation target identity 与 ambient Go specialist evidence 做 domain separation，因此不同 workspace 语义下产生的成功不会消解 gateway failure。独立的 `go_test` specialist 保持现有环境语义。可选 `scope` 只允许二选一的 portable package intent：有界 `packages`（1..8 项）把 Rust check/test 映射为重复 Cargo `-p` selector，把 Go check/test 映射为 project-relative package pattern；`all_packages=true` 则选择完整 project unit。Rust all-packages 只有在 Runner 证明 effective Cargo workspace root 与 registered Project root 完全一致后才映射为 Cargo `--workspace`，并把 Project 内 Cargo manifest graph 绑定到既有 re-plan fence；Go all-packages 保持 canonical `./...` single-module scope。带 scope 的格式检查会 fail closed。Python 使用 configured/profile/PATH 中已有的 Python 3 执行 pytest test（`python -m pytest --color=no -rA`），也支持项目本地 Ruff check/format_check（显式 target-version、禁用自动修复、缓存与字节码）；Python 的 scope 与 dependency policy 均 fail closed。Node 已支持 Runner 管理的脚本式 `check`（#995），并在 `package.json` 的 `scripts.test` 精确声明 `node --test`（或 `node --test --test-reporter=tap`）时支持独立的原生 TAP `test`。Node 测试由 Runner 使用固定 argv 直接启动，仅完整 TAP v13 汇总可证明测试数量，且需要独立能力 `project_validation_node_tap_v1`。Jest/Vitest、自定义 test 脚本、scope 和 filter 仍不支持。详见 [Node 原生 TAP 验证](implementation/node-native-tap-project-validation.md)。Python test 的 planning 与 Job 准入要求 `project_validation_python_pytest_v1`，Ruff check/format_check 则独立要求 `project_validation_python_ruff_v1`；所选工具缺失时明确 not-started，不自动安装或 fallback。环境、证据与同 Job 行为见 [Python/pytest validation](implementation/python-pytest-project-validation.md)。请求不会携带 arbitrary executable、argv、shell grammar、安装动作或显式修改源文件的意图；项目脚本和测试仍可能产生副作用；需要 ecosystem-specific 高级参数时继续使用现有 `cargo_*` / `go_test`。`project_validate` 依赖 additive `project_validation_v1` Runner capability；显式 `packages` 额外要求 `project_validation_package_scope_v1`，而 `all_packages=true` 要求 `project_all_packages_v1`；Go project-validation Job 准入还额外要求 `project_go_single_module_v1`，因此 Server 不会把 Go gateway plan 交给仍可能继承 ambient workspace 状态的旧 Runner。

Cargo all-packages provenance 是有界的 package-selection witness，并非完整构建输入快照。它要求 workspace 完全位于 registered Project 内，或独立 package 的祖先目录不存在 `Cargo.toml` marker；对 Project 外的祖先只探测 marker，不读取 manifest 内容。此 scope 不接受外部 path dependency，因为外部 manifest 的 `package.workspace` 可以把 Project 外 package 加入 workspace。相关 manifest／member／dependency alias 仍会被 fence，无关的非 manifest 链接会被忽略。无法证明的 topology 或超出边界上限时返回 `validation_scope_unavailable` / `build_scope_unavailable`；显式 package scope 和现有 specialist tools 保持各自契约。

普通的 portable Rust/Go 构建优先使用 `project_build`。它只接受精确 registered `project`、可选的 project-relative `cwd`、可选的 `auto` / `rust` / `go` adapter hint、portable `scope`（有界 `packages` 1..8 项，或 `all_packages=true`，二者不可同时出现）以及总 `timeout_secs`。Runner 解析最近且无歧义的 recipe 并拥有 canonical argv：Rust 的显式 packages 使用重复 `-p` selector，all-packages 只有在 effective Cargo workspace root 与 registered Project root 完全一致时才映射为 `cargo build --workspace`；Go 的显式 package pattern 直接传入，all-packages 映射为 `go build ./...`。Go project build 由 Runner 固定以 `GO111MODULE=on`、`GOWORK=off` 执行；完整 `go.work` workspace 语义不属于 v1 gateway，也不会从 Runner host 隐式继承。请求不能携带 executable、argv、shell、script、release/profile/target/features、原生 workspace/exclude flag、offline／network 策略或 artifact discovery contract；portable all-packages request 额外要求 additive `project_all_packages_v1` Runner capability；v1 检测到 Node/Python recipe 时 fail closed。

两个 gateway 都可选接受 `dependency_policy: {"mode":"locked"}`。这是 portable 的依赖解析保证，而不是宣称不同生态的原生 flag 完全等价：Rust build/check/test 映射为 Cargo `--locked`，Go build/vet/test 映射为 `-mod=readonly`。它要求 adapter 不得为了让本次操作成功而修复或改写项目级依赖选择状态，但**不**表示关闭 registry/module/toolchain 网络访问；offline／network policy 由 #962 作为增量 lifecycle 扩展跟踪。`project_validate(action="format_check")` 会拒绝该 policy，而不是静默忽略。携带 policy 的 planning 与 typed Job admission 都要求 additive `project_dependency_policy_v1` Runner capability。locked validation 使用独立的 durable validation target identity；省略 policy 的请求保持原有 argv 与 identity。

`project_build` 在 planning 与 typed Job admission 两处都要求 additive `project_build_v1` Runner capability；Go project-build Job 准入还额外要求 `project_go_single_module_v1`。Job 准入会重新规划 registered project/root、recipe、manifest/lock provenance、package scope 与 canonical invocation；若经过本地排队，worker 会在原生进程执行前再次核验同一个计划。计划 stale 时以 `not_started` 拒绝并释放 Job 槽位，不会静默重建或执行过期意图。长构建继续使用同一个 durable Job，并返回普通的 sparse pending continuation；pending 绝不授权 retry/redispatch。这个 closed gateway 限制的是 WebCodex 自己的命令权限，并不是 OS sandbox：Cargo/Go 构建逻辑以及项目 build script 仍可能产生自己的文件系统或网络副作用。超出 v1 contract 的构建继续显式使用 lower-level execution 工具。

Adaptive Runtime 可以把常用工具直接暴露，把 long-tail 工具通过 `call_runtime_tool` 暴露。direct/gateway 只影响 model exposure，不改变 schema validation、OAuth scope、Project authority、permission policy、Runner capability、Session fence 或 tool effects。

已删除的 ProjectConnector capability 名称（`task_start`、`files_read`、`edits_apply`、`task_finish` 等）不会作为 runtime 工具的 compatibility alias 保留。请使用当前 `tools/list` / `read_tool_manifest` 返回的 ToolRuntime 名称。

### 长任务使用 Job lifecycle

长时间 command 与 validation 使用 canonical WebCodex Job。发起调用返回 exact Job 后，用 `observe_jobs` 观察同一个 Job；只有 Job identity 确实丢失时才用 `list_jobs` 恢复，不要重复启动。Jobs 不再包装成 MCP Tasks，WebCodex 也不再 advertise 原 Connector-specific MCP Tasks extension。

ChatGPT/model turn 与一次 MCP observation request 都不拥有 Job 的生命周期。因此 Host 侧
出现 `Thinking stopped` / `Thinking failed`、request timeout 或 observation 中断，
本身不能证明 Job 已经停止。优先在原会话继续并重新观察已有 Job；identity 丢失时先恢复
Job inventory，再考虑 retry。不要仅仅因为 model turn 结束就重复 dispatch。符合条件的
terminal wait 可以提供 best-effort Host continuation，但 Host 接受 continuation 并不保证
新的 model turn 已经实际运行。详见
[Troubleshooting](TROUBLESHOOTING.zh-CN.md#长任务期间-chatgpt-显示-thinking-stopped--thinking-failed)。

## 第一个安全 prompt

```text
Use the configured WebCodex project. Inspect README.md and summarize the
project structure. Do not edit files or run commands.
```

这个 prompt 里不需要项目发现或 runtime 标识符。

## 读取与搜索边界

- `read_files` 是 canonical 有界范围读取工具，一次支持 1 到 8 个文件；单条目 batch
  就是单范围读取路径。每个成功条目返回完整文件 SHA-256 与有界行元数据；partial
  条目返回可直接执行的单条目 `read_files` continuation，且读取并非 snapshot-stable。
- `search_project_texts` 是 canonical 有界搜索面，一次支持 1 到 8 个独立查询（优先
  ripgrep，并保留现有有界 fallback）；单查询直接使用 one-query batch。

只有已识别的 backend 明确报告搜索正常完成且无匹配时，空搜索结果才是肯定的
“无匹配”证据。backend 标识缺失或畸形、完成状态缺失、状态与输出不一致、
backend 失败、Runner 失败、超时、请求丢失及 provider 失败都会返回失败。
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
| `server_unreachable` / `agent_offline` | 项目 Runner/runtime 不可用 | 运行 `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | 当前 Runner/runtime 缺少所需 coding capability | 升级所有二进制 |
| `project_registry_scope_denied` | Project-scoped credential 尝试扩张或修改其授权可见范围之外的 Project registry | 使用已可见的 Project，或使用 `work_on_project(mode=worktree)` |

## Adaptive Runtime extensions

同一个 ToolRuntime 通过一套 Adaptive Runtime contract 服务单项目、project-scoped 的本地 `share` / `run` 和多项目 hosted Server。project-scoped credential 改变可见性与 authority，不改变 model-facing runtime shape。特定 protocol capability 与 MCP App 可以 admission 额外的 hidden presentation/resource operation，但不会形成第二套 runtime surface。

### 项目级验证

`project_validate` 通过 Runner 上的现有适配器执行 Rust 的格式检查／检查／测试，
以及 Go 的检查／测试、Python 的 pytest 测试。`scope.packages` 表示 Rust/Go 的有界包范围。
`action="test"` 可使用 `test.filter`：Rust 为一个 libtest 子串，Go 为原生 `-run`
正则表达式（包含子测试的斜杠语义），Python 为原生 pytest `-k` 表达式，
上限 200 UTF-8 bytes，拒绝控制字符及选项形状的前缀；并非跨语言统一查询语法。
Go/Python 保留有意义的空格；空字符串或省略表示不加过滤。

```json
{"project":"agent:runner:repo","action":"test","test":{"filter":"selected_test","require_tests":true,"min_tests":3}}
```

`require_tests` 默认为 true，要求至少一个已证明执行的测试；false 且未设
`min_tests` 时保留原生成功，包括已证明的零测试或未知计数（未知计数仍未证明；source freshness 独立）。`min_tests` 为 1..1,000,000 的证据后置条件，
即使 require_tests=false 仍需满足；计数未知不表示零。check／format_check 不接受
该 test 块。任意显式 test 块均需 `project_validation_test_options_v1` 能力，
规划与 Job 准入各检查一次；省略时保持原有行为。完整参数不会变成任意 argv／shell。
长任务仍观察同一个 Job，不能因 Host 中断而重跑。

构建产物、修改源码的格式化、lint、Node 与其他 Python 生产适配器，以及更广泛的
workspace/exclude、offline／network 策略由 #962 跟踪；现有 cargo_*、go_test 与显式进程工具保留。

### ChatGPT 文件桥接

当当前 MCP protocol/host admission 允许 artifact capability 时，WebCodex 支持双向的 host-native 文件传输，不需要把完整二进制经由模型文本搬运：

- `import_host_files` 通过 ChatGPT host 的
  `openai/fileParams` 导入 1..10 个文件。它既适用于用户选择的当前会话附件，也
  适用于 host 能绑定为 file parameter 的本轮新生成文件。Control 端负责下载原始
  bytes，并通过现有有界 artifact write 路径提交；调用方不应自行构造下载 URL，
  也不应手工 Base64 转运这些文件。
- `inspect_project_artifact` 是首选的 Project → Model / Host 读取入口：
  `action=metadata` 用于 existence/size/MIME/digest/image/archive metadata；
  `action=inspect` 只读取一个有 snapshot fence 的有界 Base64 segment；
  `action=image` 通过 MCP native image delivery 给模型查看图片；
  `action=export` 用于把完整 artifact 交付给 host/user。不要循环 `inspect` chunk
  来完成整文件传输。
- `action=export` 继续复用现有 artifact export authority，创建短期、受认证的 MCP
  `ResourceLink` 并返回 metadata。`tools/call` 不包含完整二进制；host 通过
  `resources/read` 取得 binary resource。读取时会再次检查认证与当前
  project-read authority，并在返回 bytes 前重新验证 artifact metadata。Resource
  URI 本身不是独立 bearer authority；export handle 只是短期、process-local 的
  presentation state，现有大小、MIME、路径与 authorization 边界继续生效。

底层 `read_project_artifact_metadata` 与 `read_project_artifact_chunk` 继续作为
operator/gateway primitive 保留。旧的 `export_project_artifact` compatibility tool 已
删除；完整 host 交付统一通过 `inspect_project_artifact(action=export)` 暴露。DOCX/PPTX/XLSX
等 Office artifact 与 PDF 仍复用同一底层 artifact transport，因此在支持这些 host
能力的 ChatGPT 中，可以在 project 与 host 之间直接传递，而不需要模型手工搬运 Base64。

请阅读 [Coding 工作流](CODING_WORKFLOW.zh-CN.md)，使用 canonical `work_on_project` bootstrap / behavioral role 心智模型，并遵循其中的 validation/closeout guidance。运维工具见 [架构](ARCHITECTURE.md) 与 `webcodex` CLI。
