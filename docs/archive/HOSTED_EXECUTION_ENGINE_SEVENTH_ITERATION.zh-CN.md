# Hosted Execution Engine：第七轮 Checks 统一与 Legacy Hard Cut

> Archive：历史实现记录，不是当前使用指南。当前入口见 `../QUICK_START.zh-CN.md`。
>
> 状态：Iteration 7.2 Atomic Finish and Strict Validation Progress 已在
> 2026-07-23 的 dirty worktree 实现，等待人工 merge review；尚未判定
> Iteration 7 完成。未创建新 commit，也未 push、tag、release 或 deploy，
> Iteration 8 尚未开始。

## 1. 结果

Hosted 模型面仍严格只有 9 项：

```text
task_start
files_read
files_search
edits_apply
checks_run
commands_run
task_review
task_cancel
task_finish
```

`checks_run` 不再同步调用 `cargo_fmt/cargo_check/cargo_test` 并等待完整 ToolRuntime 结果。它与 `commands_run` 共用一个 `ExecutionService` 和同一张 `wc_executions` 表；SQLite 保存产品事实，既有 job/agent 层继续拥有队列、进程组、状态和输出。

## 2. 统一调用链

```text
Hosted MCP / Hosted OpenAPI / connector HTTP
  -> canonical checks_run adapter
  -> active writable Task + subject/project guard
  -> operation_id / request_sha256 lookup
  -> wc_executions reservation(kind=check)
  -> shared ExecutionService::execute
  -> ToolRuntime::run_job
  -> ShellClientRegistry::start_job
  -> agent JobManager
  -> 一个真实 job，顺序执行 validation plan
```

观察与控制也只有一条链：

```text
task_review
  -> shared Execution observation
  -> ShellClientRegistry::job_log_for_auth
  -> monotonic stdout/stderr cursor + bounded tail

task_cancel
  -> durable cancel_requested
  -> shared stop_job_model_facing
  -> JobManager process-group termination
  -> terminal observation / workspace release
```

没有新增 scheduler、process manager、job queue、output store、cancellation subsystem 或 monitor framework。

## 3. Check Execution 语义

每个 `checks_run` 请求只创建一个 `wc_exec_* kind=check`。`format/check/test` 是这个 Execution 内的有序计划：

- 顺序等于请求顺序；
- 重复 check 拒绝；
- 第一项 non-zero 后 fail-fast；
- 后续项投影为 `not_run`；
- 整体 timeout 为 1..=120 秒；
- 一个 cancel 终止当前 job 的真实进程组；
- 成功终态将 `check_completed` 收敛到计划长度。

调用方必须提供 `operation_id`：

- 同一 Task/Run、同 key、同 checks/cwd/filter/timeout：返回原 Execution，不 spawn；
- 同 key、不同 payload：`operation_id_conflict`；
- 新 key、同 payload：创建新 Execution 并基于当前 workspace 重新执行；
- workspace 内容不进入网络 retry identity，因此响应丢失后的精确 retry 不受 workspace 漂移影响；
- request hash 是冲突检测事实，不是命令文本的永久执行身份。

`operation_id`/`request_sha256` 与 validation provenance 是两个不同事实：

- 前者回答“这是否是同一个网络请求的精确重试”；
- 后者回答“成功 check 实际验证了哪一个 workspace 状态”；
- workspace fingerprint 不进入 `request_sha256`，否则 workspace 漂移会让同一个
  `operation_id` 产生第二条 Execution，破坏精确重试与 conflict 语义；
- 新 check reservation 单独持久化 `check_workspace_sha256`，只有 check 全部成功且
  终态 workspace fingerprint 与起始值一致时，才写入
  `validated_workspace_sha256`。

示例：

```json
{
  "task_id": "wc_task_...",
  "operation_id": "validate-after-parser-fix-1",
  "checks": ["format", "check", "test"],
  "cwd": ".",
  "test_filter": "parser",
  "timeout_secs": 120
}
```

