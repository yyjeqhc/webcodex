# WebCodex Project-first 精炼与 Execution Engine 收敛计划

> 状态：当前长期开发基线
>
> 适用分支：`refactor/project-first-experience`
>
> 基线日期：2026-07-23
>
> 本文决定近期开发顺序。已有七轮 Hosted 文档继续作为实现记录；若其“下一步”与本文冲突，以本文和 `ROADMAP.zh-CN.md` 为准。

## 1. 结论

`refactor/project-first-experience` 仍然是后续开发基础，不回到 main 的 76-tool Hosted 模型面，也不重新把旧 session、client id、runtime project id 和 ops 工具暴露给 Hosted Chat。

这个分支已经完成正确的产品层重构：

- Hosted 模型面收敛为 9 个 project-bound capabilities；
- Project、Workspace、Task、Run、Event、Result 和 Approval 成为独立事实；
- 写任务拥有隔离工作区、稳定 patch、本机 accept/reject 和一次性命令审批；
- 多文件编辑具备 precondition、幂等重试、全批 preflight、回滚和 uncertain fail-closed。
- command/check Execution 已拥有统一的 durable lifecycle、quick-yield、bounded review、取消和重启恢复。

Iteration 6 已把 `commands_run` 从旧同步 `run_shell` 路径迁出，Iteration 6.1 稳定了调用方幂等、starting cancellation、单 monitor、transport grace 和 workspace release retry。Iteration 7 又把 `checks_run` 迁入同一个 `wc_executions`/`ExecutionService` 生命周期，并删除同步 checks adapter、重复 validation projection、无生产调用者的旧 Principal 抽象和两条专用写文件 REST 兼容路由。Iteration 7.1 把成功 check 绑定到 durable workspace provenance，删除可被项目输出伪造的 stdout progress marker，并持久化 bounded assertion evidence。Iteration 7.2 把 mutation/finish 放入同一 per-task 原子边界，并要求 terminal validation verdict 有完整 structured progress。Iteration 8.0 建立 project-first setup、credential boundary 和 authenticated golden path，正式基线为 `331712537d989b3b3268da55e2984e1054a26cac`。Iteration 8.1 在同一九项 Hosted surface 内增加 Rust、Node、Python、Go project-aware validation recipe，当前等待人工 review。

因此接下来的主线不是增加 Hosted capability、远程领域工具或通用 workflow，而是：

> **建立唯一的、持久的、可观察、可取消、可恢复的 Execution Engine，并用它替换 Hosted 路径对旧同步执行语义的直接依赖。**

## 2. 当前规模基线

基于 2026-07-23、HEAD `9dc8aa9e834c` 的简单 path-based 统计：

| 指标 | 当前值 |
|---|---:|
| production Rust 文件 | 246 |
| production Rust LOC | 120,995 |
| test-path Rust 文件 | 90 |
| test-path Rust LOC | 56,454 |
| 分支相对 main 的 production Rust 净增长 | +11,439 |
| 分支相对 main 的 test-path Rust 净增长 | +41 |

最大的 Project-first 新模块：

| 文件 | LOC |
|---|---:|
| `src/connector_runtime/mod.rs` | 3,289 |
| `src/db/task_kernel.rs` | 1,944 |
| `src/hosted_connect.rs` | 1,753 |
| `src/connector_runtime/workspace.rs` | 1,551 |
| `src/task_cli.rs` | 802 |

这组数字说明：当前主要问题不是“为了测试无脑增加了大量测试”，而是新 Task/Connector 内核叠加在旧 ToolRuntime、job、session 和 transport 路径之上，尚未完成替换与删除。

测试仍需要约束，但真正的减法重点应是 production ownership、重复状态面和旧 Hosted path。

## 3. 产品承诺

### 3.1 对用户

> 在项目目录运行一个命令，让 Hosted Chat 安全地读取、修改和验证本地项目；短操作直接完成，长操作始终有状态、可取消、可恢复，最终结果由本机用户审查和接受。

### 3.2 对内部架构

> WebCodex 是 Hosted Chat 与 Private Workspace 之间的有状态工程执行运行时。线上模型拥有推理和工具编排；WebCodex 拥有作用域、审批、执行、事实、结果和恢复。

### 3.3 不变边界

- 不内置 LLM、prompt loop、agent loop 或 provider routing；
- 不变成完整 IDE；
- 不让模型代替人类 accept/reject；
- 不默认提供 autonomous DevOps；
- 不为假想兼容保留双写、aliases 或两套状态源；
- 不以工具数量、测试数量或文档数量作为完成度指标。

## 4. “精致、小而美”的验收定义

达到第一阶段“精致、小而美”，必须同时满足：

1. **一个公开入口**：普通个人用户只需要在项目中运行 `webcodex setup`，再按
   readiness 指引启动 Agent。
2. **不超过 9 个 Hosted capabilities**：每个工具只有一个清楚意图；内部 ops/admin/session 工具不暴露给模型。
3. **一个事实源**：Task、Run、Execution、Result 和 Approval 由 SQLite Task Kernel 持久化；不依赖 JSON SessionStore 或 current-session 聚合补全。
4. **一个执行内核**：短命令、长命令和 checks 使用同一个 Execution Engine；模型不再预判 `run_shell` 或 `run_job`。
5. **一个结果语义**：submission、execution、exit、assertion 和 task outcome 分开表达。
6. **始终可 review/cancel**：长执行不能持有阻止 `task_review` 或取消的 Task 级互斥锁。
7. **断线后事实仍在**：runtime/client/agent 中断后，Execution 进入可解释状态，不能只剩调用超时。
8. **默认不污染 checkout**：写任务继续使用隔离工作区，accept 前重新检查 precondition。
9. **删除大于新增**：三轮主线完成后，production Rust source 相对本基线净减少 20% 以上。
10. **指标关注有效结果**：核心指标是 calls per accepted result、maximum silent interval、polling amplification 和 terminal-unknown rate，而不是 tool-call accepted/succeeded。

