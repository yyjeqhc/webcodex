# Authentication and credential model

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex separates bootstrap administration, account onboarding, runtime API access, and agent connectivity. Do not reuse one credential across all surfaces.

## Credential summary

| Credential | Used by | Purpose | Do not use for |
| --- | --- | --- | --- |
| `WEBCODEX_TOKEN` | server admin | bootstrap/root admin | GPT/MCP/agent daily use |
| shared key | agent + GPT/MCP quick start | shared-key group onboarding | production IAM/admin |
| `wc_acct_xxx` | user CLI | create local PAT/agent token | GPT/MCP/agent |
| `wc_pat_xxx` | GPT Action/MCP/API | runtime tools | agent connection |
| `wc_agent_xxx` | `webcodex-agent` | connect agent to server | GPT/MCP/runtime API |

## `WEBCODEX_TOKEN`

`WEBCODEX_TOKEN` is the server bootstrap/root/admin credential. It is configured in the server environment and is used for first-user creation and emergency administration.

Do not put `WEBCODEX_TOKEN` in GPT Actions, MCP clients, or day-to-day agent configs.

## Shared key quick start

A shared key is a quick-start secret supplied to `connect --key <KEY>` and to GPT Actions or MCP only when the host supports static bearer/API-key authentication. It is sent as:

```text
Authorization: Bearer <KEY>
```

The server groups shared-key callers by `shared_key_hash`. A shared key is not an admin credential, not a managed user identity, and not production IAM.

Static bearer/API-key host auth can be used with either a shared key for quick start or a `wc_pat_xxx` token for managed mode. OAuth is a separate flow; blank OAuth client fields do not become no-auth or static bearer.

Direct Bearer shared-key fallback is controlled by
`WEBCODEX_SHARED_KEY_ENABLED`. In that quick-start mode, an unknown non-`wc_`
Bearer value is not treated as a traditional wrong managed token; it becomes a
lightweight `shared_key` principal grouped by `shared_key_hash`. Bearer values
with a WebCodex managed prefix (`wc_`) that fail validation are rejected and
must not fall back to shared-key mode. Empty or whitespace Bearer values are
also rejected.

`WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE` is separate from
`WEBCODEX_SHARED_KEY_ENABLED`. The bridge flag enables shared-key entry on the
OAuth authorization page for OAuth-only hosts; it does not enable direct Bearer
shared-key fallback. Enabling direct Bearer shared-key fallback does not enable
the OAuth bridge.

## `wc_acct_xxx`

`wc_acct_xxx` is an account credential issued once when an administrator creates a user with `--issue-credential`.

The user uses it locally with:

```bash
webcodex-cli token create-local
webcodex-cli agent-token create-local
```

Those commands generate plaintext tokens locally and register only token hashes with the server.

Do not use `wc_acct_xxx` as a GPT Action token, MCP token, runtime API token, or agent connection token.

## `wc_pat_xxx`

`wc_pat_xxx` is a personal API token generated locally by the user. The server stores only its hash.

Use `wc_pat_xxx` for:

- GPT Actions
- MCP
- Runtime API calls
- Tool calls such as `/api/tools/list` and `/api/tools/call`

Scope the PAT to the workflow. For example, a GPT Action that inspects and edits projects may need runtime, project, and job scopes.

## `wc_agent_xxx`

`wc_agent_xxx` is an agent token generated locally by the user. The server stores only its hash and binds the token to `allowed_client_id`.

Use `wc_agent_xxx` only for `webcodex-agent` connectivity. It cannot call runtime, project, tool, MCP, or account endpoints.

## `client_id`

`client_id` identifies one agent client instance, such as:

```text
ubuntu-client
alice-macbook
ci-runner-1
```

An agent token is bound to an allowed `client_id`. This prevents an agent token minted for one client from registering as a different client.

## Runtime project ids

Agent-backed runtime project ids use this shape:

```text
agent:<client_id>:<project_id>
```

Examples:

```text
agent:ubuntu-client:webcodex
agent:alice-macbook:my-repo
```

The `<project_id>` comes from a top-level `id` field in an agent `projects.d/*.toml` file:

```toml
id = "webcodex"
path = "/root/git/private-drop"
```

Do not use server-side `[projects.<id>]` syntax in agent `projects.d/*.toml` files.

## Hash storage

For user-created PATs and agent tokens, the server stores token hashes, not plaintext `wc_pat_xxx` or `wc_agent_xxx` values. Plaintext tokens are shown once at creation time and must be stored by the user or agent host.
