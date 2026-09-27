# v0.4.2 后审查与 Host-native Code Mode 开发方向

日期：2026-09-27。本文讨论 Host 自己的 code mode，不讨论 WebCodex 的 nested Code Mode，也不引入新的执行引擎。

## 1. 范围与结论

审查仓库为 special 的 `/root/git/webcodex`。开始时工作区干净，`HEAD` 与抓取后的 `origin/main` 均为 `be952daf75defcd4bb08cd37d6e4a2a82b1f04f5`。发布基线 `v0.4.2` 为 `44ac49e79b4870959be4395f04210e898b3cf4df`；区间有 51 个 first-parent 提交，涉及 517 个文件。这是基于风险的定向审查，不是对全部文件的完整安全审计。

重点追踪了 Host Job handoff/pending 策略（#692、#696）、MCP invocation envelope（#717）、编辑入口（#714）、Git review/closeout（#723），并读取近期 Codex session 和 sf 的现有 ActionAudit 聚合数据。修复集中在 #723 的真实调用链，没有修改权限、Job 生命周期、Goal/Session 身份语义或部署配置。

结论：下一阶段应让 Host 更可靠地消费现有工具合同、在同一 cell 内处理机械确定的步骤，并减少无收益的观察及模型输出。现有证据不支持首先更换 VM、放宽安全边界、增加常驻 telemetry 或简单提高并发。

## 2. 已确认问题与本次修复

### 2.1 Workspace snapshot 缺少暂存区和分支身份

原 `GitReviewSourceIdentity::Workspace` 仅包含 HEAD commit 和冻结的工作目录 tree。冻结 tree 使用私有临时 index，因此以下情况会得到相同身份：

- 文件内容不变，只执行 `git add`，但 staged/unstaged 分类已经改变。
- 工作目录内容和 `MM` 状态不变，暂存区 blob 从另一版本变成第三版本。
- 切换到指向相同 HEAD 的另一个分支。

`review_changes` 的续页和 compact closeout 依赖该身份判断缓存是否仍适用。漏掉这些状态，会复用过期的 review 元数据或 staged counts。只比较短 status 的 XY 字符也不能解决第二种情况。

修复为冻结 tree 加上 porcelain-v2 status 指纹，覆盖分支、index object IDs、模式、冲突和未跟踪分类。status 在 Runner 的临时文件内由 Git `hash-object --no-filters` 计算摘要，仅传回固定大小的指纹；不把大仓库的整份 status 传回 Server。读取使用 `--no-optional-locks`，沿用禁止仓库自定义 clean/process/filter/fsmonitor 执行的安全配置。真实 index 不被改写。

源代码：`src/tool_runtime/changes.rs`、`src/tool_runtime/git_review_snapshot.rs`。Final Changes 的普通 tree 冻结路径不请求额外 status 指纹。

### 2.2 review_changes 的 next_call 不符合自身输入 schema

原先初始页和续页都会把未提供的 `session_id`、`paths` 和 paging 参数写成 JSON null。发布 schema 中这些字段是 optional，而不是 nullable。能被 Rust Option 反序列化，并不等于能够通过 Host 的 JSON Schema 检查。

修复集中生成 next_call，省略缺省字段，保留显式的 0、空数组、Session、scope 和 continuation。没有放宽 schema，也没有改变 opaque token 的含义。测试实际用注册的 input_schema 验证第一页和第二页生成的 next_call，并连续读取三页。

源代码：`src/tool_runtime/review_changes.rs`。

### 2.3 续页读取过程中发生变化也必须拒绝过期结果

原续页只在读取 diff 之前检查 workspace 身份。本次增加读取后的同一身份检查；当 Runner 已生成 diff、但返回期间分支发生改变时，结果为 `snapshot_stale`，不把旧页当成当前事实返回。

这是正确性取舍：workspace 续页多一次 source observation；committed-range 路径没有这个额外检查。没有通过取消 freshness fence 换取表面上的低延迟。后续若要减少这次观察，应从同一不可变快照派生 metadata 和 diff，而不是简单删掉检查。

### 2.4 验证证据

