# Native Tool Plugins

[English](PLUGINS.md) | [简体中文](PLUGINS.zh-CN.md)

Native Tool Plugins let a WebCodex Runner expose tools implemented by an
arbitrary local executable. A Plugin can be Node.js, Bun, Deno, Python, `uv`,
Ruby, a Rust/Go/C/C++ binary, or another program that can read stdin and write
stdout. It does **not** need to implement an MCP server or depend on an MCP SDK.

The Plugin process runs on the Runner machine. The Server never receives its
command, argv, cwd, prepared environment, PID, stderr, or local credentials.

Native Plugins are **trusted local executables**. WebCodex does not sandbox,
contain, sign, or otherwise make an untrusted executable safe. A Plugin has the
same practical local-process trust implications as launching that executable
directly with the prepared Runner environment.

## Configure a Plugin

Plugins have their own `runner.toml` section; they are not MCP providers:

```toml
[shell]
default_profile = "node"

[shell.profiles.node]
program = "bash"
args = ["-lc"]
init_script = """
source ~/.nvm/nvm.sh
nvm use 22
"""

[plugins]
request_timeout_secs = 30

[[plugins.providers]]
id = "repo-tools"
name = "Repo Tools"
command = "node"
args = ["tools/plugin.mjs"]
cwd = "/root/git/example"
profile = "node"
timeout_secs = 30
```

`id`, `name`, and `command` are required. `args`, `cwd`, `profile`, and
`timeout_secs` are optional. `cwd`, when present, is an absolute path.

Plugin execution deliberately reuses the Runner shell environment model:

1. `plugins.providers[].profile`, when set;
2. otherwise `shell.default_profile`;
3. otherwise the base Runner shell environment.

The selected shell profile prepares an environment snapshot, including
`shell.path_prepend`, `shell.env`, profile environment, and profile
`init_script`. The Plugin itself is then launched as a **native argv process**,
not through a shell. A bare command such as `node`, `python`, `uv`, or `bun` is
resolved from the prepared snapshot's `PATH`, not only from the Runner parent
process PATH. Sensitive WebCodex process credentials are filtered from that
environment.

On Windows, native executables follow the Runner's normal `PATH`/`PATHEXT`
rules. `.cmd` and `.bat` commands are rejected for this ABI because they require
shell semantics; configure the native runtime executable instead.

## Startup and Dynamic planes

There are exactly two Plugin planes.

### Startup

When the Runner process starts, it reads `[plugins]`, prepares each provider,
starts it, performs `initialize`, calls `tools/list` **once**, validates the
bounded Tool schemas, canonicalizes the result, and freezes that catalog against
the exact `provider_instance_id`. The admitted provider process remains
persistent for calls, but ordinary discovery/call paths never ask that same
instance to list again.

The frozen provider catalog is immutable for that provider-instance lifetime.
Its canonical validated form has a stable SHA-256 catalog digest and exact Tool
count for stale detection/audit. A catalog can change only through a new
provider instance created by reload/restart. The startup plane itself remains
immutable for the lifetime of the Runner process.
Editing Plugin source or `runner.toml`, or using `plugin_tool reload`, does not
change first-class tools. A Runner restart creates a new Runner/provider
instance and performs startup admission again.

The complete provider catalog and the startup direct-eligible subset are
separate contracts. `PLUGIN_STARTUP_MAX_DIRECT_TOOLS` (currently 64) is a Runner
safety cap for the direct subset, not a claim that a provider can expose only 64
Tools and not a per-model/per-surface routing policy. A bounded provider catalog
can therefore remain usable through `plugin_tool` even when no direct subset is
admitted. Server-side surface budgets remain a separate governance concern.

If a startup tool name is valid, bounded, unique across the caller-visible
startup inventory, and does not conflict with a WebCodex-reserved tool name, it
appears directly in MCP `tools/list`. The model can call it normally, for
example:

```text
search_symbol({"query":"RunnerRegistry"})
```

