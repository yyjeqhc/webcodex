# Coding 工作流

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

本文面向普通 WebCodex 用户，以及正在使用 WebCodex 的 coding agent。它说明如何
bootstrap coding task、选择模型行为、做验证，以及如何理解 closeout evidence；它不是
开发 WebCodex 本身时使用的 contributor architecture 文档。

## 唯一的 canonical mental model

`work_on_project` 是普通 coding/review 的 **canonical model bootstrap**。它建立或延续
project-scoped Workflow Session evidence，并返回有界 workflow guidance 与 project-local
instructions；它不是 role selector。

- 普通 coding bootstrap 优先用 `work_on_project`；它的 task `instruction` 正是描述模型
  应该做什么的自然位置。
- `include_project_instructions=true`（默认）始终投影当前适用的有界 repository
  instruction 正文；即使精确续用同一个 Workflow Session 且 repository delta status 为
  `reused`，正文仍会返回。只有 caller 当前模型上下文已保留这些 instructions 时才应传
  false；WebCodex 仍会重新观察文件并更新 Workflow Session instruction metadata。
- `include_workflow_guidance=true`（默认）始终投影 canonical 内置 coding workflow。
  只有 caller 当前模型上下文已保留该 guidance 时才应传 false。该 flag 只控制
  model-facing projection，不改变 Workflow Session state、authority、role selection 或
  execution semantics。
- WebCodex 不会从 `wc_sess_*` Workflow Session、MCP/HTTP transport identity、client
  window、credential、project 或 Server process lifetime 推断当前模型上下文是否仍保留
  静态内容。Workflow Session 只表示业务 continuity，同一个 Session 可以被多个独立模型
  上下文 resume；只有 caller-explicit 的 include flags 才能省略静态 model-facing 内容。
- `work_on_project` 的成功输出默认采用 sparse projection。省略的默认 section 表示：没有
  Session execution defaults、普通已有 project 的解析没有特殊事件、repository overview
  按设计未请求、readiness 为 pass/non-blocking、没有值得报告的 Job，或 blockers/warnings
  为空。Instruction source 始终保留 path/fingerprint identity；false/null/empty 的正文投影
  字段会省略。真实 warning、blocker、truncation、非默认 project resolution 和值得报告的
  Job state 仍会显式返回。
- `work_on_project` 是唯一 canonical 的外部 coding bootstrap / continuation 入口。
  `start_coding_task` 这一旧 wire/API tool name 已退休，调用会 fail closed 并提示改用
  `work_on_project`；其 advanced startup fields 不再构成公共兼容面。内部仍保留
  `StartCodingTask` primitive，仅作为 canonical workflow 的共享实现细节。
- behavioral role 由 **task instruction** 显式选择。实现任务明确写使用
  `implementation_owner` guidance；独立评审明确写使用 `independent_review` guidance。
- 返回的 role guidance 永远只是 model guidance。它不会创建 authority、permission、
  Session mode 或 capability。认证、项目访问、tool policy 与 runtime guard 仍独立决定
  实际 authority。

不存在 `role` wire field，也不存在 durable Session role state。同一个 Session 可以延续，
而后续 pass 的 task instruction 可以要求模型采用不同的 behavioral role。

WebCodex 还有 project-bound Connector surface，它的入口是 `task_start`。使用该 surface
时应遵循它自己的 task workflow；原则仍相同：bootstrap state 与 behavioral guidance 都
不是 execution authority。

## 可直接复制的 prompt

实现：

```text
使用 WebCodex bootstrap 或继续这个 coding task。本次实现使用 implementation_owner
guidance。沿现有架构实现 <任务>，运行聚焦的 structured validation，并审查最终 diff。
```

独立评审：

```text
使用 WebCodex bootstrap 或继续这个 coding task。本轮使用 independent_review guidance。
独立评审 <改动或 commit>，只修复具体发现，并运行聚焦 regression validation，最后说明
该改动是否可接受。
```

role 名称应写在 instruction 文本中，不要在 `work_on_project` 上寻找 role 参数。

## 手动多窗口协作

需要把一个有界独立子任务交给另一个窗口时，coordinator `C` 与 worker `W` 始终保持
**不同的** Workflow Session。coordinator 在 `C` 发布 `todo`；`W` 调用
`get_session_assignment(session_id=C, message_id=<todo_id>)`，从同一次原子读取取得 exact
todo、全部 retained direct replies（受上限约束）和 opaque `assignment_fence`。所有工具调用、
validation 与 review evidence 都保留在 `W`；完成时调用 `complete_session_message`，同时传入
exact `session_id=C`、`message_id`、caller `completion_key` 与原样的
`expected_assignment_fence`，原子创建 bounded answer 并 resolve todo。`author_session_id` 只来自
已经独立授权的 explicit `recording_session_id=W`；没有 recorder 时保持为空，不从 window、
credential、project 或 legacy `mcp-session-id` 推断，caller 也不能伪造该 provenance。

