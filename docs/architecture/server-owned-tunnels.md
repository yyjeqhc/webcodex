# Server-owned OpenAI Tunnels

A long-lived Server can own up to 16 explicitly embedded named profiles. This is
an opt-in lifecycle adapter over `webcodex-openai-tunnel::TunnelClient`, not a
second Tunnel wire implementation. No existing profile is migrated by upgrade.

## Configuration and ownership

`WEBCODEX_TUNNEL_ENVIRONMENT` selects one absolute, private Environment directory.
The Server reads its `tunnel.json` once at startup. Old records without
`host_mode` retain `standalone`; only records explicitly marked `embedded` are
loaded. An embedded record cannot also claim an installed or started standalone
service. Duplicate profile names and duplicate Tunnel identities are rejected.
Configuration is capped at 64 records, 16 embedded profiles and 32 KiB per profile.

Each embedded profile uses the existing layout:

```
<environment>/server/tunnels/<profile>/webcodex.env
<environment>/server/tunnels/<profile>/readiness.json
```

Its private `webcodex.env` must contain exactly one non-empty value for each of
`WEBCODEX_TUNNEL_PROFILE_ID`, `CONTROL_PLANE_TUNNEL_ID`, `CONTROL_PLANE_API_KEY`, and
`WEBCODEX_TOKEN` (local MCP credential). The profile ID must match the record.
The local target is derived from the actual Server listener, not from the profile
or an arbitrary URL. Wildcard listeners map to the corresponding loopback address;
non-loopback-only listeners are rejected for embedded use.

An optional `WEBCODEX_TUNNEL_PROXY` is an explicit HTTP(S) control-plane proxy,
for example `http://127.0.0.1:7890`. Absence means direct control-plane access;
ambient proxy variables are not used by embedded profiles. The local MCP hop and
readiness probe always disable proxies, redirects and implicit request retries.
Identity, key, local token and proxy form one immutable binding. Loading a missing
field never consults inherited process environment or another profile.

For an installed user Environment, the explicit command is:

```
webcodex environment tunnel-host <profile> --host embedded
webcodex environment tunnel-host <profile> --host standalone
```

Use the existing Environment selection options. Stop the previous owner cleanly
and uninstall its exact standalone service before transferring ownership; the
command refuses a live embedded owner or remaining standalone installation. It
does not erase run markers, start a replacement owner, or migrate credentials to
another Windows account. Windows system-service ACL transfer is not implemented;
retain standalone for that configuration. Restart the selected owner explicitly.

## Startup, status and teardown

HTTP and the Tunnel supervisor are polled together. Each profile first proves its
authenticated local MCP endpoint ready and then starts its owned Tunnel task.
Startup is bounded at 60 seconds. The supervisor observes task completion for its
entire lifetime; dropping it aborts every retained task instead of detaching it.
It does not automatically restart a failed or uncertain task.

Runtime information reports safe profile names, host mode, lifecycle state,
`tunnel_ready`, `local_mcp_ready`, shutdown outcome and a static diagnostic code.
Environment status uses the owning Server service for embedded profiles and the
existing per-profile service for standalone. Readiness files keep the existing
freshness contract. Transport readiness is not evidence of a successful client
operation, and process liveness is not either readiness bit.

Server termination runs these phases in order:

1. Stop new Tunnel polling/admission.
2. Settle already admitted commands and their response delivery using their
   original deadlines, with a ten-second aggregate drain bound. Keep local HTTP
   and MCP available throughout this phase.
3. Observe the exact owned task's inner result, not merely a successful join.
   Clean completion removes its private restart fence. Errors, aborts, unfinished
   work and unconfirmed delivery retain the fence.
4. Only then begin the existing Server HTTP admission fence and graceful drain.

The adapter grants twelve seconds for the owned library task; the Server's
supervisor phase has a fifteen-second outer bound. This precedes the existing
HTTP drain budget. External service managers must allow both phases plus margin;
a ten-second TERM-to-KILL wrapper is not sufficient for this mode.

## Compatibility and uncertainty

Standalone `webcodex-tunnel`, `webcodex server tunnel`, and `webcodex share` remain
supported. Standalone retains standard proxy environment handling. The library's
`new` constructor keeps that behavior; `new_with_proxy` selects an explicit policy.
`run` now performs a ten-second graceful drain, and `run_with_drain` permits an
explicit nonzero bound up to 120 seconds.

Wire command/response schemas, status acknowledgement rules and standalone
`ProtocolCompatible` deadline semantics are unchanged. Embedded WebCodex continues
to require finite command deadlines, capped at 120 seconds. No command is locally
replayed after possible dispatch. A response retry is not an MCP execution retry.
An uncertain marker requires operator reconciliation, never an automatic cleanup
intended to make restart appear successful.

## Focused validation

```
cargo test --locked -p webcodex-openai-tunnel
cargo test --locked -p webcodex-environment
cargo test --locked -p webcodex --lib tunnel
cargo test --locked -p webcodex --lib server_shutdown
cargo check --locked -p webcodex-cli
```

Tests cover independent proxy and credential bindings, local-hop bypass, inherited
credentials, old standalone records, admitted queue drain, delivery uncertainty,
exact task errors, interrupted ownership, guard reuse after a clean stop, and HTTP
availability throughout ingress settlement. Real deployment acceptance is recorded
separately; passing loopback fixtures is not a claim of a live OpenAI Tunnel test.