先只加入测试、保持原生产代码不变：3 个回归测试全部失败，分别证明 index-only、branch-only 和 nullable next_call 问题。初步修复后同 3 个测试通过。

最终新增 6 项针对性测试，覆盖 index-only、相同 MM 不同 staged blob、真实 index 不变、同 HEAD 切分支、显式 optional 值、三页 schema-valid continuation、读取期间切分支，以及 1,024 个长文件名的 unborn 大状态场景。部分场景在同一测试内断言。

最终 Rust 修改和一次格式化之后：

- `cargo test -p webcodex --lib review_`：59 passed，0 failed。
- `cargo test -p webcodex --lib changes`：132 passed，0 failed。两组有交集，不能相加声称 191 个不同测试。
- 两组都未运行 ignored tests；没有运行全库、全 workspace 或部署 E2E。
- 测试保留基线已有的 `poll_start_validation_job_with_timeout` dead-code warning，没有为消除警告扩大修改范围。

## 3. 最近 Codex session 的实际观察

样本为 `/root/.codex/sessions/2026/09/23` 至 `27` 中 10 份 rollout，cwd 是主仓库或 `webcodex-review`。解析同时覆盖 custom/function tool call 及其 output，用 call_id 配对，不读取或发布对话正文。

| 指标 | 实际观察 |
| --- | ---: |
| exec 调用及成功配对的 output | 1,396 |
| 全部工具 output 记录 | 1,407 |
| output 内容 UTF-8 字节合计 | 9,067,572 |
| exec 调用记录到对应 output 的 p50 | 0.238 秒 |
| 同一口径 p95 | 5.012 秒 |
| 同一口径最大值 | 12.233 秒 |

这里的 exec 延迟是 cell 的一次返回/让出响应时间，不是子进程完整执行时长，更不是纯 VM CPU 时间。日志输出字节也不是 token 数或线上传输字节。

源代码文本中出现 Promise.all 系列的 cell 为 72 个，出现 `text(JSON.stringify` 字面模式的 cell 为 39 个。这只是代码文本特征，不能当成实际并行执行率；嵌入的 shell/Python/Rust 文本也会污染简单正则统计。因而本报告不使用宽泛的“多调用正则命中率”作为性能结论。

这些 session 主要体现 Codex 本地工具编排。不能把它们与 sf 上的 WebCodex 请求强行一一对应，也不能据此认定 sf 上没有 pending Job。

## 4. sf 现有数据：输出与观察密度比 VM 微优化更值得优先检查

只读查询 `/var/lib/webcodex/webcodex.db` 的现有 ActionAudit。统一时间范围：`2026-09-23 00:00:00 UTC <= started_at < 2026-09-27 09:30:00 UTC`，截止时刻为首尔时间 9 月 27 日 18:30；不包含本次审查产生的新请求。没有导出原始请求 payload，也没有新增生产 telemetry。

### 4.1 exact project 范围

筛选 `project = 'agent:special:webcodex'`：

| 指标 | 实际观察 |
| --- | ---: |
| ActionAudit 事件 | 5,441 |
| Work Result state 请求 | 2,047，约 37.6% |
| 有 serialized_result_bytes 的事件 | 3,393 |
| 已测量结果字节合计 | 30,424,670 |
| read_files + search_project_texts + search_and_read | 21,508,598 字节，约占已测量结果的 70.7% |

读取/搜索是结果体的主要来源。`search_project_texts` 为 401 次、平均约 23.1 KB；`read_files` 为 473 次、平均约 19.2 KB；`search_and_read` 为 113 次、平均约 27.9 KB。这里的 KB 为十进制近似。

顶层 `response_bytes` 全部为空，但嵌套 `model_ergonomics.serialized_result_bytes` 可用；不能因此说没有任何结果大小数据。该字段仍不等于 Host 最终写入模型上下文的字节数。

Work Result state 的累计 request duration 为 1,301,954 ms。它不是模型 turn 数，也不是 CPU 时间；累加的请求耗时包含等待及并发，不能直接作为任务墙钟耗时或节省潜力。

### 4.2 两个活跃 Window 的 60/90 分钟样本

