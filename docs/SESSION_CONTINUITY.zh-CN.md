# 跨账号接续任务与模型评审

[English](SESSION_CONTINUITY.md) | [简体中文](SESSION_CONTINUITY.zh-CN.md)

WebCodex 将项目工作保存在本地，不依赖承载对话的 ChatGPT 账号。新对话可以发现有权访问的 Workflow Session，读取已保存的任务、决策、进度和验证证据，再显式接续。配置好的模型 API 后端也可以评审同一份已保存的上下文。

## 工作过程中保存上下文

让当前模型把关键决策和需要接续的进度记入当前 WebCodex Session，例如：

> 工作过程中，把迁移目标、已确认的约束、决策和剩余工作保存在 WebCodex。切换账号前保存一份简明交接。

模型使用 `post_session_message`：`kind=decision` 保存已确认决策，`kind=progress` 保存实现状态和剩余工作，按需使用 `todo`、`risk`、`question` 或 `guidance`。稳定的 `delivery_key` 使相同请求在消息及重放记录保留期间可以跨重启安全重试；发送结果不确定时，应使用同一 key 和相同正文核对。

已经使用持久 Goal 的任务，在重要节点调用 `checkpoint_goal`，并让 Goal 与准确的 Session 保持显式关联。执行和验证工具在显式提供授权 recorder 时记录各自的证据；消息中的“测试通过”仍只是报告。

WebCodex 接收工具参数和结果，不会获得宿主对话中的每句话。未保存的讨论和模型内部推理无法恢复。不要将凭据写入消息。保留范围和截断会明确标注；Session 是有界的项目历史，不是无限对话档案。

## 在新对话中接续

将新对话连接到同一个 WebCodex Server 和有权访问的 Project，然后说：

> 继续这个项目的迁移工作。找到我保存的 WebCodex Session；有多个候选时展示给我选择，读取选定任务的交接，再检查当前文件和测试后继续。

恢复过程如下：

1. 不知道项目身份时，通过 `list_projects` 发现授权 Project。调用 `list_sessions(project)`，可用 `lifecycle` 筛选，或用 `offset`、`limit` 分页。只统计并返回准确 Project 下、属于调用者 authority group 的 Session。
2. 选择一个准确的 `session_id` 或返回的 `session_ref`。列表顺序仅用于展示；标题、最新一行或“继续”两个字都不能替代存在歧义时的任务选择。
3. 读取 `session_handoff_summary(session_id, project)`。紧凑的 `handoff_brief.task.decisions` 和 `recent_progress` 带回最近记录的消息、状态、替代关系，以及覆盖范围和截断信息。默认 brief 小于 8 KiB；需要更多保留的待办、风险或消息详情时，使用 diagnostic handoff 或授权的 `list_session_messages`。行动前复查当前 Git、文件及过期的测试结论。
4. Active Session 可通过 `work_on_project(project, session_id, instruction, ...)` 显式恢复；当前模型需要项目规则和工作流时，请求 `_wc.context=["project.instructions", "webcodex.workflow"]`。Closed Session 可供恢复读取，但不能重新打开；应创建新任务并显式带入选定的上下文。

发现和交接都是读取，不会自动选择、创建、恢复或关闭任务。省略 `work_on_project.session_id` 会创建新任务。Goal 恢复有独立授权；存在多个候选时应显式选择。

切换 ChatGPT 账号后，只要两条连接使用同一 canonical WebCodex authority，并保有 Project 权限，就能使用这一流程。不同 WebCodex 用户、shared-key authority 或 project grant，不会因为连接同一 Server 就获得其他所有者的 Session。如果运营者有意使用不同身份，应通过授权渠道传递经用户审阅的摘要；本功能不会扩大授权或导入宿主私有对话。

## 委派给配置好的 API 模型

运营者安装[模型 API ACP 适配器](../integrations/model_gateway/README.md)，为每个固定 API、模型和凭据受众配置一个 Runner provider。需要 Python 3.10+。可以配置多个 OpenAI Responses 或 OpenAI 兼容 chat completions 后端。Token 保存在 Runner 显式映射的环境中；模型无法发现 provider 配置和凭据。

MCP 客户端需要已有的 `coding_agent:run` 和 `project:write` 授权才能启动 Run；读取已保存的上下文还需要 `runtime:read` 及来源 Session 的 Project、所有者权限。升级不会扩大现有 OAuth 客户端权限；运营者应先通过正常授权、同意流程为目标客户端配置权限。

让当前模型通过 `work_on_project` 或 `list_runners` 发现准确 Project 的 `coding_agent_providers`，显式选择配置好的 `provider_id`。调用时可直接带入保存的上下文：

```json
{
  "project": "<授权的 project selector>",
  "provider_id": "model-review",
  "idempotency_key": "migration-review-1",
  "instruction": "评审迁移设计和剩余验证，返回具体发现。",
  "context_session_id": "<选定的 session_id 或 session_ref>",
  "timeout_secs": 120
}
```

以上是 `coding_agent_start` 的参数。`context_session_id` 要求独立的 Session 和 `runtime:read` 授权，且必须属于委派的准确 Project。它把有界交接和独立授权的 Goal 上下文作为引用数据注入 prompt；不会恢复来源 Session，也不会选择 recorder。此快照不读取当前文件或 Git。评审需要源码时，在 `instruction` 中加入明确选择的片段；合并后的 prompt 必须满足现有 64 KiB Run 输入上限。

用 `coding_agent_observe` 观察返回的 Run：首次读取省略 token，以包含已保留的输出；后续复用这次观察返回的 observation token，只读取新事件。需要取消时用 `coding_agent_cancel`。发起结果不确定时，观察同一 Run，不要发起替代调用。上下文属于发起指纹；复用 key 时保存的快照发生变化，会返回冲突。没有自动额度回退或模型切换。

适配器将明确提供的文本发送到运营者选择的接口，流式返回有界模型文本，只有正常完整响应才算成功。它不访问文件、不修改代码、不执行模型工具，也不保存模型对话。凭据必须有权使用配置的 API；账号登录本身不代表这项资格。审阅结果后，如果希望它在下一次账号或模型切换后保留，应显式将有用发现保存为 Session 决策或进度。委派模型的结论不能代替原生验证或批准证据。

现有 [ACP Run 契约](agent/acp-coding-agent-run.md) 负责授权、持久化调度、观察和取消。MCP 通过 canonical gateway 提供此流程；GPT Actions 的冻结工具集合保持不变。
