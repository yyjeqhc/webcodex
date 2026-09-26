# WebPi 资源模型与架构质量探索

状态：探索性评审，不是新的运行时契约。本文会随真实 dogfood 和产品使用证据修订，而不是反过来要求实现服从路线图。2026-09-13 复审后，后续资源路线从 user-private / Memory-first 收缩为 usage-driven 的 Skill / Plugin-first；现有 Memory 保留但冻结扩张，Runner 继续承担执行与资源 placement，而不再作为面向用户的资源 scope。

评审日期：2026-09-12。精确源码基线：`3d8332190c336fc7becaa0ae65700e0fd08cb43d`。分析在 special 主项目的独立 managed worktree 中进行，分支为 `explore/architecture-resource-boundaries`。下文源码行号指这一基线；符号名用于后续定位。

## 1. 总体判断与范围

WebPi 不缺架构：Server/Runner 分离、分层 workspace、统一 ToolDefinition、RunnerOperation、事务式 Memory、精确 Plugin binding、独立 Workflow Session/Job/AgentTask 都已经存在。继续增加一层通用框架，未必比收拢现有概念更好。

当前最值得投入的是：**统一观察与描述，收敛规则所有权，保留业务状态机，降低模型决策成本。** 目标应是以后新增一种资源或一种工具时，更少触碰不相关模块，而不是让所有对象实现同一个 CRUD 接口。

新增一条优先级约束：**真实使用证据优先于架构完整性。** Skill 已被实际使用，Native Plugin 已完成多轮 dogfood 且是产品扩展能力的重点，因此它们应优先验证资源抽象；Project Memory 虽已有完整 CAS / revision / scope 实现，但尚缺真实使用反馈，不应仅因为已有 foundation 就继续派生 repository read-through、user-private namespace、sharing、ACL 或 migration。

本轮按边界抽样追踪了 workspace 策略、工具契约/Kernel/MCP、Memory/Skill/Plugin、启动与上下文投影、执行预算、存储、卡片投影及相关测试。没有逐行审计所有代码，没有验证所有前端页面、Windows/macOS、真实断网/重启或部署场景，也没有做全仓性能画像。未发现并复现可在此直接定级为安全漏洞的问题；下面的维护风险、性能假设与功能提议分别标注。

## 2. 应保留的结构，而不是推倒重写

| 已有结构 | 源码证据 | 判断 |
|---|---|---|
| 17 个 workspace package 的依赖策略 | [workspace-boundaries.toml](../../workspace-boundaries.toml)、[检查器](../../scripts/workspace_boundary_check.py)，本轮实际检查通过 | 分层已有机器约束，不需要再发明一套目录约定替代它 |
| 统一工具定义 | [ToolDefinition](../../crates/webcodex-tool-contracts/src/tool_definition.rs)，698–712 | 审计、策略、模型暴露、Session evidence 已有合理归属，应沿此继续收敛 |
| 线协议与内部语义分离 | [RunnerOperation](../../crates/webcodex-core/src/runner_operation.rs)，1–6、30–44 | 在边界解码旧 DTO，内部携带类型化操作；比重写全部 wire protocol 更稳健 |
| 描述与执行权限分离 | [PluginBinding/PluginOperation](../../src/plugin_gateway.rs)，27–34、135–187 | describe 不是执行授权，metadata hint 不是权限；必须保留 |
| Memory 事务与条件更新 | [set_project_memory_attributed](../../crates/webcodex-store/src/memory.rs)，962–1054 | 内容相同的幂等重试、CAS、实例/代际身份不能简化为一个内容哈希 |
| 语义不同的任务对象独立 | [Durable Agent 设计](durable-agent-runtime.md)、[Session 模型](../agent/session-model.md) | AgentTask、Workflow Session、Job、ClientWindow 不是四种同名 Task |
| 有界观察而非完整转储 | [context_projection](../../src/tool_runtime/context_projection.rs)、[startup_brief](../../src/tool_runtime/startup_brief.rs) | 模型上下文是受预算约束的投影，不能当完整数据库或权限凭证 |

建议继续采用模块化单体与明确 Runner 执行边界。当前没有测量证据支持为了代码量而拆微服务；那会把本地调用复杂度换成部署、网络失败、分布式事务和版本协调复杂度。

## 3. Memory / Skill / Plugin 可以统一成什么

### 3.1 产品 scope、source 与 placement 分开，但不提前发明多租户资源 ACL

最初的 `runner 级 / project 级 / 用户自定义级` 需求可以收敛成更简单的产品模型：

| 维度 | 回答的问题 | 当前建议 |
|---|---|---|
| Kind | 这是什么？ | Memory、Skill、Plugin；以后确有需求再加入其它 kind |
| Product scope | 用户如何理解它属于哪里？ | `user` 或 `project` |
| Source / lifecycle | 内容或配置从哪里来、怎样变化？ | repository、configured root、managed package、Runner config、Control DB |
| Placement / applicability | 它在哪里运行、在哪个 Project 适用？ | Runner placement、exact Project cwd、Control storage；不是新的 scope |
| Domain operation | 怎样读取、绑定或执行？ | Skill read、Memory CAS、Plugin describe/call 各自保留 |

这里的 `user` 是产品层“当前 WebPi 用户可用的个人资源”概念，不等价于新建一个 `AuthContext.user_id` 私有安全域。对当前 self-hosted 模式，能够操作某个 Runner 已经代表对该执行宿主的用户权限；资源抽象不再额外引入 principal namespace、resource ACL、sharing grant 或 admin enumeration 体系。现有 authentication、Runner access、tool scope、Project authority 与 Plugin exact binding 继续按各自边界执行。

