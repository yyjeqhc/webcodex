# Roadmap

这份 roadmap 故意保持很短。长期边界、LOC/test 预算、验收门禁和时间估算见 [PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md](PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md)。

## 已完成：Iteration 6/6.1 — Command Execution Engine

- 建立持久化 `wc_exec_*` Execution 生命周期。
- `commands_run` 采用约 8 秒 quick-yield：短命令直接完成，长命令返回 durable running。
- `task_review` 提供状态、queue/silent metadata、output cursor 和有界 wait。
- 增加单意图 `task_cancel`，完成 executor/process-group 取消闭环。
- runtime restart 后将未完成 execution 标为 interrupted/unknown。
- active/unknown execution 存在时，`task_finish` fail closed。
- 复用现有 job/process 资产，删除 Connector direct synchronous command path。

## Iteration 7 merge gate：7.2 待人工评审

- `checks_run` 已迁移到同一 Execution Engine，一个 ordered fail-fast plan 对应一个 `kind=check` Execution。
- check 复用 quick-yield、review、cancel、monitor、restart reconciliation 和 finish blocker。
- `operation_id` 支持精确重试、payload conflict 与新 key 有意重跑。
- assertion failure 与 submission/transport/execution failure 分层。
- Hosted credential 停止访问 legacy 76-tool/session/admin surface。
- 删除同步 checks adapter、重复 projection、无调用者 Principal 抽象和两条专用写文件 REST 兼容路由。
- MCP、OpenAPI、HTTP 与 OAuth 保持 canonical 9-capability 合同。
- production Rust 相对 Iteration 7 起点净减 594 行；10% 长期减幅尚未达到。
- 成功 check 独立持久化 workspace provenance；后续 mutation 使
  `task_finish` 返回 `checks_stale`，不改变 operation exact-retry identity。
- stdout marker 控制路径已删除；agent JobManager 拥有 structured validation
  progress，项目输出不能伪造 completed/failed step。
- failed check 持久化最大 16 KiB 的 sanitized assertion evidence；旧数据库 additive
  upgrade，历史成功 check 缺 provenance 时要求重跑。
- `edits_apply`、command/check reservation、cancel 和 `task_finish` 使用同一
  per-task coordination domain；Result capture 与 provenance comparison 原子，
  不同 Task 仍可并发。
- structured validation terminal success 必须有完整 plan-aligned progress；
  malformed progress 显式成为 executor protocol failure，DB 不再按 plan 文本推断
  completed steps。
- Agent 通过 `structured_validation_jobs` 明确协商；旧 Agent 收到
  `structured_validation_unavailable`，没有 marker、普通 shell 或 stdout fallback。
- Iteration 7.2 当前是 dirty worktree，等待 final squash/amend 和人工 merge review；
  本文不自行宣布 Iteration 7 已完成。

## 准入后下一步：Iteration 8 — 产品精修

Iteration 8 尚未开始。先完成 Iteration 7.2 focused/full suite、fresh/Iteration 7
数据库升级验证、LOC/删除清单登记和 dirty worktree 人工 merge review；评审前不扩
capability 或 validation recipe。

- 一条命令 onboarding、doctor、status。
- Rust/Node/Python/Go 的最小 project-aware check recipes。
- 最小 review/cancel/accept CLI 与 Browser UI。
- replay baseline、真实 ChatGPT MCP/OpenAPI acceptance、第二轮删除。
- production Rust LOC 相对 2026-07-23 基线净减少 20% 以上。

## 时间判断

- 三轮合计约 7–11 个专注开发周。
- 单人非全职推进更现实为 3–5 个月。
- SSH/跨设备 Operations Profile 在 Coding Profile 收敛后另行规划，不阻塞当前主线。

## 已完成基础

- Hosted connect 与 project-bound 8 项 capability。
- SQLite Task/Run/Event/Result/Approval。
- 隔离执行工作区与本机 accept/reject。
- 事务式多文件 edit/create/delete/rename。
- LSP Phase 1–3 read-only 能力。
- Validation Intelligence MVP。

## Non-Goals

- 内置模型或 agent loop。
- 完整 IDE replacement。
- 默认 autonomous DevOps。
- 在 Execution Engine 稳定前扩展 SSH、Android、Kubernetes 或 workflow DSL。
- 为假想消费者保留 aliases、dual shapes 或旧 Hosted tool surface。
- 以测试数量、工具数量或 LOC 增长作为完成度。
