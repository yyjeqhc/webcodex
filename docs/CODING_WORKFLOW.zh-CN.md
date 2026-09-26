# Coding 工作流

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

本文面向普通 WebCodex coding/review 工作，只说明模型真正需要遵循的流程，不展开内部 continuity、audit 或 transport 协议。

## 普通循环

日常 WebCodex coding loop 应该保持很小：

```text
work_on_project
→ inspect / search / read
→ edit
→ substantial work 调用一次 present_work_result
→ focused validation
→ review changes
→ finish_coding_task
```

`work_on_project` 是普通 coding/review 的 canonical bootstrap。把当前任务 instruction 交给它，然后遵循连接到的 Server 返回的 project instructions 与 tool surface。

对于 substantial coding，在 Workflow Session 进入真实工作状态后（例如第一次有意义的源码 mutation，或开始长时间 validation），对该 exact Session 调用一次 `present_work_result(project, session_id)`。挂载后的 MCP App 会自行进行有界的 Workspace / Validation / Review live read，因此不要重复创建卡片，也不要为了给卡片喂状态而额外消耗 model turn。tiny/read-only 工作不需要 progress card。`finish_coding_task` 在 non-blocking closeout 时 seal eligible final changes，已经挂载的同一张卡会在后续 App refresh 中发现这份 immutable snapshot；如果此前没有挂卡而 closeout 明确返回 presentation suggestion，再在收尾时调用一次即可。
它的 primary output 默认保持紧凑，不重复静态 instruction/workflow 正文；当前模型上下文缺少这些材料时，分别显式请求 `context_request=["project.instructions"]` 和/或 `context_request=["webcodex.workflow"]`。Workflow Session identity 不证明当前模型仍保留这些上下文。
Bootstrap 或 discovery 返回 `project_ref` 后，普通 Project-scoped tool call 的 `project` 应优先复用这个短 selector。Canonical `agent:<client_id>:<project_id>` 仍保留用于 diagnostic 与显式 addressing，但模型无需机械重复。`project_ref` 由 Server 持久维护、按 principal 隔离，不携带 authority；每次调用都会根据其钉住的 canonical Project/root identity 重新授权。

当 `work_on_project`、`start_session`、`session_summary` 或显式 handoff 返回 `session_ref` 时，后续显式 Session 选择可优先复用这个短 selector。Business `session_id` 与 wrapper `recording_session_id` 仍是两套独立语义，但都可以显式携带已签发的 ref：Runtime 会先把它还原为钉住的 canonical `wc_sess_*`，再执行各自原有的授权、生命周期或 guard 逻辑。Canonical identity 仍是持久化、审计、诊断和内部关联的权威身份。`session_ref` 只是按 principal 隔离的便利选择器；省略 recorder 时不会自动推断，也不会形成隐式或粘滞的 recorder context。
默认情况下，它还会返回一个很小且有界的 `extensions` selection catalog：Skill metadata 来自 canonical 的 project / Runner-configured `skills.roots` / Runner-managed Skill Store 三类来源；Plugin metadata 只包含 configured working directory 与当前 Project root 匹配、且已 ready/committed 的 provider。该 metadata 不授予任何 authority，也不会自动读取 Skill body 或创建 Plugin binding；模型选择后使用 `skill_read_file` 读取 Skill 文本，`run_skill_resource` 只执行可信 Runner-configured live `scripts/` resource（由 `expected_definition_revision` fence definition）或 Runner-installed managed resource（另由 `expected_package_revision` fence package），Plugin 则走 `plugin_tool describe -> call`。Configured resource bytes 会一直保持 live 到实际执行时，并不会预先被 package revision 固定。只有当前模型上下文仍明确保留这些 discovery metadata 时，才应设置 `include_extension_catalog=false`。

## 工具策略 guidance

`work_on_project` 的 `guidance_profile` 默认是 `direct`。Workflow contract v21
保持共享的 `guidance`、`model_protocol` 和 review `roles`，并在显式
`context_request=["webcodex.workflow"]` 时通过
`tool_strategy` 返回本次请求选中的策略。

- `direct`：简单 observation 直接调用最合适的 primitive；预先确定且独立的
  observations 可以批量执行，模型根据结果顺序决定 adaptive follow-up。
