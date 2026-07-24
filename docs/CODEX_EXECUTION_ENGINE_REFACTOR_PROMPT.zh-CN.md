# Codex Prompt：Project-first Execution Engine 大重构

> Archive notice：这是 Iteration 6 的历史执行 prompt，不是当前开发入口。
> 当前入口和状态以 `PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md` 与
> `ROADMAP.zh-CN.md` 为准。

下面的 prompt 用于在 `refactor/project-first-experience` 上立即启动下一轮大重构。它要求完成一个可运行的 vertical slice，而不是只输出分析或计划。

```text
你正在维护 WebCodex 仓库 yyjeqhc/webcodex。

期望分支：refactor/project-first-experience
期望基线：先自行检查 git status、git log、HEAD 和 AGENTS.md；不要假定工作树干净，也不要覆盖他人修改。

请先完整阅读：
- AGENTS.md
- docs/PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md
- docs/PRODUCT_DEVELOPMENT_PLAN.zh-CN.md 的 Task/Run/Result、canonical surface 和下一步部分
- docs/archive/HOSTED_TASK_KERNEL_SECOND_ITERATION.zh-CN.md
- docs/archive/HOSTED_TASK_RESULT_THIRD_ITERATION.zh-CN.md
- docs/archive/HOSTED_EXECUTION_EFFICIENCY_FOURTH_ITERATION.zh-CN.md
- docs/archive/HOSTED_EDIT_RELIABILITY_FIFTH_ITERATION.zh-CN.md

## Goal

直接完成 Project-first Iteration 6：实现一个最小但完整的 durable Execution Engine vertical slice，并把 Hosted `commands_run` 从同步等待旧 `run_shell` 的路径迁移过去。

本轮不是再写一份设计稿。你必须修改 production code、schema/store、connector projection、必要测试和文档，运行验证，并删除被替换的旧 Connector 同步路径。

最终用户体验必须是：

- 短命令在约 8 秒 quick-yield 窗口内完成时，一次调用直接返回终态；
- 长命令或静默命令在窗口到期时返回 durable `execution_id` 和 `running/queued` 状态，而不是等待 120 秒或报告模糊 timeout；
- `task_review` 可以立即查看或有界等待 execution 的状态、最近输出和 cursor；
- 用户/模型可通过单意图 `task_cancel` 请求取消；
- 取消必须到达 executor、终止进程组、观察终态并释放 slot；
- runtime 重启后，未完成 execution 进入 `interrupted` 或 `unknown`，Task 进入 `needs_attention`；
- 存在 active、cancel-pending 或 unknown execution 时，`task_finish` 必须 fail closed；
- submission accepted、execution terminal、exit code 和 capability outcome 不再混成一个 success 字段。

## 当前事实与约束

当前 Hosted surface 是 8 项：

- task_start
- files_read
- files_search
- edits_apply
- checks_run
- commands_run
- task_review
- task_finish

本轮允许增加且最多只增加一个 Hosted capability：

- task_cancel

不要把 `job_status`、`job_tail`、`job_log`、`stop_job`、session/admin/agent 工具重新暴露给 Hosted 模型。

当前简单基线约为：

- production Rust LOC：120,995
- test-path Rust LOC：56,454
- 本分支相对 main 的 production Rust 净增长：+11,439
- `connector_runtime/mod.rs`：约 3,289 行
- `db/task_kernel.rs`：约 1,944 行

现有可复用执行资产包括：

- src/tool_runtime/jobs.rs
- src/tool_runtime/local_jobs.rs
- src/tool_runtime/job_tools.rs
- src/shell_client/jobs.rs
- src/shell_client/job_updates.rs
- src/bin/webcodex-agent.rs 中的 JobManager
- 现有 process-group termination、output files/buffers、timeout、approval、workspace policy 和 agent dispatch

必须复用或抽取这些能力。禁止再实现第二套 scheduler、第二套 process manager、第二套 output store 或 Connector 私有 job system。

`edits_apply.operation_id` 已经是 caller-generated idempotency key。不要改变它的含义，也不要把 durable execution entity 叫相同的 operation_id。新实体使用：

- execution_id：wc_exec_*

## 最小事实模型

本轮只实现：

Project -> Workspace -> Task -> Run -> Execution -> Result

Execution 只覆盖 command execution。为后续 checks 预留 kind，但本轮不要同时迁移全部 checks。

不要在本轮新增以下完整模型：

- Step
- ExecutionAttempt
- Process entity graph
- Fact store
- Workflow DSL
- ExecutionTarget/SSH
- assertion DSL
- Browser UI
- multi-language validation matrix

这些方向全部延后。不要以“为未来扩展”增加抽象层。

## Execution contract

Execution 至少持久化：

- execution_id
- kind
- task_id
- run_id
- state
- submitted_at
- queued_at
- started_at
- last_output_at
- finished_at
- stdout_cursor
- stderr_cursor
- exit_code
- failure_source
- failure_code
- cancel_requested_at
- terminal_reason
- request_fingerprint
- executor reference / legacy job reference（仅内部）

状态最少为：

- accepted
- queued
- starting
- running
- cancel_requested
- succeeded
- failed
- cancelled
- interrupted
- unknown

明确 transition，并让所有状态修改经过一个 application/service boundary；HTTP/MCP/OpenAPI handler、executor callback 和 CLI 不得各自随意更新状态。

统一结果中至少分开：

- submission_status
- execution_status
- exit_code
- terminal_reason
- assertion_status（本轮可固定为 not_run）
- capability_outcome

不要把 HTTP 200、enqueue 成功或 command exit 0 自动等同于 Task 成功。

## Quick-yield

`commands_run` 的正常路径：

1. 校验 Task/Run/approval/workspace precondition；
2. 在 SQLite 中创建 accepted execution，再 dispatch；
3. 进入 queued/starting/running；
4. 最多等待约 8 秒；
5. 若 terminal，直接返回 terminal projection；
6. 若仍 active，返回 execution_id、状态、queue/silent metadata、output cursors 和 next action；
7. Hosted client 的请求结束不取消 execution。

同一精确 request fingerprint 的网络重试必须幂等；不得重复启动进程。不要做基于模糊相似度的自动去重。

## Review/watch

扩展 `task_review` input，保持单意图 review：

- after_cursor?: integer
- wait_ms?: bounded integer，建议最大 15 秒或 30 秒
- max_events?: bounded integer
- include_output_tail?: boolean

返回至少包括：

- active_execution
- queue_age_ms
- queue_reason
- blocker_execution_id（可知时）
- silent_for_ms
- last_progress_at
- stdout_cursor
- stderr_cursor
- bounded output_tail
- recent task/execution events
- blocking
- next_action

`wait_ms` 是有界 long-poll。在新输出、状态变化、heartbeat deadline 或 terminal 时返回。无 wait_ms 时立即返回。

不要要求模型连续手工调用低层 status/tail；不要实现无限 SSE 作为本轮前置条件。

## Cancel

新增 `task_cancel(task_id, reason?)`，作为第 9 个 Hosted capability。

取消链路必须覆盖：

- durable cancel_requested
- dispatch cancel to local/agent executor
- executor acknowledgement or transport-unknown result
- process-group termination
- terminal observation
- durable terminal event
- queue/workspace slot release
- idempotent repeated cancel

删除 server waiter 或内存 map 记录不算取消成功。

若 execution 仍 queued，取消不得先启动它。

## Restart reconciliation

Connector runtime 启动时，扫描本项目未完成 execution：

- 能从现有 executor/job authority 确认终态时，持久化真实终态；
- 无法验证 handle 时，标记 interrupted 或 unknown；
- 不伪造 failed/succeeded；
- Task 转为 needs_attention；
- task_review 返回恢复/检查建议。

本轮不要求跨进程重新附着任意 OS child。

## Locking and concurrency

当前 Task 级锁不得包住整个外部命令 await。

采用短事务/短锁：

1. 校验并 reserve execution；
2. 释放 Task lock；
3. dispatch/wait execution；
4. 以 revision/CAS 或合法 transition 更新状态；
5. task_review/task_cancel 始终可进入；
6. mutable workspace lease 与 Task metadata lock 分开。

必须增加一个真实并发测试证明：长命令 active 时，task_review 和 task_cancel 不被阻塞。

## Required implementation sequence

1. 记录 before baseline：branch/head、production/test LOC、最大文件、当前 Hosted tool list。
2. 画出当前 commands_run -> invoke_kernel(run_shell) -> executor 的实际调用链和现有 job assets；在最终报告中说明复用了什么。
3. 在当前 schema 策略下增加 execution store/model。不要建立长期兼容 migration 或 dual write。
4. 建立小型 ExecutionService/application boundary。
5. 抽取/复用现有 job execution adapter；不要复制 process code。
6. 迁移 commands_run 到 quick-yield execution。
7. 扩展 task_review projection 和 bounded wait。
8. 增加 task_cancel，更新 canonical registry、MCP、OpenAPI、OAuth scope policy 和一致性测试。
9. 实现 restart reconciliation 和 task_finish fail-closed。
10. 删除 Connector direct synchronous command glue、无调用者 helper、重复 projection 和只服务旧路径的测试。
11. 更新 docs/PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md 中的实际完成状态，并新增一份简短 iteration record；不要再写另一份千行总计划。
12. 运行 focused tests、fmt/check，最后再运行 full suite。

## Code budget

本轮目标是替换，不是叠加。

- production Rust 首选净减少；
- production Rust 净新增硬上限：1,200 行；
- test-path Rust 净新增硬上限：800 行；
- 新 production 文件目标 <= 600 行，硬上限约 800 行；
- 新函数目标 <= 80 行，超过 120 行必须拆分或在报告中解释；
- connector_runtime/mod.rs 和 db/task_kernel.rs 不得继续无界增长；
- 若超过预算，缩小范围或先删除旧路径，不允许保留“以后再删”的兼容实现。

不要为了压 LOC 写晦涩代码、宏滥用或无意义 façade。LOC 是防止双实现的约束，不是唯一质量指标。

## Test budget and policy

不要无脑扩充测试。

测试只保护外部 contract、transition、并发、取消和恢复不变量。优先 table-driven/state-machine tests，并删除被替换路径的旧测试。

核心场景最多围绕以下 12 项组织，不要求一项一个测试文件：

1. 1 秒 command 一次调用 terminal；
2. silent command 超过 yield 后返回 running；
3. output cursor 单调、不重复；
4. task_review(wait_ms) 被输出/终态/heartbeat 唤醒；
5. queued execution 返回 queue age/reason；
6. queued/running execution 均可取消；
7. cancel 终止 process group 并释放 slot；
8. restart 后 active execution -> interrupted/unknown；
9. active/unknown execution 阻止 task_finish；
10. non-zero exit 与 submission success 分离；
11. precise retry 不重复 spawn；
12. MCP/OpenAPI business semantics 一致，旧同步 Connector path 不可达。

删除：

- legacy alias tests；
- 同一 schema 在多个 adapter 中逐字段复制的测试；
- 已由 registry consistency 覆盖的重复 tests；
- 只验证内部 helper 实现、不保护 contract 的 tests；
- 随删除路径失效的 fixtures/helpers。

不得通过 weaken assertion、吞异常、增加 ignore 或删除安全覆盖来达成 green。

## Explicit non-goals

本轮不要做：

- project-aware multi-language checks；
- SSH/remote host/Android/Kubernetes；
- Session Facts；
- workflow/checkpoint DSL；
- dashboard 或完整 review UI；
- LSP 扩展；
- 新 edit tools；
- tool discovery façade；
- compatibility aliases/dual shapes；
- internal LLM；
- release、deploy、push、tag。

## Validation

开发中运行 touched-domain focused tests。完成前至少运行：

- cargo fmt --check
- cargo check --all-targets
- 与 connector/task kernel/job/MCP/OpenAPI/auth 相关的 focused tests
- cargo test --bin webcodex
- git diff --check
- git status --short

full suite 只在本轮接近完成后运行，不要每个小修改都重复执行。

## Completion criteria

只有同时满足以下条件才可报告完成：

- commands_run 不再同步等待旧 run_shell 的最终结果；
- long/silent command 在 yield 内返回 durable running；
- task_review/task_cancel 在 command active 时可用；
- cancellation 有 executor/process-group terminal evidence；
- restart reconciliation 可解释 unknown/interrupted；
- task_finish 对 active/unknown fail closed；
- submission/execution/exit/capability outcome 分离；
- Hosted surface 最多 9 项且没有 legacy job/session/admin tools；
- 删除清单和 LOC before/after 已给出；
- focused/full validation 通过，或准确报告阻塞失败；
- 没有 push、tag、deploy；除非用户另行明确要求，不创建 commit。

## Final report format

最终必须按以下结构报告：

1. Architecture implemented
2. User-visible behavior
3. Files added/changed/deleted
4. Old paths deleted
5. Existing job/process assets reused
6. State machine and cancellation semantics
7. Test cases added, consolidated, and deleted
8. Validation commands/results
9. Before/after production LOC and test LOC
10. Largest files before/after
11. Remaining risks and exact Iteration 7 handoff
12. Commit status（默认写：no commit created）

不要只说“重构完成”。用具体调用链、状态转换、测试和删除证据证明。
```
