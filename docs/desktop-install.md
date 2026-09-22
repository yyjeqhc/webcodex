# WebCodex Desktop: quick install and ChatGPT connection

[English](desktop-install.md) | [简体中文](desktop-install.zh-CN.md)

For normal Windows or macOS personal use, **WebCodex Desktop + the official
OpenAI Secure Tunnel is the recommended path**. WebCodex Desktop runs the local
Server and Runner; you send coding requests from **ChatGPT Web**. You do not
need a public WebCodex endpoint, reverse proxy, or ChatGPT OAuth configuration.

## Complete quick start

Before starting, prepare:

- the real project directory that ChatGPT should use, preferably under version
  control;
- a ChatGPT Web account that shows **Developer Mode** and the **Plugins** page;
- access to the OpenAI Platform pages for creating a Tunnel and API key.

WebCodex can read and modify files and run commands inside registered project
boundaries. Register only directories you intend to expose, review tool calls,
and keep API keys, tokens, and other secrets out of prompts, screenshots, Git,
issues, and shared logs.

1. **Install and open WebCodex Desktop.** Download the Windows installer or DMG
   matching your machine's architecture from [GitHub Releases](https://github.com/yyjeqhc/webcodex/releases).
   On macOS, use **System Settings → Privacy & Security → Open Anyway** if the
   current non-notarized build is blocked; do not disable Gatekeeper globally.
2. **Choose the project.** Select **Local Full Runtime / Use WebCodex on this
   computer**, choose the actual repository directory, then wait until
   **Service**, **Runner**, and **Project** are all Ready. WebCodex Desktop is a
   runtime controller, not the chat interface.
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
6. **Start the Tunnel.** Open **Connection → OpenAI Secure Tunnel** and select
   **Start secure tunnel**. Continue only after WebCodex Desktop reports
   **OpenAI Secure Tunnel ready; waiting for ChatGPT**.
