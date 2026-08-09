# Roadmap

WebCodex 是面向 coding assistant 的远程、可审计、有界执行层。它不是内置模型、自主 agent loop，也不是完整浏览器 IDE。

## 当前已交付基线

- project-bound MCP 与 OpenAPI 暴露精简的 canonical capability surface。
- Task、Execution、Event、Result、Approval、续接 review 和有界输出均可持久化。
- server、CLI 和 runner 通过 workspace library crates 共享代码，并由 package boundary 检查约束。
- 认证、project grant、allowed roots、路径策略、authority mode 和审计证据保持显式边界。
- structured validation 支持 Rust、Node、Python 和 Go recipe，不安装依赖，也不运行联网 setup hook。
- review console、重连续接、只读 LSP 导航、shell profile 和 transport fallback 已可用。

## 下一阶段优先级

本次 execution cycle 以 **降低模型执行摩擦** 为中心，而不是继续扩张 fleet/运维能力；A–F 已完成最终验收，后续只做 maintenance/stabilization。设计约束见 [MODEL_EXECUTION.md](MODEL_EXECUTION.md)。

1. Execution Phase A–F 已完成实现和最终验收：可信 lifecycle、structured process/script、同一次执行 Job handoff、有界 batch observation、non-pinning polling、实用 Job concurrency/observability，以及 Windows 本地 process output normalization 均已通过 Linux 回归与真实 Windows/MSVC service-context 验证。
2. 本轮后续 execution 工作只做 maintenance/stabilization：只修有证据的 regression，保留 Linux/Windows acceptance evidence；没有新的具体需求时不再增加 execution feature phase。
3. 继续保持 structured MCP result，并允许可选的 conversation-level Orchestrator，但不让 UI 或可选 MCP 2026 extension 成为 execution truth 的一部分。

## 暂缓，直到出现当前需求

- Runner drain/maintenance/self-upgrade 与更完整的 fleet dashboard。
- Windows SCM service lifecycle 产品化；当前 dogfood 可以继续使用外部/手工 service wrapper。
- 通用 process/service management API、batch Job launch、PTY terminal UX 或精细化 MCP App UI。
- 在目标 Host 尚未形成稳定产品合同前依赖 MCP Tasks、MRTR、elicitation 或 progress extension。

## 完成标准

Roadmap 项目只有在公开合同已文档化、聚焦与回归验证通过、失败行为明确，并且涉及运维时具备部署或回滚说明后才算完成。

## 明确非目标

- 内置模型选择、prompt loop、context compaction 或 token budget。
- 完整 IDE replacement 或任意 computer use。
- 默认自主部署或生产环境变更。
- 为假想消费者保留 compatibility alias。
- 把工具数、测试数或 LOC 当作产品进度。
