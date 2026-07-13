# MCP 无响应挂起修复 — 评审报告

| 字段 | 值 |
|---|---|
| 分支 | `fix/mcp-no-reply-hangs` |
| 相对基线 | `main`（`09914ce`） |
| 提交数 | 8 |
| 范围 | `0e88679` … `1e5655b` |
| 状态 | 本地提交完成，**未 push** |
| 报告日期 | 2026-07-13 |

---

## 1. 问题回顾

### 1.1 症状

Chat 窗口通过 `/mcp` 调用工具时，**有时没有回复**（HTTP 请求长时间挂起，客户端先超时）。

### 1.2 根因模型

`/mcp` 是同步请求/响应：`tools/call` 会 `await runtime.call_tool_with_context(...)`。修复前：

- `mcp_post` 无外层超时
- Salvo 无全局请求超时

因此只要工具执行卡住或极慢，HTTP 请求就会一直悬着 → 前端「没有回复」。

间歇性来自时序/环境触发（agent 在途断连、本地命令派生后台进程、ledger/磁盘负载等）。

### 1.3 原评审问题清单 vs 本分支处置

| # | 问题 | 原优先级 | 本分支 | 对应提交 |
|---|---|---|---|---|
| 1 | Agent 断连时同步工具 waiter 被遗弃 | P0 | **已修** | `0e88679` |
| 2a | 本地命令 process group / 管道泄空无限挂 | P0 | **已修** | `748c7ff` |
| 2b | 本地 `spawn_blocking` 无外层超时 | P0 | **已修** | `b81232b`、`1e5655b` |
| 3 | 每次工具调用同步全量 ledger 落盘 | P1 | **已修** | `3f22e4a` |
| 4 | 同步 sqlite 堵 async worker | P1 | **明确不做**（性能项） | — |
| 5a | `mcp_post` 无外层超时 | P1 | **已修** | `af72bf5` |
| 5b | Salvo 无请求超时 | P1 | **已修** | `fea0ecf` |
| 6a | 入队不判断 agent 在线 | P2 | **已修** | `4cf8bca` |
| 6b | 后台 reaper 清掉线 client/pending | P2 | **未做**（长稳） | — |
| 6c | 全面 cancel-on-drop | P2 | **部分已有**，未扩展 | — |
| 7 | ws/quic 空闲读超时 / 服务端主动 ping | P2 | **未做**（半开连接加速失败） | — |

**结论（对 hang 主症状）：** 直接成因与纵深超时、主要放大器均已落地；DB / reaper / server ping 属性能或长稳，不阻塞本修复合入。

---

## 2. 提交一览（时间正序）

| # | Hash | 标题 | 层 |
|---|---|---|---|
| 1 | `0e88679` | fail pending sync agent requests fast on transport disconnect | Agent 路径 |
| 2 | `748c7ff` | reap local command process groups to prevent output-drain hangs | 本地子进程 |
| 3 | `af72bf5` | bound MCP dispatch with a hard timeout backstop | MCP 边界 |
| 4 | `b81232b` | bound local command spawn_blocking with an outer hard timeout | 本地子进程 |
| 5 | `fea0ecf` | add Salvo whole-service HTTP request timeout backstop | HTTP 边界 |
| 6 | `3f22e4a` | move session ledger persistence off the request path | 会话持久化 |
| 7 | `4cf8bca` | reject agent enqueue when the client is outside the online window | Agent 入队 |
| 8 | `1e5655b` | bound local git tool spawn_blocking with the same hard timeout | 本地 git 一致性 |

**Diff 规模（相对 `main`）：** 15 files, +754 / −107。

---

## 3. 逐提交说明

### 3.1 `0e88679` — Agent 断连 fail-fast（P0）

**问题**

`reconcile_disconnect` 只清理 job 型请求。MCP 常用的同步工具（`run_shell` / `read_file` / write / lsp / 项目操作）入队时 `job_id: None`，oneshot waiter 活在全局 `pending_by_id` 中。Agent socket 在途掉线后：

- 等待者不会被 complete
- 调用方只能干等自己的 `tokio::time::timeout`（最长约 120s+）
- Chat 客户端早已超时 → 「没有回复」

**改动**

- 新增 `resolve_disconnected_sync_requests_locked`：按 `client_id` 找出 `job_id.is_none()` 的 pending，对其 waiter 发送明确错误（含 `"offline"`，便于 `agent_offline` 分类）
- 在 `reconcile_disconnect` 的 job 清理之后调用
- Job 型请求仍走原有 `lost` 路径，互不干扰

