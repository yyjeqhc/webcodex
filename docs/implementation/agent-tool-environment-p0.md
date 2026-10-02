# Agent Tool Environment Bridge P0 (#817)

## Scope and selected boundary

Implemented against WebCodex main `c2cd4f4877273f73d5592524b117e3bc6ebe25d1`,
following v0.5's separation of canonical capabilities from model-visible tools.
The first adapter uses the installed Pi `1.0.0` SDK. It provides configured native
tool-environment reuse, not another autonomous coding agent.

```text
ChatGPT / another reasoning Host
  -> existing plugin_tool (direct or call_runtime_tool)
  -> existing Project/Runner authority, permission, audit and instance fences
  -> existing Runner Plugin transport
  -> one fixed projectBound agent_environment tool
  -> exact managed native Pi session
  -> local provider emits supplied tool-call IDs/names/arguments
  -> Pi validation / native hooks / native tool execution
  -> turn_end native tool results
```

No global `pi_read`, `pi_bash`, per-MCP-tool definitions, new generic runtime,
second transport, second model reasoning loop, or Plugin tool-count increase.

### Alternatives considered

**Native Tool Plugin (selected).** Its existing process lifecycle, schema and
provider identity fencing, Runner routing, audit and transport outcomes fit P0.
The fixed outer catalog contains only the dynamic namespace gateway, so eager
Plugin v1 discovery does not bound the inner environment to 128 native tools.
The one missing business-authority primitive was explicit Project binding; that
is a typed generic Plugin feature, not a Pi-only special case.

**Runner-local MCP / Chappie server.** Existing local `mcp_tool` is a viable
transport for ordinary MCP servers, but Chappie's server currently obtains its
conversation identity from `context.mcpReq._meta['openai/session']`. WebCodex's
outer metadata trust boundary deliberately forbids forwarding it. Integrating
that broker requires a different explicit-session protocol and would import more
lifecycle/resource machinery than this P0 needs. No outer metadata passthrough
was added and the local MCP gateway remains unchanged.

**Dedicated agent core gateway.** Could express the namespace directly, but would
add core dispatch/contracts/Runner operations that duplicate the existing Plugin
transport and authority. This P0 has no need to own a second general runtime or
to abstract several unimplemented agents.

## Chappie reference, not a vendored runtime

Read the local Chappie checkout `6f380df` (v1.1.0) under `/root/git/chappie`:
`host.ts` separates host/native operations; `session.ts` serializes broker
requests, emits `output.toolCalls(request.calls)` then `output.done('toolUse')`,
and collects native results; `broker.ts` tracks registered exact sessions and
pending requests. `server.ts` supplies the small external gateway and its
ChatGPT metadata binding. `pi.ts` uses Pi's native provider path and `turn_end`;
OMP follows the same native-provider pattern, while OpenCode/Codex adapt their
own provider responses and result capture. `local.ts` shows explicit session
arguments as an alternative to implicit window selection.

This implementation borrows that **execution principle**, not Chappie's broker,
HTTP/IPC transport, question relay or resource subsystem. Chappie itself was not
installed/run as part of the acceptance test; the real dogfood uses the WebCodex
adapter and installed Pi. The older `/root/git/pi` checkout was not treated as
Pi 1.0 evidence; API decisions were verified against the actual installed package.

## Authority and provenance

The native Plugin descriptor and exact schema observation contain a typed
`projectBound` boolean. It is deliberately outside advisory MCP annotations.
Server `describe` requires an explicit Project for such tools and retains its
canonical ID, Runner-local ID and root fingerprint in the opaque Plugin binding.
The actual call reauthorizes that Project and its write scope, verifies the
Runner, and selects permission policy for the explicit business Project.
Recording Workflow Sessions still supply provenance/guards, never the target.

The Runner snapshots both the canonical provider launch root and its physical
directory identity, then rechecks the current registered Project id/root/fingerprint,
physical root and enabled/write permission while holding the provider dispatch lock.
This rejects rename + same-path directory replacement even though the textual path is
unchanged and the already-running provider would otherwise retain its old cwd.
Revoking `allow_patch` likewise denies new native calls without requiring a provider
or path change. The existing lightweight registry context carries this flag; tool
admission does not spawn Git/status probes. Concurrent calls return
`plugin_provider_busy`, rather than queuing an observed old target. A removed/replaced
registry root, mismatched Project, stale schema, provider instance or Runner instance
fails closed before native dispatch.

Trusted delegation provenance (Runner/provider instances and canonical Project)
is rendered separately from untrusted Pi content. Direct MCP preserves the
native `structuredContent` schema and prepends an explicitly trusted target text.
Generic Runtime output adds a separate `delegation` envelope. Native session,
source `pi`, request, native tool, result/error state remain in the bounded native
payload. Caller `_meta` and model-supplied configuration paths are not passed on.

This does **not** enforce WebCodex shell/file policy inside Pi. A native tool has
Pi's own filesystem/process/network behavior and any native sandbox. `cwd` is a
routing observation, not a sandbox. Credentials required by native MCP/extensions
remain native configuration responsibility. The local model provider uses its
own offline state and never reads the user's LLM auth file or requests remote
model inference; native tools may legitimately perform their own network work.

## Pi 1.0 semantics actually exercised

| Native exposure | Bridge mapping |
| --- | --- |
| Active `direct` | Native model-call declaration; describe then call. |
| Active `model-only` | Native model-call declaration, but not promoted into codemode's callable set. |
| Callable undeclared `codemode` | Discoverable schema marked `native_orchestrator_only`; invoke through native codemode. |
| Callable `deferred` | Search-based discovery; preserve native tool-search/codemode route rather than force activation. |
| `hidden` or unavailable | No bridge discovery/direct invocation. |

