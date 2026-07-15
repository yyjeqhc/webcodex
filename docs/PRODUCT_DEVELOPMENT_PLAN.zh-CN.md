# WebCodex 产品评审与后续开发计划

> 评审日期：2026-07-15
>
> 目标：轻量接入、稳定完成任务、结果可观察且可归因
>
> 范围：产品与开发优先级，不是发布承诺

## 1. 结论

WebCodex 值得继续做，但下一阶段不应该继续以“增加能力”为主。项目已经有一套扎实的私有代码执行内核；当前真正限制可用性的，是这套内核还没有被收敛成一个简单、稳定、可验证的产品闭环。

最重要的产品判断是：

> WebCodex 不必成为另一个 Claude Code 或 Codex CLI。它应该成为“线上模型连接私有代码时，最好接入、最好审查、最能说明自己做了什么的执行层”。

这意味着后续开发顺序应调整为：

1. 先让一个新用户用一条主命令跑起来。
2. 再把模型默认看到的工具从 76 个收窄到一条核心路径。
3. 再让每个任务都产生可独立查看、可归因的结果，而不是只依赖模型最后一句话。
4. 然后完成真正的高风险操作审批。
5. 最后再扩平台、语言和高级能力。

在上述四项完成前，建议冻结新增 model-visible runtime tool、额外传输协议、更多 LSP 写能力和大规模内部重构。

## 2. 本次评审方法

本结论主要来自实际代码和命令行为，而不是把现有设计文档当作事实：

- 实际运行三个 binary 的帮助输出并检查 CLI parser、setup、connect、doctor 和 server 管理实现。
- 核查 MCP、OpenAPI、runtime tool registry、coding task、session、permission gate 和 console 实现。
- 核查 npm 安装 manifest、真实 model-facing tool 数量和 compact 开关的默认行为。
- 阅读 coding-loop eval 脚本，区分“runtime 机械链路测试”与“真实模型能否做好任务”。
- 用 `cargo test --bin <name> -- --list` 编译并枚举测试：server 1,604 个、agent 346 个、CLI 200 个，共 2,150 个；这里只枚举了测试，没有运行完整测试套件。

已有 README、roadmap、评审报告只用于和代码交叉检查。代码与文档冲突时，本计划以代码行为为准。例如 README 仍把 LSP 写成缺失能力，但代码已经存在 Rust、Python、TypeScript/JavaScript 的只读导航实现。

## 3. 项目真实状态

### 3.1 已经做得好的部分

这些是应该保留和复用的资产，不需要为了产品化而重写：

- server 不直接接触项目文件，agent 主动外连并解释项目路径，信任边界清楚。
- ToolRuntime 复用于 MCP、GPT Actions 和 REST，行为一致性有大量测试保护。
- agent 协议、allowed roots、session guards、OAuth scope 和敏感输出边界做得认真。
- `start_coding_task`、`finish_coding_task`、validation evidence、workspace hygiene 和 checkpoint 已经提供了构建任务产品层所需的大部分底层原料。
- 最近的 timeout、断线、agent online window、异步 session persistence 等修复，说明执行内核的可靠性正在成熟。
- 工具定义开始集中，且新增了编辑工具使用遥测，已经具备做表面积减法的条件。

因此，后续应在现有内核上增加一层薄的产品外壳，而不是先拆 workspace、换前端框架或重写 runtime。

### 3.2 为什么 CLI 心智过重

这不是主观感受，当前命令契约确实把内部拓扑直接暴露给了用户：

| 代码事实 | 用户实际承担的心智 |
|---|---|
| npm 安装后同时出现 `webcodex`、`webcodex-agent`、`webcodex-cli` | 用户必须先理解 server、agent、management CLI 的分工 |
| `webcodex` 既是 server，又保留一组 admin command | “主程序”与“服务进程”的含义混在一起 |
| `webcodex-cli` 的手写 parser 约 1,962 行，有 20 个独立 usage 页面 | 快速接入、生产部署、token、pairing、service、doctor、ops 同处一个顶层帮助面 |
| `server up` 只写 env 文件，并不启动 server；`--foreground` 明确未实现 | `up` 的动词承诺和真实行为不一致 |
| `connect` 只生成 agent config 与项目条目，并不启动或确认 agent 已连接 | 用户运行“connect”后仍需复制路径、启动另一个进程和自行验证 |
| 快速开始仍要求两个终端、手动生成 key、复制 key、运行 curl、复制完整 project id | 第一次成功依赖多个可错步骤，而不是一个可重复的 profile |
| npm installer 认识多个平台，但当前发布 manifest 只有 Linux x64 artifact | 安装入口看似通用，实际覆盖面仍窄 |

