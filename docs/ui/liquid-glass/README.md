# WebCodex UI review

This is the historical glass revision. See the [current solid Web revision](../soft-surfaces/README.md) for Runtime/Admin screenshots and the current focused checks.

These screenshots show the macOS workbench revision using isolated browser fixtures, not a live Server or native Desktop backend. Run `npm --prefix scripts/ui-smoke test` to regenerate the full set at 1440, 1280, 1024, 768, and 390 px in light and dark themes, plus targeted 1900 and 2560 px captures, under `artifacts/liquid-glass-ui/`. The captured page baselines use the default blue accent; separate picker captures exercise user colors. The smoke also checks aligned rows and section edges, long paths, horizontal overflow, keyboard focus, navigation, dialogs, large surfaces against the active light or dark theme, compact desktop preferences, and reduced-transparency and reduced-motion fallbacks. See [the audit and design rules](../systematic-ui-review.md) for the rationale.

## Runtime WebUI

![Runtime Work goals at 1440 px](runtime-goals-1440.png)

![Runtime Goal plan at 2560 px](runtime-goals-2560.png)

![Runtime compact navigation in dark mode at 924 px](runtime-compact-sidebar-dark-924.png)

![Runtime Goal project menu at 390 px](runtime-390-project-picker.png)

![Runtime runner menu at 390 px](runtime-390-runner-picker.png)

![Runtime Projects at 1440 px](runtime-projects-1440.png)

![Runtime Goal project menu in dark mode at 1440 px](runtime-dark-1440-project-picker.png)

![Runtime Session in dark mode at 1440 px](runtime-session-dark-1440.png)

![Runtime Session Workflow in light mode with compact navigation at 1900 px](runtime-session-light-1900.png)

![Runtime Session Workflow in light mode at 2560 px](runtime-session-light-2560.png)

![Runtime Session Workflow in dark mode at 2560 px](runtime-session-dark-2560.png)

![Runtime Window Activity in dark mode at 2560 px](runtime-window-dark-2560.png)

![Runtime Projects at 390 px](runtime-projects-390.png)

![Runtime Add Project dialog at 390 px](runtime-add-project-dialog-390.png)

![Runtime long project path at 390 px](runtime-390-long-text.png)

![Runtime Work in dark mode at 390 px](runtime-goals-dark-390.png)

![Runtime accent palette at 390 px](runtime-390-accent-picker.png)

## Desktop

![Desktop Home at 1440 px](desktop-home-1440.png)

![Desktop Home in light mode at 2560 px](desktop-home-2560.png)

![Desktop Home in dark mode at 2560 px](desktop-dark-2560.png)

![Desktop Activity project menu at 1440 px](desktop-1440-project-picker.png)

![Desktop Activity project menu in dark mode at 1440 px](desktop-dark-1440-project-picker.png)

![Desktop Projects at 1440 px](desktop-projects-1440.png)

![Desktop Projects at 390 px](desktop-projects-390.png)

![Desktop Home in dark mode at 390 px](desktop-dark-390.png)

![Desktop Home in light mode at 390 px](desktop-home-390.png)

![Desktop Connection dialog at 390 px](desktop-connection-dialog-390.png)

![Desktop permission dialog with theme-aware backdrop](desktop-permissions.png)

![Desktop long project path at 390 px](desktop-390-long-text.png)

![Desktop accent palette at 390 px](desktop-390-accent-picker.png)

## Admin Console

![Admin dashboard at 1440 px](admin-dashboard-1440.png)

![Admin dashboard in dark mode at 390 px](admin-dark-390.png)

![Admin dashboard in light mode at 390 px](admin-dashboard-390.png)

![Admin project dialog at 390 px](admin-dialog-390.png)

![Admin accent palette at 390 px](admin-390-accent-picker.png)