Duplicate or reserved names are not exposed directly. They remain reachable
through the dynamic `plugin_tool` describe/binding flow with an explicit Runner
and Plugin provider at describe time.

A provider process failure does not prevent the Runner itself from registering.
If an admitted startup provider later fails, WebCodex retires that exact
provider instance and fails direct calls closed. It does not silently restart
the provider under the same identity.

### Dynamic

Committed Plugin discovery uses one three-level ladder:

```text
plugin_tool(action="list")
    -> caller-visible Plugin-capable Runners
plugin_tool(action="list", runner="my-runner")
    -> current effective providers on that exact Runner
plugin_tool(action="list", runner="my-runner", plugin="repo-tools")
    -> bounded current tool names/titles for that exact provider
plugin_tool(action="describe", runner="my-runner", plugin="repo-tools", tool="search_symbol")
    -> { ..., "binding": "wc_pbind_..." }
plugin_tool(action="call", binding="wc_pbind_...", arguments={"query":"foo"})
```

`list(runner, plugin)` is a runtime observation of the already committed/effective
provider. It resolves the exact caller-visible Runner and provider instance, then
reads that instance's frozen catalog locally. It does **not** issue another
provider `tools/list` round trip, reread `runner.toml`, start a disposable
candidate, run `check`, reload a provider, mutate the dynamic overlay, create a
binding, or alter `firstClassRestartRequired`. If the exact Runner/provider was
replaced, discovery fails closed and tells the caller to re-list; WebCodex never
silently resolves the same logical name to the replacement or replays the
operation. Full schemas remain exclusive to `describe`; list returns only
bounded discovery metadata such as tool names and optional titles.

Provider-list `status` describes current effective runtime health. When the
logical provider also existed in the frozen startup catalog, `startupAdmission`
describes that separate frozen admission (`direct`, `secondary`, or `failed`),
with `startupAdmissionCode` when applicable. Reloading a dynamic provider does
not rewrite startup admission. Even `startupAdmission=direct` does **not**
guarantee that every caller sees a first-class MCP tool: Server-side reserved
names and caller-visible duplicate Plugin tool names can still suppress direct
exposure.

`check` is the recommended preflight before `reload`. The Runner rereads its
current `runner.toml`, locates only the requested provider, prepares the same
shell/profile environment used by the normal Plugin runtime, resolves and
**really starts** the configured executable, performs `initialize` and
`tools/list`, validates the normal Plugin protocol/bounds, then terminates that
candidate process tree. It never calls provider `tools/call` and never commits
the candidate into the dynamic overlay. Because a Native Plugin is an arbitrary
local executable, its own startup/initialize/list behavior can still have
external side effects; `check` is not a purely static config linter.

A successful check returns `ready=true` plus bounded tool summaries containing
only names and optional titles. A broken candidate is normally a successful
check operation with `ready=false`, a structured `phase`, a stable error `code`,
and a bounded WebCodex-generated `detail`. Tool-definition validation failures
also include a small `diagnostic` with a finite WebCodex-defined code and, when
safe, a validated tool name and finite field label such as `inputSchema`.
Diagnostics come only from WebCodex protocol parsing/validators; raw serde
errors, protocol lines, schema fragments, Plugin stdout/stderr, executable
configuration, environment, and process identities are never copied into the
report. Plugin stderr remains Runner-local. The Runner keeps only a bounded,
control-sanitized local stderr ring for live providers and the most recent
disposable `check` candidate; this local projection is not part of the Plugin
gateway response.

`startupToolShape.eligible=true` means only that this checked provider's own Tool
definitions satisfy the stricter per-provider startup Tool bounds. It is **not**
a guarantee of final first-class MCP exposure: actual startup admission also
depends on whole-catalog bounds, and Server exposure still depends on reserved
names and caller-visible name uniqueness.

`call` has one dispatch identity: the opaque `binding` returned by that exact
`describe`. It does not accept `runner`, `plugin`, or `tool` as call-time
routing fields. Each describe creates an independent binding for the exact
Runner instance, provider instance, tool name, and schema observation seen at
that moment. The handle does not encode or expose those internal identities.

