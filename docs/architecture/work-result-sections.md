# Work Result read-only sections

The App registry owns template identity and MCP bindings. Within the Work Result
App, `frontend/src/mcp-apps/work-result/sections.mjs` now composes five bundled,
synchronous renderers: validation, review, Job summary, Window activity summary,
and already-loaded activity detail. Their implementations live in `checks.mjs`,
`jobs.mjs` and `activity.mjs`.

## State and authority

The mounted HTML controller still owns handshake, validated input, exact
Project/Window/Session identity, polling, deadlines, deduplication, caches,
collaboration drafts, immutable file snapshots, lazy detail/content reads, focus,
scroll anchoring and teardown. The factory receives only DOM/time formatting
adapters. Renderers accept already-validated display values and never receive a
send, refresh, retry, export, Session selection or execution function.

This is trusted first-party composition, not a JavaScript sandbox. There is no
runtime registration, dynamic import, new listener, timer, external dependency,
network request or global mutable registry. No disposable registration API is
needed for stateless, synchronous functions. The prior uncalled Session activity
and workflow-stage renderers are removed rather than retained as competing UI
paths. The Window-first activity path remains authoritative for this display.

Labels, row ordering, classes, text-only DOM construction and historical/current
evidence distinctions are unchanged. Rendering does not turn stale/unknown
checks into a pass or completed Jobs into proven success. Existing invalid-input
rejection and byte/item budgets stay with the original controller and Server.

## Build and cache identity

`npm --prefix frontend run build:work-result` builds the existing Markdown region
and a separate first-party sections region inside the same single-script HTML.
`check:dist` checks both generated regions. The sections builder rejects missing,
duplicate or reversed markers, embedded marker injection and unreviewed package
dependencies; it escapes closing script sequences and preserves every byte
outside its region. No browser-side import or additional resource fetch is added.

The shipped HTML changes, so its current URI advances from Work Result v22 to
v23. No retired-URI alias is introduced. An already-mounted v22 card keeps its
own self-contained code and unchanged backend tools; new descriptors bind v23.
This does not promise that a Host with a stale descriptor can fetch a retired URI.
Other App URIs/templates, wire schemas and persisted data remain unchanged.

## Validation and staged adoption

Source-module tests cover isolated factories, immutable inputs, current validation
truth, partial review history, Job uncertainty, zero timings and text-only DOM.
Builder tests cover deterministic replacement and malformed-region rejection.
The existing shipped-HTML DOM tests remain unchanged and cover actual rendering,
message ordering, drafts, lazy loading, focus, origin checks and teardown. Rust
App tests check new resource identity and normal capability/authority gates.

This stage works with the existing MCP presentation projection helpers and needs
no external extension host. It is not a full extraction of the HTML controller,
a claim of latency improvement, or permission to load third-party renderer code.
