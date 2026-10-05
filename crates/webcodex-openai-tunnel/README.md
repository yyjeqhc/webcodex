# Secure MCP Tunnel client

A standalone Rust client for the OpenAI Secure MCP Tunnel wire protocol. This
crate has no dependencies on other WebCodex crates. It forwards one `main`
channel to one fixed Streamable HTTP MCP endpoint. JSON-RPC requests, no-ID
notifications, SSE notifications/results and session termination are supported.

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

The default is `RequireFinite { max_duration: 120 seconds }`. This is a
security-oriented execution policy, **not** exact behavioral parity with the Go
client. Supported timeout values use the Go signed 64-bit nanosecond range.

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

There is no SQLite ledger and no across-restart exactly-once claim. A host must
resolve prior effects and pending work before reconstructing a client following
an unclean/uncertain run. WebCodex enforces this with an exclusive, synced,
per-tunnel run marker in its private user state. It removes the marker only after
observed shutdown with no uncertain work. Crash, aborted owner, incomplete MCP
exchange or unconfirmed terminal delivery retains it. The marker is a coarse
restart latch, not a distributed transaction or an execution receipt database.
It is local to that state directory; do not run the same tunnel on another host
or with a different state root to bypass an unresolved run.

The crate owns no background tasks. `run` owns its poll and worker futures and
cancels them on shutdown. `Health` exposes safe local observations; readiness
requires a successful poll and never claims that a ChatGPT connector was created.
Hosts must monitor run completion and separately verify local MCP readiness.

## Scope and validation

No stdio, Harpoon, Cloudflare companion, OAuth rewriting, admin UI, profiles,
routing correction or durable dedupe. WebCodex supplies its existing authenticated
MCP endpoint; this crate does not bypass runtime authorization.

Run `cargo test -p webcodex-openai-tunnel`. Integration fixtures use bounded
loopback HTTP servers and synthetic credentials. They cover ambiguous response
and notification delivery, repeated poll commands, side-effect disconnects,
headers, JSON-RPC errors, SSE framing, session termination, deadlines, capacity,
authorization failures, redirects and cancellation. They are not evidence of a
live OpenAI service or real ChatGPT end-to-end acceptance.

For packaging, use `cargo package -p webcodex-openai-tunnel`. Publication is a
separate authorized release action; package metadata/version are independent of
the WebCodex application version.