## 5. 最小事实模型

近期模型保持为：

```text
Project
  -> Workspace
  -> Task
  -> Run
  -> Execution
  -> Result
  -> Human Decision
```

### 5.1 Task

用户请求的一个明确结果。Task 生命周期负责：

```text
active
needs_attention
ready_for_review
accepted
rejected
cancelled
```

### 5.2 Run

Task 在一个固定 Workspace 上的一次执行尝试。Run 不等同于一条系统进程，也不因单条命令结束而结束。

### 5.3 Execution

Run 内一项可等待执行，例如 command 或 check。Iteration 6/7 已把两者纳入同一 Execution 生命周期；一个有序 `checks_run` validation plan 对应一个 `kind=check` Execution，而不是每个 check 各建一个顶层执行。

Execution ID 使用 `wc_exec_*`。**不要复用或改变 `edits_apply.operation_id`**：后者仍是 caller-generated idempotency key，不是持久化进程实体。

Execution 至少记录：

```text
execution_id
kind
task_id
run_id
state
submitted_at
queued_at
started_at
last_output_at
finished_at
stdout_cursor
stderr_cursor
exit_code
failure_source
failure_code
cancel_requested_at
terminal_reason
operation_id
request_sha256
executor_reference
first_status_failure_at
last_successful_observation_at
status_failure_code
check_plan
check_completed
check_workspace_sha256
validated_workspace_sha256
failed_check
assertion_evidence_json
```

`operation_id`/`request_sha256` 是网络重试 identity；validated workspace
provenance 是成功 check 的业务事实。workspace 不进入 request hash，否则同 key
在漂移后可能创建第二条 Execution。成功 check 只在起止 workspace fingerprint
完全一致时写入 provenance；`task_finish` 发现缺失或不匹配时以
`checks_stale` fail closed。

状态：

```text
accepted
queued
starting
running
cancel_requested
succeeded
failed
cancelled
interrupted
unknown
```

### 5.4 暂不增加的实体

评审中提出过 Step、ExecutionAttempt、Process、Fact 和 Workflow。它们可能长期有价值，但下一轮不同时引入。

当前先用一个 Execution 表达“已受理、排队、运行、输出、取消和终态”。只有真实 replay 数据证明以下问题无法在该模型内表达时，再单独写 RFC：

- 同一业务步骤需要多次可比较 attempt；
- 一个 execution 必须拥有多个独立 process；
- 大量 verified facts 被重复确认；
- 固定 runbook 的 checkpoint/resume 成为主要需求。

这是防止再次从一个聚合模型跳到过度建模的关键约束。

## 6. Iteration 6：Execution Engine vertical slice

预计：2–3 个专注开发周。

### 6.1 单一执行语义

Hosted capability 不再同步等待最长 120 秒后把 client wait timeout 当作失败。内部执行接口采用 quick-yield：

```text
execute(
  run_id,
  kind,
  command_or_recipe,
  cwd,
  yield_ms = 8000,
  deadline,
  output_limits
)
```

- 在 yield 窗口内完成：直接返回终态；
- 尚未完成：返回 durable `execution_id`、状态、输出 cursor 和最近进展；
- client 等待结束不等于进程失败；
- 相同 `operation_id` 与相同规范化 request hash 的精确重试不得无意启动第二个进程；
- 新 `operation_id` 必须允许在同一 Task/Run 内有意重跑相同 command/cwd/timeout；
- short command 仍应一轮完成，不强迫所有调用进入手工轮询。

### 6.2 复用现有执行资产

不得再实现第二套 scheduler/process manager。优先抽取并复用：

- `src/tool_runtime/jobs.rs` 的状态、output、timeout、process-group 与 stop 语义；
- `src/tool_runtime/local_jobs.rs` 的本地持久化执行资产；
- `src/shell_client/jobs.rs`、`job_updates.rs` 的 agent job 路径；
- agent `JobManager` 的 spawn、concurrency、log 和 termination；
- 现有 shell policy、approval、workspace-bound cwd 和 output limits。

Task Kernel 是产品状态 authority；executor/job manager 是进程事实 authority。两者通过稳定 adapter 连接，而不是复制实现。

### 6.3 分开结果语义

禁止继续用一个 `success` 表达所有层次。最少区分：

```text
submission_status
execution_status
exit_code
terminal_reason
assertion_status
capability_outcome
```

Iteration 6 中 assertion 可以只支持 `not_run` 和未来扩展位置，不开发完整 assertion DSL。

### 6.4 review/watch

扩展 `task_review`，而不是重新向 Hosted 模型暴露 `job_status`、`job_tail` 和 `job_log`：

```text
task_review(
  task_id,
  after_cursor?,
  wait_ms?,
  max_events?,
  include_output_tail?
)
```

返回：

```text
active_execution
queue_age_ms
queue_reason
silent_for_ms
last_progress_at
recent_events
stdout_cursor
stderr_cursor
output_tail
blocking
next_action
```

`wait_ms` 是有界 long-poll；在新输出、状态变化、heartbeat deadline 或 terminal 时返回。普通无等待 review 仍立即返回。

### 6.5 取消

增加一个单意图 Hosted capability：

```text
task_cancel(task_id, reason?)
```

它是允许的第 9 个 capability。不要把取消塞进 `task_review.operation` 枚举，也不要重新暴露低层 `stop_job`。

取消链路必须是：

```text
request cancellation
-> durable cancel_requested
-> executor acknowledgement
-> process-group termination
-> terminal observation
-> durable terminal event
-> workspace/queue slot release
```

删除服务端 waiter、HTTP request 结束或从内存 map 移除都不等于取消完成。

### 6.6 restart reconciliation

runtime 启动时：

- 没有可验证 executor handle 的 `starting/running/cancel_requested` Execution 进入 `interrupted` 或 `unknown`；
- 不伪造 `failed` 或 `succeeded`；
- Task 进入 `needs_attention`；
- `task_review` 返回最后输出、终态不确定性和恢复建议。