`reload` is the cross-platform authoritative reload path on Windows, macOS, and
Linux. The Runner rereads its own `runner.toml`; the Server does not upload an
executable, environment, or raw Plugin config. Candidate management is
serialized across both `check` and `reload`: a second check returns
`plugin_check_busy`, while reload keeps the existing `plugin_reload_busy` result.
Candidate operations are rejected with `NotStarted` instead of being queued.
This gate does not block list/describe/call or direct startup tools from using the
currently committed provider while a candidate is being prepared.

A changed provider is initialized and listed successfully before it replaces
the previous dynamic instance. A failed candidate leaves the previous working
dynamic instance intact. A removed provider is removed from the dynamic view.
None of those operations changes the frozen startup catalog.

This creates the normal development loop:

```text
edit Plugin code/config
    -> plugin_tool check
    -> fix the bounded diagnostic until the disposable candidate is ready
    -> plugin_tool reload
    -> plugin_tool list(runner, plugin)
    -> plugin_tool describe
    -> receive opaque binding
    -> plugin_tool call(binding, arguments)
    -> finish debugging
    -> restart Runner
    -> new startup admission / first-class promotion
```

If startup provider A is first-class and reload creates dynamic provider B,
direct `search_symbol(...)` still calls A. A fresh dynamic `describe` observes B
and its returned binding calls only that exact B instance. Only a Runner restart
can replace the first-class startup binding.

Running `check` before that reload does not replace A or the current dynamic
provider, does not alter `firstClassRestartRequired`, does not create a binding,
and does not change the direct MCP tool inventory. The successful check candidate
is disposed instead of being reused by a later reload.

## WebCodex Plugin Protocol v1

The native protocol is newline-delimited JSON-RPC 2.0 framing with the protocol
version `webcodex-plugin-v1`. Each request or response is exactly one JSON line.
It is a WebCodex Plugin protocol, not MCP.

Initialization:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"webcodex-plugin-v1"}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"webcodex-plugin-v1"}}
```

Tool discovery:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```

```json
{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"add","description":"Add two numbers","inputSchema":{"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]}}]}}
```

Tool call:

```json
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":2,"b":5}}}
```

```json
{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"7"}],"structuredContent":{"sum":7},"isError":false}}
```

Tool definitions support `name`, optional `title`, `description`,
`inputSchema`, optional `outputSchema`, and optional `annotations`. The first
version of tool results supports only text content, optional object
`structuredContent`, and `isError`. Images, resources, sampling, callbacks,
streaming, pagination, and arbitrary JSON-RPC tunneling are not part of v1.

Messages, schemas, arguments, JSON structure, tool counts, and results are
bounded. Malformed or unsupported data fails closed instead of being truncated
into a different tool contract.

### Native Plugin Schema Profile v1

`inputSchema` and `outputSchema` use a deliberately small WebCodex profile; they
are **not** advertised as full JSON Schema 2020-12. Every schema node requires a
single string `type`. Supported types are `object`, `array`, `string`, `number`,
`integer`, `boolean`, and `null`. Supported keywords are:

- all nodes: `type`, optional `title`, `description`, `enum`, `const`;
- object: `properties`, `required`, boolean `additionalProperties`;
- string: `minLength`, `maxLength`;
- array: `minItems`, `maxItems`, `items`.

Input/output schema roots must be `type: "object"`. Property counts, required
entries, enum size, schema bytes/depth/nodes/strings, and declared length/item
bounds are finite. Unknown keywords are rejected at provider admission rather
than silently ignored. In particular v1 does not support `$ref`/remote refs,
`$defs`, recursive refs, `pattern`, `format`, numeric `minimum`/`maximum`, schema
forms of `additionalProperties`, union `type`, `anyOf`, `oneOf`, `allOf`, `not`,
or arbitrary draft-specific keywords.

