# MCP

[English](MCP.md) | [简体中文](MCP.zh-CN.md)

WebCodex exposes an MCP endpoint so ChatGPT, Claude, and other MCP clients can work with repositories through the Runner that owns them. Ordinary users only need to choose between **full use** and a **temporary trial**; protocol surfaces, scopes, and credential taxonomy are reference material, not onboarding prerequisites.

## ChatGPT: recommended full setup

For everyday use, run a regular Server + Runner. Follow the [Full Setup guide](PERSONAL_SETUP.md) for one-time login and project registration, and use `--print-mcp-config` during that same `webcodex login` to obtain regular HTTPS MCP connection values. Public HTTPS, Cloudflare Tunnel, and OpenAI Secure MCP Tunnel are reachability choices; they do not change the capabilities of this full development path.

If you only want to try one repository temporarily, use the `share` path below.

## ChatGPT host-side Developer MCP errors

If ChatGPT reports:

```text
FORBIDDEN: This conversation does not support developer MCPs
```

The rejection comes from the ChatGPT host or the conversation-level Developer MCP admission and routing layer. It does not by itself mean that WebCodex permanently disabled developer access, that the Runner is offline, or that the project registration is invalid.

Check the Runner and project state independently. If they remain available and the request did not reach WebCodex, retry in a conversation where ChatGPT admits Developer MCPs. No WebCodex configuration change is required solely because this host-side error appeared.

## ChatGPT: temporary `share`

Explicit `share` is supported on Linux, macOS, and Windows and owns a temporary single-project environment for that foreground run. Windows x64 can use the managed default Cloudflare Quick Tunnel; Windows ARM64 needs a trusted explicit/PATH `cloudflared` because the pinned Cloudflare release publishes no official ARM64 artifact. Managed OpenAI `tunnel-client` supports both Windows x64 and arm64.

For the default temporary public path, WebCodex reuses an explicit/PATH `cloudflared` or downloads its pinned verified managed copy automatically, then run:

```bash
npm install -g @yyjeqhc/webcodex
cd /path/to/your/repository
webcodex share
```

When the CLI says **WebCodex ready**:

1. In ChatGPT Developer Mode, create a custom app using MCP.
2. Paste the printed **MCP URL**.
3. Choose **Access token / API key** (Bearer token) for the default share.
4. Paste the printed temporary **Credential**.
5. Run **Scan Tools**.
6. Try: `Inspect this repository and summarize its structure. Do not make changes.`

The command performs project setup itself. Hosted ChatGPT cannot reach a
loopback-only `webcodex run`, so `setup`, `doctor`, and `run` are not required
steps before `share`. ChatGPT UI labels can vary by rollout; use the CLI output
as the source of truth for URL and authentication. Developer Mode, custom MCP
apps, and write/modify actions are controlled independently by the ChatGPT plan,
workspace, and admin settings; those client-side permissions are not widened by
WebCodex scopes.

If ChatGPT itself reports `FORBIDDEN: This conversation does not support
developer MCPs` (or says the current conversation disabled the developer MCP
server), treat that as a Host/conversation admission problem until proven
otherwise. If the Host refuses to dispatch `get_runtime_status`, that text is not a
WebCodex tool result. Verify the Server/Runner independently before changing
credentials or Runner configuration; see [Troubleshooting](TROUBLESHOOTING.md).

## Claude and other MCP clients

Use the same printed `/mcp` URL and authentication values. In Claude, add a
custom connector and paste the MCP URL. Other MCP clients should be configured
with the same endpoint and the authentication mechanism reported by the CLI.
When a client cannot set a Bearer header, `webcodex share --auth query-token`
provides an explicit temporary-share fallback: paste the printed sensitive
`/mcp?token=...` URL and choose No authentication. The query accepts only the
current share Project Credential; it is not a general PAT/OAuth/shared-key query
auth mechanism. Treat the full URL as a secret because URL queries may be logged.
For local-only clients, `webcodex share --tunnel none` exposes the loopback MCP
endpoint without `cloudflared`.

For an OpenAI-only private transport, create/select a Secure MCP Tunnel, export
`CONTROL_PLANE_TUNNEL_ID` plus a Restricted `CONTROL_PLANE_API_KEY` with Tunnels
Read + Use, and run `webcodex share --tunnel openai`. ChatGPT uses Connection:
Tunnel + No authentication; the temporary WebCodex Bearer stays local and is
injected by the pinned verified OpenAI `tunnel-client`.

For a long-lived **loopback-only** Server reached through OpenAI Secure Tunnel,
ChatGPT host-file rewrites authenticated by the explicitly allowed local tunnel
credential can be trusted by setting
`WEBCODEX_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT=true`. Starting with v0.4.2,
WebCodex Desktop writes this value by default for the local loopback Server it owns; an
existing explicit value is never overwritten. The exception works only when
`WEBCODEX_ADDR` resolves to loopback and the authenticated credential is either a
normal user API token or the configured Server bootstrap credential used by the
Desktop regular Tunnel. The regular Tunnel derives that credential from the local
`WEBCODEX_TOKEN` configuration and injects it into its private tunnel-client
authorization; users should not copy or expose that credential. Independent/network-
accessible Servers remain off by default and must not use this as a substitute for
OAuth.