Runner 因此从面向用户的资源 scope 降为 **placement / provider host**。例如 configured / managed Skill 可以在产品层解释为 User Skill 的两种 source；Native Plugin 是由 Runner 托管的 User Plugin provider，再通过 exact cwd 等事实决定 Project applicability；Project Skill 仍直接来自 repository。Project 与 User 同名时是否默认优先、并存或 fail closed 属于具体资源的选择语义，不由一个通用 inheritance/ACL 框架决定。

### 3.2 当前支持情况，不把提议误写成现状

| 资源 | 当前存放与作用域 | 读取/使用方式 | 生命周期 |
|---|---|---|---|
| Project Memory | Product scope=`project`；Control DB 持久化，scope identity 仍由现有 Project runtime/Runner/root facts 派生 | memory_search / memory_read；读取继续受现有 project 与 Memory scopes 约束 | 现有实现保留；在缺少真实使用反馈前冻结新的 read-through / user scope / sharing 扩张 |
| Project Skill | Product scope=`project`；source=`repository`，项目 `.agents/skills` | skill_list / skill_read_file；正文需显式读取 | 仓库活文件，definition revision |
| Runner configured Skill | 产品上逐步解释为 scope=`user`、source=`configured`、placement=`runner`；当前 wire / descriptor 名称暂不要求兼容性重写 | 同一 Skill 发现入口，Runner 解析 opaque id | 活目录；不等于已安装不可变包 |
| Runner managed Skill | 产品上逐步解释为 scope=`user`、source=`managed`、placement=`runner` | 同一发现入口；安装/版本/激活/删除由管理操作负责 | 包 revision、active pointer、state revision、幂等记录 |
| Native Plugin | scope=`user`、placement=`runner` 的 executable provider；当前 Project applicability 由 provider cwd 与 exact Project root 匹配产生 | plugin_tool list / describe / call；binding 固定 exact Runner/provider instance 与 schema | 进程实例、冻结 catalog、check/reload、结果不确定性；现有 execution authority 不由资源目录替代 |
| User-private ACL / sharing namespace | 当前无真实产品需求，近期不实现 | 不新增 `wc_userns_*`、resource ACL、share grant 或跨用户枚举 | 若未来出现真实多用户/SaaS需求，再以独立产品问题重新设计 |

证据：[Memory scope](../../src/tool_runtime/memory.rs)，38–56；[Skill descriptor/locator](../../src/tool_runtime/skills.rs)，61–94、562–638；[Skill store](../../crates/webcodex-core/src/skill_store.rs)，35–103；[Plugin project catalog](../../crates/webcodex-runner/src/webcodex_runner/plugin.rs)，392–490。

### 3.3 推荐的共同边界是只读目录，不是万能 ResourceManager

概念结构如下，仅表示设计方向，不是新公开 API：

```text
Memory provider    Skill providers    Plugin provider
       \                |                 /
        existing access checks + domain observations
                          |
           bounded metadata projection / catalog
                          |
         id + kind + scope + source + placement/applicability
         revision + completeness + diagnostics + next read/describe
                          |
           domain-specific read / describe / mutation
```

共同描述层可以负责：身份与来源说明、可见性原因、描述长度与预算、空/不可用/不完整状态、分页、稳定顺序、下一步入口。它不能负责启动 Plugin、修改 Memory、自动执行 Skill、生成执行授权，或为所有领域定义相同的 revision。

现有 [ContextMaterialSpec](../../src/tool_runtime/context_projection.rs)，19–80，已经是一个合适的起点：材料 key、project required、scope policy、surface。不要在旁边另建一套功能重叠的总注册表。先让现有启动目录和 context sidecar 共用小型投影原语；在确有第三个消费者时再决定是否提炼 provider 接口。

### 3.4 三阶段模型

1. **Discover**：拿到小目录与可选操作，尚未读取正文或获得执行能力。
2. **Materialize / bind**：Memory/Skill 读取正文；Plugin describe 固定 schema 与实例。二者不能伪装成相同效果。
3. **Act**：使用现有具体工具更新 Memory、安装 Skill 或调用 Plugin；每次仍经原有权限与执行链路。

这也解释了为什么 Skill 不是 Plugin 的子类：Skill 是交给模型的指导，Plugin 是可执行 provider。统一“它们可发现”不意味着统一“它们可运行”。

## 4. 优先改进项与源码证据

### A. 收敛扩展家族分类的所有权（后续 consolidation 已落地）

最初探索时，Skill/Memory 的 runtime/management 分类分别由实现模块中的 `is_*_tool_name` helpers 持有，Kernel、发现 surface、MCP adapter 与 registry 又各自消费或重复维护这些名字集合。这不是已证明的权限漏洞，而是“新增工具时容易改漏一处”的结构风险。

后续 consolidation 已将这一个静态事实收敛到 canonical `ToolDefinition`：`ToolOperatorExtensionFamily` 只描述 `SkillRuntime / SkillManagement / MemoryRuntime / MemoryManagement / TraceDiagnostics` 的 Stateless Operator protocol admission family，各定义在 Skill、Memory、diagnostic 的 ToolDefinition 旁显式声明。Registry、Kernel capability gate、tool manifest 与 MCP Stateless protocol-extension projection 统一通过 `runtime_tool_operator_extension_family` 消费；旧 Skill/Memory classifier 和 registry 中对应的硬编码 family name sets 已从 live code 删除。

证据：[tool_definition.rs](../../crates/webcodex-tool-contracts/src/tool_definition.rs)、[tool_policy.rs](../../crates/webcodex-tool-contracts/src/tool_policy.rs)、[tool_specs.rs](../../crates/webcodex-tool-contracts/src/registry/tool_specs.rs)、[kernel.rs](../../src/tool_runtime/kernel.rs)、[surface.rs](../../src/tool_runtime/surface.rs)、[mcp/tools.rs](../../src/mcp/tools.rs)。

