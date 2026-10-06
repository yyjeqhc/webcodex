# Using WebCodex Desktop

[English](desktop-guide.md) | [简体中文](desktop-guide.zh-CN.md)

Desktop starts and maintains the local WebCodex runtime; you describe the work in ChatGPT or another AI client. Local use does not require choosing, adding, activating, or unregistering a Project in Desktop. For installation, Tunnel configuration, and system permissions, see the [installation and connection guide](desktop-install.md).

For contributor workflows—frontend/Tauri development, source runtime resolution, NSIS/DMG packaging, and native smoke tests—see [Desktop development](DESKTOP_DEVELOPMENT.md).

## Runtime project inventory

**Projects** shows Runtime Projects observed on the current Runner, including Git branches, active Sessions, and recent activity. **Add local folder** is an optional shortcut for explicitly authorizing and registering another folder on this computer, not a prerequisite for model-driven work. Runtime Project identity remains the authorization, routing, persistence, audit, and Session boundary, but it is not a Desktop setup resource that users need to maintain.

For local Full Runtime, ChatGPT/model-driven calls supply the concrete workspace path. `work_on_project(path)` reuses the exact registered Project when present or registers it on demand when Runner policy permits. Multiple Projects can remain active concurrently; opening one does not revoke another.

The Runner filesystem policy remains the authority boundary. A fresh Desktop local Runner uses the normal Runner policy defaults; an empty `allowed_roots` resolves to the user's home directory. Explicit Runner policy can narrow that scope. This is Runner-level authority, separate from Runtime Project identity: a Project on one drive or root does not authorize another drive or unrelated path unless that target is also inside `allowed_roots`. Project registration never expands this scope.

## Configuration and data

Open **Settings → Configuration and data** to see the current Environment root,
the actual Desktop app-data root, and saved Server, Runner, Tunnel, credential,
data and log locations. These remain separate authorities. Locations come from
saved configuration; they do not confirm a running process's effective settings.
Unknown, missing, remote and unconfigured locations have distinct states. Open
is available only for a native-confirmed local directory. Journal/system log
entries explain their existing viewing method; Desktop Activity/output is in
memory.

Refresh, path copy and JSON export are explicit actions. Exports contain private
local paths and identity metadata, so review them before sharing and choose a
private destination. The default **backup manifest** describes safe projections,
excluded contents and missing recovery materials; it contains no configuration,
credential, database, log or project files and cannot restore an Environment.
Incomplete inventories cannot be exported. Full secret backup and restore require
a separate design. See the [schema and validation report](implementation/configuration-inventory-backup-manifest.md).

## Windows startup and upgrades

**After sign-in (recommended)** uses user-session Task Scheduler tasks. This is the ordinary Desktop path and does not require an administrator account or a service password. Server, Runner and Tunnel use the signed-in user's identity. Closing Desktop does not stop these tasks; signing out ends the user-session path.

**At computer startup (advanced)** is under **Advanced deployment options**. It uses SCM and is intended for always-on or remotely managed machines that must run before sign-in. It requires administrator authorization and has different filesystem and desktop-session permissions. It is not a more reliable or more capable version of normal Desktop startup.

Upgrading a legacy Desktop-owned environment preserves its Server, Runner identity, projects and saved Tunnel credentials. New migrations default to user-session startup. An interrupted migration or an already managed environment retains its recorded manager; Desktop never silently replaces an existing SCM service with a second scheduled task. Resume the saved operation after fixing its diagnostic rather than deleting configuration or creating another connection.

## First use