For a regular independent Windows Server + Runner reached through OpenAI Tunnel, or to troubleshoot a case where local `/readyz` is healthy but ChatGPT Connector creation still fails, see the [Windows + OpenAI Secure MCP Tunnel deep dive](WINDOWS_OPENAI_TUNNEL.md). It is advanced setup/troubleshooting material, not required reading for a first-time user.

## Result cards

On Stateless MCP 2026 requests that advertise MCP Apps HTML support, clients can display a
small set of read-only milestone cards for `list_jobs`, `read_validation_summary`, and
`read_git_review_summary`. The Job card shows only active or attention-requiring Jobs;
routine successful terminal Jobs stay out of the foreground. Aggregate validation
and committed-range review cards remain bounded and do not embed raw logs, diffs,
or hunks.

High-frequency calls such as `observe_jobs`, `cargo_check`, `cargo_test`, `go_test`,
and `read_workspace_changes` intentionally keep the Host's native tool presentation instead
of creating an extra custom App card for every call. Cards do not poll, retry, or
invoke tools; the canonical tool result remains available independently.
`WEBCODEX_MCP_APPS_ENABLED=false` disables App metadata and resources without
disabling the underlying tools.

The current Result App is intentionally static. September 2026 Host experiments proved that a separately designed MCP App controller can poll server-owned state and request later ChatGPT model turns, including a bounded foreground autonomous multi-turn loop, but background-tab model-turn scheduling is not an immediate guarantee. Those findings and the production design constraints are recorded in [`agent/mcp-app-continuation-experiments.md`](agent/mcp-app-continuation-experiments.md); they do not change the current Result App contract.

### Live Work Result card

`present_work_result` opens the separate Window work card with Activity, Results,
and Collaboration tabs. Results shows the current Project's uncommitted files,
rename paths, staging state, and available line counts while work is in progress.
The bounded workspace snapshot can include changes from other work; partial file
lists and missing line counts are labelled. A clean workspace is not task success.
Linked Session check/review evidence appears when available.

Hosts supporting conversation thread panels can open the public rendering
`work_result_thread_panel` entrypoint with an empty argument object. It resolves
the latest successful `present_work_result` in the same authenticated Host Window,
including that presentation's explicit business Session when supplied. Other
actions and failed presentations cannot retarget the panel. Presentation records
this durable binding without starting live Window activity; the thread entrypoint
and App refresh calls also stay outside live Window activity. The model still
calls `present_work_result` once near the first successful Project action.

The thread panel opens Review first: Changed files and lazy diffs, Session checks,
then sealed Final Changes. Activity and Collaboration remain secondary tabs;
Project, Window and Session identifiers are folded under Diagnostics. The inline
card retains its Activity-first layout. Opening Review does not eagerly load diffs.

An open panel retains its exact Project and explicit Session selection on refresh.
Window-linked Session evidence never becomes refresh authority. Reopen the panel
to select a newer successful presentation; missing Window identity or binding
fails closed. Current authorization and snapshot fences still apply on every read.
Changed files and Final Changes offer lazy Full text previews only for advertised
paths. Current files use the pinned working-tree snapshot; final files use the
sealed final tree, even after later workspace edits. Content loads in explicit
32 KiB pages up to 256 KiB per file; the card labels partial content and the cap.
A deleted file has no final version. Binary, non-UTF-8, symlink and submodule
contents are unavailable; failed or expired reads never fall back to a live path.
Markdown is enabled for `.md`/`.markdown` only after the complete file is loaded.
The bundled markdown-it parser supports standard Markdown, tables and
strikethrough, without promising every GFM extension. Rendering uses DOM node and
attribute allowlists: raw HTML remains text, unsafe URLs are rejected, links stay
inert and external images show a placeholder instead of loading resources.
Line/selection-to-chat interactions remain deferred.

The Markdown bundle is checked into the single-script App resource, so Rust-only
builds need no npm toolchain. After `npm ci --prefix frontend`, regenerate with
`npm --prefix frontend run build:work-result`; `node frontend/scripts/build-work-result-markdown.mjs --check`
checks deterministic output and also runs under `check:dist`.

Rendering entrypoints retain the default model/App visibility. App-only bridge
helpers return data without `ui.resourceUri`: ChatGPT rejects private tools that
declare a rendering resource when refreshing the connected App. After updating
the Server's tool descriptors, refresh tools for the existing App in ChatGPT's
plugin settings before testing the new entrypoint.

After closeout, Results also shows the sealed final task changes with on-demand
per-file diffs. Those diffs keep their original snapshot identity even if the live
workspace changes. Refresh uses the existing App-only observation path; opening
Results adds no tool calls. Automatic refresh pauses while the App document is
hidden and uses a bounded visible cadence so background cards do not continuously
exercise the Host tool bridge. Discuss these changes opens the existing composer
without sending a message. Window activity calls are compact, collapsed by default,
and fetch their sanitized trace/timing details only when expanded. The inline card
shows the canonical hashed Window key used by the Window activity ledger; the
thread panel puts the same identity under Diagnostics, making
support traces attributable without exposing the Host's raw Window identifier.
New cards use `ui://webcodex/work-result/v17` so Hosts with cached older templates
load the current thread-panel, lazy-detail and canonical tool-name contract.
Retired resource URIs fail closed instead of serving a new template under an old cache key.

