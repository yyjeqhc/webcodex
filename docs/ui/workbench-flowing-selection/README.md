# Workbench surfaces and flowing selection

The MCP Workbench now uses the same neutral, opaque visual language as Runtime
WebUI and Desktop. Fullscreen has a compact sidebar and separate content region;
inline and narrow layouts use top navigation. Runner selection remains an explicit
environment choice. Project and Workflow Session controls appear inside project
context, and opening a project never chooses a Session or attaches a reference.
References are an expandable tray with a count, explicit add/remove actions and
the existing Host attachment/manual-copy behavior.

Repeated read failures share one diagnostic per message, retaining the affected
resource names. Loading, successful empty data, permission failure and failed
refresh with an explicitly stale snapshot remain distinct. Changing observation
scope clears the former scope's work; a denied read clears retained work and
capabilities. Missing capabilities keep submission controls disabled with a
visible explanation. The canonical-read backend failure itself is not repaired
by this presentation change.

![Workbench with unavailable data in light mode](workbench-unavailable-light.png)

![Project files and preview in dark mode](workbench-files-dark.png)

## Selection motion contract

A shared, framework-neutral Web Animations controller owns one persistent plate
per group. It moves for 260 ms, stretches at most 3% along its movement direction
and settles without a bounce. An inert, clipped inverse-ink layer keeps text and
icons stationary and legible while the plate crosses them. The original controls
retain accessible names, focus, radio/tab state and events.

Desktop Activity, Settings and Extensions use the controller, including category
grids at narrow widths. Runtime Work level, Window/Session views, Agent and
Runtime tabs, inspector, message kinds and activity-order controls use the same
controller. Work level navigation stays mounted above the changing workbench so
its plate can continue between Goals and Activity. Workbench project tabs inline
the same first-party controller and CSS; no external script or dependency is
loaded. Primary sidebars, entity pickers and ordinary buttons retain their own behavior.
The separate [Work Result App](../work-result/README.md) now reuses the same
controller, neutral surfaces and first-party brand.

Rapid changes retarget from the visible plate rectangle. Initial placement,
responsive reflow, font/language changes and theme changes snap to valid geometry.
Sticky-to-static category transitions retain a correct positioning container;
native radio fieldsets account for their legend when measuring that container.
React radio rewrites and an unused library indicator do not cancel plate motion.
Reduced-motion preferences disable movement, while forced colors use platform
selection colors and visible focus treatment.

The following crops show an actual paused intermediate frame, rather than a
second static selected button:

![Desktop selection during motion](desktop-settings-motion.png)

![Runtime message kind selection during motion](runtime-message-motion.png)

Short browser recordings from the passing fixtures are included for review:

- [Desktop category and activity motion](desktop-motion.webm)
- [Runtime tabs and segmented controls](runtime-motion.webm)
- [Workbench project navigation and context](workbench-motion.webm)

The recordings include deterministic paused intermediate frames used by the
assertions, as well as live interactions. They are synthetic browser fixtures,
with no live user data.

## Reproducing the checks

Build frontend and Desktop, then run:

```sh
npm --prefix frontend run build
npm --prefix apps/desktop run build
node frontend/scripts/build-selection-motion.mjs --check
node scripts/ui-smoke/flowing-selection.mjs
WEBCODEX_MOTION_THEME=dark node scripts/ui-smoke/flowing-selection.mjs
npm --prefix scripts/ui-smoke run test:mcp-workspaces
```

The motion harness serves the real production bundles against isolated loopback
fixtures. It checks intermediate and final frames, stretch, interruption, label
geometry, group isolation, keyboard behavior, responsive grids, reduced motion
and forced colors. Screenshots, short recordings, bundle digests and its JSON
report are generated under ignored `artifacts/flowing-selection/`.

The MCP harness mounts the shipped HTML under a synthetic parent bridge. It covers
fullscreen and inline layouts at 320/390/800/1440 px, opposing Host/system themes,
exceptional data states, project navigation, resource previews, exact Session
selection, context lifetime, uncertain submission, size notifications and teardown.
Its report and screenshots are generated under ignored `artifacts/mcp-workspaces/`.
Controller tests additionally verify grouped diagnostics, scope changes, retained
snapshot failures, denied reads and explicit reference tray behavior.

These are fixture/browser checks, not native Desktop or real ChatGPT/Codex Host
acceptance. They use no live backend, credentials, model, services or persistent
Tunnel. Windows/macOS WebViews and actual Host rollout remain unverified.
Workbench uses `ui://webcodex/workbench/v7`; deployment and a Host resource refresh
are separate operations and are not performed by this change.

## Local validation result

The combined source passed frontend and Desktop typechecks/builds, 243 Runtime
frontend tests, 740 Desktop tests plus the native-control CSS contract, 93 focused
MCP/controller tests and the generated-region builder test. The motion browser
matrix passed 23 checks in each theme across all 11 integrated Desktop/Runtime
groups. The MCP matrix passed 21 cases, including explicit reference attachment
and Session changes, four widths, both display modes and exceptional read states.
Five focused resource-registry tests, the production warning check, formatting,
Markdown links and generated-asset consistency also passed.

The motion report separately records an existing synthetic Desktop fixture
operation warning; this is not native-backend acceptance. The Desktop bundler
also retains its existing large-chunk advisory. A first reduced-motion fixture
check raced the emulated preference update; the final harness waits for the
browser's matching media query before exercising the reduced-motion control.
