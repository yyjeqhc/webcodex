# Secure MCP Tunnel client

A standalone Rust client for the OpenAI Secure MCP Tunnel wire protocol. This
crate has no dependencies on other WebCodex crates. It forwards one `main`
channel to one fixed Streamable HTTP MCP endpoint. JSON-RPC requests, no-ID
notifications, SSE notifications/results and session termination are supported.

## Standalone program or embedded API

**Standalone:** build/install one executable, without a WebCodex Server, Runner,
Go installation or extra daemon:

```sh
cargo build --locked --profile dogfood -p webcodex-openai-tunnel --bin webcodex-tunnel
# From this checkout (not a claim that the package is already on crates.io):
cargo install --path crates/webcodex-openai-tunnel --bin webcodex-tunnel
```

Use the same `CONTROL_PLANE_TUNNEL_ID`, `CONTROL_PLANE_API_KEY`,
`CONTROL_PLANE_BASE_URL` and `MCP_SERVER_URL` environment names as the official
client. Run `webcodex-tunnel doctor`, then `webcodex-tunnel run --json`.
`webcodex-tunnel status --json` reads the existing listener; it neither polls the
control plane nor dispatches an MCP request. Default health address is
`127.0.0.1:8080`; use `--health.listen-addr 127.0.0.1:0` to choose a free port and
read its actual address from the started event. Stop with Ctrl-C/SIGTERM, or use
`--stop-on-stdin-eof` when a parent owns the foreground process. No install or
privileged service registration is required.

Local authentication is optional for standalone MCP servers. Set
`MCP_AUTHORIZATION` to the complete local header, or pass
`--mcp.authorization file:/path/to/local-header` when the target needs it.
`--control-plane.api-key env:NAME` and `file:PATH` are also supported; literal
secrets in command arguments are rejected to keep them out of process listings.
There is no fallback to an unrelated `OPENAI_API_KEY`. Unknown official CLI
options are rejected rather than pretending to support YAML, OAuth, stdio,
multiple channels, administrative management or arbitrary extra headers.

The standalone program defaults to **ProtocolCompatible** deadlines: honor valid
wire timeouts, without adding an arbitrary shorter maximum to long MCP calls.
`--max-response-seconds N` explicitly selects a finite policy. This is separate
from a WebCodex Job's lifetime: returning a pending Job completes the transport
request; observing it later uses another request, not a redispatched operation.

**Embedded:** depend on the library with `default-features = false`. The Server
uses the same `TunnelClient` directly; it does not spawn `webcodex-tunnel` or
require that executable to be installed. The `cli` feature only supplies the
standalone entry point, signal/health server support and host run-marker helper.
Keep the host's own cancellation and credential ownership. `Health::snapshot()`
returns a small, serializable `HealthSnapshot`; `unsettled_commands` includes
in-flight work, not just failures. No URLs, keys, payloads or request IDs appear
in that snapshot. `FixedMcpTarget::unauthenticated(url)` is an explicit option for
an endpoint that requires no local header; the control-plane key is never reused.

```rust,no_run
use std::time::Duration;
use webcodex_openai_tunnel::{
    ControlPlaneIdentity, Credential, FixedMcpTarget, TunnelClient,
    policy::{DeadlinePolicy, Limits},
};

# async fn example(key: &str, local_key: &str) -> Result<(), Box<dyn std::error::Error>> {
let identity = ControlPlaneIdentity::new(
    "https://api.openai.com", "tunnel_example", Credential::bearer(key)?,
)?;
let target = FixedMcpTarget::new(
    "http://127.0.0.1:8000/mcp", Credential::bearer(local_key)?,
)?;
let client = TunnelClient::new(identity, target,
    DeadlinePolicy::RequireFinite { max_duration: Duration::from_secs(120) },
    Limits::default(),
)?;
let health = client.health();
// Supply an owned cancellation future. Dropping run cancels all of its work;
// hosts must retain a restart fence if completion cannot be observed.
client.run(std::future::pending::<()>()).await?;
# Ok(()) }
```

## Wire compatibility and execution policy

Wire decoding accepts additive fields and preserves unknown command types.
Malformed, wrong-type and overflowing `response_timeout` values remain decodable.
`DeadlinePolicy::ProtocolCompatible` implements the official legacy no-deadline
behavior for absent/invalid values. `RequireFinite` rejects invalid timing
metadata, caps valid durations, and supplies a finite duration when absent/null.
Zero expires immediately. All budgets start at poll response headers receipt,
before body decoding, and include queueing, MCP exchange and response delivery.

