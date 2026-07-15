# WebCodex Project-first 重构设计与开发计划

> 评审日期：2026-07-15
>
> 状态：v2 方向稿；替代“在现有 tool/session 之上继续收口”的保守方案
>
> 目标：轻量打开项目、稳定执行任务、跨设备审查结果，并为未来多用户保留正确边界
>
> 范围：产品模型、核心架构和分阶段开发计划；不是发布承诺

## 1. 结论：应当换主语，而不是继续补聚合

WebCodex 当前最根本的问题不是 CLI 文案，也不是某几个工具不好用，而是产品主语放错了：

- 用户面对的是 server、agent、client id、runtime project id、session id、recording session、tool manifest 和 76 个工具；
- 外部 GPT 窗口仍然拥有完整的任务控制权；
- WebCodex 只能在每次反馈之后增加一个工具、一个 summary 或一个兼容字段；
- 最终形成了功能很多、状态很多，但任务结果仍然依赖模型有没有按预期编排的系统。

继续给现有 start_coding_task、finish_coding_task、session handoff 或 compact response 加字段，只会让补丁更完整，不会让产品更像一个可靠的 coding system。

本计划建议做一次明确的方向切换：

> WebCodex 不再以“远程模型可调用的工具集合”为核心，而以“Project → Workspace → Task → Run → Result”为核心。模型、CLI、console、MCP 和 GPT Actions 都只是这个任务内核的不同 driver 或 adapter。

新的产品承诺应当是：

> 打开一个项目，描述一个结果，让任务在受控工作区中执行；无论模型窗口是否中断，WebCodex 都能给出可定位、可验证、可接受或拒绝的 Task Result。

这意味着：

1. 日常入口只有一个 webcodex。
2. 用户先打开 Project，再开始 Task；不先理解 agent 和工具拓扑。
3. 写任务默认在隔离的执行工作区中进行，不直接污染用户当前 checkout。
4. WebCodex 自己拥有 task lifecycle、baseline、event、approval 和 result。
5. 外部 GPT 只是 task driver，不再决定系统的数据模型。
6. 默认模型工具面从 76 个硬切到 7 个 task-level tools。
7. 用户、设备、连接、逻辑项目和本地 workspace 必须拆成不同实体。
8. 不保留旧 session/tool/client_id wire compatibility；旧数据只归档，不双写。

## 2. 本次判断来自代码，而不是现有文档

本轮重新检查了 CLI、agent 注册、认证、项目解析、session、action audit、permission 和 coding-task 实现。以下代码事实决定了重构边界。

| 当前代码事实 | 直接后果 |
|---|---|
| webcodex、webcodex-agent、webcodex-cli 三个 binary 同时面向用户 | 用户必须先理解部署拓扑，才能打开一个仓库 |
| server up 只写 env，不启动 server；connect 只写配置，不启动或确认 agent | “up / connect”没有完成动词承诺 |
| connect 会把完整 Bearer 提示拼进普通输出 | 快速接入与凭据卫生冲突 |
| MCP 默认暴露 76 个 model-facing tools | 模型先学习 WebCodex 内部 API，才开始理解项目 |
| start_coding_task 有 15 个输入字段，并聚合 runtime、Git、规则、manifest、LSP 和权限信息 | 调用次数减少了，概念数没有减少 |
| coding_task.rs 明确只是 deterministic aggregate，不拥有 LLM loop、重试或完成保证 | 任务能否闭环仍取决于外部窗口 |
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

因此，真正需要替换的是状态所有权和身份模型，而不只是默认帮助、tool profile 或 compact payload。

## 3. 从 Codex 打开新项目的流程中借鉴什么

这里不把 Codex 当作功能清单，也不试图复制其模型能力。值得借鉴的是它的产品顺序：

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

WebCodex 应吸收上述顺序，但保持自己的优势：模型可以来自 ChatGPT、Claude、本地 runner 或未来 provider；代码执行仍在用户控制的设备上；Task Result 可以跨 client、跨窗口和跨设备查看。

## 4. 目标：打开一个新项目时实际发生什么

用户在仓库中运行：

    cd my-repo
    webcodex

目标流程如下。

