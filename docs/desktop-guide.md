# Using WebCodex Desktop

[English](desktop-guide.md) | [简体中文](desktop-guide.zh-CN.md)

Desktop prepares local projects and manages connections. You ask for work in ChatGPT or another AI client. For installation, Tunnel configuration, and system permissions, see the [installation and connection guide](desktop-install.md).

For contributor workflows—frontend/Tauri development, source runtime resolution, NSIS/DMG packaging, and native smoke tests—see [Desktop development](DESKTOP_DEVELOPMENT.md).


## Runner project inventory

Projects lists this Runner’s projects, Git branches, active Sessions and recent activity. Multiple projects can be used concurrently; models select an exact runtime Project ID and do not require a Desktop project switch. The home page’s primary project is an internal display default.

Add Project retains folder selection and registration. Windows drive, UNC and extended-length spellings deduplicate without changing registered IDs or canonical paths; the UI presents ordinary user paths.

Unregister requires confirmation of the exact project. It removes the Runner registration and matching Desktop saved entries, preserving the directory, Git files and allowed roots. The Server rejects removal when active Jobs conflict. Failures or uncertain outcomes are never automatically retried; refresh and inspect the inventory first. Removing the home page’s default does not activate another project; adding a folder remains available through setup.

## First use

1. Choose **Use WebCodex on this computer** on the welcome page, the recommended personal setup.
2. Click **Choose folder** and select the actual project you want AI to use. Review the project path at the top, then apply the setup.
3. Tunnel presence checks live under **Optional: check ChatGPT tunnel configuration**. Enter the Tunnel ID and API key and click Save configuration to prefer the local file without restarting. You can also prepare the local project before configuring a Tunnel. If configuration is present, you can opt into connecting ChatGPT after setup.
4. On Home, confirm the current project path. The three steps are **Prepare your project → Connect your AI client → Start working**.
5. Click **Connection** in the sidebar, select **OpenAI Secure Tunnel**, then click **Start secure tunnel**. Selecting a connection method alone does not start a process.
6. When the tunnel is ready, follow the connection page instructions to enter the Tunnel ID in ChatGPT and perform one real project read.

**Local tunnel readiness does not prove ChatGPT is connected.** Home keeps the verification step pending until Desktop observes a real call against the current project.

If you already have a remote Server, choose the existing Server option on the welcome page, enter its address and pairing code, and select a local project. Connection shows the remote Server; its operator manages external connectivity.

For temporary use of one project, choose Quick Share and a connection provider. Quick Share has its own temporary lifecycle, with handoff information and a stop control on Home.

## Start each day on Home

Home prioritizes overall readiness, the next action, and your current project. Click **Start** when the runtime is stopped; start a secure tunnel once the local runtime and Tunnel configuration are ready.

- **Current project** shows its name and full path. In a configured local Full Runtime, **Choose another project** or **Add project** opens the folder picker and applies that exact project immediately; there is no second setup confirmation.
- **Three steps** distinguish project readiness, connection preparation, and verified use; they collapse after completion.
- **View runtime diagnostics** expands the detailed Service, Runner, project, and connection states, plus runtime controls.
- **Sidebar navigation** opens Projects, Connection, Extensions, Activity, and Settings. Home also provides direct connection and extension shortcuts.
- **Projects → Saved roots on this Runner** shows previously selected exact folders. Select another root or add a folder without configuring another MCP application. The list is scoped to the saved Runner configuration; choosing one project does not revoke the others.

You do not need to stop the runtime or OpenAI Secure Tunnel before changing projects. Desktop adds the selected exact project root to the Runner policy, hot-activates it on a compatible Runner, persists the current selection, and keeps the existing Service and Tunnel. Only a legacy or incompatible Runner may need its Desktop-owned Runner process refreshed. Do not broaden allowed directories to work around a project loading failure.

## Connections and recovery

Connection keeps **Tunnel connection settings** visible near the top. The ID and write-only API key remain editable with the regular Tunnel running or stopped. Save persists the configuration and replaces only an active Desktop-owned regular Tunnel; Server and Runner keep running. A stopped Tunnel stays stopped until explicitly started. Blank API key retains the saved key. Saved keys are never returned to the UI.

Saving and reconnecting are separate outcomes. A failed replacement reports **Configuration saved, but the tunnel needs recovery**: retry the connection rather than re-entering the saved key. Unconfirmed process cleanup retains ownership for retry instead of claiming the old process stopped. Quick Share keeps its temporary lifecycle; changed credentials apply on its next start.

A real observed project call verifies prior client use, not current host presence. No observed call or unavailable observation is **unverified**, not proof that ChatGPT is disconnected.

