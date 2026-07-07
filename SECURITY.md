# Security Policy

WebCodex is a remote tool execution system for private code. Deploy it as a permissioned bridge: online clients can request bounded tool calls, but repository access and command execution stay behind your self-hosted server and connected agent.

## Supported Versions

WebCodex v0.2.x is an early release. Security fixes are expected to target the latest v0.2.x release unless stated otherwise.

## Security Model Summary

- The online model can only call exposed WebCodex tools.
- Projects are registered by agents; the server does not scan your filesystem.
- Project work runs on the agent machine inside configured project boundaries.
- Structured read, edit, validation, review, and finish tools should be the default workflow.
- Shell and job tools are bounded but powerful and require operator discipline.
- Session, handoff, validation, and hygiene outputs provide review evidence.

## What The Online Model Can Do

Depending on the token, scopes, client surface, session state, and agent policy, the model can ask WebCodex to:

- Discover runtime health and registered projects.
- Read bounded project files and search project text.
- Inspect Git status, diffs, and changed files.
- Apply structured line edits or checked patches.
- Run structured validation helpers such as Cargo format, check, and test.
- Request bounded shell commands or async jobs when the deployment exposes them.
- Produce `show_changes`, `workspace_hygiene_check`, `finish_coding_task`, and `session_handoff_summary` evidence for review.

## What The Online Model Cannot Do

WebCodex does not grant the model:

- Direct filesystem access outside exposed tools.
- Automatic discovery of local repositories from the server.
- Access to projects not registered by an agent.
- Admin, account-management, pairing, token-creation, or agent-token creation through GPT Actions or MCP.
- Permission to bypass path safety, sensitive-path denial, read-only session guards, or agent policy.
- A reason to see secrets, tokens, env files, Authorization headers, or complete agent configs.

## Project Access Model

Projects live on the agent machine. The agent registers allowed directories with the server. The server does not scan your filesystem.

Runtime project ids use:

```text
agent:<client_id>:<project_id>
```

Use narrow allowed roots and register only repositories you intend the selected client to operate. Remove a project from the agent registry, narrow the allowed root, or stop the agent to remove access from that client path.

## Agent Trust Boundary

The agent is trusted to enforce local project policy and execute work on the machine that owns the code. Treat an agent token as a credential for that execution boundary.

Operational guidance:

- Run agents under an OS user appropriate for the repositories they serve.
- Keep project roots narrow.
- Configure shell profiles deliberately; do not inherit broad interactive shell state by accident.
- Do not copy complete agent configs between machines unless that is the intended deployment action.

## Shell And Job Risk

`run_shell` and `run_job` are bounded escape hatches, not the default coding loop. They can run project commands, so they are more powerful than read, edit, or review tools.

Use them only when:

- The command is needed for validation or diagnostics.
- The project, timeout, output limit, and shell profile are appropriate.
- The resulting output will not expose secrets.
- A human can review the command, output summary, and workspace state.

Prefer structured tools first: `read_file`, `search_project_text`, structured edits, `validate_patch`, `cargo_fmt`, `cargo_check`, `cargo_test`, `show_changes`, and `workspace_hygiene_check`.

## Token Handling

Do not expose secrets in prompts, tool output, docs, examples, screenshots, logs, or committed config files.

Never share or commit:

- server bootstrap tokens,
- user API tokens,
- OAuth access or refresh tokens,
- shared keys,
- account credentials,
- agent tokens,
- env files,
- complete `agent.toml` files,
- Authorization headers.

Use the right credential for the right surface:

- GPT Actions, MCP, and runtime API calls use a shared key for quick evaluation or a scoped user token for managed mode.
- Agents use agent tokens.
- Server bootstrap/admin credentials stay server-side.
- Account credentials are for local token creation, not for model-facing clients.

Detailed PAT, shared-key, and OAuth behavior belongs in [docs/AUTH_MODEL.md](docs/AUTH_MODEL.md), not in prompts or README examples.

## Session And Audit Evidence

WebCodex records bounded task evidence for review and handoff:

- session ids and tool status,
- selected project ids,
- validation summaries,
- permission decision summaries,
- workspace review and hygiene summaries,
- finish or handoff verdicts.

These records are intentionally bounded and redacted. They are not a substitute for full infrastructure logs, Git review, or secret scanning. They must not contain raw secrets, full env values, Authorization headers, unbounded stdout/stderr, or complete private file dumps.

## Revoking Access

Use the narrowest revocation that matches the risk:

- Remove or rotate the shared key used for quick evaluation.
- Revoke or rotate a user token used by MCP, GPT Actions, or REST clients.
- Revoke OAuth tokens when using OAuth.
- Remove a project from the agent registry or narrow its allowed root.
- Stop the agent when the client should no longer reach that machine.
- Rotate an agent token if the agent credential may have leaked.
- Rotate server bootstrap/admin credentials if they were exposed.

After revocation, verify with `runtime_status`, `list_projects`, and a read-only client call.

## Reporting Vulnerabilities

Please report vulnerabilities through GitHub Issues on `yyjeqhc/webcodex` or by contacting the maintainer privately through GitHub if the report contains sensitive details.

Do not publish real tokens, env files, complete agent configs, private repository contents, or exploit details in public issues. Use placeholders and minimal reproduction steps.

## Known Limitations

WebCodex v0.2.x is intended for controlled self-hosted environments. It is not a hosted SaaS, not a full identity provider, and not a replacement for normal code review, Git hygiene, endpoint hardening, or least-privilege operating-system policy.
