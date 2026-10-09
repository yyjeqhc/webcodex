# Cloudflare connections for the regular Server

Named and Quick profiles expose the existing Server Runtime, Runner and projects
through an OAuth-only listener. They require a persistent local Environment and
an authenticated private Server. The temporary, single-project `webcodex share`
workflow remains available separately.

The shared `EnvironmentStore` catalog accepts OpenAI, Cloudflare Named and
Cloudflare Quick profiles. Old records default to OpenAI and retain their IDs,
credentials and startup intent. Several OpenAI connections can run together with
one active Cloudflare connection. Saved Cloudflare alternatives may coexist;
only one can be selected for startup. Quick's generated origin is never saved in
the desired configuration.

## Configure and connect

In Desktop, configure the persistent Runtime first. Add a connection, select
Cloudflare Named or Quick and choose its owner. Named takes a fixed HTTPS origin,
the Tunnel identity and a write-only token. Saving configuration reports the
actual local forwarding target and any explicit owner restart/apply action.

Equivalent CLI configuration, using the selected Environment:

```sh
webcodex environment configure-tunnel personal \
  --provider cloudflare_named --host embedded --autostart true \
  --public-origin https://mcp.example.com --tunnel-id YOUR_TUNNEL_ID \
  --token-file /secure/cloudflare-token --json

webcodex environment configure-tunnel trial \
  --provider cloudflare_quick --host embedded --autostart false --json
```

`--token-file` must be a protected, owner-controlled file. A protected
`--credentials-file` JSON object with `tunnel_id`, `public_origin` and `token` is
also supported. Tokens are not accepted as command-line values. Subsequent
profile edits require the displayed `--expected-revision N`; omitting credentials
retains only that exact profile's existing token. Provider/host transfer cannot
silently take over a running owner.

Start or explicitly restart the Server when configuration reports that action,
then observe and start the selected embedded profile:

```sh
webcodex environment cloudflare-status personal --json
webcodex environment cloudflare-start personal --expected-revision N --json
```

For Named, configure the remotely managed Cloudflare Tunnel's public hostname to
forward to the displayed **Cloudflare local target** (`http://127.0.0.1:<port>`).
The private Server management port is a different listener. The token identifies
the remotely managed Tunnel; WebCodex does not edit its Cloudflare dashboard
routing. Activation requires a round-trip probe proving the exact Server,
profile, process generation and unpredictable nonce. A different WebCodex Server
or a generic successful HTTP response cannot satisfy the probe.

Copy the ready MCP address (`https://<origin>/mcp`) into ChatGPT's OAuth connection
configuration. Register the **exact callback URL displayed by ChatGPT**:

```sh
webcodex environment cloudflare-oauth personal \
  --redirect-uri 'https://EXACT-CALLBACK-FROM-CHATGPT' --json
```

