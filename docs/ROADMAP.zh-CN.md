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

## 已完成：Iteration 7 — Durable Validation Provenance

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
- Agent 通过 structured validation capability 明确协商；旧 Agent 收到
  `structured_validation_unavailable`，没有 marker、普通 shell 或 stdout fallback。
- Iteration 7 已完成人工 review、完整测试与最终 squash，正式基线为
  `a1547bba3b93669e8bdf6d0fec2388e0ae2b138e`。

## 已完成：Iteration 8.0/8.0.1 — Product Entry, Credential Boundary and Golden Path

Iteration 8.0 只交付第一条垂直切片，不扩展 Hosted 九项 capability：

- 唯一普通用户入口 `webcodex setup`，后续显式
  `webcodex doctor` → `webcodex agent start` → `webcodex status`。
- setup 生成一个由 Connector 与 Agent 共用的精确 Project Credential；两者映射到
  同一非秘密 project grant，普通 arbitrary shared-key fallback 在 project mode
  关闭，loopback 不免除认证。
- readiness、file read/search/edit、command/check、monitor/log 与 cancel 都按请求
  principal 验证 Agent grant；跨 grant 调用在 Task/Execution/pending request 等
  副作用前 fail closed。
- 新 setup client ID 包含非秘密 grant suffix；旧同名 lease 的跨组注册不能覆盖原
  group。
- application-level Project readiness facts 同时投影给 CLI、Connector API 和
  Browser；不再读取 runtime registry 自行拼三套状态。
- malformed/conflicting registration、invalid credential、workspace unavailable、
  server unreachable、credential rejected 与 agent offline 使用不同 stable code；
  doctor/status 保持只读。
- project-bound Connector happy path 从 `task_start` 直接开始，不要求
  `list_projects`、`runtime_status`、`tool_manifest` 或 workflow session。
- golden path 由真实 Auth middleware、Connector HTTP adapter、Agent registry、
  durable Task/Execution/event store 与本机 accept 覆盖；真实 recorder 证明没有
  discovery/ops/session call，手工 calls vector 已删除。
- normal task 在 `task_finish` 前必须运行 structured checks；check spawn failure
  属于 executor infrastructure failure，不生成 assertion evidence/provenance。
- 删除旧 `webcodex connect` process/tunnel orchestration、`webcodex-cli connect`
  与重复 doctor projection；运维 registry/discovery 工具继续保留。
- Browser 只提供 readiness surface；完整 Browser IDE 明确 deferred。

## 当前：Iteration 8.1 — Project-Aware Validation Recipes

Iteration 8.1 的实现候选保持 Hosted 九项 capability，不新增 discovery call：

- `checks_run` 增加可选 `recipe: rust|node|python|go`；省略即 deterministic auto，
  不提供 `auto` alias。
- resolver 在 Task execution workspace 内从相对 `cwd` 向 root 查找最近 marker，
  不扫描 sibling；同 root 多 marker 自动模式 fail closed，显式 recipe 必须匹配。
- Rust 保持 `format/check/test`；Node 只运行 allowlisted 非修改型 script，并从
  `packageManager` 或单一 lockfile 确定 manager；Python 只使用
  `pyproject.toml` 证明的 Ruff/Black、Ruff/Mypy、pytest；Go 支持
  `check/test`，`format` 明确 unavailable。
- 所有 invocation 都是 canonical `program + argv`；recipe 不安装依赖、不运行
  install hook、不修改 lockfile、不联网。旧 command-string Agent 通过
  `structured_validation_argv` capability fail closed。
- recipe ID/version、相对 root、semantic checks、tool identity 和
  invocation/manifest digest 持久化并进入 request hash；manifest、lockfile 或
  workspace 变化使 provenance stale。
- planner failure 不创建 Execution；tool unavailable/spawn failure 属于 executor
  failure；只有已启动 validator 的 non-zero verdict 属于 assertion failure。
- 四类项目由真实 Auth/HTTP/registry/store/structured progress/local accept
  golden fixture 覆盖；task path 仍不需要 discovery/session call。

本轮等待 focused/full suite、LOC 门禁与人工 review；不自行宣布 merge gate
通过。通过后再继续：

- 已完成（Iteration 8.2）：Browser `/console` 的最小 review/cancel/accept UI —
  工作队列 + review 详情（bounded diff/output tail）+ Accept/Reject/Cancel 复用与
  CLI 相同的本机授权，不新增第十项 capability，credential 仅在内存中。
- replay baseline、真实 ChatGPT MCP/OpenAPI acceptance、第二轮删除。
- production Rust LOC 相对 2026-07-23 基线净减少 20% 以上。

## 时间判断

- 三轮合计约 7–11 个专注开发周。
- 单人非全职推进更现实为 3–5 个月。
- SSH/跨设备 Operations Profile 在 Coding Profile 收敛后另行规划，不阻塞当前主线。

## 已完成基础

- project-bound Connector 与当前 9 项 capability。
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
