# MCP workspace entrypoints and presentation

## Outcome and acceptance

Each connection exposes one global **Projects & Resources** launcher and one
conversation **WebCodex Work Result** launcher. Opening a new chat's result panel
shows an empty state with a next step. Both Apps follow the Host theme and display
mode, remain usable at 320 px, and report changing inline content height without
resize loops. Fullscreen uses the available space with a bounded reading width.
Project, Runner and Session selection remains explicit.

## Findings and design

The reported menu contains repeated Projects & Resources, WebCodex Work Result
and WebCodex review labels. Current source registers Workbench in both `global`
and `thread`, but registers Work Result only in `thread`; `present_work_result`
does not register a menu entry. WebCodex review is a fallback title inside the
App, not another current launcher descriptor. Multiple connected historical
instances were visible to the diagnostic Host. That supports multiple registrations
as an explanation for repeated labels, but does not prove which saved connection
produced each screenshot row. One server cannot deduplicate other connections or
delete a Host's cached entries.

[OpenAI's extension documentation](https://developers.openai.com/plugins/build/extensions)
defines global launchers for the sidebar and thread launchers for conversation
panels. Workbench now owns only the global entrypoint. Work Result retains the
conversation entrypoint and its exact Window/principal binding.

The shipped HTML did not apply initial or changed `hostContext.theme` and
`displayMode`, or emit `ui/notifications/size-changed`. Browser fixtures with a dark
Host and a light system preference reproduced the theme mismatch in both Apps;
the original Apps emitted no height updates. Both now accept the known light/dark
themes and inline/fullscreen modes from the parent bridge. A body ResizeObserver
reports changed inline height, deduplicates identical dimensions, and disconnects
on teardown. Fullscreen relies on the Host viewport rather than attempting to
resize it. Measuring content instead of document scroll height allows inline
frames to shrink as well as grow.

Workbench groups optional inventory searches in a disclosure, retains the
explicit Runner/Project/Session selectors, and keeps references beside content
on wide screens and below it on narrow screens. Empty manual-copy fields stay
hidden until a reference exists. Work Result moves full Window/Session identities
into folded diagnostics, uses a two-column activity layout on wide fullscreen
views, and stacks activity outcomes on narrow screens. The old narrow-screen
summary rule preceded its base rule and lost in the cascade. Inline cards fold
call history until the user opens it; fullscreen starts with history expanded.
Repeated Host mode notifications preserve that user choice. Page-level activity,
file lists and collaboration share document scrolling. Detailed code/document
previews retain their own viewing controls.

The thread launcher formerly returned JSON-RPC `-32602` when there was no prior
successful presentation. A successful lookup with no exact binding now returns
`work_result: null`, with private thread context `{empty: true, session_id: null}`.
Only this launcher's output schema accepts null; canonical state reads remain
Project-bound. Missing identities, unavailable stores, lookup failures, denied
scopes and conflicting targets still fail closed. An empty panel makes no runtime
reads and does not create a binding, select a Project/Session, or poll another
Window. A later bound result can initialize it normally. Empty metadata cannot
erase an already mounted result.

Changed HTML uses Workbench `v3` and Work Result `v31` resource identities. Old
identities are rejected rather than silently serving different bytes. Workbench
prefers fullscreen; Work Result supports inline and fullscreen.

## Validation

Local results: 55 focused Rust tests, 239 Node controller tests and nine Chromium
scenarios passed. Production workspace/binary checking completed with zero
warnings. Rust formatting, diff whitespace and 806 tracked local Markdown links
passed. The final initialization-notification ordering change is covered by Node
and browser checks and compiled into the production check; unchanged Rust
behavior reuses the focused-suite result.

The focused Rust suites cover registry identities/metadata, unique scoped
entrypoints, resource mentions, rendering policy, canonical state reads, thread
HTTP framing and exact Window/principal binding. Empty-state tests cover protocol
and App switches, scopes, missing identities and DB failures. Existing Session,
artifact and private-result behavior stays covered.

Node controller tests cover empty/result notification ordering, private-result
fallback, malformed or conflicting thread context, Host-context message origin,
teardown, and existing snapshot/selection/collaboration lifetimes.

`npm --prefix scripts/ui-smoke run test:mcp-workspaces` runs the real bundled HTML
in Chromium with an isolated synthetic parent bridge. Nine browser cases cover
320/800/1440 px fullscreen layouts, opposing system/Host themes, theme changes,
readable controls, keyboard tabs, explicit selection, disposal, inline expansion
and contraction, and first-open empty state without runtime reads. Screenshots
and a JSON report are written to ignored `artifacts/mcp-workspaces/`.

Run the relevant Node suites with:

```sh
node --test src/mcp_tests/workbench_app.test.mjs \
  src/mcp_tests/work_result_app.test.mjs \
  src/mcp_tests/work_result_read_lifetimes_app.test.mjs \
  src/mcp_tests/workspace_host_context.test.mjs
```

An intermediate additional SQL-corruption assertion failed because
`latest_successful_window_action` already excludes events without a Project.
That assertion modeled an impossible returned binding and was removed after
checking the query contract; the disposable-store lookup-failure regression
remains. This was a test-design correction, not a runtime fix or a passing retry.

## Host rollout limits

These checks use Linux, an in-process backend and a synthetic browser Host. They
do not establish native ChatGPT/Codex acceptance or remove installed connections.
The Server must serve the new code, and the Host must refresh its tool discovery
to apply changed entrypoints and resource identities. Existing historical
connections can continue contributing duplicate menu rows until separately
disconnected. Keep the intended active connection and remove obsolete test
connections through Host settings when the account owner chooses to do so.
No live service restart, deployment or connection removal is part of this change.