Desktop offers the same callback/scopes form. The explicit first configuration
response reveals the client ID and secret once. Repeating the same configuration
returns the existing client ID without revealing the secret. If it is lost, or
callback/scopes change, use Desktop's explicit replacement action or repeat the CLI command with `--replace`; existing
clients and their grants are then revoked. Ordinary status, logs, profile
snapshots, child arguments and saved Desktop state contain no client secret or
Named token. Existing first-party OAuth management APIs retain their authority
requirements. See [OpenAI authentication requirements](https://developers.openai.com/plugins/build/auth).

The default consent profile is the existing `LOCAL_USER`: `runtime:read`,
`runner:manage`, `session:collaborate`, `project:read`, `project:write`, `job:run`.
The authorizing user's PAT further limits the actual grant. Browser, Computer,
local MCP and SSH capabilities require explicitly selected supported scopes;
they are not added to this default. Administrative authority continues through
the existing protected management mechanisms. A missing/incorrect Environment
user or Runner owner yields an explicit repair error; repair identity using the
existing Environment credential/setup flow while preserving projects and recovery
records. Activation requires the configured, connected Runner with that exact
owner, and never grants bootstrap authority to public MCP callers.

## Ownership, status and recovery

The Server always owns the dedicated listener, database, OAuth entry and
admission. Embedded ownership gives the Server the cloudflared process tree.
Standalone ownership gives the selected managed service that process tree,
using a protected runtime binding to the private loopback Server control API.
This API is authenticated and every process callback carries its acquired
Server instance, profile and generation. A stale callback cannot activate or stop
a replacement attempt. A lost standalone heartbeat closes admission; the driver
does not silently reacquire a replacement Server.

Standalone Cloudflare services must have saved startup selection enabled before
installation/start. Stop and uninstall the exact service before deselecting it.
Native Start uses durable service intent; the newly launched driver obtains a
fresh Server instance once, then pins all callbacks to it. Desktop action
requests use the observed instance/revision and never retry against a newly
observed owner automatically.

OAuth client provisioning and replacement also carry the caller-observed Server
instance and `process_generation`. The Server verifies that exact active attempt
and its current profile revision before changing the client. A stale request
preserves the existing client and authorization grants.

Desktop and CLI distinguish process/network state, OAuth configuration and an
actually observed valid OAuth request. A successful transport probe does not
prove that ChatGPT has connected. A temporary forwarding failure becomes
`disconnected`; recovery keeps the same generation and OAuth grants. Stopping or
crashing Quick immediately removes readiness and invalidates its current grants.
After a new Quick URL, update the ChatGPT MCP address and authorize again. Named
restarts with the same origin and owner preserve grants; a changed origin/owner,
or deleting and recreating a profile, invalidates the old authorization.

Removal verifies the stopped lifecycle owner, then records the exact Environment,
profile incarnation and revision being retired. If private cleanup fails, retry
removal with that original observation even if the catalog entry is already gone.
Pending cleanup blocks launch, editing and recreation; unknown private files are
retained and reported instead of deleted. The retirement record permits cleanup
recovery only and never supplies a launchable configuration.

Network reconnect changes observation only. It never resubmits a tool call or
recreates a Job. Observe the existing Job by its original identity using the
Runtime's existing reconciliation and recovery contracts.

The ingress port is allocated once and saved separately. The first dedicated
listener is applied at Server startup. A running Server that predates it returns
`cloudflare_ingress_not_applied` with `next_action: restart_server`; Desktop offers
an explicit Server restart for this response. Ordinary connection errors do not
imply that a restart is required. A port conflict fails startup explicitly; the
Server does not pick a different hidden target. Windows services use
installer-granted protected runtime materializations instead of
reading the user's root catalog. Changed/new protected metadata requires an
explicit stopped-owner Server apply/restart to grant its exact service identity;
unchanged files preserve their existing ACL. No broad root-directory ACL is added.

cloudflared runs with an isolated home/config and a minimal environment. Both
providers use `ManagedChild` process-tree ownership (Unix process groups and
Windows Job Objects), including cancellation, timeout and shutdown. Named uses
`--token-file`, requiring cloudflared 2025.4.0 or later; version/download resolution
uses the existing verified binary path. See [Cloudflare run parameters](https://developers.cloudflare.com/tunnel/reference/run-parameters/).

## Public boundary and Quick limits

The dedicated loopback listener mounts only `/mcp`, OAuth discovery, browser
login/consent, code exchange, refresh, revocation and a bounded forwarding probe.
It shares the existing database and ToolRuntime. Console, management APIs,
pairing, Runner and OpenAI Tunnel channels are absent. Public MCP accepts only
entry-bound OAuth bearer access tokens. Bootstrap, PAT, Agent/Project tokens,
shared keys, cookies, query tokens and anonymous fallback are rejected. Host and
Origin must match the listener's active entry; forwarding headers cannot select
an entry or manufacture authority.

Authorization codes, access/refresh tokens and browser sessions bind to the
entry and authorization epoch. Exchange and refresh recheck this binding in the
SQLite writer transaction even when `resource` is omitted. Issuer, resource,
challenge and MCP App origins use the same request entry snapshot, without
changing process-global configuration.

Cloudflare documents Quick as a development/testing transport with temporary
hostnames, no uptime guarantee, a 200 in-flight-request limit and no SSE support.
This integration uses finite JSON HTTP MCP calls; it does not promise an SSE
stream through Quick. Use Named for stable production connections. See
[Quick Tunnel limitations](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

## Automated and manual validation

The opt-in local acceptance harness uses real Server/Runner/CLI processes, an
isolated project, a fake cloudflared executable and verified local TLS proxies.
It requires Unix, Python 3 and openssl; no production account or paid model is
used. Its explicit native-root build lets child-local `SSL_CERT_FILE` trust an
ephemeral CA while keeping TLS verification enabled:

```sh
cargo build --locked -p webcodex -p webcodex-runner -p webcodex-cli \
  --features reqwest/rustls-tls-native-roots \
  --bin webcodex-server --bin webcodex-runner --bin webcodex
python3 scripts/e2e_cloudflare_oauth.py --bin-dir target/debug
```

The default production build/warning gate runs separately without this TLS-root
feature. Unit/integration suites cover entry fencing, concurrent refresh,
browser isolation, credential denial, configuration migration and process-tree
cleanup. The harness covers Named restart, Quick replacement, network recovery,
OAuth and real project read/write; its actual result is recorded in the PR.

Real Cloudflare/ChatGPT and native Windows/macOS execution remain separate manual
checks. On those platforms, verify process-tree cleanup and exact service ACLs;
on the real network, verify Named dashboard routing, discovery/issuer/resource,
the exact ChatGPT callback, S256 login/consent, tools/list and a disposable project
read/edit. Restart Named and reuse authorization; stop/recreate Quick and confirm
that the old connection fails until its address and authorization are updated.
Never describe these manual checks as passed unless they were actually run.
