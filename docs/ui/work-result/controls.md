# Work Result control corrections

[简体中文](controls.zh-CN.md)

Follow-up to the completed Work Result redesign, tracked in
[Issue #1253](https://github.com/yyjeqhc/webcodex/issues/1253). The user reported
an Appearance panel outside the embedded viewport, uneven header controls,
confusing tab focus and oversized empty-results actions.

The Appearance panel now uses viewport positioning, clamps to either edge and
repositions on resize or scroll. Escape restores focus to its summary; clicking
or focusing outside closes it. The portaled select menu still belongs to the
panel, so choosing a local preference does not dismiss its parent prematurely.
Disposal closes the panel and removes its listeners. This shared behavior also
applies to the Spreadsheet Appearance control; Workbench retains its existing
Instance options menu.

Work Result uses aligned 32px header controls, with 44px touch targets. Narrow
cards place the controls on one toolbar row when space permits. Refresh reserves
room for its loading label. Tabs retain their moving selection underline;
keyboard focus uses a separate dashed inset outline. Pointer selection does not
leave a keyboard outline.

Results actions use their content width and wrap together. Collapse appears only
when a file is expanded and returns focus to the file when invoked. An empty
following workspace hides Pin and the tracking explanation. A pinned snapshot
retains Follow changes even if the current workspace becomes clean. Snapshot,
identity and submission semantics are unchanged.

## Card polish

The density pass keeps the same system and changes how it is spent. The summary
is one divided bar instead of separate pills, so the five facts read as one row
and colour stays in the text of the cell that needs attention. Validation and
review are labelled cells (`验证` / `审查`) split by a hairline when there is
room, and their values carry the good/attention/unavailable tone. File rows put
the path on the left and the change counts right-aligned in tabular figures, so
every row shares one right edge with the section's own total. Job outcomes and
the validation value carry status tone; the message composer's Send button
is the card's one filled primary. Times render in the card's own language with a
24-hour clock, and the delivery diagnostics name their fields in words instead
of protocol keys. Code, paths and diffs read at 13px; labels and metadata stay at
12px and 11px, nothing below 11px.

## A folded card with three ways out

The card opens folded: the header, one live line (state dot, what is running, how
long), the facts as plain text, and at most four quiet links. Everything else —
tabs, panels, files, messages, diagnostics — sits behind Details (详情). Work in
progress shows one travelling highlight under the header and a pulsing dot; no
timer drives it. The server supplies host links in the state (`links`) or with
the private payload (`_meta["webcodex/links"]`), each an absolute http(s) URL:
`console` from `WEBCODEX_PUBLIC_URL` (`<public>/runtime`) and `preview` while a
gateway preview lease for the project is live; `files` is reserved until the
server can serve a project-file view. The card calls `ui/open-link` and hides any link the server did
not send, so a Host never sees a dead button. Thread review opens expanded and
hides the compact bar.

## Plain wording and folded diagnostics

The card speaks about what the reader sees, not about the operator's model. The
third tab is Messages (消息) rather than "collaboration"; the tool-call list is
Records (调用记录); message delivery states live under Message states (消息状态);
the two participants are named You and WebCodex; the composer asks to leave
WebCodex a note. Diagnostics carry transport facts and identity for operators
and stay folded unless the card is degraded: an unavailable card, repeated
refresh failures, or a tool result delivered without its structured payload
reveal them. A healthy card never shows them.

## Evidence and reproduction

These captures show shipped HTML with public synthetic data in a disposable
loopback MCP Host fixture. They do not establish native ChatGPT/Codex Host,
live-runtime or deployed-build acceptance. No stable/test service or credential
was changed.

![Narrow Appearance dropdown](controls-mobile.png)

![Results actions sized to content](controls-results.png)

```sh
node frontend/scripts/build-selection-motion.mjs --check
npm --prefix frontend run typecheck
node --test frontend/test/mcp-card-selection.test.mjs frontend/test/selection-motion-build.test.mjs src/mcp_tests/*.test.mjs
node scripts/ui-smoke/work-result-controls.mjs
node scripts/ui-smoke/mcp-workspaces.mjs
cargo test --locked -p webcodex --lib mcp::app_registry
cargo test --locked -p webcodex --lib work_result
cargo test --locked -p webcodex --lib retired_app_resources
```

The controls script writes screenshots and `report.json` beneath
`artifacts/work-result-controls/`. It covers light/dark themes, 320/390/800/1440px,
inline/fullscreen display, pointer/keyboard focus, nested dropdown selection,
resize, dismissal, empty/populated Results and coarse touch. Node tests cover
shared panel disposal and pinned-snapshot preservation; the existing MCP browser
matrix covers normal motion, reduced motion and forced colors.

Changed embedded assets advance cache identities to Work Result v36, Workbench
v9 and Spreadsheet v5. Previous identities remain retired. These cache versions
do not change the product version or deploy an updated Host resource.