The library policy default is `RequireFinite { max_duration: 120 seconds }`.
The standalone program explicitly chooses `ProtocolCompatible` instead.
`RequireFinite` is a security-oriented execution policy, **not** exact behavioral
parity with the Go client. Supported timeout values use the Go signed 64-bit nanosecond range.

Only `main` is admitted. Unknown types/channels and invalid commands do not
execute MCP; `Health::rejected_commands` observes these rejections without
logging payloads. Request headers admit singleton `Mcp-Session-Id`,
`Mcp-Protocol-Version`, `Last-Event-ID`, `Mcp-Method`, `Mcp-Name`,
`X-OpenAI-Session` and `X-OpenAI-Subject`. These are protocol/context metadata,
never replacements for the bound credential. Control-plane ingress/tracing
headers are bounded and discarded, including forwarded-client-certificate
metadata; they do not make a legitimate command invalid. Host, Accept,
Content-Type and body framing are owned by the local hop. Connection-nominated
fields are removed. Authorization, Proxy-Authorization and Cookie overrides
still reject the command, as do duplicate/multivalue admitted fields and invalid
or over-budget headers. Arbitrary forwarding and OAuth header rewriting remain
unsupported.

## Authority and retries

Control-plane origin, tunnel ID, target URL and both credentials are private,
immutable bindings. Reconfiguration requires a new client. URLs reject userinfo
and fragments; non-TLS HTTP is limited to numeric loopback addresses. The
control plane uses configured environment/system proxy discovery; MCP bypasses
proxies. All redirects, including same-origin redirects, are rejected. HTTP
library retries are disabled. There is no automatic credential fallback.

Polls retry only transport failures, 408, 429 and 5xx with bounded backoff/jitter.
401/403 stop the client. No `wrong-cluster-v1` capability is advertised or
partially implemented. Command shard tokens are echoed only to response POSTs.

A terminal response can be retried on transport failure, 408, 429, 502, 503 or
504, with identical body, correlation and original deadline. 404 is terminal.
Notifications retry only 429: without a trustworthy write fence, all transport
failures are treated as ambiguous. After a notification delivery failure, later
notifications are suppressed while the final MCP result is still drained and
attempted. Retry-After supports seconds and HTTP dates, capped at 60 seconds,
and cannot extend cancellation or deadlines. Valid MCP JSON-RPC errors retain
their IDs, error values, HTTP status and allowed response headers. Rejected
notifications and session termination preserve the target's HTTP status as a
terminal acknowledgement, including 404/405. Empty-data SSE priming events are
ignored. A malformed, oversized or incomplete MCP response produces a bounded
terminal failure while retaining execution uncertainty; it never replays MCP.
An already attempted terminal response is never replaced by a different result.
SSE reconnect/resumption and server-initiated JSON-RPC requests are not supported.

## Boundedness and no replay

The guarantee is **no local replay after possible dispatch, within one client
lifetime**. Every admitted request ID is retained without eviction. Each command
has one MCP dispatch entry; response recovery never calls it again. A disconnect,
timeout, dropped future or shutdown does not mean an effect was rolled back.

`limit` is a server hint. A complete poll batch within the configured ingress
bound is accepted even if larger than requested. Execution permits and the
bounded ingress buffer are separate: at most `concurrency + ingress_commands`
commands are active/queued. No further poll starts until the previous batch's
queue has drained, and a whole ingress burst must fit in remaining receipt
capacity. Oversized poll bodies/batches and exhausted receipt capacity stop the client
without evicting earlier receipts. Defaults: 8 workers, 128-command burst,
65,536 lifetime receipts, 4 MiB bodies, 1 MiB SSE events, 3 delivery attempts.
The lifetime limit requires deliberate operational planning for long-lived use.

There is no across-restart exactly-once claim. Hosts can opt into
`with_restart_recovery` before starting the client, retaining its returned run
guard until execution ends. WebCodex's persistent local/embedded OpenAI adapter does this.
The client syncs a checksummed request-ID hash before local MCP dispatch, keeping
the receipt after both clean and abrupt shutdown. With the same origin, Tunnel,
local target URL and local credential, it suppresses those IDs after restart.
It never retries an old effect or invents its result; a caller retry with a new
request ID is a separate operation, not covered by this deduplication.

Protected runs use a v3 marker. Windows owners record their process creation
time, distinguishing a reused PID after reboot from the original process.
A host-proven absent owner permits archival of
that exact marker and creation of a new run, only after the existing receipt
journal validates. Live/unknown owners, legacy v2 markers, changed bindings,
missing journals and corrupt complete records remain blocked. The ledger holds
at most 1,000,000 receipts (64,000,040 bytes), without eviction or automatic
reset; capacity exhaustion fails closed. An incomplete final append is removed
because local dispatch requires a completed, synced admission first. These
durability guarantees rely on the filesystem honoring sync operations.