无效 validation 参数在 durable reservation 前拒绝，避免留下无法启动的 `accepted` Execution。

## 4. Quick-yield 与结果分层

shared Execution wait window 默认约 8 秒：

- 短 plan 在原调用中返回 `succeeded/failed`；
- 长 plan 返回 `queued/running`、durable `execution_id`、queue/silent metadata 与 cursor；
- 外部 await 不持有 Task metadata lock，review/cancel 可并发进入。

投影保持以下事实互相独立：

```text
submission_status
execution_status
exit_code
terminal_reason
failure_source
failure_code
assertion_status
capability_outcome
```

check 已受理时 `submission_status=accepted`。全部通过时 `execution_status=succeeded`、`assertion_status=passed`；真实断言 non-zero 时 `execution_status=failed`、`failure_source=check`、`failure_code=assertion_failed`、`assertion_status=failed`，并保留真实 exit code。active 状态统一投影 `assertion_status=in_progress`；submission rejection、cancelled、interrupted 和 unknown 不伪造成 assertion failure。

失败证据复用既有 Rust validation adapter/parser。SQLite 不复制 stdout/stderr，
但会持久化最大 16 KiB 的结构化 `assertion_evidence_json`：显式
`failed_check`、真实 exit code、stable failure kind、parser/version 和 bounded
sanitized diagnostics。即使没有 output tail、job log 已不可用或 runtime/database
重开，投影仍来自同一 durable evidence。

## 5. Review、取消、重启与 finish

- `task_review` 返回最新 command 或 check Execution；
- cursor 只单调前进，output tail 仍有界，但 stdout/stderr 不参与状态控制；
- long-poll 由新 task event、状态、cursor、终态或 heartbeat 唤醒；
- running check 走同一 stop-job 路径；queued check 可在未 dispatch 前取消；
- JobManager 的 process-group SIGTERM/SIGKILL 语义未复制；
- transient status/transport failure 在 grace 内保持 degraded active，恢复后继续观察；
- grace 到期进入 `unknown`，保留内部 executor reference 但不向 Hosted client 泄露；
- runtime restart 把未完成 check 标为 `interrupted`；
- active、cancel_requested、unknown 阻止 `task_finish`；
- succeeded/failed/cancelled/interrupted 不成为永久 finish blocker；
- `task_finish` 的 validation result 从最新 `kind=check` Execution 投影，不再扫描旧
  `checks_run` task event；最近成功 check 缺少 provenance 或当前 workspace
  fingerprint 不匹配时返回 409 `checks_stale`，并要求以新 `operation_id`
  重跑 `checks_run`。旧 Execution 保持 succeeded/passed，不被改写成 failed/not_run。

Task/Execution 的 project、subject ownership guard 保持不变；read-only Task 在 executor dispatch 前拒绝 checks。`checks_run`/`task_cancel` 仍要求 `job:run`，`task_finish` 仍要求 `project:write`。

## 6. Iteration 7.1：可信 progress、provenance 与 failure evidence

旧实现把固定 `__WEBCODEX_CHECK_STEP__:*` stdout 行当作控制通道。项目测试、
build script 或 proc macro 与 agent job 使用同一 stdout/stderr，因此项目代码可以
伪造 `passed:test`，把 failed Execution 投影成 passed check。该 marker 生成器、
monitor parser 和 output filtering glue 已全部删除；marker 字符串现在只存在于
对抗测试输入。

新的 owner/data flow 是：

```text
Connector builds bounded validation steps (最多 3 项)
  -> ShellClientRegistry records the trusted step names
  -> agent JobManager sequentially owns spawn/fail-fast/transition
  -> authenticated agent job_update carries bounded validation_progress
  -> ShellClientRegistry rejects out-of-range/out-of-order/name mismatch
  -> monitor persists check_completed + failed_check + evidence
```

普通 `commands_run` 仍走单 command job，行为不变；没有第二套 scheduler、process
manager、queue 或 output store。一个 `checks_run` 仍对应一个 durable Execution
和一个可取消的真实 agent job，JobManager 只在当前 step 运行一个真实 process
group，取消仍终止该 group。