1. Launch WebCodex Desktop and choose **Create main node** or **Join main node**. Ordinary creation runs separate Server and local Runner services; ordinary joining connects this computer’s Runner to an existing Server. An initial Project is optional; skipping it keeps the Runner enabled. Confirm setup before persistent services or system authorization are requested; merely opening Desktop never binds a fresh machine to a local Server.
2. Start describing work in ChatGPT. When a request identifies a workspace path, the runtime resolves or registers that Project automatically within the Runner's allowed scope. Later launches observe saved persistent services rather than silently restarting stopped components.
3. **Projects** shows Runtime Projects that have appeared. To add a local folder manually, choose **Add local folder**, select it, and confirm **Authorize and add folder**. This explicitly adds access to that folder and its subfolders through the existing local Runner; Server and Tunnel credentials are reused. Viewer-only connections cannot add local folders.
4. Configure **OpenAI Secure Tunnel** only when external ChatGPT reachability is needed. Tunnel setup controls connectivity; it does not define Project authority.
5. A real observed project call verifies prior client use, not current host presence.

For **Join main node**, enter a direct Server URL reachable from this computer and a one-time pairing code in the protected input. ChatGPT uses OpenAI Tunnel separately; a Tunnel address or the main node’s loopback URL is not the additional Runner’s endpoint. You do not need the main node’s Tunnel key or bootstrap credential. Saved connections reuse their identity. **Advanced** retains explicit Server-only and viewer-only setup; reopening setup preserves an existing environment’s role and startup ownership.

For temporary sharing of one project, expand **Advanced**, choose Quick Share and a connection provider. Quick Share keeps its own explicit project selection and temporary lifecycle.

## Add another device

On a saved local Server environment, open **Projects → Add device** (also available before any device has appeared). Opening the dialog does not issue a code. Supply a Server URL reachable from the other machine, then explicitly choose **Create invitation**. The existing Core and Server authorization decide whether issuance is allowed; the advertised URL never selects the issuance target. No listener, firewall or service settings are changed.

The ten-minute, single-use code is masked by default. **Show code** and each copy action require a click. Install WebCodex on the other machine, choose **Join main node**, and enter that URL and code. Its initial Project can be skipped. Closing the dialog, changing environment or ending the local display period clears the displayed code; closing does not revoke the Server invitation. Failed or uncertain requests are not retried automatically.

Choose **View devices and projects** to refresh the existing authorized inventory. Invitation creation, a Runner appearing online, and a real ChatGPT Project read are separate observations. Projects remain on their owning machines; identical names and paths are distinguished by Runner and Project identities.

## Start each day on Home

Home shows Server, Runner, and connection status, a short ChatGPT handoff, and shortcuts to Projects, Activity, and Extensions. Project and activity lists live on their respective pages. Healthy state avoids duplicate readiness information; problems still expose recovery actions. A local Full Runtime is healthy with no default Project; Project readiness is not a prerequisite for starting Desktop.

- **Projects** shows observed Runtime Projects and an optional **Add local folder** shortcut. Model/runtime path resolution remains the normal workflow.
- **Activity** and **Extensions** operate on the Runtime Project associated with the selected Session or observed context.
- **Connection** manages external reachability independently from Project authority.
- **Settings** exposes Runner configuration, diagnostics, and explicit operational controls.

Persistent environments use the shared setup and service lifecycle described in the [unified installation guide](unified-installation.md). Desktop also shows the Server-authorized fleet, including remote Runners and their projects; remote paths are display-only on this computer. A viewer has no local Runner. The [native acceptance record](unified-deployment-validation.md) distinguishes automated checks from real-machine installer and migration acceptance.

You do not need to stop the runtime or OpenAI Secure Tunnel when ChatGPT moves between workspace directories. Compatible paths are resolved against current Runner policy and registered lazily by the runtime.

Projects lists working folders before **Task execution devices**. Runner reads files and runs commands on each device; **Connected to Server** describes its service connection. An unavailable desktop session concerns screenshots, windows, mouse and keyboard actions. File and command tools do not depend on that session. Missing or stale observations remain unconfirmed, and a connected desktop session does not establish system permissions. The local permission shortcut opens Settings without starting services or requesting permissions.

Activity separates **Tool calls**, **Workflow Sessions**, and **Service events**. Call sources show their latest call and open the full history. **Call completed** describes execution, not ChatGPT receipt. Sessions show progress, running jobs, and outstanding items; **Open** does not imply running. Check results use readable states, without duplicate job counts or empty attention items. HTTP delivery evidence and internal identifiers remain in details.

