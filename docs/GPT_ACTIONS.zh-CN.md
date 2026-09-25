# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Custom GPT 需要通过 Server 的 OpenAPI 兼容集成调用 WebCodex 时使用 GPT Actions。客户端直接支持 MCP 时优先使用 [MCP](MCP.zh-CN.md)；MCP 仍然是 ChatGPT 的主要接入方式。

`/openapi.json` 使用同一套 canonical Adaptive Runtime routing policy：

- runtime Server 投影 canonical Adaptive Runtime model contract；
- project-scoped `share` / `run` credential 只收窄 authority/visibility，不定义第二套 Action surface。

## 导入 schema

导入：

```text
https://your-domain.example/openapi.json
```

ChatGPT 需要公网 HTTPS。API-key 认证配置为 HTTP Bearer，使用生成的 user token（`wc_pat_*`）。Runner token（`wc_agent_*`）只用于 Runner transport，不能放进 GPT。

如果 Server 以前使用过旧 generic GPT Actions schema，升级后请**重新导入 `/openapi.json`**。新的 generic operation 名称直接使用 WebCodex canonical runtime tool 的 snake_case 名称，不再使用旧 camelCase Action vocabulary。

## 普通 Runtime Server

Generic GPT Actions 不再拥有独立 tool registry。它只是 Adaptive Runtime MCP 所使用同一份 `ToolDefinition` authority 的受限 OpenAPI projection：

```text
ToolDefinition
  -> Adaptive Runtime direct tools
      -> direct GPT Action operations
  -> Adaptive Runtime long tail
      -> call_runtime_tool
```

一个工具被标记为 Adaptive Direct 后，默认会自动成为 direct GPT Action；只有 canonical definition 明确声明 GPT Actions 无法表达其协议语义时才排除。因此以后增删或重新排序 Adaptive Direct 工具时，GPT Actions 会自动跟随，不存在第二套 GPT Action rank 或 operation list。

Direct operation 直接使用 canonical snake_case 名称和 canonical input contract。例如 `work_on_project`、`runtime_status`、`tool_manifest`、`search_project_texts`、`read_files`、`apply_text_edits`、`run_process`、`run_script`、`run_detached_process`、`run_shell`、`observe_jobs`、`list_jobs`、`cargo_check`、`cargo_test`、`git_review_summary`、`git_diff_hunks`、`show_changes` 等会按当前定义自动投影；收尾辅助工具 `workspace_hygiene_check` 与 `finish_coding_task` 是 model-visible long-tail 工具，统一通过 `call_runtime_tool` 调用。

Long-tail model-visible 工具统一通过：

```json
{
  "tool": "apply_patch",
  "arguments": {
    "project": "agent:runner:project",
    "patch": "..."
  }
}
```

operation 名就是 `call_runtime_tool`。它只接受 `tool` 与 `arguments`，不再有 `params`，也没有把所有 runtime tool 参数铺平到顶层的巨大 union。当前 direct tool 应优先调用自己的 direct Action。ModelHidden、未知工具以及显式 GPT-Action-unsupported 工具都会 fail closed。

MCP-only presentation 不会伪装成 Action 能力。例如 Goal Plan / Agent continuation / Work Result App presentation、基于 MCP ResourceLink 的 artifact export，以及依赖另外授权 MCP Host binding 的 continuation Endpoint rotation 都不会出现在 GPT Actions 中。

所有真实授权仍进入同一个 ToolRuntime kernel。Action adapter 不拥有 OAuth/PAT scope、Project authority、permission/approval、Runner capability、path policy、Session fence、retry 或 destructive semantics。`x-openai-isConsequential` 只是从 canonical approval metadata 派生出的 ChatGPT host UX hint，不是权限来源。

### 300 字符 description 约束

Custom GPT Actions 对 operation/tool description 有 300 characters 硬上限。WebCodex 保持 canonical MCP description 的更大预算不变：canonical description 不超过 300 时直接复用；超过时只在同一个 `ToolDefinition` 上提供简短 Action presentation override。Schema/property description 使用 presentation-only bounded projector，只改变 description 文本，不改变 JSON Schema 的 type、required、enum、oneOf/anyOf、约束或对象形状。

### OpenAPI 导入体积

Custom GPT importer 还会拒绝达到 1 MB 的 OpenAPI schema。WebCodex 因此为 generic Action document 保留内部 800,000-byte JSON 预算，并在 CI 中同时检查 compact 与 pretty-printed serialization。Direct Action request schema 继续完整使用 canonical `ToolSpec.input_schema`；response schema 只描述真实的 compact `ToolResult` envelope，并把 `output` 保持为 generic，而不再为每个 operation 内联可能很大的 canonical output schema。这只改变 OpenAPI presentation contract；实际 runtime JSON result 以及 canonical/MCP output schema 都不变。

### 对话文件导入

`import_conversation_files_to_project` 在它属于 Adaptive Direct 时仍是 direct generic Action。ChatGPT 提供 `openaiFileIdRefs`；HTTP adapter 把 Action host file-reference shape 转为 canonical 内部 shape，并附加私有 GPT Action host provenance。模型 JSON 自己不能设置这个 provenance。

MCP host-file import 保留独立的 trusted provenance 路径。普通 network-accessible Server 继续要求配置过的 trusted OAuth MCP client；显式 opt in 的 loopback-only OpenAI Secure Tunnel 部署可以改为信任允许的本地 tunnel credential（普通 user API token，或 Desktop regular Tunnel 使用的已配置 Server bootstrap credential）。Action 与 MCP 两种 provenance 模式共享 canonical authorization，但不能互相伪造。

## Project-scoped local `share` / `run`

`webcodex share` 或 `webcodex run` 启动的 Server 使用与普通 Server 相同的 generic Adaptive Runtime OpenAPI projection。Project-scoped authentication 只把调用方限制在自己的 ProjectGrant，不会切换到单独的 Connector capability registry。

Custom GPT 可以使用 canonical runtime workflow：

```text
使用 work_on_project 建立精确 Project 与 Workflow Session。
先 read/search 再 edit；使用 discovery 返回的 canonical runtime tool 名称。
只有用户需要隔离 managed worktree 时才使用 work_on_project(mode=worktree)。
validation 或 command 继续异步运行时观察同一个 Job。
使用 show_changes 审查，并用 finish_coding_task 收尾。
不要从 chat、credential 或猜测的 id 推导 Project/Session authority。
```

旧 ProjectConnector Action 名称与 host-side `webcodex task` review workflow 不再作为 compatibility alias 投影。

## 管理与安全

OpenAPI model surface 有意排除 users、API token、Runner token、pairing/enrollment、setup、doctor、npm、server 管理与 audit endpoint。这些请使用 `webcodex` CLI。

MCP 与 GPT Actions 使用同一个 ToolRuntime authority model。GPT Actions 只改变 model presentation 和 HTTP transport，不会获得同一个 canonical tool 在 MCP/runtime 执行路径中没有的权限。

## 相关文档

- [完整使用指南](PERSONAL_SETUP.zh-CN.md)
- [快速试用](QUICK_START.zh-CN.md)
- [MCP](MCP.zh-CN.md)
- [认证模型](AUTH_MODEL.zh-CN.md)
- [部署指南](DEPLOYMENT.zh-CN.md)
- [SECURITY.md](../SECURITY.md)