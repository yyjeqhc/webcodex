# Using WebCodex Desktop

[English](desktop-guide.md) | [简体中文](desktop-guide.zh-CN.md)

Desktop starts and maintains the local WebCodex runtime; you describe the work in ChatGPT or another AI client. Local use does not require choosing, adding, activating, or unregistering a Project in Desktop. For installation, Tunnel configuration, and system permissions, see the [installation and connection guide](desktop-install.md).

For contributor workflows—frontend/Tauri development, source runtime resolution, NSIS/DMG packaging, and native smoke tests—see [Desktop development](DESKTOP_DEVELOPMENT.md).

## Runtime project inventory

**Projects** is a read-only view of Runtime Projects observed on the current Runner, including Git branches, active Sessions, and recent activity. Runtime Project identity remains the authorization, routing, persistence, audit, and Session boundary, but it is not a Desktop setup resource that users need to maintain.

For local Full Runtime, ChatGPT/model-driven calls supply the concrete workspace path. `work_on_project(path)` reuses the exact registered Project when present or registers it on demand when Runner policy permits. Multiple Projects can remain active concurrently; opening one does not revoke another.

The Runner filesystem policy remains the authority boundary. A fresh Desktop local Runner uses the normal Runner policy defaults; an empty `allowed_roots` resolves to the user's home directory. Explicit Runner policy can narrow that scope. Project registration never expands it.

## First use

1. Launch WebCodex Desktop. On a fresh local install, Desktop automatically prepares the local Server and Runner with no default Project.
2. Start describing work in ChatGPT. When a request identifies a workspace path, the runtime resolves or registers that Project automatically within the Runner's allowed scope.
3. **Projects** can be used to observe which Runtime Projects have appeared; there is no Add, Activate, Reactivate, or Unregister workflow in the normal Desktop UI.
4. Configure **OpenAI Secure Tunnel** only when external ChatGPT reachability is needed. Tunnel setup controls connectivity; it does not define Project authority.
5. A real observed project call verifies prior client use, not current host presence.

If you already have a remote Server, open setup and choose the existing Server option, enter its address and pairing code, and select the local project used for that remote enrollment. The remote operator owns its Runner policy and external connectivity.

For temporary sharing of one project, choose Quick Share and a connection provider. Quick Share keeps its own explicit project selection and temporary lifecycle.

## Start each day on Home

Home prioritizes Runtime, Runner, connection, and observed ChatGPT status. A local Full Runtime is healthy with no default Project; Project readiness is not a prerequisite for starting Desktop.

- **Projects** shows observed Runtime Projects only. Project lifecycle is driven by model/runtime path resolution rather than Desktop buttons.
- **Activity** and **Extensions** operate on the Runtime Project associated with the selected Session or observed context.
- **Connection** manages external reachability independently from Project authority.
- **Settings** exposes Runner configuration, diagnostics, and explicit operational controls.

You do not need to stop the runtime or OpenAI Secure Tunnel when ChatGPT moves between workspace directories. Compatible paths are resolved against current Runner policy and registered lazily by the runtime.
## Connections and recovery

Connection keeps **Tunnel connection settings** visible near the top. The ID and write-only API key remain editable with the regular Tunnel running or stopped. Save persists the configuration and replaces only an active Desktop-owned regular Tunnel; Server and Runner keep running. A stopped Tunnel stays stopped until explicitly started. Blank API key retains the saved key. Saved keys are never returned to the UI.

Saving and reconnecting are separate outcomes. A failed replacement reports **Configuration saved, but the tunnel needs recovery**: retry the connection rather than re-entering the saved key. Unconfirmed process cleanup retains ownership for retry instead of claiming the old process stopped. Quick Share keeps its temporary lifecycle; changed credentials apply on its next start.

A real observed project call verifies prior client use, not current host presence. No observed call or unavailable observation is **unverified**, not proof that ChatGPT is disconnected.

| Situation | Next action |
| --- | --- |
| Local runtime is not ready | Restore it on Home or reopen Desktop; no Project selection is required |
| A requested workspace is outside Runner policy | Narrowly update the Runner access policy for the intended workspace, then retry the natural-language request |
| Tunnel ID or API key is missing | Enter and save the Tunnel ID and API key in Desktop; quitting and reopening is needed only for environment-variable changes |
| Start failed with no active tunnel | Fix configuration or networking, then click Start again |
| An existing tunnel reports an error | Stop it, then start it again; stop failures remain visible and can be retried |
| Tunnel ready but clipboard handoff failed | Use Copy Tunnel ID on Connection, or select the displayed ID and copy manually; no restart needed |
| Tunnel ready, waiting for ChatGPT | Configure the Tunnel in ChatGPT and ask it to work in the intended workspace |
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

### Projectless Runtime

A Full Runtime consists of the Server, Runner, and the Runner's observed Project inventory. A Desktop default/display Project is optional and a fresh local Desktop intentionally starts without one.

Desktop restart resumes the saved Server/Runner identity and Connections without registering a Project. Concrete Runtime Projects appear when model-driven work resolves a workspace path. A complete online Runner inventory is authoritative for those registrations; Desktop may reconcile stale display history, but the normal Desktop UI does not mutate Project registration state.

Project directories, Git files, and Runner policy remain separate concerns: lazy Project registration creates or reuses runtime identity for an already-authorized path and does not broaden `allowed_roots`.