# GPT Actions

[English](GPT_ACTIONS.md) | [简体中文](GPT_ACTIONS.zh-CN.md)

Use MCP if your client supports remote MCP.
Use GPT Actions if you are building a Custom GPT.
Both surfaces call the same WebCodex ToolRuntime.

GPT Actions gives a Custom GPT a focused OpenAPI surface for WebCodex runtime tools. It is the right path when you want a ChatGPT Custom GPT rather than a generic remote MCP connector.

## Schema URL

```text
https://your-domain.example/openapi.json
```

For local inspection:

```text
http://127.0.0.1:8080/openapi.json
```

ChatGPT Actions require a public HTTPS URL for actual use.

## Create A GPT Action In ChatGPT

The screenshots in `docs/assets/gpt-action-*.png` are UI landmarks. ChatGPT may move buttons over time, but the flow is the same.

1. **Open or create a GPT.**

   ![Open GPT editor](assets/gpt-action-1.png)

2. **Enter the GPT configuration screen.**

   ![Configure GPT](assets/gpt-action-2.png)

3. **Open Actions and add an Action.**

   ![Add an Action](assets/gpt-action-3.png)

4. **Configure Action authentication.**

   ![Set Action authentication](assets/gpt-action-4.png)

   Choose API-key or HTTP authentication, set the auth type to Bearer, and paste the first-evaluation shared key. In shared-key quick-start mode, this value identifies the same lightweight group as the agent key by hash. Do not use bootstrap/admin, account, or agent tokens.

5. **Import the OpenAPI schema.**

   ![Import OpenAPI schema](assets/gpt-action-5.png)

   Import the schema URL from your WebCodex server. If ChatGPT asks for a privacy policy URL, use your own product or deployment policy URL and do not put secrets in it.

6. Save the Action.
7. Test `getRuntimeStatus`, then `listProjects`, then a read-only project call.
8. Use mutation tools only after a read-only task has finished cleanly.

## Authentication

Configure GPT Actions with Bearer/API-key authentication.

For the first evaluation, use the same long random Bearer value that you used with `webcodex-cli connect --key`. In shared-key quick-start mode, this value is not pre-enrolled; it identifies a lightweight shared-key group by hash. Use the same value for the agent and the client. For production, use scoped user tokens or OAuth. See [AUTH_MODEL.md](AUTH_MODEL.md) for the full credential model.

Do not paste bootstrap/admin, account, or agent tokens into GPT Actions.

Pairing, token creation, agent enrollment, server setup, and other management tasks belong in `webcodex-cli`, not GPT Actions.

## Tool Surface

GPT Actions exposes a focused public operation surface and a generic `callRuntimeTool` operation for runtime tools. It intentionally does not expose admin, setup, pairing, token-management, agent-token, server-management, or audit endpoints.

When using `callRuntimeTool`, pass the runtime tool name and flattened top-level fields expected by the OpenAPI schema. Use focused discovery rather than sending the full tool catalog into a model prompt.

MCP and GPT Actions share the same runtime, project ids, session recording, agent bridge, and safety boundaries.

For an unfamiliar project, call `project_overview` through `callRuntimeTool`
after `listProjects`. It returns bounded structure and project-relative path
metadata only; it does not read contents or perform semantic/LSP analysis. Use
`read_file` afterward for the specific README, rules, manifest, or source path.

## Default Coding Loop

Use this loop for Custom GPT coding tasks:

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

The intended closeout order is:

```text
start_coding_task -> inspect -> edit -> validate -> show_changes -> workspace_hygiene_check -> finish_coding_task
```

Use `session_handoff_summary` when another operator or client needs to continue the task.

## Advanced And Escape-Hatch Tools

```text
run_shell:
  bounded escape hatch, not default editing or validation path

run_job:
  for explicit async jobs, not default coding loop

artifact / checkpoint / cleanup:
  advanced workflow tools
```

Shell and job tools can execute project commands through the agent. Use them only when a structured validation helper is not enough, and review the resulting workspace state before finishing.

Artifact, checkpoint, and cleanup tools support advanced workflows. They are not replacements for structured source edits or normal code review.

## First Safe Prompt

```text
Use WebCodex on project agent:<client_id>:<project_id>.
Start a coding task, inspect README.md, summarize the project, show changes
without a diff, run workspace hygiene, and finish. Do not edit files.
```

After that succeeds, try one small, reversible edit on a disposable branch.

## Common Errors

### Schema Import Fails

Confirm the server is reachable over public HTTPS and `/openapi.json` returns the WebCodex schema.

### Auth Fails

Confirm the Action uses Bearer/API-key auth and the token is intended for GPT Actions or runtime access.

### GPT Chose The Wrong Project

Put the full `agent:<client_id>:<project_id>` in the prompt. Ask the GPT to call `listProjects` or `list_projects` before reading or editing.

### Response Too Large

Use compact runtime status, focused tool manifest discovery, bounded file reads, `show_changes(include_diff=false)`, and summary-only finish or handoff outputs.

### Shell Is Suggested Too Early

Redirect the GPT to the default loop: inspect, structured edit, structured validation, review, and finish. Use shell/job only as an explicit escape hatch.

## Related Docs

- Quick Start: [QUICK_START.md](QUICK_START.md)
- Demo workflow: [DEMO.md](DEMO.md)
- MCP: [MCP.md](MCP.md)
- Concepts: [CONCEPTS.md](CONCEPTS.md)
- Auth model: [AUTH_MODEL.md](AUTH_MODEL.md)
- Security: [../SECURITY.md](../SECURITY.md)