`list_session_messages(message_id=...)` / `reply_to=...` 仍用于通用浏览和结果读取，不是
executable todo 的 assignment source。若 completion 返回 `assignment_stale`，必须重新评估
返回的 current assignment 后才能使用其中 durable fresh fence；history-loss / oversize 不是
blind retry 信号。`observe_session_messages` 只是可选的 generic delta observation，不属于上述
happy path。coordinator 随后重新观察权威的 project/Git/artifact state。worker execution history 不会复制到
`C`，知道 Session/message id 也不会获得 authority。任何 recording Session 都必须先通过
统一 Session authorization，才能参与 ledger/lifecycle/provenance。project-scoped Session
既要求当前 stored project authorization，也要求 creation-time immutable canonical authority-group
fingerprint；project-less Session 使用同一内部 durable fence。direct shared-key 与对应 OAuth
shared-key bridge 会归一到同一 authority group。跨 Session collaboration 还要求双方 stored
project scope 完全一致，因此 scoped/unscoped 两个方向都不能作为 project boundary bridge。
message board 只是 collaboration metadata，不是 claim、lease、filesystem/worktree/branch lock。并发写应使用独立 Git
worktree 与 WebCodex Project。此流程不增加自动 worker spawning、scheduler、共享
transcript、隐式跨 Session authority 或 cross-owner delegation。

详细的 coordinator/implementation、implementation/reviewer、并行 worktree 与 cross-host
示例，以及 uncertain-result retry/idempotency 语义见
[Manual Multi-Window Collaboration](agent/manual-window-collaboration.md)。

## 已被 dogfood 证明重要的习惯

**修复后复用 validation identity。** structured validation 使用 `assertion_name` 时，同一个
logical validation 在修复后重跑，应复用原 `assertion_name`。这样 validation ledger 才能把
它表达成同一个已解决 assertion，而不是两个互不相关的检查。

**把 guarded edit 冲突视为 zero-write failure。** SHA 或 edit anchor stale 时会 fail closed。
重新读取当前文件，确认新的精确内容，再用 fresh guard 重试原本的编辑。不要为了让编辑
成功而削弱 guard。

**把 `apply_patch` 作为模型生成编辑的默认路径。** 小型、精确且带 SHA guard 的修改使用
`apply_text_edits`；只有输入本身已经是 raw unified diff 时才使用 `apply_unified_diff`。
普通 `apply_patch` 保留 Codex-compatible 的 `exact` → `trim_end` → `trim` 匹配顺序。
每个 update chunk 都返回 bounded positioning metadata：`match_mode`、`match_source`、
`matched_start_line`、`candidate_count` 和 `strict_match`。只有该 chunk 用于定位的所有
文本匹配都 exact 且 unique 时，`strict_match=true`。没有 anchor 的 append 不执行文本匹配，
但仍然 strict-safe；它返回 `match_source=append`，且 `match_mode` / `candidate_count` 为 null。
Server-first 滚动升级期间，旧版 `apply_patch` Runner 可以只缺省这些新增 match 字段；
Server 会在请求 admission 时绑定 legacy response contract，同时继续校验原有 transactional
success 字段，不会在 result 返回后根据当前 Runner 状态重新猜测版本。

当要求所有需要定位的 chunk 在任何文件写入前都满足 exact-and-unique 规则时，设置
`strict_matching=true`。该模式要求 Runner 显式支持 `apply_patch_strict_matching` capability，
并会拒绝 fuzzy 或 ambiguous placement，而不是静默降级。Server 会用自己解析的 patch
校验 Runner 的 success match metadata；success metadata 缺失或互相矛盾时返回
`outcome_unknown`，不会把它当成 clean success。

**优先 structured validation。** 当 `cargo_test`、`go_test` 或其他 structured validation
能够表达目标检查时，优先使用它们；只有结构化 surface 无法覆盖时才用 shell。结构化
结果能给 Session ledger 更安全的 evidence，而不必解析任意 command text。

当 focused `cargo_test` 必须证明测试确实运行过时，使用 `require_tests: true` 要求至少一个
test，或使用 `min_tests: N` 声明更大的 bounded minimum；两者同时存在时取更严格的要求。
两者都省略时，exit code 为零但运行零个 test 的 invocation 仍保持 execution success，并
返回 `tests_run_count: 0` 与 `zero_tests_run: true`。显式 count assertion 只有在完整 parser
evidence 能证明达到 minimum 时才通过；evidence 缺失或被截断时 validation contract 会失败，
但不会改写真实 process exit code。count assertion 不能与 `no_run: true` 同时使用。

**把 closeout 当 evidence，不当 completion authority。** `finish_coding_task` 返回 recorded
Session evidence 的 deterministic advisory snapshot；按请求可包含 validation、workspace、
jobs 与 tool history。它不决定任务已经完成，不替代直接的 diff/test review，也不替代最后
面向用户的 acceptance 判断。

## broader runtime 的典型循环

```text
work_on_project
→ inspect/search/read
→ guarded edits
→ structured focused validation
→ review diff/workspace
→ finish_coding_task
```

始终遵循当前 Server 实际暴露的 tool surface 与返回的 project instructions。workflow
guidance 用来帮助模型组织本轮工作，但绝不会扩大调用方的实际权限。
