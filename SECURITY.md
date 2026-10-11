# Security Policy

WebCodex is a self-hosted tool runtime for registered directories, files, and code. The Server authenticates and routes tool calls; the connected Runner executes them on the machine that owns the files. Requested file contents and command output can be returned to the online client, so local execution does not mean that all project data stays local.

## Supported Versions

Security fixes are expected to target the latest stable release unless stated
otherwise. Check [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases/latest)
for that version; v0.4.6 is the published stable baseline for the current v0.5
development transition. `main` documents development behavior and is not itself
a published release. Installation/data compatibility is a separate contract:
see [Compatibility policy](docs/compatibility-policy.md) for upgrade sources.

## Security Model Summary

- The online model can only call exposed WebCodex tools.
- Projects are registered by Runners; the Server does not scan your filesystem.
- Structured project operations enforce registered roots and path policy. Process tools run with the Runner OS account's permissions; project roots are not an operating-system sandbox.
- Structured read, edit, validation, review, and finish tools should be the default workflow.
- Shell and job tools are bounded but powerful and require operator discipline.
- Session, handoff, validation, and hygiene outputs provide review evidence.

## What The Online Model Can Do

Depending on the token, scopes, client surface, Session state, and Runner policy, the model can ask WebCodex to:

- Discover runtime health and registered projects.
- Read bounded project files and search project text.
- Inspect Git status, diffs, and changed files.
- Apply structured line edits or checked patches.
- Run structured validation helpers such as Cargo format, check, and test.
- Request bounded shell commands or async jobs when the deployment exposes them.
- Produce `read_workspace_changes`, `check_workspace_hygiene`, `finish_coding_task`, and `read_session_handoff` evidence for review.

## What The Online Model Cannot Do

WebCodex does not grant the model:

- Direct filesystem access outside exposed tools.
- Automatic discovery of local repositories from the server.
- Project-tool access to unregistered or unauthorized projects. Registration and managed worktree creation have their own authorization checks.
- Admin, account-management, pairing, or token-creation tools through MCP. Operational account APIs have separate authentication and scope checks; GPT Actions is retired on the v0.5 development line.
- Permission to bypass path safety, sensitive-path denial, read-only Session guards, or Runner policy.
- A reason to see secrets, tokens, env files, Authorization headers, or complete Runner configs.

## Project Access Model

Projects live on the Runner machine. The Runner registers allowed directories with the Server. The Server does not scan your filesystem.

Runtime project ids use:

```text
agent:<client_id>:<project_id>
```

The historical `agent:` prefix identifies a Runner Project, not a Durable Agent.
Use narrow allowed roots and register only directories you intend the selected client to operate. Remove a project from the Runner registry, narrow the allowed root, or stop the Runner to remove access from that client path.

## Runner Trust Boundary

The Runner is trusted to enforce local project policy and execute work on the machine that owns the files. Treat its Runner token (`wc_agent_*`) as a credential for that execution boundary, distinct from a Durable Agent identity.

Operational guidance:

- Run Runners under an OS user appropriate for the directories they serve.
- Keep project roots narrow.
- Configure shell profiles deliberately; do not inherit broad interactive shell state by accident.
- Do not copy complete Runner configs between machines unless that is the intended deployment action.

## Shell And Job Risk

`run_process`, `run_shell`, `run_script`, and `run_job` can execute commands with
the Runner account's OS permissions. Timeouts and output limits bound execution
and observation; they do not isolate the process from other files or networks
available to that account. Prefer structured file and validation tools when they fit.

Use them only when:

- The command is needed for the user's authorized task, including validation or diagnostics.
- The project, timeout, output limit, and shell profile are appropriate.
- The resulting output will not expose secrets.
- A human can review the command, output summary, and workspace state.

Prefer `read_files`, `search_project_texts`, `edit_project_files`, `cargo_fmt`,
`project_validate`, `read_workspace_changes`, and `check_workspace_hygiene` when
they fit. Use `run_process` for literal argv and `run_shell` when shell syntax is
needed. Use ecosystem validators such as `cargo_check`, `cargo_test`, or `go_test`
when their advanced specialist contract is needed. These are current development
tool names; released clients must use their Server's advertised MCP schema.

The default `trusted_agent` authority mode auto-authorizes consequential tools
after hard safety checks. `restricted` denies those calls; it does not queue
them for approval. Neither mode grants authority beyond the user's task or
bypasses scopes, path policy, or Session guards. See the
[authority contract](docs/agent/permission-model.md).

## Token Handling

Do not expose secrets in prompts, tool output, docs, examples, screenshots, logs, or committed config files.

Never share or commit:

- server bootstrap tokens,
- user API tokens,
- OAuth access or refresh tokens,
- shared keys,
- account credentials,
- Runner tokens,
- env files,
- complete `runner.toml` files (and legacy `agent.toml` files),
- Authorization headers.

Use the right credential for the right surface:

- MCP and runtime API calls use a shared key for quick evaluation or a scoped user token for managed mode.
- Runners use bound Runner tokens in managed mode; shared-key deployments use the matching shared key.
- Server bootstrap/admin credentials stay on their intended operator machine. The same-owner Desktop/Server Tunnel uses a protected local authorization binding; it is not a credential to paste into a remote client.
- Account credentials are for local token creation, not for model-facing clients.

Shared-key quick-start is a lightweight grouping mechanism, not production IAM.

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

- For direct shared-key quick-start, stop exposing resources to the compromised group and disable direct shared-key authentication if it must be rejected. Merely choosing a new Bearer value does not revoke the old value: enabled shared-key mode accepts unknown non-managed values as separate groups. Revoke any separately issued OAuth credentials as well.
- Revoke or rotate a user token used by MCP, or REST clients.
- Revoke OAuth tokens when using OAuth.
- Remove a project from the Runner registry or narrow its allowed root.
- Stop the Runner when the client should no longer reach that machine.
- Revoke or rotate a Runner token if the Runner credential may have leaked.
- Rotate server bootstrap/admin credentials if they were exposed.

After revocation, verify both that the old credential or resource access is
denied and that the intended replacement still works, using read-only calls
without logging credentials. Stopping access does not erase data already returned
to a client or establish that previously started Jobs have stopped.

## Reporting Vulnerabilities

Please report vulnerabilities through GitHub Issues on `yyjeqhc/webcodex` or by contacting the maintainer privately through GitHub if the report contains sensitive details.

Do not publish real tokens, env files, complete Runner configs, private repository contents, or exploit details in public issues. Use placeholders and minimal reproduction steps.

## Known Limitations

WebCodex is intended for controlled self-hosted environments. It is not a hosted SaaS, not a full identity provider, and not a replacement for normal code review, Git hygiene, endpoint hardening, or least-privilege operating-system policy.
