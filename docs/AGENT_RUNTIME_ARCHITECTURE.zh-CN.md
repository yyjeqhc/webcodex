# Agent Runtime Architecture

[English](AGENT_RUNTIME_ARCHITECTURE.md) | [简体中文](AGENT_RUNTIME_ARCHITECTURE.zh-CN.md)

本文档沉淀 WebCodex 的长期 runtime 演进方向。它是设计锚点，不是 release checklist，也不承诺每个章节都已经实现。

WebCodex 最开始是一个把 GPT Actions、MCP 客户端和本地 agent 接到远程项目上的 coding bridge。现在的方向更大：WebCodex 应该成为一个面向真实机器、真实项目和未来多 agent workflow 的远程、可审计、策略感知 agent runtime。

## 核心判断

WebCodex 不应该被理解成一堆 MCP tools。它应该被理解成 agent runtime：

```text
LLM / online agent platform
  -> WebCodex Agent Operating Contract
  -> runtime tool IR
  -> policy and scope checks
  -> project-scoped execution
  -> session, artifact, and audit records
  -> final report
```

短期产品仍然要务实：做一个 online coding and operations agent，让 GPT Actions、MCP clients 和未来的 hosted AI clients 能安全接入注册机器。与此同时，架构应该让后续 coding harness、operations workflow、artifact generation 和 multi-agent shared space 都能自然落进来。

## 不要复制其它 agent 的 prompt

Codex、Claude、Gemini 和其它 agent 可能都有自己的 built-in instructions。WebCodex 可以借鉴“agent 需要稳定操作协议”这个思想，但不应该复制其它 agent 的 prompt 文本。

WebCodex 有自己的 runtime model：

- 通过 WebSocket、polling 或 QUIC 连接的 remote agents；
- registered projects 和 canonical project ids；
- `allowed_roots` 和 project-scoped execution；
- OAuth2、PAT、account credentials 和 agent tokens；
- MCP tools、GPT Action operations 和 CLI/admin surfaces；
- session recorder、current-session bindings、message boards 和 task guards；
- tool metadata、risk classes、OAuth scopes 和 MCP annotations；
- workspace checkpoints、artifacts、jobs 和 bounded logs。

WebCodex-native instruction layer 应该描述这个环境。它可以叫 **Agent Operating Contract** 或 **WebCodex Runtime Instruction**。它不是 `AGENTS.md`、项目 instructions 或用户任务 prompt 的替代品，而是 WebCodex runtime 注入的稳定行为契约，让模型知道如何通过这个 runtime 安全行动。

## Access and onboarding modes

WebCodex 的接入模式分三层，避免把临时便利路径误当成生产身份系统：

- **Open demo mode:** server 必须显式 `--open` 或 `WEBCODEX_ALLOW_ANONYMOUS=true`，client 使用 `connect --open`。生成的 agent token 为空，agent、GPT Actions 和 MCP 都不发送 Authorization。它只适合 localhost、可信 LAN 和临时 demo；open anonymous caller 共享一个 demo current-session principal。
- **Shared-key quick start mode:** `server up` 默认不允许匿名访问，并启用 shared-key quick start。agent 和 GPT/MCP 使用同一个 Bearer key，server 按 `shared_key_hash` 分组；同 key group 可见，不同 key group 隔离。shared-key 不是 admin，也不是 managed user。
- **Managed production mode:** 使用 pairing、`setup single-user`、`wc_pat_*` 和 `wc_agent_*`。它适合多用户、可撤销 token、scope 和生产审计，不应该被 shared-key 替代。

这三层是 onboarding 和 identity 的产品边界。Open demo mode 不是公网安全模式，shared-key quick start 不是 production IAM，managed mode 才是生产部署应收敛到的身份模型。

## Agent Operating Contract

operating contract 应该让模型遵守稳定 workflow：

