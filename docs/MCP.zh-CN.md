# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex 通过 MCP endpoint 暴露与 GPT Actions 相同的 runtime tools。

## Endpoint

```text
https://your-domain.example/mcp
```

## 部署模型

WebCodex 当前提供的是一个远程 MCP endpoint，背后连接 WebCodex runtime tools。`webcodex-agent` 是本地执行 agent，不是 MCP 协议意义上的 client。

在 MCP 术语中，AI host 创建 MCP client 连接，WebCodex server 扮演 MCP server，WebCodex agent 在 server 后面执行项目工作。local stdio MCP-server 注册和外部 MCP-server brokering 属于未来扩展，不是当前 endpoint 的前置条件。

把示例中的 `your-domain.example` 替换成你自己的 WebCodex HTTPS 域名。

## 创建 ChatGPT MCP app / connector

`docs/assets/mcp-*.png` 中的截图展示了 ChatGPT app/connector 流程：

![打开 ChatGPT apps](assets/mcp-1.png)
![选择 webcodex](assets/mcp-2.png)
![配置 MCP URL 和认证](assets/mcp-3.png)
![连接 webcodex](assets/mcp-4.png)

1. 打开 ChatGPT 的 apps/connectors 区域，选择创建或配置 MCP app。
2. 将 app name 设置成容易识别的名称，例如 `webcodex`。
3. 将 MCP server URL 设置为：

   ```text
   https://your-domain.example/mcp
   ```

4. 将 authentication 配置成 HTTP/API key Bearer auth。quick start 使用 shared key；managed mode 使用 `wc_pat_xxx` personal API token。shared-key quick start 不要选择 OAuth。
5. 保存 app，然后在 ChatGPT 提示时连接它。
6. 先用低风险 discovery tools 测试：列出 tools、检查 runtime status、列出 projects，再调用只读 project tool。

## 认证

使用 Bearer authentication。quick start 使用 shared key；managed mode 使用 `wc_pat_xxx` personal API token。静态 Bearer/API-key 认证既可以把任一值作为 `Authorization: Bearer ...` 发送。

OAuth 是独立 flow。OAuth client 字段留空不会变成 no-auth，也不会变成静态 Bearer。Open demo mode 只能用于 Host 明确提供 None / No authentication / no-auth 设置，且 WebCodex server 已用 `--open` 启动的场景。

MCP 不要使用这些凭据：

- `WEBCODEX_TOKEN`：server bootstrap/root/admin credential。
- `wc_acct_xxx`：只供用户 CLI 创建本地 PAT 和 agent token。
- `wc_agent_xxx`：只供 `webcodex-agent` 使用。

生产部署推荐流程是管理员一次性签发 user account credential，然后用户运行 `webcodex-cli token create-local` 在本地生成 `wc_pat_xxx`，服务器只登记其 hash。

## Runtime surface

MCP 和 GPT Actions 共享同一个 `ToolRuntime`。通过 MCP 发起的 tool call 会到达相同 runtime、agent registry、project ids 和 safety boundaries。

常见 MCP tools 包括：

- Discovery / health：`list_tools`、`runtime_status`、`list_projects`、`list_agents`。
- 只读项目检查：`list_project_files`、`read_file`、`search_project_text`、`git_status`、`git_diff`、`git_diff_summary`、`git_diff_hunks`。
- 推荐结构化编辑：`replace_line_range`、`insert_at_line`、`delete_line_range`。
- Patch workflows：`validate_patch`、`apply_patch_checked`。
- 项目命令与 jobs：`run_shell`、`run_job`、`job_status`、`job_log`、`job_tail`。
  `job_status` 默认不返回 `command_preview`；仅在定向调试时传 `include_command_preview=true`。它不会返回 stdout/stderr body。
- Structured Cargo helpers：`cargo_fmt`、`cargo_check`、`cargo_test`。

Codex delegation（`run_codex`）当前已从 MCP `tools/list` 和模型可见 runtime discovery 隐藏/禁用。需要 Codex 时请在 WebCodex 外部运行。legacy `/api/codex/run` endpoint 默认不挂载；只有设置 `WEBCODEX_ENABLE_LEGACY_CODEX_RUN=1` 才恢复旧 endpoint 形状，但不会重新启用 `run_codex`。

已知目标行号时，优先使用 structured line edit tools。多文件修改使用 patch tools。把 `run_shell` 和 `run_job` 当作 diagnostics/build/test fallback，而不是首选源码编辑方式。

Agent-backed project ids 形如：

```text
agent:<client_id>:<project_id>
```

例如：`agent:ubuntu-client:webcodex`。

## 示例客户端配置

具体格式取决于 MCP client。secrets 应使用 placeholders 或环境变量，不要把真实 token 提交进配置文件。

```json
{
  "mcpServers": {
    "webcodex": {
      "url": "https://your-domain.example/mcp",
      "headers": {
        "<bearer-auth-header-name>": "Bearer ${WEBCODEX_PAT}"
      }
    }
  }
}
```

其中 `WEBCODEX_PAT` 在 quick start 中可保存 shared key；在 managed mode 中保存由 `webcodex-cli token create-local` 生成的 `wc_pat_xxx`。

## 常见错误

### 401 Unauthorized

Token 缺失、格式错误、过期、已撤销，或 server 不认识。确认 quick start 的 shared key 与 agent/server 一致；managed mode 则生成新的 `wc_pat_xxx`，并确认 MCP client 读取的是正确环境变量。

### 403 Forbidden

Token 有效，但缺少请求工具或项目操作所需 scope。为当前工作流创建具备所需 scopes 的 PAT。

### Token 类型错误

MCP 的静态 Bearer 可以使用 quick-start shared key 或 managed `wc_pat_xxx`。`WEBCODEX_TOKEN`、`wc_acct_xxx` 和 `wc_agent_xxx` 分别属于其他 surface。

### Agent offline

Server 在线，但所选 `client_id` offline 或 stale。启动 `webcodex-agent`，并检查 `runtime_status` 或 `list_agents`。

### Project not registered

Agent 在线，但请求的 `agent:<client_id>:<project_id>` 不存在。添加顶层 agent `projects.d/*.toml` 文件，包含 `id` 和 `path`，然后重启或刷新 agent。