### 4.1 Discover：先确认工作上下文

WebCodex 自动完成：

1. 找到当前 Git worktree root；非 Git 目录使用显式 workspace root。
2. 读取本地 workspace binding；Git 项目通过 git rev-parse --git-path 解析实际 gitdir 后保存 binding，兼容普通 checkout 和 linked worktree，并避免写入被跟踪目录。
3. 从项目根到当前目录加载分层 AGENTS.md / AGENTS.override.md。
4. 检查 branch、HEAD、dirty state、冲突、submodule 和工具链 markers。
5. 发现项目内显式 actions/checks 配置；没有配置时只做保守推断。
6. 生成不含绝对路径和源码的 repo fingerprint，fingerprint 只用于发现候选项目，绝不作为授权身份。

### 4.2 Identity：自动建立当前人和当前设备

本地个人模式：

- 首次运行自动启动本地 control plane；
- 自动创建 owner account 和当前 device；
- 使用本地 socket 或内部 device credential；
- 不要求用户生成、复制或看到 Bearer token。

远程模式：

    webcodex login https://code.example.com

- 通过浏览器或 device-code flow 登录一次；
- 为当前机器签发独立、可撤销的 device credential；
- agent 永远不持有人类 PAT；
- 普通输出只显示设备名和 credential 前缀，不显示 secret。

### 4.3 Resolve Project：识别逻辑项目，而不是拼 runtime id

解析顺序：

1. 本地 binding 中已有 project_id：直接复用。
2. 同一 account 下 repo fingerprint 只有一个候选：自动建议并绑定。
3. 有多个候选：让用户选择，不能静默猜测。
4. 没有候选：创建新的 logical Project。

Project ID 由 server 生成，与路径、设备、remote URL、client id 都无关。

### 4.4 Register Workspace：把当前 checkout 注册成项目的一个副本

当前机器上的实际 checkout 是 Workspace：

- Workspace 属于一个 Project 和一个 Device；
- agent 本地保存 workspace_id 到绝对路径的映射；
- server 默认只保存 workspace 名、repo fingerprint、capabilities 和状态，不需要保存绝对路径；
- 一个 Project 可以同时有 laptop、desktop、server 等多个 Workspace；
- 同一设备上的一个 agent 可以管理多个 Workspace。

### 4.5 Ready：只显示与开始工作有关的信息

首次 ready 输出应接近：

    Project      my-repo
    Workspace    macbook/my-repo
    Rules        3 instruction sources loaded
    Git          main @ 67c9594, clean
    Safety       isolated writes, approval for raw commands
    Runtime      ready

    MCP          configured for this project
    Review       http://127.0.0.1:8080/projects/wc_proj_.../tasks

不显示 agent lease、transport fallback、runtime project id、tool count、session binding 或 token。

若 hosted client 无法访问 localhost，CLI 必须明确说明远程 HTTPS/OAuth 边界，不能输出一个实际上不可达的 localhost 配置。

### 4.6 Start Task：任务才是后续所有操作的 handle

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

    User ──< Device ──< AgentConnection
      │
      └──< ProjectMembership >── Project ──< Workspace
                                      │          │
                                      └──< Task ─┴──< Run ──< Event
                                                   ├──< Approval
                                                   └──< Artifact

| 实体 | 生命周期与职责 | 绝不能再混入 |
|---|---|---|
| User | 人类身份、登录、项目成员关系 | device connection、workspace path |
| Device | 一台已注册且可撤销的机器 | user token、logical project identity |
| AgentConnection | 一次短生命周期在线连接 | 稳定设备身份、项目身份 |
| Project | 长期逻辑代码项目和协作边界 | client_id、绝对路径 |
| ProjectMembership | user 在 project 中的 role/capabilities | server 全局 admin 角色 |
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
- wc_task_*：任务
- wc_run_*：执行尝试
- wc_evt_*：事件
- wc_apr_*：审批
- wc_art_*：结果 artifact

不复用 wc_sess_* 作为 task id，也不保留 agent:<client_id>:<project_id>。

### 5.2 核心身份不变量

