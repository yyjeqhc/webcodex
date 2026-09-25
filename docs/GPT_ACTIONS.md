# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Use GPT Actions when a Custom GPT needs the Server's OpenAPI compatibility integration. Use [MCP](MCP.md) when the client supports MCP directly; MCP remains the primary ChatGPT integration.

The schema at `/openapi.json` follows one canonical Adaptive Runtime routing policy:

- every runtime Server projects the canonical Adaptive Runtime model contract;
- project-scoped `share` / `run` credentials restrict authority and visibility without defining another Action surface.

## Import the schema

Import:

```text
https://your-domain.example/openapi.json
```

ChatGPT requires public HTTPS. Configure API-key authentication as HTTP Bearer and use a generated user token (`wc_pat_*`). Runner tokens (`wc_agent_*`) are Runner-transport credentials and must not be placed in a GPT.

After upgrading a Server that used the older generic GPT Actions schema, **re-import `/openapi.json`**. The generic operation names are now the canonical WebCodex runtime tool names rather than the retired camelCase Action vocabulary.

## Generic runtime Server

Generic GPT Actions do not have an independent tool registry. They are a constrained OpenAPI projection of the same `ToolDefinition` authority used by Adaptive Runtime MCP:

```text
ToolDefinition
  -> Adaptive Runtime direct tools
      -> direct GPT Action operations
  -> Adaptive Runtime long tail
      -> call_runtime_tool
```

A tool marked Adaptive Direct is automatically a direct GPT Action unless its canonical definition explicitly declares that GPT Actions cannot represent its protocol semantics. Adding, removing, or re-ranking Adaptive Direct tools therefore updates GPT Actions automatically; there is no separate GPT Action rank or operation list.

Direct operations use canonical snake_case names and canonical input contracts. Examples include `work_on_project`, `runtime_status`, `tool_manifest`, `search_project_texts`, `read_files`, `apply_text_edits`, `run_process`, `run_script`, `run_detached_process`, `run_shell`, `observe_jobs`, `list_jobs`, `cargo_check`, `cargo_test`, `git_review_summary`, `git_diff_hunks`, and `show_changes` when those tools are currently Adaptive Direct; the closeout helpers `workspace_hygiene_check` and `finish_coding_task` are model-visible gateway tools.

Long-tail model-visible tools use the single gateway:

```json
{
  "tool": "apply_patch",
  "arguments": {
    "project": "agent:runner:project",
    "patch": "..."
  }
}
```

The operation name is `call_runtime_tool`. It accepts only `tool` and `arguments`; there is no `params` envelope and no flattened union of every runtime tool's fields. A currently direct tool should use its own direct Action operation. Model-hidden tools, unknown tools, and tools explicitly unsupported by GPT Actions fail closed.

Protocol-only MCP presentation is not emulated through Actions. For example, Goal Plan / Agent continuation / Work Result App presentation, MCP ResourceLink artifact export, and continuation Endpoint rotation that depends on a separately authorized MCP Host binding are excluded from GPT Actions.

All authorization still runs through the normal ToolRuntime kernel. The Action adapter does not own OAuth/PAT scope policy, Project authority, permission/approval decisions, Runner capability checks, path policy, Session fences, retry rules, or destructive semantics. `x-openai-isConsequential` is only a host UX hint derived from canonical tool approval metadata.

### Descriptions and the 300-character Action limit

Custom GPT Actions reject operation/tool descriptions above 300 characters. WebCodex keeps the canonical MCP description budget independent and larger. An Action reuses the canonical description when it fits; only over-limit tools carry a short presentation override on their canonical `ToolDefinition`. Generated schema/property descriptions are bounded by a presentation-only projector that changes description text, not JSON-schema shape.

### OpenAPI import size

The Custom GPT importer also rejects OpenAPI schemas at 1 MB. WebCodex therefore keeps the generated generic Action document below an internal 800,000-byte JSON budget with CI coverage for compact and pretty-printed serialization. Direct Action request schemas remain the canonical `ToolSpec.input_schema`; response schemas use the real compact `ToolResult` envelope with a generic `output` field instead of inlining each potentially large canonical output schema. This changes only the OpenAPI presentation contract: actual runtime JSON results and canonical/MCP output schemas are unchanged.

### Conversation file import

`import_conversation_files_to_project` remains a direct generic Action when it is Adaptive Direct. ChatGPT supplies `openaiFileIdRefs`; the HTTP adapter converts the host's Action file-reference shape to the canonical internal shape and attaches private GPT Action host provenance. The model cannot set that provenance itself.

MCP host-file import remains a separate trusted provenance path. Normal network-accessible Servers require the configured trusted OAuth MCP client; an explicitly opted-in loopback-only OpenAI Secure Tunnel deployment may instead trust an allowed local tunnel credential (a normal user API token or the configured Server bootstrap credential used by the regular Desktop Tunnel). The Action and MCP provenance modes share canonical authorization but are not interchangeable.

## Project-scoped local `share` / `run`

A Server launched by `webcodex share` or `webcodex run` uses the same generic Adaptive Runtime OpenAPI projection as an ordinary Server. Project-scoped authentication limits the caller to its ProjectGrant; it does not switch schema generation to a separate Connector capability registry.

Suggested Custom GPT instructions can therefore use the canonical runtime workflow:

```text
Use work_on_project to establish the exact Project and Workflow Session.
Read/search before editing; use the canonical runtime tool names returned by discovery.
Use work_on_project(mode=worktree) only when an isolated managed worktree is wanted.
Use focused validation and observe the same Job when work continues asynchronously.
Review with show_changes and close with finish_coding_task.
Do not infer Project or Session authority from chat, credential, or guessed ids.
```

The retired ProjectConnector Action names and host-side `webcodex task` review workflow are not projected as compatibility aliases.

## Management and safety

The OpenAPI model surface intentionally excludes users, API tokens, Runner tokens, pairing/enrollment, setup, doctor, npm, server management, and audit endpoints. Use the `webcodex` CLI for those tasks.

Both MCP and GPT Actions use the same ToolRuntime authority model. GPT Actions changes model presentation and HTTP transport only; it never grants authority that the same canonical tool would not have through MCP/runtime execution.

## Related

- [Full Setup](PERSONAL_SETUP.md)
- [Quick Trial](QUICK_START.md)
- [MCP](MCP.md)
- [Authentication](AUTH_MODEL.md)
- [Deployment](DEPLOYMENT.md)
- [SECURITY.md](../SECURITY.md)