CLI 的问题不是参数文案不够详细，而是把“实现组件”当成了“用户任务”。继续补更多 subcommand 或文档不会根治这个问题。

### 3.3 为什么结果仍像盲盒

WebCodex 已有很多证据字段，但它还没有控制完整任务：

1. `coding_task.rs` 明确只做确定性聚合，不运行 LLM、不制定计划、不重试失败。真正的 agent loop 完全属于外部 ChatGPT、Claude 或其他 client。
2. MCP 默认把 76 个 runtime tools 全部暴露给模型。compact schemas 默认关闭；即使打开，也只删除 `outputSchema`，不会减少工具数量，测试还要求 compact payload 大于 10 KB。
3. `tool_manifest(intent=coding)` 只是另一次工具调用返回的推荐列表，不会改变 MCP `tools/list`，也不会约束模型只能走推荐路径。
4. `start_coding_task` 本身有 15 个输入字段；默认还会组合 runtime、Git、规则、manifest 和 semantic-navigation 信息。它减了调用次数，却没有减少模型需要理解的概念数。
5. 现有 eval harness 由脚本预先决定每一步工具调用，验证的是 runtime mechanics，不是模型能否自行选对工具、恢复失败、运行正确验证并调用 finish。
6. `finish_coding_task` 是可选聚合调用，而且不会关闭 session；低层 `close_session` 是另一个工具。client 忘记 finish 或中途停止时，没有产品层保证用户仍能拿到完整结果。
7. browser console 的源码明确限定为 runtime/agent 只读状态页，没有 task 列表、diff、validation、job、approval 或结果视图。
8. 默认 permission mode 是 `dev_auto_approve`；`require_approval` 目前不是等待人工决定，而是以 `require_approval_not_implemented` 拒绝所有受控写操作。

还有一个容易被“finish summary 很完整”掩盖的关键缺口：

- 任务开始会读取当前 Git 状态，但 session record 不保存可比较的 worktree baseline。
- 任务结束时 `show_changes` 展示的是此刻整个工作区相对 Git 的变化。
- 因此在本来就 dirty 的仓库里，系统能提示“有已有改动”，却不能可靠区分“任务前已有改动”和“本任务新增改动”；通过 `run_shell` 产生的变化尤其难以归因。

用户真正需要的不是更多总结字段，而是能够回答三个简单问题：

1. 它现在进行到哪一步？
2. 这些变化哪些是本次任务造成的？
3. 我能否安全接受、拒绝或继续？

当前产品还不能稳定回答这三问，所以会产生盲盒感。

## 4. 产品定位和边界

### 4.1 建议的一句话定位

> 用一条命令把线上模型接到当前私有仓库，并为每次修改提供可归因的 diff、验证证据和风险控制。

这句话比“76 个工具”“多传输协议”或“完整 session ledger”更接近用户购买的结果。

### 4.2 首要目标用户

先服务一个窄而明确的用户：

- 熟悉 Git，但不想维护一套复杂 agent 基础设施的个人开发者或小团队。
- 希望继续使用 ChatGPT、Claude 等现有 client，而不是再绑定一个模型供应商。
- 代码不能直接交给 hosted coding agent，或者希望保留本地执行与审查边界。
- 能接受自托管，但不能接受每次使用都手工管理 server、agent、token 和 project id。

生产级多租户、复杂 OAuth、QUIC 调优和企业 fleet 运维继续保留，但从默认帮助和首次体验中移到 advanced/admin 路径。

### 4.3 暂不追求