1. 授权只基于稳定 user_id、device_id、project_id 和 membership。
2. username、hostname、display name、repo remote 和 path 都只是显示或发现信息。
3. agent credential 只代表一台 Device，不代表用户会话。
4. AgentConnection 每次进程启动生成新 ID；同一设备允许重连，多个设备允许同时在线。
5. 所有 workspace 操作先解析 project membership，再解析 workspace routing。
6. server admin 不自动等于所有 Project 的 owner；恢复入口与日常项目授权分离。

## 6. 单用户多设备与多用户应如何工作

### 6.1 同一用户、多台设备

场景：同一账号在 laptop 和 desktop 都 checkout 了 my-repo。

- 两台机器分别注册为 wc_dev_laptop 和 wc_dev_desktop；
- 两个 checkout 分别是不同 workspace_id；
- 它们都属于同一个 project_id；
- 两个 agent 可以同时在线，不复用 credential，也不争夺 client_id lease；
- 新 Task 自动选择当前设备的 Workspace；远程发起时选择唯一在线候选，存在多个候选时明确展示选择；
- Task 的 Run 固定到一个 Workspace，避免一半操作落到另一台机器；
- 从一台设备 handoff 到另一台设备会创建新的 Run 和新的 baseline，不伪装成同一个 filesystem 继续执行。

用户可以在任意设备查看 Task Result。若 patch 的 base precondition 匹配，也可以在另一台 Workspace 上接受结果。

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

- client disconnect 只影响 driver lease；
- Task 和 Run 事件已经持久化；
- 超时后 Run 标记 interrupted，Task 保持 needs_attention；
- 新窗口可以读取 task_review 并创建恢复 Run；
- 没有 finish 时也能看到 incomplete result，不靠模型最后一句话补救。

## 8. 默认 model-facing 工具面：7 个

新 MCP/GPT adapter 默认只暴露以下工具：

| Tool | 职责 | 关键约束 |
|---|---|---|
| task_start | 创建 Task/Run，返回精简 context | 只接受 goal、mode、可选 project/workspace |
| task_inspect | 批量 read/search/list/symbol/diff | 只读、严格 bounded、支持 cursor |
| task_edit | 原子 structured edits | 必须带 file version/hash precondition |
| task_check | 运行项目声明或安全推断的 checks | 默认不接收任意 shell 字符串 |
| task_command | 高级 escape hatch | 受 policy/approval；只能在 Run root |
| task_review | 读取进度、events、diff、validation 和 warnings | 增量 cursor；不返回 giant aggregate |
| task_finish | 原子结束 Run 并固化 Task Result | 自动 capture diff/check state，进入 ready_for_review |

人类操作 accept、reject、approve、deny、device revoke、member manage 不属于模型工具。

### 8.1 不是把 76 个工具塞进两个 God tools

task_inspect、task_edit 和 task_command 必须使用严格 tagged operations：

- operation enum 固定；
- 每批数量、字节数、文件数和耗时有上限；
- 路径始终相对 Run root；
- edit 必须有 precondition；
- command 默认只能调用项目声明的 action；
- raw shell 只在显式 advanced policy 下存在；
- 响应使用 cursor 和 artifact reference，不把所有内容一次塞回模型窗口。

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

GPT Actions 的 flattened fields、compact mode 和 OpenAPI operation 限制只存在于 adapter，不进入 domain model。

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

- Identity：User、Device、ProjectMembership；
- Project：Project、Workspace、WorkspacePolicy；
- Task：Task、Run、state transitions；
- Approval：ActionIntent、ActionHash、Decision；
- Result：Baseline、ChangeSet、ValidationResult、TaskResult。

### 9.2 Application

用例层拥有业务顺序：

- OpenProject
- RegisterWorkspace
- StartTask
- InspectTask
- ApplyTaskEdits
- RunTaskCheck
- RequestTaskCommand
- FinishTask
- AcceptTask
- RejectTask
- ResumeTask

任何 adapter 都只能调用这些 use cases。

### 9.3 Ports

核心只依赖接口：

- IdentityRepository
- ProjectRepository
- TaskRepository
- EventStore
- WorkspaceExecutor
- ApprovalBroker
- ArtifactStore
- Clock / IdGenerator

