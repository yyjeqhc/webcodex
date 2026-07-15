# WebCodex Hosted Chat-first / Project-first 重构设计与开发计划

> 评审日期：2026-07-15
>
> 状态：v4 执行基线；前四轮个人本地主路径已落地：hosted connect、project-bound canonical surface、稳定 Task Result/本机人工决策、可复用写槽位与 Project 级 Cargo cache；下一阶段先增强编辑/读取/validation 工具，shared control plane/browser review 后置
>
> 目标：一个命令把本地项目安全接入线上聊天窗口，稳定执行工具调用、审查结果，并为多设备和多用户保留正确边界
>
> 范围：产品模型、核心架构和分阶段开发计划；不是发布承诺

## 1. 结论：WebCodex 是线上聊天的本地代码连接层

先明确产品边界：WebCodex 不是 Claude Code/Codex 的本地替代品，也不是一个等待接入模型的 agent framework。它服务的就是 ChatGPT 等线上聊天窗口，以及任何能够使用 Connector、MCP 或 GPT Actions/OpenAPI 的托管模型平台。

推理、规划、何时继续调用工具由线上模型负责；WebCodex 负责的是：

- 让线上模型在明确授权下找到一个本地 Project/Workspace；
- 提供小而稳定、效果可预测的代码操作能力；
- 在本地执行读、改、检查和受控命令；
- 持久化 baseline、变更、校验、审批和结果，使用户看得见发生了什么；
- 把平台协议、网络接入和核心任务状态分开。

WebCodex 当前最根本的问题不是依赖线上 GPT，而是把某次聊天窗口的反馈直接沉淀成新的聚合工具、summary 字段和兼容分支：

- 用户面对的是 server、agent、client id、runtime project id、session id、recording session、tool manifest 和 76 个工具；
- 外部窗口拥有任务控制权本来就是产品边界，但核心没有给它一个稳定的项目与任务协议；
- 每次线上反馈都会增加一个工具、一个 summary 或一个兼容字段；
- 最终形成了功能很多、状态很多，但任务结果仍然依赖模型有没有按预期编排的系统。

继续给现有 start_coding_task、finish_coding_task、session handoff 或 compact response 加字段，只会让补丁更完整，不会让连接层更轻、更稳定。

本计划建议做一次明确的方向切换：

> WebCodex 是“Hosted Chat ↔ Private Workspace”的安全连接与执行层。内部以“Project → Workspace → Task → Run → Result”为事实模型；对外通过 MCP、Connector 和 GPT Actions/OpenAPI 投影能力。它不拥有模型循环，也不决定下一步推理。

新的产品承诺应当是：

> 在项目中运行一个命令，即可让支持的线上聊天窗口安全访问这个项目；每次工具调用都有明确作用域和结果，窗口中断后仍能看到已经发生的操作和未完成状态。

窗口中断后 WebCodex 可以保存和恢复事实，但不会在没有线上模型的情况下继续思考或自动完成任务。消除“盲盒感”依靠更清晰的能力契约、baseline、event 和 review，而不是偷偷补一个内置 LLM。

这意味着：

1. 日常入口只有一个 webcodex。
2. 个人用户不需要先部署独立 server；一个本地 WebCodex runtime 集成 control plane、存储、workspace executor 和协议 endpoint，外部 tunnel client 由它监督。
3. Hosted client 的协议与网络 ingress 是两个独立 adapter，不把 ChatGPT/Cloudflare 特例写进 domain。
4. 用户先打开 Project，再建立 Connector/Task；不先理解 agent 和工具拓扑。
5. 写任务默认在隔离的执行工作区中进行，不直接污染用户当前 checkout。
6. WebCodex 拥有 task lifecycle、baseline、event、approval 和 result；线上窗口拥有推理与工具编排。
7. 默认模型面从 76 个历史工具硬切为一组小而明确的 workspace/task capabilities；具体数量由真实 hosted-client 验收决定，不把“恰好 N 个”当 KPI。
8. 用户、设备、connector、连接、逻辑项目和本地 workspace 必须拆成不同实体。
9. 不保留旧 session/tool/client_id wire compatibility；旧数据只归档，不双写。

## 2. 本次判断来自代码，而不是现有文档

本轮重新检查了 CLI、agent 注册、认证、项目解析、session、action audit、permission 和 coding-task 实现。以下代码事实决定了重构边界。

| 当前代码事实 | 直接后果 |
|---|---|
| webcodex、webcodex-agent、webcodex-cli 三个 binary 同时面向用户 | 用户必须先理解部署拓扑，才能打开一个仓库 |
| server up 只写 env，不启动 server；connect 只写配置，不启动或确认 agent | “up / connect”没有完成动词承诺 |
| connect 会把完整 Bearer 提示拼进普通输出 | 快速接入与凭据卫生冲突 |
| MCP 默认暴露 76 个 model-facing tools | 模型先学习 WebCodex 内部 API，才开始理解项目 |
| start_coding_task 有 15 个输入字段，并聚合 runtime、Git、规则、manifest、LSP 和权限信息 | 调用次数减少了，概念数没有减少 |
| coding_task.rs 明确只是 deterministic aggregate，不拥有 LLM loop | 这是应保留的边界；需要重做的是任务事实和工具契约，不是补 model loop |
| workflow session 存 JSON ledger，current session 是进程内 map，HTTP action audit 另存 SQLite | 同一任务存在三个状态面，只能靠 summary 拼接 |
| SessionRecord 没有可比较的 Git/worktree baseline | dirty workspace 中无法证明哪些变化属于本任务 |
| finish_coding_task 不关闭 session，close_session 是另一个低层工具 | “完成任务”没有一个原子终态 |
| require_approval 不是 pending，而是 require_approval_not_implemented 的立即拒绝 | 系统只有自动执行或拒绝，没有真实人工闭环 |
| server 没有持久化 Project、Workspace、Device 或 ProjectMembership | 项目只是在线 agent 上报的内存摘要 |
| runtime project id 是 agent:<client_id>:<project_id> | 逻辑项目身份被某台机器的某个 client 绑定 |
| client_id 同时用于 token 绑定、连接 lease、请求路由和 project namespace | 一个字段承担四种生命周期 |
| 同一 client_id 只允许一个在线 agent_instance_id | 同用户多设备或同配置并发在线会互相排斥 |
| 普通授权主要比较 owner username；shared key 以 key hash 分组 | 这不是可扩展的项目成员与设备授权模型 |
| console 是 runtime/agent 只读状态页 | 用户看不到 task、diff、validation、approval 和 accept/reject |

因此，真正需要替换的是状态所有权、身份模型、接入拓扑和能力契约，而不只是默认帮助、tool profile 或 compact payload。

## 3. 从 Codex 打开新项目的流程中借鉴什么

这里不把 Codex 当作功能清单，也不复制其模型循环。WebCodex 借鉴的是“一个 coding agent 打开新项目后需要什么上下文与安全边界”，再把这些能力提供给线上聊天窗口：

1. 先选定目录或 project，目录成为工作上下文。
2. 在执行前发现 Git root、分层 instructions、配置和权限。
3. 每个明确结果是一个独立 task，project 保存可复用上下文。
4. 复杂工作先建立 plan，再进入修改。
5. sandbox、approval 和项目配置是持久规则，不是每个工具结果里的提示。
6. diff、review、worktree 和任务恢复是一等能力。

官方参考：