1. 识别目标 project，并解析为 canonical runtime project id。
2. 编辑前先探查：runtime status、project status、相关文件和 current session state。
3. 优先使用 bounded read/search/diff tools，再考虑 shell。
4. 优先使用 structured edit tools 和 patch validation，再考虑大范围写入。
5. mutation 要最小化，并且必须 project-scoped。
6. shell/job tools 只在必要、受限、且 policy 允许时使用。
7. 保护 secrets，永远不要打印 token values、完整 env files 或 credentials。
8. 跨多次调用的任务要记录或绑定 session。
9. 完成前使用 checkpoints、diffs 和 validation。
10. tool 失败时先缩小请求并检查原因，不要盲目重试。

这份 contract 应该跨 GPT Actions、MCP clients、CLI helpers 和未来 online clients 保持稳定。平台相关提示可以叠加在上面，但核心行为应该是 WebCodex-native 的。

## Tools as a runtime standard library

Runtime tools 应该像标准库一样组织，而不是 flat function table。

建议的概念分层：

```text
core:
  manifest, status, project identity, policy metadata

project:
  list projects, resolve project ids, list files, read files, search text

edit:
  line edits, exact block edits, text edit batches, patch validation, patch apply

git:
  status, diff, diff hunks, git log, restore/discard helpers, show_changes

session:
  start_session, current session binding, session messages, summary, guards

job:
  run_shell, run_job, job_status, job_log, job_tail, bounded async execution

artifact:
  save, inspect, chunked read, generated images, imported files, reports, zips

desktop:
  screenshot, window inventory, input actions, desktop evidence, replay records

checkpoint:
  create, list, show, delete workspace checkpoints

admin:
  register/create projects, token and client management, server operations
```

工具名可以为了兼容保持稳定，但文档、metadata、recommended flows 和未来的 `tool_manifest` 输出应该强化这些概念层级。

## Tool calls as execution IR

模型 prompt 不是 execution plan。WebCodex 应该把 tool calls 看成结构化 intermediate representation：

```text
inspect -> locate -> read -> edit -> diff -> validate -> checkpoint -> report
```

runtime 因而可以理解 risk、policy、scope、ordering 和 observability。这让系统更像 compiler/runtime，而不是函数路由器：

- user request：源码级意图；
- planner：语义分析和任务拆解；
- tool call sequence：execution IR；
- policy/scope checks：类型系统和 borrow rules；
- tool metadata：标准库签名和 risk annotations；
- session ledger：execution trace；
- show_changes and checkpoints：review and rollback support；
- validation tools：test and diagnostics passes；
- final response：build artifact/report。

这个类比不是要求 WebCodex 行为上模仿 Rust，而是一种设计纪律：显式 effects、scoped authority、bounded execution 和 reviewable outputs。

## Safety model as a type system

WebCodex 应该逐步把权限显式化：

```text
&Project       read-only project access
&mut Project   project write access
Job            async execution capability
Artifact       bounded generated/imported object
Checkpoint     restorable workspace snapshot
unsafe         shell, destructive, or admin-class operation
```

当前机制已经在朝这个方向走：

- OAuth scopes 和 tool metadata；
- read-only session mode 和 task guards；
- destructive/consequential annotations；
- agent policy summaries 和 `allowed_roots`；
- project-scoped tool execution；
- redaction 和 bounded output handling。

未来 policy work 应该把 runtime state 划得更清楚：read-only、writable、approval-required、shell-enabled、admin 和 dangerous。目标不是阻止自动化，而是在 agent 跨越权限边界之前让边界可见。

## Runtime optimizer

WebCodex 可以不改变模型本身，只通过 execution ergonomics 提高 agent reliability：

- **Lazy context loading:** 先 search，再只读相关 file ranges。
- **Common subexpression elimination:** 避免重复读同一文件或重复跑同一 status command。
- **Dead work elimination:** 不探查无关文件，不运行无关命令。
- **Memoization:** 在仍然有效时复用 file hashes、git status、search results 和 project manifests。
- **Query planning:** 按 task risk 和 project size 选择 read/search/diff/edit 工具。
- **Backpressure:** 用 bounded logs、tails、pagination 和 summaries，避免倾倒完整输出。
- **Streaming:** 长任务优先用 `job_status` 和 `job_tail`，而不是等待全部输出结束。
- **Checkpointing:** 围绕高风险多步骤修改创建可审查 recovery points。