### 9.4 Adapters

- SQLite：用户、设备、项目、任务、event、approval、artifact metadata；
- Agent transport：workspace 执行；
- MCP / GPT Actions：外部 model driver；
- CLI / browser console：人类操作；
- future LocalModelDriver：可选的本地 agent loop。

### 9.5 外部 GPT 不再塑造核心

引入 TaskDriver 概念：

- McpDriver：外部模型逐步拉取和调用；
- GptActionDriver：适配 stateless/flattened Action 限制；
- Future LocalModelDriver：WebCodex 自己驱动 provider loop；
- HumanDriver：用户直接运行 check、accept 或恢复。

Domain event 不包含 recommended_next_tool、compact_startup、flattened_args 等 client-specific 字段。每个 adapter 自己投影需要的 payload。

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
- workspaces
- tasks
- task_runs
- task_events
- workspace_leases
- approvals
- artifacts
- human_sessions / oauth_clients / oauth_tokens

AgentConnection 可以主要保留在内存 connection manager 中，但连接/断线事实写入 task event 或设备 last_seen。

### 10.3 Artifact 策略

为了支持跨设备 review，server 需要持久化有界 Task artifacts：

- task-attributed patch；
- file/change manifest；
- validation report；
- review summary；
- approval record。

默认不持久化完整仓库、任意 shell stdout、环境变量或 secret。Artifact 有 sha256、size、content type、retention 和 sensitivity classification。敏感路径在 agent 侧先拒绝；大型输出只保存 bounded sanitized excerpt。

## 11. Agent protocol 的新职责

Agent 不再上报“一个 client 拥有哪些 project path”，而是作为 Device 的 workspace executor。

内部协议建议收敛为：

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

路由键是 workspace_id + connection_id，不是 client_id。

server 是以下信息的 authority：

- User、Device、Project、Membership；
- Task/Run state；
- approval；
- event ordering；
- artifact metadata。

agent 是以下信息的 authority：

- workspace_id 到本地 path 的映射；
- filesystem/Git 实际状态；
- execution root；
- baseline 内容；
- command process 与资源限制。

## 12. 认证和安全模型

### 12.1 三类 credential，不能混用

| Credential | 代表谁 | 使用面 |
|---|---|---|
| Human session / PAT / OAuth | User | CLI、console、model client |
| Device credential | Device | agent transport |
| Ephemeral connection proof | AgentConnection | 单次在线 lease |

quick-start 不再让 agent 与 GPT/MCP 共用同一个 shared key。

### 12.2 本地模式仍然轻

“正确建模”不等于强迫个人用户先配置团队系统：

- 本地第一次运行自动创建 owner user、device、project membership；
- 所有多用户实体由系统隐藏创建；
- localhost 优先使用 socket/loopback 限制和内部 credential；
- 只有添加第二台设备或第二个用户时才显示相关概念。

### 12.3 远程和团队模式

- Device 通过一次性 enrollment code 或 device authorization flow 注册；
- model client 通过 OAuth 授权到 user + project scopes；
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
    webcodex status          当前项目、workspace、task、runtime 状态
    webcodex doctor          自动诊断当前 binding
    webcodex login URL       登录远程 control plane
    webcodex task ...        list/show/accept/reject/resume
    webcodex device ...      list/enroll/revoke
    webcodex project ...     show/members/workspaces
    webcodex down            停止本地个人 runtime

server、agent worker 可以暂时保留为内部模式或 hidden subcommand，但安装和文档只暴露一个 webcodex artifact。

### 13.2 本地个人路径

    cd repo
    webcodex

幂等完成：

- local control plane ready；
- 当前 device ready；
- background agent ready；
- Project/Workspace binding ready；
- rules 和 Git 状态 ready；
- 打开或打印 project review URL。

第二次运行不得创建新 user、credential、project 或 workspace。

### 13.3 远程路径

    webcodex login https://code.example.com
    cd repo
    webcodex

若 device、agent、workspace 或 HTTPS/OAuth 有问题，命令在当前步骤停止并给出一个 suggested action；不要求用户手工拼多个 doctor 参数。

### 13.4 Review 路径

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

