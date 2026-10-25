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

## ChatGPT Dot

ChatGPT Dot 目前是 best-effort，取决于 ChatGPT 为该 Dot 提供的插件能力。WebCodex 不把 Dot 定义成不兼容。不同账号或 Dot 的表现可以不一致：有的能正常调用 WebCodex，有的能发现插件，随后却要求转交给本地 Codex 或本地任务。本项目还没有一份可以依赖的公开说明，用来确定 Dot 何时可以直接操作本机资源，因此这里不把其中某一种表现写成契约。

如果插件可以发现，但调用没有发到 WebCodex，先删除该 Dot，再重新创建，并重新添加或授权 WebCodex 插件。差异可能来自插件状态、缓存，或创建 Dot 时保存的能力配置；重建有时会改变这一表现。保持现有的 ChatGPT 认证和权限设置；重建 Dot 不需要修改 WebCodex 凭据或配置。

如果重建后同一现象仍然稳定出现，把复现步骤补充到 [issue #955](https://github.com/yyjeqhc/webcodex/issues/955)。这些步骤用来判断 WebCodex 自身是否还能改善这种情况。

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

## Claude Code / Codex 直接连接

CLI 客户端直连本地或 hosted Server 时，在**每次请求**（包括 discovery）发送
`X-WebCodex-MCP-Surface: cli`。省略或设为 `adaptive` 会保持原有 ChatGPT 投影。
此设置独立于 `direct` / `host_code_mode` 等待策略，不改变认证和权限，也不在请求间
记忆 Project/Session。修改连接 header 后重新连接或刷新工具。空值、未知值和重复的
偏好 header 在 dispatch 前拒绝，不回显原文。

CLI 投影提供：

- 小型常用编码工具列表，直接暴露目录枚举、tracked-file glob、`find_definition` 和
  `find_references`。构建、验证、专项编辑等已准入工具仍通过 `read_tool_manifest` 与
  `call_runtime_tool` 调用，manifest 会反映当前路由。MCP App 工具不能经 direct 或
  gateway 调用；不接受 `_wc` 附带参数。
- WebCodex 结果使用文本：源码不转义，保留文件/行号、read revision。搜索匹配省略
  `read_hint`，被动 `peer_awareness` 也省略；部分失败、未知效果、Job 身份、覆盖范围
  限制和带版本保护的恢复/续读参数仍保留。原生 image/resource block 与第三方 provider
  passthrough 保持各自协议。CLI 下 WebCodex 结果覆盖 text-JSON 和 OpenAI structured
  failure 兼容投影：不返回 `structuredContent`，使用标准 `isError`。
- 简短的 `initialize.instructions`（stateless `server/discover` 也返回）。
  `work_on_project` 自动返回项目指令，复用同一次授权观察的快照。在原有 20 KiB context
  envelope 内优先保留完整已观察正文；过大或观察不完整时明确保留截断标记和 `read_more`。
  workspace 状态、Session 身份、权限、警告及阻塞原因继续返回，省略 ChatGPT 工作流文案。
  CLI 默认不附扩展目录，需要时设置 `include_extension_catalog=true`。启动仍会创建/恢复
  Workflow Session；结果不确定时先观察已有 Session，不要直接再创建。

可选 `X-WebCodex-MCP-Project` 接受 Server 签发的 `project_ref`（例如 `~p17`）或完整
`agent:<runner>:<project>` ID。reference 保留 principal/root 绑定，过期时拒绝；完整 ID
与显式工具参数使用相同解析语义，不接受简称。它为要求 Project 的工具（含 gateway）补充
省略的 `project`，也适用于新建 work_on_project。显式 project 优先；精确 Session 恢复和
`client_id`/`path` 注册保持自己的来源规则。不会填充 `list_jobs.project` 之类可选过滤器。
此 header 只允许 CLI surface，最多 512 字节，不持久化为连接权限。默认目标需由用户明确
选择，不代表授权。

Claude Code `.mcp.json` 示例（环境变量 URL 应使用实际报告的 endpoint）：

```json
{
  "mcpServers": {
    "webcodex-cli": {
      "type": "http",
      "url": "${WEBCODEX_MCP_URL}",
      "headers": {
        "Authorization": "Bearer ${WEBCODEX_MCP_TOKEN}",
        "X-WebCodex-MCP-Surface": "cli",
        "X-WebCodex-MCP-Profile": "direct",
        "X-WebCodex-MCP-Budget-Secs": "55"
      }
    }
  }
}
```

凭据存放在环境变量中，不写入配置文件。配置格式依据 Claude Code 的
[HTTP header / 环境变量说明](https://code.claude.com/docs/en/mcp#environment-variable-expansion-in-mcp-json)。
使用 OAuth 时省略 Authorization 项，通过客户端常规 OAuth 流程登录。

Codex `config.toml` 示例（替换示例 URL）：

```toml
[mcp_servers.webcodex_cli]
url = "http://127.0.0.1:8080/mcp"
bearer_token_env_var = "WEBCODEX_MCP_TOKEN"
tool_timeout_sec = 60
http_headers = { "X-WebCodex-MCP-Surface" = "cli", "X-WebCodex-MCP-Profile" = "direct", "X-WebCodex-MCP-Budget-Secs" = "55" }
```

[Codex MCP 配置说明](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)定义了 `http_headers`、
`bearer_token_env_var` 和 `tool_timeout_sec`。55 秒 Host budget 为客户端 60 秒 timeout
留出余量，但不是所有 RPC 的统一 deadline。timeout 不授权重试写入或重新派发 Job。
这些示例不依赖客户端优先读取哪种 content，也不假设它如何加载工具列表。

远程仓库可在本地 `CLAUDE.md` / `AGENTS.md` 中增加说明，例如：

```text
当前任务的仓库是 Runner dev 上的 WebCodex Project agent:dev:repo。
请用 webcodex-cli MCP 工具处理该仓库的文件、Git 和命令。
本地 shell/文件系统属于客户端机器，不一定是该 Project 所在的机器。
```

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
刷新调用也不会启动 live Window activity。新卡片使用当前 descriptor 公布的 Work Result
资源 URI，其身份由[canonical App registry](../src/mcp/app_registry/builtin.rs)维护。
刷新 Host schema 并重新打开缓存 reader，以加载当前 thread-panel、lazy-detail 和工具名
契约；旧版资源 URI 不再提供模板，避免缓存界面调用已退役的工具名。

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

同一个 canonical ToolRuntime 服务两种 MCP 投影。**Adaptive Runtime** 仍是包括 ChatGPT 在内的默认路径：canonical `ToolDefinition` rank 决定 direct tools；普通 model-visible long-tail tools 通过 `call_runtime_tool` 调用；server-owned protocol capability 与 MCP App admission 可以为对应请求加入 hidden extension。显式的请求级 CLI surface 只改变 discovery 与 presentation，配置见下文。`read_tool_manifest(tool_name=...)` 只负责 discovery，不会动态向 Host 注册一个新 tool。exact manifest 的 `route.primary` 给出首选 callable；普通 direct tool 还会给出经 `call_runtime_tool` 的 `route.fallback`，用于 Host 当前没有该 direct callable 的情况；显式 MCP App presentation tool 会标明 Apps enabled 时该 fallback 被禁止。direct/gateway 只改变 presentation，不会绕过目标工具的 authentication、Project authority、permission、Runner capability、Session 或 safety checks。

### 按客户端配置可选工作流工具

Adaptive Runtime（包括 ChatGPT）和 CLI 默认都精简工作流工具：面向模型的 Goal、
Agent 续跑和 Job 续跑入口需要显式启用。在 Server 的既有环境配置中设置
`WEBCODEX_MCP_TOOL_EXPOSURE`，值为闭合 JSON：

```json
{
  "adaptive": {"goals": false, "agent_continuation": false, "job_continuation": false},
  "cli": {"goals": false, "agent_continuation": false, "job_continuation": false},
  "oauth_clients": [
    {
      "client_id": "YOUR_REGISTERED_CHATGPT_OAUTH_CLIENT_ID",
      "adaptive": {"goals": true, "agent_continuation": true, "job_continuation": true}
    }
  ]
}
```

Server 在启动前一次性读取并验证配置。运维修改后需要重启 Server，并在 Host
刷新工具目录或重新连接。示例只为精确匹配的已认证 OAuth 客户端启用完整工作流，
所以线上 ChatGPT 无需自定义请求头。`X-WebCodex-MCP-Surface: cli` 仍按请求选择
CLI surface。客户端未提供的 surface 继承部署默认；提供的 surface 完整替换默认，
其中省略的布尔开关为 false。不会根据客户端名称、`clientInfo`、Window、Session
或之前的请求推断覆盖配置。

| 开关 | 控制的模型工具 |
|---|---|
| `goals` | canonical Goal 类别，包括准备、读取、checkpoint、更新及 `present_goal_plan`。 |
| `agent_continuation` | Agent 身份/设置、Endpoint 轮换/分离、bootstrap/Wake 消费、`wait_for_agent_events`、Task Endpoint 续跑及 Agent 续跑卡片/bridge。 |
| `job_continuation` | `wait_for_job_terminal` 及 Job 续跑卡片/bridge。同一 Job 的普通 readiness 和 observation 仍可用。 |

三个开关独立。Goal 驱动的自动续跑需要同时启用 `goals` 和 `agent_continuation`。
启用后，需要独立审核的操作在两种 surface 都得到 canonical standalone descriptor；
CLI 仍使用文本结果，没有 Apps 或 `_wc`。关闭分组会移除模型 descriptor 及对应
可选 App resource/bridge helper；exact manifest 报告 unavailable route，生成的
follow-up 不再指向不可用目标，direct/gateway 调用也不能启用该分组。工作台独立
保留既有 App-only Goal 读取/创建/更新控件：这些 descriptor 仍为
`ui.visibility=["app"]`，不属于模型路由，也不提供 gateway fallback。
这属于 MCP 暴露策略，不是凭据 scope，也不是 Runtime API/UI 的全局功能禁用开关。

配置最多 16 KiB、64 个唯一 OAuth 客户端条目；client id 非空、最多 256 UTF-8
字节，不含空白或控制字符。未知字段、重复客户端、JSON 格式或类型错误会拒绝启动；
诊断不输出输入值或 client id。运维状态仅报告 surface 默认和覆盖条目数量。
原有 scope 过滤、canonical business schema、所有权、generation/revision fence、
effect 检查和独立 Host 审核要求继续适用。`WEBCODEX_MCP_COMPACT_SCHEMAS` 控制
schema 详略，`WEBCODEX_MCP_APPS_ENABLED` 独立控制 App 准入，两者都不会隐式启用
工作流工具。暴露配置也独立于 `WEBCODEX_GOAL_WORKFLOW` 偏好和
`WEBCODEX_MCP_APP_RESUME_MODE` readiness 策略。

### Tool result framing

默认 Adaptive surface 的 MCP tool machine-readable 结果位于 `structuredContent`；`content` 只保留简短的人类可读或 protocol-native fallback。需要结构化字段的 client 应读取 `structuredContent`，不要解析文本。

对于标准客户端的命令失败（`isError=true`、`tool_failure=false`，且 `failure_kind=command_exit_nonzero` 或 `timeout`），`content[0].text` 还会包含可用的 stdout/stderr 尾部日志，让隐藏结构化错误的 host 仍能诊断命令。每路复用现有命令日志上限：12,000 个 Unicode 字符（最多 48,000 个 UTF-8 字节），另加简短标签；上游或呈现阶段发生截断时均标记 `[truncated]`。空且未截断的日志不附加。这项纯文本投影保留结构化错误的精简形式、执行状态、重试指引及成功回执。OpenAI 失败兼容模式沿用原有呈现；文本 JSON 兼容模式只输出 JSON 副本，不再额外附加日志段。

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

需要有意修改 Go 源码格式时，`project_format` 接受 `adapter="go"`，或配合显式 `.go` 路径使用 `auto`；一次选择同一最近且无歧义 `go.mod` recipe 下的 1–8 个文件。需要 `project:write`、`job:run`、`project_format_v1` 及新增的 `project_format_go_v1` Runner capability。已安装的 `gofmt` 使用固定空参数，从 stdin 读取私有源内容、向 stdout 输出候选；受保护写入和持久 `state_changed` 证据仍由 Runner 管理。不增加工作区遍历、`-w`/重写/简化参数、goimports、模块加载、安装或自动重试。`go.mod` 仅作有界 UTF-8 范围与文件身份凭据，不解析模块语义；保留 gofmt stdin 接受代码片段的原生行为，格式化不等于编译验证。工具缺失时明确失败。pending 或不确定结果须观察原 Job，不得重新派发。详见[有界项目格式化](implementation/project-format-lifecycle.md)。

普通的有界 Project validation 优先使用 `project_validate`；项目自定义脚本与测试仍可能修改文件或访问网络。它只接受封闭的 `format_check` / `check` / `lint` / `test` intent，以及可选的 `auto` / `rust` / `go` / `python` / `node` adapter hint；Runner 在自己注册的真实文件系统上解析最近且无歧义的 recipe，然后进入现有 structured validation Job。Rust 分别映射到 `cargo fmt -- --check`、`cargo check --all-targets`、`cargo clippy --all-targets -- -D clippy::correctness`、`cargo test`；Go 映射到 `go vet ./...` 或 `go test -json ./...`。Go project validation 由 Runner 固定为 single-module 模式（`GO111MODULE=on`、`GOWORK=off`），因此 ambient module mode 或父目录 `go.work` 选择不会静默改变 gateway 的 workspace 语义；其 validation target identity 与 ambient Go specialist evidence 做 domain separation，因此不同 workspace 语义下产生的成功不会消解 gateway failure。独立的 `go_test` specialist 保持现有环境语义。可选 `scope` 只允许二选一的 portable package intent：有界 `packages`（1..8 项）把 Rust check/lint/test 映射为重复 Cargo `-p` selector，把 Go check/test 映射为 project-relative package pattern；`all_packages=true` 则选择完整 project unit。Rust all-packages 可附加 `exclude_packages`（1..8 个精确 Cargo workspace member 名称）；未知名称或排除全部成员会在执行前拒绝。Rust build 与 check/lint/test 支持排除；Go/Node/Python 和 `format_check` 明确拒绝。新增能力 `project_workspace_exclusions_v1` 同时门控 planning 与 typed Job admission。Rust all-packages 只有在 Runner 证明 effective Cargo workspace root 与 registered Project root 完全一致后才映射为 Cargo `--workspace`，并把 Project 内 Cargo manifest graph 绑定到既有 re-plan fence；Go all-packages 保持 canonical `./...` single-module scope。带 scope 的格式检查会 fail closed。Python 使用 configured/profile/PATH 中已有的 Python 3 执行 pytest test（`python -m pytest --color=no -rA`），也支持项目本地 Ruff check/format_check（显式 target-version、禁用自动修复、缓存与字节码）；Python 的 scope 与 dependency policy 均 fail closed。Node 已支持 Runner 管理的脚本式 `check`（#995），并在 `package.json` 的 `scripts.test` 精确声明 `node --test`（或 `node --test --test-reporter=tap`）时支持独立的原生 TAP `test`。Node 测试由 Runner 使用固定 argv 直接启动，仅完整 TAP v13 汇总可证明测试数量，且需要独立能力 `project_validation_node_tap_v1`。复合/自定义 test 脚本、scope 和 filter 仍不支持；下面说明独立的原生 Vitest/Jest JSON profile。详见 [Node 原生 TAP 验证](implementation/node-native-tap-project-validation.md)。Python test 的 planning 与 Job 准入要求 `project_validation_python_pytest_v1`，Ruff check/format_check 则独立要求 `project_validation_python_ruff_v1`；所选工具缺失时明确 not-started，不自动安装或 fallback。环境、证据与同 Job 行为见 [Python/pytest validation](implementation/python-pytest-project-validation.md)。请求不会携带 arbitrary executable、argv、shell grammar、安装动作或显式修改源文件的意图；项目脚本和测试仍可能产生副作用；需要 ecosystem-specific 高级参数时继续使用现有 `cargo_*` / `go_test`。`project_validate` 依赖 additive `project_validation_v1` Runner capability；显式 `packages` 额外要求 `project_validation_package_scope_v1`，而 `all_packages=true` 要求 `project_all_packages_v1`；Go project-validation Job 准入还额外要求 `project_go_single_module_v1`，因此 Server 不会把 Go gateway plan 交给仍可能继承 ambient workspace 状态的旧 Runner。

空 `exclude_packages: []` 等价于省略，配合显式 `packages` 时也成立，且不要求排除能力。非空排除仅影响 Cargo workspace 顶层选择；被排除的成员仍可能作为依赖构建。这不是执行权限或依赖图过滤。


Cargo all-packages provenance 是有界的 package-selection witness，并非完整构建输入快照。它要求 workspace 完全位于 registered Project 内，或独立 package 的祖先目录不存在 `Cargo.toml` marker；对 Project 外的祖先只探测 marker，不读取 manifest 内容。此 scope 不接受外部 path dependency，因为外部 manifest 的 `package.workspace` 可以把 Project 外 package 加入 workspace。相关 manifest／member／dependency alias 仍会被 fence，无关的非 manifest 链接会被忽略。无法证明的 topology 或超出边界上限时返回 `validation_scope_unavailable` / `build_scope_unavailable`；显式 package scope 和现有 specialist tools 保持各自契约。

普通的 portable Rust/Go/Python/Node 构建优先使用 `project_build`。它只接受精确 registered `project`、可选的 project-relative `cwd`、可选的 `auto` / `rust` / `go` / `python` / `node` adapter hint、portable `scope`（有界 `packages` 1..8 项，或 `all_packages=true` 并可选 Rust 专用 `exclude_packages`，1..8 个精确 workspace member 名称；未知或排除全部成员时执行前拒绝；Go/Node/Python 拒绝排除）以及总 `timeout_secs`。Runner 解析最近且无歧义的 recipe 并拥有 canonical argv：Rust 的显式 packages 使用重复 `-p` selector，all-packages 只有在 effective Cargo workspace root 与 registered Project root 完全一致时才映射为 `cargo build --workspace`；Go 的显式 package pattern 直接传入，all-packages 映射为 `go build ./...`。Go project build 由 Runner 固定以 `GO111MODULE=on`、`GOWORK=off` 执行；完整 `go.work` workspace 语义不属于 v1 gateway，也不会从 Runner host 隐式继承。请求不能携带 executable、argv、shell、script、任意 release/target/features flag、原生 workspace/exclude flag、任意 network 策略或任意 artifact discovery contract；portable all-packages request 额外要求 additive `project_all_packages_v1` Runner capability；Rust exclusions 还要求 `project_workspace_exclusions_v1` 在 planning 与 typed Job admission 时通过。Python 要求 `pyproject.toml`，拒绝 package/all-packages scope，使用已有 configured/profile/PATH Python 解释器运行 `python -m build --no-isolation`。默认先构建 sdist，再从 sdist 构建 wheel；保留 frontend/backend dependency checks。依赖必须预先存在：adapter 不安装包、不创建环境。`dependency_policy.mode=locked` 保持同样的 no-install 语义。

Node 构建通过检测到的 npm/pnpm/yarn/bun 执行 `package.json` 中已有的 `build` script，复用 packageManager/lockfile 检测；缺失/冲突的 manager evidence 或缺失 script 均 fail closed。Node 拒绝 scope 与 dependency policy，并要求 `project_build_node_v1`；不会安装依赖或包管理器；Runner 覆盖 Corepack 网络/下载/auto-pin 配置，防止隐式引导包管理器。脚本及 lifecycle hooks 可能写文件或访问网络。成功只证明脚本完成；通过 `finish_coding_task(outputs=...)` 显式检查产物元数据，产物存在并不证明功能正确。

Node `project_validate(action=test)` 还支持 `scripts.test` 精确为 `vitest`、`vitest run` 或 `jest` 的已安装本地框架。Runner 使用固定官方 JSON reporter 参数与每个 Job 独占的新建私有报告文件（上限 1 MiB）。只有完整且一致的报告才能证明已执行测试数；不会从 stdout/stderr 推测数量。聚合 evidence 保留后删除报告。缺失、超限、无效报告、超时与取消均不能证明数量。JSON Job admission 要求 `project_validation_node_json_v1`。复合或自定义脚本返回 `structured_test_report_unavailable`，仍可用普通 recipe 执行；Node scope/filter 仍不支持。

两个 gateway 都可选接受 `dependency_policy: {"mode":"locked"}`。这是 portable 的依赖解析保证，而不是宣称不同生态的原生 flag 完全等价：Rust build/check/lint/test 映射为 Cargo `--locked`，Go build/vet/test 映射为 `-mod=readonly`。它要求 adapter 不得为了让本次操作成功而修复或改写项目级依赖选择状态，但**不**表示关闭 registry/module/toolchain 网络访问；#962 中的 Rust build/check/test 已新增可选的离线依赖解析策略。`project_validate(action="format_check")` 会拒绝该 policy，而不是静默忽略。携带 policy 的 planning 与 typed Job admission 都要求 additive `project_dependency_policy_v1` Runner capability。locked validation 使用独立的 durable validation target identity；省略 policy 的请求保持原有 argv 与 identity。

Rust `project_build` 另支持可选 `profile`、`targets: {"library": true, "binaries": ["server"]}` 与 `artifact_policy: "observe"`，要求新增 `project_build_rust_options_v1`；其他后端拒绝，省略时保留默认行为。观察有界、沿用原 durable Job，核验所选 Project 内的 Cargo 输出，缓存产物不代表本次创建、正确性或源码新鲜度。缺失、截断或不安全证据使观察失败，但保留原生退出码。见 [Rust 构建选择与产物观察](implementation/rust-build-selection-artifacts.zh-CN.md)。

Rust `project_validate(action="lint", adapter="rust"|"auto")` 使用 Runner 固定的 Cargo Clippy 命令 `--all-targets -- -D clippy::correctness`，采用独立的 `cargo_clippy` 工具/证据身份与 `lint` kind。Rust check/test 与 Node lint 身份保持不变；Node lint 仍只选择 `scripts.lint`，保留 check 证据。scope 支持 1..8 个排序去重 package，或经证明的 Cargo workspace（`all_packages=true`）。独立的 `dependency_policy: {"mode":"locked"}` 添加 `--locked` 并改变目标身份。Rust lint 支持 `dependency_resolution_policy=offline`，可独立组合 `--locked`；planning（含 auto）及类型化 Job 准入前均要求 additive `project_validation_rust_clippy_offline_v1`。缓存不足时直接失败，不在线重试；它仅限制 Cargo 依赖解析，不限制构建脚本或操作系统网络访问（[#1274](https://github.com/yyjeqhc/webcodex/issues/1274)）。显式 Rust lint planning 和所有已解析 Clippy Job 准入均要求 `project_validation_rust_clippy_v1`；auto planning 保留现有 Node 支持，并在执行前检查实际 backend 能力。Clippy 缺失时在验证命令启动前返回 `validation_tool_unavailable`，不会安装、接受任意 argv、回退 shell 或重新派发。Go/Python lint 明确不支持。队列准入与执行前重新验证 manifest/lock/workspace provenance，发生漂移则 fail closed。同一个持久 Job 负责执行和观察。退出码为零时可保留警告；非零、缺失退出码、超时或取消均不能产生成功 lint 证据。

Rust `project_validate(action="check"|"lint"|"test")` 可选传入 `dependency_resolution_policy: "offline"`，由 Runner 为 Cargo 生成固定的 `--offline`，仅禁止 Cargo 自身在线解析/获取依赖。它与 `dependency_policy: {"mode":"locked"}` 独立，可同时使用；省略时保持原有命令与验证身份。对非 Rust recipe 或其他 action，必须在执行前明确拒绝，不会悄悄回退在线运行；本地缓存不足时验证直接失败，不会自动在线重试。在线/离线请求有不同的规范命令与验证目标身份，原有持久 Job 和测试证据语义不变。**这不是操作系统级网络沙盒**：构建脚本和测试依然可能访问网络；`project_build` 也为 Rust 构建支持相同策略。


Rust `project_build` 可选 `dependency_resolution_policy: "offline"`，Runner 生成规范的 `cargo build --offline`，并可独立组合 `--locked`。省略此字段保持原有命令及执行身份。非 Rust 构建必须在运行前明确拒绝，依赖缓存缺失时正常失败，绝不自动重试在线构建。planning 和 Job 准入两处都要求 additive `project_dependency_resolution_offline_v1` capability；原有 artifact 观察和持久 Job 恢复语义不变。这不是 OS 网络沙盒，构建脚本仍可能访问网络。

`project_build` 在 planning 与 typed Job admission 两处都要求 additive `project_build_v1` Runner capability；Go project-build Job 准入还额外要求 `project_go_single_module_v1`。显式 Python planning 和每次 Python Job 准入要求 additive `project_build_python_v1`，旧 Runner 缺省为 false。Job 准入会重新规划 registered project/root、recipe、manifest/lock provenance、package scope 与 canonical invocation；若经过本地排队，worker 会在原生进程执行前再次核验同一个计划。计划 stale 时以 `not_started` 拒绝并释放 Job 槽位，不会静默重建或执行过期意图。长构建继续使用同一个 durable Job，并返回普通的 sparse pending continuation；pending 绝不授权 retry/redispatch。这个 closed gateway 限制的是 WebCodex 自己的命令权限，并不是 OS sandbox：Cargo/Go/Python/Node 构建逻辑以及项目 build script 仍可能产生自己的文件系统或网络副作用。超出 v1 contract 的构建继续显式使用 lower-level execution 工具。 Python 终态成功可包含 `build_artifacts`：默认 `dist/` 目录内最多 16 个现存普通文件（总计不超过 256 MiB），通过已有 artifact reader 记录 Project-relative path、大小、SHA-256 和 MIME。这些观察可包含旧构建文件；不证明本 Job 创建文件、package correctness 或 source freshness。原生退出码为 0 但未观察到 sdist 和 wheel 时，报告 `build_artifacts_missing` 并保留原生退出码。恢复时历史缺失 metadata 仍为 unknown，已知观察由同一个 Job 保留。符号链接、逃逸或敏感输出路径 fail closed。文件内容通过现有 artifact tools 读取或导出。

Adaptive Runtime 可以把常用工具直接暴露，把 long-tail 工具通过 `call_runtime_tool` 暴露。direct/gateway 只影响 model exposure，不改变 schema validation、OAuth scope、Project authority、permission policy、Runner capability、Session fence 或 tool effects。

在对应工作流分组启用后，九个持久 Agent 和 Goal 操作遵循更严格的 Host 暴露契约：`create_agent_identity`、`rotate_agent_continuation_endpoint`、`list_agent_identities`、`prepare_goal_workflow`、`get_goal`、`checkpoint_goal`、`update_goal`、`bootstrap_agent_conversation` 和 `consume_agent_wake` 分别以独立 Host tool 暴露，并携带canonical 输入 schema、准确的 effect annotations 和适用的 OAuth 要求。开始新的试用前，Host 必须正常审核这些工具。如果 Host 未加载或拒绝某个专用 descriptor，WebCodex 不会改用 `call_runtime_tool`；该 generic route 不是这些操作的 fallback。`_wc.control` 中对应的 Goal checkpoint、Wake 消费和 Goal 完成表达也会在任何主调用或 sidecar 副作用发生前被拒绝，必须使用独立的 canonical 工具。这一有限阶段不声称其他 Plugin 操作也已满足独立审查要求。此暴露策略不授予 authority，也不保证 Host 接受调用，并且不改变各操作现有的 authentication、OAuth scopes、permissions、ownership、lifecycle、replay-safety 或 effect checks。此前 Host 拒绝的原因仍未知；本契约不声称已经解决该问题。其他普通 direct tools 保留现有 gateway fallback，hidden App methods 仍保持隐藏。

已删除的 ProjectConnector capability 名称（`task_start`、`files_read`、`edits_apply`、`task_finish` 等）不会作为 runtime 工具的 compatibility alias 保留。请使用当前 `tools/list` / `read_tool_manifest` 返回的 ToolRuntime 名称。

### 长任务使用 Job lifecycle

普通 Job 观察保留一个可跨服务重启使用的 observation token，不再同时输出可能因
进程重启或缓存淘汰而失效的 observation ref。无日志游标的交接 token 表示首次
baseline，不代表日志重置；真正的日志代次变化和历史丢失仍保留 reset/retention
证据。生命周期或退出码已表达的通用 running activity 和普通命令结果摘要省略；
验证证据和有用的阶段变化继续保留。

process、script、shell 在受理时同时考虑工具上限和 Runner 最近注册的超时策略。
仅被截断且仍在后台的交接结果增加一行，例如
`timeout: 3600s (requested 99999; capped by runner policy)`；快速结束的命令不增加
说明。Runner 仍执行其当前策略。detached 保留 native argv 和按键恢复语义；虚拟
环境训练应直接调用该环境的 Python 路径，而不是 `sh -c`。

CLI 文本将文档符号显示为标识符位置和嵌套子项，将 Job/Project 清单与搜索文件/
计数结果按行显示，并合并单 Job 的等待和更新信息。Rust 文档符号现在等待索引
就绪，LSP 客户端声明支持层级符号。明确指定的文件行范围读完后不再建议越界续读；
确实被截断的读取仍提供带快照版本约束的后续调用。同步 process/script/shell 输出
每个流最多显示 200 行，保留截断标记，并说明如何将后续执行的完整输出写入项目文件。

Stateless MCP 2026 的执行工具可通过 `_wc.compact_execution=true` 显式启用紧凑执行结果。
它将稳定的 `output.execution` 控制事实与 `output.details` 分开（`observe_jobs`
按条目处理），同时保留日志、恢复和协作义务。只有 `outcome=passed` 才能证明执行成功；
工具接受请求或成功返回观察结果本身不能证明成功。该选项不会等待或重试命令。
支持的工具、gateway 放置位置、未知结果及 pending-chain 设计限制见
[Host execution control](experiments/host-execution-control.md)。

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

### 项目发现与恢复

`list_projects` 在 MCP 和 canonical Runtime API 中均默认返回 30 个项目，
显式 `limit` 归一化到 1–100。每次先鉴权，再应用精确 `client_id` /
`project` 和字面 `query` 筛选，最后按 Runtime Project ID 稳定升序排列，
再用 `offset` 分页。响应包含 `matched_count`、本页 `count`、实际
`offset` / `limit` 和 `next_offset`；末页的 `next_offset` 为 null，
`truncated` 表示后面仍有页。后续请求保持相同筛选，将 `next_offset`
作为新的 `offset`。

分页没有全局保留状态，每次重新获取当前授权可见的清单。两页之间发生登记、
删除或权限变化时可能重复或遗漏；需要重新遍历时从零开始并按精确 ID 去重。
这不是不可变快照。固定的 107 个匹配结果可以完整访问，不受单页 100 上限阻挡。

MCP CLI 与 Adaptive 默认 `summary_only=true`，保留精确身份、路径、名称、
来源、可用状态与活跃 Job 数。需要 revision、Shell/策略或烟雾测试能力时显式
设为 false；依赖 revision 的维护操作也应先读取详情。Canonical API 默认仍
返回详情，但数量同样有界。Console 保留显式完整查询的独立 2,000 行运维上限，
复用同一鉴权和筛选实现。CLI 运维清单最多遍历 20 个 100 行分页，并明确报告
截断；已知目标的连接、注销和烟雾检查使用精确 ID。

自动登记项目继续可被发现，也可用权威 `registration_source` / `kind`
元数据（例如 `auto_registered`、`managed_worktree`）进行查询。不会根据
路径猜测来源，不会删除登记、worktree 或分支，也不会推断最近使用时间。

项目解析错误最多展示 5 个授权可见候选，依次按精确短 ID/名称、忽略大小写的
相等、前缀/子串、最多 64 字符上的有阈值编辑距离、连接状态和稳定 ID 排序。
名称及输入展示有界，过长身份会被省略，不会截成另一个看似可执行的选择器。
`candidate_count` 表示本次解析范围内授权可见的候选池大小，
`candidates_truncated` 表示有省略。候选只列一次，并提供有界的
`list_projects` 恢复调用；模糊建议不会自动选择执行目标或扩大权限。

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

Stateless MCP 2026 只通过一个可选且封闭的 `_wc` envelope 接收通用、不可信的调用元数据。
根据工具，它可允许 `record`、`ack`、`ack_ref`、`resolve`、`reply`、`context`、`control`
和 `compact_execution`。这些仅属于 adapter 元数据，不会成为 canonical ToolCall
业务参数或授予权限。旧的根级 wrapper（如 `recording_session_id`、
`ack_session_message_ids`、`ack_ref`、`session_message_resolution`、`window_reply`、
`context_request` 和 `_control`）在 Stateless 2026 上会被拒绝；其他旧版/非 Stateless
传输保留各自契约。`call_runtime_tool` 只在外层 gateway 接收 `_wc`；嵌套目标的
`arguments` 必须是 canonical 业务参数，不能再次包含 `_wc`。

### 项目级验证

适配器支持范围、能力准入和副作用以[当前 coding 验证说明](#project-scoped-ordinary-runtime)为准：
Rust 格式检查／检查／测试、Go 检查／测试、Python pytest 测试和 Ruff 检查／格式检查，
以及 Node 脚本式检查和受支持的原生 TAP 测试。`scope.packages` 仅用于 Rust/Go 的有界包范围。
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

Rust/Go 构建使用当前 `project_build`；项目级验证不会执行修改源码的格式化。
更广泛的适配器、workspace/exclude 和 offline／network 策略仍由 #962 跟踪；
不要将其视为已实现能力。现有 cargo_*、go_test 与显式进程工具保留。

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

Artifact 上传按已知 `expected_bytes` 精确预留，零字节也合法；大小未知时预留
有界上限（默认 256 MiB）。每个 Project 仍最多允许 32 个活动上传和 512 MiB
预留字节。新上传将超出任一配额时，Runner 在上传锁内检查有界 reservation
索引，清理至少空闲 24 小时的上传记录和临时文件，再重新计算配额；不会递归扫描
Project，也不新增后台任务。最近的 chunk 写入会保留上传的有效状态；恢复证据
不可读或不一致时拒绝继续。Finish/Abort 成功清理后释放预留；传输结果不确定
不构成重放授权。已有 reservation 沿用原始额度恢复，记录格式不变。为兼容旧
Runner，Server 对空文件发送最小 1 字节上限；当前 Runner 对
`expected_bytes=0` 精确预留 0 字节。
降级到要求持久化额度为正数的旧 Runner 前，应先完成或终止这些零字节上传。

底层 `read_project_artifact_metadata` 与 `read_project_artifact_chunk` 继续作为
operator/gateway primitive 保留。旧的 `export_project_artifact` compatibility tool 已
删除；完整 host 交付统一通过 `inspect_project_artifact(action=export)` 暴露。DOCX/PPTX/XLSX
等 Office artifact 与 PDF 仍复用同一底层 artifact transport，因此在支持这些 host
能力的 ChatGPT 中，可以在 project 与 host 之间直接传递，而不需要模型手工搬运 Base64。

请阅读 [Coding 工作流](CODING_WORKFLOW.zh-CN.md)，使用 canonical `work_on_project` bootstrap / behavioral role 心智模型，并遵循其中的 validation/closeout guidance。运维工具见 [架构](ARCHITECTURE.md) 与 `webcodex` CLI。

## Goal 与 Agent 续接时序诊断

当前资源为 `ui://webcodex/goal-plan/v10`（App 10.0.0）与
`ui://webcodex/agent-continuation/v24`（App 24.0.0），旧资源别名被拒绝。
Goal Plan 和 Agent Continuation 卡片提供 **Enable timing diagnostics**，默认关闭。
每个 View 本地最多保留 64 条时序记录：计划与实际轮询延迟、单调时钟测量的 RPC
开始/完成/错误耗时、可见性与卸载。Agent 环形缓冲区还围绕现有 `ui/message`
记录 `host_dispatch_start`、`host_dispatch_complete` 或 `host_dispatch_error`、
`duration_ms` 耗时，以及封闭结果（`dispatch_accepted`、`delivery_unknown`）。现有
`host_dispatch_result` 在调用结束后记录同一结果。
错误仅含封闭失败类别；不记录消息正文、消费 token 或原始错误。Host 接受与精确 Wake
消费仍分别表达。关闭即清空，重新挂载从空记录开始。不增加 RPC、
浏览器持久化、模型消息、时间权威或调度器。Goal 诊断可附带已有的有界 `app_call_id`；
MCP 适配器验证并移除该字段后才进入 ToolRuntime。Agent 调用保留现有的关联方式。

运维可单独启用 `WEBCODEX_TOOL_REQUEST_TRACE=metadata`，用这些 ID 关联已有的请求
开始/完成记录。`goal_sync_timing` 记录封闭的核对原因、有效活动年龄、覆盖情况与
运行中请求数；执行到持久化观测检查时还记录观测年龄与 75 秒租约剩余量。
`continuation_identity` 用已有 Event/Wake/Attempt ID 关联创建、分发和精确消费。
这些记录复用有界且失败开放的 trace 写入器，不添加提示词、消息正文、消费 token
或原始 ClientWindow，也不复制到模型可见的 Goal 结果。现有 full trace 的正文与
隐私规则独立适用。租约剩余量为负表示观测已过期；缺失观测或部分覆盖不能授予唤醒权限。

Goal Plan 隐藏时目标轮询间隔为 30 秒，仍保留 75 秒观测租约与 10 秒 RPC 超时。
接近租约到期时按已过时间缩短间隔，过期尝试之后退避；增加的余量无法保证浏览器
不会继续节流。Agent 隐藏心跳仍为 15 秒。用户辅助的基线/候选版本证据流程见
[真实 Host 对照步骤](implementation/mcp-app-auto-resume-restoration.md#goal-hidden-cadence-baseline-and-candidate-comparison)。

单独获授权后，可在真实 ChatGPT 中绑定 Agent 载体并展示精确关联的 Goal，启用本地
诊断和 metadata trace，对比前台轮询与隐藏聊天/浏览器超过 75 秒的行为。将实际
定时器间隔、慢 RPC 和错误与 Server 的观测租约对照。过期卡片不得创建 Wake，直到
后续成功观测满足全部现有资格检查。隐藏/显示、卸载/重挂载及单独获授权的 Server
重启应保留单飞协调与每个有效活动周期仅一个逻辑 Wake。关联该 Wake 的分发与精确
消费；Host 接受不等于新模型回合。假时钟本地测试不能证明 ChatGPT 后台调度或无人确认续接。