只保留 `window_continuity_eligible=1` 的请求；在每个窗口中，以现有请求开始时间为候选，选择完全落入 60/90 分钟区间且请求数量最多的一段。这是按吞吐量挑选的描述性样本，不是随机试验，也不代表一次自然、无中断的用户 turn。

| Window 前缀 | 窗口长度 | 开始时间 UTC（9/27） | 请求数 | observe_jobs |
| --- | ---: | --- | ---: | ---: |
| 792b34ca5862 | 60 分钟 | 04:49:18 | 330 | 91（27.6%） |
| 792b34ca5862 | 90 分钟 | 04:48:00 | 461 | 140（30.4%） |
| d41d327207ba | 60 分钟 | 02:45:41 | 215 | 103（47.9%） |
| d41d327207ba | 90 分钟 | 02:23:55 | 293 | 115（39.2%） |

Window 范围可以包含 projectless Job observation 和其他项目，不应与上一节 exact-project 分母混用。已有 `job_convergence` 记录也表明，read/edit/test 等普通调用确实收到过 passive terminal delivery；问题不是完全缺少该能力。

观察密度提示应检查“不变的状态查询、已有被动结果仍继续观察、短等待切片后的机械重查”。不过当前聚合没有证明每次 observe 都无用，也没有测得这些 PR 的因果收益。

### 4.3 时间解释的边界

请求之间的 gap 只能称为 WebCodex observed timing。跨小时/跨日窗口会混入用户暂停、重新交互、Host 调度、网络、服务重启和其他活动。成功响应后的长空档不能单独证明模型在思考、Host 卡死或网络有问题。不同 build/profile、不同任务的历史窗口也不能直接作为 A/B。

## 5. Codex 参考代码与责任边界

读取 `/root/git/codex` 的参考提交 `8f195c93d`（2026-09-27 05:41:49 UTC），没有修改该仓库。

`codex-rs/core/src/tools/code_mode/execute_spec.rs` 将已启用/延迟加载的工具及 schema 交给 Host exec。`code_mode/delegate.rs` 展示 cell dispatch gate、取消 token、经既有 ToolCallRuntime 提交子调用，以及带输出预算的 notify。代码支持的方向是复用普通工具边界做编排，而不是另开不受约束的 fs/shell/network 通路。

WebCodex 的 `src/mcp_host.rs` 已有 host_code_mode profile：默认 Host budget 55 秒，初始 handoff、同步切片、continuation wait 为 5 秒。#692/#696 也已合并 pending 后继续独立工作的指导。这些不是下一轮需要重新实现的新功能。请求明确的 guidance 选择与已配置的 Host profile 要按现有优先级处理；guidance 本身不是权限或能力证明。

建议的责任分工：

- Host：短生命周期的确定性编排、已有 deferred discovery、独立只读 fan-out、选择给模型的证据、cell 取消和返回。
- WebCodex：canonical tool schema、真实 effect/authority、Project/Session/revision 校验、durable Job 和准确 continuation、完整性与截断语义。
- Runner：现有执行能力，不增加第二套 Code Mode VM/协议。

## 6. 建议按三个小 PR 继续，不做执行架构重写

### A. 让返回的下一步真正可执行

以本次修复为起点，定向检查当前高频流程的 next_call/recovery。用现有注册 schema 验证实际生成的参数，而不是只验证 Rust 能否反序列化。将缺省字段、canonical 参数和默认值的机械归一化留在服务端；身份、权限、歧义、stale revision 和 outcome_unknown 仍然严格拒绝。

每个决策只保留一个明确的主要下一步。普通 paging next_call 与 pending Job 的 observation fallback 不能混为同一种自动递归指令。不要做一个见到 next_call 就无限执行的通用循环。

### B. 将已有 Host hints 变成可回放的短链路

目标是减少没有语义必要的模型返回，而不是继续扩写提示词或再造批量工具。优先使用已有 `read_files.items`、`search_and_read.queries`、`edit_project_files.changes`、`cargo_check.packages`。

目标和修改已经确定时，可在同一个 Host cell 内读取准确 revision、执行 guarded edit，并把结构化验证提交到现有生命周期。存在设计选择、搜索歧义或新权限需求时，返回模型决策，不猜测补丁。

