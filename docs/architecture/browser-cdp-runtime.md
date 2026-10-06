# Browser/CDP runtime architecture

Status: Browser runtime implementation contract. This document describes the first-class
Browser domain and the boundaries that later Browser work must preserve. Browser
is not part of Computer Use: Computer owns OS/window/accessibility/pointer/keyboard
semantics, while Browser owns page/navigation/DOM-or-AX/CDP semantics.

Persistent profiles, external extension attachment, ownership-specific cleanup,
and exact Browser/Computer handoff are specified in
[Browser session continuity](browser-session-continuity.md). The CDP/identity,
request bounds and action-admission rules below apply to both transports.

## Shape of the runtime

Phase 1 intentionally exposes only two primary model-facing tools while retaining
precise native Runner operations internally:

```text
observe_browser / control_browser
        -> closed typed action enums
        -> canonical specialized governance
        -> exact Runner capability + operation
        -> Runner-owned BrowserSupervisor
        -> webcodex-browser CDP runtime
        -> Chromium-family browser
```

`observe_browser` is guaranteed read-only and has the closed actions `targets`,
`discover`, `browsers`, `pages`, `surface`, `snapshot`, `screenshot`, `console`, `network`, and `diagnostics`.
`control_browser` has the closed actions `launch`, `attach`, `new_page`, `navigate`, `reload`,
`click`, `input_text`, `select_option`, `set_value`, `upload_file`, `batch`, `key`,
`clear_diagnostics`, `close_page`, and `close_browser`. The model surface does not expose one MCP tool
per CDP primitive, and it does not accept arbitrary protocol methods, scripts,
Browser executables, command-line arguments, profile paths, debugger endpoints,
or remote CDP endpoints.

The Runner wire remains more precise than the model facade. It uses distinct
`browser_*` operations for launch, listing, snapshot/screenshot, page creation,
navigation, element effects, key input, and close operations. This separation
preserves rolling compatibility, capability admission, telemetry, and exact
failure attribution without inflating the model tool inventory.

### Bounded form batches

`control_browser(action=batch)` sends one `browser_batch` Runner invocation containing
1..32 ordered `input_text`, `select_option`, `set_value`, `click`, or `upload_file`
operations for one exact Browser/page. Each operation carries only an opaque
element identity and its action-specific value; upload additionally supplies the
authorized same-Runner Project and relative path. Server Project authorization
and Runner upload-path validation run before any batch effect. The batch payload
is capped at 256 KiB; individual field byte bounds are unchanged. Old Runners
without the additive `browser_batch` capability are rejected before dispatch.

The existing Supervisor holds its operation lock across the batch. Every element
effect uses the same document freshness, page, snapshot generation and admitted
action checks as a single effect. Every successful operation, including clicks,
is followed by document reconciliation. Ordinary effects keep sibling identities
valid; navigation, document replacement and new snapshots invalidate them. No
label-based navigation prediction or remapping exists. Successful batches settle
once and check document freshness again after settling. Dispatch has a 20-second
absolute budget checked before each operation; a running operation retains its
existing bounded backend requests. The batch transport wait is 120 seconds to
leave room for that operation's terminal reply, not to add retries or per-field
settle waits.

Any rejection, uncertain outcome or failed freshness check stops the batch.
`completed_count` records known completed operations; `stopped_at_index` is
zero-based and `stopped_execution_state` uses the existing certainty enum.
Aggregate `execution_state=not_started` means no effect started, `completed`
preserves known completion even when a later operation was rejected, and
`outcome_unknown` preserves uncertainty. `remaining_count` counts definitely
unstarted operations, including a rejected operation but excluding an uncertain
one. A post-effect freshness failure stops at the completed operation's index.
Transport loss can omit progress counts: never infer zero completion from their
absence. `needs_snapshot` and observation recovery replace retry suggestions.
Always take a fresh verification snapshot after filling, and observe after
structural/page changes. `campus-application` remains a planner and final submit
remains `ready_for_review`.

## Authority and model surfaces

Browser uses independent scopes:

- `browser:read` for `observe_browser`;
- `browser:launch` for `control_browser(action=launch)`;
- `browser:control` for the remaining `control_browser` effects.

The outer `control_browser` ToolDefinition admits callers that hold control or launch
authority, but that union is only catalog admission. The canonical specialized
governance path resolves the exact action and exact scope before any effect can be
dispatched. Browser control uses `ToolRisk::BrowserControl`, Standard permission,
and non-idempotent effect semantics. Internal process creation is an implementation
detail of Browser launch; it does not grant shell, generic process, or Job
authority. A ReadOnly Workflow Session may call `observe_browser` and must reject
`control_browser` before dispatch.