这个收敛没有把 authorization 混入 family：Skill Management 的 admin 要求、Memory conjunctive scopes、Project/Runner authority、permission 与 Session evidence 仍由原 canonical policy 所有；Goal Plan、Work Result、Agent Continuation 的 MCP App/Host capability 也保持独立。Definition invariant 还要求 operator-extension family 必须保持 `ModelHidden`，避免普通 model-visible 工具因误标 family 同时进入通用 registry 与 extension projection。

### B. Skill 目录观察与精确读取（P1 风险已进入 observer/resolver split）

#420 建立 baseline 时，`discover_project_skills` 会逐个读取包定义，`discover_skills` 依次观察 Project、configured、managed 来源，而 `skill_read_file` 也先重建完整目录再按 id 定位。Stage 2 后续实现已把 known-id read 从这个完整 catalog observer 中分离；`skill_list`、startup/context catalog 仍保留完整 `discover_skills` 语义。证据：[skills.rs](../../src/tool_runtime/skills.rs)、[configured_skills.rs](../../crates/webcodex-runner/src/webcodex_runner/configured_skills.rs)。

注意：启动层的 Skill 和 Plugin 已经并行等待，见 [coding_task.rs](../../src/tool_runtime/coding_task.rs)，1232–1269；不能把它描述为整个启动串行。

可行顺序：先按来源记录扫描次数、条目数、耗时、字节量和不可用原因；再分离目录观察与已知引用解析；最后才考虑有界并发或缓存。缓存键至少绑定 Project authority、Runner 实例、source/配置版本及适用 revision。缓存 descriptor 不是缓存授权，已移除资源不能被缓存复活；不通过猜 opaque id 格式路由。

将来源失败降级为 partial catalog 会改变外部语义，应单独评审并显式报告 incomplete；本轮没有把失败偷偷转换成成功空目录。

#### B.1 2026-09-13 follow-up：Skill source fanout 基线与观测边界

#420/#421 用现有 fake/local Runner recorder 固定了优化前后的 request fanout。Project authority 仍独立：空 Project catalog 的 `skill_list` 是 1 次 `file_skill_list_packages`；若 Project 有 `N` 个 Skill，完整 Project catalog 仍是 `1` 次 package list + `N` 次 `SKILL.md` definition read。known-id Project exact resolver 则只枚举 package identities并读取 target definition；普通 Project resource read继续在 resource I/O 后 recheck definition revision。lexical invalid resource path仍在任何 Runner request前 fail closed，因此保持 0 request。

Runner-local 侧在 Stage 2 前有两套跨进程 read family：configured `List/Read` 与 managed `ListActive/Read`。完整 catalog 的跨 Runner sequence 是 `Project catalog -> Configured List -> Managed ListActive`；mixed Runner target exact read是 `Project package list -> Configured Read probe -> Managed Read probe -> target Read`，即 4 个跨 Runner requests。这个 baseline说明真实 storage边界虽然不同，但 cross-process ownership 被拆得比领域边界更碎。

Server 侧的 fail-open、低基数 observation 继续保留，但 Stage 2 后固定 source 收敛为 `project / runner_local`；operation仍是 `catalog_list / catalog_definition_read / exact_resolve / resource_read / definition_recheck`。一个 canonical Runner request只计为一个 `runner_local` request，不再假装同时是 configured 与 managed 两个 Server source requests。每个已发出的请求仍可观察 count、Server elapsed、Runner已有 `duration_ms`、response bytes、catalog item count（仅当前层有 authoritative count时）和 bounded outcome class；没有 Project/Skill/package/root/resource/query/error body/content/绝对路径/principal label，也没有为内部 scan count新增 telemetry wire。

Runner-local structured tracing继续分别描述真实内部 source work：configured identity/definition scan记录 roots、directory entries、definition attempts/reads/bytes、valid/invalid/truncated/elapsed；managed exact resolution记录 `skill_keys_listed`、`skill_keys_examined`、hit与 elapsed。managed opaque-id resolve仍通过 `list_skill_keys()` 做 O(number of keys) scan；Skill数量预计很小，本阶段明确不做 reverse index/cache。Runner scan counter仍不会被塞进业务 response，因此 Server metrics描述 cross-process请求，Runner tracing描述 source-local工作。

#### B.2 Stage 2：一个 Runner Skill boundary，observer 与 exact resolver 继续分离

Runner-local Configured 与 Managed 现在通过一个 canonical `RunnerSkillRequest` / `RunnerOperation::Skill` / wire kind `skill` 暴露，但**没有合并 storage object**。Configured仍由 trusted `[skills].roots`、root canonicalization、symlink/reparse protection、bounded package enumeration与live file reads负责；Managed store仍独立持有 install archive、active revision state、state revision、replay/idempotency、activate/remove lifecycle与 immutable package read。统一的是 orchestration/protocol boundary，不是 filesystem/store implementation。

canonical Runtime surface是 `List / Resolve / Read`，Management surface是 `Versions / Install / Activate / RemoveRevision`；Runner capabilities也只剩独立的 `skill_runtime` 与 `skill_management`。`Versions`仍属于 Management，runtime capability不会授予 install/activate/remove/version inventory。Registry只保留一个 typed Skill enqueue error与 `SkillDispatchFence { runner_instance_id, management }`；dequeue时仍在锁内重新校验 exact Runner instance与对应 capability。management mutation的 waiter/timeout语义仍保留 dispatched-aware `outcome_unknown`，没有因协议收敛而变成安全 retry。

`RunnerSkillDescriptor` 是 source-tagged closed enum：Configured只携带 `skill_id/name/description/definition_revision`；Managed必须携带 `skill_key/package_revision`以及同样的 identity/definition metadata。Server只根据显式 source tag投影既有 `source_scope=runner` 与 `trust=operator_configured_guidance/operator_installed_guidance`，不通过 nullable字段猜 source。Runner `List` 同时观察 configured live roots与 managed active store；同一 opaque `skill_id`若在两个 Runner-local source同时有效则 fail closed，不做 priority/shadowing。configured diagnostics、invalid count与 discovery truncation进入 unified List；managed没有对应事实时不伪造。

