# Using WebCodex Desktop

[English](desktop-guide.md) | [简体中文](desktop-guide.zh-CN.md)

Desktop sets up or joins a WebCodex environment, shows the projects and Runners your Server credential authorizes, and manages local connections. You ask for work in ChatGPT or another AI client. Start with the [unified installation guide](unified-installation.md) and its [native acceptance status](unified-deployment-validation.md). The [existing release installation guide](desktop-install.md) covers its Desktop and Tunnel workflow in detail.

For contributor workflows—frontend/Tauri development, source runtime resolution, NSIS/DMG packaging, and native smoke tests—see [Desktop development](DESKTOP_DEVELOPMENT.md).

## Projects and Runners

Projects shows the Server-authorized project and Runner fleet, including remote Runners and their projects when your credential grants access. A remote path belongs to its Runner's computer; Desktop does not treat it as a local folder. The page also shows Git branches, active Sessions, recent activity, and whether an online Runner reports a GUI session. A viewer has no local Runner but can still inspect this authorized fleet. Multiple projects can be used concurrently; models select an exact runtime Project ID and do not require a Desktop project switch. The home page’s primary project is an internal display default.

With a local Runner, Add Project selects and registers a folder on this computer. From a viewer-only Home, Add Project opens setup to add a local Runner; that conversion needs a one-time Runner pairing code from the Server operator. The Projects page does not offer a local folder picker to a viewer. Windows drive, UNC and extended-length spellings deduplicate without changing registered IDs or canonical paths; the UI presents ordinary user paths.

Unregister is available for projects on this Desktop's local Runner and requires confirmation of the exact project. It removes the Runner registration and matching Desktop saved entries, preserving the directory, Git files and allowed roots. The Server rejects removal when active Jobs conflict. Failures or uncertain outcomes are never automatically retried; refresh and inspect the inventory first. Removing the home page’s default does not activate another project; adding a folder remains available through setup.

## First use

1. Choose **Create a WebCodex environment** to create an environment, or **Join an existing WebCodex environment** to join one. Select **Choose folder** for a local project or skip it:

   | Setup | With a local project | Without a local project |
   | --- | --- | --- |
   | Create | Local Server and Runner | Server only |
   | Join | Local Runner connected to the remote Server | Viewer with no local Runner |

2. For a join with a project, enter the Server address and the short-lived Runner pairing code supplied by its operator. For a viewer join, enter a personal Server user credential in the protected field; viewer access does not use a pairing code. Reopening setup for the same saved Server can reuse the applicable saved credential or Runner registration.
3. Apply setup and inspect Home and Projects. A project on another Runner appears in the authorized inventory; its path is not a local folder. When this computer has a local project, confirm its path before asking the AI client to use it.
4. If this Desktop owns a regular OpenAI Secure Tunnel, configure its Tunnel ID and API key in **Connection**, then explicitly start the tunnel. Selecting a connection method alone does not start it. Follow the connection instructions in ChatGPT and perform a real project read. A remote Server's operator manages that Server's external connection.

**Local tunnel readiness does not prove ChatGPT is connected.** When there is a current project, Home keeps verification pending until Desktop observes a real call against it. A viewer can inspect authorized activity without registering a local project.

For temporary use of one project, choose Quick Share and a connection provider. Quick Share has its own temporary lifecycle, with handoff information and a stop control on Home.

## Start each day on Home

Home prioritizes overall readiness, the next action, and an optional current project. It also summarizes authorized projects and recent activity. For a persistent environment, Desktop observes service status without automatically restarting a stopped service; use explicit local controls in Diagnostics when available. Start a Desktop-owned Tunnel explicitly after its configuration is ready.

- **Current project** shows its name and full path when one is selected. With a local Runner, **Add Project** opens the folder picker; on a viewer it opens setup to add a Runner and requires pairing. Remote project paths shown in the fleet are never used as local folder selections.
- **Three steps** distinguish project readiness, connection preparation, and verified use; they collapse after completion.
- **View runtime diagnostics** expands the detailed Service, Runner, project, and connection states, plus runtime controls.
- **Sidebar navigation** opens Projects, Connection, Extensions, Activity, and Settings. Home also provides direct connection and extension shortcuts.
- **Projects** shows the authorized Server fleet. When this computer has a Runner, its saved roots are local to that Runner; selecting one project does not revoke the others.

You do not need to stop the runtime or OpenAI Secure Tunnel before changing a local project. Desktop adds the selected exact project root to the local Runner policy, hot-activates it on a compatible Runner, persists the current selection, and keeps the existing Server and Tunnel. Only a legacy or incompatible Desktop-owned Runner may need its process refreshed. Do not broaden allowed directories to work around a project loading failure.

## Connections and recovery

Connection keeps **Tunnel connection settings** visible near the top. The ID and write-only API key remain editable with the regular Tunnel running or stopped. Save persists the configuration and replaces only an active Desktop-owned regular Tunnel; Server and Runner keep running. A stopped Tunnel stays stopped until explicitly started. Blank API key retains the saved key. Saved keys are never returned to the UI.

Saving and reconnecting are separate outcomes. A failed replacement reports **Configuration saved, but the tunnel needs recovery**: retry the connection rather than re-entering the saved key. Unconfirmed process cleanup retains ownership for retry instead of claiming the old process stopped. Quick Share keeps its temporary lifecycle; changed credentials apply on its next start.

A real observed project call verifies prior client use, not current host presence. No observed call or unavailable observation is **unverified**, not proof that ChatGPT is disconnected.