| Situation | Next action |
| --- | --- |
| Local runtime is not ready | Restore it on Home; reconfigure the same project if necessary |
| Tunnel ID or API key is missing | Enter and save the Tunnel ID and API key in Desktop; quitting and reopening is needed only for environment-variable changes |
| Start failed with no active tunnel | Fix configuration or networking, then click Start again with the same selection |
| An existing tunnel reports an error | Stop it, then start it again; stop failures remain visible and can be retried |
| Tunnel ready but clipboard handoff failed | Use Copy Tunnel ID on Connection, or select the displayed ID and copy manually; no restart needed |
| Tunnel ready, waiting for ChatGPT | Configure the Tunnel in ChatGPT and list the selected project's top-level files (an empty directory is a valid result) |

**Settings → OpenAI Tunnel network** controls automatic, direct, and custom HTTP proxy modes. Stop a running tunnel before changing its proxy, save, then start it again.

## Instructions, Skills, and native Tool Plugins

**Extensions** shows the selected project's conventional `AGENTS.md` location, global instruction file paths, configured Skill roots, and saved native Plugin IDs. A displayed `AGENTS.md` location is not a claim that the file exists; edit its contents using the project editor. Save up to 16 absolute instruction paths and 16 absolute Skill roots. Saves target the exact observed Runner configuration and reject stale path edits, preserving unrelated configuration, comments, credentials, and existing provider settings.

**Add a native Tool Plugin** registers a new trusted provider by ID, display name, executable, string-array arguments, and optional absolute working directory. Existing IDs are never overwritten. Arguments are write-only and cleared after submission; use configuration profiles for credentials. The saved registration list is not a live health check. Existing registration edits/removal and advanced provider fields remain in the shown Runner configuration file.

Saving paths or registering a Plugin does not start it or silently restart the Runner. **Restart Desktop-owned Runner** explicitly applies saved configuration, interrupts its current work, and keeps Server and regular Tunnel processes. An independently managed Runner must be restarted by its actual owner.

## macOS permission guidance

The first foreground launch with missing Desktop permissions presents an explanation with **Request Accessibility**, **Request screen recording**, and a continue action. Background/login launches do not open a permission dialog over other applications. Requests require an explicit button press; denying or dismissing them does not prevent ordinary project work. Recheck from Settings after changing system permissions.

Desktop's own permission results do not prove that a separately launched Runner is authorized. Computer Use executes in the Runner: grant permissions to the actual process shown by macOS and follow the system's restart guidance. The UI labels unobserved Runner authorization honestly. Windows does not display macOS permission controls.

## Activity, settings, and background operation

If the local Server exits during startup, expand the error's **Details**. Desktop shows the exit code when available and, for a recognized HTTP listener bind failure, the socket address and OS error code (for example, `127.0.0.1:54611` and `os error 10013`). These diagnostics are extracted from bounded process output; arbitrary log text is not displayed. This does not change port selection or recovery behavior.

**Activity** shows newest entries first. Search content or sources, or select **Warnings and errors only**. Filtering never deletes records.

**Settings** contains language, launch at login, Computer Use permissions on macOS, Tunnel networking, and diagnostics. You can also switch language at the bottom of the sidebar. Supported languages are 简体中文, English, 日本語, 한국어, Deutsch, and Français. The selection is remembered across restarts, and activity times follow the selected locale. System tray menus and raw backend diagnostics remain in English; the operating system controls native file-picker language.

Closing the window hides it in the menu bar or system tray; the runtime continues in the background. **Quit WebCodex** in the tray ends Desktop and its owned processes. Stopping the Desktop-managed runtime on Home also updates the saved runtime startup preference. These controls do not terminate independently started WebCodex processes.

Use **⌘ + 1–6** on macOS or **Ctrl + 1–6** on Windows to switch between Home, Projects, Connection, Extensions, Activity, and Settings. Navigation shortcuts also work inside inputs and language selectors; ordinary typing and text-editing shortcuts remain available. Use Tab to focus controls and Enter to activate them; diagnostic disclosure controls also support the keyboard.

Runtime controls on Home and technical diagnostics in Settings are collapsed by default. An explicit stop displays Stopped with a Start action. Activity prioritizes results; enable Show process details for routine process events. Configure Tunnel ID and credentials on Connection; API keys are never displayed.

### Runtime after unregistering a project

A Full Runtime consists of the Server, Runner, and its project inventory. The
Desktop default/display project is optional. Unregistering that project (including
the last project) leaves the Runtime available, clears its project-specific ChatGPT
observation, and does not select a replacement. Add Project remains available.
Desktop restart resumes the saved Server/Runner identity and Connections without
logging in again or registering saved projects.

A complete, online Runner inventory is authoritative. Desktop reconciles stale
saved registration history only within the same Runner configuration and client
identity; offline, inaccessible, truncated, or failed observations do not prune
history. Late responses from before a Desktop operation are rejected. Local state
is written only when reconciliation changes history or a confirmed unregister's
previous write needs retrying. Unregister remains registry-only: project folders,
Git files, allowed roots, and running Server/Runner processes are preserved.