- `host_code_mode`：Host 确实提供 native orchestration 时使用。预先确定的同类输入
  首先使用工具自己的 canonical batch，不要拆成同类 micro-call 并发；预先确定、互相
  独立的 cross-tool read-only observation 才适合 Host 并行。native batch 之后若仍需
  cross-tool fan-out，partial evidence 仍有价值时优先 `Promise.allSettled`，只有真正的
  all-or-nothing 才使用 `Promise.all`。对于 result-dependent search/read/branch chain，
  只要下一调用由结果机械确定且没有新的语义判断，就继续留在
  同一个 Host cell。单个 child ToolResult 返回本身不是 model-turn boundary；需要 semantic
  choice、ambiguous result、新用户决策、authority/permission、uncertain outcome、竞争性
  recovery 或 mutation intent 尚未确定时才自然回到模型。完整 ToolResult 尽量留在 Host
  cell，只返回下一次决策需要的紧凑证据。每个 Host cell 应是短生命周期 dependency DAG，
  而不是承载长时间 Job lifetime。Job handoff 保存精确 identity 后，先完成已经确定的独立工作；
  如果剩余工作主要只是等待，就结束当前 cell，之后从 exact continuation 恢复，不要让 cell
  持续挂在长等待上，也不要因为 Job 存在就机械 `observe_jobs`。startup
  `tool_strategy.host_orchestration` catalog 与 exact
  `tool_manifest(tool_name=...)` hint 都从 canonical `ToolDefinition` metadata 派生；
  它们只提供 guidance，不改变 `ToolCompositionPolicy`、authority、effect、permission、
  retry、idempotency 或 runtime scheduling，默认/broad ToolSpec 也不携带这批 metadata。
  该 profile 不授予任何 WebCodex capability/authority，也不要求 nested WebCodex Code Mode。
- `code_mode`：简单单步 observation 仍直接调用；相关 search/read、跨文件定位或
  综合调查能减少外层模型往返时，优先 read-only Code Mode。在同一个 cell 内顺序
  完成依赖结果的 follow-up，只并发独立 observations。Raw child results 留在 cell
  内，先筛选、提取、交叉引用和归纳，再用 `text(...)` 输出下一步决策需要的紧凑证据；
  避免 `text(results)` 原样倾倒，并在触及 outer-output limit 前主动 projection。

这只是本次请求的 presentation 选择，不增加 admission、权限或 execution semantics，
不写入 Session。Exact resume 可以重新选择，也不会根据 Window、Session 或历史调用
猜测。未编译 Experimental Code Mode 时，显式 `code_mode` 被拒绝为无效输入。
在 `work_on_project` 调用中，`webcodex.workflow` sidecar 使用该次请求的
`guidance_profile`；其他无 profile context 的普通工具显式请求该 material 时，
继续使用 canonical default `direct`。

这些策略共用 scope、recovery、validation truth、Job continuation、review 和 closeout。
默认仍走 canonical edit 和 structured validation；只有多个相关 validation 或
adaptive read → one guarded edit 确实减少外层往返时，才考虑相应的 effectful/mutating
Code Mode。Nested canonical authority、effects、evidence 和 retry certainty 不变。

## 开始或继续任务

新任务和显式 continuation 都使用 `work_on_project`。WebCodex 会保留有界 Workflow Session evidence，让 validation、review 与 handoff 可以指向同一轮工作，但 Workflow Session 不是认证凭据，也不会扩大 project authority。

普通使用不需要理解 WebCodex 内部的 continuity/audit field；这些属于 implementation/maintainer contract。

内置默认 guidance 本身就是普通 implementation workflow。正常的“implement/fix/refactor”任务不需要知道任何实现角色名：把已授权工作推进到具体、可评审的完成状态，端到端覆盖跨层改动，保持设计最小化，按范围验证，并如实报告证据。只使用当前暴露 schema 支持的工具与协议字段。

`independent_review` 是唯一保留的可选 named role，因为它确实改变行为。只有任务明确要求独立评审 pass 时才使用：

```text
使用 independent_review guidance。独立评审 <改动或 commit>，
报告有文件/行号证据和影响说明的具体发现，不修改文件。
```

如果也希望修复，明确补充“修复具体发现，并运行聚焦回归验证”。单独指定评审角色不代表授权修改，任何 role 都不会授予额外 authority。

