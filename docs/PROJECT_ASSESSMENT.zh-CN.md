# WebCodex 项目整体评估（2026-07-11）

这份文档回答三个问题：**这个项目整体是什么样、架构撑不撑得住继续做、往下做怎样能做好。**
评审方式：通读架构/概念/安全/威胁模型文档，深入阅读工具调度、agent 传输、LSP、
编码工作流（sessions/handoff/hygiene/checkpoint/validation）、认证（7 种凭据 +
OAuth2 桥）、数据层（SQLite 15 表）的代码，并核对 git 历史（2026-05-27 起，
45 天 727 提交，单人开发）。工程债细项见姊妹篇
[OPTIMIZATION_REVIEW.zh-CN.md](OPTIMIZATION_REVIEW.zh-CN.md)，本文不重复。

---

## 一、结论先行

**1. 能继续做下去吗？——能。**
架构没有需要推倒重来的根本性缺陷。分层正确（协议适配 → 协议无关 ToolRuntime →
agent 桥 → agent 执行），信任边界清晰（服务器永不碰文件系统），协议是封闭类型
（不存在任意透传），资源全部有界。继续叠功能不会先撞到架构墙。

**2. 能做好吗？——工程能力已经被证明，风险不在"写不写得出来"。**
45 天做出 15.4 万行、1856 个测试、成文威胁模型、schema 防漂移测试、eval 对照
脚本、双语文档——这不是 demo 纪律，是产品纪律。真正决定"做没做好"的是三件事：
**把权限审批从脚手架变成真闸门**（见 4.1，这是产品灵魂但目前是 no-op）、
**做概念减法**（两套 session、双 verdict、断言框架，见 4.2）、以及**采用/分发**
（工程之外的事）。

**3. 架构设计怎么样？——核心决策全对，边缘有中期债。**
做对的五个关键决策见第三节；要还的债集中在概念收敛和持久层硬化，都是可以
渐进偿还的，不阻塞新功能。

---

## 二、整体功能全景（我理解的 WebCodex）

一句话：**自托管的"在线模型 → 私有代码库"工具网关**——ChatGPT/Claude 等在线
客户端通过 MCP 或 GPT Actions 调用工具，代码永远留在 agent 所在机器上，服务器
只做认证、策略、路由和留证。

### 2.1 四层结构

```
协议适配层    /mcp (MCP 2025-06-18) | /openapi.json (GPT Actions, 29 操作) | REST
                └── 三个入口翻译到同一个 ToolRuntime，无重复业务逻辑
运行时层      ToolRuntime：约 74 个工具 / 12 个发现分组；统一调度闸门
                (dispatch.rs 仅 354 行：会话-项目校验 → 工具禁用 → 会话守卫
                 → agent 授权 → 权限决策 → 领域路由)
桥接层        shell_client 注册表（server 侧）+ QUIC → WebSocket → 轮询降级
                （agent 主动外连，每客户端队列上限 256，typed envelope）
执行层        webcodex-agent：文件/补丁/git/shell/job/artifact/checkpoint/LSP，
                全部在注册项目根内执行；LSP 是受约束 rust-analyzer 子进程
```

### 2.2 编码工作流闭环（产品的核心交互）

`start_coding_task`（一次调用返回启动包：会话 id、git 状态、权限画像、项目
规则、LSP 能力探测、启动裁决）→ inspect（read/search/symbols）→ edit（结构化
行编辑 / 校验补丁）→ validate（cargo fmt/check/test，**验证状态从会话账本回放
派生**，而非独立状态机）→ review（show_changes / hygiene 检查）→
`finish_coding_task`（聚合裁决）或 `session_handoff_summary`（跨会话交接）。

两个设计值得点名：
- **账本派生验证**：`validation_events.rs` 回放工具事件得出 passed/failed/
  mixed，包括"零测试通过不算数"（跑了 0 个测试的 cargo_test 不能冲销历史
  失败）和"会话内修复可豁免"的细则。方向优雅，但细则链条偏长（见 4.2）。
