# WebCodex Agent Infra 面试学习路径（源码对照版）

> **定位**：2026-11 在独立文档分支上编写的维护者学习地图，不是新的执行合同、默认功能承诺或全面源码审计。命令、身份、权限和生命周期以 [AGENTS.md](../../AGENTS.md)、[总体架构](../ARCHITECTURE.md)、下述领域合同和现行源码为准。历史实验必须保留其原始基线，不把实验结果当作当前线上数据。

## 学习目标及系统图

先讲清“AI Agent 为什么需要 Runtime”：Host 的模型负责语义判断，WebCodex Server 负责身份、权限、路由与状态持久化；Runner 在获准的 Project 上持有真实代码、进程、编译器、浏览器/桌面执行环境。不同客户端可使用同一个基础设施，但服务端并不拥有所有 Host 的隐藏推理或模型轮次。

~~~text
ChatGPT / Codex / Claude
  | MCP / Apps / HTTP
  v
WebCodex Server
  +-- MCP ToolDefinition / OAuth / Project authority / ToolRuntime
  +-- Workflow Session: task instructions, redacted events, messages
  +-- Job Registry: ongoing commands, observation, recovery
  +-- Goal / Agent Task / Conversation / Wake: separate domains
  +-- Context/Control Sidecars, bounded result projection
  |
  v
Runner(s) -> registered Project(s) / Shell / Git / Tests
           -> Browser CDP or Computer OS accessibility
~~~

不要混淆：Host Window ≠ Workflow Session；Job ≠ Session ≠ Goal；MCP 传输身份 ≠ 业务任务身份；Host Code Mode ≠ WebCodex 实验型 V8 Code Mode。

## 八个面试模块

### 1. Server/Runner、Project 和多设备

- **问题**：为什么不让云端模型直接读本机文件？为什么需要 Project ID 和独立 Runner？
- **规范**：[架构](../ARCHITECTURE.md)、[Runner](../RUNNER.md)、[权限决策](permission-model.md)。
- **代码**：[project_tools.rs](../../src/tool_runtime/project_tools.rs)、[runner crate](../../crates/webcodex-runner/src/main.rs)、[tool definitions](../../crates/webcodex-tool-contracts/src/tool_definition.rs)。
- **回答要点**：Project ID 绑定注册的 Runner/Project；Server 根据权限路由，Runner 拥有实际执行；当前可达性、Runner 实例、项目根和作用域不是一个概念。Runner 重连不授权对不确定副作用进行重放。
- **反问**：跨设备是否等于任意文件路径都有访问权？不是；需逐请求检查身份与 Project、Root、Runner Capability。

### 2. Workflow Session、SQLite 与恢复

- **问题**：Session 为什么可以长期 Active 又在内存中 Cold？
- **规范**：[Session 模型](session-model.md)、[SQLite/驻留](../implementation/session-active-residency.md)、[恢复指引](../SESSION_CONTINUITY.md)。
- **代码**：[model.rs](../../crates/webcodex-workflow-session/src/model.rs)、[store.rs](../../crates/webcodex-workflow-session/src/store.rs)、[database.rs](../../crates/webcodex-workflow-session/src/persistence/database.rs)、[writer.rs](../../crates/webcodex-workflow-session/src/store/writer.rs)。
- **回答要点**：Active/Closed 是业务生命周期；Hot/Cold 是内存驻留。Current State 与 Event/Message 表分离；Dirty 状态不可丢弃；有界 Handoff 不重放工具或恢复 ChatGPT 隐藏上下文。
- **反问**：finish_coding_task 是否自动关闭 Session，work_on_project 不传 session_id 是否自动恢复最近任务？均否。历史 Session ID 也不提供权限。

### 3. Job、异步执行与不确定性

- **问题**：命令请求超时以后能否重新调用？Job 的真实生命周期由谁控制？
- **规范**：[Job 可靠性](job-reliability-and-concurrency.md)、[Host Readiness 实验](../experiments/host-job-readiness-reactor.md)。
- **代码**：[observe_jobs.rs](../../src/tool_runtime/observe_jobs.rs)、[job_updates.rs](../../crates/webcodex-runner-registry/src/job_updates.rs)、[structured_execution.rs](../../src/tool_runtime/structured_execution.rs)。
- **回答要点**：请求结束 ≠ Job 结束；Pending 持有同一次执行的 Job ID；不确定副作用不能盲重试。Readiness 的 any/all 是依赖屏障，Terminal 不一定 Success。
- **反问**：为什么真实 Host Promise.race 的外部行为曾和预期不同？只能依据实验的先后时间和后续调用证明，不能推断 Host 私有实现。

