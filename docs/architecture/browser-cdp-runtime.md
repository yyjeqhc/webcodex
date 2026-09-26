# Browser/CDP runtime architecture

Status: Phase 1 implementation contract. This document describes the first-class
Browser domain and the boundaries that later Browser work must preserve. Browser
is not part of Computer Use: Computer owns OS/window/accessibility/pointer/keyboard
semantics, while Browser owns page/navigation/DOM-or-AX/CDP semantics.

## Shape of the runtime

Phase 1 intentionally exposes only two primary model-facing tools while retaining
precise native Runner operations internally:

```text
browser_observe / browser_act
        -> closed typed action enums
        -> canonical specialized governance
        -> exact Runner capability + operation
        -> Runner-owned BrowserSupervisor
        -> webcodex-browser CDP runtime
        -> Chromium-family browser
```

`browser_observe` is guaranteed read-only and has the closed actions `targets`,
`browsers`, `pages`, `snapshot`, and `screenshot`. `browser_act` has the closed
actions `launch`, `new_page`, `navigate`, `click`, `input_text`, `key`,
`close_page`, and `close_browser`. The model surface does not expose one MCP tool
per CDP primitive, and it does not accept arbitrary protocol methods, scripts,
Browser executables, command-line arguments, profile paths, debugger endpoints,
or remote CDP endpoints.

The Runner wire remains more precise than the model facade. It uses distinct
`browser_*` operations for launch, listing, snapshot/screenshot, page creation,
navigation, element effects, key input, and close operations. This separation
preserves rolling compatibility, capability admission, telemetry, and exact
failure attribution without inflating the model tool inventory.

## Authority and model surfaces

Browser uses independent scopes:

- `browser:read` for `browser_observe`;
- `browser:launch` for `browser_act(action=launch)`;
- `browser:control` for the remaining `browser_act` effects.

The outer `browser_act` ToolDefinition admits callers that hold control or launch
authority, but that union is only catalog admission. The canonical specialized
governance path resolves the exact action and exact scope before any effect can be
dispatched. Browser control uses `ToolRisk::BrowserControl`, Standard permission,
and non-idempotent effect semantics. Internal process creation is an implementation
detail of Browser launch; it does not grant shell, generic process, or Job
authority. A ReadOnly Workflow Session may call `browser_observe` and must reject
`browser_act` before dispatch.

Browser tools are model-visible but deliberately do not expand the Local Coding or
Adaptive Runtime startup-direct schemas. Local Coding does not expose them;
Adaptive Runtime discovers them and reaches them through `call_runtime_tool`; Full
Operator may expose them directly.

## Runner capabilities and lifecycle

The Runner advertises three independent registration-required capabilities:
`browser_observe`, `browser_control`, and `browser_launch`. They are absent/false on
older Runners and are never inferred by the Server from OS identity, protocol
generation, shell support, Computer capabilities, or an assumed browser install.
At startup the Runner performs deterministic local Chromium-family executable
discovery and advertises Browser capabilities only when that runtime is actually
available.

Discovery currently considers Microsoft Edge and Google Chrome on Windows, Google
Chrome and Microsoft Edge applications on macOS, and `google-chrome`,
`google-chrome-stable`, `chromium`, or `chromium-browser` on Linux. The discovered
native executable path remains Runner-private.

`BrowserSupervisor` is Runner-owned process state rather than a global singleton.
It bounds browsers, pages, request time, semantic snapshot nodes/bytes, image
bytes, input text, idle time, and absolute runtime lifetime. Every launched browser
uses a WebPi-owned temporary profile. It never attaches the user's normal
Chrome/Edge profile or logged-in session. The CDP endpoint is bound to loopback and
its port, target IDs, session IDs, websocket URL, profile path, process IDs, and
native node identities remain private to the Runner/runtime.

The owned Chromium process tree is spawned through `webcodex-process::ManagedChild`.
Manual close, idle/lifetime reaping, and Runner shutdown all use bounded process-tree
termination/reaping. Runner restart intentionally invalidates all ephemeral Browser
identities; Phase 1 does not persist or recover Browser sessions.

## Identity, stale fencing, and semantic snapshots

Model-facing `browser_id`, `page_id`, and `element_id` values are opaque,
process-local identities. They are not aliases for CDP target IDs, backend node
IDs, ports, PIDs, or filesystem paths.

A semantic snapshot is bounded and primarily derived from CDP Accessibility
semantics. It returns enough normalized role/name/value metadata to find readable
content and interactive controls without returning unbounded raw HTML, DOM, or AX
trees. Actionable snapshot nodes receive fresh opaque `element_id` values.

Element authority is fenced to Browser identity, page identity, current document
(loader) identity, and snapshot generation. Navigation, document replacement, page
replacement, a newer snapshot, or Runner restart makes older element IDs stale.
Before `click` or `input_text`, the runtime re-observes the current page document
and requires the complete fence to remain exact. A stale failure never guesses or
retargets a replacement element; recovery is a fresh
`browser_observe(action=snapshot, ...)`.

Model-authored navigation permits only absolute `http` and `https` URLs. The
runtime may use `about:blank` internally for safe page creation/startup, but callers
cannot navigate to `file:`, script/data URLs, browser-internal pages, developer-tool
pages, or other schemes. Browser therefore does not become an alternate Project
filesystem or shell authority.

## Effect certainty and recovery

Every Browser effect preserves the same three-state certainty model used by other
WebPi effectful runtimes:

- `not_started`: WebPi proved the effect did not cross its effect boundary;
- `completed`: the exact requested effect has a known terminal result;
- `outcome_unknown`: the effect may have been dispatched or partially completed,
  but a trustworthy terminal result is unavailable.

Transport loss, timeout, Runner interruption, or a later failed stage after an
earlier Browser effect was submitted must not be converted into a retry-safe
failure. WebPi never automatically repeats navigation, click, text input, key
input, page creation, close, or launch. `outcome_unknown` returns an observation
first recovery call, normally `browser_observe(action=pages)` or
`browser_observe(action=snapshot)`, so the caller reconciles current state before
choosing another effect.

## Privacy and image delivery

Durable audit/session projections are semantic metadata, not Browser recordings.
They may retain bounded opaque IDs, action name, counts, image dimensions/MIME/
byte count/digest, and execution state. They do not retain page body text, raw DOM
or AX data, screenshot Base64, CDP traffic/endpoints, executable/profile paths,
form values, caller input text, or query-bearing navigation URLs. `input_text`
audit records only presence and byte length; navigation audit records URL presence,
not the URL.

`browser_observe(action=screenshot)` returns a bounded PNG from the Runner data
plane. At the MCP boundary it reuses the shared native-image framing used by
Computer snapshots: the Base64 body is moved into an MCP image content block and
removed from structured content, which records `content_delivery=mcp_image`.
Browser does not introduce a second image pipeline or a Phase 1 MCP App.

## Phase 1 limits and dogfood

Phase 1 intentionally does not implement arbitrary JavaScript/evaluate, cookies or
storage mutation, downloads, file upload, real-profile attachment, remote CDP
attachment, durable profiles, network interception, proxy configuration,
extensions, password-manager access, credential extraction, cloud Browser
scheduling, Browser MCP Apps, or Agent-specific Browser ownership.

CI uses deterministic fake CDP/backend fixtures and must not require Chrome. The
first live validation target after review/merge is MSI on Windows using the locally
available Edge/Chrome installation; mini on macOS is the second-platform parity
check. The special development host having no Chromium-family browser is a valid
capability-unavailable state and is not a CI blocker.