Iteration 6 不承诺在 runtime 重启后重新附着任意 OS 子进程；先保证事实正确。

### 6.7 queue 可观测性

至少持久化：

```text
queued_at
queue_deadline
queue_reason
blocker_execution_id
```

达到 deadline 后进入明确终态，不允许永久 queued。复杂优先级和分布式 scheduler 延后。

### 6.8 本轮迁移范围

Iteration 6 必须完成：

- `commands_run` 使用 Execution Engine；
- `task_review` 投影 active execution 与 output cursor；
- `task_cancel` 完成端到端取消；
- `task_finish` 在 active、cancel-pending、unknown execution 存在时 fail closed；
- runtime restart reconciliation；
- Connector 不再直接同步 `invoke_kernel("run_shell")` 等待最终结果；
- 删除被替换的 Connector 同步 command glue 和只服务该路径的测试。

本轮不迁移全部 checks，不扩多语言 validation，不做 SSH，不做 Browser UI。

### 6.9 实际完成状态（2026-07-23）

Iteration 6 已在 `refactor/project-first-experience`、起点 HEAD `9dc8aa9e834c` 的当前工作树完成：

- `wc_executions` 持久化 command lifecycle、输出 cursor、失败来源、取消事实、调用方 `operation_id`、规范化 request hash 和内部 job reference；
- `commands_run` 在短 Task lock 内完成校验、审批与 `accepted` reservation，释放锁后复用 `ToolRuntime::run_job`、`ShellClientRegistry` 和 agent `JobManager`，默认约 8 秒 quick-yield；
- `task_review` 支持 `after_cursor`、最长 15 秒 `wait_ms`、bounded events 和直接来自既有 job output authority 的 bounded tail；
- 第 9 项 `task_cancel` 覆盖 durable `cancel_requested`、queued removal、agent stop、process-group termination、终态观察、Task 取消和 workspace lease 释放；
- runtime 启动将无法重新验证的 active Execution/Run 标为 `interrupted`，Task 投影为 `needs_attention`；
- `task_finish` 对 active、cancel-pending 和 `unknown` Execution fail closed；
- Connector 的 `commands_run -> invoke_kernel("run_shell")` 同步路径及其旧审批/replay 测试已删除，checks 未在本轮迁移。

本轮没有复制 scheduler、process manager 或 output store。SQLite 是 Task/Execution 生命周期 authority；既有 job/agent 层继续拥有队列、进程组和 bounded output。

简单 path-based 统计从 production `246 files / 120,995 LOC`、test-path `90 files / 56,454 LOC` 变为 production `249 files / 122,194 LOC`、test-path `92 files / 57,247 LOC`，净变化分别为 `+1,199` 和 `+793`，均在本轮硬预算内。最终 `cargo test --bin webcodex` 为 `1645 passed / 0 failed / 4 ignored`。

详细实现、删除与验证记录见
[`HOSTED_EXECUTION_ENGINE_SIXTH_ITERATION.zh-CN.md`](archive/HOSTED_EXECUTION_ENGINE_SIXTH_ITERATION.zh-CN.md)。

### 6.10 Iteration 6.1 behavioral stabilization（2026-07-23）

- `commands_run.operation_id` 是 Task/Run 内的调用幂等键，`request_sha256` 只检测同 key 不同 payload；workspace precondition 只属于审批 hash，不进入 retry identity；
- 同 key 精确 retry 即使 workspace 漂移也返回原 Execution；相同命令要真实重跑必须使用新 key；
- `starting` cancel 可在 executor reference 尚不可用时持久存在；late attach 原子保存 job ID，重新读取 cancel state 后执行补偿性 stop；
- monitor registry 保证一个 Execution 同时只有一个 observer，并在 observer 退出时清理；
- status/transport 故障使用默认至少 30 秒的时间 grace、degraded projection 和有界退避；未识别状态不再默认映射为 running；
- `unknown` 表示当前 runtime 越过 grace 或取消 dispatch 无法确认终态，保留 executor reference 并阻止 finish；`interrupted` 表示 runtime restart 后旧 handle 未验证；
- workspace release 用 per-task async lock 让并发调用等待同一次清理，失败后等待者或后续调用可以重试；lease absence 是成功事实；
- `wait_for_terminal` 的 store error 通过 `Result` 传播；agent process-group signal 区分已退出、权限错误和其他系统错误。

相对 Iteration 6 完成点，6.1 的简单 path-based 统计为 production `249 files / 122,194 LOC -> 250 files / 122,628 LOC`（`+434`），test-path `92 files / 57,247 LOC -> 92 files / 57,773 LOC`（`+526`），均在稳定轮次预算内。完整 `cargo test --bin webcodex` 最终为 `1655 passed / 0 failed / 4 ignored`。

## 7. Iteration 7：Checks、hard cut 与删除

状态：Iteration 7/7.1/7.2 已完成人工评审、完整测试和最终 squash，正式基线为
`a1547bba3b93669e8bdf6d0fec2388e0ae2b138e`。
验证记录见
[`HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md`](archive/HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md)。

实际交付：

- 一个 `checks_run` 请求保留调用方 `operation_id`、规范化 request hash、ordered fail-fast plan，并创建一个 `wc_exec_* kind=check`；
- check 与 command 共用 reservation/start/attach/quick-yield/monitor/review/cancel/restart/finish-blocking/workspace-release 代码；
- check projection 分开 `submission_status`、`execution_status`、真实 `exit_code`、`failure_source`、`assertion_status` 和 validation evidence；
- 旧 `checks_run -> invoke_kernel -> cargo_* -> run_project_command_capture` 同步等待与 task-event validation 拼装已删除；
- Hosted MCP、OpenAPI 与 HTTP adapter 都从同一 9-capability registry 取得 schema；
- 删除无生产调用者的 `Principal/AuthMethod` 未来抽象，以及已经由 canonical runtime tool dispatch 替代的 `/api/projects/replace_in_file`、`/api/projects/write_file`；
- production Rust 从 Iteration 7 起点 `122,628` 降至 `122,034`，净减 `594` 行。