- 不做完整终端 coding agent、IDE 或模型供应商。
- 不在近期把 LLM loop 塞进 server。
- 不追求工具数量、语言数量或传输协议数量。
- 不先做插件市场、subagent、computer use 或完整 DevOps 自动化。
- 不为了代码结构美观先进行 workspace 拆分或大文件重构。

如果经过“窄工具面 + 官方 client instructions + 任务结果中心”后，真实模型成功率仍然达不到目标，再单独评估可选 local orchestrator。它应该是一次有数据的 go/no-go 决策，而不是当前默认方向。

## 5. 目标体验

### 5.1 本地首次体验

理想入口应接近：

```text
$ cd my-repo
$ webcodex start

✓ Server ready      http://127.0.0.1:8080
✓ Agent online      local-a13f
✓ Project ready     my-repo
✓ Safety profile    reviewed

MCP URL:  http://127.0.0.1:8080/mcp
Client setup: webcodex client-config <local-client>
Task review: http://127.0.0.1:8080/console/tasks
```

这条命令应真正完成启动、注册和 readiness 检查。重复运行必须幂等，Ctrl-C 或 `webcodex stop` 必须能收干净；用户不应手动执行 `openssl`、`curl`、查 config path 或记完整 runtime project id。

这条 localhost 路径只适用于能访问本机的 client。ChatGPT 等 hosted client 仍需要公网 HTTPS；CLI 应直接识别并说明这个边界，不能给出一个 hosted client 实际无法访问的 localhost 配置。

### 5.2 连接远程 server

远程 HTTPS 是无法用产品文案消除的真实边界，应明确做成第二条路径：

```text
$ webcodex connect https://code.example.com --repo .
✓ Credential stored in profile work-laptop
✓ Agent online
✓ Project registered
✓ Public MCP health verified
```

若公网 HTTPS、证书或反向代理不满足条件，命令应停止并给出一条可执行的修复建议。第一版不应默认安装或启动第三方 tunnel；可以检测并生成配置，但必须让用户明确选择。

### 5.3 每个任务的可见结果

无论 client 最后说了什么，WebCodex 自己都应保留一张稳定的 Task Result：

```text
Task: wc_sess_...
State: needs_review
Progress: inspect ✓  edit ✓  validate ✓  review ✓

Task-attributed changes: 3 files
Pre-existing workspace changes: 2 files (untouched)
Validation: 4 passed, 0 failed, 1 skipped with reason
High-risk actions: 1 approved shell, 0 denied
Active jobs: 0
Result: ready for review
```

用户应能从 CLI 或 browser console 打开同一结果，并查看按文件 diff、验证摘要、警告、批准记录和回滚入口。模型的自然语言总结只是补充，不再是唯一结果来源。

## 6. 十二周建议路线

以下时间按一个主开发者、保持现有测试纪律估算。阶段出口比具体周数更重要：上一个阶段没有达到验收指标，就不要靠并行增加新功能来掩盖。

### 阶段 0：建立真实基线（第 1 周）

目标：从“功能存在”改为“真实 client 能完成任务”。

交付：

- 冻结新增 model-visible tools，建立 tool-surface budget。
- 建立 8 个小型 golden tasks：只读理解、文档修改、单文件 bug、多文件修改、验证失败恢复、dirty worktree、agent 掉线、危险 shell 请求。
- 至少用一个真实 MCP client 和 GPT Actions 各跑三轮；若自动化受限，先用可复核的人工 acceptance 记录，不伪装成自动模型评测。
- 在现有无内容遥测基础上记录：任务阶段、工具名、成功/失败、调用次数、响应字节、耗时、是否 finish；不得记录代码、diff、命令正文、token 或环境值。
- 给每个失败归类：接入失败、工具选择失败、上下文不足、执行失败、恢复失败、结果缺失。

出口指标：

- 能给出当前版本的 time-to-first-success、task success rate、finish rate、validation rate、raw-shell ratio 和中位工具调用数。
- 能从失败样本证明阶段 1 和阶段 2 的优先级，而不是只凭感觉排期。

### 阶段 1：做一个真正的产品入口（第 2～4 周）

目标：当前仓库里一条主命令启动，一条命令诊断。

交付：