这些是 runtime 和 tool-design improvements，和新增工具一样重要。

## Artifact bus and evidence artifacts

Artifacts 应该成为横向 runtime bus，而不是狭义文件 helper。长期流转应该是：

```text
ChatGPT upload
  -> WebCodex artifact
  -> agent workspace / desktop session
  -> generated logs, screenshots, builds, reports
  -> WebCodex artifact
  -> user download
```

这同时支撑代码审查、文档转换、GUI 测试、安装器验证、构建排障和 incident reporting。Artifacts 应该携带 provenance 和 retention metadata：session id、project id、source、creator、content type、size、SHA-256、preview support 和 download routing。Desktop screenshots 和 before/after evidence 应该使用与 generated reports、build outputs 相同的 artifact system。

## Capability providers

当前 `ToolKernel` 和 metadata foundation 未来应该支持 provider-style capabilities。Provider 是实现稳定 runtime capabilities 的 backend integration。

例子：

```text
LSP provider:
  code.diagnostics, code.references, code.rename, code.format

Tree-sitter provider:
  code.symbols, code.node_range, syntax-aware edit planning

Git provider:
  status, diff, log, restore/discard, change review

System provider:
  system.status, process listing, service status, port checks

Docker/systemd/nginx/cert providers:
  operations diagnostics and controlled remediation workflows

Artifact providers:
  generated images, PDFs, zips, imported files, reports

Desktop providers:
  screenshot, window_list, focus_window, input control, action_trace

Message providers:
  future email, chat, webhook, or agent-to-agent notifications
```

当存在更高层 capability 时，model-facing surface 不应该暴露 backend implementation details。例如，优先暴露 `code.diagnostics`，而不是 raw LSP JSON-RPC；当 system provider 能安全回答时，优先暴露 `system.service_status`，而不是 arbitrary shell。

## Coding capability direction

WebCodex coding 能力应该通过增强 workspace，而不是只依赖模型变强来提高可靠性。

近期能力：

- canonical project id resolution；
- project-scoped sessions with validation；
- compact tool manifests and recommended flows；
- file range reads with line numbers；
- atomic multi-block edits；
- workspace checkpoints；
- session-aware `show_changes`；
- bounded validation commands。

下一阶段能力：

- code symbols and file outline；
- diagnostics after edits；
- reference and rename support；
- formatter integration；
- compile/test error summarization；
- edit transactions and rollback hints。

LSP 和 Tree-sitter 应该被视为 providers，而不是 public protocol。public protocol 应该保持为稳定 capability names，例如 `code.symbols`、`code.diagnostics` 和 `code.rename`。

## Operations product direction

WebCodex 可能先成为有价值的 AI operations control plane，再成为完整 IDE backend。运维任务通常是状态探查和有限修复：

- runtime status and agent inventory；
- process、port、disk、memory 和 log inspection；
- service status and restart workflows；
- Nginx、certificate、Docker 和 systemd diagnostics；
- deployment smoke tests；
- incident reports and artifact bundles。

这个方向必须 policy-first。Read-only diagnostics 应该和 mutating operations 分开。Restart、delete、deploy、raw shell 和 admin-class operations 应该有明确 scopes、risk metadata 和 approval semantics。

## Desktop Sessions / Computer Use direction

Computer use 应该被视为 WebCodex 未来的 visual execution backend，而不是裸鼠标控制。产品概念是 **WebCodex Desktop Sessions**：可控、可审计、可回放的桌面工程会话。详细战略见 [DESKTOP_SESSIONS.zh-CN.md](DESKTOP_SESSIONS.zh-CN.md)。

有用的循环是：

```text
observe -> decide/propose -> authorize -> act -> verify -> record -> replay/report
```