**关键路径**

- `src/shell_client/requests.rs`
- `src/shell_client/agents.rs`
- 回归测：`reconcile_disconnect_fails_pending_sync_requests_fast`

**行为变化**

| 场景 | 修复前 | 修复后 |
|---|---|---|
| Agent 在途断连 + 同步工具 | 干等至工具层超时 | 立即失败，错误含 offline |
| 异步 job | 不变（仍标 `lost`） | 不变 |

---

### 3.2 `748c7ff` — 本地命令进程组回收（P0）

**问题**

`run_command_sync` 在子进程退出后调用 `wait_with_output()` 读到 EOF。若命令后台派生了继承 stdout/stderr 的孙进程（`some-daemon &`、nohup、double-fork 等），管道写端不关 → **永久阻塞**。该逻辑在不可取消的 `spawn_blocking` 内，本地分支原先无外层 timeout → MCP 永久挂起。超时分支的 `child.kill()` 只杀直接子进程，同样有缺陷。

**改动**

- Unix：`process_group(0)`，子进程 pgid == pid
- 退出或超时后、读管道前：`libc::kill(-pgid, SIGKILL)` 杀整组
- `Cargo.toml` 增加直接依赖 `libc`（此前已是传递依赖）
- 测试锁定：后台挂起不 hang、正常 exit/stdout、前台超时行为不变

**关键路径**

- `src/tool_runtime/helpers.rs`（`run_command_sync` / `reap_process_group`）

**风险与边界**

- 仅针对该命令自己的 process group，不杀无关进程
- 若子孙 `setsid` 逃出组，本组 reap 管不到（由下一提交硬超时兜底）

---

### 3.3 `af72bf5` — MCP dispatch 硬超时（P1 纵深防御）

**问题**

`mcp_post` 对 `handle_mcp_request_with_lifecycle` 无外层界。任意无界 await 都会让 HTTP 永久沉默。

**改动**

- 常量 `MCP_DISPATCH_HARD_TIMEOUT = 150s`
- `tokio::time::timeout` 包裹 dispatch
- 超时：JSON-RPC `-32000`、HTTP 500、lifecycle 事件 `dispatch_hard_timeout`

**设计约束**

- 各工具内层等待 ≤ ~124s，150s **不会抢在合法内层超时之前**
- 只在真正无界挂起时触发

**关键路径**

- `src/mcp.rs`

---

### 3.4 `b81232b` — 本地 `spawn_blocking` 外层硬超时（P0 收尾）

**问题**

进程组 reap 覆盖常见挂法；`setsid` 等逃逸仍可能让 `wait_with_output` 永不返回。`spawn_blocking` 不可取消，仅 await join 会永久停住。

**改动**

- `run_command_sync_bounded(cmd, cwd, timeout_secs)`：`timeout_secs + LOCAL_RUN_HARD_GRACE_SECS(10)` 外层界
- 失败枚举：`LocalRunFailure::HardTimeout | Join`
- 应用：`run_shell` 本地分支、本地 `search_project_text`

**关键路径**

- `src/tool_runtime/helpers.rs`
- `src/tool_runtime/shell.rs`
- `src/tool_runtime/files.rs`

**说明**

Hard timeout 时 blocking 线程被放弃直到 straggler 退出（`spawn_blocking` 无法取消）；调用方与 MCP 请求已返回。

---

### 3.5 `fea0ecf` — Salvo 全站请求超时（P1 纵深防御）

**问题**

仅 MCP 有界时，其它 handler 若引入无界 await 仍会 silent hang。

**改动**

- Salvo feature：`timeout`
- 路由顶层 `Timeout::new(300s)`（`REQUEST_HARD_TIMEOUT_SECS`）
- 300s > MCP 150s > 工具内层 ~122s → **内层先报、信息更准**

**关键路径**

- `Cargo.toml`
- `src/main.rs`

**不受影响**

- Agent WebSocket：upgrade 握手短，长连接在升级后的 task 中，不占用该 hoop 的长 await

---

### 3.6 `3f22e4a` — Session ledger 后台写入（P1 放大器）

**问题**

带 `session_id` 的调用会 2–3 次 `push_event`，每次同步 `persist_after_mutation`：