这是跨 CLI、认证、数据库、agent、runtime、MCP 和 console 的重构，不能在一个巨型修改中同时完成。每个阶段必须形成可运行的 vertical slice，并在进入下一阶段前删除对应旧路径。

### Slice 0：冻结旧表面积并建立验收集（2～3 天）

交付：

- 冻结新增 model-visible tools、session aggregate 字段和 compatibility alias；
- 建立 10 个 golden scenarios；
- 固化当前 tool count、setup steps、payload size 和真实 client 失败样本；
- 写 architecture decision：Project/Workspace/Task/Run 与 hard cut。

Golden scenarios：

1. 新 Git 项目首次打开；
2. 已绑定项目幂等打开；
3. clean repo 小修改；
4. dirty repo 隔离修改；
5. validation failure；
6. raw command approval；
7. client 中断后恢复；
8. 同用户第二台设备；
9. viewer 越权执行；
10. agent 离线后重连。

出口：

- 后续每个 slice 都能用同一组场景比较；
- 不再接受“新增 summary 字段”作为 blind-box 问题的修复。

### Slice 1：新领域模型与 vNext store（约 1 周）

交付：

- User/Device/Project/Membership/Workspace/Task/Run/Event typed model；
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
- restart 后 Task/Run/Event 完整恢复；
- event append 与 state transition 原子。

这一 slice 不改默认 MCP，不做 UI，先把后续所有行为的 authority 建好。

### Slice 2：一个 binary 的 Project Open（约 1～2 周）

交付：

- webcodex 无参数/open 流程；
- Git root、worktree、instructions 和 toolchain discovery；
- 本地 binding；
- 本地 solo bootstrap；
- remote login + device enrollment；
- background agent supervision 和 readiness；
- Workspace 注册；
- 普通输出不含 secret。

同时删除：

- webcodex-cli connect 的默认产品入口；
- server up 只写 env 的误导路径；
- agent 与 model 共用 shared key 的 quick-start；
- 用户手写 runtime project id 的流程。

出口：

- 新环境中最多一条 WebCodex 命令打开本地项目；
- 远程环境是 login 一次 + 项目中运行 webcodex；
- 重复运行完全幂等；
- 同账号两台设备可同时注册和在线。

### Slice 3：Task Kernel、隔离 Run 与 Task Result（约 2 周）

交付：

- StartTask / FinishTask / AcceptTask / RejectTask；
- Git execution worktree；
- workspace lease；
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

这些旧模块暂时只供尚未切换的旧 model surface 编译，不做 dual write，也不再增加能力；在 Slice 4 切换默认 surface 后物理删除。

出口：

- clean/dirty repo 的默认写任务都不直接改当前 checkout；
- client 中断后 Task 可查；
- finish 产生稳定 Result；
- accept 有 precondition，reject 不留下执行 worktree；
- 跨设备能够查看同一个 Result。

### Slice 4：7-tool MCP/GPT surface 硬切（约 1～2 周）

交付：

- 7 个 task tools；
- strict tagged schemas；
- project-scoped connection/context resource；
- cursor-based inspect/review；
- adapter-specific GPT Action flattening；
- 真实 MCP client acceptance。

同时删除：

- 旧 76-tool model registry；
- tool_manifest 推荐层；
- model-facing session/message/ops/project registration；
- 多套 edit/patch compatibility tools；
- giant start/finish payload。
- JSON SessionStore、current-session binding 和独立 action session；

出口：

- 默认 tools/list = 7；
- 模型的 consequential calls 100% 属于 active Run；
- 小修改不需要 list_agents/list_projects/runtime_status；
- tools/list 和典型 startup context 相比当前显著下降；
- golden scenarios 的工具选择错误率可度量降低。

### Slice 5：真实 approval 与 review console（约 1～2 周）

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

### Slice 6：完成多设备与多用户产品面（约 1～2 周）

基础 schema 在 Slice 1 已存在，此阶段完成用户可见闭环：

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
- device revoke 立即失去 agent routing；
- viewer 无法创建/执行任务；
- editor 不能在 owner_only Workspace 执行；
- Project owner 能审查每个 actor 的 Task/Approval。