- 建立唯一的用户入口 `webcodex`：`start`、`status`、`stop`、`doctor`、`client-config`。
- `webcodex start` 复用现有 server/agent 代码，真正监督两个进程、等待 readiness、注册当前 repo 并保存 profile。
- profile 名和 client id 默认从规范化 repo path 稳定派生；重复运行复用 profile，不要求 `--overwrite`。
- key 自动生成并写入 0600 文件；默认只显示存储位置和短前缀，通过显式 `client-config` 输出客户端所需配置，避免在普通日志重复打印 secret。
- `client-config` 把完整 runtime project id 写入生成的 client instructions/profile，日常使用不再要求用户记忆或手工粘贴；多项目时仍让模型先列出并确认项目，不能静默选错。
- `status` 默认从当前目录找到 profile，输出 server、agent、project、public URL、client readiness 五项红/绿结果。
- `doctor` 默认继承 profile，不要求用户重新传 server URL、多个 token file、agent config 和 project id；错误必须包含 `reason_code` 与一条下一步命令。
- 把 token、pairing、systemd、ops 等现有能力收进 `webcodex admin ...` / `webcodex service ...`，默认帮助只显示日常路径。
- 第一阶段先做 façade，不把“拆三个 crate / 重命名所有内部 binary”作为前置条件。

出口指标：

- 已安装 binary 的新用户在干净 Linux 环境中，用不超过 2 条 WebCodex 命令完成本地首次成功。
- 不需要手写 key、不需要 curl、不需要手工启动第二个进程、不需要把完整 project id 粘进 prompt。
- 从命令开始到 agent/project ready 的 p50 小于 30 秒（不含首次下载安装）。
- 第二次运行幂等，不新增 credential 或重复 project。

### 阶段 2：让模型默认只看到正确路径（第 4～6 周）

目标：从“推荐模型少用工具”升级为“默认就只给模型核心工具”。

交付：

- 引入真实的 tool surface profile：`coding-core`（默认）、`read-only`、`advanced`。
- MCP `tools/list` 必须按 profile 实际过滤；不能再只靠 `tool_manifest` 告诉模型忽略其余工具。
- `coding-core` 以现有 coding intent 为起点，经阶段 0 数据收敛到 12～18 个工具。默认不包含：低层 session/current-session/message 工具、compatibility edit、artifact upload、checkpoint 管理、destructive cleanup、运维发现工具。
- `coding-core` 中的 write、shell/job 和 validation 调用必须归属一个有效 active workflow session；缺失时返回可恢复错误并指向 start，而不是产生无法形成 Task Result 的游离修改。advanced profile 可保留现有低层行为。
- advanced profile 保留完整能力，避免调试和特殊任务无路可走；profile 必须在 client setup 时明确可见。
- MCP compact 和 GPT Action compact 从实验开关变成经过兼容验证的默认输出策略；compact 的成功标准是减少模型上下文和失败率，不只是 JSON 字节变小。
- 提供版本化、由 `webcodex client-config <client>` 自动生成的官方 instructions：start、inspect、structured edit、validate、show changes、finish，以及标准错误恢复规则。
- 统一常见错误的结构：`reason_code`、`retryable`、`user_action_required`、`suggested_command`，覆盖 agent offline、unknown project、session mismatch、validation unavailable 和 approval required。
- 暂不新增复合工具。先证明缩小现有表面积仍不够，再评估 bounded multi-read 或统一 `validate_project`。

出口指标：

- 默认 MCP tool count 不高于 18，advanced 保持完整。
- `tools/list` 序列化体积比当前默认下降至少 70%。
- golden tasks 的工具选择错误率下降至少 50%。
- 小修改任务 finish rate 不低于 95%，structured edit 使用率不低于 90%，无理由 raw shell 使用率低于 10%。
- `coding-core` 下 100% 的 consequential tool calls 能在对应 task ledger 中找到。

### 阶段 3：建立可归因的 Task Result 与最小 review console（第 6～9 周）

目标：即使模型中断或总结失真，用户仍知道发生了什么。

交付：