成功 provenance 使用既有 `WorkspaceManager::action_precondition`，其 hash 覆盖
HEAD、index tree，以及通过临时 index 捕获的 tracked/staged/relevant untracked
内容；SQLite 只保存 hash，不保存文件内容。check 起止 fingerprint 不一致时不写
成功 provenance。check 后 `edits_apply`、已批准的 mutating `commands_run` 或外部
workspace 修改都会使 `task_finish` 返回 `checks_stale`。同 operation key 精确
retry 仍返回原 Execution 且不 spawn；只有新 key 才重新验证当前 workspace。

Iteration 7 数据库通过幂等 `ALTER TABLE ADD COLUMN` 原地增加：

```text
check_workspace_sha256
validated_workspace_sha256
failed_check
assertion_evidence_json
```

旧 command Execution 不受影响。旧 succeeded check 的 provenance 保持 unavailable，
不会根据当前 workspace 猜测回填；`task_finish` 要求重新执行 checks。

## 6.2 Iteration 7.2：原子 finish 与严格 terminal progress

人工评审发现的两个剩余 merge blocker 已按同一 fail-closed 原则收口，但最终是否
通过仍由人工评审决定。

### Result/provenance 原子边界

`edits_apply`、`checks_run` reservation、`commands_run` reservation、
`task_cancel` 和 `task_finish` 共用现有 per-task `task_lock`。锁不再由通用
dispatch 隐式代持，而由真实 mutation/lifecycle 方法拥有：

```text
edits_apply
  cheap parse
  -> task lock
  -> re-read active writable Task
  -> operation replay/pending/conflict gate
  -> real workspace mutation
  -> durable operation result
  -> unlock

task_finish
  cheap parse
  -> task lock
  -> re-read Task + blocking Execution
  -> latest check + provenance comparison
  -> capture Result
  -> durable ready_for_review transition
  -> unlock
```

因此 fingerprint 属于状态 A 而 Result patch 属于状态 B、同时 validation 仍为
passed 的组合不可形成。edit/reservation 先占锁时，finish 会看到 stale workspace
或 active Execution；finish 先占锁时，等待者在 durable transition 后重新读取
Task，并以 `task_not_active` 稳定拒绝。Execution 的长时间运行仍在 reservation
之后、task lock 之外进行，review/cancel 不会被长任务阻塞。锁按 Task ID 隔离，
不同 Task 仍可并发；可复用写 workspace 的 prepare/release 继续只由既有
`workspace_ops` 管理，没有新增全局 mutation lock。

review 只读取 workspace/Execution；accept/reject 消费已完成 Result，不会回到
active execution workspace；cancel 使用同一 task lock 串行 lifecycle transition。
没有发现另一条 Hosted capability 可绕过该 coordination domain 启动 workspace
mutation。

### Structured progress 严格协议

非空 `validation_steps` 的 job 只有在 authenticated structured progress 与 durable
plan 完整一致时才能得到 terminal verdict：

- running 必须报告当前 plan step；`completed` 单调、单次最多前进一项，不允许跳步、
  回退、越界或错误名称；完全相同的 progress 可幂等重复；
- terminal success 必须是 `completed + exit_code=0`、`completed == plan.len()`，
  且 `current_step/failed_step` 都为空；
- assertion failure 必须是显式 failed terminal，并在 `completed` 对应位置报告完全
  匹配的 `failed_step`；只有这种 progress 才能生成 durable assertion evidence；
- stopped/cancelled/timeout/lost 不伪造 failed step，且永远不获得 passed provenance；
- 普通 command job 必须没有 validation progress。

malformed update 不再只返回 error 后永久留 active。ShellClientRegistry 将 job
显式收敛为 executor protocol terminal failure，稳定使用
`validation_progress_missing`、`validation_progress_incomplete`、
`validation_progress_invalid` 或 `validation_progress_unexpected`；Execution
投影为 failed/not_run、next action 为升级 Agent 并用新 operation key 重跑 checks。
它不会写 `failed_check`、assertion evidence 或 validated workspace。