- **checkpoint 的恢复安全**：恢复前先 `git apply --reverse --check` 预检，
  失败则回滚已应用 hunk 并重放原工作区状态；敏感路径在捕获与恢复两端都排除。
  这是全库最谨慎的一段状态变更代码。

### 2.3 认证与数据

7 种凭据（bootstrap / shared key / `wc_acct_` / `wc_pat_` / `wc_agent_` /
`wc_oat_` 及 OAuth 支持件），全部只存 SHA-256（256 位随机令牌，不需 KDF 的
理由成文写在 `pat.rs`）；bootstrap 用常数时间比较；OAuth2 桥（授权码 + PKCE +
刷新轮换）解决"ChatGPT 只会说 OAuth、不会带静态 Bearer"的现实问题，配有
成文威胁模型且代码可逐条对应。多账户（admin/user 角色）但**非多租户**——
会话/审计数据是服务器全局的，与其自述定位一致。

持久化三分：SQLite（15 表：身份/令牌/OAuth/审计）＋ 内存+JSON 的编码会话
账本（上限 100 会话 × 200 事件，LRU）＋ checkpoint JSON 文件。

### 2.4 成熟度外围

CLI（server up / connect / pairing / doctor / 服务安装）、systemd + nginx
部署样例、e2e 零配置脚本、npm 分发（linux-x64）、发布检查清单、
**eval_coding_loop.sh 的 baseline/guided/compare 三模式对照评测**（用数据验证
"引导式工作流是否优于裸工具"——个人项目里罕见的自我怀疑机制）。

---

## 三、架构评价：五个做对了的关键决策

1. **协议无关的 ToolRuntime 内核。** MCP、GPT Actions、REST 是三张皮，业务
   只写一遍。将来接任何新客户端协议（或 MCP 版本升级）都只在适配层发生。
   这是全项目最有复利的决策。
2. **服务器永不接触文件系统 + agent 主动外连。** 路径解释权全部在 agent；
   服务器只按 `agent:<client_id>:<project_id>` 路由。安全叙事因此简单可信，
   也天然适配 NAT/内网（agent 出站连接，无需入站打洞）。
3. **封闭类型协议。** 桥上传输的是 typed enum（如 LSP 的 7 种操作），未知
   操作在反序列化就失败；shell 是显式 escape hatch 而非隐式 fallback。
   "模型能做什么"由类型系统而非运行时判断兜底。
4. **有界性纪律。** 队列 256、消息 8 MiB、账本 100×200、stderr 64 KiB、
   全部文本有截断——长驻服务的资源事故面被系统性压缩。
5. **防漂移测试基建。** 74 个工具要在 ToolCall/注册表/scope/MCP/OpenAPI 五处
   同步，这本是高危的重复，但 schema drift test + 安全回归测试把"忘了同步"
   变成编译期/测试期错误。ARCHITECTURE.md 还写明了新工具的不变量清单。
   **这是单人 + AI 辅助高速开发还能保持质量的真正原因。**

---

## 四、架构短板与风险（按重要性排序）

### 4.1 权限闸门还是脚手架（产品层面最重要的一件事）

`permissions.rs` 目前恒为 `dev_auto_approve`：每个决策都返回
`auto_approved / human_approval_required: false`；`require_approval` 只是被
命名为"发布推荐策略"的字符串，没有实现。也就是说，**产品承诺的"人工审查后
接受"目前靠的是事后证据（show_changes/hygiene/审计），而不是事前闸门**。
对"让在线模型操作私有代码"这个定位，同步审批（或至少按风险分级的审批）是
灵魂功能：它是与"直接给模型一个 shell"的本质区别。好消息是决策点、审计
挂点、结果附着都已就位，只差真实的 pending/approve 流与一个操作界面。

### 4.2 概念层需要减法（复杂度的主要来源不是代码而是概念）

- **两套互不相识的 "session"。** 编码会话账本（`wc_sess_*`，内存+JSON）与
  HTTP 动作审计会话（UUID，SQLite，1800 秒空闲）同名、职责相近、零交叉引用。
  操作者迟早会问"这两个 session 哪个是真的"。应合并或至少互相引用。