### 7.1 Validation Provenance Hardening

- check reservation 与成功终态分别持久化起始和 validated workspace hash，覆盖
  tracked、staged 与 relevant untracked result changes，不保存 workspace 内容；
- 后续 edit、mutating command 或外部修改使 `task_finish` 返回 409
  `checks_stale`；旧成功 Execution 不被篡改，同 key retry 仍返回原 Execution；
- agent JobManager 是 structured validation step transition 的 owner；
  authenticated job update 只携带 bounded progress，项目 stdout/stderr 不再控制状态；
- failed check 显式持久化 `failed_check` 和最大 16 KiB 的 sanitized structured
  evidence；review tail、job log 可用性和 runtime/database restart 不改变投影；
- Iteration 7 数据库通过幂等 additive migration 增加四列；历史成功 check 不猜测
  provenance，要求以新 operation key 重跑。

Iteration 7.1 的 path-based 统计为 production
`250 files / 122,034 LOC -> 250 files / 122,531 LOC`（`+497`），test-path
`92 files / 57,864 LOC -> 92 files / 58,514 LOC`（`+650`）。production 比
`<=450` 目标高 47 行；该口径把 `src/db.rs` 中 47 行 migration contract test
计入 production，其余增长服务可信 progress、durable provenance/evidence 和
stale guard，没有扩 Hosted capability。完整 `cargo test --bin webcodex` 为
`1,637 passed / 0 failed / 4 ignored`。

### 7.2 Atomic Finish and Strict Validation Progress

- `edits_apply`、command/check reservation、cancel 与 `task_finish` 共用 per-task
  coordination domain；finish 从重读 Task/Execution、比较 provenance、capture
  Result 一直持锁到 durable `ready_for_review` transition；
- edit/reservation 先占锁时 finish 看到 stale/active 事实；finish 先占锁时等待者
  重读非 active Task 后拒绝。不同 Task 的锁独立，长 Execution 不在锁内等待；
- structured running progress 必须单调、逐步且名称与 durable plan 完全匹配；
  terminal success 要求完整 progress，assertion failure 要求可信 failed step；
  cancel/timeout/lost 不伪造 failure，也没有 provenance；
- malformed progress 显式收敛为 executor protocol failure，不会 succeeded/passed，
  不写 assertion evidence 或 validated workspace；stdout/stderr 完全不是控制通道；
- DB 删除 succeeded check 的 plan-text completion inference，并再次验证完整 count、
  空 failed step 与匹配的非空 provenance；
- `structured_validation_argv` capability 默认 false，当前 Agent 显式 true；旧 Agent
  的 `checks_run` 返回 `structured_validation_unavailable`，不创建 reservation，
  不静默降级到 marker 或普通 shell。

Iteration 7.2 的 path-based 统计为 production
`250 files / 122,531 LOC -> 250 files / 122,778 LOC`（`+247`），test-path
`92 files / 58,514 LOC -> 92 files / 59,014 LOC`（`+500`），均在本轮目标内。
这些数字和最终验证仍属于人工 merge gate 输入，不表示评审已通过。

LOC 的 10% 长期目标尚未达到：相对 Iteration 6 起点仍高 `1,039` 行，距离 `108,896` 还差 `13,138` 行。当前仍有真实调用者的 Session/current-session、ToolRuntime、ShellClientRegistry、JobManager、local jobs、文件/LSP 和本机 CLI 路径不能为追逐数字而删除；下一轮必须在保留这些产品/安全边界的前提下继续做引用驱动的减法。

## 8. Iteration 8：产品精修与受控 validation

预计：2–3 个专注开发周。

状态：Iteration 8.0 已形成正式基线。Iteration 8.1 只实现 Project-Aware
Validation Recipes。Iteration 8.2 交付 Browser `/console` 的最小
review/cancel/accept 控制台：它复用与 CLI 相同的本机人类授权（不新增第十项
model-facing capability），credential 仅在内存中。完整 Browser IDE、真实公网
acceptance、credential rotation、数据分析平台和 Iteration 8 总体 20% LOC 目标仍
不在这些切片内。

交付：

- 一条命令 onboarding、doctor 和 status；
- project-aware checks recipe，优先 Rust、Node、Python、Go 的最小稳定集合；
- review/cancel/accept 的 CLI 与最小 Browser UI；
- replay 数据管道和 release baseline；
- 文档只保留当前路径，历史迭代文档归档；
- 第二轮删除、命名和错误契约整理。

退出条件：

- 普通 coding golden scenarios 不需要 discovery/ops 工具；
- 从 task start 到 accepted result 的路径可解释且稳定；
- production Rust LOC 相对本文基线净减少 20% 以上；
- 用户可见概念不出现 agent、client id、runtime project id 和 workflow session。

### 8.1 Iteration 8.0 真实产品路径审计

审计基于 `src/startup.rs`、`src/hosted_connect.rs`、`src/bin/webcodex-cli.rs`、
`src/bin/webcodex_cli/{connect,doctor,doctor_support}.rs`、
`src/bin/webcodex-agent.rs`、`src/bin/webcodex_agent/{config,projects}.rs`、
`src/connector_runtime/{mod,surface,http}.rs`、`src/task_cli.rs`、
`src/console_web.rs` 和对应 CLI/Connector tests 的实际 dispatch/caller。