Guidance 通过工具结果交给客户端，不是客户端的 system prompt，也不会授予执行权限。Host 指令、用户任务、适用项目规则、认证和运行时安全策略仍然有效。返回 guidance 不等于模型已经读取、记住或遵守；只有当前模型上下文仍保留内容时才应关闭其返回。

## 编辑前先检查

选择能够保持正确语义的最简单 inspection primitive。已经知道 symbol、test 或具体实现区域时，优先读取有界目标范围；多个相关范围在调用前已经确定时可以一起 batch。做 broad discovery 时，先用 files-with-matches、count 或少量低 context match 等窄 projection，再读取真正相关的范围。小型、已知 scope、输出可预测的 native `rg` 通过 `run_process`/`run_shell` 同样是一等路径。

Bootstrap 只读取固定的几个指令入口，不会扫描所有子目录规则。修改某个路径前，需要检查适用的子目录指令，并补读相关缺失或被截断的规则内容。

做 branch/PR review 时，先使用当前 Server 提供的有界 review/change-summary 工具，再按需要缩小到具体文件或 diff hunks。

## 编辑

模型生成的普通编辑，在 `read_files` 读取当前源码后，canonical/default 路径是 `apply_text_edits`。`read_revision` 是模型侧的 snapshot handle；需要 whole-file stale-context fence 时，把它作为 `expected_read_revision`。全局唯一的 exact local edit 可以不带 revision；使用位置型 `line_scope`/`occurrence`、delete 或 rename 时必须携带。ToolRuntime 会在内部把 revision 解析成 Runner 的精确 SHA guard，模型不需要复制 digest。即使一次修改很多行，默认路径仍然不变；“改动行数多”本身不是选择 `apply_patch` 的理由。只有当 contextual patch 明显更自然、large/multi-hunk rewrite 用 guarded exact edit 表达明显笨重，或 patch-style context 本身更清楚地表达修改关系时，才使用 `apply_patch`。对于 repetitive code，每个 patch chunk 都必须带稳定且唯一的 surrounding context，优先使用 containing function / impl / type / test / module；不要只拿重复出现的单行或短片段作为 mutation anchor。保持请求原有的 matching guard，不要削弱明确的 stale-context/concurrency fence。输入本身已经是标准 unified diff 时才使用 `apply_unified_diff`。

Guard failure 是 **zero-write conflict**，不是削弱 guard 的理由。重新读取当前源码，并基于最新状态重新生成原本的编辑。

遇到 `matching_mode_rejected` 时，保持 matching guard，不要切换到 `first_match`。先重新读取当前源码；如果原本修改很容易表达成 exact edit，优先转为 `apply_text_edits`，需要位置型或更强的 whole-file fence 时使用当前 `read_revision`。如果 patch 形式仍明显更合适，则消费返回的有界、parser-ready `read_files` recovery call，并保留原请求的 patch guard。不要降级明确的 stale-context/concurrency fence。

对于确定性的 `context_mismatch`，同样消费有界 `read_files` recovery，并基于 current source 重新生成 patch；不要盲目重复相同 patch。若结果是 `outcome_unknown`，先检查 workspace，再决定是否允许任何写入重试。

具体 matching metadata 与 transactional protocol 属于维护 WebCodex 本身时才需要的细节，应以 tool contract/tests 为准。

## Validation

Formatting 属于收尾，不是每次编辑后的 validation。普通循环是：编辑 → focused validation → 必要时继续编辑 → 源码稳定 → format 一次 → 最终 review/validation。Rust 格式化应在相关源码稳定后、最终 diff/closeout 前执行；只有后续 Rust 编辑可能改变格式时才重跑。`cargo_fmt(check=false)` 用于有意执行最终格式化，`check=true` 用于需要最终只读格式证明的情况。CI/release 格式检查保持不变。

能使用 `cargo_test`、`cargo_check`、`go_test` 等 structured validation 时优先使用它们。先运行能够发现当前回归的最小检查，只有实际受影响的边界需要时才扩大范围。

检查一个 Cargo workspace package 时，`cargo_check` 接受 `package`；检查多个 package 时传入 `packages`。WebCodex 会对该集合排序并去重，然后在同一个 Cargo 进程中使用重复的 `-p` selector。两个 selector 互斥，显式空列表无效。

