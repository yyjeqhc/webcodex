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

For interactive Browser Use, run `npm --prefix scripts/ui-smoke run serve` and use the printed loopback URL. Routes are `/runtime/`, `/desktop/`, `/admin/`, `/desktop/?state=disconnected`, and `/desktop/?permissions`. The server expires after 30 minutes. Stop only its exact owned process when finished.

This is not a substitute for native permission, installer, real Tunnel, or Windows platform testing. Native Rust tests independently cover configuration preservation, stale-target rejection, process ownership, project inventory, and Server/Runner PID preservation on failed Tunnel replacement.

For focused Runtime workflow layout checks after building `frontend`, run
`node scripts/ui-smoke/runtime-workflow.mjs`. This checks visible running calls,
Project attribution, expanded Session activity, and horizontal overflow at 1440,
1024, and 390 pixels in both themes. Fixture screenshots and the report go to
`artifacts/liquid-glass-ui/runtime-workflow/`.