| User goal | Current entry point | Internal concepts exposed | Extra calls required | Failure modes | Canonical replacement | Old path to delete |
|---|---|---|---|---|---|---|
| 把当前 Git 项目接入普通 coding client | README 的 `webcodex-cli server up` + `webcodex-cli connect` + `webcodex-agent --config`；另一条实现路径是 `webcodex connect <target> --via <ingress>` | client id、`agent:<client>:<project>`、transport、server/admin/client token 分工、私有 state/config path | `runtime_status`、`list_projects`，再把 runtime project id 放进 prompt | 两套入口行为冲突；一体化 connect 同时写配置、启动三个长期进程和 ingress；partial state 难修复 | 项目目录中的唯一 `webcodex setup`，后续显式 `webcodex agent start` | `webcodex connect` dispatch/tunnel orchestration、`webcodex-cli connect` |
| 判断为何当前项目不能工作 | `webcodex-cli doctor`、`doctor_support`、`agent status`、`ops status` 和 runtime status 各自拼状态 | config/token path、client id、transport、runtime registry、QUIC 实现细节 | 用户先选正确 doctor，再提供 server URL/token/config/project id | 自由文本 verdict、检查重复、局部成功被误认为整体 ready | application-level structured readiness facts → `webcodex doctor` | 旧 general/QUIC/local-agent doctor projection；底层 transport smoke 保留为 ops |
| 快速判断能否 coding | Browser `/console` 和 `webcodex-cli ops status` 都读取完整 `/api/runtime/status` | agent table、client id、transport、tool/project counts | 还要人工解释哪一个 agent/project/capability 属于当前项目 | registry 在线但当前 bound project 不 ready；旧 Agent 缺 structured validation 时仍需人工判断 | 同一 readiness facts → `webcodex status` 和只读 Connector readiness API/Browser | Browser runtime/agent registry projection 和 CLI status 拼装 |
| 启动普通 Hosted coding task | Project-bound profile 的 `task_start(goal, mode?)` | 默认 envelope 只有 logical project/task/run durable ids；executor ref 在 adapter 内部 | 已正确启动 connector profile时不需要 discovery/ops | 未启动 project-bound profile 时 surface disabled；Agent offline；旧 Agent capability 缺失 | setup 保存的 deterministic connector context，由 server 启动时绑定 | happy path 前置的 `list_projects`、`runtime_status`、`tool_manifest`、session glue |
| 读取、搜索、编辑、验证并提交结果 | `files_read/search` → `edits_apply` → `checks_run` → `task_review` → `task_finish` | task/operation/execution/result durable ids；kernel 的 client/request/runtime id 会递归清除 | 不需要 `start_session`/`current_session`；只有长 execution 需要 review | Agent offline、precondition conflict、structured validation unavailable、active/unknown execution、stale checks | 保持现有 9-capability surface 和 exact retry identity | Connector happy path 中任何新增 discovery/session fallback |
| 本机 review/accept | `webcodex task show/accept/reject` 按同一 Git root/profile 打开 SQLite state | task/result id（用于精确人类决定） | `task_finish` 后一条本机 accept/reject | target checkout 漂移、patch precondition、runtime restart 后需 resume/reject | 保持现有 host-local authority；Iteration 8.2 新增 Browser `/console` 作为共享同一授权的第二入口 | 无；这不是模型 capability，也不能移到 Hosted |

Project selection 已由 `ConnectorContext::from_env` 和
`Database::ensure_connector_binding(project, subject, profile)` 固定：一个 project-bound
runtime 只有一个 logical project、workspace 和 executor reference；subject 仍按认证
principal 隔离。Iteration 8.0 只把 setup 生成的稳定配置作为该 context 的来源，不从
最近使用记录、目录同名或模型文本猜项目；root/config 不一致时 fail closed。

Iteration 8.0.1 进一步固定唯一 credential design：setup 生成一个由 Connector 与
Agent 共用的 Project Credential，两者经 exact verifier 映射到稳定、非秘密的
`project_grant_id`。secret 只保存在 owner-only private file 中，数据库不存明文；
runtime 保存 SHA-256 verifier value，并用 constant-time comparison。project mode
关闭普通 `WEBCODEX_SHARED_KEY_ENABLED` arbitrary-token fallback；loopback 不构成
认证豁免。readiness、file read/search/edit、command/check、monitor/log 与 cancel
全部携带原始 `AuthContext`，在 Agent request/job mutation 前验证相同 grant。

### 8.2 Iteration 8.0 最小设计与删除清单

依赖方向：

```text
CLI / Browser / readiness HTTP adapter
  -> Project readiness application facts
  -> setup state / connector context / Agent registry
```

`doctor` 返回稳定的 `name/status/code/summary/next_action` facts；`status` 和 Browser
只做较小投影，不解析 doctor 文本。setup 只写仓库外的私有 state，不启动进程、
不改 Git、不开放非 loopback 端口、不覆盖有效配置或 token。`agent start` 是用户
单独执行的 consequential step，继续复用现有 server、Agent、registry、policy、
workspace 和 Execution Engine。

本切片预定删除：

| Path/symbol | Previous responsibility | Replacement / no-caller proof |
|---|---|---|
| `hosted_connect::{parse,run}` 与 ingress/process glue | 把配置写入、server/Agent/tunnel 启动混在 `webcodex connect` | `startup` 只 dispatch setup/doctor/status/agent start；docs 和 CLI tests 不再引用 connect |
| `webcodex_cli::connect` | shared-key/open agent config 与 project file 的第二套 onboarding | canonical setup；删除 `CliAction::Connect`、parser、usage 和 tests |
| `webcodex_cli::{doctor,doctor_support}` | general、local Agent、QUIC 的第三套 status projection | shared readiness model；高级 transport smoke 继续由 ops/transport lanes 承担 |
| `webcodex-agent init` parser alias | 与 `webcodex-cli agent init` 重复的 config generator 入口 | canonical setup 调用共享 generator；管理 CLI 仍保留唯一低层 init |
| Browser runtime/Agent table projection | 展示全 registry observability | connector readiness endpoint；不增加第二套 API 或前端框架 |

删除前后都必须由实际 dispatch、`rg` 引用、9-capability registry/OpenAPI/MCP
consistency tests 和当前 docs 引用证明；不得删除 session guard、scope、exact retry、
durable Execution、capability negotiation、migration、restart reconciliation 或
local/remote executor 共享路径。

### 8.3 Iteration 8.0 错误合同