Browser tools are model-visible but deliberately do not expand the Local Coding or
Adaptive Runtime startup-direct schemas. Local Coding does not expose them;
Adaptive Runtime discovers them and reaches them through `call_runtime_tool`; Full
Operator may expose them directly.

## Runner capabilities and lifecycle

The Runner advertises independent registration-required capabilities:
`observe_browser`, `browser_control`, `browser_launch`,
`browser_element_action_admission`, and `browser_batch`. They are absent/false on
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
bytes, input text, idle time, and absolute runtime lifetime. Default launch uses a
WebCodex-owned temporary profile; managed launch uses a private persistent named
profile. Existing user Chrome tabs require explicit extension consent and native
attachment, never profile adoption. Owned CDP endpoints are loopback-only; bridge
endpoints are authenticated and lease-scoped. Ports, target/session IDs, websocket
URLs, profile paths, PIDs and native node identities remain Runner-private.

The owned Chromium process tree is spawned through `webcodex-process::ManagedChild`.
Manual close, idle/lifetime reaping, and Runner shutdown use bounded process-tree
termination/reaping only for owned instances; external attachments only detach.
Runner restart invalidates all opaque Browser identities. Managed profile data
survives, but no old Browser/page/element identity is recovered or reused.

## Identity, stale fencing, and semantic snapshots

Model-facing `browser_id`, `page_id`, and `element_id` values are opaque,
process-local identities. They are not aliases for CDP target IDs, backend node
IDs, ports, PIDs, or filesystem paths.

A semantic snapshot is bounded and derived from CDP Accessibility semantics plus
one DOM control classification. It returns enough normalized role/name/value
metadata to find readable content and interactive controls without returning
unbounded raw HTML, DOM, or AX trees. Actionable snapshot nodes receive fresh
opaque `element_id` values and an `actions` list.

`actions` is the canonical admission for that element. It is not inferred from
the accessibility role alone. The resolved element's local name and input type
select the effect: native `select` admits `select_option`; text-like inputs and
`textarea` admit `click` and `input_text`; `number`, `range`, date/time-like
inputs, and `color` admit `set_value`; `file` admits `upload_file`. Native
`option` nodes remain observable choices and admit no effect. Accessibility
`spinbutton` and `slider` nodes are not generically actionable. Descendants
inside an `input`, `select`, or `textarea` shadow tree admit nothing.

This is an intentional compatibility change from role-wide actionability. A
native `select` no longer accepts `click` or `input_text`. A file input no
longer accepts `click`. `number`, `range`, `date`, `month`, `week`, `time`,
`datetime-local`, and `color` no longer accept `click` or `input_text`. Call
only the action listed on the current snapshot node. Light-DOM buttons, links,
checkboxes, radios, and text fields keep their previous actions when the DOM
index contains them. An author button inside a custom element's shadow root
also keeps `click` when that button is in the index.

Accessibility `disabled: true` removes every effect and the element id. The
node stays in an interactive snapshot so the control remains visible.
`disabled: false` and a missing `disabled` property keep the ordinary actions.
`readonly: true` removes `input_text` and `set_value` and keeps `click` when
the control already admitted it. A missing `readonly` property removes nothing.
A promoted owner that the accessibility tree omitted copies a present HTML
`disabled` or `readonly` attribute; absence of the attribute is not treated as
either state.

If the owning control is absent from the accessibility tree, one extra snapshot
node is added for that owner. It uses the owner's role, accessible label, and
DOM value, and its element id addresses the owner. The shadow part keeps its
own role, name, and backend node, and admits no effect.

A successful DOM index that omits a node grants that node nothing. Depth
truncation therefore cannot turn a browser-private shadow picker into a click
target. When `DOM.getDocument` fails, legacy role admission remains for ordinary
controls that are not accessibility descendants of `Date`, `DateTime`,
`InputTime`, `ColorWell`, `spinbutton`, `slider`, or `combobox`. Those
descendants admit nothing, so a shadow picker does not regain `click`.
DOM classification failure preserves that legacy authority for ordinary
controls, but does not grant role-only effect authority to an ambiguous
top-level `DateTime` structured-control host. Its input type is unknown, so
the runtime cannot distinguish `month`, `week`, and `datetime-local`, which
admit `set_value` when classification succeeds. A custom `combobox` still
keeps legacy `click`. Pages containing frames require successful DOM
classification; they do not fall back to role-only authority when that read fails.
`click`, `input_text`, `select_option`, `set_value`, and `upload_file` reject an
element that does not list that action.

