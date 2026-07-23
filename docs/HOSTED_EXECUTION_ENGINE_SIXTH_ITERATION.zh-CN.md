# Hosted Execution Engine：第六轮与 6.1 稳定化记录

> 状态：Iteration 6 durable command vertical slice 与 Iteration 6.1 behavioral stabilization 已完成，2026-07-23。未 push、tag、deploy，也未创建 commit。

## 1. 结果

Hosted surface 现在是 9 个单意图 capability：

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

`commands_run` 不再调用 Connector 的同步 `invoke_kernel("run_shell")`。短命令可在一次调用内返回终态；长命令约 8 秒后返回 durable `execution_id`、状态、queue/silent metadata、output cursor 和下一动作。请求返回不会取消后台 Execution。

## 2. 实际调用链与复用资产

```text
commands_run
  -> Task 校验 / workspace precondition / 一次性审批
  -> SQLite wc_executions: accepted
  -> ExecutionService: starting
  -> ToolRuntime::run_job
  -> ShellClientRegistry start_job
  -> agent JobManager
```

观察链复用 `ShellClientRegistry::job_log_for_auth` 的 job 状态、单调 cursor 和 bounded output；SQLite 不复制 stdout/stderr body。取消链复用 `stop_job_model_facing`、agent `JobManager::stop` 和既有 workspace lease：

```text
task_cancel
  -> durable cancel_requested
  -> queued removal 或 stop_job dispatch
  -> process-group SIGTERM / SIGKILL
  -> cancelled 或 transport-unknown
  -> durable terminal event
  -> Task cancelled
  -> workspace lease release
```

没有新增 scheduler、process manager、Connector 私有 job system 或第二套 output store。

## 3. Execution contract

`wc_executions` 记录 `wc_exec_*`、kind、Task/Run、时间戳、状态、stdout/stderr cursor、exit code、failure source/code、取消时间、terminal reason、调用方 `operation_id`、规范化 `request_sha256` 和内部 job reference。

状态链为：

```text
accepted -> starting -> queued/running
queued/running -> succeeded/failed
accepted -> cancelled
queued/starting/running -> cancel_requested -> cancelled/failed/unknown
active --runtime restart--> interrupted
active --lost terminal authority after grace--> unknown
```

`commands_run.operation_id` 现在与 `edits_apply` 一样是调用方生成的幂等键：

- 同一 Task/Run 内，相同 `operation_id` 加相同 command/cwd/规范化 timeout 返回原 Execution，不重复 spawn；
- 相同 `operation_id` 加不同 payload 稳定返回 `operation_id_conflict`；
- 相同命令需要有意重跑时必须生成新的 `operation_id`，即使旧 Execution 已终态也会创建新 Execution；
- `request_sha256` 不包含 workspace hash，所以响应丢失后的精确重试即使 workspace 已变化也仍复用原 Execution；
- approval action hash 继续额外绑定 workspace precondition，新一次有意执行仍需要针对当时 workspace 的本机审批。

典型调用：

```json
{
  "task_id": "wc_task_...",
  "operation_id": "cargo-test-after-fix-02",
  "command": "cargo test",
  "timeout_secs": 120
}
```

网络重试必须原样复用 `cargo-test-after-fix-02`；修复代码后再次运行同一条 `cargo test` 必须换一个新 key。`submission_status`、`execution_status`、`exit_code`、`terminal_reason`、`assertion_status=not_run` 和 `capability_outcome` 继续独立投影。

## 4. Review、恢复与 fail-closed

- `task_review` 支持 `after_cursor`、`wait_ms <= 15000`、`max_events <= 50` 和 `include_output_tail`；
- long-poll 在 task event、Execution 状态、output cursor、terminal 或 heartbeat deadline 时返回；
- `starting` 阶段取消只持久化 `cancel_requested`，没有 job ID 时不声称 stop 已发送，也不启动普通 status monitor；
- `run_job` late return 后，attach 事务始终保留真实 job ID，再读取最新状态；如已取消，立即补发 stop 并观察 agent 终态；
- monitor registry 按 `execution_id` 去重；retry、重复 cancel 和 attach 同时触发时仍只有一个 observer，任务退出由 drop guard 清理注册；
- executor status 查询失败或 agent transport `lost` 先进入 `observation_status=degraded`；默认 grace 为 30 秒，轮询从快速阶段退避到 500ms/1s，持续失败越过 grace 后才进入 `unknown`；
- 未识别 executor status 记录 `executor_status_unrecognized`，保持原 active state 和 degraded 投影，不伪装成 `running`；
- startup reconciliation 不伪造成功/失败；无法重新验证的 active handle 进入 `interrupted`，Run/Task 进入 `interrupted`/`needs_attention`；
- `task_finish` 在 active、cancel-pending 或 `unknown` Execution 存在时返回 `execution_not_terminal`；
- Task 外部命令 await 不持有 Task metadata lock，因此 active command 期间 review/cancel 可进入。