### Slice 7：可选 LocalModelDriver，用数据决定（后续）

只有 7-tool external-driver 路径仍然无法达到目标成功率时，再实现：

- provider-neutral model loop；
- plan/act/check/review driver；
- OpenAI/Anthropic/local provider adapters；
- budget、retry、context compaction；
- webcodex task start 的本地 agent 模式。

Task Kernel、WorkspaceExecutor 和 Result 不因 provider 改变。这样即使最终做本地 coding agent，也不会再把某个 GPT 窗口的反馈写进核心数据模型。

## 17. 当前模块到目标模块的迁移地图

| 当前实现 | 目标 | 处理 |
|---|---|---|
| ShellClientRegistry | ConnectionManager + WorkspaceRouter | 按 device/connection/workspace 重写 |
| client_id / agent_instance_id | device_id / connection_id | 删除 client_id 多重语义 |
| ProjectConfig path + client_id | Project + agent-local Workspace binding | server 不以 path 定义项目 |
| ToolRuntime | TaskService + WorkspaceExecutor | use case 化，逐步删除 monolith dispatch |
| coding_task aggregates | StartTask / FinishTask | 重写，不迁移 giant payload |
| SessionStore JSON | TaskEventStore SQLite | 硬切 |
| action_sessions/action_events | task_events projections | 合并事实源 |
| current session | explicit task_id/run_id | 删除 fallback |
| permission evaluator summary | ApprovalBroker | 真实 pending/decision |
| 76 runtime tool registry | 7 task tools + internal agent ops | 硬切 |
| runtime console | Project/Task/Review console | 重做信息架构 |
| webcodex-cli + agent CLI | single public webcodex | worker 入口隐藏 |

## 18. 验收指标

| 指标 | 目标 |
|---|---|
| 本地首次打开项目 | 安装后 1 条 WebCodex 命令 |
| 远程首次打开项目 | login 1 次，之后每个项目 1 条命令 |
| 普通 secret 输出 | 0 个完整 credential |
| 默认 model tools | 恰好 7 个 |
| 写任务归因 | Git isolated mode 100% |
| dirty checkout 污染 | 默认 0 |
| client 中断可见性 | 100% Task 留下 interrupted/needs_attention |
| Task Result 完整性 | changed files、patch、checks、warnings、actor、workspace 全部存在 |
| accept 安全 | base/precondition 不匹配时 100% fail closed |
| 多设备 | 同用户至少 2 台设备并发在线 |
| 多用户隔离 | capability matrix 的越权路径 100% 拒绝 |
| approval | 未批准不 enqueue；重复批准不重复执行 |
| restart durability | Project/Task/Run/Event/Approval 全部可恢复 |

指标只记录结构化元数据。不得为了评测采集 prompt、源码、完整 diff、command body、token、env 或未脱敏 stdout/stderr。

## 19. 明确不做

在 Slice 1～6 完成前，不做：

- 新 model-visible tool；
- 新 transport；
- LSP 写能力；
- plugin/subagent 市场；
- organization、billing、SSO；
- 自动 tunnel；
- cloud code hosting；
- 为旧 session/tool schema 写长期 migration；
- 只优化 JSON compact 百分比、但不改变模型决策面的工作；
- 用更多文档步骤掩盖 CLI 没有真正完成启动。

## 20. 下一步

下一次代码改动应从 Slice 1 开始，而不是先改工具名：

1. 写 Project/Workspace/Task/Run 的 typed model 和状态机。
2. 新建 vNext SQLite store。
3. 用两个 Device、一个 Project、两个 Workspace、一个 Task/Run 的集成测试证明关系成立。
4. 保持旧 runtime 仍可编译，但不让新 model 依赖旧 client_id/session 结构。
5. Slice 1 验收后，再接 Project Open；不要同时改 CLI、MCP 和 console。

这个顺序看起来比继续加一个 façade 慢，但它第一次把 WebCodex 的核心从“某个线上 GPT 窗口调用过哪些工具”转成“一个项目中的任务产生了什么可审查结果”。这是解决 CLI 心智、盲盒感、多设备和未来多用户的共同根基。
