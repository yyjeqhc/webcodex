# Roadmap

这份 roadmap 故意保持很短。长期边界、LOC/test 预算、验收门禁和时间估算见 [PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md](PROJECT_FIRST_REFINEMENT_PLAN.zh-CN.md)。

## 当前：Iteration 6 — Execution Engine

- 建立持久化 `wc_exec_*` Execution 生命周期。
- `commands_run` 采用约 8 秒 quick-yield：短命令直接完成，长命令返回 durable running。
- `task_review` 提供状态、queue/silent metadata、output cursor 和有界 wait。
- 增加单意图 `task_cancel`，完成 executor/process-group 取消闭环。
- runtime restart 后将未完成 execution 标为 interrupted/unknown。
- active/unknown execution 存在时，`task_finish` fail closed。
- 复用现有 job/process 资产，删除 Connector direct synchronous command path。

## 下一步：Iteration 7 — Checks、Hard Cut、Deletion

- `checks_run` 迁移到同一 Execution Engine。
- Hosted credential 停止访问 legacy 76-tool/session/admin surface。
- 删除 JSON SessionStore、current-session 和旧 Hosted orchestration 中已无调用者的路径。
- 收敛 MCP、OpenAPI、OAuth scope、CLI 和重复 projection。
- production Rust LOC 相对 Iteration 6 起点净减少至少 10%。

## 然后：Iteration 8 — 产品精修

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