| Stable code | 含义 | retryable | user action | next action |
|---|---|---:|---:|---|
| `project_not_configured` | 当前 Git root/profile 尚未 setup | false | true | `webcodex setup` |
| `project_registration_invalid` | registration/config 存在但 malformed、incomplete 或与当前 root/profile 冲突 | false | true | 解决指出的字段后重跑 setup；不覆盖现有 state |
| `project_credential_invalid` | Project Credential 缺失、不可读、权限不安全、格式无效或 Connector/Agent 不匹配 | false | true | 恢复两份匹配 private file，或明确退役整个 profile 后重新 setup |
| `project_credential_rejected` | server 可达但拒绝本地配置的 Project Credential | false | true | 恢复 server 对应 credential；不得折叠为 `agent_offline` |
| `server_unreachable` | project-bound loopback runtime 不可达 | true | true | `webcodex agent start`，仍失败则 doctor |
| `agent_offline` | server 可达但绑定的本地 Agent 不在线 | true | true | `webcodex agent start` / `webcodex doctor` |
| `required_capability_unavailable` | Agent binary 或九项 coding path 所需 capability 缺失 | false | true | 升级全部 WebCodex binaries |
| `structured_validation_unavailable` | 旧 Agent 未协商 structured validation | false | true | 升级全部 WebCodex binaries |
| `workspace_unavailable` | Git、project path 或 private writable state 不可用 | false | true | 恢复 workspace/权限后 doctor |
| `task_not_active` | Task 已 ready/cancelled/interrupted，不能继续 mutation/execution | false | true | 新建 Task，interrupted 时先本机 resume/reject |
| `execution_not_terminal` | active/unknown Execution 阻止 finish | true | unknown 时 true | `task_review` wait/cancel；unknown 时本机检查 executor |
| `checks_required` | normal Task 尚未运行 structured check | false | true | 新 operation id 调用 `checks_run` 后重试 finish |
| `checks_stale` | 成功 check 的 provenance 与当前 workspace 不一致 | false | true | 新 operation id 重跑 checks |
| `validation_recipe_not_found` | `cwd` 到 Task root 没有 supported marker | false | true | 提供含 marker 的 `cwd` |
| `validation_recipe_ambiguous` | 最近目录存在多个 supported marker | false | true | 提供显式 `recipe` 或更具体 `cwd` |
| `validation_recipe_mismatch` | 显式 recipe、marker 或安全路径边界不匹配 | false | true | 修正 `recipe`/`cwd` |
| `validation_manifest_invalid` | manifest malformed、不可读或越界 | false | true | 修复公开 manifest |
| `validation_check_unavailable` | recipe 没有请求的 semantic check | false | true | 更改 checks 或项目配置 |
| `package_manager_ambiguous` | Node manager 证据缺失或冲突 | false | true | 修正 `packageManager`/lockfile |
| `test_filter_unsupported` | 该 recipe 不支持安全 filter 映射 | false | true | 移除 filter 或使用 Rust recipe |
| `validation_tool_unavailable` | executable 或 Python module 缺失 | 以新 operation 重试 | true | 在 Agent host 提供项目已有工具 |
| `validation_step_spawn_failed` | validation command 未能启动，是 executor/infrastructure failure | 以新 operation 重试 | true | 修复/升级 Agent runtime 后用新 operation id 重跑 |

`validation_step_spawn_failed` 的 terminal Execution 必须保持
`failure_source=executor`、`failed_check=null`、`assertion_evidence=null` 和
`validated_workspace_sha256=null`。只有 command 已实际运行并 non-zero 时，才是
assertion failure，并允许记录对应 failed step evidence。

### 8.4 Iteration 8.0 实现候选与规模

当前 Iteration 8.0.1 dirty worktree 已实现 `webcodex setup`、共享 readiness
application facts、`doctor`/`status`、显式 `agent start`、Connector readiness
API、最小 Browser readiness surface，以及 exact Project Credential boundary。
本地 invalid registration、invalid credential、workspace unavailable、server
unreachable、credential rejected 与 agent offline 保持独立 stable code，doctor
只读且不修写损坏 state。

真实 golden contract 经过 Auth middleware、Connector HTTP adapter、Agent grant
registry、Task/Execution store、Agent request enqueue/complete 和本机 result
accept。adapter dispatch recorder、Agent request recorder 与 durable event ledger
共同证明实际请求没有 `list_projects`、`runtime_status`、`tool_manifest`、
`start_session`、`current_session` 或 `list_agents`。原手工维护 calls vector 已删除。
完整 Browser IDE、active execution Browser projection、多语言 recipe 和真实公网
ChatGPT acceptance 均未纳入这一切片。

相对正式基线的同口径 path-based 统计为：

- production Rust：`250 files / 122,778 LOC -> 249 files / 121,134 LOC`
  （`-1 file / -1,644 LOC`）；
- test-path Rust：`92 files / 59,014 LOC -> 91 files / 59,700 LOC`
  （`-1 file / +686 LOC`）。

最大 production 文件从 `src/bin/webcodex-agent.rs` 的 `6,869 LOC` 降为
`6,675 LOC`；`src/tool_runtime/files.rs` 保持 `4,265 LOC`；
`src/connector_runtime/mod.rs` 从 `3,453 LOC` 增至 `3,619 LOC`，因为 readiness、
grant authorization 与 Execution auth propagation 直接复用既有 Connector
context/Agent registry，而没有另建 registry、status store 或 privileged job
facade。production Rust 已达到修复轮 `net <= -1,500 LOC` 门禁，但
Iteration 8 总体相对长期基线减少 20% 的目标仍未完成。

### 8.5 Iteration 8.1 Project-Aware Validation Recipes

Iteration 8.1 保留唯一公开入口 `checks_run` 和 `format/check/test` 三个 semantic
check；可选 `recipe` 只有 `rust/node/python/go`，字段省略即 auto，不增加
`detect_language`、`list_recipes` 或第十项 Hosted capability。