完整 catalog现在是 `Project catalog -> Runner Skill List`：相较旧路径少一次 Runner-local cross-process request，并把 configured/managed union ownership放回 Runner。known-id exact read现在是 `Project package list -> Runner Skill Resolve -> Runner Skill Read`；mixed configured/managed target从旧 4 requests收敛为 3，而且 Server不再分别 probe两个 Runner-local source。若 Runner不支持 `skill_runtime`，Project-only target仍可工作；若 capability适用但 Runner无法可靠回答 Resolve，则仍以 `skill_catalog_unavailable` fail closed。Project与Runner都命中同一 id时同样 fail closed，不通过 source priority解决 collision。

`Resolve { skill_id }` 不构建完整 Runner catalog。Configured复用 identity-first resolver：枚举 bounded package identities、计算既有 opaque id，只对 target match读取definition，同时继续扫描其它 identity以发现 ambiguity。Managed复用现有 O(N) key scan。0 个有效 Runner-local candidate是 absent，1个返回source-tagged descriptor，2个或source uncertainty均fail closed。Project exact resolver仍独立复用 package-name enumeration，并只读取匹配Project package的definition。

统一协议后 `Read` 显式携带 `expected_source`。Runner actual Read前后都重新确认 target仍唯一且source未改变，因此 `Resolve -> Configured -> race -> Managed Read`、target消失或新增跨source collision都不会静默成功；source identity race以内部稳定错误收敛到既有model-facing fail-closed catalog语义。Configured Resolve观察到的 definition revision继续作为 actual Read的隐式 expected definition revision，Resolve→Read变化返回 `skill_definition_changed`。Managed Resolve观察到的 package/definition revision只证明membership/source identity，**不会**升级为用户未要求的CAS；managed actual Read仍只应用用户显式 `expected_package_revision/expected_definition_revision`。Project resource read继续保留definition recheck与sensitive-path/byte/range边界。

本阶段明确没有 cache、persistent reverse index、数据库表、background refresh、source priority/shadowing或 partial catalog success。旧 `ConfiguredSkillRootsRequest` family、旧 read-side `SkillStoreRequest::{ListActive,Read}`、旧两个wire kind、三个旧capability与compatibility-only enqueue wrappers/tests均已删除；不维持尚未形成真实用户负担的Runner/Server交叉版本适配层。

### C. 区分仓库知识身份与执行 worktree 身份（Stage 4A1 foundation 已建立）

当前隔离继续保留：managed worktree 仍是独立 execution Project，自己的 `ProjectConfig.path`、file/patch authority、Session Project guard、Plugin cwd/binding、LSP root、artifact root、Runner operation target 与 Job/process authority 都指向 target worktree。Repository knowledge association 是另一条只读描述关系，不会改写这些 execution facts；现有 `repository_identity` 仍由 target execution root 计算，没有被偷偷改成 source repository identity。

Stage 4A1 把关联的 authoritative owner 放在 Runner project registry，而不是 Server DB。原因是 managed-worktree 的 `managed_source`、`managed_base_sha`、managed lifecycle 与 Project root canonicalization 本来就由 owning Runner 创建并持久化；Runner 也是唯一能在创建时用自己的 canonical registry 精确回答“这个 source root 当前对应哪个 Runner Project”的组件。Server 只消费 `RunnerProjectSummary.lineage` 的 typed projection，不再建立第二份 lineage truth，也没有新增 SQLite table、migration、background reconciler 或 cache。

canonical lineage 是 closed `RunnerProjectLineage::ManagedWorktreeSource`。新 managed record 除原有 `managed_source` 与 `managed_base_sha` 外，还持久化 `managed_source_project_id` 与 `managed_source_root_fingerprint`；只有这两个新字段同时存在时才产生 knowledge association。source Project identity 是 **exact Runner Project id + independent root fingerprint** 的组合：project id 防止 path-only 猜测，root fingerprint 防止 project id 被 unregister 后重新注册到另一个 root 时 silent retarget。fingerprint 使用独立 domain `webcodex-project-root-identity-v1` 与 `wc_projroot_` 前缀，并复用 Runner config 已有 `normalize_path_identity` 的平台 path semantics；它不是 Memory scope fingerprint 类型，也不会进入 model-facing metadata。

新 managed worktree 创建要求 source checkout 已经在同一 Runner registry 中以唯一、enabled Project 存在；Git remote、仓库名、basename、Git config、`registration_source` 或“看起来像 worktree”的路径都不会建立 association。带显式 lineage 的 resume 重新解析同一 source root，并验证 persisted source Project id 与 root fingerprint 都仍相同；任一变化都 fail closed。旧 managed record若只有 `managed_source` 而没有新 authoritative pair，仍可按原 execution 语义存在/恢复，但 `lineage=None`，不会通过 heuristic 自动升级。

Server `ResolvedProject` 只增加独立 `knowledge_association` descriptor；source path绝不塞入 `ProjectConfig`。canonical resolver `resolve_project_knowledge_source_for_auth` 每次使用都重新观察 target owning Runner、要求当前 project inventory `complete`、按 exact source id 取 source、要求 source enabled且 current root fingerprint 与 persisted identity 相同，然后再次走普通 `resolve_project_input_for_auth`。association 不是 capability token，也不缓存一次成功的 authorization。当前 Project authority主要由 Runner/project visibility表达；如果以后增加更细粒度 Project ACL，这个重复走 canonical source resolution 的边界会自然继承它，而不是引入 `knowledge_read_scope` 绕开现有权限。

