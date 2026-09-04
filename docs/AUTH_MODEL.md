# Authentication and credentials

[English](AUTH_MODEL.md) | [简体中文](AUTH_MODEL.zh-CN.md)

WebCodex has several ways to authenticate because Server administration, model/API access, and Runner connectivity are different trust boundaries. Ordinary users do **not** need to learn WebCodex's internal identifier vocabulary to use it safely.

## The short version

For normal daily use, follow [Full Setup](PERSONAL_SETUP.md): redeem a one-time login code, let `webcodex login` create the local user and Runner credentials, and use the connection values it reports for ChatGPT.

If you are using an existing hosted shared-key Server, use the shared key supplied by its operator with `webcodex connect`. If you are only trying one repository temporarily, use the credential printed by `webcodex share` for that run.

Do not copy the Server bootstrap token to a client, and do not use a Runner token as an MCP/API token.

## Credentials you may encounter

| Credential | Typical form | Used for |
| --- | --- | --- |
| Server bootstrap token | `WEBCODEX_TOKEN` in the Server env | Initial administration and emergency recovery |
| Pairing code | `wc_pair_...` | One-time device/user enrollment |
| Personal API token (PAT) | `wc_pat_...` | MCP, GPT Actions, and runtime API access for a managed user |
| Runner token | `wc_agent_...` | `webcodex-runner` transport only |
| Shared key | `wck_...` | Hosted shared-key MCP/runtime access and the matching Runner group |
| Project Credential | protected project-private file | One project-first Connector/share environment |
| OAuth access token | `wc_oat_...` | Delegated MCP/GPT access when OAuth is enabled |
| Account credential | `wc_acct_...` | Advanced managed-account token creation only |

The prefixes are useful for diagnosing configuration mistakes. They are not a reason to expose every internal identifier to users.

## Credentials are not the same as identifiers

WebCodex also uses non-secret IDs and opaque tool state internally. Users normally do not need to learn their formats.

| Kind | Examples | Meaning |
| --- | --- | --- |
| Credential | PAT, Runner token, shared key, OAuth token | Authenticates a caller |
| Resource ID | Project, Job, Workflow Session, task | Identifies an object; does not grant access to it |
| Opaque tool state | continuation/recovery values returned by a tool | Helps continue or safely retry a specific workflow; not authentication |

The rule is simple: **knowing an ID or opaque tool value never substitutes for authentication and authorization.** Exact internal identity and continuity formats belong in maintainer contracts and code, not in this user guide.

## Secret handling

- Do not print, log, commit, or paste whole credential files into chat.
- Prefer `--token-file <path>` over command-line plaintext tokens.
- Let an AI agent point to the exact protected file or field; the human should copy the secret when necessary.
- Status, diagnostics, and normal API responses intentionally avoid returning plaintext credentials.
- If a credential may have leaked, rotate or replace the credential according to the flow that created it.

## Server bootstrap token

`WEBCODEX_TOKEN` is the Server's bootstrap/admin credential. `webcodex server init` stores it in the Server environment. Use it for initial administration, user creation/pairing, and emergency recovery. Do not use it for MCP, GPT Actions, Runner connectivity, or ordinary daily work.

## Pairing and managed login

`webcodex pairing create` produces a short-lived `wc_pair_*` code. A repository machine redeems it with `webcodex login <server-url> --code <code>`. Login then creates the ordinary local files needed for user/API access and Runner connectivity.

The pairing code is temporary; it is not a long-lived API credential.

## Personal API token (`wc_pat_*`)

A PAT represents a managed user on MCP, GPT Actions, and the runtime API. The Server stores only its hash. `webcodex login` normally writes the user's token to `webcodex-user-token` under that Server/user's local configuration directory.

Use the smallest scopes needed for the workflow. A PAT used by an MCP coding client normally needs runtime/project scopes appropriate to the actions that client will perform. Account-management authority is separate and should not be added to ordinary coding clients.

## Runner token (`wc_agent_*`)

A Runner token authenticates `webcodex-runner` and is bound to the configured Runner `client_id`. It is rejected on MCP/runtime/account surfaces.

The `wc_agent_*` prefix is a compatibility-facing historical name. In current product terminology this is a **Runner token**, not a Durable Agent identity. The same rule applies to other retained `agent_*` wire/storage names: do not infer the Durable Agent domain from the compatibility name.