- resolver 在 Task execution workspace 内、per-task mutation/finish lock 中从相对
  `cwd` 向 root 查找最近的 `Cargo.toml`、`package.json`、`pyproject.toml` 或
  `go.mod`；不递归扫描、不读取 sibling、拒绝绝对路径、`..` 和 symlink escape；
- 同一最近目录有多个 marker 时 auto 返回 `validation_recipe_ambiguous`；显式
  recipe 只能解除该目录内真实 marker 的歧义，否则返回
  `validation_recipe_mismatch`；
- Rust 保留 Cargo 三项行为和 safe one-argv test filter；Node 从有效
  `packageManager` 或单一 lockfile 解析 manager，只选择固定 script 名顺序；
  Python 只启用 `pyproject.toml` 配置证明的 Ruff/Black、Ruff/Mypy、pytest；Go
  只支持 `go vet ./...` 与 `go test ./...`，format 明确 unavailable；
- recipe 从不安装 dependency、不运行 install hook、不生成配置、不修改 lockfile、
  不联网；模型不能提供 program、argv、script body 或 shell command；
- validation protocol hard cut 为 canonical `program + argv`，
  `structured_validation_argv` 缺失时 reservation 前 fail closed，不回退普通 shell；
- durable Execution 保存 recipe ID/version、相对 root、semantic checks、tool
  identity、invocation digest 和 manifest/lock digest；request hash 因而拒绝旧
  operation ID 在新 recipe binary 上错误复用，workspace provenance 继续独立绑定；
- planner、executor/tool、assertion 和 protocol failure 保持分层；spawn/module
  unavailable 不写 failed check、assertion evidence 或 success provenance；
- 四语言 fixture 复用 Auth middleware、Connector HTTP adapter、project
  credential、Agent grant registry、durable store、structured progress 与 local
  acceptance；recorder 继续证明普通路径没有 discovery/session call。

`task_start` 的 recipe hint 投影在本轮 deferred；`checks_run` 自身的 stable code
和安全 details 已足够 actionable，避免为提示复制 resolver 或扩大范围。本轮仍需
focused/full suite、LOC 门禁与人工 review，不自行宣布 merge gate 通过。

### 8.6 Iteration 8.2 Host Console 架构收敛

相对 `554e735c3dfdba8d2defec8dce8f1f2701b7f6ec`，同口径 path-based
统计为 production Rust `250 files / 121,827 LOC -> 251 files / 122,426 LOC`
（`+1 file / +599 LOC`），test-path Rust
`92 files / 60,501 LOC -> 93 files / 61,192 LOC`
（`+1 file / +691 LOC`）。最大 production 文件前后均为
`src/bin/webcodex-agent.rs`（`6,721 LOC`）；受本轮影响的最大 production 文件
`src/connector_runtime/mod.rs` 从 `3,601` 增至 `3,710 LOC`。最大 test-path
文件前后均为 `src/tool_runtime/tests/handoff.rs`（`3,505 LOC`），
`src/project_entry_tests.rs` 从 `1,415` 增至 `1,716 LOC`。

本轮删除独立 `HostError`、`HostTaskRow`、host decision lock/accept/reject 流程和
独立 review durable projection，改为复用 `ConnectorTaskStoreError`、
`ConnectorCallOutcome`、Database credential-safe queue row、
`WorkspaceManager::decide_connector_result_local` 以及 Connector `task_review`
projection。Browser adapter 只保留六个精确 route 的 prepare/parse/call/render。
这些数字和验证证据仅作为人工评审输入，不表示 merge gate 已自行通过。

## 9. 代码收敛原则

### 9.1 依赖方向

目标依赖方向：

```text
MCP/OpenAPI/Connector adapter
  -> Task/Execution application service
  -> Execution adapter
  -> local/agent executor
```

- adapter 不直接写 Task 状态；
- executor 不拥有产品状态机；
- protocol flattening 不进入 domain model；
- SQLite store 不返回 protocol-specific JSON；
- Local 与 remote executor 不分叉两套 Task 生命周期。

### 9.2 大文件治理

拆文件本身不算简化，只有同时减少责任和删除重复路径才算。

约束：

- 新 production 文件目标不超过 600 行，硬上限约 800 行；
- 新函数目标不超过 80 行，超过约 120 行必须说明不可再拆的原因；
- `connector_runtime/mod.rs` 应逐轮缩小，主要保留组装与 dispatch；
- `db/task_kernel.rs` 不继续增长，query/store 按实体或 use case 分离；
- 不为减少单文件行数制造无意义 traits、re-export 或一层转发 façade。

### 9.3 LOC 预算

Iteration 6：

- production Rust 首选净减少；
- production Rust 净新增硬上限：1,200 行；
- test-path Rust 净新增硬上限：800 行；
- 若超过，缩小范围或先删除旧路径，不能以“以后再删兼容代码”解释。

Iteration 7：

- 必须净减少 production Rust；
- 目标相对 Iteration 6 起点至少减少 10%；
- 删除旧 Hosted surface、session glue 和重复 job projection 是正式交付物。

Iteration 8 完成：

- 目标相对本文 120,995 行 production Rust 基线减少 20% 以上；
- 若功能正确但代码量仍增长，不能宣称已达到“小而美”。

LOC 不是质量本身，但在功能边界已经明确后，持续净增长通常意味着旧抽象没有被真正替换。

## 10. 测试策略：风险覆盖，不做测试通胀

### 10.1 原则

- 测试外部契约、状态转换和恢复不变量，不镜像每个内部函数；
- 一条 table-driven/state-machine test 优于十条只改变一个字段的测试；
- 删除行为时同步删除只服务旧行为的测试；
- 机械重构不新增重复 snapshot；
- focused lanes 是开发反馈环，full suite 是阶段门禁；
- 不通过降低安全断言、增加 ignore 或吞掉真实失败来减少测试；
- 新测试必须在注释、名字或评审报告中对应一个真实失败类别、contract 或回归风险。

