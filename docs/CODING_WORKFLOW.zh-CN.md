# Coding 工作流

[English](CODING_WORKFLOW.md) | [简体中文](CODING_WORKFLOW.zh-CN.md)

本文面向普通 WebCodex 用户，以及正在使用 WebCodex 的 coding agent。它说明如何
bootstrap coding task、选择模型行为、做验证，以及如何理解 closeout evidence；它不是
开发 WebCodex 本身时使用的 contributor architecture 文档。

## 唯一的 canonical mental model

`start_coding_task` 与 `work_on_project` 都是 **bootstrap 与 continuity 工具**。它们建立
或延续 project-scoped Workflow Session evidence，并返回有界 workflow guidance 与
project-local instructions；它们不是 role selector。

- 普通 coding bootstrap 优先用 `work_on_project`；它的 task `instruction` 正是描述模型
  应该做什么的自然位置。
- 如果 caller 当前的模型上下文已经保留适用的 repository instructions，可使用
  `work_on_project(include_project_instructions=false)` 省略本轮 bootstrap response 中的
  instruction 正文。WebCodex 仍会观察这些文件，并为本次 bootstrap 对应的 Workflow
  Session 记录当前的 instruction metadata。
- 如果 caller 当前模型上下文已经保留 WebCodex 内置 coding-workflow guidance，可使用
  `work_on_project(include_workflow_guidance=false)` 省略 response 中静态的 `workflow`
  section。这只控制 model-facing projection，不改变 Workflow Session state、authority、
  role selection 或 execution semantics。
- `work_on_project` 的成功输出默认采用 sparse projection。省略的默认 section 表示：没有
  Session execution defaults、普通已有 project 的解析没有特殊事件、repository overview
  按设计未请求、readiness 为 pass/non-blocking、没有值得报告的 Job，或 blockers/warnings
  为空。Instruction source 始终保留 path/fingerprint identity；false/null/empty 的正文投影
  字段会省略。真实 warning、blocker、truncation、非默认 project resolution 和值得报告的
  Job state 仍会显式返回。
- 需要精确 `resume_session_id`、显式 `new_session=true` 隔离等高级 continuity 控制时，
  使用 `start_coding_task`。
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

role 名称应写在 instruction 文本中，不要在 `start_coding_task` 或 `work_on_project` 上寻找
role 参数。

## 手动多窗口协作

需要把一个有界独立子任务交给另一个窗口时，coordinator 与 worker 应保持**不同的**
Workflow Session。coordinator 在自己的 Session 中发布 `todo`；worker 新建独立 Session，
读取 coordinator 的 `session_handoff_summary` 与对应 open todo，在自己的 Session 下完成
子任务，然后向 coordinator Session 发布带 `reply_to=<todo_id>` 的有界 `answer`，最后
resolve 精确的 todo。

第一版故意保持手动：没有自动 claim、worker scheduler、共享 transcript，也没有隐式的
跨 Session authority。一个 todo 由人工分配给一个 worker。多个窗口共享 worktree 时优先
让 worker 以读为主；确实需要独立并发写时优先隔离 worktree/project。`read_only` 请求或
Session mode 是有用的执行姿态与 guard context，但不是 worker 整个生命周期实际行为的
权威证明。worker 应如实回报 mutation、shell/process、validation、external effect 等重要
操作或偏离预期的行为；coordinator 以记录下来的 tool/effect evidence 与当前 workspace
状态为准。回传结论、关键 evidence 与 result path，而不是把长 transcript 注入 coordinator。

详细协议与后续 convenience primitive 的 dogfood gate 见
[Manual Multi-Window Collaboration](agent/manual-window-collaboration.md)。

## 已被 dogfood 证明重要的习惯

**修复后复用 validation identity。** structured validation 使用 `assertion_name` 时，同一个
logical validation 在修复后重跑，应复用原 `assertion_name`。这样 validation ledger 才能把
它表达成同一个已解决 assertion，而不是两个互不相关的检查。

**把 guarded edit 冲突视为 zero-write failure。** SHA 或 edit anchor stale 时会 fail closed。
重新读取当前文件，确认新的精确内容，再用 fresh guard 重试原本的编辑。不要为了让编辑
成功而削弱 guard。

**优先 structured validation。** 当 `cargo_test`、`go_test` 或其他 structured validation
能够表达目标检查时，优先使用它们；只有结构化 surface 无法覆盖时才用 shell。结构化
结果能给 Session ledger 更安全的 evidence，而不必解析任意 command text。

**把 closeout 当 evidence，不当 completion authority。** `finish_coding_task` 返回 recorded
Session evidence 的 deterministic advisory snapshot；按请求可包含 validation、workspace、
jobs 与 tool history。它不决定任务已经完成，不替代直接的 diff/test review，也不替代最后
面向用户的 acceptance 判断。

## broader runtime 的典型循环

```text
work_on_project（高级 continuity 时用 start_coding_task）
→ inspect/search/read
→ guarded edits
→ structured focused validation
→ review diff/workspace
→ finish_coding_task
```

始终遵循当前 Server 实际暴露的 tool surface 与返回的 project instructions。workflow
guidance 用来帮助模型组织本轮工作，但绝不会扩大调用方的实际权限。