`interrupted` 专指 runtime restart 后无法重新验证旧 handle；`unknown` 专指当前 runtime 在故障 grace 到期后仍无法确认 executor 终态，或取消 dispatch 明确无法确认。两者都不伪造命令失败或成功；`unknown` 继续保存已知 executor reference 并阻止 `task_finish`。

取消后的 workspace release 由 per-task async lock 合并并发调用：后来的调用会等待当前 release，而不是抢先返回 terminal response。真实成功由 lease 已移除表示；warning/任务失败后，等待者或后续调用可重试，不会留下永久“已释放”假状态。

为避免在本稳定轮次制造 projection 兼容迁移，`blocker_execution_id` 字段名暂时保留；它只表示阻止 `task_finish` 的 Execution，不表示 queue predecessor。

## 5. 删除与测试收敛

删除：

- Connector `commands_run -> invoke_kernel("run_shell")` 同步执行与旧 output/error glue；
- `Database::interrupt_connector_runs`，由 Execution-aware reconciliation 替换；
- `raw_command_waits_for_local_one_time_approval` 旧同步 replay 测试；
- 已由 canonical registry、安全或 contract lanes 覆盖的重复/helper 测试。

新增/整合的核心证据覆盖：短命令终态、静默 quick-yield、cursor/output 唤醒、heartbeat、active 并发 review/cancel、queued 不启动即取消、workspace slot 释放、restart interrupted、unknown finish blocker、non-zero exit 分层、`operation_id` retry/conflict/有意重跑、starting cancel late attach、单 monitor、transient failure grace、未知 status、release retry、store error 传播，以及真实 descendant process-group 终止和 `ESRCH`/`EPERM`/其他 signal error 分类。

## 6. LOC 与验证

| 指标 | before | after | 净变化 |
|---|---:|---:|---:|
| production Rust | 246 files / 120,995 LOC | 249 files / 122,194 LOC | +3 files / +1,199 LOC |
| test-path Rust | 90 files / 56,454 LOC | 92 files / 57,247 LOC | +2 files / +793 LOC |
| `connector_runtime/mod.rs` | 3,289 | 3,171 | -118 |
| `db/task_kernel.rs` | 1,944 | 1,921 | -23 |

新 production 文件分别为 `execution.rs` 512 行、`executions.rs` 525 行、`execution_model.rs` 203 行，均低于 600 行目标。

Iteration 6.1 以 Iteration 6 完成点为 before baseline：

| 指标 | Iteration 6.1 before | Iteration 6.1 after | 净变化 |
|---|---:|---:|---:|
| production Rust | 249 files / 122,194 LOC | 250 files / 122,628 LOC | +1 file / +434 LOC |
| test-path Rust | 92 files / 57,247 LOC | 92 files / 57,773 LOC | +0 files / +526 LOC |
| `src/bin/webcodex-agent.rs` | 6,716 | 6,741 | +25 |
| `src/connector_runtime/mod.rs` | 3,171 | 3,206 | +35 |
| `src/connector_runtime/execution.rs` | 512 | 525 | +13 |
| `src/db/executions.rs` | 525 | 594 | +69 |
| `src/db/task_kernel.rs` | 1,921 | 1,928 | +7 |

monitor ownership 从 `execution.rs` 原位移到 `execution/monitor.rs`（233 行），替换旧内联 monitor glue；`execution.rs`、`execution/monitor.rs`、`executions.rs` 和 `execution_model.rs` 最终均不超过 600 行。

Iteration 6.1 通过的门禁：

- `cargo fmt --all -- --check`；
- `cargo check --all-targets`；
- Connector focused：30 passed；
- Task Kernel：9 passed；
- Runtime HTTP：92 passed、4 ignored；
- MCP：55 passed；
- OpenAPI：52 passed；
- metadata：118 passed；
- OAuth：247 passed；
- scope：68 passed；
- agent JobManager：3 passed；
- workspace release race 重复 10 次：10 passed；
- full `cargo test --bin webcodex`：1655 passed、0 failed、4 ignored。

## 7. Iteration 7 精确交接

本节是第六轮完成时的历史交接。Iteration 7 已按该边界把 `checks_run` 迁入同一 Execution lifecycle，并完成首轮 hard cut；实际语义、删除证据、LOC 结果和未删除边界见
[`HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md`](HOSTED_EXECUTION_ENGINE_SEVENTH_ITERATION.zh-CN.md)。

第六轮要求的 command retry、取消竞态、monitor 生命周期和故障恢复合同仍由统一 Execution 测试保护；第七轮没有另建 check-specific service、scheduler、process manager 或 output store。

仍需外部 acceptance 的风险是：真实 ChatGPT MCP/OpenAPI client 的 long-poll/取消重连行为、CI 对 branch head 的独立成功记录，以及 runtime restart 后任意 OS child 的跨进程重新附着；最后一项仍是明确非目标。