独立只读的跨工具调用可以有限 fan-out，并分别处理失败；相同文件的修改保持有序，Cargo validation 保持串行。native batch 优先于多个独立请求。不要修改 ToolCompositionPolicy 或 nested admission 来实现 Host 指导。

Job 首次返回 pending 后保存同一个 Job identity，继续不依赖该结果的工作。普通响应已提供 terminal attention 时不再重复观察。没有独立工作且终态确实成为硬依赖时，优先只注册一次 `wait_for_job_terminal`（仅当存在真实 Host carrier），随后 yield/end 当前 turn，等待 Host continuation；不要把 5 秒 `observe_jobs` 切片改写成同一 cell 内的轮询循环，也不要每个切片都返回模型再重新进入 Code Mode。只有需要日志、诊断或恢复时才显式 `observe_jobs`。不得重新执行命令冒充 continuation。

cell 内保留完整 ToolResult、revision、cursor 和恢复数据；给模型输出决策所需证据。压缩时必须保留失败、截断、缺失和待确认状态，不能用只剩 success=true 的摘要掩盖不完整结果。

### C. 用现有数据验证收益，再决定是否需要新观测能力

默认不增加 telemetry 表、持续扫描或生产采样负担。先离线复用上述 ActionAudit 字段、Job convergence、现有 Host session 和 source/build 信息。缺少可靠关联时保留未知，不把 server gap 改名为 model turns。

对照任务至少包括：独立多文件读取、确定性 search/read 后的 guarded edit、长测试 Job handoff、三页 Git review、取消/结果未知恢复。长 Job 场景应单独比较 `pending → 独立 DAG → passive attention`、`pending → 一次 wait_for_job_terminal → Host continuation` 与错误基线 `pending → 多次 observe_jobs/多次模型重入`；不要把同一 cell 的 observe 轮询当成目标架构。固定源代码、Host/profile、任务、缓存条件和运行次序；不要把冷 Cargo 编译与热缓存任务直接比较。

评价优先级：任务正确完成和安全边界不退化，其次是端到端耗时、实际可观察的模型决策轮次、结果输出体积、有效与无变化的 observation。继续观察 60/90 分钟工作窗口，但必须标注用户继续、暂停和版本切换，不用窗口跨度替代单次不中断执行。

Work Result state 请求单独评估：检查现有可见性、后台暂停和状态变化后的刷新策略，再判断是否需要额外 backoff。不要把 App 刷新请求计作模型 turn，也不要假定 #712 等已部署到所有历史样本。

## 7. 可复核的数据查询

下面查询只返回聚合，不返回原始内容。使用只读 SQLite 连接；实时库可能持续新增，必须保留相同截止时间。

```sql
SELECT operation, count(*) AS calls,
       count(json_extract(summary_json,
         '$.model_ergonomics.serialized_result_bytes')) AS measured_calls,
       sum(json_extract(summary_json,
         '$.model_ergonomics.serialized_result_bytes')) AS result_bytes
FROM action_events
WHERE started_at >= strftime('%s', '2026-09-23')
  AND started_at < strftime('%s', '2026-09-27 09:30:00')
  AND project = 'agent:special:webcodex'
GROUP BY operation
ORDER BY result_bytes DESC;
```

Codex session 统计的配对规则：只对 name=exec 的 custom_tool_call/function_call 建立 call_id 到 timestamp 的映射，再用 custom_tool_call_output/function_call_output 的同一 call_id 配对。延迟使用两条记录的 UTC timestamp 差，字节使用 output 内容的 UTF-8 长度；两者均不推导 token 数或完整任务时长。

## 8. 未做的工作

没有对 517 个文件逐一审计，没有做可归因的 Host A/B，也没有定位所有长 gap 的根因。该审查报告生成时没有修改 `/root/git/codex`、sf 数据或服务配置，也尚未 push、创建 PR、部署或重启。后续独立 review/交付动作不改变上述审查时点。该分支提供已测试的正确性修复和可实施的下一轮设计，不宣称已经取得线上性能提升。