resolution 语义刻意把“association存在”与“source当前可用”分开：普通 Project是 `NotAssociated`；Runner offline、inventory incomplete、source missing/disabled、source root identity不可证明或 caller无当前 source authority都是 `Unavailable`；source id相同但root fingerprint改变是 `Stale`；只有全部 current checks通过才是 `Available`。`Unavailable/Stale` 只使 source knowledge unavailable，不会让 target Project 本身停止普通 coding、改变 cwd 或丢失自己的权限。诊断层在有 association 时才投影；available 可显示 caller已经有权看到的 source runtime Project id和创建时 `base_sha`，unavailable/unauthorized 不显示 source id，所有状态都不输出 absolute source path、root fingerprint、managed operation id，且明确 `read_through=false`。

`managed_base_sha` 只证明“这个 managed worktree 最初从 source repository 的哪个 Git commit 创建”。它不证明 Memory snapshot、Skill catalog/content revision、Plugin revision 或统一 Knowledge revision。Memory 是独立 durable mutable state，Project Skill 当前是 live filesystem knowledge；真正 snapshot若未来需要，必须分别使用各领域自己的 catalog/content revision 或 Git object read，不能把 base SHA提升成跨资源 authority/version。

Stage 4A1 因此保留为 **managed-worktree source lineage foundation**：它解决 source checkout 身份不会在 worktree 创建后丢失的问题，但不再自动生成一个必须完成的“knowledge inheritance”路线。现阶段不推进 Memory read-through；只有 managed-worktree 中复用 source Project Skill 出现真实使用需求时，才单独评估 Project Skill discovery/read，并继续保留 definition revision、exact identity 与冲突语义。Runner-hosted User Skill 不因 repository association重复继承；Plugin具有 executable process / cwd / binding 语义，继续明确不从 source association继承。

### D. ToolRuntime 是逐渐膨胀的组合对象（P2，维护风险）

[ToolRuntime](../../src/tool_runtime/runtime.rs)，106–177，持有多个 gateway、Session、执行预算、reconciliation 锁、观察组件和数个数据库注入字段。它作为 composition root 合理，但所有领域方法都依赖整个对象，会扩大可触达状态和测试装配面。

优先让一个具体用例依赖小而明确的服务集合，例如 SkillCatalogService 只借用所需 Runner/catalog 端口，而不是立刻拆成很多新 crate 或制造动态 ServiceLocator。先抽纯投影与解析，再处理有状态服务；明确锁、缓存、超时由谁持有。

### E. 存储访问需要测量隔离，不宜先换数据库（Stage 3A 已建立基线）

[Database](../../crates/webcodex-store/src/lib.rs) 仍由单个 `Mutex<Connection>` 持有 SQLite connection，因此不同 store domain 在结构上可能互相排队；这仍然只是 bottleneck 假设，不是优化依据。Stage 3A 没有改连接数、线程模型、WAL、事务、CAS、replay 或 schema，而是把 production connection acquisition 收敛到一个 store-local observation boundary。

该边界只暴露三个稳定 measurement name：`store_connection_acquisitions_total`、`store_connection_lock_wait_seconds`、`store_connection_hold_seconds`。唯一业务 label 是 closed `domain`，当前集合为 `accounts`、`activity`、`admin_project_lifecycle`、`agent_task`、`agent_wake`、`audit`、`communication`、`core`、`executions`、`goal`、`job_receipts`、`memory`、`oauth`、`schema`、`task_kernel`、`window_activity`。不记录 project/principal/account/Agent/Goal/Task/request id、路径、SQL 文本、table 动态字符串或用户数据。

`lock_wait` 定义为调用 `Mutex::lock` 前的 monotonic timestamp 到成功取得 connection guard；`hold` 定义为成功取得 connection guard 到真实 `MutexGuard<Connection>` 释放。后者是 **connection critical-section duration**，不是 SQLite statement duration：一个 guard 内可能包含多条 query、transaction、validation、CAS 检查、commit 和少量 Rust 逻辑。若某个 domain 的 hold 异常，再做 statement/transaction drill-down；Stage 3A 不包装 rusqlite API。

默认 observation 以 `webcodex_store::connection` target 的 structured trace 发出，并使用 `trace` level，避免每次 connection acquisition 在普通 `info` 日志中产生高频噪声。Server 默认 `RUST_LOG`/fallback filter 是 `info`，因此 **这些样本默认不会输出**；dogfood 采样窗口必须显式启用该 target，例如 `RUST_LOG=info,webcodex_store::connection=trace`（保留部署环境原有其它 filter 时应合并而不是覆盖）。没有该 target 的 trace event 只能说明采样未启用或没有观测到事件，不能解释为 `lock_wait=0`。真实 connection guard 在 observer callback 前先释放，因此 trace subscriber 不会扩大被测 connection critical section，也不会让其它 store caller 因 telemetry 继续等待该 mutex。observer panic 在这个小边界内 `catch_unwind`，不会被翻译成 DB/business error；原有 `Mutex::lock().unwrap()` poison panic 仍保持。启用的同步 trace subscriber 理论上仍可能在 **mutex 已释放后** 延迟当前 caller 返回，Stage 3A 不为此增加后台 telemetry worker；dogfood 采样时也应观察这一开销。

因此 Stage 3A 的结论只是“现在可以测量”，不是“数据库需要优化”。p50/p95 必须来自 dogfood 或 production-like workload，不伪造 benchmark 改善数字。Stage 3B 只有在数据支持时才进入：