- [Quickstart](https://learn.chatgpt.com/docs/quickstart)
- [Projects, chats, and tasks](https://learn.chatgpt.com/docs/projects)
- [AGENTS.md discovery](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
- [Best practices](https://learn.chatgpt.com/guides/best-practices)

WebCodex 应吸收上述顺序，但保持自己的产品边界：模型来自支持 Connector、MCP 或 GPT Actions 的线上平台；代码执行仍在用户控制的设备上；Task Result 可以跨窗口查看，并在共享 control plane 模式下跨设备查看。

不借鉴的部分同样明确：WebCodex 不提供 prompt loop、模型选择、context compaction、token budget、provider retry 或本地推理。对 Codex/Claude Code 的研究只用于改进 workspace 工具、安全语义和结果反馈，不用于把 WebCodex 变成另一个 coding agent。

## 4. 目标：打开一个新项目时实际发生什么

只想先打开本地项目和 review UI 时，个人用户在仓库中运行：

    cd my-repo
    webcodex

这只启动或复用一个本地 WebCodex runtime。多数首次使用可以直接运行下面的命令；`connect` 会先幂等完成 Project Open，不要求先单独执行 `webcodex`：

    webcodex connect chatgpt --via openai

或使用通用的公开 HTTPS ingress：

    webcodex connect mcp --via cloudflare --profile personal
    webcodex connect gpt-actions --via cloudflare --profile personal

`connect <target>` 是平台/协议 preset，`--via <ingress>` 是网络接入方式。两者必须正交，不能为每个平台复制一套 runtime、认证和工具实现。

### 4.1 两种运行拓扑

**个人 zero-managed-server 模式（默认）**

    Hosted Chat
        │
        ▼
    Ingress Adapter (OpenAI Tunnel / Cloudflare / direct)
        │
        ▼
    local WebCodex runtime (stdio or 127.0.0.1 only)
        ├── Project/Task store
        ├── WorkspaceExecutor
        └── Review UI

这里的“无需 server”准确含义是**无需部署和维护独立远程 server**，不是没有服务进程。Cloudflare 路径仍需要一个只监听 loopback 的本地 HTTP origin；OpenAI Secure MCP Tunnel 若以 stdio 启动 MCP 子进程，甚至可以不开放本地 TCP 端口。control plane、SQLite、executor、MCP/OpenAPI adapter 和 tunnel supervisor 都由一个 `webcodex` 产品入口管理。

**共享 control plane 模式（多设备/多用户）**

    Hosted Chat ── HTTPS/MCP ──> shared WebCodex control plane
                                      │
                                      ▼
                              DeviceConnection
                                      │
                                      ▼
                               WorkspaceExecutor

共享模式用于持久在线入口、跨设备统一 Task/Result 和项目成员协作。它是可选的部署拓扑，不再是个人首次使用的前置条件。两种拓扑复用同一 domain 和 use cases，只替换 store/executor/ingress adapter。

### 4.2 接入方式矩阵

| Ingress | 适用 client | 本地服务是否公开 | 定位 |
|---|---|---:|---|
| direct | 能访问 localhost/LAN 的 MCP client | 否 | 本地开发与诊断 |
| OpenAI Secure MCP Tunnel | ChatGPT、Codex、Responses API 等支持该能力的 OpenAI surface | 否 | OpenAI 平台的个人首选；仅 MCP，不假设可承载 GPT Actions |
| Cloudflare Quick Tunnel | 需要临时 HTTPS URL 的 GPT Actions 或经验证可用的非流式 MCP client | 是，随机 URL | 显式 `--temporary` 的试用/诊断路径，不是正式默认 |
| Cloudflare Named Tunnel | 任意支持远程 HTTP MCP、Connector 或 GPT Actions/OpenAPI 的平台 | 是，稳定 hostname | 跨平台的长期个人入口 |
| existing HTTPS | 已有反向代理或共享 WebCodex 部署 | 由用户架构决定 | 团队与自托管入口 |

[OpenAI Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels) 通过本地 `tunnel-client` 主动建立 outbound HTTPS，把 OpenAI 托管端的 MCP 请求转发给本地 stdio 或 HTTP MCP server，并支持中间 SSE。它需要 Platform 中创建的 `tunnel_id`、runtime API key 和对应组织/workspace 权限，所以 WebCodex 可以自动检测、生成本地 profile、启动和诊断进程，但不能承诺替用户自动创建账号权限。

[Cloudflare Quick Tunnel](https://developers.cloudflare.com/cloudflare-one/networks/connectors/cloudflare-tunnel/do-more-with-tunnels/trycloudflare/) 无需账号即可生成随机 `trycloudflare.com` URL，但官方将其限定为测试用途，并明确不支持 SSE。WebCodex 必须在真实 MCP handshake 中验证当前响应模式；需要 SSE 时直接拒绝 Quick Tunnel 并建议 OpenAI Tunnel、Named Tunnel 或 existing HTTPS，不能静默降级后让用户得到盲盒体验。[Cloudflare Named Tunnel](https://developers.cloudflare.com/tunnel/setup/) 提供稳定 hostname，适合通用 hosted client，但它形成公开 URL，因此 WebCodex 应用层认证始终必需。

OpenAI Tunnel 的 OAuth discovery 可以经过 tunnel，但 authorization server 不会被自动 tunnel。第一版个人 OpenAI 路径优先验证两种更小的边界：

1. `tunnel-client` 直接启动 project-bound stdio MCP 子进程；或
2. 转发到带每次启动 ephemeral proof 的专用 loopback endpoint。

这两种方式都把 tunnel profile 静态绑定到一个 Project/ConnectorGrant，不要求为了单用户模式额外公开 WebCodex OAuth server。是否能安全映射 OpenAI workspace/user identity，必须通过 Slice 1 的真实请求验证后再定，不能仅凭 tunnel control-plane 登录作推断。

之后的目标流程如下。

### 4.3 Discover：先确认工作上下文

WebCodex 自动完成：

1. 找到当前 Git worktree root；非 Git 目录使用显式 workspace root。
2. 读取本地 workspace binding；Git 项目通过 git rev-parse --git-path 解析实际 gitdir 后保存 binding，兼容普通 checkout 和 linked worktree，并避免写入被跟踪目录。
3. 从项目根到当前目录加载分层 AGENTS.md / AGENTS.override.md。
4. 检查 branch、HEAD、dirty state、冲突、submodule 和工具链 markers。
5. 发现项目内显式 actions/checks 配置；没有配置时只做保守推断。
6. 生成不含绝对路径和源码的 repo fingerprint，fingerprint 只用于发现候选项目，绝不作为授权身份。

### 4.4 Identity：自动建立当前人和当前设备

本地个人模式：

- 首次运行自动启动本地 control plane；
- 自动创建 owner account 和当前 device；
- 使用本地 socket 或内部 device credential；
- 不要求用户生成、复制或看到 Bearer token。

共享 control plane 模式：

    webcodex login https://code.example.com

- 通过浏览器或 device-code flow 登录一次；
- 为当前机器签发独立、可撤销的 device credential；
- device worker 永远不持有人类 PAT；
- 普通输出只显示设备名和 credential 前缀，不显示 secret。

### 4.5 Resolve Project：识别逻辑项目，而不是拼 runtime id

解析顺序：

1. 本地 binding 中已有 project_id：直接复用。
2. 同一 account 下 repo fingerprint 只有一个候选：自动建议并绑定。
3. 有多个候选：让用户选择，不能静默猜测。
4. 没有候选：创建新的 logical Project。

Project ID 由本地或共享 control plane 生成，与路径、设备、remote URL、client id 都无关。

### 4.6 Register Workspace：把当前 checkout 注册成项目的一个副本

当前机器上的实际 checkout 是 Workspace：

- Workspace 属于一个 Project 和一个 Device；
- LocalExecutor 或 device worker 在本机保存 workspace_id 到绝对路径的映射；
- 共享 control plane 默认只保存 workspace 名、repo fingerprint、capabilities 和状态，不需要保存绝对路径；
- 一个 Project 可以同时有 laptop、desktop、server 等多个 Workspace；
- 同一设备上的一个 executor 可以管理多个 Workspace。

### 4.7 Ready：只显示与开始工作有关的信息

首次 ready 输出应接近：

    Project      my-repo
    Workspace    macbook/my-repo
    Rules        3 instruction sources loaded
    Git          main @ 67c9594, clean
    Safety       isolated writes, approval for raw commands
    Runtime      local, ready

    Connector    not connected; run `webcodex connect`
    Review       http://127.0.0.1:8080/projects/wc_proj_.../tasks

不显示 agent lease、transport fallback、runtime project id、tool count、session binding 或 token。

若 hosted client 无法访问 localhost，CLI 必须给出可执行的 `connect` 路径，不能输出一个实际上不可达的 localhost 配置。已配置 connector 时，Ready 只显示 target、ingress、scope 和 health，不显示 tunnel credential、内部端口或完整公开 bearer。

### 4.8 Start Task：任务才是后续所有操作的 handle

外部 client 通过 task_start 创建 Task。后续只传 task_id，不再反复传 project、client_id、session_id 和 recording_session_id。

Task 创建时 WebCodex 自动：

- 固化 project、workspace、actor 和 instructions snapshot；
- 创建 Run；
- 为写任务建立隔离 execution worktree；
- 捕获 baseline；
- 取得 workspace mutation lease；
- 返回小型 context，而不是完整 runtime dump。

## 5. 新的领域模型

关系如下：

    User ──< Device ──< DeviceConnection
      │
      ├──< ProjectMembership >── Project ──< Workspace
      │                               │          │
      └──< ConnectorGrant >───────────┤          │
                                      └──< Task ─┴──< Run ──< Event
                                                   ├──< Approval
                                                   └──< Artifact

    IngressSession ──routes as──> ConnectorGrant

| 实体 | 生命周期与职责 | 绝不能再混入 |
|---|---|---|
| User | 人类身份、登录、项目成员关系 | device connection、workspace path |
| Device | 一台已注册且可撤销的机器 | user token、logical project identity |
| DeviceConnection | 远程 WorkspaceExecutor 的一次短生命周期在线连接 | 稳定设备身份、项目身份 |
| Project | 长期逻辑代码项目和协作边界 | client_id、绝对路径 |
| ProjectMembership | user 在 project 中的 role/capabilities | control-plane 全局 admin 角色 |
| ConnectorGrant | 某个 hosted client/preset 被授予的 user + project + capabilities | tunnel vendor credential、模型会话内容 |
| IngressSession | direct/tunnel 的短生命周期路由与健康状态 | 人类身份、Task lifecycle、授权事实 |
| Workspace | 某个 device 上 Project 的具体 checkout | Task lifecycle |
| Task | 用户希望得到的一个独立结果 | transport、进程 lease |
| Run | Task 在某个 Workspace 上的一次执行尝试 | 跨设备永久身份 |
| Event | 单调序号的事实记录 | 为某个 UI 临时拼出的 summary |
| Approval | 某个 action hash 的一次性人工决定 | 对整个任务的无限授权 |
| Artifact | patch、diff、validation report 等有 hash 的结果 | 未经边界检查的任意文件 |

### 5.1 ID 规则

新系统一次性使用新命名，不发送 aliases：

- wc_usr_*：用户
- wc_dev_*：设备
- wc_proj_*：逻辑项目
- wc_ws_*：workspace replica
- wc_cgr_*：connector grant
- wc_task_*：任务
- wc_run_*：执行尝试
- wc_evt_*：事件
- wc_apr_*：审批
- wc_art_*：结果 artifact

不复用 wc_sess_* 作为 task id，也不保留 agent:<client_id>:<project_id>。

### 5.2 核心身份不变量

1. 人类与设备授权只基于稳定 user_id、device_id、project_id 和 membership；hosted client 额外通过 project-scoped ConnectorGrant 进入。
2. username、hostname、display name、repo remote 和 path 都只是显示或发现信息。
3. device credential 只代表一台 Device，不代表用户会话。
4. DeviceConnection 和 IngressSession 每次连接生成新 ID；它们可以重连，但都不能成为稳定授权身份。
5. 所有 workspace 操作先解析 project membership，再解析 workspace routing。
6. tunnel_id、Cloudflare hostname 和公开 URL 只是路由信息，不代表用户或项目权限。
7. control-plane admin 不自动等于所有 Project 的 owner；恢复入口与日常项目授权分离。

## 6. 单用户多设备与多用户应如何工作

先区分“schema 支持”与“部署后真的可用”。个人本地 runtime 的 SQLite 和 Task Result 只存在于当前机器；Tunnel 只提供 ingress，不会把状态同步到另一台设备。若电脑关机，线上窗口也无法继续调用。WebCodex 不能把这一限制包装成多设备能力。

| 模式 | 单用户多设备 | 多用户 | authority |
|---|---|---|---|
| 个人本地 + Tunnel | 每台设备可独立接入；默认不共享 Task/Result | 不提供 | 当前设备的本地 runtime |
| 共享 control plane | 多台 Device/Workspace 同时在线并共享 Project/Task/Result | 通过 ProjectMembership 提供 | 持久化共享 control plane |

未来若增加 WebCodex 自有 relay/sync，可形成第三种拓扑，但不属于本计划。第一版要把共享 control plane 做成可选能力，而不是让个人首次接入也被迫部署它。

### 6.1 同一用户、多台设备

场景：同一账号在 laptop 和 desktop 都 checkout 了 my-repo。

- 两台机器分别注册为 wc_dev_laptop 和 wc_dev_desktop；
- 两个 checkout 分别是不同 workspace_id；
- 它们都属于同一个 project_id；
- 两个 device worker 可以同时在线，不复用 credential，也不争夺 client_id lease；
- 新 Task 自动选择当前设备的 Workspace；远程发起时选择唯一在线候选，存在多个候选时明确展示选择；
- Task 的 Run 固定到一个 Workspace，避免一半操作落到另一台机器；
- 从一台设备 handoff 到另一台设备会创建新的 Run 和新的 baseline，不伪装成同一个 filesystem 继续执行。

以上共享和 handoff 能力只在共享 control plane 模式成立。此时用户可以在任意已登录设备查看 Task Result；若 patch 的 base precondition 匹配，也可以在另一台 Workspace 上接受结果。个人本地模式只允许导出有 hash 的 patch/result 后手工带到另一台设备，不声称存在透明同步。

### 6.2 多用户

第一版不需要 organization、计费或复杂企业目录，但 schema 必须从第一天支持 ProjectMembership。

建议初始角色：

| Role | 读取结果 | 创建任务 | 在共享 workspace 执行 | 决定高风险审批 | 管理成员 |
|---|---:|---:|---:|---:|---:|
| owner | 是 | 是 | 是 | 是 | 是 |
| editor | 是 | 是 | 按 workspace policy | 可配置 | 否 |
| viewer | 是 | 否 | 否 | 否 | 否 |

角色只是 capability bundle；授权代码最终检查 capability，避免未来再增加大量角色字符串。

每个 Workspace 另有 execution visibility：

- owner_only：只有设备拥有者能让任务在此执行；默认值。
- project_editors：项目 editor 可调度；必须由设备拥有者显式开启。

这样“能看到 Project”不自动等于“能在别人的电脑上运行 shell”。

### 6.3 并发规则

- 一个 Workspace 可并发运行多个 read-only Run。
- 同一个 in-place Workspace 同时只允许一个 mutable Run。
- 隔离 worktree 模式允许多个 mutable Run，但每个 Run 有独立 execution root。
- accept patch 时重新检查目标 Workspace 的 base 和 changed-path preconditions。
- lease 过期只暂停或中断 Run，不删除 Task 和结果。

## 7. Task/Run 生命周期：用状态机替代状态聚合

### 7.1 用户可见 Task 状态

| 状态 | 含义 |
|---|---|
| open | 已创建，正在准备或执行 |
| needs_attention | 等待审批、设备离线、冲突或需要用户选择 |
| ready_for_review | Run 已结束，Task Result 已固化 |
| accepted | 结果已应用到目标 Workspace |
| rejected | 用户明确拒绝结果 |
| cancelled | 用户取消，未形成可接受结果 |

### 7.2 内部 Run 状态

| 状态 | 允许的下一步 |
|---|---|
| queued | preparing、cancelled |
| preparing | active、waiting_for_workspace、failed |
| active | waiting_for_approval、completed、interrupted、failed、cancelled |
| waiting_for_approval | active、failed、cancelled |
| waiting_for_workspace | preparing、interrupted、cancelled |
| completed | 终态 |
| interrupted | 新建 Run 恢复，旧 Run 不复活 |
| failed | 新建 Run 重试，旧 Run 不改写 |
| cancelled | 终态 |

所有 transition 只经过 TaskService；adapter、tool handler 和 console 不直接写状态。

### 7.3 写任务默认隔离

Git 项目的 writable Task 默认：

1. 从明确 base commit 创建受管 execution worktree。
2. 所有 edit、check 和 command 在该 worktree 内运行。
3. 用户当前 checkout 的 dirty changes 不会被模型覆盖。
4. finish 时生成 task patch、changed files、validation 和 warnings。
5. 人类通过 CLI 或 console 执行 accept 或 reject。
6. accept 在目标 Workspace 上检查 base/hash 后应用；冲突时进入 needs_attention，不静默覆盖。

这比在 dirty worktree 上事后猜测 attribution 更可靠，也把“允许模型执行”与“接受最终代码”分成两个不同决定。

显式 in-place 模式保留给可信个人工作流，但必须先捕获完整 baseline，并在无法证明 attribution 时标记 partial。非 Git 项目第一版只支持 in-place + bounded snapshot，不声称拥有 Git 级回滚能力。

### 7.4 client 中断不再等于任务消失

- hosted client disconnect 只结束 IngressSession，不删除 ConnectorGrant；
- Task 和 Run 事件已经持久化；
- 超时后 Run 标记 interrupted，Task 保持 needs_attention；
- 新窗口可以读取 task_review 并创建恢复 Run；
- 没有 finish 时也能看到 incomplete result，不靠模型最后一句话补救；
- WebCodex 不会在断线后自行继续模型推理，只会让已经提交的本地原子操作安全结束或进入可判定的 interrupted 状态。

## 8. 默认 hosted-client 能力面：小而明确，不追求固定数量

“76 个太多”不自动推出“恰好 7 个就正确”。上一版把 read/search/list/symbol/diff 塞进 `task_inspect`，仍然要求模型学习一套嵌套 mini-protocol，只是把 God API 藏进了参数。新的原则是：一个 tool 对应一个可解释的意图，名字、schema 和副作用单独可见；允许同语义的 bounded batch，不把不相关操作塞进 `operation` 枚举。

第一轮待验收的 canonical capability surface 是以下 8 个候选工具：

| Tool | 职责 | 关键约束 |
|---|---|---|
| task_start | 创建 Task/Run，返回精简 context | 只接受 goal、mode、可选 project/workspace |
| files_read | 读取一个或多个已知文件/range | 只读、bounded、路径相对 Run root |
| files_search | text/glob/symbol discovery | 明确 query kind、分页、结果上限 |
| edits_apply | 原子 structured edits | 必须带 file version/hash precondition |
| checks_run | 运行项目声明或安全推断的 checks | 默认不接收任意 shell 字符串 |
| commands_run | 高级 escape hatch | 受 policy/approval；只能在 Run root |
| task_review | 读取 events、diff、validation 和 warnings | 增量 cursor；不返回 giant aggregate |
| task_finish | 原子结束 Run 并固化 Task Result | 自动 capture diff/check state，进入 ready_for_review |

这 8 个不是永久 wire contract。Slice 1 和后续 tool-surface slice 必须分别在真实 MCP Connector 与 GPT Actions 中记录：工具选择错误、往返次数、schema 拒绝、payload、任务成功率。只有有证据时才拆分或合并；不因某一次聊天回复临时加 façade。人类操作 accept、reject、approve、deny、device revoke、member manage 不属于模型工具。

### 8.1 能力契约

- 每批数量、字节数、文件数和耗时有上限；
- 路径始终相对 Run root；
- edit 必须有 precondition，并返回逐文件结果；
- checks 优先使用项目声明的 action；
- raw command 只在显式 advanced policy 下存在；
- 长响应使用 cursor 和 artifact reference；
- 每个 consequential call 先写 intent/event，再执行，并返回稳定 operation_id；
- tool description 只说明事实、限制和错误恢复，不嵌入“下一步应调用什么”的脆弱 prompt orchestration。

### 8.2 Project context 不应是一个必调工具

连接可以绑定默认 project，task_start 会自动返回：

- instructions source 与 digest；
- workspace/Git 摘要；
- 可用 checks/actions；
- safety policy；
- 当前 blocking condition。

较大的 instructions、timeline 和 artifacts 通过 MCP resources 或 task_review 按需读取。模型不需要先 list_agents、list_projects、runtime_status、tool_manifest 再开始任务。

### 8.3 统一响应形状

所有 task tools 使用同一 envelope：

    ok
    task_id
    run_id
    event_cursor
    data
    warnings
    blocking

错误统一为：

    code
    message
    retryable
    user_action_required
    suggested_action

GPT Actions 的 flattened fields、compact mode 和 OpenAPI operation 限制只存在于 protocol adapter，不进入 domain model。MCP resources、OpenAPI discovery 和 Connector metadata 可以有不同投影，但必须来自同一 capability registry 和 schema tests。

### 8.4 从默认模型面删除

以下能力不是全部删除实现，而是从 model-facing surface 移除或重做为内部 service：

- runtime_status、list_agents、list_projects、tool_manifest；
- start_session、close_session、bind_current_session 和 message board；
- recording_session_id 与 current-session fallback；
- 低层 artifact chunk/upload/checkpoint 管理；
- 多套 edit aliases 和 compatibility patch 入口；
- 独立 git status/diff/hygiene/handoff aggregates；
- agent 注册、job transport 和 admin/ops 工具；
- 任意项目注册和 client_id 选择。

## 9. 新框架：Task Kernel 与 adapters 分离

建议目标分层：

### 9.1 Domain

纯 Rust 类型与状态机，不依赖 HTTP、MCP、SQLite 或 agent transport：

- Identity：User、Device、ProjectMembership、ConnectorGrant；
- Project：Project、Workspace、WorkspacePolicy；
- Task：Task、Run、state transitions；
- Approval：ActionIntent、ActionHash、Decision；
- Result：Baseline、ChangeSet、ValidationResult、TaskResult。

### 9.2 Application

用例层拥有业务顺序：

- OpenProject
- RegisterWorkspace
- StartTask
- ReadFiles
- SearchFiles
- ApplyEdits
- RunChecks
- RunCommand
- ReviewTask
- FinishTask
- AcceptTask
- RejectTask
- ResumeTask

任何 adapter 都只能调用这些 use cases。

### 9.3 Ports

核心只依赖接口：

- IdentityRepository
- ConnectorGrantRepository
- ProjectRepository
- TaskRepository
- EventStore
- WorkspaceExecutor
- ApprovalBroker
- ArtifactStore
- Clock / IdGenerator

### 9.4 Adapters

- Store adapters：本地 SQLite 或共享 control-plane store；
- Executor adapters：同进程 LocalExecutor 或远程 DeviceExecutor；
- Protocol adapters：MCP、Connector metadata、GPT Actions/OpenAPI；
- Ingress adapters：direct、OpenAI `tunnel-client`、Cloudflare `cloudflared`、existing HTTPS；
- Connector presets：ChatGPT 或其他平台的 capability/profile 配置与安装提示；
- Human adapters：CLI 与 browser console 的 review/approval 操作。

Protocol 与 ingress 必须分包。`McpProtocolAdapter` 不知道请求来自 OpenAI Tunnel 还是 Cloudflare；`OpenAiTunnelIngress` 不知道 `files_read` 的业务语义。Connector preset 是薄的组合配置，不得复制 tool handler。

### 9.5 不引入 TaskDriver 或 ModelProvider

WebCodex 不驱动线上模型，因此 `TaskDriver` 会制造错误抽象。实际调用方向始终是：

    hosted model -> protocol adapter -> application use case -> WorkspaceExecutor

CLI/console 是人类控制面，也不是 model driver。代码中不建立 `ModelProvider`、`PromptRunner`、`AgentLoop` 或 provider-neutral completion interface。

Domain event 不包含 recommended_next_tool、compact_startup、flattened_args、tunnel_id 或 public_url 等 client/ingress-specific 字段。每个 adapter 自己投影需要的 payload；若某个平台需要提示下一步，提示属于该 connector preset 的版本化 metadata，不进入任务事实。

## 10. 状态存储：一个事件事实源，多个小投影

### 10.1 取代三套 session 状态

删除以下并行概念：

- JSON workflow session ledger；
- process-local current session binding；
- 独立 action_sessions/action_events 汇总表。

改为一个 SQLite TaskEventStore：

- 每个 Task 内 event sequence 单调递增；
- event 有 task_id、run_id、actor、type、timestamp 和版本化 payload；
- state transition 与 consequential action 在同一事务边界写入；
- task summary、timeline、result、approval inbox 都是 projection；
- projection 可重建，不是新的事实源。

### 10.2 建议的 vNext 表

- users
- devices
- device_credentials
- projects
- project_memberships
- connector_grants
- ingress_profiles（只存非 secret 配置和 provider reference）
- workspaces
- tasks
- task_runs
- task_events
- workspace_leases
- approvals
- artifacts
- human_sessions / oauth_clients / oauth_tokens

DeviceConnection 与 IngressSession 主要保留在内存 connection manager 中；连接/断线只更新 health/last_seen projection。它们不是 Task 事实，除非确实导致某个 Run interrupted。

OpenAI runtime API key、Cloudflare tunnel token 和 WebCodex credential 不进入普通 SQLite payload、CLI 参数或 event。优先使用系统 keychain/credential helper；最低限度使用权限收紧的 provider 原生配置或环境注入。

### 10.3 Artifact 策略

为了支持 restart 后 review，本地或共享 control plane 需要持久化有界 Task artifacts；共享模式再把这些 artifacts 用于跨设备 review：

- task-attributed patch；
- file/change manifest；
- validation report；
- review summary；
- approval record。

默认不持久化完整仓库、任意 shell stdout、环境变量或 secret。Artifact 有 sha256、size、content type、retention 和 sensitivity classification。敏感路径在 executor 侧先拒绝；大型输出只保存 bounded sanitized excerpt。

## 11. 执行层：个人模式不再强迫 server + agent

`WorkspaceExecutor` 有两种实现：

- **LocalExecutor**：个人默认；与 control plane 同进程，直接操作已打开的本地 Workspace，不存在 agent 注册、lease 路由或第二套 CLI。
- **RemoteDeviceExecutor**：共享模式；设备侧 worker 连接持久化 control plane，在指定 Workspace 上执行。

只有 RemoteDeviceExecutor 需要内部 device protocol。它不再上报“一个 client 拥有哪些 project path”，而是作为已认证 Device 的 workspace executor，协议收敛为：

- device.register / connection.heartbeat
- workspace.register / workspace.describe
- run.prepare / run.release
- fs.inspect
- fs.apply_edits
- check.run
- command.run
- baseline.capture
- result.diff
- patch.apply

路由键是 workspace_id + device_connection_id，不是 client_id。LocalExecutor 也必须经过相同的 WorkspacePolicy、TaskService、ApprovalBroker 和 baseline 逻辑，不能因为同进程就绕过安全检查。

本地或共享 control plane 是以下信息的 authority：

- User、Device、Project、Membership；
- Task/Run state；
- approval；
- event ordering；
- artifact metadata。

LocalExecutor 或远程 device worker 是以下信息的 authority：

- workspace_id 到本地 path 的映射；
- filesystem/Git 实际状态；
- execution root；
- baseline 内容；
- command process 与资源限制。

个人模式可以在实现上把多个组件链接进同一 binary，但模块边界仍保留。共享模式只是替换 Executor 和 Store adapter，不分叉一套“team product”。

## 12. 认证和安全模型

### 12.1 五类身份/credential，不能混用

| Credential | 代表谁 | 使用面 |
|---|---|---|
| Human session / PAT / OAuth | User | CLI、console、成员管理与 review |
| ConnectorGrant proof | 某个 user + project + capabilities | hosted client/platform 的 MCP/Action 调用；stdio 模式可由受管进程绑定而不使用 bearer |
| Ingress provider credential | tunnel 进程对 OpenAI/Cloudflare 的权限 | 只建立路由，不自动成为 WebCodex User/Project 授权 |
| Device credential | Device | 共享模式的 remote executor transport |
| Ephemeral connection proof | DeviceConnection/IngressSession | 单次在线 lease 或本地 origin 保护 |

quick-start 不再让 device worker、hosted client 和 tunnel provider 共用同一个 shared key。

### 12.2 本地模式仍然轻

“正确建模”不等于强迫个人用户先配置团队系统：

- 本地第一次运行自动创建 owner user、device、project membership；
- 所有多用户实体由系统隐藏创建；
- 使用同进程 LocalExecutor，不启动需要用户理解的独立 agent；
- direct/Cloudflare origin 只监听 loopback，并使用每个 connector 独立的 project-scoped proof；
- OpenAI stdio MCP 路径通过受管进程参数绑定 project/grant，不开放公共 listener；
- Cloudflare URL 即使是临时 URL 也不能匿名访问；
- 只有添加第二台设备或第二个用户时才显示相关概念。

### 12.3 远程和团队模式

- Device 通过一次性 enrollment code 或 device authorization flow 注册；
- model client 通过 OAuth 或平台支持的 app auth 授权到 user + project scopes；
- membership 以 user_id/project_id 检查；
- device revoke 立即断开其 connections；
- workspace 是否允许其他 editor 执行由 workspace policy 决定；
- approval 绑定 actor、task、run、action hash、precondition、TTL，批准后重新校验。

### 12.4 不再默认支持

- unknown bearer 自动变 shared-key principal；
- 普通运行中的 --open anonymous；
- 在终端打印完整 Authorization 提示；
- username/owner 字符串作为项目授权；
- agent token 绑定 client_id。

临时无认证 demo 可以留在 test-only binary 或显式开发 feature，不属于正式产品路径。

## 13. CLI：一个入口，项目优先

### 13.1 默认帮助

日常命令控制在：

    webcodex                 打开/注册当前项目并确保 runtime ready
    webcodex connect ...     把当前项目接入一个 hosted client
    webcodex disconnect ...  停止/撤销 connector ingress
    webcodex status          当前项目、workspace、task、runtime 状态
    webcodex doctor          诊断 binding、protocol 与 ingress
    webcodex login URL       登录远程 control plane
    webcodex task ...        list/show/accept/reject/resume
    webcodex device ...      list/enroll/revoke
    webcodex project ...     show/members/workspaces
    webcodex down            停止本地个人 runtime

server、device worker、MCP stdio child 可以保留为内部 mode 或 hidden subcommand，但安装和文档只暴露一个 `webcodex` artifact。

### 13.2 本地个人路径

    cd repo
    webcodex

幂等完成：

- local control plane ready；
- 当前 device ready；
- 同进程 LocalExecutor ready；
- Project/Workspace binding ready；
- rules 和 Git 状态 ready；
- 打开或打印 project review URL。

第二次运行不得创建新 user、credential、project 或 workspace。

### 13.3 Hosted client 接入路径

命令必须可以直接从未执行过 `webcodex` 的仓库运行，并先幂等完成 Project Open：

    webcodex connect chatgpt --via openai
    webcodex connect gpt-actions --via cloudflare --temporary
    webcodex connect mcp --via cloudflare --profile personal
    webcodex connect gpt-actions --via cloudflare --profile personal

行为约束：

- `chatgpt`、`mcp`、`gpt-actions` 是可检查的 preset；未知平台优先落到标准 MCP/OpenAPI，而不是新写核心分支；
- `--via openai` 只允许 MCP target，并检查 `tunnel-client`、tunnel_id、权限、doctor/readiness；
- Cloudflare Quick 必须显式 `--temporary`，显示 URL 易变和 SSE 限制，并做实际 capability probe；
- Named Tunnel/profile 复用稳定 hostname，不在命令行或普通输出打印 tunnel token；
- supervisor 同时观察 local runtime 与 tunnel child；任一方退出时 status 必须准确，不留下“configured = online”的假状态；
- 输出最终只给 connector target、project scope、health、下一条平台侧配置动作和可复制的非 secret endpoint/id。

`connect` 不自动修改用户的 hosted-platform/Cloudflare 账号，不绕过 workspace admin 或 provider permission。能通过官方 API 安全自动化的步骤后续可加，但 CLI 必须先把不可自动化的唯一一步说清楚。

### 13.4 共享 control plane 路径

    webcodex login https://code.example.com
    cd repo
    webcodex

若 device worker、workspace 或 HTTPS/OAuth 有问题，命令在当前步骤停止并给出一个 suggested action；不要求用户手工拼多个 doctor 参数。共享 control plane 可以使用 existing HTTPS，也可以在其部署侧使用 tunnel，但它不改变 hosted-client protocol adapter。

### 13.5 Review 路径

    webcodex task list
    webcodex task show wc_task_...
    webcodex task accept wc_task_...
    webcodex task reject wc_task_...

accept/reject 是人类命令，不授权模型替用户接受结果。

## 14. Browser console：从运维状态页变成 Project/Task UI

最小信息架构：

1. Projects：逻辑项目、成员、在线 workspace。
2. Tasks：状态、发起人、执行 workspace、更新时间。
3. Task detail：timeline、changed files、diff、checks、warnings。
4. Review：accept/reject、目标 workspace、precondition/conflict。
5. Approvals：pending action、risk、TTL、approve once/deny。
6. Devices：last seen、workspace、revoke。

runtime transport 和 protocol details 移入 Diagnostics，不占首页。

## 15. 不兼容迁移策略：硬切，但不破坏旧文件

当前只有单个实际用户，这是一次性清理错误抽象的最佳窗口。建议：

1. 新版本使用新的 schema version 和新的数据库文件或明确 namespace。
2. 旧 SQLite 与 session ledger 原样归档为只读备份，不自动删除。
3. 不写 old→new 的长期兼容层，不做双读、双写或 alias 字段。
4. 只提供一次性 import：可选择导入 users 的显示信息；不导入旧 session/tool history。
5. 旧 agent:<client_id>:<project_id>、wc_sess_*、shared key 和 tool names 直接失效。
6. CLI 首次启动检测旧数据，解释 backup 位置和新初始化结果。
7. 删除完成后更新 README/quick start；不让新文档继续解释旧模型。

“不兼容”不等于静默删除用户数据。旧数据保留、可检查，但新 runtime 不继续背负它。

## 16. 分阶段实施计划

这是跨 CLI、认证、数据库、executor、runtime、MCP/OpenAPI、ingress 和 console 的重构，不能在一个巨型修改中同时完成。先验证最不确定、最接近用户价值的 hosted-client ingress，再重建内部模型；每个阶段必须形成可运行的 vertical slice，并在进入下一阶段前删除对应旧路径。

> 执行校准（2026-07-15）：下面的 Slice 仍保留最初的架构拆分依据，不再作为“从 Slice 1 重新开始”的待办清单。当前代码已经完成个人 hosted connect、8 项 canonical surface、SQLite Task/Run/Result/Approval、隔离执行与可复用槽位的纵向主路径。真实 provider 账号验收仍未完成；近期主线已调整为工具效果 Round 5/6，详见第四轮文档。

### Slice 0：冻结旧表面积并建立验收集（2～3 天）

交付：

- 冻结新增 model-visible tools、session aggregate 字段和 compatibility alias；
- 建立 golden scenarios 与 connector acceptance ledger；
- 固化当前 setup steps、tool-selection error、payload size、首次成功调用耗时和真实 client 失败样本；
- 写 architecture decision：Hosted Chat 产品边界、Project/Workspace/Task/Run、protocol/ingress 两轴与 hard cut。

Golden scenarios：

1. 新 Git 项目首次打开；
2. 已绑定项目幂等打开；
3. clean repo 小修改；
4. dirty repo 隔离修改；
5. validation failure；
6. raw command approval；
7. client/tunnel 中断后恢复；
8. OpenAI Tunnel 首次接入；
9. Cloudflare 稳定 URL 首次接入；
10. Cloudflare Quick 的 SSE 不兼容被明确诊断；
11. 同用户第二台设备；
12. viewer 越权执行；
13. remote device worker 离线后重连。

出口：

- 后续每个 slice 都能用同一组场景比较；
- 不再接受“新增 summary 字段”作为 blind-box 问题的修复。

### Slice 1：Hosted connector / Tunnel 可行性 spike（3～5 天）

这是下一次代码改动，刻意复用当前 runtime 的最小部分，不先做大规模 domain 重构。

交付：

- 用官方 `tunnel-client` 把 WebCodex MCP 以 stdio 和 loopback HTTP 两种方式接入 OpenAI Secure MCP Tunnel；
- 用官方 `cloudflared` 验证 Quick Tunnel 与 Named Tunnel；
- 真实完成 MCP initialize、tools/list、一个只读调用和一个有界写调用；
- 通过 Cloudflare stable hostname 完成一次 GPT Actions/OpenAPI 调用；
- 记录 OpenAI workspace/org 权限、tunnel_id、runtime key、app auth/OAuth 的实际边界；
- 测试 SSE、超时、进程退出、重连、随机 URL 变化、public_url/discovery 和 credential redaction；
- 形成 capability matrix，而不是立刻写统一 `TunnelManager` 大框架。

约束：

- runtime/origin 只绑定 stdio 或 127.0.0.1；
- spike 不允许 `--open` anonymous，不把 provider credential 写进命令行、日志或数据库；
- Quick Tunnel 仅用于测试；若当前 MCP 响应需要 SSE，预期结果是清楚地 fail closed，不为通过 spike 发明私有流式协议；
- OpenAI Tunnel 只验证官方声明支持的 MCP 路径，不把它宣传为 GPT Actions 通用 tunnel；
- 不 fork、不 vendor、不重写 tunnel client。

出口：

- 对每种 ingress 给出“支持 / 有条件支持 / 不支持”及可复现证据；
- 确定 ChatGPT 默认 ingress、通用 MCP 默认 ingress 和 GPT Actions 默认 ingress；
- 从全新仓库到首次 hosted tool call 的人工步骤和失败提示可度量；
- 只有 spike 证明可行的路径进入正式 CLI 设计。

### Slice 2：新领域模型与 vNext store（约 1 周）

交付：

- User/Device/Project/Membership/ConnectorGrant/Workspace/Task/Run/Event typed model；
- Task 和 Run 状态机；
- vNext SQLite schema 与 repositories；
- 单调 event sequence 和 projection rebuild；
- 新 ID 生成与解析；
- 旧数据库只读检测，不做 dual write。

第一批测试：

- 非法 transition 全部失败；
- membership capability matrix；
- 两台 Device 可同时 online；
- 同一 Project 可绑定多个 Workspace；
- 每个 ConnectorGrant 只属于一个 user/project scope，IngressSession 不能替代授权；
- restart 后 Task/Run/Event 完整恢复；
- event append 与 state transition 原子。

这一 slice 不改默认 MCP，不做 UI，先把后续所有行为的 authority 建好。

### Slice 3：一个 binary 的 Project Open + Connect（约 1～2 周）

交付：

- webcodex 无参数/open 流程；
- Git root、worktree、instructions 和 toolchain discovery；
- 本地 binding；
- 本地 solo bootstrap；
- 同进程 LocalExecutor；
- Workspace 注册；
- ProtocolAdapter、IngressAdapter 和 ConnectorPreset 的最小稳定接口；
- `webcodex connect <target> --via <ingress>`；
- OpenAI/Cloudflare child process supervision、readiness、doctor 与 disconnect；
- project-scoped ConnectorGrant 和 loopback ephemeral proof；
- 普通输出不含 secret。

同时删除：

- webcodex-cli connect 的默认产品入口；
- server up 只写 env 的误导路径；
- 个人模式必须启动独立 server + agent 的路径；
- agent 与 model 共用 shared key 的 quick-start；
- 用户手写 runtime project id 的流程。

出口：

- 新环境中一条 `webcodex` 打开本地项目，一条 `webcodex connect ...` 完成 WebCodex 侧 hosted 接入；
- 直接运行 `connect` 也能幂等完成 Project Open；
- 重复运行完全幂等；
- tunnel 与 runtime 的在线状态真实可诊断；
- 个人路径不要求部署独立远程 server。

### Slice 4：Task Kernel、隔离 Run 与 Task Result（约 2 周）

交付：

- StartTask / FinishTask / AcceptTask / RejectTask；
- reusable Git execution slot（最初为 per-run worktree，第四轮已收敛为固定槽位）；
- run-bound workspace lease；
- baseline.capture 与 result.diff；
- patch artifact；
- checks/validation result；
- interrupted/resume；
- task list/show CLI。

新 Task 路径停止使用：

- JSON SessionStore；
- current-session binding；
- action session 双状态；
- finish 与 close 的分裂；
- 无 baseline 的 whole-worktree attribution。

这些旧模块暂时只供尚未切换的旧 model surface 编译，不做 dual write，也不再增加能力；在 Slice 5 切换默认 surface 后物理删除。

出口：

- clean/dirty repo 的默认写任务都不直接改当前 checkout；
- client 中断后 Task 可查；
- finish 产生稳定 Result；
- accept 有 precondition，reject 不留下任务修改现场；受管 idle slot 可保留供下一 Task 复用；
- 本地 restart 后能够查看同一个 Result；共享模式的跨设备查看留到 Slice 7。

### Slice 5：canonical MCP/GPT capability surface 硬切（约 1～2 周）

交付：

- 第 8 节的 8 个候选 capability，并根据真实 acceptance 数据允许小幅拆分/合并；
- 单意图 strict schemas，不用 God tool 的 nested operation 隐藏复杂度；
- project-scoped connection/context resource；
- cursor-based read/search/review；
- adapter-specific GPT Action flattening；
- registry、MCP tools/list、OAuth scope policy、OpenAPI 和 metadata 一致性测试；
- 真实 MCP Connector 与 GPT Actions acceptance。

同时删除：

- 旧 76-tool model registry；
- tool_manifest 推荐层；
- model-facing session/message/ops/project registration；
- 多套 edit/patch compatibility tools；
- giant start/finish payload；
- JSON SessionStore、current-session binding 和独立 action session；

出口：

- 默认 tools/list 只含验收后的 canonical capabilities，数量本身不是成功指标；
- 模型的 consequential calls 100% 属于 active Run；
- 小修改不需要 list_agents/list_projects/runtime_status；
- tools/list 和典型 startup context 相比当前显著下降；
- golden scenarios 的工具选择错误率、schema retry 和无效往返可度量降低；
- 同一 capability 在 MCP 与 GPT Actions 中具有相同业务语义。

### Slice 6：真实 approval 与 review console（约 1～2 周）

交付：

- durable Approval；
- action hash、TTL、precondition 和 exactly-once decision；
- console Projects/Tasks/Review/Approvals；
- CLI approve once/deny；
- accept/reject UI；
- raw command、destructive action 的 reviewed policy。

同时删除：

- require_approval_not_implemented；
- permission summary 代替真实 gate 的行为；
- runtime-only console 首页。

出口：

- 未批准 action 绝不 enqueue；
- 重复批准不重复执行；
- precondition 变化后批准安全失效；
- 高风险 action approval 与最终 patch accept 明确分离。

### Slice 7：共享 control plane、多设备与多用户（约 2 周）

基础 schema 在 Slice 2 已存在，此阶段增加可选共享拓扑并完成用户可见闭环：

- shared store 与 RemoteDeviceExecutor；
- remote login + device enrollment；
- device list/revoke；
- project workspace selector；
- run handoff/new attempt；
- project member add/remove；
- owner/editor/viewer capability enforcement；
- workspace execution visibility；
- 跨设备 accept 的 base/precondition 检查；
- 审计 actor 显示。

出口：

- 同一用户两台设备并发在线、各自 Workspace 可选择；
- device revoke 立即失去 executor routing；
- viewer 无法创建/执行任务；
- editor 不能在 owner_only Workspace 执行；
- Project owner 能审查每个 actor 的 Task/Approval；
- 个人本地模式仍可独立安装使用，未被共享部署复杂度回灌。

本计划到此结束，不包含 LocalModelDriver。若未来产品方向真的改变，应另写独立 RFC，而不是在上述 adapter 中预埋 provider abstraction。

## 17. 当前模块到目标模块的迁移地图

| 当前实现 | 目标 | 处理 |
|---|---|---|
| ShellClientRegistry | DeviceConnectionManager + WorkspaceRouter | 只服务远程 executor；按 device/connection/workspace 重写 |
| 无独立 hosted-client 连接模型 | ConnectorManager + ConnectorGrant + IngressSession | 与 DeviceConnection 明确分开 |
| client_id / agent_instance_id | device_id / device_connection_id | 删除 client_id 多重语义 |
| ProjectConfig path + client_id | Project + executor-local Workspace binding | control plane 不以 path 定义项目 |
| ToolRuntime | TaskService + WorkspaceExecutor | use case 化，逐步删除 monolith dispatch |
| coding_task aggregates | StartTask / FinishTask | 重写，不迁移 giant payload |
| SessionStore JSON | TaskEventStore SQLite | 硬切 |
| action_sessions/action_events | task_events projections | 合并事实源 |
| current session | explicit task_id/run_id | 删除 fallback |
| permission evaluator summary | ApprovalBroker | 真实 pending/decision |
| 76 runtime tool registry | 验收后的 canonical capabilities + internal executor ops | 硬切，不以固定数量替代可用性 |
| 手工反向代理/tunnel 文档 | ProtocolAdapter + IngressAdapter + ConnectorPreset | 官方 tunnel client 由单入口监督 |
| runtime console | Project/Task/Review console | 重做信息架构 |
| webcodex-cli + agent CLI | single public webcodex | server/device worker/stdio child 入口隐藏 |

## 18. 验收指标

| 指标 | 目标 |
|---|---|
| 本地首次打开项目 | 安装后 1 条 WebCodex 命令 |
| 个人 hosted 接入 | 平台前置权限就绪后，1 条 `webcodex connect`；无需部署独立远程 server |
| 首次真实调用 | 从空 connector 配置到 hosted tools/list + 只读调用可在 5 分钟内完成 |
| 本地暴露 | OpenAI stdio 为 0 listener；其他个人 ingress 只绑定 loopback |
| Tunnel 诊断 | runtime/tunnel 任一离线均准确显示；无“configured = online” |
| Quick Tunnel | SSE/能力不兼容 100% 在 preflight 明确拒绝 |
| 共享模式首次接入 | login/enroll 1 次，之后每个项目 1 条命令 |
| 普通 secret 输出 | 0 个完整 credential |
| 默认 model tools | 0 个 legacy/ops/session 工具；每个 canonical tool 只有一个可解释意图 |
| hosted-client 可用性 | golden scenarios 的工具选择错误、schema retry、无效往返均低于当前 baseline |
| 写任务归因 | Git isolated mode 100% |
| dirty checkout 污染 | 默认 0 |
| client 中断可见性 | 100% Task 留下 interrupted/needs_attention |
| Task Result 完整性 | changed files、patch、checks、warnings、actor、workspace 全部存在 |
| accept 安全 | base/precondition 不匹配时 100% fail closed |
| 多设备 | 共享模式下同用户至少 2 台设备并发在线；个人模式不虚假宣称同步 |
| 多用户隔离 | capability matrix 的越权路径 100% 拒绝 |
| approval | 未批准不 enqueue；重复批准不重复执行 |
| restart durability | Project/Task/Run/Event/Approval 全部可恢复 |

指标只记录结构化元数据。不得为了评测采集 prompt、源码、完整 diff、command body、token、env 或未脱敏 stdout/stderr。

## 19. 明确不做

本计划明确不做：

- 内置 LLM、模型下载、provider SDK、prompt loop、agent loop 或自动续写任务；
- 为未来模型 provider 预埋 `LocalModelDriver` / `ModelProvider` 抽象；
- WebCodex 自有公网 relay、tunnel 协议或 cloud proxy；第一版只集成官方 `tunnel-client` / `cloudflared`；
- 把 Cloudflare Quick Tunnel 宣传为生产路径；
- 自动创建或绕过 OpenAI/Cloudflare 账号、workspace、RBAC 和管理员审批；
- 在对应 capability-surface slice 之外继续追加 production model-visible tool；
- LSP 写能力；
- plugin/subagent 市场；
- organization、billing、SSO；
- cloud code hosting；
- 为旧 session/tool schema 写长期 migration；
- 只优化 JSON compact 百分比、但不改变模型决策面的工作；
- 用更多文档步骤掩盖 CLI 没有真正完成启动。

## 20. 下一步

接入和 Task 主路径完成后，近期开发不再围绕某个线上窗口增加状态聚合，也不立刻扩展 shared control plane。按用户体验收益排序：

1. **Round 5：编辑与读取可靠性。** 在现有 `edits_apply` 内实现原子 multi-file edit/create/delete/rename、逐文件 expected hash、批量 preflight、幂等 operation id 和结构化冲突；`files_read` 返回 hash，`files_search` 使用稳定 cursor；`task_start` 只给小型 Project Brief。
2. **Round 6：project-aware validation。** 用明确 marker/Project recipe 支持 Rust、Node、Python 等 validation profile，统一 bounded diagnostics、changed-path check selection、progress/result，以及 passed/failed/not_run/stale 终态。
3. **真实 hosted acceptance 作为并行验收 lane，而不是架构前置。** 在具备账号权限的机器上记录 OpenAI Tunnel、Cloudflare Named/Quick、MCP/GPT Actions 的 handshake、重连、SSE、身份和 credential redaction 证据；失败不驱动临时 façade。
4. **之后才进入共享模式。** 实现 User/Device/ProjectMembership/Workspace routing、device revoke 和跨 Workspace Result apply precondition；个人本地模式仍保持一条命令接入与默认单写槽位。
5. Browser review/approval UI 可与共享模式一起做，但不能替代工具成功率改进，也不能重新暴露 agent/session/runtime 内部概念。

Round 5/6 使用 golden tasks 记录首次编辑成功率、schema retry、平均工具调用数、重复读取、warm validation 时间、诊断可操作率和磁盘峰值。只有指标或真实 hosted acceptance 证明有必要时才拆分/新增模型能力；否则继续保持当前 8 项 project-bound surface。

这个顺序把 WebCodex 放回原本的位置：线上聊天窗口负责智能，WebCodex 负责把私有项目以轻量、安全、可预测、可审查的方式接进去。先让读、改、检查的单次效果稳定，再把同一套事实模型扩展到多设备和多用户，比同步更多模糊状态更有价值。