The per-Tunnel `.lock` file also contains an immutable, checksummed 72-byte
protected-history header. It is synced before first journal creation and is
never removed on clean stop. Once that header exists, missing receipts block
startup even when `.active` is absent. A missing/empty protection header with
surviving receipts, an active protected run or recovered-marker directory also
blocks initialization; corrupt headers and changed bindings are not rewritten.
The exception is a bounded directory containing only valid, exact v2 archives:
these record explicit legacy reconciliation and permit first-time protection.
Empty, invalid or v3 archives cannot authorize fresh history, including through
the legacy guard API when other protected files are missing.
An interrupted first initialization stays blocked rather than reseeding history.
State rollback is outside this local guarantee: restoring an older valid journal
can discard admissions without violating its checksums. Do not restore protected
files independently. Losing the entire private namespace cannot be distinguished
from a fresh namespace without an external trust anchor.

Automatic recovery retains at most 128 archived run markers and 64 KiB of marker
contents per Tunnel, with a 4-KiB limit per marker. Its bounded preflight rejects
invalid archive entries and exhausted budgets before journal repair or marker
movement. No archive is automatically evicted: the active marker, receipts and
permanent protection remain intact for operator reconciliation when full.

The default library constructor, standalone CLI and temporary `webcodex share`
retain manual recovery for unprotected runs. Each share uses a new local credential;
a clean stop permits a new share, while an uncertain stop remains fenced.
The legacy guard API cannot reopen a protected receipt store.
Do not downgrade, delete receipt/protection history, switch hosts or change state roots to
bypass replay protection. Restored connection readiness does not settle an old
task's unknown result; archived run markers remain available for reconciliation.

### Explicit local credential transition

`authorize-local-credential` authorizes a credential-only change for an existing
protected Tunnel. It preserves the original protection header, receipt journal
and archived runs. It neither creates credentials nor changes a control-plane
key, Tunnel ID, URL, configuration file or Host review decision. The operator
must first stop the exact owner cleanly; an active marker, unknown shutdown,
missing/corrupt history or concurrent owner blocks authorization. Ordinary
restart and legacy recovery do not authorize a credential change.

The operation requires two explicit trust inputs:

- The fixed control plane truthfully supplies each command's original RFC3339
  `created_at` enqueue time, including on delayed or repeated delivery.
- The operator checks a trusted UTC source and confirms a whole-second UTC
  cutoff later than the start of this operation. This is recorded as
  `operator_attested`; the client does not independently verify clock accuracy
  or interpret an empty poll as a queue barrier.

The new binding permanently admits only commands whose valid enqueue timestamp
is strictly after that cutoff. Old, equal, missing and malformed timestamps
are rejected before local dispatch, across restarts and recovery. Every old
admission receipt still suppresses its request ID, even if a repeated command
claims a newer timestamp. This authority check also covers session termination.
Bindings that have never transitioned retain their existing timestamp behavior.
Response deadlines remain anchored to poll receipt; the cutoff is not a timeout.
Do not substitute a fixed waiting period for this predicate.

After checking those premises, with the existing control-plane configuration
and original local authorization, run:

```sh
webcodex-tunnel authorize-local-credential \
  --state-dir /absolute/existing/tunnel-runs \
  --mcp.authorization file:/absolute/private/original-header \
  --new-mcp-authorization file:/absolute/private/replacement-header \
  --enqueue-after "$WEBCODEX_REBIND_CUTOFF_UTC" \
  --expected-revision 0 \
  --confirm-future-enqueue-cutoff
```

Both files hold complete local authorization headers and must stay private;
the replacement file must be a bounded regular file, not a symlink, and on Unix
must have no group/other permissions or extra hard links. `env:NAME` is also
supported; literal credentials in arguments are rejected. The exact state path
and all transition options are required, with no implicit environment defaults.
Revision zero denotes the initial protected header. Later changes require the
current revision and a strictly increasing cutoff. At most 16 transitions are
retained; capacity exhaustion stops without eviction.

Authorization appends a checksummed, linked record to the existing protection
file; it does not rewrite its original header or any receipt. A synced immutable
intent precedes the append. Only after success should the operator update the
same host's protected local configuration to the authorized credential and
restart its protected client. The JSON receipt reports the revision, cutoff,
exact-retry status and unchanged configuration; it contains no credentials or
binding hashes. Starting with the old binding is rejected. Older binaries also
reject the extended protection format, so binary rollback requires a compatible
build; never restore an older receipt/history snapshot as a rollback mechanism.