7. **Add the plugin in ChatGPT Web.** Open
   [Security and login](https://chatgpt.com/#settings/Security), enable
   **Developer Mode**, then open the [Plugins page](https://chatgpt.com/plugins).
   Create a plugin using **Tunnel**, select the correct available Tunnel or use
   **Use tunnel ID instead**, choose **No Auth**, accept the custom MCP warning
   only if you trust this installation, select **Create**, and then **Connect**.
   Never paste the Tunnel API key into ChatGPT Web. If the Tunnel list looks
   stale, compare the complete ID with the OpenAI Tunnels page.
8. **Verify the complete path from ChatGPT Web.** Send: “List the WebCodex
   projects, then list the top-level files in the project I just selected;
   report an empty directory as empty.” This real project read—not local green
   status alone—is the final proof that ChatGPT Web, the Tunnel, Server, Runner,
   and project authority are connected.

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

Current macOS builds are ad-hoc signed and are not notarized. If Gatekeeper blocks the first launch of a newly downloaded build, open **System Settings → Privacy & Security → Open Anyway**, then confirm **Open**. Do not disable Gatekeeper globally.

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

Open **Connection → Tunnel connection settings**. This editor stays visible while the regular Tunnel is running or stopped:

1. Enter your Tunnel ID in **Tunnel ID**.
2. Enter an API key authorized for that Tunnel in the **Tunnel API key** password field.
3. Click **Save configuration**. Once the source shows the local configuration file, you can start the connection. **No Desktop restart is required.**

Desktop stores the API key **unencrypted** in the current user's local
application-data directory. Keep it out of projects, Git, tickets, screenshots,
and shared backups.

**Success looks like:** the source is the local file and both presence checks
pass. Next, select the actual project ChatGPT should use.

### Optional: saved configuration behavior and storage

The same fields are available in the optional Tunnel section during local setup. When a key is already saved, leaving its field blank keeps that key. Desktop never retrieves the secret into the UI; submission clears the input. A failed save retains the Tunnel ID but requires re-entering an unsaved key.

**Priority: complete saved configuration → inherited Desktop process environment.** Desktop never combines a saved Tunnel ID with an environment API key. Saving does not modify system variables. An active Desktop-owned regular Tunnel is replaced with the saved configuration, without restarting Server or Runner. A stopped Tunnel remains stopped. OpenAI Quick Share uses the new values on its next start. Save success and connection recovery are distinct: a replacement failure retains the new configuration and asks you to retry the connection.

The file is `secrets/tunnel-config.json` in Desktop's local application data directory:

- macOS: `~/Library/Application Support/dev.webcodex.desktop/secrets/tunnel-config.json`.
- Windows: `%LOCALAPPDATA%\dev.webcodex.desktop\secrets\tunnel-config.json`.

macOS/Unix writes are owner-only (`0600`); Windows inherits access permissions from the current user's local application data directory. Saves use atomic replacement without retaining old secret backups. The `secrets` directory is excluded by WebCodex’s existing sensitive-path policy. Ordinary `desktop-state.json` remains non-secret runtime state.

**Clear saved configuration and use environment** clears the saved pair and restores environment fallback; the file records `null`. Invalid or unreadable saved configuration does not fall back automatically. Repair it by saving again in the UI or explicitly clear it. Manual file edits require restarting Desktop; in-app saves do not.

### Optional: continue using environment variables

Without saved configuration, Desktop uses its inherited process environment:

```text
CONTROL_PLANE_TUNNEL_ID
CONTROL_PLANE_API_KEY
```

No additional `OPENAI_ADMIN_KEY` or `OPENAI_API_KEY` is needed. On first OpenAI Secure Tunnel use, WebCodex automatically downloads and verifies a pinned `tunnel-client`; manual installation is normally unnecessary. If download fails, check networking or proxies. Advanced users can set `WEBCODEX_TUNNEL_CLIENT_BIN`.

Windows users can set persistent variables for the current user. On macOS, Finder / Dock launches do not read `~/.zshrc`; launch from a Terminal that has loaded the variables or configure the login session environment. After changing variables through this advanced path, use **Quit WebCodex** in the tray and launch it again. Closing the window only hides it and cannot refresh its process environment. **Recheck configuration** neither executes shell startup scripts nor reloads manually edited configuration files.

### Optional: macOS Computer Use permissions

A first foreground launch with missing Desktop permissions opens an in-app explanation. Explicit request buttons use the native macOS permission APIs; **Continue** leaves permissions unchanged. Background/login launches do not steal focus. **Settings → Computer Use permissions** shows observed Desktop permissions and lets you recheck or open System Settings. It does not infer the actual Runner's authorization from Desktop's state.

For screenshots, window observation, keyboard or pointer control, grant the relevant permissions under **System Settings → Privacy & Security** to the process actually running WebCodex Runner/Desktop. These include **Screen & System Audio Recording**, and **Accessibility** for UI control. Restart the affected process when macOS requires it.

## 4. Start the local runtime and add your project

On first use, choose **Local Full Runtime / Use WebCodex on this computer** and
select the actual repository directory that ChatGPT should use. WebCodex grants
access only to projects you explicitly add, not unrelated directories or the
whole disk.

On Home, expand **View runtime diagnostics** and confirm:

- Service: Running / Ready;
- Runner: Connected / Ready;
- Project: Ready, with the directory you selected.

To use another repository later, open **Projects** and select **Choose another
project** or **Add project**.

**Success looks like:** Service, Runner, and Project are all Ready, and the displayed project path is exact.

**If it fails:** use **Activate project again**. If it still fails, inspect the error and
**Activity** details. Do not broaden project access to work around the error.

**Next:** start the OpenAI Secure Tunnel only after these three are ready.

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

Open **Connection**, select **OpenAI Secure Tunnel**, then click **Start secure tunnel**. Selection alone does not start or stop processes. If an existing tunnel reports an error, stop it before starting again; failures remain visible with a retry path. When local handoff is ready, Desktop should show wording such as:

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

1. Open [ChatGPT Web account security settings](https://chatgpt.com/#settings/Security).
2. Under **Account security and sign-in**, enable **Developer Mode**. Review the
   warning shown by ChatGPT before enabling it; Developer Mode permits adding
   connectors that may make permanent changes or delete data.

![Enable Developer Mode in ChatGPT Web](desktop-install/chatgpt-enable-developer-mode.en.png)

3. Open the [ChatGPT Plugins page](https://chatgpt.com/plugins).
4. Create a new plugin, then choose **Tunnel** as the connection method.
5. Under **Available tunnels**, select the Tunnel configured for WebCodex. To
   use a specific ID, choose **Use tunnel ID instead** and enter the current
   Tunnel ID from WebCodex Desktop/OpenAI. Verify the ID if the available list
   appears stale or shows a similarly named Tunnel.
6. Set **Authentication** to **No Auth / None / No authentication**.
7. Read the custom MCP server warning and select **I understand and want to
   continue** only if you trust this WebCodex installation.
8. Select **Create**.
9. On the **Add … to ChatGPT** confirmation screen, select **Connect**.

You do not configure OAuth in ChatGPT Web for this path. WebCodex keeps the MCP authorization credential locally and the Tunnel client injects it; ChatGPT Web does not need the local credential.

After saving the connector in ChatGPT Web, return to WebCodex Desktop. Saving a connector in ChatGPT Web does not magically upgrade local Tunnel evidence into an authoritative “connected” signal. If this WebCodex Desktop version has no stable external-MCP-client observation signal, it intentionally continues to say that it is waiting for ChatGPT.

**Success looks like:** the ChatGPT Web connector is saved and the WebCodex Desktop Tunnel remains running.

**If it fails:** verify that you selected or entered the current Tunnel ID, not the API key; Authentication should be No Auth / None / No authentication. If the Tunnel is missing or rejected, verify that it is associated with the target ChatGPT workspace and that your OpenAI Platform identity has **Tunnels: Read + Use** for that Tunnel; Developer Mode and Tunnel permissions are separate. If the available Tunnel list merely looks stale, compare the exact ID with the OpenAI Tunnels page or use **Use tunnel ID instead**. Do not paste the Tunnel API key into ChatGPT Web.

**Next:** immediately perform one real project read.

![Create a Tunnel plugin in ChatGPT Web](desktop-install/chatgpt-create-tunnel-plugin.en.png)

![Connect the Tunnel plugin to ChatGPT](desktop-install/chatgpt-connect-tunnel-plugin.en.png)

![Inspect the connected plugin and its tools](desktop-install/chatgpt-tunnel-plugin-tools.en.png)

## 8. Minimal acceptance check

After connecting, start with **one minimal real project read**, for example:
“List the WebCodex projects, then list the top-level files in the project I just
selected; report an empty directory as empty.” If it succeeds, the basic
ChatGPT Web → Tunnel → Server → Runner → Project path is working.

The following checks are optional and exercise additional capabilities:

- list the WebCodex projects;
- read a file from the project you explicitly added;
- create, read back, and remove one temporary file inside that project;
- run a read-only command such as `git status`, `uname -a`, or `ver`;
- if you enabled Computer Use, list windows or capture a browser window.

Run only the checks you need and understand, especially those that modify files
or use Computer Use.

If Desktop shows a healthy Service / Runner / Project / Tunnel but ChatGPT still cannot list projects or read a file, do **not** treat local green status as proof of an external connection. Recheck the Tunnel ID and ChatGPT connection configuration, then inspect Desktop Connection / Activity for the latest state.

## Troubleshooting

**OpenAI Secure Tunnel is disabled:** check Desktop's OpenAI Tunnel configuration diagnostics, which identify a missing Tunnel ID or API key. **Recheck configuration** observes the current process only; if using environment fallback, fully quit and reopen WebCodex after changing variables.

**macOS Terminal sees the values but Desktop does not:** Finder/Dock apps do not load `~/.zshrc`; use the Terminal-launch or `launchctl setenv` path above. Desktop Recheck does not execute shell startup scripts.

**Tunnel startup or ChatGPT connection times out:** check **Settings → OpenAI Tunnel network** first, then stop and restart the Tunnel after changing the proxy mode.

**A repository is not accessible:** add that directory explicitly on the **Projects** page instead of broadening the default Desktop project's filesystem authority.

**You see `project_not_loaded` / Project not ready:** use **Activate project again**. Desktop retries the same project and manages only its own Runner; normal users do not need to edit or understand the internal project registry.

**I closed the window and reopened it, but new environment variables are still missing:** since the background-lifecycle change, closing the window hides Desktop in the tray/menu bar. Use **Quit WebCodex** there, then start a new process.

## Keep the Shell, update the Runtime

Use **Settings → Runtime** to inspect an extracted official archive or native source build, then explicitly activate it. Do not edit the app bundle. See [Runtime compatibility and diagnostics](DESKTOP_RUNTIME_COMPATIBILITY.md); unknown/missing Custom files do not silently fall back to bundled binaries.