## Existing Server

For an existing hosted Server intentionally configured for shared-key clients,
use the long-lived shared-key path with the credential supplied by its operator:

```bash
webcodex connect https://webcodex.example --key-file /private/path/shared-key
```

`connect` starts/reuses the local Runner and prints the MCP URL and credential
source after the connection is verified. This is separate from fresh self-hosted
Docker enrollment: keep the Docker Server bootstrap administrator token on the
Server, create a short-lived pairing code there, and use `webcodex login` on the
repository machine. Self-hosting is documented in [Deployment](DEPLOYMENT.md).

Bearer/shared-key authentication is the simplest path. When a client requires
OAuth, use `share --auth oauth` or `connect --auth oauth` with that client's exact
callback URL and follow the CLI output. Managed-user OAuth remains a separate
advanced identity flow.

## Advanced / reference

### Adaptive Runtime routing

There is one model-facing MCP runtime contract: **Adaptive Runtime**. Canonical `ToolDefinition` rank decides the direct tools; ordinary model-visible long-tail tools are invoked through `call_runtime_tool`; server-owned protocol capabilities and MCP App admission may add hidden extensions for the relevant protocol request. There is no startup model-surface selector. `read_tool_manifest(tool_name=...)` is discovery only: it never dynamically registers a new Host tool. Its exact `route.primary` describes the preferred callable, and a normal direct tool also exposes `route.fallback` through `call_runtime_tool` for the case where that direct callable is not present; explicit MCP App presentation tools mark that fallback as blocked while Apps are enabled. Direct versus gateway routing changes presentation only and never bypasses the target tool's authentication, Project authority, permission, Runner capability, Session, or safety checks.

### Request-local client policy

A shared Server can serve ordinary calls and Host-native orchestration without
changing tools or restarting between clients. Configure these optional HTTP
headers on the **client connection**, or inject them on a dedicated proxy route:

```http
X-WebCodex-MCP-Profile: direct
X-WebCodex-MCP-Budget-Secs: 20
```

`X-WebCodex-MCP-Profile` accepts exactly `direct` or `host_code_mode`. Omission
uses `WEBCODEX_MCP_HOST_PROFILE` (whose default is `direct`); it does not detect
client brands or prove that a particular call was programmatically orchestrated.
Send the header on each request, not just `initialize` or `work_on_project`.
There is no sticky Window, Session, credential or transport selection. For a
Server defaulting to `host_code_mode`, ordinary clients must explicitly select
`direct`; clients unable to set headers can use a configured proxy route.

The optional positive-integer budget header can only **reduce** the deployment's
resolved `WEBCODEX_MCP_HOST_BUDGET_SECS` budget. Return waits preserve the existing
five-second guard (and the existing one-second floor for tiny budgets). A Direct
request on a Server budget of 55 seconds therefore defaults to a ten-second
execution handoff, with a 50-second synchronous/observation ceiling; Host Code
Mode retains its five-second handoff/observation slices. `wait_for_job_readiness`
uses up to 45 seconds, further capped by the selected budget minus the guard,
not by the five-second handoff slice. Header omission leaves deployment behavior
unchanged. Empty, repeated or malformed headers fail before dispatch without
echoing their values; oversized numeric budgets are capped, not used to enlarge
Server limits.

This budget limits execution handoff and Job observation/readiness waiting. It
is **not** a universal RPC timeout or an execution lifetime: `timeout_secs`, Job
identity, effects, authorization, internal orchestration caps and Runner ownership
are unchanged. Explicit `work_on_project.guidance_profile` still selects guidance
only; it does not override transport timing. Subsequent context refreshes use the
policy of their own request. `/api/tools/call`, result-text compatibility,
`tools/list`, Apps admission and standard error semantics are unaffected.
`runtime_status.effective_config.mcp_host` remains the deployment snapshot, not a
claim about every connected client's policy.

Metadata/full trace records the actual selection as `mcp_request_policy_selected`:
`selection.effective`, separate profile/budget sources, parsed requested budget
and deployment cap. This distinguishes omitted headers from explicit defaults
and a clamped request; raw headers are not retained. Normal results and authority
are unchanged. See [client contracts and validation](implementation/mcp-client-contract-alignment.md).

Ordinary work stays in the current turn: finish independent work, use one bounded
`wait_for_job_readiness` join for blocking Jobs, then `observe_jobs` for needed
results. At deadline reassess work/dependencies instead of mechanically refilling
waits. Preserve pending identities when work cannot finish; never redispatch or
assume a new model turn will start. `wait_for_job_terminal` is optional durable
attention for an explicitly established continuation workflow, not a blocking
wait or a prerequisite for ordinary MCP.

### Tool result framing

Machine-readable MCP tool results are returned in `structuredContent`; `content` is a concise human-readable/protocol-native fallback. Clients that need fields should consume `structuredContent` rather than parse text.

Ordinary clients retain standard MCP `isError` behavior. For requests whose `_meta["io.modelcontextprotocol/clientInfo"].name` is exactly `openai-mcp` (any version), the MCP adapter applies an **OpenAI structured-failure compatibility projection**: WebCodex-owned canonical `ToolResult` failures use `isError=false`, while `structuredContent.success=false` remains authoritative and the complete output/error is preserved. The current OpenAI Host promotes `isError=true` to an exception without exposing `structuredContent`; this projection preserves machine-actionable failure and recovery data and can be removed when that Host behavior changes. JSON-RPC/protocol errors remain errors, and upstream third-party MCP/Plugin passthrough results retain provider semantics.

