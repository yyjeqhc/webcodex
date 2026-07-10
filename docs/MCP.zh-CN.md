# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

如果 client 支持 remote MCP，使用 MCP。
如果你在构建 Custom GPT，使用 GPT Actions。
两者调用同一个 WebCodex ToolRuntime。

WebCodex 扮演 remote MCP server。WebCodex agent 不是 MCP 协议里的 client；它是 WebCodex server 后面的本地执行 worker。

## Endpoint

```text
https://your-domain.example/mcp
```

本地 smoke test：

```text
http://127.0.0.1:8080/mcp
```

Hosted client 通常要求 HTTPS。把 `your-domain.example` 换成你自己的 WebCodex 域名。

## 认证

MCP client 使用 Bearer/API-key authentication：

```text
Authorization: Bearer <shared key>
```

第一次评估时，使用和 `webcodex-cli connect --key` 相同的长随机 Bearer 值。在 shared-key quick-start 模式下，这个值不会被预先登记；它会通过 hash 标识一个轻量 shared-key group。agent 和客户端必须使用同一个值。MCP 不要使用 bootstrap/admin、account 或 agent tokens。

生产环境使用 scoped user tokens 或 OAuth。完整 credential model 见 [AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)。

不要把真实 token 写进提交的 MCP config。优先使用环境变量或 client secret store。

## 创建 ChatGPT MCP App / Connector

`docs/assets/mcp-*.png` 截图是 ChatGPT app/connector 流程的 UI 路标：

![打开 ChatGPT apps](assets/mcp-1.png)
![选择 webcodex](assets/mcp-2.png)
![配置 MCP URL 和认证](assets/mcp-3.png)
![连接 webcodex](assets/mcp-4.png)

1. 打开 ChatGPT apps/connectors，创建或配置 MCP app。
2. 起一个容易识别的名字，例如 `webcodex`。
3. MCP server URL 填 WebCodex `/mcp` endpoint。
4. 配置 HTTP/API-key Bearer authentication。
5. 保存并连接 app。
6. 先跑 discovery 和只读 project calls，再进入写任务。

## 第一次检查

让 client 先跑低风险检查：

1. compact 或 summary 形态的 `runtime_status`。
2. `list_projects`。
3. 用 `project_overview` 获取陌生项目的有界结构化概览。
4. 对概览返回的关键路径做有界 `read_file`。
5. `show_changes`，并设置 `include_diff=false`。

project id 应该长这样：

```text
agent:<client_id>:<project_id>
```

prompt 里写完整 project id，避免模型选错仓库。

## 默认 Coding Loop

使用这个 workflow，不要让模型自行发明 shell session：

```text
startup:
  start_coding_task

inspect:
  project_overview
  list_project_files
  search_project_text
  read_file

edit:
  replace_line_range
  insert_at_line
  delete_line_range
  apply_text_edits
  apply_patch_checked

validate:
  validate_patch
  cargo_check
  cargo_test
  cargo_fmt

review:
  show_changes
  git_diff_hunks
  workspace_hygiene_check

finish:
  finish_coding_task
  session_handoff_summary
```

`project_overview` 只返回确定性的结构与项目相对路径元数据。它不读取文件
内容，也不执行 semantic/LSP analysis；随后仍应使用 `read_file` 查看 README、
规则、manifest 或源码。

`start_coding_task` 返回 session id，后续 review 和 finish tools 可以继续使用。`finish_coding_task` 是完成任务的推荐收口工具；`session_handoff_summary` 用于把上下文交给另一个 operator 或后续 client。

## 只读 LSP 导航

当前 LSP tools：

- `lsp_status`
- `document_symbols`
- `goto_definition`
- `find_references`

Phase 1 只支持 Rust。这些 tools 是只读的，只在已注册 workspace 内工作，也不会
导航到 dependencies。它们不提供 client-controlled document synchronization，也不
提供任何 write operation。可用性取决于所选 agent 是否声明
`lsp_read_only_navigation`。

当 `start_coding_task.semantic_navigation.recommended=true` 时，推荐：

```text
document_symbols
→ goto_definition / find_references
→ read_file
```

semantic navigation 不可用时，使用：

```text
project_overview
→ search_project_text
→ read_file
```

## Advanced / Escape-Hatch Tools

```text
run_shell:
  bounded escape hatch, not default editing or validation path

run_job:
  for explicit async jobs, not default coding loop

artifact / checkpoint / cleanup:
  advanced workflow tools
```

这些工具有用，但不应该成为模型第一选择。优先使用结构化 read、edit、validation、review 和 finish tools。

## Tool Discovery

MCP 可以直接暴露 runtime tools。不要把完整工具目录塞进每个 prompt。日常发现工具时，使用 compact manifest 或 focused category，然后按上面的默认 coding loop 工作。

只有调试 client/tool schema behavior 时，才使用完整 schema-oriented discovery。

## 示例客户端配置

具体格式取决于 MCP client：

```json
{
  "mcpServers": {
    "webcodex": {
      "url": "https://your-domain.example/mcp",
      "headers": {
        "Authorization": "Bearer ${WEBCODEX_MCP_BEARER}"
      }
    }
  }
}
```

使用 `WEBCODEX_MCP_BEARER` 表示 MCP client 中配置的 Bearer 值。
它可以是 quick-start shared key，也可以是生产 user token。
它不应该是 server bootstrap `WEBCODEX_TOKEN`、account credential 或 agent token。

## 常见错误

### 401 Unauthorized

token 缺失、格式错误、过期、已撤销，或 server 不认识。确认 MCP client 正在发送预期 Bearer value。

### 403 Forbidden

token 有效，但缺少请求工具或项目操作所需 scope。使用面向 runtime/project/job access 的 token。

### Agent Offline

server 可达，但所选 agent 未连接。启动 `webcodex-agent` 并检查 `runtime_status`。

### Project Not Registered

agent 在线，但请求的 `agent:<client_id>:<project_id>` 不存在。通过 agent connection flow 注册项目，然后重试 `list_projects`。

### Response Too Large

使用 compact runtime status、focused manifest discovery、有界文件范围、`show_changes(include_diff=false)`，以及 summary-only finish 或 handoff calls。

## 相关文档

- 快速开始：[QUICK_START.zh-CN.md](QUICK_START.zh-CN.md)
- Demo 工作流：[DEMO.zh-CN.md](DEMO.zh-CN.md)
- GPT Actions：[GPT_ACTIONS.zh-CN.md](GPT_ACTIONS.zh-CN.md)
- 认证模型：[AUTH_MODEL.zh-CN.md](AUTH_MODEL.zh-CN.md)
- 安全：[../SECURITY.md](../SECURITY.md)
