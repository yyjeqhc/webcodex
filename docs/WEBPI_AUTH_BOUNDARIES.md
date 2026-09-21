# WebPi credential isolation

Current namespace/upgrade rules are in [WEBPI_IDENTITY.md](WEBPI_IDENTITY.md). Native WebPi now uses its own `WEBPI_*` configuration; earlier deployment observations below remain historical.

WebPi owns its deployment credentials. A WebCodex installation may be used as an
upstream/reference implementation during development, but it is never a credential
source for WebPi at runtime.

## Credential ownership matrix

| Concern | WebPi-owned source | Compatibility detail | Shared with WebCodex? |
| --- | --- | --- | --- |
| Server bootstrap/admin credential | `.webpi-state/server/webpi.env` | The WebPi native runtime reads `WEBPI_TOKEN` in this **WebPi-owned file**. Old keys are migrated explicitly without changing their secret values. | No |
| Runner transport credential | WebPi connection `runner.toml` under `.webpi-state/connections/` | Uses the inherited `wc_agent_*` token format. | No |
| Managed user PAT | WebPi connection directory under `.webpi-state/connections/` | Uses the inherited `wc_pat_*` format. | No |
| Web GPT / Action PAT | `.webpi-state/webpi-action-token` | Baseline coding scopes plus `plugin:inspect` and `plugin:invoke`; it does not grant `plugin:manage`. | No |
| OpenAI Tunnel ID | `.webpi-state/secrets/openai-tunnel.json` or `WEBPI_CONTROL_PLANE_TUNNEL_ID` | Mapped to `CONTROL_PLANE_TUNNEL_ID` only inside the Tunnel child process. | No |
| OpenAI Restricted Tunnel API key | Same WebPi secret file or `WEBPI_CONTROL_PLANE_API_KEY` | Mapped to `CONTROL_PLANE_API_KEY` only inside the Tunnel child process. | No |
| tunnel-client executable | `.webpi-runtime/tunnel-client/0.0.14/windows-amd64/tunnel-client.exe` | WebPi pins the current reviewed OpenAI release and verifies both archive and extracted-binary SHA-256 values. | No runtime dependency |
| Optional tunnel-client override | `WEBPI_TUNNEL_CLIENT_BIN` or the WebPi secret file | The WebPi fork reads this native WebPi variable directly; `WEBCODEX_TUNNEL_CLIENT_BIN` is never used as WebPi input. | No |
| Broad OpenAI API/Admin keys | Not used by WebPi Tunnel | `OPENAI_API_KEY` and `OPENAI_ADMIN_KEY` are scrubbed from WebPi Server/Runner/Tunnel child environments. | No |
| Future OAuth client secret | Must be provisioned for WebPi separately | Do not reuse a WebCodex OAuth client/secret. | No |
| Git hosting credential | Operating-system/user Git credential manager if an upstream fetch needs it | Not a WebPi runtime credential. The WebCodex upstream remote is fetch-only and its push URL is disabled. | May be OS-shared, but irrelevant to WebPi runtime |

## Fail-closed rules

1. WebPi never reads `CONTROL_PLANE_TUNNEL_ID`,
   `CONTROL_PLANE_API_KEY`, or `WEBCODEX_TUNNEL_CLIENT_BIN` as configuration
   inputs.
2. The WebPi secret file wins over `WEBPI_*` environment input. If the secret file
   exists but is malformed or incomplete, WebPi fails instead of falling back to
   environment variables.
3. A partial Tunnel pair is rejected: Tunnel ID and Restricted API key must be
   configured together.
4. WebPi Server, Runner, and admin helper subprocesses scrub Tunnel credentials and
   broad OpenAI credentials from their inherited environment.
5. Only the Tunnel child receives compatibility `CONTROL_PLANE_*` names, created
   from WebPi-owned credentials.
6. Without an explicit WebPi client path, Tunnel startup requires the pinned
   WebPi-local tunnel-client. It does not silently fall back to the WebCodex managed
   cache or PATH.
7. Secret values are not passed on argv. The interactive key prompt uses
   `getpass`; one-time pairing codes are passed to login on stdin.
8. Standalone WebPi forcibly disables `WEBPI_SHARED_KEY_ENABLED`, `WEBPI_ALLOW_ANONYMOUS`, `WEBPI_OAUTH2_SHARED_KEY_BRIDGE`, and `WEBPI_PROJECT_SHARE_MCP_QUERY_TOKEN_ENABLED`. Native process entries discard all inherited `WEBCODEX_*`; the standalone launcher also removes ambient `WEBPI_*` before applying the selected local configuration. An unknown non-`wc_*` Bearer must be rejected instead of creating a new lightweight shared-key tenant.
9. The bootstrap credential remains an exact constant-time checked admin secret; normal web-GPT use is through the separately issued Action PAT. Runner agent tokens remain restricted to Runner transport surfaces.

## Historical Windows deployment record

The earlier setup record named the following independent deployment root:

```text
E:\WebPi\webpi-core
```

The 2026-09-19 security-review session could access only `E:\WebCodex Desktop\webpi-core`; access to the directory above was rejected by Runner allowed roots. The following earlier enrollment record is historical, not a fresh verification of that deployment. Current observations and manual blockers are in [the security review](WEBPI_SECURITY_REVIEW_2026-09-19.md).

Its local Server/Runner enrollment and Action credential have been regenerated in
that directory. The previous development tree's `.webpi-state` was not copied.