1. 全局 `persistence_write_mutex`
2. 克隆/序列化整个 store（最多约 100×200 事件）
3. 同步 `fs::write` + `rename`

全程在 async worker 上，无节流 → 并发与磁盘慢时拖垮调度，表现为间歇变慢/无响应。

**改动**

- 专用 OS 线程 `session-ledger-writer`
- 突变路径：`mark_dirty` + 唤醒，立即返回
- Writer：在同一 write mutex 下 snapshot + `write_ledger_atomic`
- `flush_persistence()`：等待 in-flight（含「dirty 已清、写盘未完成」），供测试与再打开 ledger 文件
- 线程 spawn 失败 → 回退同步路径；内存 store 无 writer

**测试适配**

- `flush_and_restore` / 显式 `flush_persistence` 后再读盘
- `coding_task` 中 sentinel ledger 断言前 flush

**语义变化**

| 点 | 说明 |
|---|---|
| 崩溃窗口 | 进程崩溃时，最后若干事件可能未落盘（换吞吐与 worker 安全） |
| 并发正确性 | 与 `persist_after_mutation_with` 共用 write mutex；写前再 snapshot，避免旧快照盖新数据 |
| 对外 API | 无协议变更；读盘前需 flush（进程内查询走内存，不受影响） |

**关键路径**

- `src/tool_runtime/sessions/store.rs`
- `src/tool_runtime/sessions/tests.rs`
- `src/tool_runtime/tests/coding_task.rs`

---

### 3.7 `4cf8bca` — 离线 agent 入队拒绝（P2）

**问题**

`ensure_dispatch_supported_locked` 只 `contains_key`，不看 `last_seen`。已注册但掉线的 agent 仍接 enqueue：

- 注定等满 caller timeout
- 可把队列堆到 `MAX_QUEUED_REQUESTS_PER_CLIENT`（256），之后该 client **永久** `too many pending requests` 直到进程重启

**改动**

- 入队前：`last_seen` 须在 `CLIENT_ONLINE_WINDOW_SECS`（60s）内
- 否则立即错误：`... is offline (no keepalive within 60s)...`
- 回归测：`registry_rejects_enqueue_when_client_offline`

**关键路径**

- `src/shell_client/jobs.rs`
- `src/shell_client/mod.rs`（测试）

---

### 3.8 `1e5655b` — 本地 git 工具对齐硬超时（一致性收尾）

**问题**

`git_status` / `git_diff` / `git_diff_summary` 本地路径仍裸 `spawn_blocking(run_command_sync)`。进程组已护住常见挂；`setsid` 逃逸仍可能卡 join。

**改动**

- 三处改为 `run_command_sync_bounded(..., 30)`
- Hard timeout → 结构化 `ToolResult::err`

**关键路径**

- `src/tool_runtime/git.rs`

---

## 4. 超时层次（合入后）

```
HTTP 请求
  └─ Salvo Timeout 300s                    ← 全站兜底 → 503
       └─ mcp_post
            └─ MCP dispatch hard 150s      ← JSON-RPC -32000 / dispatch_hard_timeout
                 └─ 各工具内层
                      ├─ agent round-trip  ≤ ~122s（原有 timeout）
                      └─ 本地命令
                           ├─ 命令 timeout_secs（原有）
                           └─ hard bound = timeout + 10s（新增）
```

设计原则：**越内层越具体、越外层越兜底**；外层数值严格大于内层，避免抢先报「硬超时」而丢掉工具层错误分类。

---

## 5. 行为变化摘要（评审关注点）

| 维度 | 变化 |
|---|---|
| Agent 在途断连 | 同步工具立即失败（offline），不再干等 |
| 本地 `cmd &` 占管道 | 进程组杀死后正常返回；逃逸则 hard timeout |
| MCP 无界 hang | 最多 ~150s 有 JSON-RPC 错误 |
| 任意 HTTP 无界 hang | 最多 ~300s 有超时响应 |
| Session ledger | 写盘异步；进程崩溃可能丢最近若干事件 |
| 离线 agent | 新请求入队即失败，不再堆队列 |
| 协议 / 工具 schema | **无变更** |
| OAuth / OpenAPI 操作数 | **无变更** |

---

## 6. 有意不做的项