## Shared key (`wck_...`)

A shared key is the simple hosted connection credential used by `webcodex connect`. The same key can authenticate the MCP/runtime client and the matching Runner group. Different shared keys remain isolated from each other.

The protected profile stores the key after creation; repeated `connect` reuses it rather than printing it again. Shared-key mode is intended for simple trusted deployments, not as a replacement for managed multi-user IAM.

## Project Credential

`webcodex setup` and temporary project-first flows use a protected Project Credential tied to one project environment. It is not a general user/admin token and must not be reused for unrelated projects.

After verification, WebCodex keeps the non-secret authorization metadata it needs internally. It is not another credential the user needs to copy or manage.

## OAuth2

OAuth lets MCP/GPT clients use the authorization-code flow instead of storing a long-lived PAT in the client. Register the exact callback URL required by the client and follow the connection output from `webcodex share --auth oauth` or `webcodex connect --auth oauth`.

The user-facing OAuth concepts are:

- **client id** — public identifier for the OAuth client;
- **client secret** — secret returned when the client is created;
- **access token** — delegated credential used by the client;
- **refresh token** — protocol credential used to refresh access; `offline_access` adds no WebCodex permission by itself;
- **allowed scopes** — the maximum WebCodex permissions that client may request.

WebCodex never silently expands an existing OAuth client's allowed permissions when new scopes are introduced. Changing the allow-list is an explicit administrative action and invalidates old grants so the client must authorize again.

For ordinary hosted shared-key OAuth, `webcodex connect ... --auth oauth` keeps the Runner on its shared key and gives the MCP client a separate OAuth credential. `--oauth-computer-permissions` explicitly enables the additional Computer permissions offered by that flow; `--oauth-local-mcp` explicitly enables access to configured Runner-owned local MCP providers. Managed-user OAuth remains a separate advanced flow (`--auth managed-oauth`).

Server configuration is in [Deployment](DEPLOYMENT.md#oauth2); MCP client setup is in [MCP](MCP.md#oauth2).

## Scopes and authority

Authentication answers **who the caller is**. Scopes answer **which classes of operation that caller may request**. Project/path checks, Session guards, Runner capabilities, and the Server's authority mode remain separate checks.

A token having a scope does not bypass project boundaries or native safety checks. Conversely, knowing a Project/Session/Job identifier does not create the missing scope.

The Server's `WEBCODEX_AUTHORITY_MODE` is also separate from authentication. It controls whether consequential operations auto-execute after hard safety checks or require the configured human-authorization path; it does not change credential identity or scope membership. See [Authority model](agent/permission-model.md) for maintainer-level detail.

## Computer observation and control authorization

Computer Use permissions are intentionally separate from ordinary project/runtime access. Read-only Computer observation requires `computer:read`; effectful control requires `computer:control`; launching applications uses `computer:launch`. Full-display, pointer, and global clipboard operations have additional explicit scopes and are not silently inherited by older credentials.

OAuth clients receive these optional permissions only through explicit operator/user opt-in. The runtime still rechecks the current Runner capability and native OS permission at the moment of the call. See [Computer Use](COMPUTER_USE.md) for the product-facing feature overview.

## Public compatibility names you may still see

Some compatibility-facing names remain because changing them would break stored configuration or wire compatibility:

- `wc_agent_*` — Runner token;
- `agent:<client_id>:<project_id>` — runtime Project address;

These do **not** refer to WebCodex's separate Durable Agent / Conversation / Agent Task domain. Other process/protocol compatibility fields remain implementation details. New documentation should say **Runner** unless it is quoting one of the public compatibility-facing names above.

## Where credentials are stored

| Credential | Typical location |
| --- | --- |
| `WEBCODEX_TOKEN` | Server env file, commonly `/etc/webcodex/webcodex.env` |
| Managed user PAT | `~/.config/webcodex/<server-slug>/<user>/webcodex-user-token` |
| Managed Runner token | inline in the matching `runner.toml` |
| Hosted shared key | protected hosted profile `runner.toml` |
| Project Credential | protected project-private state |
| OAuth client secret | returned at client creation; store it in the client/operator's secret store |

For command-specific setup and recovery paths, use [CLI](CLI.md) and [Troubleshooting](TROUBLESHOOTING.md). Internal identity and continuity formats are intentionally omitted from this user-facing reference.