数据库删除 succeeded check 根据 `check_plan` 文本推断 `check_completed` 的 SQL
分支。observer 写成功 check 时，store 再次要求完整 completed count、空
`failed_check` 和与起始 hash 一致的非空 `validated_workspace_sha256`；rollback、
普通 command progress、无可信 failed step 的 evidence 和非成功 provenance 都
fail closed。stdout/stderr 只保留 bounded diagnostics/output 作用，完全不是状态
或 step 控制通道。

Agent capability 新增 `structured_validation_jobs`，serde 默认 false；当前 Agent
注册为 true。`checks_run` 在创建 reservation 前检查该 capability，旧 Agent 返回
稳定的 `structured_validation_unavailable` 和升级建议，不创建 Execution、不发送
`start_validation_job`，也不回退到 marker 或普通 shell 协议。

## 7. Legacy hard cut

已删除：

1. Connector 内 `checks_run -> invoke_kernel -> cargo_* -> run_project_command_capture` 同步等待循环；
2. checks 专用 output/error response 拼装和 task-event validation projection；
3. 无生产调用者的 `Principal/AuthMethod` 未来抽象及其只验证该抽象的测试；保留实际 depot `AuthContext/AuthKind/AuthError`；
4. `/api/projects/replace_in_file` 与 `/api/projects/write_file` 专用 REST compatibility handler、request DTO、router、OAuth route mapping 和重复测试；
5. 未使用的 scope helper 与旧 raw HTTP request DTO。

真实调用者已迁移或已有 canonical 入口：

- checks 统一进入 `wc_executions`；
- edit tools 继续通过 `/api/tools/call`/runtime MCP 和底层 ToolRuntime 使用；
- Hosted connector credential 只能进入 9-capability adapter。

代码搜索只剩一个 Hosted `checks_run` 实现；其中没有 `invoke_kernel` 或 `run_project_command_capture`。MCP 和 Hosted OpenAPI 都从 `surface::capability_specs()` 生成 schema，HTTP handler 只把同名 capability 转发给 `ConnectorRuntime::call`。

## 8. 复用而未删除的资产

- `ToolRuntime::run_job`；
- `ShellClientRegistry` 与 auth-aware job log；
- agent `JobManager` 与 process-group termination；
- local/agent job queue、status、cursor 和 bounded output；
- Rust validation profile、parser 与 diagnostics；
- reusable workspace slot、lease、result capture/release；
- 文件编辑、Git、LSP 与受支持本机 CLI 能力；
- workflow Session/current-session ledger 与 guard：它们仍有 coding task、generic runtime、MCP/HTTP 和测试中的真实调用者，不属于可安全删除的无调用者 façade。

## 9. LOC

统计口径与第六轮一致：`src/**/*.rs`，路径组件命中 `test/tests/*test*` 的文件计入 test-path。

| 指标 | Iteration 7 before | Iteration 7 after | 净变化 |
|---|---:|---:|---:|
| production Rust | 250 files / 122,628 LOC | 250 files / 122,034 LOC | -594 LOC |
| test-path Rust | 92 files / 57,773 LOC | 92 files / 57,864 LOC | +91 LOC |

相对 Iteration 6 的 `120,995` production 基线，目前仍为 `+1,039`；目标 `108,896` 尚差 `13,138`。本轮没有通过压缩格式、删除错误处理、安全边界、有效测试或把 production 逻辑移入测试来伪造减幅。

Iteration 7.1 以 `652d647` 为 before：production `250 files / 122,034 LOC`，
test-path `92 files / 57,864 LOC`。after 为 production
`250 files / 122,531 LOC`（`+497`），test-path `92 files / 58,514 LOC`
（`+650`）。test 达到目标上限；production 超目标 47 行，其中 path-based 口径把
`src/db.rs` 内 47 行 fresh/upgrade/idempotent migration test 计入 production。
其余净增长用于替换 marker runner、增加 executor-owned structured progress、
provenance/evidence 持久化与 stale guard；没有通过移动代码到 test 路径或压缩格式
伪造预算。

