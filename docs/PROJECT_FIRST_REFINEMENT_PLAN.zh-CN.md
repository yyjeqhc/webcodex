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

Iteration 6 已把 `commands_run` 从旧同步 `run_shell` 路径迁出，Iteration 6.1 稳定了调用方幂等、starting cancellation、单 monitor、transport grace 和 workspace release retry。Iteration 7 又把 `checks_run` 迁入同一个 `wc_executions`/`ExecutionService` 生命周期，并删除同步 checks adapter、重复 validation projection、无生产调用者的旧 Principal 抽象和两条专用写文件 REST 兼容路由。Iteration 7.1 把成功 check 绑定到 durable workspace provenance，删除可被项目输出伪造的 stdout progress marker，并持久化 bounded assertion evidence。Iteration 7.2 把 mutation/finish 放入同一 per-task 原子边界，并要求 terminal validation verdict 有完整 structured progress。Hosted surface 仍严格为 9 项；Iteration 7.2 等待人工 merge review，Iteration 8 尚未开始。

因此接下来的主线不是增加更多 validation recipe、远程领域工具或 UI 页面，而是：

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

1. **一个公开入口**：普通个人用户只需要 `webcodex` 和 `webcodex connect ...`。
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
[`HOSTED_EXECUTION_ENGINE_SIXTH_ITERATION.zh-CN.md`](HOSTED_EXECUTION_ENGINE_SIXTH_ITERATION.zh-CN.md)。

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

状态：Iteration 7/7.1 已形成实现基线；Iteration 7.2 已在 dirty worktree 实现，
等待人工 final squash/amend 与 merge review，尚不宣布 Iteration 7 完成。
验证记录见
[`HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md`](HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md)。

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
- `structured_validation_jobs` capability 默认 false，当前 Agent 显式 true；旧 Agent
  的 `checks_run` 返回 `structured_validation_unavailable`，不创建 reservation，
  不静默降级到 marker 或普通 shell。

Iteration 7.2 的 path-based 统计为 production
`250 files / 122,531 LOC -> 250 files / 122,778 LOC`（`+247`），test-path
`92 files / 58,514 LOC -> 92 files / 59,014 LOC`（`+500`），均在本轮目标内。
这些数字和最终验证仍属于人工 merge gate 输入，不表示评审已通过。

LOC 的 10% 长期目标尚未达到：相对 Iteration 6 起点仍高 `1,039` 行，距离 `108,896` 还差 `13,138` 行。当前仍有真实调用者的 Session/current-session、ToolRuntime、ShellClientRegistry、JobManager、local jobs、文件/LSP 和本机 CLI 路径不能为追逐数字而删除；下一轮必须在保留这些产品/安全边界的前提下继续做引用驱动的减法。

## 8. Iteration 8：产品精修与受控 validation

预计：2–3 个专注开发周。

状态：尚未开始。只有 Iteration 7.2 focused/full suite、fresh/upgrade migration、
LOC/删除清单全部登记并经人工评审确认 Iteration 7 merge gate 后，才允许进入。

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
| Iteration 7/7.1/7.2：checks/provenance/atomic finish | 7.2 待人工 merge review |
| Iteration 8：产品精修/validation/onboarding | 2–3 个专注开发周 |
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