| Situation | Next action |
| --- | --- |
| A local Server or Runner is stopped | For a Core-managed persistent environment, use that component's explicit control in Diagnostics; otherwise use its actual service owner |
| Tunnel ID or API key is missing | Enter and save the Tunnel ID and API key in Desktop; quitting and reopening is needed only for environment-variable changes |
| Start failed with no active tunnel | Fix configuration or networking, then click Start again with the same selection |
| An existing tunnel reports an error | Stop it, then start it again; stop failures remain visible and can be retried |
| Tunnel ready but clipboard handoff failed | Use Copy Tunnel ID on Connection, or select the displayed ID and copy manually; no restart needed |
| Tunnel ready, waiting for ChatGPT | Configure the Tunnel in ChatGPT and list the selected project's top-level files (an empty directory is a valid result) |

**Settings → OpenAI Tunnel network** controls automatic, direct, and custom HTTP proxy modes. Stop a running tunnel before changing its proxy, save, then start it again.

## Instructions, Skills, and native Tool Plugins

When this computer has a local Runner and a selected local project, **Extensions** shows that project's conventional `AGENTS.md` location, global instruction file paths, configured Skill roots, and saved native Plugin IDs. A displayed `AGENTS.md` location is not a claim that the file exists; edit its contents using the project editor. Save up to 16 absolute instruction paths and 16 absolute Skill roots. Saves target the exact observed local Runner configuration and reject stale path edits, preserving unrelated configuration, comments, credentials, and existing provider settings. A viewer does not configure a remote Runner's local files through this panel.

**Add a native Tool Plugin** registers a new trusted provider by ID, display name, executable, string-array arguments, and optional absolute working directory. Existing IDs are never overwritten. Arguments are write-only and cleared after submission; use configuration profiles for credentials. The saved registration list is not a live health check. Existing registration edits/removal and advanced provider fields remain in the shown Runner configuration file.

Saving paths or registering a Plugin does not start it or silently restart the Runner. **Restart Desktop-owned Runner** explicitly applies saved configuration, interrupts its current work, and keeps Server and regular Tunnel processes. An independently managed Runner must be restarted by its actual owner.

## macOS permission guidance

The first foreground launch with missing Desktop permissions presents an explanation with **Request Accessibility**, **Request screen recording**, and a continue action. Background/login launches do not open a permission dialog over other applications. Requests require an explicit button press; denying or dismissing them does not prevent ordinary project work. Recheck from Settings after changing system permissions.

Desktop's own permission results do not prove that a separately launched Runner is authorized. For a persistent Runner, GUI operations execute in its same-user interactive helper and require an active, unlocked session; the project service can remain online without GUI availability. Grant permissions to the actual helper process shown by macOS and follow the system's restart guidance. A legacy foreground Runner still uses its own Computer backend. The UI labels unobserved Runner authorization honestly. Native GUI helper and session behavior still requires the platform checks in [Deployment validation](unified-deployment-validation.md). Windows does not display macOS permission controls.

## Activity, settings, and background operation

If the local Server exits during startup, expand the error's **Details**. Desktop shows the exit code when available and, for a recognized HTTP listener bind failure, the socket address and OS error code (for example, `127.0.0.1:54611` and `os error 10013`). These diagnostics are extracted from bounded process output; arbitrary log text is not displayed. This does not change port selection or recovery behavior.

**Activity** shows newest entries first. Search content or sources, or select **Warnings and errors only**. Filtering never deletes records.

**Settings** contains language, launch at login, Computer Use permissions on macOS, Tunnel networking, and diagnostics. You can also switch language at the bottom of the sidebar. Supported languages are 简体中文, English, 日本語, 한국어, Deutsch, and Français. The selection is remembered across restarts, and activity times follow the selected locale. System tray menus and raw backend diagnostics remain in English; the operating system controls native file-picker language.

Closing the window hides Desktop in the menu bar or system tray. Persistent Server, Runner, and Tunnel services continue independently of that window, including when Desktop quits. **Quit WebCodex** ends Desktop and processes it directly owns, such as a temporary Quick Share and legacy Desktop-owned runtime or regular Tunnel. Use the explicit Diagnostics controls for Core-managed local services; they appear only for components this computer owns. A viewer has no local Runner control, and a Server-only computer has no Runner control. A custom legacy service can be observed through its Server connection without Desktop taking over its lifecycle. Home's legacy Desktop-managed runtime stop also updates its saved startup preference; it does not stop an independent service.

Use **⌘ + 1–6** on macOS or **Ctrl + 1–6** on Windows to switch between Home, Projects, Connection, Extensions, Activity, and Settings. Navigation shortcuts also work inside inputs and language selectors; ordinary typing and text-editing shortcuts remain available. Use Tab to focus controls and Enter to activate them; diagnostic disclosure controls also support the keyboard.

Runtime controls on Home and technical diagnostics in Settings are collapsed by default. An explicit stop displays Stopped with a Start action. Activity prioritizes results; enable Show process details for routine process events. Configure Tunnel ID and credentials on Connection; API keys are never displayed.

### Runtime after unregistering a project

A persistent environment can have a Server with no local Runner, a viewer with no
local Runner, or a local Runner with no registered project. The Desktop default
project is optional. Unregistering that project (including the last project)
leaves the environment available, clears its project-specific ChatGPT observation,
and does not select a replacement. Add Project remains available through the
appropriate local Runner or viewer setup path. Reopening Desktop observes the
saved environment and its authorized Server fleet without registering saved
projects again or starting a stopped persistent service.

A complete, online Runner inventory is authoritative. Desktop reconciles stale
saved registration history only within the same Runner configuration and client
identity; offline, inaccessible, truncated, or failed observations do not prune
history. Late responses from before a Desktop operation are rejected. Local state
is written only when reconciliation changes history or a confirmed unregister's
previous write needs retrying. Unregister remains registry-only: project folders,
Git files, allowed roots, and running Server/Runner processes are preserved.