Iteration 7.2 以 `0eff1c5` 为 before：production
`250 files / 122,531 LOC -> 250 files / 122,778 LOC`（`+247`），test-path
`92 files / 58,514 LOC -> 92 files / 59,014 LOC`（`+500`）。两项均在本轮
`+250/+500` 目标内；新增状态只收紧原子 ownership、progress protocol、
capability negotiation 和 DB invariant，没有增加 Hosted capability。

## 10. 验证与测试收敛

合同测试覆盖：

- 短 check 终态、长 check quick-yield；
- `kind=check`/ordered plan 持久化；
- operation retry/conflict/new-key rerun/workspace drift；
- fail-fast、真实 exit code、assertion/submission 分层；
- cursor、bounded output、review long-poll；
- queued/running cancel、workspace release；
- degraded grace recovery、unknown reference preservation；
- restart interrupted、active/unknown finish blocker；
- read-only 与 subject isolation；
- MCP/OpenAPI schema 完全相等、9-capability surface；
- 旧专用 REST edit compatibility route返回 404。
- check provenance 的 no-mutation/edit/command/exact-retry/new-key/restart 矩阵；
- forged marker、structured progress 越界/跳步/回退/重复与真实 fail-fast；
- evidence 在无 tail、log 不可用、database reopen、tail window 外仍稳定，且有
  16 KiB 上限与路径 sanitizer；
- fresh DB、Iteration 7 DB additive upgrade 和重复 migration。
- fingerprint 后并发 edit 的确定性复现，以及 edit/finish、command/check
  reservation/finish 的双向 task-lock 顺序；
- 不同 Task 并发、exact edit replay 与 pending/uncertain 不重复 mutation；
- terminal success/failure 缺 progress、partial/current/wrong failed step、
  duplicate/rollback/skip/out-of-bounds 和普通 command progress；
- old Agent capability gate、malformed progress 无 provenance/passed projection，
  以及 DB 不再推断 completed count。

只服务旧同步 checks 与旧 REST wrapper 的测试已删除；共享 command/check 状态机测试保留并改为覆盖通用 Execution 不变量。最终验证：

| lane | 结果 |
|---|---:|
| connector runtime | 44 passed |
| Task Kernel | 9 passed |
| runtime HTTP | 88 passed / 4 ignored |
| MCP | 55 passed |
| OpenAPI | 52 passed |
| metadata | 118 passed |
| OAuth | 245 passed |
| scope | 61 passed |
| ShellClient | 69 passed |
| agent JobManager | 4 passed |
| full `cargo test --bin webcodex` | 1,642 passed / 4 ignored |

所有 lane 均为 0 failed；`cargo fmt --all -- --check`、`cargo check --all-targets` 和 `git diff --check` 也通过。

## 11. Iteration 8 边界

下一轮只做产品精修和继续减法：

- 一条命令 onboarding/doctor/status；
- 在同一 check adapter 上增加受证据约束的 project-aware recipe，不增加模型 capability；
- 真实 ChatGPT MCP/OpenAPI quick-yield/reconnect acceptance；
- 对仍有真实调用者的 Session/runtime/CLI 边界先迁移调用者，再做第二轮 hard cut；
- 命名、错误 envelope 和大文件 ownership 收敛。

Iteration 8 不提前引入远程 executor reattach、多 Execution 并行 scheduler、workflow DSL、浏览器产品面或第 10 项 Hosted capability。

Iteration 8 的准入条件增加为：Iteration 7.2 atomic finish/strict progress、
provenance/forged-progress/durable-evidence 的 focused 与 full suite 全绿，
fresh/Iteration 7 DB upgrade 通过、production/test LOC 与删除清单完成记录，
并经人工评审当前 dirty worktree。准入前不得扩 capability 或 recipe；本文不宣布
Iteration 7 已通过 merge gate。