如果一个确定需要执行的 validation 超过 Server 管理的 synchronous grace，它会自动以**同一个 execution** handoff 为 Job，模型不需要调整 handoff timing。随后只继续独立的源码读取、搜索、diff/architecture inspection 或 review，再观察该 Job；不要为了“并行”额外启动 CPU-heavy validation。如果运行中的 validation 所覆盖源码随后发生 mutation，那么其结果只能算 stale/cache-warmup evidence，不能证明 final workspace；最终源码仍需重新运行 task-appropriate validation。

如果某次 test invocation 必须证明“测试确实执行了”，使用 `require_tests: true` 或 `min_tests: N`。它们是本次调用的 evidence assertion，不会自动变成 Workflow Session 的持久要求。如果 validator execution 成功，但请求的 test 数量未满足或无法证明，closeout 会把这次调用保留为 evidence gap，而不是代码/测试 correctness failure。否则，exit-zero 但合法运行零个 test 只是 execution result，并不能证明 test coverage。

把 validation failure 当作 evidence，而不是必须立即清空的队列。如果失败说明当前实现方向无效，或者阻塞后续依赖工作，就先诊断修复；否则保留该 evidence，继续真正独立的工作，在依赖或 closeout 需要时再解决/重验。故意重跑同一个逻辑 assertion 时复用 `assertion_name`。Mutation 会让相关旧 evidence 变 stale，`outcome_unknown` 继续 fail closed。

只有 structured validation 无法表达检查时，才使用 shell/process escape hatch。

## Review 与 closeout

编辑和 validation 之后，必须检查真实 workspace/diff。Tests 通过不能替代 diff review；反过来，diff 看起来合理也不能替代行为变化所需的 focused validation。

`finish_coding_task` 返回有界 closeout evidence。把它当成 advisory summary，不要把它当成“任务已经正确完成”的 authority。最终工程判断仍由模型完成并向用户报告。

## 长时间运行的工作

命令或 validation 超过同步等待窗口时，会作为同一条 WebCodex Job 继续执行。保留其精确 Job identity 与 parser-ready continuation；如果仍有有用的独立工作，就先继续这些工作，之后再 observe，不要为了“保持可见”反复轮询 running Job。只有下一步真正依赖 terminal result 时，才使用返回的 continuation；Server 会按照当前 MCP Host profile 对 observation wait 做有界适配。Runtime 内部仍保留 transport-neutral observation ceiling，而 MCP 等待会适配 Host budget。单个 Job 或任一 terminal result 即可推进时使用 `terminal`；预先确定的一组 Job 必须全部结束才能推进时使用 `all_terminal`。Recovery/continuation hint 不会授权对不确定 effect 做 retry。

## 手动多窗口协作

多窗口协作属于高级 maintainer workflow，不是普通 coding loop。独立 writer 应使用不同 worktree/Project，并保持各自 Workflow Session 分离；使用当前 Server 返回的 assignment/completion 工具，不要复制另一个窗口的 execution history。

精确的 concurrency、retry、provenance 与 cross-Session authorization 规则见 [Manual Multi-Window Collaboration](agent/manual-window-collaboration.md)。对应 protocol field 有意不放在普通工作流里。

## 如何判断是否有效

运行时测试可以证明 guidance 的返回一致、有界、符合 schema，且不会变成执行权限。`scripts/eval_coding_loop.sh` 检查的是脚本化工具循环，没有运行模型，不能衡量模型是否遵守提示词。

要衡量行为收益，应固定模型、工具、参数与任务样本，对比有无 guidance 的多次运行。样本至少包括小型修复、只读评审、已有无关改动、子目录规则，以及结果不确定的长时间操作。先比较正确性与任务范围保持，再比较不必要的询问、重复执行、验证质量、工具调用和 token 成本。不能从 schema 测试通过推断模型成功率提升。

## 内部协议细节

开发 WebCodex 本身时，直接阅读 maintainer contract，而不是继续扩充这份普通用户指南：

- [Session model](agent/session-model.md) —— Workflow Session continuity、message 与 evidence 语义。
- [Authority model](agent/permission-model.md) —— execution authority 与 hard-safety layering。
- [Job reliability and concurrency](agent/job-reliability-and-concurrency.md) —— Job recovery/observation contract。
- [Architecture decisions](agent/architecture-decisions.md) —— 当前长期有效的实现决策。

普通 coding client 不应为了完成日常仓库任务而必须理解这些内部文档。