Some MCP hosts do not expose `structuredContent` to the model. This has been observed with Claude Custom Connector even when WebCodex successfully executes the tool and returns the complete structured result. Operators serving such a host can explicitly set `WEBCODEX_MCP_TEXT_JSON_COMPAT=true`. Ordinary runtime tool results then keep `structuredContent` canonical while also serializing that same JSON value into `content[0].text`. The option is off by default because the duplicate representation increases response/model-context size; protocol-native image/resource framing and the existing App-only compatibility paths remain unchanged.

Recovery fields in a result describe the next safe **explicit** call. They never grant authority and never trigger a hidden retry. In particular, an uncertain outcome must be reconciled before repeating an effect.

### Built-in local MCP gateway

A hosted Server can expose Runner-owned local stdio MCP providers through the same `/mcp` endpoint. Authorized callers use the single `mcp_tool` entry to list, describe, and call configured providers; provider process/instance identities and schema-revision state stay internal.

Calls sharing a provider connection wait up to two seconds within the original request deadline. If `provider_busy` reports `dispatchState=not_started`, wait briefly and retry serially with the original arguments and any idempotency key, using bounded retries. Reconcile `outcome_unknown` before repeating an effect.

Configure local providers on the Runner under `[mcp]`. Access requires the explicit `mcp:local` permission; hosted OAuth clients opt in with `webcodex connect ... --oauth-local-mcp`. See [Runner](RUNNER.md#provider-side-gateway-v1-compatibility) for provider compatibility details.

### Managed SSH resource onboarding

The `manage_ssh_resource` tool provides a narrow Runner-local onboarding path for
named SSH resources. `list` observes safe logical names and returns an opaque
exact-Runner/revision binding; `register` and `remove` consume that binding and
change only durable desired state. They never silently retarget a replacement
Runner or replay after an uncertain outcome. Raw SSH targets are not returned
in tool results or normal/full trace bodies. When `restart_required=true`,
restart the Runner and list again before selecting the resource in a Workflow
Session.

This surface requires the optional `ssh:local` permission. It is not part of the
ordinary hosted OAuth baseline; opt in explicitly with
`webcodex connect ... --oauth-local-ssh`. See
[Runner](RUNNER.md#ssh-session-resources-advanced) for static-vs-managed and
PersistentShell details.

### OAuth2

When OAuth is enabled, MCP clients can use the authorization-code flow instead of a static token. Register the client's exact callback URL, keep `offline_access` when the host requests refresh-token support, and follow the connection values produced by `share --auth oauth` or `connect --auth oauth`. Server setup is in [Deployment](DEPLOYMENT.md#oauth2).

For ordinary hosted `connect --auth oauth`, the Runner keeps its hosted credential while the MCP client receives a separate OAuth credential. Add `--oauth-computer-permissions`, `--oauth-local-mcp`, or `--oauth-local-ssh` only when those optional capabilities are needed. Existing clients are not silently widened; a real permission change requires reauthorization.

Project-first `share --auth oauth` remains bound to that temporary share environment. Managed-user OAuth is a separate advanced flow (`connect --auth managed-oauth`). OAuth credentials are never valid on Runner transport.

For the credential and scope model, see [Authentication](AUTH_MODEL.md#oauth2).

### Grok custom connector (OAuth)

Grok supports custom MCP connectors and can complete the OAuth flow required by
the MCP server. For a self-hosted WebCodex Server, first expose
`https://your-domain.example/mcp` over public HTTPS and enable OAuth:

```text
WEBCODEX_OAUTH2_ENABLED=true
WEBCODEX_OAUTH2_ISSUER=https://your-domain.example
WEBCODEX_PUBLIC_URL=https://your-domain.example
```

For the current Grok web connector flow (verified in August 2026), register this
redirect URI exactly:

```text
https://grok.com/connectors-oauth-exchange-code/
```

If Grok later presents or uses a different callback, register that exact value
instead. Create a dedicated OAuth client; the client secret is returned only
once:

```bash
curl -fsS -X POST https://your-domain.example/api/oauth/clients/create \
  -H "Authorization: Bearer $WEBCODEX_PAT" \
  -H "Content-Type: application/json" \
  -d '{"name":"Grok MCP","redirect_uris":["https://grok.com/connectors-oauth-exchange-code/"],"allowed_scopes":["runtime:read","project:read","project:write","job:run"]}'
```

In Grok's **Custom Connector** form, use:

| Field | Value |
| --- | --- |
| MCP server URL | `https://your-domain.example/mcp` |
| Client ID | the returned `wc_client_*` value |
| Client Secret | the returned one-time `wc_csec_*` value |
| Authorization Endpoint | `https://your-domain.example/oauth/authorize` |
| Token Endpoint | `https://your-domain.example/oauth/token` |
| Scopes | `runtime:read`, `project:read`, `project:write`, `job:run`, `offline_access` |
| Token Auth Method | `client_secret_post` |

WebCodex advertises PKCE `S256`; Grok can use PKCE together with
`client_secret_post`. Do not select `none (PKCE only)` for a WebCodex OAuth
client that has a client secret. `offline_access` is a protocol-level scope for
refresh tokens and is intentionally not stored in the OAuth client's
`allowed_scopes` permission list. The MCP Protected Resource Metadata omits
`scopes_supported` because pre-registered clients can have different scope
ceilings. General-purpose MCP clients can therefore omit `scope` and let WebCodex
default the authorization request to that client's registered `allowed_scopes`.

When the WebCodex authorization page opens, sign in with a current user PAT
(`wc_pat_*`) for the user whose authority Grok should receive. A Runner token
(`wc_agent_*`) is not a user login token. The resulting OAuth access token is
bound to that user and remains constrained by the registered/requested scopes.

Common setup failures:

- **Save & Connect is disabled:** Grok requires a Client ID before it can start
  the OAuth flow.
- **`invalid token`:** the PAT must authenticate against the same current
  WebCodex Server database. Do not use a Runner token or a stale PAT left from
  an older Server/database.
- **`invalid scope`:** every requested WebCodex permission scope must be in the
  OAuth client's `allowed_scopes`. For normal Grok MCP use, do not request
  `account:manage`; `offline_access` is accepted separately as a protocol scope.
- **redirect mismatch:** the redirect URI must match the registered value
  exactly, including path and trailing slash.

See xAI's [Connector documentation](https://docs.x.ai/grok/connectors) for the
current Grok Custom MCP UI and availability.

## Project-scoped ordinary runtime

`webcodex run` and `webcodex share` bind one configured repository, start a local Server + Runner, and expose the ordinary Adaptive Runtime. The temporary or persistent Project Credential is an authentication/ProjectGrant boundary; it does not select a separate capability surface.

A typical coding flow is:

```text
work_on_project
→ read_files / search_project_texts / semantic navigation as needed
→ edit_project_files or other canonical edit tools
→ present_work_result once when substantial work becomes materially stateful
→ run_process / run_shell / focused validation tools as needed
→ read_workspace_changes
→ finish_coding_task
```

`work_on_project` starts or resumes an explicit Workflow Session on an ordinary registered Project. If the user requests isolation and a registered Project is already known, use `work_on_project(project=..., mode=worktree)`: the Server reauthorizes that source Project, the Runner derives its internal managed placement, and the resulting worktree is registered as another ordinary Project. The model does not reconstruct a Runner path or choose the managed destination. `client_id + path + mode=worktree` remains a compatibility/bootstrap form and keeps ordinary path authority checks. Without an isolation request, local `share`/`run` work directly on the Project already registered by setup.

`present_work_result` is a one-card presentation layer for substantial coding, not a correctness primitive. Once mounted, its App-only state reads keep current progress, workspace, validation, and review visible without model polling. A non-blocking `finish_coding_task` seals eligible final changes in the presentation cache at closeout; the same card then discovers that immutable snapshot and can lazily expand per-file diffs. Tiny/read-only work should skip the card; repeated presentation of the same Session should be avoided.

For ordinary portable read-only validation, prefer `project_validate`. It accepts only a closed `format_check` / `check` / `test` intent plus an optional `auto` / `rust` / `go` / `python` adapter hint; the Runner resolves the nearest unambiguous recipe on its own registered filesystem and then starts the existing structured validation Job. Rust maps to `cargo fmt -- --check`, `cargo check --all-targets`, or `cargo test`; Go maps to `go vet ./...` or `go test -json ./...`. Go project validation runs in Runner-owned single-module mode with `GO111MODULE=on` and `GOWORK=off`, so ambient module mode or parent `go.work` selection cannot silently change the gateway's workspace semantics. Its validation target identity is domain-separated from ambient Go specialist evidence, so success under different workspace semantics cannot reconcile a gateway failure. The standalone `go_test` specialist keeps its existing environment behavior. Optional `scope` selects exactly one portable package intent: bounded `packages` (1..8 entries) narrows Rust check/test through repeated Cargo `-p` selectors and Go check/test through project-relative package patterns, while `all_packages=true` selects the complete project unit. Rust all-packages maps to Cargo `--workspace` only when the Runner proves the effective Cargo workspace root is exactly the registered Project root and binds the in-Project Cargo manifest graph into the existing re-plan fence; Go all-packages keeps the canonical `./...` single-module scope. Scoped formatting fails closed. Python supports test only, using an existing configured/profile/PATH Python 3 interpreter and canonical `python -m pytest --color=no -rA`; Python check/format, all scope and dependency policy fail closed. Node detection returns a bounded unsupported result. Python planning and Job admission require `project_validation_python_pytest_v1`; missing pytest is a definite not-started tooling failure with no automatic installation or fallback. See [Python/pytest validation](implementation/python-pytest-project-validation.md) for environment selection, evidence and same-Job behavior. The request never carries arbitrary executable, argv, shell grammar, installation, or source mutation. Existing `cargo_*` / `go_test` tools remain available for ecosystem-specific advanced options. `project_validate` requires the additive `project_validation_v1` Runner capability; explicit `packages` additionally require `project_validation_package_scope_v1`, while `all_packages=true` requires `project_all_packages_v1`. Go project-validation Job admission additionally requires `project_go_single_module_v1`, so a Server cannot hand a Go gateway plan to an older Runner that may inherit ambient workspace state.

Cargo all-packages provenance is a bounded package-selection witness, not a complete build-input snapshot. It requires a contained workspace, or a standalone package with no ancestor `Cargo.toml` marker; parent markers are only probed, never read outside the registered Project. External path dependencies are unavailable in this scope because their `package.workspace` metadata can add out-of-Project members. Relevant manifest/member/dependency aliases remain fenced, while unrelated non-manifest links are ignored. Unproven topology or exhausted bounds returns `validation_scope_unavailable` / `build_scope_unavailable`; explicit package scope and the existing specialist tools retain their contracts.

For ordinary portable Rust/Go builds, prefer `project_build`. It accepts only an exact registered `project`, optional project-relative `cwd`, an optional `auto` / `rust` / `go` adapter hint, optional portable `scope` selecting either bounded `packages` (1..8 entries) or `all_packages=true`, and total `timeout_secs`. The Runner resolves the nearest unambiguous recipe and owns canonical argv: Rust maps explicit packages to repeated `-p` selectors and all-packages to `cargo build --workspace` only after proving the effective Cargo workspace root is the registered Project root; Go maps explicit package patterns directly and all-packages to `go build ./...`. Go project builds execute with Runner-owned `GO111MODULE=on` and `GOWORK=off`; full `go.work` workspace semantics are outside the v1 gateway rather than inherited implicitly from the Runner host. The request cannot provide an executable, argv, shell, script, release/profile/target/features, native workspace/exclude flags, offline/network policy, or artifact-discovery contract. Portable all-packages requests require the additive `project_all_packages_v1` Runner capability. Node/Python recipes fail closed as unsupported in v1.

Both gateways optionally accept `dependency_policy: {"mode":"locked"}`. This is a portable dependency-resolution guarantee, not a literal cross-ecosystem flag contract: Rust build/check/test use Cargo `--locked`, while Go build/vet/test use `-mod=readonly`. The policy tells the adapter not to repair project dependency selection state in order to make the operation succeed; it does **not** disable registry/module/toolchain network access. Offline/network policy remains a separate #599 extension. `project_validate(action="format_check")` rejects the dependency policy instead of silently ignoring it. Policy-bearing planning and typed Job admission both require the additive `project_dependency_policy_v1` Runner capability. Locked validation derives a distinct durable validation target identity, while requests that omit the policy preserve the historical argv and identity.

`project_build` requires the additive `project_build_v1` Runner capability at both planning and typed Job admission. Go project-build Job admission additionally requires `project_go_single_module_v1`. Admission replans the registered project/root, recipe, manifest/lock provenance, package scope, and canonical invocation; the worker rechecks that same plan after any local queue wait, before native process execution. A stale plan fails as `not_started` and releases its Job slot rather than silently rebuilding or executing the outdated intent. Long builds keep the same durable Job and return the ordinary sparse pending continuation; pending never authorizes retry or redispatch. The closed gateway bounds WebCodex's command authority but is not an OS sandbox: Cargo/Go build logic and project build scripts may still have their own filesystem or network effects. Existing lower-level execution tools remain explicit escape hatches for build forms outside this v1 contract.

For `action="test"`, optional `test` selects tests and states the evidence requirement:

```json
{"project":"agent:runner:repo","action":"test","scope":{"packages":["package-a"]},"test":{"filter":"selected_test","min_tests":3}}
```

Rust interprets `filter` as one libtest substring; Go interprets it as a native
`-run` regexp, including Go's slash-separated subtest semantics. Python uses a
native pytest `-k` expression, bounded to 200 UTF-8 bytes with controls and
option-shaped prefixes rejected. Go and Python preserve meaningful whitespace;
this is not a cross-language query syntax. Empty/omitted filters keep
the unfiltered default. Filters cannot introduce arbitrary argv. `require_tests`
defaults to true (at least one proven executed test); explicit false accepts
native success when `min_tests` is absent, including proven zero or unknown counts
(unknown counts remain unproven; source freshness is independent). A requested `min_tests` (1..1,000,000)
still applies with false, and count uncertainty is not zero. These are evidence
postconditions, not extra tests to run. The test block is invalid for check or
format_check. Any supplied test block requires the additive
`project_validation_test_options_v1` Runner capability; it is checked at both
planning and Job admission. Old calls without that block retain their old wire
and execution defaults. See [project-validation test options](implementation/project-validation-test-options.md)
for exact scope, identity, and remaining #599 work.

Adaptive Runtime may expose common tools directly and long-tail tools through `call_runtime_tool`. Direct versus gateway exposure never changes schema validation, OAuth scope, Project authority, permission policy, Runner capability checks, Session fences, or effects.

The removed ProjectConnector capability names (`task_start`, `files_read`, `edits_apply`, `task_finish`, and related operations) are not compatibility aliases for runtime tools. Use the current ToolRuntime names returned by `tools/list`/`read_tool_manifest`.

### Long work continues as Jobs

`observe_jobs(summary_only=true)` is an opt-in presentation mode for proven successful
structured validation Jobs. It removes routine passed-test and Cargo progress lines,
while preserving test summaries, unknown text, warnings, validation evidence, lifecycle,
and truncation/reset/retention flags. Failures, zero/unproven tests, compile-only test
runs, incomplete validation evidence, and ordinary commands keep their normal output.
Tiny results are unchanged when summary metadata would make them larger.

A summarized item includes `logs_omitted` and a parser-ready `suggested_call` with
`summary_only=false`. That call preserves the **original** observation cursor; using
the newly returned observation token instead would skip omitted lines. Expansion is
bounded by the existing log retention and may report reset or unavailable history.
No log copy, model invocation, Job execution, permission, or waiting policy is added.
Omitting `summary_only` preserves the existing behavior.

`search_file_context` reuses ordinary search-result sparsification after read planning.
It omits redundant phase metadata, not source text, query indexes, failure evidence,
read revisions, or snapshot-bound continuations.


Long-running commands and validations use the canonical WebCodex Job lifecycle. Observe the exact Job returned by the initiating call with `observe_jobs` (or recover it with `list_jobs` when identity was genuinely lost) instead of starting another copy. Jobs are not wrapped as MCP Tasks; WebCodex does not advertise the former Connector-specific MCP Tasks extension.

The ChatGPT/model turn and one MCP observation request do not own the Job lifetime.
A Host-side `Thinking stopped` / `Thinking failed`, request timeout, or dropped
observation therefore does not by itself prove that the Job stopped. Resume the same
conversation and re-observe the existing Job; recover Job inventory before any retry
when identity was lost. Do not redispatch solely because the model turn ended.
Eligible terminal waits may expose best-effort Host continuation, but Host acceptance
does not guarantee that a new model turn actually ran. See
[Troubleshooting](TROUBLESHOOTING.md#chatgpt-reports-thinking-stopped--thinking-failed-during-long-running-work).

## First safe prompt

```text
Use the configured WebCodex project. Inspect README.md and summarize the
project structure. Do not edit files or run commands.
```

No project discovery or runtime identifier belongs in this prompt.

## Read and search bounds

- `read_files` is the canonical bounded range reader for one to eight files. A
  one-item batch is the single-range path. Each successful item returns the
  complete-file SHA-256 and bounded line metadata; partial items carry a
  positional one-item `read_files` continuation and are not snapshot-stable.
- `search_project_texts` is the canonical bounded search surface for one to
  eight independent queries (ripgrep first with the existing bounded fallback).
  A one-query batch is the single-query path.

An empty search result is affirmative no-match evidence only after a recognized
backend reports a successful completed/no-match status. Missing or malformed
backend identity, missing completion status, status/output disagreement,
backend failure, Runner failure, timeout, request drop, and provider failure
return failures instead. Search failures retain the compatibility `code` and
add bounded `failure_stage` plus a specific `reason_code`. Batch failure items
retain their broad `reason_code` and preserve the single-search provenance as
`failure_stage` and `detail_code`; successful batch items remain sparse.

Failures return small structured errors with project-relative paths only —
never absolute paths, commands, Runner/provider stderr, or arbitrary provider
prose.

## Common errors

| Code | Meaning | Action |
| --- | --- | --- |
| `project_not_configured` | No canonical setup exists | Run `webcodex setup` |
| `project_credential_invalid` | Private Project Credential is missing or mismatched | Restore both matching private files or recreate the profile |
| `project_credential_rejected` | The reachable server rejected the credential | Restore the server-matching credential |
| `workspace_unavailable` | The configured Git workspace is unavailable | Restore the workspace, then run doctor |
| `server_unreachable` / `agent_offline` | The project Runner/runtime is unavailable | Run `webcodex run` / `webcodex doctor` |
| `required_capability_unavailable` | The current Runner/runtime lacks a required coding capability | Upgrade all binaries |
| `project_registry_scope_denied` | A project-scoped credential tried to expand or mutate the Project registry outside its granted visibility | Use an already-visible Project or `work_on_project(mode=worktree)` |

## Adaptive Runtime extensions

The same ToolRuntime serves project-scoped local `share`/`run` instances and multi-project hosted Servers through one Adaptive Runtime contract. Project-scoped credentials change visibility and authority, not the model-facing runtime shape. Protocol-specific capabilities and MCP Apps may admit additional hidden presentation or resource operations without creating another runtime surface.

Stateless MCP keeps Memory tools and the Skill compatibility tools `list_skills`
and `read_skill_file` off the top-level `tools/list`, even with full OAuth scopes.
Their exact contracts remain available through `read_tool_manifest(tool_name=...)`
and execute through `call_runtime_tool` with unchanged scope, Project, permission,
and capability checks. Existing direct protocol compatibility and the
`memory.bootstrap` context sidecar remain supported. Ordinary Skill selection
and execution keep the direct `load_skill` and `run_skill_resource` paths.
The optional closeout helpers `check_workspace_hygiene` and `finish_coding_task`
are model-visible gateway tools; review/coding catalogs still recommend them.

Stateless MCP 2026 exposes common untrusted invocation metadata only through one optional closed `_wc` envelope. Depending on the tool, the envelope may admit `record`, `ack`, `ack_ref`, `resolve`, `reply`, `context`, and `control`. These are adapter metadata only: they never become canonical ToolCall business arguments or grant authority. Legacy flat root wrappers such as `recording_session_id`, `ack_session_message_ids`, `ack_ref`, `session_message_resolution`, `window_reply`, `context_request`, and `_control` are rejected on this Stateless 2026 surface; legacy/non-stateless transports keep their existing contracts. `call_runtime_tool` carries `_wc` only on the outer gateway call; the nested target `arguments` remain canonical business arguments and reject a second `_wc`.

`WEBCODEX_MCP_COMPACT_SCHEMAS` defaults to `true`. Compact `tools/list` omits
`outputSchema` and projects shorter MCP-specific tool/input descriptions for
selection: purpose, nearby tool distinctions, and essential continuation guidance.
Repeated `_wc` copy is shortened too. Compact discovery preserves the envelope
shape and bounds while omitting only repeated prose and exact opaque-ID regexes on
`_wc.record`, `_wc.ack.items`, and `_wc.resolve.message_id`; the full manifest
retains the complete contracts. Business-ID, hash/Git fence and resource-path
patterns, all bounds, required fields, enums, object/union shape, annotations, and
MCP App/file metadata are preserved. This is discovery presentation only; runtime
argument validation and execution authority do not change.

Use `read_tool_manifest(tool_name=...)` for the full exact input contract and operational
description, or set compact schemas to `false` for full discovery schemas.
Canonical ToolSpecs are never rewritten. Focused MCP tests compare inputs against
canonical schemas (with the explicit host-file reference overlay) and enforce
serialized-byte and advertised-tool-count budgets on final Stateless results,
including Session wrappers, gateway tools, and optional App metadata/tools.

### ChatGPT file bridge

For viewing a project PDF, call `present_pdf(project, path)` directly. It opens
the dedicated PDF App (`ui://webcodex/pdf/v3`) with a filename, page/zoom/search
toolbar and a full-height continuous-scroll reading area. It does not include Work Result activity,
change lists or collaboration. Unchanged and untracked PDFs are supported;
Git and Workflow Sessions are not prerequisites. The Host controls its outer
sidebar and display mode. Rendering requires an MCP Apps-capable Host.

The dedicated reader uses the App-only `read_app_artifact_chunk` bridge for bytes.
That bridge is format-neutral and reuses the canonical artifact export chunk
transport: each `tools/call` reauthorizes Project access, validates the pinned
path/size/SHA-256 identity, and returns at most the canonical 1 MiB artifact chunk through private MCP metadata.
A changed source fails closed. For a 9 MiB document this is roughly nine Host
round trips rather than the legacy 128 KiB `read_pdf_chunk` loop's roughly seventy.
The legacy PDF-specific reader remains available for compatibility but is no longer
the dedicated reader's primary path. The PDF remains bounded at 20 MiB and PDF.js
rechecks the assembled digest/header before rendering. Use `present_work_result`
for substantial coding progress, and `present_pdf` when the user asks to view a
PDF. See [PDF document viewer](architecture/pdf-document-viewer.md).
The dedicated reader keeps PDF.js as the rendering engine but uses a small host-compatible continuous-scroll shell with viewport observation and nearby-page lazy rendering; Work Result retains its compact single-page preview. The transport still assembles and verifies the complete pinned file before PDF.js starts rendering, so progressive first-page range loading remains a separate optimization.

When the connected MCP protocol/host admits the artifact capabilities, WebCodex supports
host-native file transfer in both directions without routing complete binary
payloads through model text:

- `import_host_files` imports 1..10 files supplied by the
  ChatGPT host through `openai/fileParams`. This applies to user-selected
  conversation attachments and to newly generated files when the host binds
  them as file parameters. The Control downloads the referenced bytes and
  commits them through the existing bounded artifact-write path; callers should
  not construct download URLs or manually Base64-transfer those files.
- `inspect_project_artifact` is the preferred Project-to-model/host read surface. Use
  `action=metadata` for existence/size/MIME/digest/image/archive facts,
  `action=inspect` for one bounded snapshot-fenced Base64 segment,
  `action=image` for native MCP image delivery, and `action=export` for complete
  host/user delivery. Do not loop `inspect` chunks to transfer a complete file.
- `action=export` reuses the existing artifact export authority and returns a
  short-lived authenticated MCP `ResourceLink` plus metadata. `tools/call` does
  not contain the complete binary. The host follows `resources/read` to obtain
  the binary resource; authentication and current project-read authority are
  checked again, and the artifact metadata is revalidated before the bytes are
  returned. The resource URI is not standalone bearer authority; export handles
  are short-lived process-local presentation state, and the normal size, MIME,
  path, and authorization bounds remain in force.

The lower-level `read_project_artifact_metadata` and `read_project_artifact_chunk`
tools remain operator/gateway primitives. The legacy `export_project_artifact`
compatibility tool has been removed; complete host delivery is exposed only as
`inspect_project_artifact(action=export)`. Office artifacts such as DOCX/PPTX/XLSX and
PDFs use the same underlying artifact transport and can therefore move between
a project and a supporting ChatGPT host without a model manually carrying their
Base64.

Use [Coding Workflow](CODING_WORKFLOW.md) for the canonical `work_on_project` bootstrap, behavioral-role mental model, and validation/closeout guidance. See [Architecture](ARCHITECTURE.md) and the `webcodex` CLI for operator tooling.