The pinned OpenAI `tunnel-client v0.0.14` Windows x64 binary is stored under the
new WebPi `.webpi-runtime`. The reviewed release archive SHA-256 is
`784ab8da7b5a88f0109f1fd8aaf0a1c86067430b896dddf307ef7e3cc49fa1a5`, and the
extracted executable SHA-256 is:

```text
fcc85a69ec0ad82518e4f8964f60c45e31787957782a0fc9c1b0c44e82d61b9b
```

At the time this contract was written, the new WebPi deployment intentionally has
**no Tunnel ID or Restricted Tunnel API key configured**. `webpi.cmd status`
therefore reports `tunnel_configured=false` and `tunnel_config_source=none`.
This is preferable to silently borrowing another product's credential.

## Configure the WebPi Secure MCP Tunnel

Use a **new WebPi-specific** Tunnel ID and Restricted Tunnel API key. Do not paste
the key into chat or shell history. This Tunnel is for the ChatGPT developer-mode
App/MCP path. It is not a generic HTTPS proxy for WebPi's GPT Actions OpenAPI
endpoint.

From PowerShell in `E:\WebPi\webpi-core`:

```powershell
.\webpi.cmd tunnel-config
.\webpi.cmd status
.\webpi.cmd run-web
```

`tunnel-config` stores the pair in
`.webpi-state/secrets/openai-tunnel.json`. The API key prompt is hidden.

For non-interactive operator-managed deployment, set the pair together:

```text
WEBPI_CONTROL_PLANE_TUNNEL_ID
WEBPI_CONTROL_PLANE_API_KEY
```

Optionally set `WEBPI_TUNNEL_CLIENT_BIN` to an explicit absolute WebPi-controlled
binary. The project-local pinned binary is the recommended default.

### GPT Actions are a separate exposure path

If WebPi is used as a Custom GPT **Actions** backend, expose the WebPi
`/openapi.json` and `/api/actions/*` endpoints through an ordinary trusted HTTPS
origin/reverse proxy or another appropriate HTTPS tunnel. Use a WebPi-owned Action
credential. Do not assume an OpenAI Secure MCP Tunnel publishes those HTTP Action
routes.

The ChatGPT developer-mode **App/MCP** path instead selects the WebPi Secure MCP
Tunnel and reaches WebPi's MCP endpoint through that Tunnel.

For an externally managed Cloudflare Tunnel, register the public origin with `webpi.cmd cloudflare-config https://<hostname>`. WebPi does not own the Cloudflare token in this mode. The Cloudflare route should terminate at the loopback WebPi origin (`http://127.0.0.1:56542`); never expose the local Server by rebinding it to a public interface merely to make Tunnel routing work.

`/openapi.json` and the static Runtime/Admin HTML/JS/CSS shells are public metadata/assets in the inherited HTTP topology. Their backing `/api/*` calls and `/mcp` are protected by `AuthMiddleware`. Public access to the static shell is therefore not equivalent to execution authority; the critical invariant is that missing, invalid, or arbitrary Bearer credentials receive `401` on protected surfaces.

## Authentication acceptance, not configuration optimism

WebPi child environments force all four hardened auth flags to `false` and scrub inherited credentials case-insensitively. Startup requires a non-empty WebPi-owned bootstrap credential. Public-URL configuration rejects controls/userinfo/path/query fragments and updates the env file atomically with a concurrent-content check.

`webpi.cmd status` distinguishes env configuration from the running server's invalid-Bearer result; an offline server is not marked as authenticated. Managed startup readiness and the OpenAI tunnel launch reject a running origin that fails negative authentication checks.

```powershell
.\webpi.cmd verify
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip --with-action-token
```

The acceptance tool checks missing Bearer, unknown non-managed Bearer, unknown managed PAT and wrong scheme on Actions, generic runtime and MCP. Every negative case must return 401. Public OpenAPI must be real schema JSON, its server origin must match, and the positive managed-PAT call must succeed. Redirects are not followed; credentials and response bodies are not logged. `--with-action-token` reads only this deployment's own token and sends it only to loopback or its manifest-configured public origin.

Cloudflare Tunnel and WebPi authentication are separate layers. For the user's Cloudflare/GPT Actions path use `webpi.cmd run` for WebPi and the separately managed `cloudflared`; `run-web` starts the different OpenAI Secure MCP Tunnel workflow. Do not start a public tunnel to a still-permissive origin.

## Uninstalling WebCodex

WebCodex can be removed only after the final WebPi public-Tunnel + web-GPT smoke
test succeeds from the actually deployed root. For Cloudflare Actions, use the local/public acceptance above plus a real web-GPT project and Pi-tool call. The following older checklist applies specifically to the optional OpenAI Secure MCP Tunnel path:

- `.\webpi.cmd status` shows Server state, Runner config, Action token, and Tunnel
  configuration under `E:\WebPi\webpi-core`;
- the local pinned tunnel-client exists and verifies;
- `.\webpi.cmd run-web` brings up the WebPi Server, Runner, and WebPi Secure MCP
  Tunnel without reading WebCodex credentials;
- the ChatGPT developer-mode App is configured to use the new WebPi Tunnel and
  invokes the WebPi project, returning
  `agent:webpi-local:webpi-core`;
- a Pi bridge call reports `engine=pi`.

After those checks, WebPi has no runtime need for the WebCodex Desktop application,
its Runner, its data directory, its Tunnel credentials, or its managed
tunnel-client cache.
