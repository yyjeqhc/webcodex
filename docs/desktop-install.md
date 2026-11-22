# WebCodex Desktop: quick install and ChatGPT connection

[English](desktop-install.md) | [简体中文](desktop-install.zh-CN.md)

Desktop supports Simplified Chinese, Traditional Chinese (`zh-TW`), English, Japanese, Korean, German, and French. Change language in Settings; the selection persists and controls activity time formatting. Traditional Chinese uses dedicated catalogs. Tray menus follow the selected language, including background startup. Raw backend diagnostics remain English; file dialogs follow the operating system language.

For normal Windows or macOS **personal, same-owner use**, **WebCodex Desktop +
the official OpenAI Secure Tunnel is the recommended path**. WebCodex Desktop
runs the local Server and Runner; you send coding requests from **ChatGPT Web**.
You do not need a public WebCodex endpoint, reverse proxy, or ChatGPT OAuth
configuration. This personal Tunnel is **not** the recommended multi-user setup;
for teams, see [Deployment](DEPLOYMENT.md) and [Authentication](AUTH_MODEL.md).

This guide covers the published Desktop artifact path, including v0.4.6 Windows
x64/arm64 setup `.exe` and macOS Intel/Apple Silicon `.dmg` files. For
version-specific commands and UI, use the [v0.4.6 guide](https://github.com/yyjeqhc/webcodex/blob/v0.4.6/docs/desktop-install.md).
The current development unified NSIS/`.pkg`/`.deb` workflow is documented in
[Unified installation](unified-installation.md) and [Deployment validation](unified-deployment-validation.md);
its pending native acceptance is separate from published Desktop artifacts.

## Complete quick start

For a new **personal, same-owner** WebCodex Desktop/Tunnel setup, prepare:

- the real project directory that ChatGPT should use, preferably under version
  control;
- access to the **Plugins** page in the intended ChatGPT Web workspace; only
  Business owners/admins can create or publish custom MCP plugins;
- access to the OpenAI Platform pages for creating a Tunnel and API key.

**Already using a team-managed Business workspace plugin?** If your owner/admin
has provided an approved plugin backed by a centrally managed WebCodex Server
with appropriate per-user authorization, **skip the personal Desktop/Tunnel
steps 1–7**. Follow [Connect an approved Business workspace plugin](#connect-an-approved-business-workspace-plugin)
and then [verify an authorized project](#8-minimal-acceptance-check).
Joining a workspace does not connect to your own Desktop or create an
independent WebCodex identity.

WebCodex can read and modify files and run commands inside registered project
boundaries. Register only directories you intend to expose, review tool calls,
and keep API keys, tokens, and other secrets out of prompts, screenshots, Git,
issues, and shared logs.

1. **Install and open WebCodex Desktop.** Download the Windows installer or DMG
   matching your machine's architecture from [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases).
   On macOS, use **System Settings → Privacy & Security → Open Anyway** if the
   current non-notarized build is blocked; do not disable Gatekeeper globally.
2. On a fresh installation, click **Get started**. Desktop prepares this computer under your signed-in account, observes both services and opens Connections. Add folders later in **Projects**. Open **Settings → Runtime and services** for alternate roles, custom binaries and service controls.
3. **Create the OpenAI Tunnel credentials.** Create a Tunnel on the
   [OpenAI Tunnels page](https://platform.openai.com/settings/organization/tunnels),
   record its exact Tunnel ID, and create an API key on the
   [API keys page](https://platform.openai.com/settings/organization/api-keys).
   Prefer a Restricted key with only the Tunnel permissions required, including
   **Tunnels: Read** and **Tunnels: Use**.
4. **Save the credentials in WebCodex Desktop.** Open **Connection → Tunnel
   connection settings**, enter the Tunnel ID and API key, then select **Save
   configuration**. The API key is stored unencrypted in the current user's
   local application-data directory; do not share or commit that file.
5. **Set networking only when needed.** Leave **Settings → OpenAI Tunnel
   network** on **Automatic** unless your environment requires **Direct** or a
   **Custom HTTP proxy**. After changing this setting, stop and restart the
   Tunnel; restarting WebCodex Desktop is not required.
6. **Start the Tunnel.** Open **Connections → OpenAI Secure Tunnel** and select
   **Start**. Continue only after WebCodex Desktop reports
   **OpenAI Secure Tunnel ready; waiting for ChatGPT**.
7. **Connect your private personal Desktop Tunnel in ChatGPT Web.** Select the correct workspace and open
   the [Plugins page](https://chatgpt.com/plugins). If you have permission to
   create a plugin (for Business, this requires **Owner/Admin**), select
   **+ → Add custom MCP server**, choose **Tunnel**, and select the right Tunnel
   or **Use tunnel ID instead**. Set **Authentication** to **No Auth**, read and
   acknowledge the warning only if you trust the installation, then choose
   **Create as a plugin** (or **Create** in older interfaces), review its tools,
   and connect/install it as prompted. The current Plugins flow does **not**
   require enabling Developer Mode first; follow a legacy interface's explicit
   prompt if one appears. Business **members** should skip this personal Tunnel
   step and follow [the approved team plugin flow](#connect-an-approved-business-workspace-plugin)
   instead.
   Never paste the Tunnel API key into ChatGPT Web. If the Tunnel list looks
   stale, compare the full ID with the OpenAI Tunnels page.
8. **Verify the complete path from ChatGPT Web.** Send: “List the WebCodex
   projects, then list the top-level files in a project I am authorized to
   access; report an empty directory as empty.” This real project read—not
   local green status alone—is the final proof that ChatGPT Web, the Tunnel,
   Server, Runner, and project authority are connected.

“OpenAI Secure Tunnel ready” proves only that the local Tunnel is ready **for**
ChatGPT; WebCodex Desktop may continue to display “waiting for ChatGPT” even
after the plugin is saved. Use the real project read in step 8 as the acceptance
check. If a step fails, use the matching detailed section below; those sections
retain operational and recovery details and are not ordered as a second setup
checklist.

For everyday operation after setup, see [Using Desktop](desktop-guide.md). For
CLI, an existing remote Server, production hosting, or advanced networking, use
the [Full Setup](PERSONAL_SETUP.md) or [Deployment](DEPLOYMENT.md) guides.

Contributors who want to modify Desktop or build their own Windows/macOS package should use [Desktop development and local packaging](DESKTOP_DEVELOPMENT.md); the install guide below assumes a finished release artifact.

## 1. Install WebCodex Desktop

Download the matching Desktop artifact from the [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases) page:

- **Windows:** use the installer matching your architecture, x64 or ARM64. Windows ARM64 Desktop is part of the v0.4.2+ release build path.
- **macOS:** use the DMG matching your Mac architecture, Intel or Apple Silicon.

macOS signing has three explicit modes: `self-signed` is the current persistent fallback for public releases and long-lived dogfood; `developer-id` uses Developer ID Application plus notarization/stapling when Apple credentials are available; `adhoc` is only for disposable CI/local verification and promises no TCC upgrade continuity. Keep the same self-signed certificate **and private key** across builds and upgrades. Recreating a certificate with the same name changes identity. Both Desktop (`dev.webcodex.desktop`) and bundled Runner (`dev.webcodex.runner`) use certificate-anchored designated requirements without binary cdhash. Final app/DMG verification runs after Tauri nested signing. This does not preserve grants from historical ad-hoc builds automatically: users may need to reauthorize the new persistent identity once. Self-signed distribution is not Apple notarized; use **System Settings → Privacy & Security → Open Anyway** if Gatekeeper requires it, without disabling Gatekeeper globally.

Launch WebCodex Desktop after installation.

**Success looks like:** the main WebCodex window opens without a missing package/runtime error.

**If it fails:** use **Open Anyway** for the Gatekeeper case above; if the packaged runtime is missing, reinstall the same Desktop version instead of manually assembling internal binaries.

**Next:** if you are following the complete quick start above, choose the real
project before starting the Tunnel. The following sections provide detailed
reference for each setup area.

### Optional: background operation and Launch at Login

Closing the main window hides WebCodex Desktop; it does **not** quit the
application or stop the local runtime and Tunnel:

- On **macOS**, use the WebCodex menu-bar item to reopen the window.
- On **Windows**, use the WebCodex system-tray icon to reopen the window.
- Use **Stop Desktop-owned runtime** to stop the runtime, or **Quit WebCodex** to exit
  the application and stop Desktop-managed processes.
- Enable **Settings → Background & startup → Launch WebCodex at login** if you
  want WebCodex to start in the background when you sign in.

For navigation, keyboard shortcuts, Activity filters, and other controls used
after setup, see [Using Desktop](desktop-guide.md).

## 2. Prepare an OpenAI Tunnel

Create a Tunnel in the OpenAI Platform and prepare an API key that can use that Tunnel:

- [Tunnels - OpenAI API](https://platform.openai.com/settings/organization/tunnels)
- [API keys - OpenAI API](https://platform.openai.com/settings/organization/api-keys)

The Tunnel name is up to you. Record the Tunnel ID. A Restricted API key with
only the required **Tunnels: Read** and **Tunnels: Use** permissions is
recommended.

![OpenAI Tunnels page](desktop-install/image-20260906171606559.png)

![OpenAI API Keys page](desktop-install/image-20260906171633208.png)

Do not commit or share real API keys, WebCodex tokens, or authorization values.

## 3. Save Tunnel configuration inside Desktop (recommended)

Open **Connections** and add a ChatGPT connection. In a persistent local Environment:

1. Enter a profile name and the exact **Tunnel ID**.
2. Enter the authorized API key in the write-only password field.
3. Choose **Run with WebCodex Server (recommended)**, or choose **Separate Tunnel service (advanced)** only when an independent service lifecycle is required.
4. Save the profile. Saving never restarts a working Server. Add any remaining profiles, then use the single explicit **Restart Server** action if Desktop reports that the saved runtime state has not been applied.

A Server-owned profile is written to the current `EnvironmentStore` and is loaded only when the Server starts. It never creates a standalone Tunnel service. A separate-service profile retains per-profile Start, Stop and Restart controls. Existing owners cannot be changed in the ordinary editor; use the explicit ownership-transfer flow after the previous owner is cleanly stopped and uninstalled.

Desktop never reads an API key back into the form. Leaving the key blank while editing retains the selected profile's saved key; a replacement is accepted only against the current profile revision. Keys remain unencrypted in an owner-private binding, so keep Environment data, support bundles, tickets, screenshots and backups appropriately protected.

**Success looks like:** each card shows its independent name, Tunnel ID, owner and readiness. Server-owned profiles additionally show their per-profile startup selection. A Server-owned profile may show **Restart Server** until the one explicit restart loads all pending profiles. Saving configuration and observing a live Tunnel are separate results.

### Profile authority and legacy storage

For a persistent Environment, `EnvironmentStore` is canonical for both CLI and Desktop: `tunnel.json` stores the catalog and `server/tunnels/<profile>/webcodex.env` stores the private binding. A profile added by CLI appears on the Desktop Connections page, and a Desktop save is visible to CLI `tunnel-status`.

A historical Desktop `secrets/tunnel-config.json` is not a second writable catalog in persistent mode. It is checked only as a fail-closed identity reconciliation fence: matching profile/Tunnel identities are accepted, but a missing or different identity blocks mutation rather than silently overwriting, importing or deleting either side. Historical API-key bytes are not compared after authority transfer, because key rotation belongs only to the revision-fenced EnvironmentStore binding.

Legacy/non-persistent Desktop runtimes continue to use `secrets/tunnel-config.json` in the local application-data directory:

- macOS: `~/Library/Application Support/dev.webcodex.desktop/secrets/tunnel-config.json`.
- Windows: `%LOCALAPPDATA%\dev.webcodex.desktop\secrets\tunnel-config.json`.

macOS/Unix writes are owner-only (`0600`); Windows inherits current-user application-data permissions. Saves use guarded atomic replacement and retain no old secret backup. Invalid, unreadable, symlinked or oversized files fail closed.

### Optional: continue using environment variables in legacy mode

Without saved legacy/non-persistent configuration, Desktop can use its inherited process environment:

```text
CONTROL_PLANE_TUNNEL_ID
CONTROL_PLANE_API_KEY
```

No additional `OPENAI_ADMIN_KEY` or `OPENAI_API_KEY` is needed. Environment variables are not a persistent multi-profile format and are never merged field-by-field with saved data. OpenAI Secure Tunnel uses the built-in native Rust client, so there is no separate `tunnel-client` download or installation. If startup fails, check connection health and the control-plane route; if WebCodex reports unconfirmed prior work, resolve that state before restarting the same Tunnel identity.

Windows users can set persistent variables for the current user. On macOS, Finder / Dock launches do not read `~/.zshrc`; launch from a Terminal that has loaded the variables or configure the login session environment. After changing variables through this advanced path, use **Quit WebCodex** in the tray and launch it again. Closing the window only hides it and cannot refresh its process environment. **Recheck configuration** neither executes shell startup scripts nor reloads manually edited configuration files.

### Optional: macOS Computer Use permissions

A first foreground launch with missing Desktop permissions opens an in-app explanation. Explicit request buttons use the native macOS permission APIs; **Continue** leaves permissions unchanged. Background/login launches do not steal focus. **Settings → Computer Use permissions** shows observed Desktop permissions and lets you recheck or open System Settings. It does not infer the actual Runner's authorization from Desktop's state.

For screenshots, window observation, keyboard or pointer control, grant the relevant permissions under **System Settings → Privacy & Security** to the process actually running WebCodex Runner/Desktop. These include **Screen & System Audio Recording**, and **Accessibility** for UI control. Restart the affected process when macOS requires it.

## 4. Start the local runtime and add your project

On a fresh installation, click **Get started**. Desktop prepares this computer under your signed-in account, observes both services and opens Connections. Add folders later in **Projects**. Open **Settings → Runtime and services** for alternate roles, custom binaries and service controls. [Windows acceptance checklist](desktop-windows-acceptance.md)

## 5. Optional: configure Tunnel networking

Most users should leave **Settings → OpenAI Tunnel network** on **Automatic**
and continue to step 6. Change it only when your environment requires a proxy
or Tunnel startup reports a network error:

- **Automatic (recommended):** use the proxy inherited by Desktop; Windows can also detect the system proxy.
- **Direct:** do not use a proxy.
- **Custom HTTP proxy:** for example `http://127.0.0.1:7890`.

If the Tunnel is already running, stop it, save the new network setting, and start it again. You do **not** need to restart Desktop; each Tunnel start reads the current setting.

If the error continues, inspect the Tunnel error and **Activity** details. Do not
change project access or Runner configuration to work around a network error.

After saving any required change, continue to step 6.

## 6. Start the official OpenAI Secure Tunnel

**Start automatically** in each connection controls restoration when Desktop opens. For a Desktop-managed local Runtime, enabled connections with autostart resume after Runtime becomes ready, including when Runtime autostart is off but Runtime is already running. **Stop** disables that connection, so reopening does not restart it. Persistent environments delegate Tunnel lifecycle to system services; opening Desktop observes their actual status without restarting stopped services.

Open **Connections**, select **OpenAI Secure Tunnel**, then click **Start**. Selection alone does not start or stop processes. If an existing tunnel reports an error, stop it before starting again; failures remain visible with a retry path. When local handoff is ready, Desktop should show wording such as:

> OpenAI Secure Tunnel ready; waiting for ChatGPT

Desktop copies the Tunnel ID to the clipboard when the operating system allows it.

Desktop must **not** promote daemon readiness or a successful clipboard copy to “ChatGPT connected” or “Ready to use”. Those facts prove only that the Tunnel is **ready for ChatGPT**.

**Success looks like:** the Tunnel itself is locally ready while the overall presentation still makes clear that ChatGPT connection/final verification is pending.

**If it fails:** first confirm that both configuration-presence fields from step 3 are Detected, then check the proxy mode in step 5 and use the on-screen Tunnel recovery action.

**Next:** enter the Tunnel ID in ChatGPT Web.

## 7. Add WebCodex to ChatGPT Web

All ChatGPT setup in this section is performed in **ChatGPT Web**. "Desktop"
throughout this guide refers to **WebCodex Desktop**, not a ChatGPT desktop
application.

### ChatGPT Business (formerly Team) workspaces

ChatGPT Team was renamed **ChatGPT Business** on August 29, 2025; existing
Team workspaces need no migration. Select the intended **Business workspace**
in ChatGPT Web, not your personal workspace. A Business subscription does
**not** include OpenAI API Platform usage or automatically provide Tunnel
credentials: the Tunnel ID and API key in sections 2–3 are managed separately
through the OpenAI Platform. See OpenAI's [rename FAQ](https://help.openai.com/en/articles/12111915-chatgpt-team-is-now-chatgpt-business)
and [API billing guidance](https://help.openai.com/en/articles/9039756-billing-settings-in-chatgpt-vs-platform).

**Choose the WebCodex deployment before making a plugin available to coworkers:**

- **Personal Desktop + OpenAI Secure Tunnel (same owner):** The Tunnel forwards
  MCP requests to that owner's local Server with its `WEBCODEX_TOKEN`
  bootstrap credential, which has **admin** authority. ChatGPT's **No Auth**
  setting means ChatGPT does not supply a separate WebCodex credential; it
  does **not** mean anonymous or per-member access. Anyone allowed to use a
  published copy of this plugin would share the **same bootstrap/admin WebCodex
  identity** when reaching the **same Server, configured Runners, registered
  projects, and exposed tools**
  (still subject to Runner/project boundaries and other runtime checks).
  They do not connect to their own Desktop or get individual WebCodex scopes.
  **Do not publish a personal Desktop-backed Tunnel as the normal team setup.**
- **Team/multi-user setup:** Independently deploy a **centrally managed Server
  and its Runners**, and configure **appropriately scoped, per-user or delegated
  credentials** (for example, managed-user OAuth or PAT where supported by
  the chosen MCP connection). Choose compatible ChatGPT-side authentication;
  simply publishing a **Tunnel + No Auth** plugin cannot add per-user WebCodex
  authorization. Never distribute the Server bootstrap credential.
  Follow [Authentication](AUTH_MODEL.md) and [Deployment](DEPLOYMENT.md).

**ChatGPT Business workspace roles:**

- **Workspace owner/admin:** Only owners and admins can create and publish a
  custom MCP plugin for a Business workspace. Check your role under
  **Workspace settings → Members**. For personal use, keep the Desktop Tunnel
  private; for team use, first configure a separate, centrally managed Server
  and per-user authorization, review exposed tools (including write actions),
  then approve/publish the **team plugin** through workspace controls.
- **Workspace member:** Ask an owner/admin for the approved **team-managed**
  WebCodex plugin. Select the Business workspace, then connect/install it from
  **Plugins** using the authentication configured for your account. Members
  cannot create or publish Business workspace plugins or override admin
  restrictions. Do not request or share the owner's Tunnel API key, Server
  bootstrap token, or local credentials.

Business workspace app availability is managed at the workspace level;
Enterprise/Edu have additional role-based app access controls. ChatGPT-side
plugin availability or sharing is **not** WebCodex per-user authorization.
For creation and publication rules, see OpenAI's
[custom MCP server guide](https://help.openai.com/en/articles/12584461-connecting-custom-mcp-servers-in-chatgpt).

### Connect an approved Business workspace plugin

If your admin has published a team-managed WebCodex plugin backed by a
separately deployed Server, **do not follow the personal Desktop/Tunnel steps**:

1. Select the intended **Business workspace** in ChatGPT Web and open
   [Plugins](https://chatgpt.com/plugins).
2. Find the WebCodex plugin published/approved by your workspace admin.
   If it is missing or disabled, ask the admin to enable it; do not use
   another person's Desktop Tunnel as a workaround.
3. Select **Connect/Install** as prompted and complete the team's configured
   authentication (such as managed-user OAuth or an authorized PAT where
   supported). Never enter the Server bootstrap token or someone else's credentials.
4. Run the [minimal acceptance check](#8-minimal-acceptance-check) with a
   project your WebCodex identity is authorized to access.

This path uses the team's Server and Runners; ChatGPT plugin availability
alone does not grant WebCodex project permissions.

### Create a personal Desktop Tunnel plugin

Only follow these **Tunnel + No Auth** steps after completing personal
Desktop/Tunnel setup steps 1–6, as an authorized plugin creator. They do not
configure team/member authorization.

1. In the correct workspace, open the [ChatGPT Plugins page](https://chatgpt.com/plugins).
2. Select **+ → Add custom MCP server** (older interfaces may say
   **Create a new plugin**). The current Plugins flow does **not** require
   enabling Developer Mode first. If a legacy UI explicitly requests it,
   follow its warning and setup prompt; Developer Mode does not override
   Business workspace roles.
3. Choose **Tunnel** as the connection method.
4. Under **Available tunnels**, select the Tunnel configured for WebCodex. To
   use a specific ID, choose **Use tunnel ID instead** and enter the current
   Tunnel ID from WebCodex Desktop/OpenAI. Verify the ID if the list looks
   stale or contains a similarly named Tunnel.
5. Set **Authentication** to **No Auth / None / No authentication**.
6. Read the custom MCP server warning and select **I understand and want to
   continue** only if you trust this WebCodex installation.
7. Select **Create as a plugin** (or **Create** in older interfaces), then
   review the discovered tools.
8. If ChatGPT displays **Add … to ChatGPT**, select **Connect**.
9. **Keep this personal Desktop-backed plugin private to its owner.** Do not
   publish it as a shared Business workspace plugin. To serve coworkers,
   configure and publish a separate **team-managed** connection using the
   delegated authentication described above.

For **this personal Tunnel path only**, you do not configure OAuth in ChatGPT Web: WebCodex keeps the bootstrap MCP credential locally and the Tunnel client injects it. ChatGPT Web does not need the local credential, but this does **not** provide distinct WebCodex identities for other users. Team connections require their own appropriate authentication and permission model.

After saving the connector in ChatGPT Web, return to WebCodex Desktop. Saving a connector in ChatGPT Web does not magically upgrade local Tunnel evidence into an authoritative “connected” signal. If this WebCodex Desktop version has no stable external-MCP-client observation signal, it intentionally continues to say that it is waiting for ChatGPT.

**Success looks like:** the ChatGPT Web connector is saved and the WebCodex Desktop Tunnel remains running.

**If it fails:** verify that you selected or entered the current Tunnel ID, not the API key; Authentication should be No Auth / None / No authentication. If the Tunnel is missing or rejected, verify that it is associated with the target ChatGPT workspace and that your OpenAI Platform identity has **Tunnels: Read + Use** for that Tunnel; Business workspace creation rights and Tunnel permissions are separate. If the available Tunnel list merely looks stale, compare the exact ID with the OpenAI Tunnels page or use **Use tunnel ID instead**. Do not paste the Tunnel API key into ChatGPT Web.

**Next:** immediately perform one real project read.

![Create a Tunnel plugin in ChatGPT Web](desktop-install/chatgpt-create-tunnel-plugin.en.png)

![Connect the Tunnel plugin to ChatGPT](desktop-install/chatgpt-connect-tunnel-plugin.en.png)

![Inspect the connected plugin and its tools](desktop-install/chatgpt-tunnel-plugin-tools.en.png)

## 8. Minimal acceptance check

After connecting, start with **one minimal real project read**, for example:
“List the WebCodex projects, then list the top-level files in a project I am
authorized to access; report an empty directory as empty.” If it succeeds,
ChatGPT Web → the chosen MCP connection (personal Tunnel or team connection) →
Server → Runner → Project access works for this caller, not necessarily for
other workspace members.

The following checks are optional and exercise additional capabilities:

- list the WebCodex projects;
- read a file from the project you explicitly added;
- create, read back, and remove one temporary file inside that project;
- run a read-only command such as `git status`, `uname -a`, or `ver`;
- if you enabled Computer Use, list windows or capture a browser window.

Run only the checks you need and understand, especially those that modify files
or use Computer Use.

For a personal Desktop Tunnel, if Desktop shows healthy Service / Runner / Project / Tunnel status but ChatGPT still cannot list projects or read a file, do **not** treat local green status as proof of an external connection. Recheck the Tunnel ID and ChatGPT connection configuration, then inspect Desktop Connection / Activity for the latest state.

## Troubleshooting

**Cannot create a custom MCP plugin in a Business workspace:** check that you selected the correct Business workspace and have the Owner/Admin role. Ordinary members must ask an owner/admin to create and publish it; enabling Developer Mode cannot grant this role.

**A Business member cannot find or use WebCodex:** ask the workspace admin to check that the centrally managed team plugin is published/approved and available under **Plugins**, and that the member has the required WebCodex authorization. If it says **Disabled by admin**, contact the workspace admin. Do not substitute a personal Desktop Tunnel or share credentials to bypass workspace controls.

**A shared plugin unexpectedly exposes someone's Desktop projects:** the personal Tunnel uses the owner's bootstrap/admin identity for every forwarded request. Stop sharing that connection; review its Server/Runner/project exposure and configure a separate team Server with delegated credentials before enabling multi-user access. ChatGPT plugin access settings alone do not isolate WebCodex permissions.

**OpenAI Secure Tunnel is disabled:** check Desktop's OpenAI Tunnel configuration diagnostics, which identify a missing Tunnel ID or API key. **Recheck configuration** observes the current process only; if using environment fallback, fully quit and reopen WebCodex after changing variables.

**macOS Terminal sees the values but Desktop does not:** Finder/Dock apps do not load `~/.zshrc`; use the Terminal-launch or `launchctl setenv` path above. Desktop Recheck does not execute shell startup scripts.

**Tunnel startup or ChatGPT connection times out:** check **Settings → OpenAI Tunnel network** first, then stop and restart the Tunnel after changing the proxy mode.

**A repository is not accessible:** add that directory explicitly on the **Projects** page instead of broadening the default Desktop project's filesystem authority.

**You see `project_not_loaded` / Project not ready:** use **Activate project again**. Desktop retries the same project and manages only its own Runner; normal users do not need to edit or understand the internal project registry.

**I closed the window and reopened it, but new environment variables are still missing:** since the background-lifecycle change, closing the window hides Desktop in the tray/menu bar. Use **Quit WebCodex** there, then start a new process.

## Keep the Shell, update the Runtime

Use **Settings → Runtime** to inspect an extracted official archive or native source build, then explicitly activate it. Do not edit the app bundle. See [Runtime compatibility and diagnostics](DESKTOP_RUNTIME_COMPATIBILITY.md); unknown/missing Custom files do not silently fall back to bundled binaries.

> This detailed walkthrough documents the existing release-specific Desktop + OpenAI Tunnel path. For the unified package under development, use [Unified installation](unified-installation.md); the new package is not yet published or fully accepted across native platforms.
