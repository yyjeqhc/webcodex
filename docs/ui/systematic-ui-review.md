# WebCodex interface system

The Runtime WebUI, Desktop, and Admin Console are separate builds of the same product. Their data and operation contracts remain independent, while their semantic visual foundation comes from `frontend/src/ui/foundation.css`. Runtime and Admin now opt into the solid Web material in `frontend/src/ui/web.css`; Desktop retains its existing material. The historical audits below describe earlier revisions.

## Audit (22 September 2026)

| Area | Current evidence | Design response |
| --- | --- | --- |
| Visual source of truth | Runtime has five style files with late overrides; Desktop has a second token block appended to `app.css`; Admin has independent tokens. Similar border, blue, and radius values differ across builds. | One semantic token foundation and per-app aliases during migration; remove duplicate late overrides where practical. |
| Material hierarchy | Large rounded shells and repeated bordered cards make controls and content look equally raised. Runtime Work has nested Plan, status, Sessions, Workers, and context cards; Admin has six equal KPI cards. | Level 0 quiet canvas; Level 1 opaque data; Level 2 restrained section surfaces; Level 3 inspector/dialog; Level 4 translucent navigation and floating controls. |
| Information focus | Desktop Home begins with a generic readiness title and a large mostly empty white pane. Runtime Work repeats goal facts in center and inspector. Admin repeats an overview heading and explanatory subtitle before the operational tables. | Make current project or selected work the anchor; use compact status rows; move secondary details into inspectors and disclosures. |
| Typography and copy | 11–28px sizes appear without a shared scale. Many 12px technical details are hard to read. Headings and eyebrow labels repeat section names. | Page 26, section 16, body 14, labels/captions 12, numeric 24; reserve mono for identifiers, paths, and timestamps. Remove redundant introduction copy, retain security and recovery guidance. |
| Icons and actions | Runtime uses Lucide; Desktop has custom navigation SVGs and many text-only utility actions; Admin relies mostly on text. | Use a consistent 20px line icon language and accessible names/tooltips for icon-only actions. Keep text for consequential actions. |
| Responsive | Runtime collapses columns at narrow widths; Desktop turns all six entries into a 3×2 toolbar at 600px; Admin scrolls wide tables. Intermediate widths do not have a consistent priority rule. | 1440/1280 full workspace, 1024/768 compact navigation and reduced secondary detail, mobile task-first navigation and reachable actions. Preserve horizontal table scrolling and region labels. |
| Interaction states | Focus and reduced-motion fallbacks exist, but active/pressed/loading/empty and dark mode styling vary by build. | Shared motion and state tokens, visible keyboard focus, opaque reduced-transparency fallback, dark surfaces tuned independently. |
| Code organization | React views are mostly feature-split; Runtime `RuntimeView`/`GoalWorkbench` and long minified CSS need tighter presentation boundaries. Admin HTML and CSS are dense single-line sections. | Extract only repeated page header, toolbar, status, empty and data-display patterns; keep API/state modules unchanged. |

The product has no meaningful chart, map, or canvas surface today. Project rows, sessions, agents, jobs, and diagnostics are its core visualizations. Decorative charts would obscure real results.

## Reference study

