# Window → Session record browser validation

Validated on 2026-11-04 with the isolated [Window Session smoke script](../../scripts/ui-smoke/window-session-record.mjs). The final production bundle passed at **1440 × 960** and **390 × 960** in Playwright Chromium.

## Scope

The fixture serves the real `frontend/dist` renderer on loopback and intercepts Runtime API requests. It contains 40 Windows, with Window A and Window B first in the list, and 160 calls associated with the second Window's Session. Primary Window detail returns 80 calls and full detail returns 160. The test deliberately opens the second Window, selects its Session filter, scrolls, opens the record, and returns.

The baseline used the committed production bundle from `7e308d9edcb6d091c2a161e7bd03d253c71ebe2b`. The after run used the current PR production build, including the Window tab layout fix. Neither run contacted a real Server or Runner, reused user credentials, or wrote to production.

## Before and after

| Observation after opening the record | Baseline, both widths | Final build, both widths |
| --- | --- | --- |
| Window workbench count | 0 | 1 |
| Window detail count | 0 | 1 |
| Exact second Window selection | Window list removed | Retained |
| Record destination | Standalone Session workspace | Embedded in the selected Window |
| Requests caused by opening the record | `workflow-session`, `workflow-session-messages`, `project-git`, full `overview` | `workflow-session`, `workflow-session-messages` |
| Extra full Overview / Project Git reads | 2 | 0 |
| Window tabs | Record leaves the Window workspace | Two or three equal tabs stay on one row |

The request comparison covers the interval from the actual click until the Session record heading is available. It excludes initial inventory and Window history requests. The final run also verifies that both the original activity panel and call-list DOM nodes remain mounted, the Session filter is retained, and there is no horizontal page overflow. The selected Window is checked by its complete key, not its label or list position.

Returning with **Back to Window** restores the browser's real scroll positions:

| Viewport | Scroll owner | Before opening | After returning |
| --- | --- | ---: | ---: |
| 1440 × 960 | Window list | 320 px | 320 px |
| 1440 × 960 | Call list | 700 px | 700 px |
| 390 × 960 | `.app-content` | 375 px | 375 px |

The test captures scroll at the actual click, including browser focus or automatic scrolling. Every measured scroll owner must return within 2 px. At both sizes it verifies that the two-tab and three-tab layouts use one row and that English tab labels occupy one text line. Final record and returned-view screenshots were visually reviewed.

The fixture imposes a 120 ms Session detail delay and 60 ms delays for Overview and Project Git. Single observed click-to-heading times were **255 / 250 ms before** and **231 / 221 ms after** for desktop / phone. These include local rendering and automation overhead. They are diagnostic observations, not a latency benchmark, production P95, or evidence of a percentage speedup. The reproducible improvement is retained Window navigation and two fewer navigation-triggered requests.

## Reproduction

From the repository root, install the locked frontend and smoke dependencies and ensure the Playwright Chromium browser is available:

```sh
npm --prefix frontend ci
npm --prefix scripts/ui-smoke ci --ignore-scripts --no-audit --no-fund
cd scripts/ui-smoke
npx playwright install chromium
cd ../..
npm --prefix frontend run build
node scripts/ui-smoke/window-session-record.mjs
```

The final command must exit successfully with two passing checks. It writes the machine-readable report and activity, record, and returned screenshots to `artifacts/window-session-record/after/`. Generated images are diagnostic outputs and are not committed.

To repeat the baseline, use a separate worktree at `7e308d9edcb6d091c2a161e7bd03d253c71ebe2b`, copy the current smoke script into that worktree's `scripts/ui-smoke/`, install its locked smoke dependencies, and use that commit's `frontend/dist` bundle. Do not replace the active working tree's production bundle. Run:

```sh
node scripts/ui-smoke/window-session-record.mjs --baseline
```

`--baseline` selects the `before/` output directory; it does not select or build a renderer. Against the old bundle, the script records both viewports and is expected to exit with the assertion that opening the record removed its Window workbench.

## Complementary regression coverage

[Window Session record tests](../../frontend/test-v2/window-session-record.test.tsx) cover exact Window ownership, switching Windows during a delayed record request, return selection and scroll restoration, and reopening the record tab. [Session interaction safety tests](../../frontend/test-v2/session-interaction-safety.test.tsx) cover client, authority, Project, Session, and enabled-state boundaries, including obsolete mutation responses. The embedded UI uses the independent Session read APIs; a Window association does not grant Session access.