| 项 | 原因 |
|---|---|
| DB `spawn_blocking` / 连接池 | 负载放大器，非无界 hang；查询为点查，无死等循环。属性能优化，单独立项 |
| 后台 reaper | 新请求已有在线门槛；在途已有断连清理。队列打满后的长稳问题可观察后再做 |
| 服务端主动 ws/quic ping | 已有 agent 侧 Ping 刷新 + 60s 窗口；半开加速失败非主症状 |
| 全面 cancel-on-drop | Handler 路径已有 `cancel_request`；非 hang 必做 |

---

## 7. 验证记录

### 已跑

```
cargo fmt --check
cargo check --all-targets
git diff --check

cargo test --bin webcodex session        # 200 passed
cargo test --bin webcodex mcp            # 53 passed
cargo test --bin webcodex shell_client   # 69 passed
cargo test --bin webcodex git_           # 30 passed

# 聚焦回归
reconcile_disconnect_fails_pending_sync_requests_fast
registry_rejects_enqueue_when_client_offline
run_command_sync_* (helpers)
session / concurrent_persistence / ledger flush 路径
```

### 未跑

- 完整 `cargo test --bin webcodex`（合入 `main` 前建议跑）
- 线上 / 多 agent 负载复现

### 建议线上验收

1. 打开 `WEBCODEX_TOOL_REQUEST_TRACE=true`，观察卡住请求是否停在 `dispatch_started` 后无 `handler_returned`，或出现 `dispatch_hard_timeout` / `incomplete_drop`
2. 结合 nginx `request_time` 区分：agent 干等 vs 本地 hang vs 纯延迟
3. 人为：agent 在途 kill、本地 `sleep 999 &` 类命令、离线 agent 再调工具 — 应快速结构化失败，而非无回复

---

## 8. 风险与回滚

| 风险 | 缓解 |
|---|---|
| Ledger 崩溃丢最近事件 | 可接受；需要强一致时可再引入同步 flush 策略或增量 append |
| Hard timeout 后 orphan blocking 线程 | 极少见；进程组已尽量避免；线程最终随 straggler 退出 |
| 300s Salvo 超时误伤合法长请求 | 当前同步工具设计上限远低于 300s；async job 不走长 HTTP await |
| 离线入队拒绝过严 | 窗口 60s 与既有 `connected` 语义一致；keepalive/Ping 仍刷新 `last_seen` |

回滚：按提交逆序 cherry-pick revert；超时层与断连/进程组可独立回滚。

---

## 9. 评审检查清单（建议）

- [ ] P0 两条直接成因（断连 waiter、本地管道）逻辑与错误文案是否可接受
- [ ] 超时金字塔（工具 / MCP 150 / Salvo 300）数值是否合理
- [ ] Ledger 异步写的崩溃窗口是否可接受
- [ ] 离线入队 60s 窗口是否与 agent 运维习惯匹配
- [ ] 有意不做的 DB / reaper / server-ping 是否同意延后
- [ ] 合入前是否要求跑完整 `cargo test --bin webcodex`

---

## 10. 文件变更清单

| 路径 | 角色 |
|---|---|
| `src/shell_client/agents.rs` | 断连时 drain 同步 pending |
| `src/shell_client/requests.rs` | `resolve_disconnected_sync_requests_locked` |
| `src/shell_client/jobs.rs` | 入队在线门槛 |
| `src/shell_client/mod.rs` | 回归测试 |
| `src/tool_runtime/helpers.rs` | process group + `run_command_sync_bounded` |
| `src/tool_runtime/shell.rs` | 本地 shell 使用 bounded |
| `src/tool_runtime/files.rs` | 本地 search 使用 bounded |
| `src/tool_runtime/git.rs` | 本地 git 使用 bounded |
| `src/tool_runtime/sessions/store.rs` | 后台 ledger writer |
| `src/tool_runtime/sessions/tests.rs` | flush 后再 restore |
| `src/tool_runtime/tests/coding_task.rs` | flush 后再读盘 |
| `src/mcp.rs` | dispatch 150s 硬超时 |
| `src/main.rs` | Salvo 300s 超时 |
| `Cargo.toml` / `Cargo.lock` | `libc`、salvo `timeout` feature |

---

## 11. 合入建议

1. 本报告评审通过后，跑完整 `cargo test --bin webcodex`
2. PR 描述可复用本报告 §1–§5 与 §6
3. 合入后观察 1–2 天 MCP 无回复率与 `dispatch_hard_timeout` / offline 错误比例
4. DB offload、reaper、server ping 如需，另开性能/长稳任务，不绑本 PR

---

*本文档供分支评审使用；不改变运行时行为。*