- 用户界面只使用 “Task” 这一种心智，直接复用现有 canonical `wc_sess_*` handle；wire 层先保留 `session_id`，不要同时新增同义 `task_id`。若未来确需改名，应做一次命名迁移，而不是长期双发 alias。current binding、recording session 留在高级诊断中，内部不必强行合并两种 session 存储。
- 为 writable task 在 agent 侧建立有界 baseline，至少记录 HEAD、index/worktree 状态、初始 changed paths 与必要 hash；baseline 内容留在 agent 机器，不进入 server 日志。
- finish 同时返回 `task_changes` 与 `pre_existing_changes`。若 shell 或超限导致无法可靠归因，必须标记 `attribution=partial`，不能把整个 worktree 冒充为本任务结果。
- 定义一个稳定的 Task Result schema：state、progress、attribution、changed files、validation、permissions、jobs、warnings、evidence integrity、next actions。
- 从 session event 推导 inspect/edit/validate/review progress，并显示 stalled、agent_offline、needs_user_action 等状态。
- 增加 task list/detail 查询；用户丢失 chat 中的 session id 后仍能找到任务。
- 扩展现有轻量 console，而不是先引入大型前端框架：任务列表、结果卡、按文件 diff、validation、warnings、复制 handoff。
- 做一次明确的 lifecycle 设计任务：普通用户完成 task 时应得到一个终态。可选择让 finish 原子 close，或由产品 façade 在成功聚合后 close；不要继续要求模型理解 `finish_coding_task` 与 `close_session` 的区别。

出口指标：

- 所有 writable golden tasks 都产生 Task Result；client 中断的任务能显示 incomplete/stalled。
- 干净工作区的变更归因率为 100%；dirty worktree 中结构化编辑的变更归因率为 100%；无法证明的 shell 变化明确标 partial。
- 用户无需读原始 tool JSON，即可回答 changed files、validation、remaining jobs 和是否可 review。
- task result 与 CLI、console、MCP finish 三个入口的核心字段一致。

### 阶段 4：完成真正的审批闭环（第 9～12 周）

目标：在“全自动执行”和“全部拒绝”之间提供可信的中间态。

交付：

- 将 `require_approval` 从立即 deny 改为持久化 pending request。
- pending 记录绑定 principal、project、task、tool、risk、参数摘要、前置状态 hash、TTL 和一次性 decision id；不能保存 secret 或未经脱敏的 command/env 内容。
- 在任何 agent enqueue 或 mutation 前停止；批准后重新校验 task lifecycle、project、worktree precondition 与 TTL，防止批准对象被替换。
- CLI 和 console 支持 approve once、deny、查看风险摘要；先不做复杂规则编辑器。
- 第一版风险策略：read 自动；结构化 edit 可按 profile 自动或询问；raw shell、job、destructive cleanup/checkpoint restore 默认询问。
- 把“批准执行高风险工具”和“接受最终 diff”做成两个不同概念，避免审批一次就被误解为接受所有代码变化。

出口指标：

- `require_approval` 下高风险调用能 pending、approve/deny、过期，并且未经批准绝不 enqueue。
- 重复 approve 不会重复执行；过期或 worktree 已变化的批准会安全失败并说明原因。
- console 能从 pending request 直达对应 Task Result。
- reviewed profile 的 golden tasks 无静默高风险自动执行。

## 7. 十二周后再做的事项

只有前四阶段达到出口指标后，再按真实失败数据选择：

1. macOS arm64、Linux arm64 和 Windows artifact；installer 已有平台分支，重点是 release manifest、CI matrix 和 smoke。
2. Python/TypeScript 的结构化 validation profile，而不是继续只增加 LSP server。
3. 有界 multi-read、并行只读 dispatch、结果缓存等 RTT 优化；先用 trace 找到真实 p95 瓶颈。
4. selective rollback / accept-by-file；必须建立在可靠 task baseline 之上。
5. 团队级 policy pack、审计导出和多项目 task center。
6. 可选 local orchestrator；仅当外部 client 即使在窄工具面和官方 instructions 下仍无法稳定完成任务时立项。

## 8. 建议采用的产品指标