| 观测证据 | 下一步 |
|---|---|
| `lock_wait` p95 约为 0，且没有持续 queueing | 不做 DB architecture optimization |
| 单一 domain 的 `hold` 明显偏高 | 先缩短该 domain critical section，或对它做第二层 statement/transaction drill-down |
| 多个 domain 同时出现明显 wait，且能与其它 domain 的 hold 对应 | 再评估 bounded blocking worker / DB actor；先定义 queue、cancel、outcome-unknown 与事务完成语义 |
| 大量重复只读工作占主要 hold/CPU，且 authority/revision fence 可证明 | 才评估有 revision/authority fence 的 cache |

同步 `Database` API 是否阻塞 Tokio worker 是 Stage 3 的另一个问题，但本阶段不迁移 `spawn_blocking`、DB actor 或 async mutex。已有 Connector/Runner 路径在其它 blocking work 上使用 `spawn_blocking` 的事实也不能替代 Store contention 数据。

### F. 表达规则和投影预算有小规模重复（P2，本轮验证一部分）

基线 `StartupSkillsCatalog` 和 `StartupPluginsCatalog` 重复了相同的状态、revision、count、truncated、entries、hint 外壳，以及逐条试装入 JSON 字节预算的算法；只有条目类型、上游完整性和文案不同。[startup_brief.rs](../../src/tool_runtime/startup_brief.rs)，139–293。

适合提炼的是启动专用的泛型目录投影，不是引入所有资源共用的存储、动态 provider trait 或统一执行方法。这是一个有两个真实消费者、可用差分测试约束的抽象。

### G. 字符串错误分类应停留在边界（Runner Skill admission 已收敛）

最初探索时，ToolRuntime 对 configured/managed Runner Skill provider分别解析文本错误；后续又短暂存在两套 typed enqueue error。Stage 2 现在把它们收敛为一个 canonical `EnqueueRunnerSkillError`，Registry按 `RunnerSkillRequest::requires_management_capability()` 选择 runtime/management capability，并在 Server边界把 typed admission结果映射回既有 model-facing Skill错误语义。内部不再通过 `error.contains(...)` 推断 capability或exact Runner变化。

旧 configured/managed enqueue wrapper和compatibility alias没有保留。这个收敛只改变内部protocol/admission shape：authorization、Project authority、Runner instance fencing、management outcome-unknown、ToolResult schema与模型可见Skill工具名称/参数仍由原边界保持。

同理，内容 revision、catalog revision、状态 CAS、Job observation token、Session message ACK 虽然都长得像字符串，不应共用“版本号”的业务语义。只在误传风险高、已有具体消费者的接口引入 typed boundary 或 newtype，不要求每个字符串都包装。

## 5. 可以类推到其他领域的设计

### 5.1 观察外壳统一，业务状态机分离

Memory record、Plugin binding、Job observation、Artifact、Workflow template 都可以有共同的“从哪里来、是否新鲜、结果是否完整、下一步怎么读取”的描述。但它们不是相同资源生命周期。

Job 的 running/recovering、Memory 的 CAS changed、Plugin 的 stale binding、Artifact 的 expiry 不宜合并成一个巨大的 ResourceStatus 枚举。可以共享小型原语，例如边界明确的 completeness、bounded page、来源引用，而状态机仍归具体领域。

### 5.2 把“是否执行过”与“是否成功”分开

Plugin 已有 `NotStarted / OutcomeUnknown / Completed`，见 [plugin.rs](../../crates/webcodex-core/src/plugin.rs)，83–89。结构化执行也区分排队、运行和 unknown，见 [structured_execution.rs](../../src/tool_runtime/structured_execution.rs)，162–182。

建议建立跨工具一致的恢复语义：未执行可调整输入；明确业务失败按失败处理；结果未知先观察原执行。共享的是这套知识模型，不是把所有操作转成 Job，也不是引入通用自动 retry。执行能力与可安全重试性不能从 readOnly/idempotent hint 推导。

### 5.3 明确四种预算

执行总超时、同步等待窗口、单次观察等待、模型输出预算是不同东西。`StructuredExecutionBudget` 已把前两者分开，见 [structured_execution.rs](../../src/tool_runtime/structured_execution.rs)，17–52。

后续工具组合应保留子执行的总预算与身份，外层只增加组合预算。截止时间一经创建，不应因分页/恢复反复重置。不要为了让卡片持续刷新而延长任务寿命，也不要让模型输出截断改变实际执行结果。

### 5.4 将模型上下文视为按需查询的投影

当前 context sidecar 的 contract 明确它只在主 effect/observation 之后投影，且不追溯授权或治理当前 effect；该静态语义保留在 `context_request` / `context_projection` 描述中，不再作为每次结果重复字段。见 [context_projection.rs](../../src/tool_runtime/context_projection.rs)。

可逐步增加按需刷新与来源版本说明，避免每次重复大段目录；但要保留请求级 ACK 对模型上下文保留状态的意义，不能用 session id、窗口身份或服务器缓存命中替代。summary、read body、execute schema 应分别预算。

### 5.5 UI 与观测层只消费事实

[MCP presentation](../../src/mcp/presentation.rs)，9–33，将 App descriptor eligibility 与兼容投影分开；162–176 从 canonical lifecycle 派生展示状态。这个方向应保留。

更好的卡片应回答“现在是否还有工作、是否可安全重试、下一步观察哪个身份”，而不是另建 UI 状态机。展示失败不应取消执行；显示完成不应替代测试/审查证据。若新增 Goal，应在现有执行之上做关联与推进；没有 Goal 时原 read/edit/run/test/observe 路径照常工作，不能强制普通工具先走 Goal admission。

### 5.6 组合层是调用优化，不是第二执行内核

已有 [tool-composition-research.md](tool-composition-research.md) 讨论了在 canonical tools 之上组合。本轮不重新引入另一套 DAG/Workflow engine。

资源目录能帮助选择工具，组合层能减少往返，执行内核继续拥有授权/效果/Job。这三层必须分开；外层批次不自动成为可回滚事务。先支持独立只读观察的已知组合，再考虑有证明的资源级并发。