Home and the sidebar label the local Runner as **Local task service** and count locally ready tunnels without claiming current ChatGPT presence. Project session counts describe open sessions. On platforms without native permission probing, desktop access settings explain the missing probe and point to desktop sign-in and the Projects session status; missing permission evidence does not imply unavailable desktop tools.

## Connections and recovery

Connection cards show observed Tunnel and local MCP reachability. Failures include a specific reason and a shortcut to proxy settings, Runtime, or diagnostics. The page shows the current network route; failure guidance uses the failed attempt’s proxy evidence and never changes routes automatically.

Use **Add Connection** or a connection card’s **Edit** button to enter the Tunnel ID and write-only API key. Both remain editable while the Tunnel is running or stopped. Save persists the configuration and replaces only an active Desktop-owned regular Tunnel; Server and Runner keep running. A stopped Tunnel stays stopped until explicitly started. Blank API key retains the saved key. Saved keys are never returned to the UI.

Saving and reconnecting are separate outcomes. A failed replacement reports **Configuration saved, but the tunnel needs recovery**: retry the connection rather than re-entering the saved key. Unconfirmed process cleanup retains ownership for retry instead of claiming the old process stopped. Quick Share keeps its temporary lifecycle; changed credentials apply on its next start.

A real observed project call verifies prior client use, not current host presence. No observed call or unavailable observation is **unverified**, not proof that ChatGPT is disconnected.

| Situation | Next action |
| --- | --- |
| A local Server or Runner is stopped | For a Core-managed persistent environment, explicitly start that component in Diagnostics; opening Desktop does not restart a deliberately stopped service |
| A requested workspace is outside Runner policy | Narrowly update the Runner access policy for the intended workspace, then retry the natural-language request |
| Tunnel ID or API key is missing | Enter and save the Tunnel ID and API key in Desktop; quitting and reopening is needed only for environment-variable changes |
| Start failed with no active tunnel | Fix configuration or networking, then click Start again |
| An existing tunnel reports an error | Stop it, then start it again; stop failures remain visible and can be retried |
| Tunnel ready but clipboard handoff failed | Use Copy Tunnel ID on Connection, or select the displayed ID and copy manually; no restart needed |
| Tunnel ready, waiting for ChatGPT | Configure the Tunnel in ChatGPT and ask it to work in the intended workspace |
**Settings → Network** controls automatic, direct, and custom HTTP proxy modes. Stop a running tunnel before changing its proxy, save, then start it again.

## Instructions, Skills, and native Tool Plugins

Paths appear as individual rows. Pick a file or folder, or enter its full path. Additions, edits, and removals remain drafts until **Save**; **Cancel** restores the selected category's saved paths. When a project has no Skills, use **Add Skill Folder** above the preview. Failed settings reads show a refresh prompt and retain existing path drafts.

**Extensions** groups Coding Agents, SSH resources, MCP servers, Skills, and Instructions into categories. Configure shared local Runner paths without selecting a project; the separate project picker selects the read-only catalog preview. The global instruction editor remains independent of that preview. Extensions uses category navigation. Instructions and Skills show only their own path settings. Switching categories or project previews retains drafts; saving one category does not apply unsaved changes in another. Save up to 16 absolute instruction paths and 16 absolute Skill roots. Saves target the exact observed local Runner configuration and reject stale path edits, preserving unrelated configuration, comments, credentials, and existing provider settings. A viewer does not configure a remote Runner's local files through this panel.

**Add a native Tool Plugin** registers a new trusted provider by ID, display name, executable, string-array arguments, and optional absolute working directory. Existing IDs are never overwritten. Arguments are write-only and cleared after submission; use configuration profiles for credentials. The saved registration list is not a live health check. Existing registration edits/removal and advanced provider fields remain in the shown Runner configuration file.

Saving paths or registering a Plugin does not start it or silently restart the Runner. **Restart Desktop-owned Runner** explicitly applies saved configuration, interrupts its current work, and keeps Server and regular Tunnel processes. An independently managed Runner must be restarted by its actual owner.