这个方向覆盖 API、CLI 和 MCP surface 缺失或不足的工程场景：Windows 安装器测试、GUI 应用冒烟测试、依赖浏览器登录态的 workflow、IDE 辅助调试、OBS 或网页构建平台操作、Electron/Qt 测试、远程桌面里的发行版测试，以及游戏或 AI 游戏调试。

Desktop authority 应该和 shell authority 分开。Screenshot 和 window inventory 可以是低风险观察工具；clipboard、keyboard、mouse 和 autonomous visual loops 需要更强 session policy 和显式 approval。截图应该成为 evidence artifacts，关键动作可以形成 `before.png`、`action.json`、`after.png` 和 `observation.md`。默认部署姿态应推荐 VM、test account、temporary desktop 和 dedicated OS user，而不是用户主力桌面。

## Multi-agent and open-world direction

WebCodex 的长期扩展方向之一是 shared agent runtime space：

```text
World/session = persistent collaboration context
Agent         = human, GPT, Claude, Gemini, Grok, local worker, or service bot
Capability    = scoped tool/provider access
Artifact      = object created or imported into the world
Event log     = durable history of actions and messages
Invite link   = controlled entry into a scoped world/session
Policy        = role, permission, approval, and isolation boundary
```

这可以支持类似游戏的实验，但同一抽象也能支持实际工程 workflow：builder/reviewer/operator agents、shared artifacts、deployment rooms、incident rooms 和 long-running maintenance sessions。

当前 session recorder、message board、artifacts、jobs、project identity、OAuth2 和 tool metadata 都是这个未来的早期积木。在 core runtime contract、policy model 和 provider model 稳定之前，它应该保持为长期方向。

## Current development signal

最近 WebCodex 的工作已经在指向这套架构：

- OAuth2 和 client authorization 让平台不再局限于单一 PAT workflow。
- Tool metadata 和 `ToolKernel` 把 tool execution 推向 policy-aware runtime layer。
- Session ledgers、message boards、current-session bindings 和 task guards 形成 harness-like execution trace。
- `show_changes`、git log 和 checkpoints 提高 reviewability 和 recovery。
- `tool_manifest` 让 runtime introspection 更 compact、ergonomic。
- `apply_text_edits` 和 line-edit tools 减少对 shell-based source rewriting 的依赖。
- Artifact read/write tools 为 generated media、imported files、reports 和 future world objects 做准备。
- Lightweight onboarding 已经形成 open demo、shared-key quick start 和 managed production 三层边界。
- Desktop Session 设计让未来 Computer Use 能落进 runtime，而不是退化为 raw coordinate clicking。

下一步不是继续让这些能力变成互不相关的 utilities，而是要让它们在同一架构下保持一致。

## Near-term priorities

1. 完成 project identity ergonomics：resolver、validation 和清晰 ambiguity errors。
2. 把 open demo、shared-key quick start、managed production 和 GPT Action/MCP setup 作为 first-class entry points 持续完善文档。
3. 强化 `tool_manifest`、`ToolMetadata` 和 recommended flows，让模型选择更安全的工具。
4. 谨慎扩展 session semantics：persistence、message board、guards 和 current-session rules 必须保持一致。
5. 先产品化 artifact bus，再做大范围 desktop automation。
6. 先做 policy-first operations capabilities，再做危险 remediation tools。
7. 先设计 provider abstractions，再实现 LSP、Tree-sitter、systemd、Docker、messaging 或 desktop providers。
8. 把 design documents 当作 architecture constraints，而不是 marketing text。

## Non-goals

本文档不要求立即实现：

- 完整 LSP bridge；
- Tree-sitter indexing；
- plugin marketplace；
- multi-agent open-world hosting；
- unrestricted computer use 或 generic RPA；
- image generation 或 message sending；
- read-only shell policy redesign；
- 替代现有 GPT Action 或 MCP compatibility。

近期目标是 coherence：保持兼容的同时塑造 runtime，让这些未来能力有清晰的落点。
