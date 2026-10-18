# Isolated WebUI, Desktop, and Admin renderer smoke

This harness renders the real production bundles against deterministic local fixtures. It never starts a WebCodex Server, Runner, Tunnel, Plugin, or native permission request. Its injected credentials and Tauri responses exist only in this loopback test server, never in product code.

From the repository root:

```sh
npm --prefix frontend ci
npm --prefix frontend run build
npm --prefix apps/desktop ci
npm --prefix apps/desktop run build
npm --prefix scripts/ui-smoke ci
# Needed where no installed macOS Google Chrome is available:
cd scripts/ui-smoke && npx playwright install chromium && cd ../..
npm --prefix scripts/ui-smoke test
```

The script checks 1440, 1280, 1024, 768, and 390-pixel viewports in light and dark themes, the current Work / Projects / Runtime WebUI, all Desktop navigation destinations, Connection profile creation and credential containment, exact project selection/addition, exact-target instruction/Skill saves, native Plugin registration, keyboard navigation, foreground permission explanation, explicit Workflow Session vs Window activity, preserved drafts, and the Admin access gate and dashboard. It verifies row and section alignment, bounded long paths, dialog focus and dismissal, and horizontal overflow. It also emulates reduced transparency and reduced motion in Chromium.

Screenshots and `report.json` are regenerated in `artifacts/liquid-glass-ui/` (ignored by Git). The report explicitly says `fixture: true` and `nativeBackend: false`. Browser errors fail the run; the browser and server close in `finally`.

For the dedicated PDF reader, run `npm --prefix scripts/ui-smoke run test:pdf`
after `node frontend/scripts/build-pdf-document.mjs`. This uses the shipped HTML
and actual PDF.js Worker with original, deterministic PDF fixtures; it needs no
WebCodex backend or frontend app build. Install Chromium with
`npm --prefix scripts/ui-smoke exec -- playwright install chromium`, or set
`WEBCODEX_SMOKE_BROWSER` to an installed Chrome/Chromium executable. It covers
800/390-pixel layouts, mixed page sizes, rotation, text-layer alignment, embedded
Chinese Type3 glyphs/selection/search, scan image pixel colors, rapid zoom and
resize, reading-position preservation, 32-page lazy loading and scroll-back,
the total canvas budget at DPR 2,
repeated delivery, version changes, parse errors, cancellation of pending transfers
on replacement/invalid selection, and transfer/Worker teardown (including removal
of Host request timers and rejection of late replies).
The Chinese sample tests an embedded font and Unicode text mapping, not every
external CMap or OpenType font. Expected pixel colors and glyph/geometry assertions
are portable test oracles; screenshots are review evidence rather than unchecked
golden images. Errors and external PDF fetches fail the run. Outputs are ignored
under `artifacts/pdf-reader/`. To check a prior HTML build without replacing the
checkout, use `node scripts/ui-smoke/pdf-document-reader.mjs --html PATH`.

For interactive Browser Use, run `npm --prefix scripts/ui-smoke run serve` and use the printed loopback URL. Routes are `/runtime/`, `/desktop/`, `/admin/`, `/desktop/?state=disconnected`, and `/desktop/?permissions`. The server expires after 30 minutes. Stop only its exact owned process when finished.

This is not a substitute for native permission, installer, real Tunnel, or Windows platform testing. Native Rust tests independently cover configuration preservation, stale-target rejection, process ownership, project inventory, and Server/Runner PID preservation on failed Tunnel replacement.

For focused Desktop layout checks after building `apps/desktop`, run
`node scripts/ui-smoke/desktop-workflow-layout.mjs`. This covers the six Settings
categories, preserved drafts, explicit-effect boundaries, readable form controls,
visible instruction/Skill paths, and long-folder containment at 1440, 1280, 1024,
768, and 390 pixels in both themes, including Chinese and English. Fixture
screenshots and `report.json` go to `artifacts/desktop-workflow/`.

For focused Runtime workflow layout checks after building `frontend`, run
`node scripts/ui-smoke/runtime-workflow.mjs`. This checks visible running calls,
Project attribution, expanded Session activity, and horizontal overflow at 1440,
1024, and 390 pixels in both themes. Fixture screenshots and the report go to
`artifacts/liquid-glass-ui/runtime-workflow/`.

