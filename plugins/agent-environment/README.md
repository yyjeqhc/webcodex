# Agent Tool Environment Bridge (Pi P0)

Reuse a configured Pi tool environment from WebCodex without starting a second
LLM reasoning loop or publishing every native tool as a global MCP tool.

This first-party **native Tool Plugin** publishes one project-bound tool,
`agent_environment`. WebCodex owns Runner/Project/provider authority and dispatch;
Pi owns its native catalog, schema validation, hooks, permissions, execution and
transcript. **Project/cwd binding is routing, not a sandbox.** A delegated Pi
`bash` is not WebCodex `run_shell` and does not inherit its effect/filesystem policy.

## Install

From this repository, with Node >= 22.19 (tested on 22.23.2):

```sh
npm ci --prefix npm/plugin-sdk
npm --prefix npm/plugin-sdk run build
npm ci --prefix plugins/agent-environment
```

The adapter pins Pi `1.0.0`; do not independently upgrade its dependencies without
updating and testing `src/pi-compat.mjs`. It is a repository-distributed private
package, not an npm-published release.

Add a provider to the selected Runner's own `runner.toml`, using real absolute
paths for your installation and an already authorized Project root:

```toml
[plugins]
request_timeout_secs = 30

[[plugins.providers]]
id = "pi-workspace"
name = "Configured Pi environment"
command = "node"
args = [
  "/absolute/webcodex/plugins/agent-environment/src/plugin.mjs",
  "--agent-dir", "/absolute/pi-agent-config",
  "--state-dir", "/absolute/private-bridge-state"
]
cwd = "/absolute/authorized-project"
timeout_secs = 30
```

Omitting `--agent-dir` uses Pi's normal `getAgentDir()` resolution. Passing it
selects an explicit existing or isolated Pi configuration, including settings,
packages/extensions and MCP configuration. Do not copy real credentials into
this repository. Native extensions and MCP servers remain trusted executable
code under Pi's own trust/loading rules. Their credentials stay local to that
environment; this bridge does not read or forward the user's LLM `auth.json`.
The normal Runner Plugin environment inheritance/isolation policy still applies
to native tools; this is not automatic credential sanitization for arbitrary
user-installed extensions.

`--state-dir` contains the bridge's own offline provider files and persistent Pi
transcripts. Use a private directory. Its default is a cwd-derived directory
under `~/.webcodex/agent-environments/`. The bridge overrides Pi retry/compaction
settings in memory only, without rewriting the user's settings or LLM credentials.

Use `check_runner_config`, then `reload_runner_config` with the observed
`current_generation`, to activate a changed Runner configuration without restarting
the Server or Runner. Follow the precise reload semantics below.

## Invocation

Use the existing `plugin_tool` gateway, directly where exposed or through
`call_runtime_tool`. No new global WebCodex tool names are installed.

First describe the fixed Plugin tool with an **explicit** authorized Project:

```json
{
  "action": "describe",
  "runner": "special",
  "plugin": "pi-workspace",
  "tool": "agent_environment",
  "project": "agent:special:your-registered-project"
}
```

Retain its opaque outer `binding`. Describe requires `plugin:inspect` and
`project:read`. Calls require `plugin:invoke` and `project:write`, including native
session creation/discovery through this gateway: loading extensions/session hooks
may have effects. Calls also pass existing WebCodex permission and recording
Session guards. The current Runner registration must remain enabled and writable
(`allow_patch = true`), even if the root path has not changed. This gate does not
turn Pi's native execution into a filesystem sandbox. An outer call cannot change
`project`, Runner, provider or tool.

Each subsequent `plugin_tool` call has this shape:

```json
{
  "action": "call",
  "binding": "<outer binding>",
  "arguments": { "action": "sessions" }
}
```

`sessions` lazily creates and reports **one managed native Pi session per Plugin
provider process**. It does not attach to the most recent session, an existing
Pi terminal/TUI, or a ChatGPT window. Retain its exact `session` and `next_request`.
The operator selects an environment through provider configuration; model input
cannot supply arbitrary cwd, configuration paths or LLM credentials.

Pass the following inner arguments through the same outer binding:

| Action | Inner arguments and behavior |
| --- | --- |
| `sessions` | Report/start this provider's managed session and one-time request token. |
| `tools` | Exact `session`; optional `query`, `cursor`, `limit`. At most 32 summaries per page. Search includes callable deferred tools. |
| `describe` | Exact `session`, native `tool`; returns exact native `inputSchema`, exposure, access route and per-tool `binding`. |
| `call` | Exact `session`, current `request`, 1–16 `calls`, optional `timeout_ms` / absolute Unix-millisecond `deadline`. |
| `refresh` | Exact idle `session`; native Pi reload, generation invalidation and refreshed catalog. |
| `history` | Exact `session`; last 1–16 bounded native tool results, default 8. Not full transcript export. |

A native batch uses caller-supplied unique call IDs and observed tool bindings:

```json
{
  "action": "call",
  "session": "<exact native session handle>",
  "request": "<current next_request>",
  "timeout_ms": 20000,
  "calls": [
    {
      "id": "read-1",
      "tool": "read",
      "binding": "<read schema binding>",
      "arguments": { "path": "README.md" }
    },
    {
      "id": "shell-1",
      "tool": "bash",
      "binding": "<bash schema binding>",
      "arguments": { "command": "git status --short" }
    }
  ]
}
```

