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

下一轮以 **降低模型执行摩擦** 为中心，而不是继续扩张 fleet/运维能力。设计约束见 [MODEL_EXECUTION.md](MODEL_EXECUTION.md)。

1. 先让 execution lifecycle 可信：structured state、retry safety 与人类可读 guidance 必须一致地区分“确定未启动、正在运行、已完成、结果未知”。
2. 增加最小的 structured process/argv 与 script payload 路径，让普通 native command 不再依赖 shell quoting；`run_shell` 保留为 escape hatch。
3. 泛化现有 validation 的同一次执行 handoff：短任务同步返回，长任务继续作为同一个 durable Job；同时用既有 observation token 增加有界 batch Job observation。
4. 在提高并发前先修 transport execution reliability，尤其是 polling dispatch starvation；之后只增加调参真正需要的 running/queued/limit observability。
5. 保持 Job state 与 OS 解耦并兼容未来 MCP App：统一 Windows 输出、维持 structured MCP result，并允许可选的 conversation-level Orchestrator，但不让 UI 或可选 MCP 2026 extension 成为 execution truth 的一部分。

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