If interrupted, retain all files and repeat only the same authorization inputs,
including the original expected revision and cutoff. A complete matching record
is synchronized again; a partial append is completed only against its intact,
matching published intent and exact byte prefix. Normal startup never repairs
a transition. Conflicting retries, a missing/corrupt intent or inconsistent
history remain blocked. A failure between authorization and configuration update
therefore leaves a stopped, diagnosable binding rather than selecting another
credential automatically. Returning to an earlier credential is another explicit
transition with a later cutoff, not a history rewind.

Protocol reference: the official `openai/tunnel-client` checkout at
`0e4201e5e17dac104a091a16afc5609f6ef68420`, `docs/protocol.md` and
`docs/openapi.json`, defines `created_at` as enqueue time and keeps it separate
from response deadlines. The
[hosted service limits](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels#limits)
describe an absolute request budget; that budget alone is not a local credential
transition fence. If the operator cannot establish the two trust inputs above,
the transition must not be authorized.

For a live enqueue-field preflight, `webcodex server tunnel` accepts
`WEBCODEX_TUNNEL_ENQUEUE_DIAGNOSTICS=1`. This defaults off and emits at most 16
changed counter snapshots per process at its existing health cadence. Each
`enqueue_timestamp_diagnostic` reports only valid/unverifiable timestamp counts,
without request IDs, raw timestamps, payloads or credentials. A valid count
establishes syntax observed on this connection, not independent proof of the
control plane's clock or enqueue semantics. Disable it after preflight.

The library owns no background tasks. `TunnelClient::run` owns its poll and worker futures and
cancels them on shutdown. `Health` exposes safe local observations; readiness
requires a successful poll and never claims that a ChatGPT connector was created.
Hosts must monitor run completion and separately verify local MCP readiness.

## Scope and validation

No stdio, Harpoon, Cloudflare companion, OAuth rewriting, admin UI, profiles,
routing correction. WebCodex supplies its existing authenticated
MCP endpoint; this crate does not bypass runtime authorization.

Run `cargo test -p webcodex-openai-tunnel`. Integration fixtures use bounded
loopback HTTP servers and synthetic credentials. They cover ambiguous response
and notification delivery, repeated poll commands, side-effect disconnects,
headers, JSON-RPC errors, SSE framing, session termination, deadlines, capacity,
authorization failures, redirects and cancellation. They are not evidence of a
live OpenAI service or real ChatGPT end-to-end acceptance.

## Standalone artifacts and status

`GET /healthz` is liveness, `GET /readyz` returns 200/503 for poll readiness, and
`GET /health` returns the same snapshot as the library. These are loopback-only,
read-only observations: they do not claim that the target MCP server or ChatGPT
connector is ready. HTTP connections, request sizes and waits are bounded.
The CLI owns a per-tunnel run marker using the same default location/key as the
embedded WebCodex host. Normal observed shutdown clears it. A crash or unconfirmed
effect retains it; first resolve prior work rather than deleting the marker or
using another state directory as an automatic retry. `--state-dir` explicitly
selects a deployment state root, not a replay bypass.

`.github/workflows/tunnel-build.yml` builds and tests separate native Linux,
Windows and macOS x64/ARM64 downloads. It is dispatchable/reusable and uploads
artifacts only; it does not publish a GitHub Release. Each archive includes one
binary, this README and `tunnel-build.json` with target, source revision and SHA-256.
It deliberately leaves the application's existing three-binary npm/Desktop
packages unchanged. It can be called independently of application installers.

```sh
# Development packaging; explicitly labeled non-publication output:
python3 scripts/package_tunnel_artifact.py \
  --binary target/dogfood/webcodex-tunnel --platform linux-x64 \
  --out-dir target/tunnel-artifacts --allow-development-build
```

The workflow supplies `WEBCODEX_TUNNEL_SOURCE_COMMIT` before building and requires
that exact revision when packaging. `--build-info-json` reports the standalone
crate's version/target/source; it does not borrow the application's version.
For source packaging, use `cargo package -p webcodex-openai-tunnel`. Actual
publication is a separate authorized action. The compatibility review used the
local official `openai/tunnel-client` checkout at
`c8aeedec334db55bbd69bb16db6b71276993d708`, especially `docs/protocol.md`,
`docs/openapi.json`, `docs/configuration.md` and `docs/health.md`.