### 10.2 Execution Engine 核心场景

最多围绕以下 12 个场景形成少量 contract/E2E tests：

1. 短 command/check 在一次调用内返回终态；
2. 超过 yield 的静默 command/check 快速返回 `running`；
3. output cursor 单调且不重复；
4. `task_review(wait_ms)` 在输出、终态或 heartbeat 时返回；
5. queued execution 显示 queue age/reason 并可取消；
6. 取消终止 process group 并释放 slot；
7. runtime restart 将未完成 execution 标为 interrupted/unknown；
8. `task_finish` 在 active/unknown execution 存在时拒绝；
9. non-zero check assertion 与 transport/submission success 分离；
10. 相同 `operation_id` 精确 retry 不重复启动进程，新 `operation_id` 可重跑相同 command/check plan；
11. MCP 与 OpenAPI 对相同 capability 产生相同业务 envelope；
12. Connector direct synchronous command path 不再可达。

不要求“一场景一文件”。优先 table-driven transition tests 加 2–4 条真实 executor integration tests。

### 10.3 测试删除

每轮必须检查并删除：

- 只验证 legacy alias 的测试；
- 同一 schema 在多个 adapter 中重复复制的测试；
- 已由 registry consistency test 覆盖的逐字段重复测试；
- 只验证内部函数实现细节、但不保护外部 contract 的测试；
- 与已删除路径一起失效的 fixtures 和 helpers。

## 11. 数据与指标

Chat 导出、SessionAtlas 和 WebCodex durable events 应进入同一离线分析口径。

长期指标：

```text
time_to_first_progress
maximum_silent_interval
queue_wait_p50_p95
polling_amplification
calls_per_ready_for_review
calls_per_accepted_result
terminal_unknown_rate
identical_retry_rate
shell_fallback_rate
resume_success_rate
result_acceptance_rate
```

注意：ChatGPT 导出中的 wall latency 包含模型思考、connector 调度和网络，不能直接等同于 WebCodex runtime duration。产品遥测必须区分：

```text
model/client latency
connector/transport latency
queue latency
executor duration
silent interval
```

成功率必须以 terminal execution、Task Result 和 human decision 为口径，不能把 `queued`、HTTP 200 或工具受理统计成任务成功。

## 12. 还需要多久

这是工程估算，不是发布日期承诺。

| 目标 | 预计投入 |
|---|---|
| Iteration 6：Execution Engine vertical slice | 2–3 个专注开发周 |
| Iteration 7/7.1/7.2：checks/provenance/atomic finish | 已完成 |
| Iteration 8：产品精修/validation/onboarding | 8.1 待人工 review |
| 达到第一阶段“精致、小而美” | 约 7–11 个专注开发周 |
| 单人非全职推进 | 更现实为 3–5 个月 |
| 后续成熟 SSH/跨设备 Operations Profile | 另需约 4–8 个专注开发周 |

最大不确定性：

- 旧 ToolRuntime/job/agent transport 能否抽取复用而不是复制；
- ChatGPT MCP/OpenAPI 对 long-poll、取消和重连的真实行为；
- 删除旧 session/tool surface 时暴露出的隐式调用者；
- 当前测试集的运行成本和重复覆盖程度。

如果每轮继续增加能力但不删除旧路径，时间会无限延长；如果严格执行 hard cut、LOC 预算和非目标约束，三轮足以达到第一阶段精致产品。

## 13. 合并 main 的门禁

在以下条件满足前，不把整个分支作为默认产品路径合并并宣称完成：

1. 长命令在 yield 窗口内返回 terminal 或 durable running；
2. review/cancel 不被执行锁阻塞；
3. submission success 与 terminal success 分离；
4. process-group cancellation 有端到端证据；
5. restart reconciliation 有 contract test；
6. `task_finish` 对 active/unknown execution fail closed；
7. Hosted 默认工具面不含 legacy ops/session/admin 工具；
8. 旧 Hosted path 和无调用者 compatibility code 已删除；
9. source LOC 和 test LOC 有 before/after 报告；
10. 真实 ChatGPT MCP 与至少一个 OpenAPI client 完成 golden acceptance；
11. CI 对 branch head 有独立成功记录；
12. 文档只描述一条当前用户路径。
13. 成功 check 有 durable workspace provenance，stale workspace 不能形成 passed Result；
14. 项目 stdout/stderr 不能推进 durable check progress，失败 evidence 在重启后稳定。
15. Result capture 与 mutation/reservation 由同一 per-task 原子边界串行，不同 Task 不被全局串行；
16. structured terminal verdict 必须有完整可信 progress，旧 Agent 不得得到静默降级。

## 14. 后续可选能力

只有 Iteration 8 门禁通过后再评估。

### 14.1 Operations Profile

- `ExecutionTarget`：local workspace、SSH host；
- artifact copy + hash verification；
- process/port inspection；
- connection keepalive 和 forwarding；
- 所有动作仍复用 Execution Engine。

### 14.2 Session Facts

- verified observations、evidence event、revision/expiry；
- 只在 replay 证明重复事实确认是主要调用成本后实现；
- 不把模型陈述直接标记为 verified。

### 14.3 Workflow / Checkpoint

- 只为重复、可验证、跨设备的工程 runbook；
- 不把通用聊天计划全部持久化成复杂 workflow DSL。

## 15. 每轮开发纪律

每轮开始必须记录：

```text
branch/head
production source LOC
test LOC
largest files
default Hosted tools
active state stores
representative golden task calls
```

每轮结束必须回答：

1. 删除了什么旧路径？
2. 用户可见概念减少了什么？
3. 模型平均少做了哪些调用？
4. 新增的每个 abstraction 替换了什么？
5. 新增测试覆盖了哪种真实失败？
6. 当前还有哪两个最大模块可以删除或合并？
7. production/test LOC 的 before/after 是什么？

没有删除清单、before/after 指标和真实 replay 证据的“重构完成”，不视为完成。