For multilingual layout regression checks, build both renderers and run
`node scripts/ui-smoke/localized-layout.mjs`. It covers all seven language choices,
Desktop navigation and controls at 390/768/1024/1440 pixels, and Runtime collaboration
at 390/1024/1440 pixels. It checks message/composer separation, reading space,
fixed activity controls, and text containment. Screenshots and geometry evidence
are written to `artifacts/localized-layout/`. This uses Chromium renderer fixtures;
it does not validate native Windows/macOS WebView rendering.

## Solid Web material checks

After building `frontend` and `apps/desktop`, run:

```sh
node scripts/ui-smoke/web-soft-surfaces.mjs
node scripts/ui-smoke/sidebar-geometry.mjs
node scripts/ui-smoke/projects-runtime.mjs
node scripts/ui-smoke/agents-workbench.mjs
node scripts/ui-smoke/session-navigation.mjs
```

The solid-material check covers Runtime Work/Goals/Projects/Overview, Admin, both access gates and dialogs at 1440/1280/1024/768/390 px in both themes. It checks opaque chrome, monochrome selected navigation, neutral normal-status badges, custom-accent persistence, search, modal keyboard focus/restoration, and Desktop foundation isolation. Screenshots and `report.json` are written to ignored `artifacts/web-soft-surfaces/`. These are production bundles against isolated fixtures, not live backend evidence.

The historical all-renderer `smoke.mjs`, `runtime-workflow.mjs` and `window-workspace.mjs` still contain assumptions predating the current Window-first UI. At the solid-Web baseline they respectively fail on a removed Desktop recent-project table, the removed Search Sessions control, and oldest-first activity despite the current newest-first default. The focused checks above exercise the current UI; those historical failures are recorded rather than reported as passing.

`sidebar-geometry.mjs` covers 24 English/Chinese viewport/theme/collapse cases
for the Runtime preference strip. It verifies exactly three icon controls,
horizontal/compact geometry, Mantine language radio-menu keyboard behavior,
five presets/custom color persistence, appearance cycle, popup viewport bounds,
Escape/focus restoration and separate lock reachability. It also retains the
selected-Window outline/metadata checks. Screenshots and `report.json` are stored
under `artifacts/web-soft-surfaces/sidebar/`.

`node scripts/ui-smoke/web-page-consistency.mjs` checks the broader Web page
inventory against the built `frontend`: Window activity/collaboration/details,
Goal detail, Projects/detail/add dialog, Runtime/configuration/diagnostics, Agent
Inbox/conversations/profile/create form, both access gates, and every Admin
section and project dialog. The English light/dark matrix covers
1440/1280/1024/768/390 px plus 1920 px content measures. Representative initial
loading/failure/empty/stale fixtures run at 1440 and 390 px; a 390×600 modal check
uses ordinary wheel scrolling to verify action reachability. Assertions cover
viewport containment, shared content measures and header edges, selection rails,
modal keyboard focus/restoration, and visible request states. Screenshots and
geometry/failure evidence go to ignored `artifacts/web-page-consistency/`.
The report names its limits; these are isolated Chromium fixtures, not live
backend, native WebView, real-model, or complete protocol-error coverage.
Full Session record workflow/collaboration/inspector and trace payload details
need their separate focused checks; this matrix does not claim those surfaces.


`node scripts/ui-smoke/web-detail-consistency.mjs` complements the page matrix
with full Session workflow/collaboration, Context/Evidence keyboard tabs where
visible, stale messages with retained draft, the Session Window chooser, trace
search and retained payload display. The same six widths and both themes produce
76 captures under `artifacts/web-page-consistency/details/`. It also checks that
selected Session rows have no layout rail, retain their full outline and align
the timestamp with the title. These are synthetic fixture reads only.

`node scripts/ui-smoke/tab-alignment.mjs` exercises Runtime Overview → denied
Agent → Overview → available Agent → Overview, plus Window/Collaboration switches
with expanded and collapsed metadata. It checks stable heading/tab bounds, equal
tab tracks, canonical metadata labels/values, exact fixture-only clipboard writes,
and long-ID wrapping. English covers 2560/1920/1440/1280/1024/768/390 px in both
themes; Chinese samples 1920/390 px and sidebar collapse samples 1440 px. The
harness enables measurable classic scrollbars by removing Playwright's default
`--hide-scrollbars` argument. `--baseline` records pre-fix evidence without claiming
a passing regression. Screenshots/reports are separated under ignored
`artifacts/web-page-consistency/tabs/baseline/` and `verified/`.
For mobile Window y comparisons, the expanded switch is first centered above
fixed navigation so automatic scroll-to-click is not mistaken for reflow. Both
viewport and document positions are then checked; snapshots record document and
container scrolling. Runtime heading/switch viewport x/y checks remain strict
through every denied/available transition.