- **会话没有生命周期。** CONCEPTS.md 说会话记录 closeout 状态，但
  `SessionRecord` 没有状态字段，`finish_coding_task` 不改变会话（可无限次
  finish），只有 LRU 淘汰。closeout 目前是"报告"不是"状态迁移"——文档与
  实现有真实缺口。
- **双 verdict 表示迁移中。** legacy `verdict{}` 与新的 `task_outcome /
  evidence_history / evidence_integrity` 并行输出，"能不能交付"同时有两个
  答案。迁移应尽快收尾，删掉旧形态。
- **断言/期望框架的收益存疑。** 工具参数可带 `expected_failure /
  assertion_name`，账本对每次调用做 matched/unexpected 分类并影响 closeout
  阻断——这是把一套测试 DSL 织进了生产账本，而 `8a30e0f` 的"已恢复失败
  豁免"逻辑很大程度是在给这套框架制造的误报打补丁。建议评估：它服务的
  真实场景是什么？若主要服务内部评测，应隔离到评测层。
- **零测试/历史失败判定链偏长。** cargo 输出启发式解析 → 三态
  `zero_tests_run` → 决定性事件过滤 → 历史失败状态 → closeout 豁免扣减，
  横跨三个模块。测试覆盖充分，但每加一条细则，下一条就更难推理。

### 4.3 持久层需要一次硬化（工程层面，动作明确）

- SQLite 打开时**无 WAL、无 busy_timeout、无 foreign_keys pragma**，全部
  访问串行在一把 `Mutex<Connection>` 上。单进程下正确，但读并发为零，
  外部工具一旦打开同一文件就有 BUSY 风险。开 WAL + busy_timeout 是两行改动。
- **令牌无回收**：过期/已用的 OAuth 码、访问/刷新令牌永不删除，只在查询时
  过滤——长驻服务器的慢泄漏。需要一个后台清扫任务。
- **两处明文出站凭据**：`agent_specs.auth_token`、`agent_model_profiles.api_key`
  是必须回放给第三方的凭据（无法哈希），但目前无信封加密裸存 SQLite。
  至少应在 SECURITY.md 声明，最好加静态加密。
- **三张僵尸表**：`codex_goals`、`command_requests`、DB 版 `messages` 在
  db 层之外无任何调用方（早期原型遗留，甚至还在被迁移代码维护）。确认后
  连迁移一起移除。

### 4.4 安全叙事的一个缺口

SECURITY.md 的边界描述诚实清晰，但**没有点名 prompt injection**：模型读到
仓库中恶意构造的内容（README、注释、测试夹具）后被诱导调用 `run_shell` 或
外带数据——这是此类产品的头号剩余风险。现有缓解（结构化工具优先、shell 为
受审查的 escape hatch、证据链、窄 agent 根）方向正确，但应作为命名威胁写进
SECURITY.md，并与 4.1 的审批闸门形成体系（"不可信内容 + 高危工具 = 必须
人工确认"）。

### 4.5 战略风险（工程之外，但必须想清楚）

- **平台一等公民方案的挤压。** ChatGPT/Claude 官方的编码代理（含桌面端、
  Codex/Claude Code 类产品）覆盖了"我只想让 AI 改我本地代码"的主流场景。
  WebCodex 的生存空间在别处：**自托管审计合规、多客户端中立（一套服务器
  同时喂 ChatGPT/Claude/Grok）、订阅套利（用已付费的网页版订阅而非 API
  计费）、细粒度权限与留证**。README 目前只讲 ChatGPT——楔子没错，但
  差异化叙事应该尽早写清楚。
- **GPT Actions 会 legacy 化，MCP 是对的下注。** 架构上两者已是薄适配层，
  切换成本为零——这个风险已被架构消化，保持即可。
- **单人 15.4 万行的可持续性。** 防漂移测试是目前的答案，且有效。但表面积
  只会涨不会跌：**对 74 个工具做减法（或冻结）比继续加更重要**。仓库自己
  写了原则（"默认走 generic 工具路径，除非有明确产品理由"）——执行它。

---

## 五、"能做好吗"的判断依据