A catalog page is at most 32 summaries. Query/cursor/schema/output/batch sizes are
bounded; the full environment is not registered as global tools. The tests include
an environment with 2,048 native catalog entries and paginated discovery.
Tool bindings use native session/epoch, explicit refresh generation and exact
schema/exposure/callability, not array position or a last-seen session. Unrelated
lazy tool additions retain an unchanged tool binding; page cursors fence the full
catalog and become stale when it changes.

Pi 1.0's request tools live in transcript system-message diffs, not a legacy
`context.tools` list. The adapter uses the public `getCurrentTools(messages)` to
check effective declarations at provider dispatch after native startup hooks.
The whole tool batch is emitted once as a native assistant tool-call message;
Pi performs native validation/hooks/execution and the adapter consumes `turn_end`.
No native tool handler is invoked directly. Input order is restored from exact
request-prefixed native call IDs; unknown/duplicate/missing result IDs quarantine
rather than risk confusing effects.

Two read-only version-specific seams are isolated in `src/pi-compat.mjs`:
CLI built-in extension factories, omitted by bare SDK initialization, and the
native hidden-declaration projection. They are pinned to Pi 1.0.0 and fail closed
when unavailable. The bridge must import CLI MCP/codemode/tool-search factories
or it would silently lose configured tools. These are acknowledged compatibility
limits, not claims of a stable cross-version Pi SDK extension.

Native extension auto-discovery in the tested package scans `.js`/`.ts` files,
not `.mjs`. The fixture is intentionally copied into the isolated Pi directory
as `.js`. Installing a new `.js` extension then calling native `refresh` made it
available with the same native session and no Server/Runner restart.

## Lifecycle and uncertainty

Each provider process owns one explicitly reported fresh native Pi session; P0
does not attach to existing TUI sessions or infer targets from windows/cwd alone.
No auto-resume/reconnect/replay is performed after replacement. Persistent native
transcripts live in the operator-selected private state directory.

A batch needs the current one-time `next_request` and per-tool schema bindings.
The request token is consumed before dispatch. Native validation/hook failures
return completed native error records. A signal/deadline already cancelled before
dispatch returns `not_started`. After entering native startup hooks or tool
execution, timeout/disconnect without conclusive results means `outcome_unknown`.
Pi abort is requested, but cannot undo prior effects and may be ignored by tools.
That session is quarantined; calls and native refresh are rejected until the
operator reconciles effects and explicitly replaces the provider.

Initialization/reload themselves can run extension hooks. Their failure is also
uncertain and quarantined; a repeated `sessions` request cannot silently retry a
partially failed initialization. A successful native reload invalidates prior
schema observations. Native history is bounded and can aid reconciliation; it is
not proof that a timed-out effect did not occur.

The outer Plugin transport can complete with an inner `native_dispatch_state` of
`outcome_unknown`. It would be incorrect to let arbitrary Plugin payloads select
WebCodex transport certainty. Both layers are therefore retained. An actual
Runner transport deadline uses the existing outer `outcome_unknown`/provider
retirement semantics and does not auto-replay Pi bash.

### Reload distinctions

1. Inner `refresh`: native Pi session reload, same session where Pi supports it,
   schema generation advances; this is the environment catalog-refresh path.
2. General Runner config activation: existing `check_runner_config` /
   `reload_runner_config(expected_generation)`; unchanged Plugin configuration and
   launch environment retain identity. Provider add/remove is supported live.
3. Explicit Plugin reload: existing `plugin_tool action=reload`; always restarts
   providers even if the configuration text is unchanged (code reload).

Existing Runner config currently reconstructs the **whole** provider set when a
Plugin configuration/environment changes. It does not retain individual unchanged
members of a changed set. This P0 preserves that behavior and fences every stale
binding; per-provider incremental retention is a separate improvement, not a
reason to invent a file watcher or claim a restart is required for native refresh.

## Verification and remaining scope

Automated coverage includes catalog bounds/exposures, invalid/stale identities,
schema changes/removal, duplicate names/call IDs, malformed arguments, native
partial failure, pre-dispatch cancellation/deadline, timeout/disconnect uncertainty,
metadata/credential sentinels, native initialization/reload quarantine, core typed
Project fences and Runner authorization before effects. The first-party Plugin
CI lane runs SDK tests, bridge unit tests and the real isolated Pi integration.

`plugins/agent-environment/test/pi.integration.mjs` exercises the installed Pi
SDK through native Plugin stdio. `scripts/e2e_agent_environment_ws.py` adds real
loopback MCP Server and websocket Runner processes, exact Project/scope/owner
rejection, generic/direct transport parity, real builtins/extension/MCP hooks,
partial batches, native codemode/deferred tools, dynamic refresh, provider
lifecycle and actual outer Runner deadline after a filesystem effect.

P0 limitations: Pi 1.0.0 only; no arbitrary existing native-session attachment;
headless interaction rather than a relay for Pi TUI confirmations/widgets;
conservative write authority for the whole dynamic gateway; per-provider-set
rather than per-provider config retention; bounded rich output without Chappie's
resource renderer; no automatic continuation after uncertain effects. Linux on
special is the dogfood platform; other OS/agent adapters are not claimed tested.

See the [operator guide](../../plugins/agent-environment/README.md) and the
[dogfood record](agent-tool-environment-p0-dogfood.md) for commands and evidence.