## macOS permission guidance

The first foreground launch with missing Desktop permissions presents an explanation with **Request Accessibility**, **Request screen recording**, and a continue action. Background/login launches do not open a permission dialog over other applications. Requests require an explicit button press; denying or dismissing them does not prevent ordinary project work. Recheck from Settings after changing system permissions.

Desktop's own permission results do not prove that a separately launched Runner is authorized. For a persistent Runner, GUI operations execute in its same-user interactive helper and require an active, unlocked session; the project service can remain online without GUI availability. Grant permissions to the actual helper process shown by macOS and follow the system's restart guidance. A legacy foreground Runner still uses its own Computer backend. The UI labels unobserved Runner authorization honestly. Native GUI helper and session behavior still requires the platform checks in [Deployment validation](unified-deployment-validation.md). Windows does not display macOS permission controls.

## Activity, settings, and background operation

If the local Server exits during startup, expand the error's **Details**. Desktop shows the exit code when available and, for a recognized HTTP listener bind failure, the socket address and OS error code (for example, `127.0.0.1:54611` and `os error 10013`). These diagnostics are extracted from bounded process output; arbitrary log text is not displayed. This does not change port selection or recovery behavior.

**Activity** shows newest entries first. Search content or sources, or select **Warnings and errors only**. Filtering never deletes records.

**Settings** groups controls into General, Files & permissions, Network, Runtime, Troubleshooting, and About & updates. Selecting a category shows its configuration directly. Switching categories preserves network and tracing drafts without saving or restarting services. General contains language, appearance, and launch at login; Files & permissions contains allowed Runner folders and macOS Computer Use permissions. You can also switch language at the bottom of the sidebar. Supported languages are 简体中文, 繁體中文, English, 日本語, 한국어, Deutsch, and Français. The selection is remembered across restarts, and activity times follow the selected locale. System tray menus follow the selected language, including background startup. Raw backend diagnostics remain in English; the operating system controls native file-picker language.

Closing the window hides Desktop in the menu bar or system tray. Persistent Server, Runner, and Tunnel services continue independently of that window, including when Desktop quits. **Quit WebCodex** ends Desktop and processes it directly owns, such as a temporary Quick Share and legacy Desktop-owned runtime or regular Tunnel. Use the explicit Diagnostics controls for Core-managed local services; they appear only for components this computer owns. A viewer has no local Runner control, and a Server-only computer has no Runner control. A custom legacy service can be observed through its Server connection without Desktop taking over its lifecycle. The legacy Desktop-managed runtime stop in Settings → Runtime also updates its saved startup preference; it does not stop an independent service.

Use **⌘ + 1–6** on macOS or **Ctrl + 1–6** on Windows to switch between Home, Projects, Activity, Connections, Extensions, and Settings. Navigation shortcuts also work inside inputs and language selectors; ordinary typing and text-editing shortcuts remain available. Use Tab to focus controls and Enter to activate them; Settings categories support Up/Down arrows and Home/End.

Runtime and Troubleshooting have dedicated Settings categories with their main actions visible. An explicit stop displays Stopped with a Start action. Activity prioritizes results; enable Show process details for routine process events. Configure Tunnel ID and credentials on Connection; API keys are never displayed.

### Projectless Runtime

A Full Runtime consists of the Server, Runner, and the Runner's observed Project inventory. A Desktop default/display Project is optional and a fresh local Desktop intentionally starts without one. A persistent environment can instead be Server-only or a viewer. Reopening Desktop observes persistent services without restarting components that were explicitly stopped.
Desktop restart resumes the saved Server/Runner identity and Connections without registering a Project. Concrete Runtime Projects appear when model-driven work resolves a workspace path. A complete online Runner inventory is authoritative for those registrations; Desktop may reconcile stale display history, but the normal Desktop UI does not mutate Project registration state.

Project directories, Git files, and Runner policy remain separate concerns: lazy Project registration creates or reuses runtime identity for an already-authorized path and does not broaden `allowed_roots`.