The bridge sends the whole batch to Pi; it does not independently parallelize
native tools or assume parallel safety. Returned records have `id`, `tool`,
`is_error` and `result`, in input order but correlated by exact native call IDs.
A native validation/hook/tool failure is a completed error result; mixed batches
keep both successful and failed records. Do not put dependent write/edit operations
in one batch unless Pi's configured execution semantics guarantee the dependency.

## Exposure and native discovery

- Active `direct` and `model-only` tools can be emitted as native model calls.
  Model-only tools are not made callable from codemode.
- Callable but undeclared `codemode` and `deferred` tools remain
  `native_orchestrator_only`. Describe exposes their native schema; invoke them
  through the configured Pi `codemode`/`tool_search` path, not a synthetic direct
  bridge call. The bridge never activates tools to bypass that restriction.
- Deferred tools appear in a matching search, not in unfiltered pages.
- `hidden` and unavailable/inactive tools are not exposed or directly dispatched.

For example, after describing the **native** `codemode` tool, its `code` argument
can use Pi's own `tools.<nativeName>(...)` API. MCP and extension tools execute
inside Pi's native pipeline, including its hooks and schema validation.

The bridge reproduces Pi CLI built-in extension loading because Pi 1.0's bare
SDK omits the CLI's MCP/codemode/tool-search factories. It does not import a
second LLM provider or execute native ToolDefinition handlers itself.

## Identity, reload and outcome rules

Native handles contain the native session UUID and a provider-process epoch.
Tool bindings include the session/environment generation and observed schema and
exposure. Unrelated lazy catalog additions do not invalidate an unchanged tool's
binding; explicit refresh does. Page cursors additionally fence the full catalog.
A call rechecks declarations after Pi startup/loadout hooks, at the local provider
boundary. Model/provider, native session and cwd changes fail closed.

There are three different operations:

| Operation | Identity behavior |
| --- | --- |
| Inner `refresh` | Runs Pi's native `session.reload()` in place. In the tested Pi 1.0 environment, a newly installed `.js` extension became available with the same native session ID and no Server/Runner restart. Old schema bindings become stale. |
| `reload_runner_config` | Generation-fenced Runner Hot config activation. An unchanged Plugin configuration/environment retains providers. Existing Runner behavior replaces the whole provider set when that configuration/environment changes; add/remove is supported without Runner restart, but unchanged members of a changed set are **not** individually retained. |
| `plugin_tool` `action=reload` | Existing explicit Plugin/code-reload primitive. Intentionally recreates providers even if runner.toml is unchanged. All old outer/native bindings become invalid. |

After configuration changes, use the appropriate explicit operation, then observe
and describe the resulting exact target. Never interpret discovery as permission
to replay a previous effect. File watching, per-provider incremental retention,
and attachment to an existing Pi TUI are not implemented in P0.

Calls consume `next_request` before native dispatch. A consumed token cannot
replay the request. The native budget defaults to 20 seconds, capped at 90 seconds;
keep it below the Runner provider timeout to receive a native cancellation result.
A deadline that has already expired returns `not_started` without consuming a
native effect. A timeout/cancellation after entering native hooks/tool execution
returns **`outcome_unknown`**, requests Pi abort, and quarantines the session.
An abort acknowledgement cannot undo a tool's prior effects. A tool ignoring
cancellation can still finish: the real dogfood intentionally proves this.

Quarantined sessions allow bounded observation where identity is intact, but
reject calls and native refresh. Reconcile filesystem/transcript effects first;
an operator may then explicitly recreate the provider. No automatic replay,
resume, fresh-session fallback, or hidden recovery is performed. There is no
separate concurrent interactive-cancel UI in this Plugin P0; native deadlines
and Runner-owned provider cancellation/termination are the supported controls.

The outer Plugin transport and inner native effect have separate outcomes:
transport completion does **not** prove a native effect completed. Always inspect
`native_dispatch_state`, `error`, `no_replay` and per-call error fields. Runner
transport timeout/disconnect after dispatch uses its existing outer
`outcome_unknown` handling. Trusted Runner/provider/Project provenance appears
separately from native output in direct MCP text and in the generic Runtime
`delegation` envelope. Outer MCP caller `_meta` is never forwarded to Pi.

## Limits and verification

Native names retain their original spelling, including non-ASCII names. Long help
prose is truncated with `description_truncated: true`, not the native input schema.
Native schemas and output are bounded. Very large/deep schemas fail describe
explicitly instead of returning an unusable partial schema. Large native output
is summarized with `omitted: true`; full native transcripts remain in the private
state directory. Small images/details/resources may remain nested in structured
output; this is not a full rich-resource/UI rendering subsystem.

This is headless SDK execution. Native hooks and permission blocks are preserved;
interactive Pi TUI/extension widgets are not relayed to ChatGPT. An extension
requiring an interactive human UI may deny, error or wait until cancellation
according to native behavior. The bridge does not synthesize approval.

```sh
npm --prefix plugins/agent-environment test
npm --prefix plugins/agent-environment run test:pi
cargo build --profile dogfood -p webcodex --bin webcodex-server \
  -p webcodex-runner --bin webcodex-runner
python3 scripts/e2e_agent_environment_ws.py
```

The last test launches disposable real loopback MCP Server/WS Runner processes,
real Pi, native extension and MCP fixtures, permissions, batches, dynamic reload
and post-dispatch deadlines. It does not contact a remote LLM. `--keep` retains
local diagnostic directories containing **disposable test credentials**; never
commit or share those directories. Without it, the test cleans them up.