我认为**工程侧已经越过了"能不能做出来"的证明阶段**，依据不是代码量，而是
只有成熟工程文化才会出现的工件，且它们在 45 天内全部就位：

| 工件 | 说明 |
|---|---|
| 成文威胁模型 | OAUTH2_BRIDGE_THREAT_MODEL.md，代码逐条可对应 |
| 防漂移测试 | 工具五处同步由测试强制，含 dead-code 卫生条款 |
| 对照评测 | eval_coding_loop.sh baseline/guided/compare |
| 有界性全覆盖 | 队列/消息/账本/文本全部有上限并测试 |
| 发布纪律 | RELEASE_CHECKLIST、npm 冒烟、AGENTS.md 释放门 |
| 明确的 non-goals | ROADMAP 写明不做 IDE、不做自主运维平台 |

剩下的成败变量按权重排：**① 采用与分发**（谁在用？first-run 体验、多平台、
一个真实用户社区）＞ **② 权限闸门做实**（4.1，差异化的核心）＞
**③ 概念减法**（4.2，决定两年后还能不能轻快地改）＞ ④ 工程债偿还
（OPTIMIZATION_REVIEW 清单）。①②是产品问题，③④是工程问题——工程问题
在这个项目里从来不是瓶颈。

---

## 六、建议的下一步（如果继续，按此顺序）

1. **把 `require_approval` 做成真的**：pending 队列 + 审批 API + console
   页面（现有只读 console 是合适的宿主）；按风险分级（shell/job 必审，
   结构化编辑可配置）。这一步完成之前，不宜宣传给低信任场景用。
2. **会话概念收敛**：给 SessionRecord 加生命周期状态（active/finished/
   handed_off），finish 变成状态迁移；合并或桥接两套 session；删除 legacy
   verdict。做完这步再谈新工作流功能。
3. **持久层硬化包**（一次 PR 可完成）：WAL + busy_timeout + foreign_keys、
   令牌后台清扫、僵尸表清除、出站凭据加密或声明。
4. **SECURITY.md 增补 prompt injection 章节**，与 1 的闸门呼应。
5. **分发**：多平台 npm 产物（配 CI matrix）、first-run 体验打磨、README
   增加差异化叙事（自托管/多客户端/审计）。
6. 工程债按 [OPTIMIZATION_REVIEW.zh-CN.md](OPTIMIZATION_REVIEW.zh-CN.md)
   执行（CI 最先）。
7. **表面积纪律**：新工具默认走 generic 路径；每季度审视一次工具清单，
   敢于合并/下线（4 个零文档的 session message 工具是第一批候选：
   要么写文档，要么收编进 handoff）。

---

## 七、评分卡

| 维度 | 评分 | 一句话 |
|---|---|---|
| 分层与边界 | ★★★★★ | 协议无关内核 + agent 信任边界，教科书级 |
| 协议/接口设计 | ★★★★☆ | 封闭类型协议优秀；手写 OpenAPI 是维护热点 |
| 安全模型 | ★★★★☆ | 认证/脱敏/威胁模型扎实；审批闸门未实装、注入未点名 |
| 工作流概念完整性 | ★★★☆☆ | 闭环成立且机制谨慎；双 session/双 verdict/断言框架待减法 |
| 持久层 | ★★★☆☆ | 单机定位下够用；WAL/GC/明文凭据/僵尸表需一次硬化 |
| 测试与防回归 | ★★★★★ | 1856 测试 + 防漂移 + 对照评测，单人项目罕见 |
| 文档 | ★★★★☆ | 双语、诚实、有 non-goals；个别子系统滞后 |
| 可持续性 | ★★★★☆ | 护栏充分；依赖表面积自律与（远期）workspace 拆分 |
| 产品定位 | ★★★☆☆ | 缝隙真实存在但会被挤压；差异化叙事与采用是主战场 |

**总评：这是一个架构决策正确、工程纪律超出规模预期的项目。它值得继续做；
下一阶段的敌人不是技术难度，而是概念蔓延与"最后一公里"的产品功能
（审批闸门、first-run、分发）。**