## 6. 不建议本轮采用的方案

- 一个 `ResourceManager` 同时管理 Memory 数据、Skill 文件、Plugin 进程、Job 和 AgentTask：表面统一，实则塞入大量不适用字段与例外。
- 把 User / Project / Runner 做成通用 authority 继承链：产品目录可以使用 `user` / `project` scope，但 Runner 是 placement/source，不是更高或更低一层的资源权限；同名冲突仍由具体资源定义。
- 在没有真实多用户需求时提前建立 `user_id` 私有 namespace、resource ACL、sharing/revocation、team/org/public scope：会把 self-hosted Runner authority 问题错误升级成多租户安全系统。
- 全部对象共用一张 JSON/EAV 表：放弃已有事务/索引/约束，并没有消除领域差异。
- 所有失败统一 retry，或把 outcome_unknown 转成普通业务失败：可能重复外部副作用。
- 因文件行数多就拆 crate/微服务：文件长度包含测试和契约，不能独立证明职责错误。
- 提前搭建动态注册框架：当前闭合集合适合 enum/静态声明；扩展点越靠近权限与执行，越不应无约束开放。

## 7. 使用证据驱动的后续路线

旧路线中的 4B `User-private namespace / sharing` 不再视为近期交付目标；4A1 也不再自动推出 Memory/Skill/Plugin 的完整 inheritance。后续优先级由真实使用频率、模型决策成本和已经存在的重复边界决定。

| 阶段 | 交付 | 保持不变的边界 | 验收方式 |
|---|---|---|---|
| 0，已完成 | 启动目录投影的小型复用、基线表征测试、本文 | JSON 形状、顺序、hint、预算、来源发现和权限均不改 | 新旧 JSON oracle 对照，空/不可用/上游截断、Unicode/转义、超大条目；既有 startup 测试 |
| 1，已完成 | 扩展家族 admission 声明归 ToolDefinition；Runner Skill provider enqueue typed error 小切片 | 外部错误、scope、surface、direct/gateway 语义不改 | family/registry invariant、ModelHidden invariant、surface/principal focused tests、typed-to-legacy error-kind 对照 |
| 2，已完成 | Skill catalog observer / known-id exact resolver 分离；Configured + Managed 收敛到一个 Runner Skill runtime/management boundary | opaque identity、Project authority、请求时授权、catalog 完整语义、management outcome-unknown、revision/race 语义不改 | before/after request fanout、canonical wire/capability inventory、duplicate target、source unavailable、source identity race、revision race、configured identity-first scan tests |
| 3A，已完成 | Store connection contention observability baseline | DB authority、schema、transaction/CAS/replay、poison 与 async execution model 均不改 | dogfood / production-like 数据；没有显著 contention 就停止 |
| 3B，仅数据证明需要时 | critical-section 缩短、bounded blocking worker / DB actor 或 fenced cache 中的最小必要项 | 未知结果、事务、重放、authority 与 revision 语义不改 | 必须优于 3A 实测基线；无证据则不实施 |
| 4A1，已完成并保留 | managed-worktree source lineage：Runner-owned lineage、current source identity resolution、path-safe diagnostics | target execution Project、cwd、Session、Plugin、permission、repository identity 均不改 | creation/resume lineage、source id/root drift、inventory corruption、target execution independence |
| R1，下一资源切片 | 统一产品描述词汇：`kind + scope(user/project) + source + placement/applicability`；只做 descriptor/projection，不建 ResourceManager | 不改现有 Skill/Plugin/Memory lifecycle，不新增 ACL/storage | startup/context/catalog projection 能解释资源来自哪里、属于 user 还是 project、下一步如何读取/describe |
| R2 | Skill 产品映射：Project Skill；User configured Skill；User managed Skill | 保留现有 RunnerSkillRequest、opaque id、definition/package revision、management lifecycle | 同一底层行为下减少 Runner-source 实现细节对模型/用户的暴露；exact read 行为不退化 |
| R3 | Plugin 产品映射：User Plugin + Project applicability + Runner placement | 保留 gateway-only、describe binding、exact Runner/provider/schema、scope checks、OutcomeUnknown | startup/resource catalog 能解释 Plugin placement/applicability；call 仍必须走现有 exact binding |
| R4，仅真实需求出现时 | managed worktree 复用 source **Project Skill** | source Skill只读；不继承 Plugin；不把 base SHA 当 Skill revision | source/target Skill identity、definition revision 与 conflict 语义有真实 dogfood证明 |
| R5，R1-R3稳定后 | 轻量 Resource browser / startup browser | browser只消费 canonical facts，不成为执行/授权事实 | completeness、冲突、来源、分页；不创建第二套状态机 |
| Memory freeze | 保留现有 Project Memory，不新增 user scope、repository read-through、sharing/ACL | CAS、scope identity、catalog revision、现有 tools 全部保持 | 只有出现具体、重复的真实使用场景才解除 freeze |

这条路线不否认未来可能出现多用户/private sharing需求，而是拒绝现在预付其复杂度。当前 self-hosted 产品里，Runner access已经是关键用户执行 authority；`user` resource scope是产品可见性/归属词汇，不是新的 bearer capability或 ACL namespace。若未来 SaaS、多租户或资源分享成为真实需求，应以当时的principal模型、部署方式和用户故事重新设计，而不是让今天的 `AuthContext.user_id` 决定永久资源 schema。