See [`examples/native-tool-plugin.mjs`](../examples/native-tool-plugin.mjs) for
a minimal no-dependency Node example. The repository also ships
[`plugins/safe-delete`](../plugins/safe-delete/README.md), an optional
project-root-fenced Plugin that moves one file or directory to the operating
system Trash/Recycle Bin without adding permanent deletion to WebCodex's built-in
tool surface.

## Calling and failure semantics

Each provider handles one request at a time. Concurrent calls receive a
provider-busy result instead of being silently queued without bound.

Before `tools/call` can enter the provider connection, WebCodex resolves the
exact frozen catalog entry, checks the caller's exact schema observation, and
validates `arguments` against that frozen input schema. Any failure here is
`NotStarted`; the executable sees zero `tools/call` bytes. No `tools/list` is
performed on this path. After a provider response arrives, ordinary text/result
bounds still apply. When an `outputSchema` exists, `structuredContent` is also
required to match that exact frozen output schema. A mismatch is a completed
provider contract violation and retires that provider instance fail-closed.

`plugin_tool call` requires an opaque binding from a preceding `describe`.
Bindings are bounded server-side observations, not bearer authorization tokens:
every call still requires current `plugin:invoke` authority and current access to
the logical Runner. A binding can also be evicted. If its Runner/provider
instance disappears, the tool is removed, or its schema changes, the stale call
fails `NotStarted` and must be described again. WebCodex never re-resolves the
binding to a newer same-named provider/tool, never manufactures a replacement
binding, and never replays the call. Internal Runner/provider instance ids and
schema revision machinery are not exposed in the handle.

After the local frozen-schema preflight succeeds, the provider timeout is one
total RPC budget. It starts before provider-request encoding and covers bounded
stdin queue admission, the complete frame write plus flush confirmation, and the
response wait. A Plugin that stops reading stdin therefore cannot make
`write_all` escape the provider timeout.
For effectful `tools/call`, once a frame may have started writing, a write
timeout/failure, connection loss, process death, or response timeout is
`OutcomeUnknown`; WebCodex does not automatically retry or replay it. Failures
proven to happen before any possible send are `NotStarted`.

Plugin stdout is protocol-only. Plugin stderr is drained continuously on a
separate worker so stderr flooding cannot backpressure stdout protocol progress.
The local ring retains at most 64 lines, 1 KiB per line, and 32 KiB aggregate;
control/non-UTF-8 bytes are projected safely and overlong lines are marked
truncated. Stderr is never treated as protocol, inserted into a model ToolResult
or Workflow Session ledger, or automatically sent to the Server/startup
registration catalog.

## OAuth

Native Plugin authority is operation-specific:

- `plugin:inspect` allows metadata observation such as list and describe.
- `plugin:invoke` allows `plugin_tool call` and first-class startup Plugin tools.
- `plugin:manage` allows development/management operations that can start or
  change local Plugin processes, currently check and reload. It does not imply
  `plugin:invoke`.
- None of these scopes is part of the direct shared-key model baseline.
- For the shared-key OAuth bridge, opt in explicitly with
  `webcodex connect ... --auth oauth --oauth-local-plugins`; that opt-in grants
  only `plugin:inspect` + `plugin:invoke`, never `plugin:manage`.

`mcp:local` does not grant Plugin access, and Plugin scopes do not grant
Runner-owned MCP provider access. Effectful Plugin operations also pass the
same Workflow Session guard and authority-mode permission policy as other
consequential WebCodex execution when an explicit `recording_session_id` is
supplied; WebCodex never infers that Session from MCP transport identity.

## Troubleshooting

If a Plugin does not start, verify the configured profile, prepared `PATH`,
absolute `cwd`, runtime executable, and that stdout contains only protocol JSON
lines. Use stderr for local diagnostics.

If a dynamic change works through `plugin_tool` but the direct MCP tool still
uses the old behavior, that is expected: restart the Runner to create a new
startup catalog and promote the new provider instance.

If a tool is available through `plugin_tool` but not directly in MCP
`tools/list`, check for a duplicate caller-visible Plugin tool name or a
WebCodex-reserved name conflict.
