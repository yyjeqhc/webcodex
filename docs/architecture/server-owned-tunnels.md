# Server-owned OpenAI Tunnels

This document describes the OpenAI adapter. Cloudflare Named/Quick reuse its
Environment catalog and shared Server shutdown ordering, while using their own
OAuth-only loopback ingress and process-tree owner. See
[Cloudflare connections](../CLOUDFLARE_CONNECTIONS.md) for the entry/epoch,
standalone-control, forwarding-proof and client-lifecycle contracts.

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
There is deliberately no dynamic profile reload: a running Server continues with
the startup catalog until the operator explicitly restarts it.

For a persistent Environment, `EnvironmentStore` is the only Tunnel profile
catalog authority. CLI and Desktop read and mutate the same `tunnel.json` records
and the same private per-profile bindings. The historical Desktop
`secrets/tunnel-config.json` remains authoritative only for legacy/non-persistent
Desktop runtimes. If that file is present after entering persistent mode, Desktop
uses it only as an identity conflict fence: matching profile/Tunnel identities may
coexist, while a missing or different identity fails closed. API-key bytes are not
compared after authority transfer because canonical credentials can rotate behind
the EnvironmentStore revision fence. The historical file is never silently copied
over, overwritten, or deleted, and extra CLI-created Environment profiles remain
visible in Desktop.

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
Profile ID, Tunnel ID, local token and proxy remain exact per-profile bindings.
An API key may be replaced only by a write-only, current-revision-fenced update;
loading a missing field never consults inherited process environment or another
profile.

Create a Server-owned profile directly instead of creating a standalone service
and transferring it later:

```
webcodex environment configure-tunnel work \
  --host embedded \
  --credentials-file /secure/work.json
```

This writes the exact profile record and private binding with `host_mode=embedded`,
adds the selected Environment to the owning Server configuration, and never
installs or starts a standalone Tunnel service. The structured result reports
`server_restart_required` and a `next_action`. A running Server is not hot-reloaded
or restarted implicitly, so operators can configure several profiles and perform
one explicit `webcodex environment restart server` afterward. If the Server is
stopped or absent, configuration still succeeds and reports `start_server`.
`--host standalone` retains the existing install/start semantics for a separate
per-profile service.

For an existing profile, ownership transfer remains a separate explicit operation:

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
