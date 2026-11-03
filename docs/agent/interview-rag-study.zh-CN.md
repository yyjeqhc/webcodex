# RAG / Agent 应用面试：基于 WebCodex 设计文档的本地实验

> **学习实验，不是产品合同**：WebCodex 的 Project Memory、Skill/Plugin Catalog、Context Sidecar 与 Workflow Session 并不自动组成通用向量 RAG。下述实验应放在独立的本地知识库目录中，不能替换或修改生产 ToolRuntime。先读 [面试架构学习路径](interview-architecture-study.zh-CN.md)。

## 从可信检索到可信回答的链路

~~~text
允许的 Git 仓库文档（记录准确 SHA / 文件哈希 / 路径）
  -> Markdown 结构化解析、版本和类型分类
  -> 标题/段落分块，每块绑定 path + line range + source_kind
  -> FTS5/BM25 词法检索 + 可选 CPU 多语 Embedding
用户问题 -> 查询词扩展 -> 双路 Top-K -> 融合/重排 -> 引用证据包
  -> （可选本地 LLM）严格依证据回答，不足则拒绝推断
  -> 检索、回答正确性、引用率、延迟与版本新鲜度评测
~~~

**不要混淆**：Retrieval 是拿到相关证据；Generation 是依据证据生成回答；Agent 还涉及工具选择、权限、异步任务、恢复和验收。没有引用与评测的“文档聊天”不能自称可靠 RAG。

## 设计决策、取舍及工程限制

1. **选择性建索引**：先纳入权威架构、Session、Job、MCP、Browser、Computer、Code Mode、Agent Task、Host 适配文档。不要递归索引整仓、依赖、构建文件、凭据、历史日志或用户数据。
2. **分块时保留定位能力**：按 Markdown 标题/段落拆块，保留文件路径、起止行、SHA256、仓库 Revision；不靠 LLM 猜某段出处。
3. **文档类型优先**：当前领域合同、实验报告、研究提案和历史基准即使表达相似，也不能混成同一权威。检索结果必须可见 source_kind；回答时区别“现状”和“当时实验”。
4. **精确词法与跨语言语义互补**：FTS5 适合 error_kind、API 名称、文件路径；Embedding 更适合中文问题命中英文工程说明。多语模型优先于只支持英文或中文的模型；需测命中率再选。
5. **融合检索不是授权**：检索前先判断资料是否属于可见的精确 Project 和授权主体。用户有仓库路径不意味着可读取另一个用户的 Session、私有 Trace 或密钥。
6. **本地优先**：模型及索引独立安装到本地目录；默认无远端 Embedding API。存储生成的向量/FTS 数据和日志时，同样遵循敏感数据边界。
7. **不直接照搬外部内容的命令**：Issue、Markdown、网页都可能是 Prompt Injection；它们是来源事实，不是可执行指令。
8. **可重复验证**：每次评测保存源 SHA、Chunk 算法版本、Embedding 模型、Top-K、Query 扩展版本、预期权威来源，检查召回与引用，不把流畅回答等于准确答案。

## 高频面试追问

| 问题 | 应回答什么 | 可演示的来源 |
|---|---|---|
| RAG 和微调区别是什么？ | RAG 在推理时检索外部可更新证据；微调改变模型参数，二者成本/治理不同 | 本地实验 |
| 词法和向量检索各有什么盲区？ | 精确 API/错误码靠 FTS，跨语言/同义靠 Embedding；二者需独立 Recall@K | [工具合同](tool-contract-guidelines.md) |
| Chunk 大小怎么定？ | 结合标题、代码块、上下文完整性调参与评测，不存在万能固定长度 | 本地实验 |
| 为什么要 Rerank？ | 粗召回偏覆盖，重排偏精度；增加计算与延迟，需测质量收益 | 本地实验 |
| 文档可能过时怎么办？ | SHA、源类型、更新时间、源码核验；历史实验必须明确标注 | [Code Mode 实验](../experiments/code-mode.md) |
| Agent 查询不到证据怎么办？ | 返回证据不足并提供准确的进一步检索路径，而不是编造 | [Handoff 合同](session-model.md) |
| 多租户 ACL 怎么做？ | 检索前过滤、索引/缓存与权限作用域一致；不能仅在生成后过滤 | [权限模型](permission-model.md) |
| 如何解释长期任务持续性？ | Session 是任务证据，Job 是执行；RAG 提供额外知识检索，不能代替这些状态机 | [Durable Agent](../architecture/durable-agent-runtime.md) |
| 生产如何做增量索引？ | 文件哈希和 Git SHA 变更触发局部删除/更新，不复用旧向量 | 本地实验 |
| 衡量指标有哪些？ | Retrieval Recall@K/MRR、引用准确、答案忠实度、拒答、P50/P95、索引成本 | 本地实验 |

## 四个必须做的检索实验

**A. 精确标识符**：搜索 “SessionLifecycle Active Closed Cold”，预期 [Session 模型](session-model.md) 和 [驻留](../implementation/session-active-residency.md)。评测需区分业务生命周期和内存驻留。

**B. 跨语言问答**：输入“命令超时了为什么不能直接重试”，预期 [Job 可靠性](job-reliability-and-concurrency.md)；比较纯 FTS、中文术语扩展以及本地 Embedding 的前五命中。

**C. 历史冲突**：输入“Code Mode 已经默认生产上线了吗”，要求同时查看 [Code Mode 原型](../experiments/code-mode.md) 和 [Host Sidecar](host-code-mode-sidecar-consumption.md)，不能把两者混为一谈。

**D. 证据不足**：询问“ChatGPT Pro 每次模型轮次的实际 Token 数”，WebCodex Server 仅凭 MCP ActionAudit 通常不能证明 Host 内部的完整 Token 消耗。应拒绝精确数字而非推测。

## 评测数据与答案约束

一个最小可运行的 QA 集应包含 question、expected_source_paths、source_kind 和 answerability。先统计 Top-1/3/5 的来源命中率，再做人工“是否引用支持结论”复核；给定相同输入、相同 Git SHA、模型版本和索引参数才可比较。

检索输出必须保留 path:line_start-line_end，并向学习者展示来源章节、引用摘录、检索方法及分数。可选生成层建议采用“先给证据，后给答案”的模式，找不到证据应返回“不足以回答”。

## 与 WebCodex 的真实能力区别

- [memory.rs](../../src/tool_runtime/memory.rs)：有持久化和权限边界，但不能仅据此称为向量 RAG。
- [skills.rs](../../src/tool_runtime/skills.rs)：按需发现和读取 Skill 正文，是渐进披露，不等于向量索引。
- [context_projection.rs](../../src/tool_runtime/context_projection.rs)：请求有界材料注入，不是每轮全量知识搜索。
- [plugin_gateway.rs](../../src/plugin_gateway.rs)：执行能力绑定；描述不是执行授权。
- [agent_task.rs](../../src/tool_runtime/agent_task.rs)：长期 Agent Task 的拥有者与执行状态；RAG 并不替代它。

这份学习方案适合从单机 SQLite FTS5 开始；只有初步 Recall@K 已可测，再安装本地多语 Embedding 模型做混合检索。不要一开始就下载大语言模型、部署向量数据库和自动 Agent 框架，导致无法判断哪个组件真正带来质量收益。
