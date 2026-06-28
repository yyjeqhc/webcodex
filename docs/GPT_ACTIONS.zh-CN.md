# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

WebCodex 在以下地址为 ChatGPT GPT Actions 暴露精简的 OpenAPI schema：

```text
GET /openapi.json
```

GPT Actions 和 MCP 共享同一个 `ToolRuntime`。GPT Actions 提供 typed REST operations；MCP 提供 MCP framing。

## 在 ChatGPT 中创建 GPT Action

`docs/assets/gpt-action-*.png` 中的截图展示了当前 ChatGPT UI 流程：

![打开 GPT editor](assets/gpt-action-1.png)
![配置 GPT](assets/gpt-action-2.png)
![添加 Action](assets/gpt-action-3.png)
![设置 Action 认证](assets/gpt-action-4.png)
![导入 OpenAPI schema](assets/gpt-action-5.png)

1. 打开 ChatGPT，创建或编辑一个 GPT，并进入 GPT 配置页面。
2. 打开 **Actions** 区域，选择创建新的 Action。
3. 在 **Authentication** 中选择 API key / HTTP auth，把 auth type 设置为 **Bearer**，并粘贴 `wc_pat_xxx` personal API token。不要使用 `WEBCODEX_TOKEN`、`wc_acct_xxx` 或 `wc_agent_xxx`。
4. 在 schema/OpenAPI 字段中导入或粘贴：

   ```text
   https://your-domain.example/openapi.json
   ```

5. 如果 ChatGPT UI 要求填写 privacy policy URL，请填写你自己的产品或部署隐私链接；不要在该 URL 中放 secrets。
6. 保存 Action，然后先测试无破坏性的 discovery call，例如 `getRuntimeStatus`，再测试 `listProjects` 和只读 project call，例如 `getProjectGitStatus`。
7. 在 GPT 验证完成前，mutation tools 只应对已知 disposable project 使用。

## 认证

在 GPT Action 设置中配置 Bearer/API-key 认证。secret 值必须是 `wc_pat_xxx` personal API token。

推荐流程：管理员签发一次性的 `wc_acct_xxx` account credential，用户再运行 `webcodex-cli token create-local`，在本地生成 `wc_pat_xxx`，服务器只登记它的 hash。

不要把 `WEBCODEX_TOKEN`、`wc_acct_xxx` 或 `wc_agent_xxx` 粘贴到 GPT Actions 或 MCP 凭据中：

- `WEBCODEX_TOKEN`：只用于 server bootstrap/root/admin。
- `wc_acct_xxx`：只用于用户本地创建 PAT 和 agent token。
- `wc_agent_xxx`：只用于 `webcodex-agent` 连接服务器。

`?token=` 不是 GPT Actions 认证方式。它只允许用于 `/api/agents/ws` 的 WebSocket handshake 兼容场景。

GPT Actions 要求 WebCodex server 有 public HTTPS URL。

## Token 选择

- GPT Actions / MCP / `/api/tools/list` / `/api/tools/call`：使用 `wc_pat_xxx`。
- Server bootstrap 和 emergency admin：使用 `WEBCODEX_TOKEN`。
- 本地自助注册 PAT / agent token：只在 `webcodex-cli token create-local` 或 `webcodex-cli agent-token create-local` 中使用 `wc_acct_xxx`。
- Agent 连接：只在 `webcodex-agent` config 中使用 `wc_agent_xxx`。

如果 GPT Action 配置成 `wc_acct_xxx`，它不能调用 runtime tools，而且会把错误类型的 secret 暴露到错误的 surface。应生成 PAT：

```bash
webcodex-cli token create-local \
  --server https://your-domain.example \
  --user alice \
  --credential "$WEBCODEX_ACCOUNT_CREDENTIAL" \
  --name gpt-action \
  --scopes runtime:read,project:read,project:write,job:run
```

## 工具面

GPT Actions surface 有意小于完整 admin API。它包含 runtime、project、git、patch、file、shell/job 和可选 Codex task operations。

它不暴露 user、API-token、agent-token、pairing/enrollment、setup、doctor、npm、server management 或 audit endpoints，例如：

```text
/api/users/create
/api/tokens/create
/api/agent-tokens/create
/api/pairing/create
/api/pairing/enroll
/api/audit/sessions
```

这些管理任务应使用 `webcodex-cli`。

## 推荐使用流程

1. `getRuntimeStatus` — 检查 runtime health 和 redacted agent policy summary。
2. `getRuntimeStatus`，或通过 `callRuntimeTool` 调用 `list_agents` — 确认有 online agent，并查看 redacted policy summary 或 `agent_instance_id`。
3. `listProjects` — 选择 `agent:<client_id>:<project_id>`。
4. `getProjectGitStatus`、`listProjectFiles`、`readProjectFile`、`searchProjectText` — 编辑前先检查。
5. 已知目标行号时，使用 `callRuntimeTool` 调用 structured line edit tools：`replace_line_range`、`insert_at_line`、`delete_line_range`。
6. 多文件/大范围修改时，先 `validateProjectPatch`，确认后再 `applyProjectPatchChecked`。
7. `writeProjectFile` 只用于新文件或明确的小文件整体覆盖；`replaceProjectFileText` 只用于短的精确字符串替换。
8. `runProjectShellCommand` 或 `startProjectShellJob` 只在文件编辑完成后运行受限命令。
9. `runCodexTask` 是可选高级路径，需要 agent 机器已安装并配置 Codex CLI。

`runCodexTask` 不会启动新的 agent；它只是要求已经连接的 agent 在项目中运行 Codex CLI。

## 可观测性

`getRuntimeStatus` 和通过 `callRuntimeTool` 调用 `list_agents` 可能显示 redacted policy summary：

- `allow_raw_shell`
- `allow_cwd_anywhere`
- `allowed_roots`
- `max_timeout_secs`
- `max_output_bytes`

它们不应暴露 tokens、env values、`Authorization` headers、完整 `agent.toml` 或 shell `init_script` values。

## 兼容说明

`webcodex users`、`webcodex tokens`、`webcodex agent-tokens` 等管理 CLI 兼容命令仍然可用，但当前 setup 和 operations 文档应优先使用 `webcodex-cli`。