Same-origin iframe documents are classified separately using the same DOM and AX
rules, including the disabled and read-only admission above. Admission requires a matching HTTP(S) security origin throughout the frame
ancestry, a known frame/loader identity, and an available DOM content document.
Cross-origin, opaque-origin, sandboxed-without-allow-same-origin, missing and
out-of-process documents receive no iframe element authority. There is no remote
frame attachment, arbitrary evaluation, or new model-facing frame tool.

Snapshot collection reads at most 32 eligible child frames under one request
deadline. The existing node and byte budgets apply to the combined page projection;
frame collection stops when the aggregate node budget is reached. Frame-local AX
groups remain distinct. Before returning iframe authority, the runtime checks that
the frame topology, loaders, security origins and DOM document backend identities
still match the collection start. These identities and their fingerprint stay
Runner-private.

Before element effects and during batch reconciliation, a snapshot containing
iframe authority also revalidates that fingerprint. Frame navigation, detachment,
replacement (including a document identity change without a loader change), or
origin changes conservatively invalidate all element IDs for that page. Ordinary
form-value changes preserve sibling IDs. New snapshots retain the same generation
invalidation behavior as top-document snapshots.

Element authority is fenced to Browser identity, page identity, current document
(loader) identity, and snapshot generation. Navigation, document replacement, page
replacement, a newer snapshot, or Runner restart makes older element IDs stale.
Before any element effect (`click`, `input_text`, `select_option`, `set_value`, or
`upload_file`), the runtime re-observes the current page document and requires the
complete fence to remain exact. A stale failure never guesses or
retargets a replacement element; recovery is a fresh
`observe_browser(action=snapshot, ...)`.

Model-authored navigation permits only absolute `http` and `https` URLs. The
runtime may use `about:blank` internally for safe page creation/startup, but callers
cannot navigate to `file:`, script/data URLs, browser-internal pages, developer-tool
pages, or other schemes. Browser therefore does not become an alternate Project
filesystem or shell authority.

## Effect certainty and recovery

Every Browser effect preserves the same three-state certainty model used by other
WebCodex effectful runtimes:

- `not_started`: WebCodex proved the effect did not cross its effect boundary;
- `completed`: the exact requested effect has a known terminal result;
- `outcome_unknown`: the effect may have been dispatched or partially completed,
  but a trustworthy terminal result is unavailable.

Transport loss, timeout, Runner interruption, or a later failed stage after an
earlier Browser effect was submitted must not be converted into a retry-safe
failure. WebCodex never automatically repeats navigation, click, text input, key
input, page creation, close, or launch. `outcome_unknown` returns an observation
first recovery call, normally `observe_browser(action=pages)` or
`observe_browser(action=snapshot)`, so the caller reconciles current state before
choosing another effect.

## Privacy and image delivery

Durable audit/session projections are semantic metadata, not Browser recordings.
They may retain bounded opaque IDs, action name, counts, image dimensions/MIME/
byte count/digest, and execution state. They do not retain page body text, raw DOM
or AX data, screenshot Base64, CDP traffic/endpoints, executable/profile paths,
form values, caller input text, or query-bearing navigation URLs. `input_text`
audit records only presence and byte length; navigation audit records URL presence,
not the URL.

`observe_browser(action=screenshot)` returns a bounded PNG from the Runner data
plane. At the MCP boundary it reuses the shared native-image framing used by
Computer snapshots: the Base64 body is moved into an MCP image content block and
removed from structured content, which records `content_delivery=mcp_image`.
Browser does not introduce a second image pipeline or a Phase 1 MCP App.

## Phase 1 limits and dogfood

Phase 1 intentionally does not implement arbitrary JavaScript/evaluate, cookies or
storage mutation, downloads, real-profile attachment, remote CDP
attachment, durable profiles, network interception, proxy configuration,
extensions, password-manager access, credential extraction, cloud Browser
scheduling, Browser MCP Apps, or Agent-specific Browser ownership.

CI uses deterministic fake CDP/backend fixtures and must not require Chrome. The
first live validation target after review/merge is MSI on Windows using the locally
available Edge/Chrome installation; mini on macOS is the second-platform parity
check. The special development host having no Chromium-family browser is a valid
capability-unavailable state and is not a CI blocker.