阶段 2 已按这些前置条件落地：`skill_read_file` 不再调用完整 `discover_skills`。Project exact probe继续使用 bounded package identities；Runner-local exact probe只发一个 canonical `RunnerSkillRequest::Resolve`，由 Runner内部同时判断 configured/managed membership，再用 source-pinned `Read`读取实际资源。所有 applicable authority source都参与 target uniqueness证明；unsupported `skill_runtime`时Project-only target仍可工作，capability适用但source uncertainty或target ambiguity时继续fail closed。Configured resolver不为unrelated packages读取定义；Managed仍通过 `list_skill_keys()`做 O(N) opaque-id scan，尚未声称O(1)。完整 catalog observer继续独立承担Project+Runner duplicate-id、name conflict、catalog revision、diagnostics/truncation；Runner-local List内部再承担Configured+Managed duplicate-id fail-close。后续若实测需要优化managed key scan，应另行设计与store lifecycle一致的reverse mapping，而不是在本阶段偷加cache/index。

资源模型不应长期占据主线。如果 R1-R3 能用很薄的描述层解释 Skill/Plugin，就应停止继续抽象，把主要工程投入重新转回**工具层**：模型可见 schema 成本、direct/gateway 发现一致性、错误恢复提示、批量/组合调用、Job handoff 和常用工具 ergonomics。已有 [tool-composition-research.md](tool-composition-research.md) 可作为下一轮重新评估起点，但同样必须由真实调用成本和 dogfood 证据筛选，而不是按文档逐项实现。

建议持续关注的指标不是抽象数量，而是：新增一个工具需要改多少个独立分类点；精确 Skill/Plugin 使用触发多少次 Runner 请求；为了做一个简单选择要给模型多少 schema 字节；失败是否直接给出可执行的下一步；核心改动需要编译和运行哪些无关测试。

## 8. 本轮工具使用反馈

### 实际观察

- `work_on_project(mode=worktree)` 完成精确基线解析、隔离、项目注册和指导材料加载，降低了手工 git worktree 与授权注册串联的成本。
- `read_files` 的编号、SHA 与显式 continuation 支撑可追溯分析和受保护修改；`search_project_texts` 一个子查询失败不影响其他查询。
- 本轮 `show_changes(max_hunk_lines=200)` 顶层正确报告 `diff_hunk_line_limit` 并给出恢复调用，但对应 hunk 同时带有 `truncated=false`，文本又在函数中途结束。按提示使用 `git_diff_hunks`、更窄路径与 400 行上限后取得完整 diff。建议对顶层/条目级完整性元数据增加一致性测试；本轮只记录，不修改该工具。
- 对不存在的 `src/tool_runtime/context_material.rs` 发起搜索，得到 `search_execution_failed / backend_process_failed / exit_code=2`。重新定位后实际文件是 `context_projection.rs`。相比之下，read_files 对不存在的文件明确返回 `not_found`。建议搜索入口也区分缺失路径与真正后端故障。
- 一个拟进行只读源码计数的 Python 命令被宿主以“无法确定请求的安全状态”拦截；没有 Runner 执行结果。没有改换通道重试。此事不能归因于 WebPi 权限策略，报告也不使用该计数结果。产品诊断应区分 host pre-dispatch rejection 与 Runner business error；Runner 未收到请求时不能声称观察到执行状态。
- `cargo_test` 能把同一执行交给 Job，再与独立阅读重叠，保留了执行身份。其当前结构化 schema 有 package/filter 等参数，却没有 `--lib` 选择项。可评估增加常见 target selector，减少为了精准验证退回 raw Cargo 的需要；不要求默认扩大运行范围。

### 可用性建议

以“模型每次需要额外判断什么”为优化对象，而不是只数工具数量。优先改善空目录的归属/完整性说明、错误恢复提示、相同操作在 direct/gateway 的发现一致性。不要为少一次工具调用牺牲每个子操作独立结果，也不要用更长、更严厉的工具说明弥补本可由类型和默认值解决的问题。

标准启动结果有不泄露绝对根路径的测试约束，因此即使路径展示能节省一次 `pwd`，也不能未经评审直接加入。诊断模式可以按既有可见性政策提供精确位置；普通模型结果继续保持 path-safe。

## 9. 本轮代码验证切片

本轮选择的实现是启动专用 `StartupCatalog<Entry>`：Skill/Plugin 的公共投影外壳与贪心字节预算算法共用，保留具名别名、各自构造入口、hint 和上游完整性来源。

这不会新增资源管理工具、provider 加载器、Memory 层级或 Plugin cwd 重定向，不会改变 Token/Session/Job 授权，也不会对 live services 做任何 reload。它是“共享描述，不共享行为”的小型实例，不代表本文其余提议已经完成。

新增独立测试模块 `src/tool_runtime/tests/startup_catalog.rs`，使用独立 JSON oracle 描述原有 greedy-prefix 协议。测试覆盖多种数量/长度、Unicode 与 JSON 转义、已观察条目少于 provider total、空目录、不可用目录、超大条目停止、总预算及来源截断。验证先在原实现运行，再在重构后运行；具体实际结果见下节。

## 10. 验证记录

- workspace boundary：本轮已通过，17 packages；没有修改依赖策略。
- 新增表征测试的基线运行：`cargo test -p webcodex startup_catalog`，4 passed / 0 failed；两组参数矩阵合计 100 个 JSON 对照场景，另有不可用与合并预算测试。
- 最终源码：`cargo fmt -p webcodex -- --check` 通过；`cargo test -p webcodex startup` 完成编译并执行 41 tests，41 passed / 0 failed，包含新增的 4 tests。
- 扩展目录集成：`cargo test -p webcodex work_on_project_extension_catalog`，2 passed / 0 failed；覆盖 configured roots 进入目录及关闭目录时跳过发现。
- 文档链接：`python3 scripts/check_markdown_links.py`，75 个 Markdown 文件、512 个本地链接、missing=0。
- 变更检查：`git diff --cached --check` 通过；已审查完整 Rust diff 和新增测试。性能收益没有计时验证；编译成功不等于全仓测试通过。
- 未运行：全 workspace 测试、跨平台测试、外网/生产重启、性能基准。