### 4. MCP ToolDefinition、Direct/Gateway 和结果投影

- **问题**：为什么 Runtime 可以注册很多工具，但模型不必一次看到全部 Schema？
- **规范**：[工具合同](tool-contract-guidelines.md)、[MCP](../MCP.md)、[客户端合同](../implementation/mcp-client-contract-alignment.md)。
- **代码**：[tool_definition.rs](../../crates/webcodex-tool-contracts/src/tool_definition.rs)、[surface.rs](../../src/tool_runtime/surface.rs)、[mcp tools](../../src/mcp/tools.rs)、[response.rs](../../src/mcp/response.rs)。
- **回答要点**：注册表数量、MCP tools/list 和 Host 实际加载是不同层次。Manifest 可发现名称和精确合同，但不会在 Host 注册新方法。Gateway 不得绕过权限或独立 Host 审查。结构化成功/失败与简短文本呈现应分开。
- **反问**：为什么不能用 MCP 调用总量来代表模型轮次或 Token？Host 可能有内部工具检索和 JavaScript 编排。

### 5. 两种 Code Mode

- **问题**：ChatGPT Host Code Mode 与 WebCodex 自己的 V8 组合执行有什么不同？
- **规范**：[实验型 Code Mode](../experiments/code-mode.md)、[Host Sidecar 可见性](host-code-mode-sidecar-consumption.md)。
- **代码**：[code_mode.rs](../../src/tool_runtime/code_mode.rs)、[V8 crate](../../crates/webcodex-code-mode/src/lib.rs)、[request_policy.rs](../../src/mcp/request_policy.rs)。
- **回答要点**：外部 Host 决定它的 JS 调度；WebCodex 原型有单独的、受限的 VM/回调接口。Server 的 host_code_mode 配置是超时/交接策略，并不证明某个模型使用 JS。把独立调用合并进一个模型编排单元不等于减少底层 RPC。
- **反问**：Code Mode 只打印 stdout，模型是否一定会看见附带的 Window Operator 消息？不能；必须转发重要 Sidecar 到模型可见结果。

### 6. Browser/CDP 与 Computer Use

- **问题**：为什么 Browser 和 Computer 是两种不同业务域？
- **规范**：[Browser/CDP](../architecture/browser-cdp-runtime.md)、[会话连续性](../architecture/browser-session-continuity.md)、[Computer Use roadmap](../COMPUTER_USE.md)。
- **代码**：[browser_tools.rs](../../src/tool_runtime/browser_tools.rs)、[browser crate](../../crates/webcodex-browser/src/lib.rs)、[computer_tools.rs](../../src/tool_runtime/computer_tools.rs)、[computer crate](../../crates/webcodex-computer/src/lib.rs)。
- **回答要点**：Browser 使用 Page、DOM/AX、CDP；Computer 使用 OS Window、Accessibility、Pointer、Keyboard。快照/元素身份、时间和目标变更的 Fence 防止误操作；动作成功不证明业务完成。
- **反问**：一次 form batch 或 action+observe 为什么有收益又有什么风险？减少 Host 往返，但具体 Runner 操作和检查并未消失。

### 7. Agent/Goal/Task/Wake 与协作

- **问题**：为什么已经有 Workflow Session 和 Job 还要 Agent Task、Goal？
- **规范**：[Durable Agent Runtime](../architecture/durable-agent-runtime.md)、[Conversation/Wake](../architecture/durable-agent-conversation.md)、[协作](manual-window-collaboration.md)。
- **代码**：[agent_task.rs](../../src/tool_runtime/agent_task.rs)、[store agent task](../../crates/webcodex-store/src/agent_task.rs)、[store agent wake](../../crates/webcodex-store/src/agent_wake.rs)、[store goal](../../crates/webcodex-store/src/goal.rs)。
- **回答要点**：Goal 表示高层目标，TaskAttempt 表示异步执行归属，Wake 是受控后续处理机会，Session 保存工作证据，Job 管进程。Operator ACK 不等于 Resolve，Window 消息不等于 Session 协作消息。
- **反问**：Webhook 或 GitHub 评论能否直接调用 Runner 写操作？不能；外部消息不是执行授权。

### 8. Memory/Skill/Plugin 与 RAG 的区别