The [user's UI collection](https://github.com/stars/Xiiiing/lists/develop-ui) currently emphasizes enterprise systems (Ant Design, Arco, Element Plus, TDesign) and design engineering resources. The reusable principle is shared semantics across components, rather than adopting their entire visual style. [TDesign's theme guidance](https://github.com/Tencent/tdesign-common/blob/develop/docs/web/theme.md) uses CSS variables for color, typography, radius, shadow, and size.

[Apple's material guidance](https://developer.apple.com/design/human-interface-guidelines/materials) puts Liquid Glass in navigation and control layers above opaque content. [Linear's interface refresh](https://linear.app/now/behind-the-latest-design-refresh) emphasizes scanning and navigation, while [Raycast's action bar](https://www.raycast.com/blog/a-fresh-look-and-feel) makes available actions and shortcuts visible. [Vercel Geist typography](https://vercel.com/geist/typography) binds size, line height, tracking, and weight into reusable roles. [Stripe Workbench](https://stripe.com/blog/workbench-a-new-way-to-debug-monitor-and-grow-your-stripe-integration) shows how operational lists and contextual inspection can coexist. [shadcn/ui](https://ui.shadcn.com/docs) favors composable source-owned components; [Radix accessibility](https://www.radix-ui.com/primitives/docs/overview/accessibility) documents focus and keyboard expectations.

The follow-up review inspected code as well as screenshots: [TDesign's layout source](https://github.com/Tencent/tdesign/blob/main/site/src/pages/design/layout.vue) for a consistent navigation width and content grid, [Ant Design's Sider](https://github.com/ant-design/ant-design/blob/master/components/layout/Sider.tsx) for collapse behavior, [Arco Design's Layout](https://github.com/arco-design/arco-design/blob/main/components/Layout/sider.tsx) for compact navigation, and [shadcn's dashboard example](https://github.com/shadcn-ui/ui/blob/main/apps/v4/app/(examples)/dashboard/page.tsx) for separating shell, header, and data regions. The transferable pattern is a quiet canvas, stable content surface, then translucent functional controls with a clear active state.

These are design constraints rather than copied assets. Mantine 9 now supplies controls and dialog behavior; WebCodex owns the navigation, page grids, rows, and visual tokens.

## System rules

- **Color:** neutral white/gray/black surfaces in both themes, with one user-selected accent for selection and primary action. Default is Blue `#2563EB`; presets are Indigo `#4F46E5`, Teal `#0F766E`, Violet `#7C3AED`, and Orange `#C2410C`. Users can choose any six-digit color. The interface derives a contrast-safe active shade separately for light and dark themes, while success, warning, error, and info keep their fixed semantic meanings. This follows [Radix's neutral gray and accent composition](https://www.radix-ui.com/colors/docs/palette-composition/composing-a-palette) and uses [Tailwind's curated color scales](https://tailwindcss.com/docs/colors) as preset seeds.
- **Geometry:** 4/7/10/14/18px radius scale; panels use 10–14px, floating controls may use pills. Thin borders and spacing carry most hierarchy.
- **Spacing:** 4px base rhythm, with 16–24px within sections and 32–48px between major sections.
- **Material:** only sidebar, top/context toolbar, command trigger, popover and floating action controls sample the backdrop. Data lists, logs, tables, and editor surfaces remain opaque.
- **Motion:** 140ms for press/hover, 220ms for panel/navigation changes, and 320ms for entry. Reduced-motion and reduced-transparency modes remove movement and blur.
- **Information order:** location and current state, primary result or work list, next action, secondary detail. Avoid equal-weight KPI card walls and repeated explanatory labels.
- **Accessibility:** native buttons/dialogs where possible; icon-only controls carry accessible labels and tooltip text; focus remains visible in both themes.

## Implementation review

The three builds now consume the same semantic foundation. Runtime keeps Work, Projects, and Runtime as its primary routes, with a collapsible desktop sidebar and a mobile navigation layer. Desktop retains its six destinations and keyboard shortcuts, grouped as Work and Configure; its current project anchors Home. Admin uses a dedicated navigation rail, compact status and data sections, and responsive tables and dialogs. Existing API endpoints, credentials, state stores, and operation handlers were kept in place.

Content, data rows, logs, and tables use stable opaque surfaces. Navigation, toolbars, and popovers use the glass material with a fine highlight and environmental shadow. Both themes tune material opacity and contrast separately. Shared page-header and icon-button primitives reduce repeated layout and label patterns; empty and secondary sections are quieter, while consequential actions remain labeled.

The second pass establishes one small terminal/window mark across Runtime, Desktop, and Admin. It strengthens the glass layer with backdrop sampling, saturation, separate optical edge and inner highlight, and a shadow above the ambient canvas. Runtime Work keeps the selected session in the central surface and places secondary evidence in an inspector. Desktop Home now anchors on the exact project path and presents Server, Runner, and Connections as a compact operational sequence next to recent work. Admin replaces equal KPI cards with a Server → Agents → Projects → Jobs flow. These are visual projections of existing data; no business contracts changed.

The monochrome pass removes the former teal tint from the canvas, glass, borders, and brand mark. A shared accent picker in each shell exposes the five preset swatches and the native custom color control; the choice persists in local storage and is recalculated when the theme changes. The chooser is a floating glass control, while work content remains opaque. The Runtime mobile preference panel, Desktop Settings, and Admin mobile rail provide reachable narrow-screen controls.

The final review found no decorative chart, gradient headline, neon border, or full-page glass layer. The visual focus is the current goal or project and operational status. The core smoke checks five widths, both themes, focus, navigation, actions, dialogs, overflow, and accessibility fallbacks. The [screenshot gallery](liquid-glass/README.md) records the fixture-based visual inspection. Native Desktop integration and live server data remain outside that isolated browser fixture; the TypeScript, unit-test, and build checks cover the code paths available here.

## Project controls and identity review

The follow-up audit used the supplied screenshots as concrete defects. Runtime's Goal project filter was an operating-system `<select>` that obscured nearby rows. Desktop Projects mixed letter avatars with the product's terminal mark and drew the selected row as a sharp-edged accent block. Runtime's progress summary also broke the otherwise shared radius rhythm. In sparse Desktop Activity and Extensions views, a lone sentence left the workspace without an actionable visual anchor.

The [TailAdmin integrations study](https://dribbble.com/shots/26162364-Integrations-workflows-settings-TailAdmin) shows a stable navigation frame and compact, recognizable integration controls; the corresponding [open-source TailAdmin shell](https://github.com/TailAdmin/free-react-tailwind-admin-dashboard/blob/main/src/layout/AppHeader.tsx) separates header actions from page content. These references informed the hierarchy and control composition; no visual asset or component implementation was copied. The [Cilo case study](https://www.behance.net/gallery/251143805/Cilo-AI-Ecommerce-Co-pilot-Case-Study) and the short Pinterest link supplied for this review could not be opened as inspectable visual media in this environment, so they were not treated as verified implementation evidence.

One source-owned picker now serves the Runtime Goal project filter, the Projects runner filter, and the Desktop Activity and Extensions project contexts. It uses a floating, backdrop-sampling menu with resource glyphs, project paths, selected state, search for larger lists, arrow-key selection, Escape, outside-click dismissal, and viewport-aware placement. The popover is rendered in a portal so the clipped Runtime list cannot cut it off. Content rows remain opaque. The same SVG brand mark is used by the Runtime shell and gate, Desktop shell and splash, and Admin rail; project and window rows use distinct line glyphs instead of letter avatars. The Runtime and Desktop project rows now share the same radius scale and restrained active border. Empty Activity and Extensions surfaces use a shared icon, one direct message, and an action only when one is available.

The light and dark screenshot pass found remaining hard-coded white backgrounds in Runtime's Goal identity table, Session collaboration surfaces, and inspector tabs. These now use semantic content surfaces; the browser smoke asserts the dark table, composer, inspector tabs, and runner control do not resolve to white.

## Three-end workbench implementation

Runtime, Desktop, and Admin now use Mantine 9 for foundation controls such as buttons, inputs, selection, alerts, and dialogs. A shared content-width, surface, and row contract in `frontend/src/ui/foundation.css` aligns project and work rows while each application retains its navigation and task-specific layout. The providers derive Mantine colors from the existing accent preference and resolved light or dark appearance. Runtime's Add Project flow uses a keyboard-accessible Mantine dialog and preserves its one-write behavior when the result is uncertain.

Admin is now a React application. Its existing refresh, mutation, and dialog coordinators remain responsible for request cancellation, session memory, idempotency keys, and confirmation rules. The build still emits only `admin.html`, `admin.js`, and `admin.css` at the Server's fixed URLs. The old imperative DOM renderer and event wiring were removed after controller tests moved to Vitest and React integration coverage was added. The isolated browser smoke checks five widths in both themes, aligned rows and sections, bounded long paths, modal focus and focus restoration, key actions, horizontal overflow, and reduced-motion and reduced-transparency preferences.

## macOS workbench and identity revision

The next review used the existing gallery as its baseline. Mantine was already present, but controls inherited different presentation rules from each application's later CSS layers. The Desktop bundle also still used a colorful robot app icon while its in-app mark and the two Web UIs used a monochrome terminal window. The revised direction is a restrained macOS workbench: one readable content plane, quiet navigation, light separators for data, and floating material only where an interaction needs it.

[Apple's material and motion guidance](https://developer.apple.com/design/human-interface-guidelines/materials) informed the separation between navigation and content and the use of brief, purposeful transitions. [Linear's interface refresh](https://linear.app/now/behind-the-latest-design-refresh) informed the lower border and icon weight; [Raycast's design refresh](https://www.raycast.com/blog/a-fresh-look-and-feel) informed consistent icon geometry and visible contextual actions. The open [Primer React](https://github.com/primer/react) and [IBM Carbon](https://github.com/carbon-design-system/carbon) repositories were studied for dense operational tables and state patterns. These are pattern references; no external asset or component implementation was copied.

The three builds now share a system-font stack, surface and spacing tokens, Mantine Styles API classes, and 120/200/280ms motion roles. Runtime and Desktop animate navigation selection with a stable rail; Admin does the same across its sections. Work and project selections update their local rows without replaying a full-page entry animation. Motion respects the user's reduced-motion setting, while CSS keeps reduced-transparency and forced-color alternatives. The source-owned terminal mark is now also the source of Desktop's generated PNG, ICO, and ICNS assets; navigation and empty states use one Lucide line-icon language.

## Component-first follow-up

The follow-up review focused on controls and information structures that were still assembled from plain buttons, tables, progress tracks, and decorative CSS. [Apple's sidebar guidance](https://developer.apple.com/design/human-interface-guidelines/sidebars), [toolbar guidance](https://developer.apple.com/design/human-interface-guidelines/toolbars), and [lists and tables guidance](https://developer.apple.com/design/human-interface-guidelines/lists-and-tables) informed the choice of a clear selection, contextual actions, and compact data rows. The three React renderers still run in a browser or Tauri WebView, so their shared UI uses accessible React components; the Desktop's native Tauri dialogs remain the platform integration where a system control is needed.

| Surface | Component-led interaction |
| --- | --- |
| Runtime Work | Mantine `SegmentedControl` for Goals/Sessions, `TextInput` and `ActionIcon` for search and exact lookup, and accessible `Progress` for plan and join state. |
| Desktop Home and Projects | Mantine `Table` for aligned wide-screen project data, `NavLink` for compact project rows and recent activity, and `Badge`, `ActionIcon`, and `Tooltip` for state and direct actions. |
| Admin | Mantine `Table.ScrollContainer` for bounded tables, `Badge` for semantic status, and a keyboard-operated `Menu` for project actions; the existing confirmation dialog and retry controller remain in charge. |

CSS in these areas now supplies layout constraints and shared tokens; the components own control semantics, keyboard behavior, focus, hover, disabled state, and responsive overflow. The UI smoke checks those component contracts as well as the resulting screenshots.

## Theme surface follow-up

The light-theme Session screenshot exposed a fixed near-black `.active-command` card inside otherwise light operational content. A computed-style sweep of the fixture's Runtime Goals, Session, Projects, Runtime Overview, Window Activity, Agents, and Admin overview found this as the only large dark content surface in light mode. Its border, icon, title, status, and footer were fixed to dark-theme colors too. The execution summary now uses semantic surface, border, text, and accent tokens in both themes; the timeline heading and run-status divider use the same contract. Desktop's regular content surfaces were already token-based, while its two native dialog backdrops now resolve from a light/dark token.

The browser smoke checks large surfaces in representative Runtime, Desktop, and Admin views against the active theme, plus the execution summary at the reported 1900px compact-sidebar width. Small brand marks, status icons, and genuinely modal dimming remain distinct from content surfaces.

## Solid Web surface revision

The supplied dashboard images informed this Web-only pass: neutral canvas, opaque rounded regions, clear whitespace and one restrained accent. Runtime and Admin retain their left navigation and existing information architecture. Navigation selection is black/white, inverted in dark mode; blue stays the default and custom accents still apply to actions, focus and progress. Routine status labels are quiet, while exceptional states and meaningful success evidence keep their semantic text/icons and color. No decorative charts, copied artwork or fabricated metrics were added.

Runtime separates its work list, execution pane and context through solid regions. Project details sit on one reading surface; Runtime summaries and fleet/configuration regions use whitespace and consistent geometry. Admin uses separate metric regions, rounded scrollable tables and opaque dialogs. Legacy light-only Runtime colors were replaced at their source, and obsolete dark overrides removed. The former glass layout file is now `styles-workbench.css`, rather than an additional layer of visual patches.

Web tokens are gated by an explicit renderer attribute and imported only by Web entrypoints, including portal surfaces. Desktop and MCP embedded renderers are unchanged. See the [solid Web gallery and validation boundary](soft-surfaces/README.md) for fixture screenshots and reproducible checks. The older [glass gallery](liquid-glass/README.md) remains historical comparison evidence.

## Sidebar alignment and selection correction

The next review used actual Chinese-language screenshots: the language control occupied a separate full-width row while three utility icons used a second row; an old animation rail appeared as a white stripe on black navigation; Window selection retained a curved blue inset edge and four competing metadata lines. These were presentational defects in the first solid-Web revision.

[Linear's current refresh](https://linear.app/now/behind-the-latest-design-refresh) emphasizes reducing the visual weight of navigation, consistent control placement and fewer decorative separators. [Carbon's shell guidance](https://www.carbondesignsystem.com/building-blocks/core/components/ui-shell-left-panel/guidelines) keeps product navigation distinct from system actions, while [Geist's menu guidance](https://vercel.com/geist/menu) groups actions with consistent leading/trailing affordances and keyboard access. Applying those principles here is a design interpretation: utility controls now share one icon/text grid with navigation, expanded rows have visible labels, and compact rails keep the same centered icon axis. No external artwork or component source was copied.

Runtime navigation and Goal selection no longer mount decorative rails. Window, Goal and Project selections use a complete subtle outline with a low-intensity accent surface. Window rows show the workspace title, current activity and one short Runner/Window context line. Complete paths and canonical identities remain in the selected Window details and their copy actions; removing redundant row text does not hide the authoritative detail. The Chinese accent label is translated. The [geometry regression](../../scripts/ui-smoke/sidebar-geometry.mjs) covers English/Chinese, both themes, five viewport widths, explicit collapse, mobile preferences, icon/label alignment, readable selection and keyboard reachability. Public comparison screenshots are in the [Web gallery](soft-surfaces/README.md).

## Full Web consistency pass

A source and isolated-browser audit of every reachable Web composition extended
the sidebar correction into nested surfaces. Agent and Conversation selection,
Session tabs, inspector tabs, composer modes and Session/Window filters now use
the same selection vocabulary. The Session record's remaining motion rail also
occupied a grid cell and displaced its timestamp; removing it restores the
trailing time column. Page headings, filters and primary regions share a 1440px
content measure rather than competing 1180/1480/1680px limits. Panel insets and
section gaps now consume Web layout tokens, including compact Admin navigation.

The audit reproduced a clipped Admin form footer on 390×600 screens, an invisible
initial Admin load error and false-empty Agent loading/failure messages. Dialog
content now scrolls normally; unlock has a visible pending state and failure
message; Agent inventory distinguishes loading/error/empty/stale. Admin anchor
selection follows scrolling, empty diagnostics have a readable explanation, and
Goal detail / Session message failures explain their state. Request cancellation,
authentication, polling, draft storage and confirmation controllers are preserved.

The full-page harness covers six widths and both themes, while a complementary
nested-detail harness covers Session workflow/collaboration, inspector keyboard
tabs, Session Window chooser, trace search and retained payload display. The
[gallery inventory](soft-surfaces/README.md) distinguishes these fixture checks
from private live-read smoke and explicitly unverified environments.


## Compact Runtime preferences

The next user revision replaces permanently labeled preference rows with three
small controls, reducing the sidebar footer to one row. Runtime now uses Mantine
[ActionIcon](https://mantine.dev/core/action-icon/),
[Popover](https://mantine.dev/core/popover/) and
[Menu](https://mantine.dev/core/menu/) components, including a library radio group
for language selection. The accent card retains the existing five presets and
custom color input; appearance still cycles through System/Light/Dark. The same
controls serve mobile settings and the access gate. Lock remains separately
available in the Runtime footer and mobile settings. Preference storage is unchanged.

Admin subsequently adopts the same source-owned accent and appearance controls
from `AppearanceControls.tsx`, replacing its labeled appearance row and native
color disclosure. Its two 36px targets align horizontally on expanded/mobile
navigation and vertically on compact rails. The shared Mantine palette has the
same opaque surface and preset/custom interactions in both Web renderers. Admin
remains English-only; Desktop retains its existing controls.

## Stable view switches and Window identity

The next screenshots exposed a measurable layout shift. With a real 17px classic
scrollbar, the long Runtime Overview centered its heading 8.5px to the left of the
short/denied Agent view. The source used automatic overflow without a reserved
gutter; both view switches also sized buttons from their unequal labels/counts.
Runtime now reserves the page gutter and the two targeted switches use equal
stable tracks. Their heading and switch coordinates are checked across repeated
Overview/denied/available Agent transitions and Window/Collaboration changes.

Window identity fields previously squeezed icon, label and a long code into one
row. They now use a two-column semantic definition list, stacking on narrow
screens, with each label above its complete value/copy action. Project address
is accurately labeled Project ID, Window/Runner labels explicitly name IDs, and
First observed describes the first-seen timestamp. No API/read/permission or
credential boundary changes accompany these presentation corrections.