`node scripts/ui-smoke/admin-preferences.mjs` checks Admin's two shared compact
preference buttons after building `frontend`. The English access gate and unlocked
dashboard cover 1440/1280/1024/768/390 px in both themes: exact 36 px targets,
responsive axes/gaps, centered icons, all five presets, custom color and appearance
persistence across reload, the appearance cycle, keyboard focus trapping,
Escape/focus restoration, outside dismissal, popup bounds, and horizontal overflow.
Screenshots and the report go to ignored
`artifacts/web-soft-surfaces/admin-preferences/`. These are isolated Chromium
fixtures, not live backend or native WebView evidence.

## Desktop solid-surface checks

For Desktop neutral surfaces and preference controls, build `apps/desktop` and run
`node scripts/ui-smoke/desktop-soft-surfaces.mjs`. It covers all seven destinations,
all seven Settings and six Extensions categories, Activity tabs and details, Connection
and native Plugin dialogs, selected project rows, and long bilingual metadata at
1440/1280/1024/780/768/390 pixels, in both themes and English/Chinese. The 780-pixel
case represents the native minimum width; narrower cases are supplementary renderer
checks. Assertions cover shared content origin and bounded reading measure, overflow,
large blur removal, rounded monochrome navigation without rails, selected entity label contrast of at least 4.5:1, stable category
geometry and equal tab treatment, and compact preferences in expanded/compact
navigation. Real control interactions cover accent presets/custom input, invalid
input, appearance cycling, language, focus return, reload persistence, and absence
of configuration/service effects while browsing. Screenshots and detailed geometry,
check, error, and failure evidence go to ignored `artifacts/desktop-soft-surfaces/`.
These isolated Chromium fixtures do not establish native WebView or real-model
acceptance. Set `WEBCODEX_SMOKE_BROWSER` for an installed Chromium executable.

For the Desktop dialog inventory, run `node scripts/ui-smoke/desktop-dialogs.mjs`
after building Desktop. The browser fixture opens each app-owned WorkspaceDialog
variant and the native HTML permission welcome dialog through its real UI trigger,
then resizes the same mounted dialog through 1440×960, 1024×760, 780×580, and
1440×960 again. English and Chinese cases flip light/dark through the production
appearance event while open, check containment, stable bounds, focus, ordinary
wheel access to final controls, Escape dismissal, and focus restoration. A strict
invoke allowlist records all calls and rejects configuration, service, permission,
authorization, credential and clipboard final effects. OS file selection is a
browser fixture; no native picker or backend is contacted. Outputs and the exact
surface/check/command inventory are saved under ignored
`artifacts/desktop-soft-surfaces/dialogs/`. Use `DIALOG_LOCALES=en-US` and
`DIALOG_FILTER=substring` for focused reproductions. Failed triggers remain failures
in the report; loading/busy/error states and native WebView behavior are separate
acceptance work. Inline update confirmation belongs to the page-flow harness.

For Desktop pages under live window-size and theme changes, run
`node scripts/ui-smoke/desktop-resize-theme.mjs` after the Desktop production
build. The same mounted English/Chinese views traverse 1440×960, 1280×760,
1024×580, 780×580, 1180×760 and back to 1440×960. All main destinations,
Settings/Extensions categories, onboarding choices, synthetic setup completion,
operation banner and inline update confirmation are included. Geometry,
monochrome selection, tab state, drafts, light/dark overflow, preference access,
OS-theme following and absence of unplanned fixture effects are asserted.
Update-confirmation geometry, action reachability and cancellation focus are
checked separately from the underlying page. Results are saved under ignored
`artifacts/desktop-soft-surfaces/resize-theme/`. Native Windows/macOS, OS dialogs,
and asynchronous error/busy permutations require separate acceptance.