| 指标 | 定义 | 十二周目标 |
|---|---|---|
| Time to first success | 安装完成后，到一个可访问本机的 client 成功读取当前 repo | 本地 p50 ≤ 5 分钟 |
| Setup command count | 首次 ready 前用户输入的 WebCodex 命令数 | ≤ 2 |
| Task success rate | golden task 满足结果断言并产生 Task Result | 只读 ≥ 95%，小修改 ≥ 85% |
| Finish/result rate | 启动过的有效任务有可查询终态或明确 incomplete | ≥ 95% |
| Change attribution | changed path 能区分 task 与 pre-existing | structured edit 100% |
| Validation appropriateness | 应验证的任务运行了匹配验证，跳过有理由 | ≥ 90% |
| Tool selection errors | 选兼容/错误/不必要高风险工具的次数 | 相对基线下降 ≥ 50% |
| Raw shell ratio | 非必要 raw shell / 全部写与验证调用 | < 10% |
| Review completeness | 修改任务含 diff、validation、warnings、jobs | 100% |
| Recovery quality | 常见错误带可执行下一步 | 100% 指定错误集 |

这些指标只记录结构化元数据。不要为了评测方便采集源码、diff、prompt、command body、token、env 或测试完整输出。

## 9. 优先级清单

### P0：立即开始

- 真实模型 acceptance 与无内容 task telemetry。
- `webcodex start/status/stop/doctor/client-config` 产品 façade。
- 默认 MCP tool surface profile，目标 12～18 个工具。
- Task Result schema、task list 和 agent-side baseline 设计。
- 新增 model-visible tool 冻结规则与 surface budget。

### P1：P0 有基线后开始

- 最小 task review console。
- 真 pending approval + CLI/console approve/deny。
- compact 默认化和统一 recovery error shape。
- 多平台 release artifact，至少 macOS arm64。

### P2：由数据触发

- 多语言 validation adapters。
- bounded batch inspect / validate_project。
- selective rollback、团队 policy 和审计导出。
- optional local orchestrator。

## 10. 明确停止或延后

- 停止用新增工具解决模型不会编排的问题。
- 停止把 internal schema 完整度、tool count 或文档页数当作产品进展。
- 延后新的 LSP 写能力、更多 transport、复杂 OAuth 形态和 fleet 功能。
- 延后无用户收益证明的 compatibility layer；用现有 edit telemetry 决定隐藏和删除窗口。
- 延后 workspace/crate 大重构。CLI façade 可以先调用现有模块，避免产品改进被内部整理阻塞。
- 不把补写更多 quick-start 步骤当成轻量接入；正确做法是删除步骤。

## 11. 开发执行纪律

每个产品阶段都应遵守以下门槛：

1. 先写用户场景和失败断言，再写实现。
2. 每增加一个默认命令、概念或 model-visible tool，必须同时删除或隐藏一个旧入口，除非有评测证明净收益。
3. 所有 compact、推荐 flow 和新 schema 都必须用真实模型任务验证，不能只比较 JSON 大小或单元测试通过。
4. 核心结果必须能由 WebCodex 自身查询，不能只存在于 client 的自然语言回答里。
5. 任何声称“本任务修改了这些文件”的结果都必须有 baseline 证据；证据不足时诚实标记 partial/unknown。
6. 默认帮助、README 首屏和 release acceptance 只围绕同一条首次成功路径，advanced 运维另行展开。

## 12. 最终建议

WebCodex 现在已经越过“能不能做出来”的阶段，进入“能不能忍住继续堆能力、把已有能力压缩成产品”的阶段。

最有价值的下一步不是追赶 Claude Code/Codex 的 agent 功能总量，而是建立一个它们未必优先解决的体验：用户几分钟内把任意线上模型接到私有仓；模型只看到足够完成任务的工具；每个任务的变化、验证、风险和遗留状态都能由 WebCodex 独立说明。

如果只能选择三个开发主题，应依次选择：

1. 一条命令的真实启动与自动诊断。
2. 默认窄工具面与真实模型成功率评测。
3. 有基线的 Task Result 与 review console。

这三项完成后，审批、多平台和多语言才会变成乘数；在此之前，它们只会让一套仍然偏重、仍然像盲盒的系统拥有更多功能。