- **问题**：项目中的 Memory、Skill、上下文投影是否就是通用向量 RAG？
- **规范**：[资源模型历史评审](../architecture/resource-model-and-quality-review.md)、[RAG 面试专题](interview-rag-study.zh-CN.md)。
- **代码**：[memory.rs](../../src/tool_runtime/memory.rs)、[skills.rs](../../src/tool_runtime/skills.rs)、[context_projection.rs](../../src/tool_runtime/context_projection.rs)、[plugin_gateway.rs](../../src/plugin_gateway.rs)。
- **回答要点**：Memory 持久化和权限、Skill 的渐进阅读、Plugin 的执行绑定、Context Sidecar 的有界注入是不同系统。不能把“有上下文”直接描述为带 Embedding/Rerank/Eval 的 RAG 系统。

## 面试故事与可核验依据

| 故事 | 查证入口 | 关键取舍和限制 |
|---|---|---|
| 长期 Session 引发 JSON Snapshot 膨胀 | [历史 sf 调查](../implementation/session-memory-sf-2026-10-04.md) + [SQLite 方案](../implementation/session-active-residency.md) | 把业务身份、历史、内存驻留分离；历史合成 RSS 数据不是当前生产收益 |
| Host 多 Job 等待 | [原始实验](../experiments/host-job-readiness-reactor.md) | Host Promise 行为只能通过外部时间观察；受控 Runtime 等待屏障更可靠 |
| 工具内容与 Schema 膨胀 | [基准](../experiments/tool-experience-baseline.md) | JSON 字节 != Token，RPC 次数 != 模型轮次 |
| 验证零用例、源码已修改 | [验证文档](../implementation/validation-success-evidence.md) | 退出码 0 != 充分测试证据；Source Fence 也不能证明全局绝对 Fresh |
| Browser/Computer 自动化 | [Browser 设计](../architecture/browser-cdp-runtime.md) + [Computer](../COMPUTER_USE.md) | 用精确身份、快照、动作后观察约束 GUI 风险 |

## 面试阅读中的“新旧”风险（抽样，不是维护待办清单）

| 文档 | 本轮定位 | 注意 |
|---|---|---|
| [ARCHITECTURE.md](../ARCHITECTURE.md) | 当前架构入口 | 重要细节应回到领域合同/代码 |
| [session-model.md](session-model.md) | 当前领域合同 | 与 SQLite 与 Runner 实际构建对照 |
| [browser-cdp-runtime.md](../architecture/browser-cdp-runtime.md) | 当前 Browser 合同 | 使用时检查客户端能力/部署版本 |
| [COMPUTER_USE.md](../COMPUTER_USE.md) | Roadmap 中包含已实现 substrate | 逐段区分当前能力和未来切片，不可整篇都称未来 |
| [experiments/code-mode.md](../experiments/code-mode.md) | 实验型 WebCodex V8 | 与外部 ChatGPT Host Code Mode 不是一回事 |
| [tool-composition-research.md](../architecture/tool-composition-research.md) | 研究/方向 | 不能把提议当作默认正式能力 |
| [resource-model-and-quality-review.md](../architecture/resource-model-and-quality-review.md) | 2026-09 固定 SHA 的架构评审 | 不能直接外推到当前版本 |
| [tool-experience-baseline.md](../experiments/tool-experience-baseline.md) | 2026-10 固定条件基准 | 量纲、时间与基线必须保留 |

**核查边界**：以上是面试主题的源文件/设计关系抽样，不是对 docs/ 下所有文件的逐段事实审计。没有具体反例时，不应仅凭标题、文件大小或历史词汇认定一个文档已失效。实际发现错误，应按 [维护原则](../MAINTENANCE.md) 修改 owning contract 并验证。

## 三轮学习法

**第一轮**：架构 → 项目路由 → Session → Job → Tool/Host。每个主题能讲清问题、朴素方案的缺陷、选择的边界、失败恢复与证据。

**第二轮**：按岗位选择 Code Mode、Browser/Computer Use、Durable Agent，再做独立 RAG 检索/评测实验。不要将实验型原型包装成已正式发布的生产能力。

**第三轮**：用追问检验自己。如何证明检索没有越权？怎么处理 outcome_unknown？过期 Snapshot 是否还能用于控制？如何避免一个模型窗口错误重启另一个窗口的 Job？为什么一个零测试的成功退出不等于代码验证充分？
