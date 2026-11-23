# Windows Desktop acceptance

[English](desktop-windows-acceptance.md) | [简体中文](desktop-windows-acceptance.zh-CN.md)

Build the complete installer from the branch and use an isolated standard Windows account or a VM snapshot. Record the commit, Windows version, architecture, installer and observed results. Keep credentials and private paths out of screenshots.

## From installation to a tool call

1. Install and open Desktop on an account with no WebCodex services, projects or cloudflared. Watch for console windows throughout startup, preparation and restart.
2. Click **Get started**. Click again while preparation is in progress. Expect one setup using the packaged programs, with no folder picker, initial project or elevation prompt. Connections should open after the services are ready.
3. Check Server and Runner under **Settings → Runtime and services**. In Task Scheduler verify the current user, interactive logon, ordinary privileges and the action pointing to the installed `WebCodex.exe --user-task …`. Check the background host and its worker in Task Manager. In a terminal verify ordinary CLI output with `webcodex --version`.
4. Open **Connect ChatGPT**, enter your OpenAI Tunnel credentials, add the connection in ChatGPT and request WebCodex status. Record the response.
5. Click **Connect Gemini**. Observe component preparation and startup. Copy should become available only after forwarding and registration are verified. Copy the displayed HTTPS address and check that it ends in `/mcp`. Open Gemini.
6. Add the address under **Settings → Connected Apps → Custom apps**. Cancel authorization once and check that Desktop reports no successful call. Reopen the app, sign in on the WebCodex page with the owner’s user credential (available through Desktop’s Runtime Console credential-copy action), review access and consent. The Gemini configuration form needs only the address.
7. Select the app in a Gemini conversation and ask “Show my WebCodex Runtime status.” Record the actual tool response. Repeat the query in ChatGPT. Check endpoint readiness, authorization and calls separately.
8. Close and reopen Desktop with the existing background policy. Reboot and sign in as the same user. Check worker counts, Runner identity and connections. Unattended starts should reuse installed components.

On Windows ARM64, repeat the component download on a host that reports x64 user-mode support. It should install the same pinned, verified x64 cloudflared. A system without that capability needs a trusted compatible executable. [Windows architecture capability query](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getmachinetypeattributes)

## Upgrades, recovery and rollback

- Upgrade a second test snapshot that has projects, a Runner, OpenAI credentials and a Named connection. Compare the project inventory, Runner ID, fixed hostname and working connections. Check and confirm legacy migration if offered. Interrupt and resume it. Uncertain ownership should lead to inspection and preserve credentials.
- Stop, start and restart each owned Server and Runner in service settings. Confirm the old process tree exits, new worker PIDs are observable and task counts stay constant. Explicitly restarting an owned legacy console task should convert its action to the GUI host. Modify a test task's owner or action and confirm management refuses the foreign task.
- Disconnect before the first component download, inspect the download failure, restore the network and reconnect. Interrupt an established connection briefly and observe URL withdrawal and public health recovery. Rebuild Quick, copy the new address and authorize again in Gemini. Restart Named and verify the fixed address.
- Disconnect the app in Gemini. Edit the Desktop connection, turn off dynamic registration and apply. Verify old grants fail and ChatGPT still works.
- Restore the VM snapshot to roll back the complete test. For a manual downgrade, use this build to stop the verified owned tasks first. As the same user, run `webcodex environment uninstall-service runner` and `webcodex environment uninstall-service server`. Use `uninstall-service tunnel --profile PROFILE` for an owned standalone tunnel. Preserve the Environment directory and credentials, restore the matching older package and use its `environment resume` to recreate services. Inspect unresolved ownership or journal findings before continuing.

## Automated console regression

```powershell
cargo test --locked -p webcodex-environment --lib service::windows_user::tests
cargo test --locked -p webcodex-runner --bin webcodex-runner --features runner-real-process-tests runner_real_process_git_ -- --ignored --test-threads=1
```

The native inventory regression first creates a test owner with no console, then
runs the same bounded Git and SSH capability-probe paths used by Runner startup.
It checks that
this GUI/service-like launch creates no console window, while preserving stdin
EOF, both redirected output streams and the exit code. Running only from a
terminal can mask the failure through console inheritance. A hidden console from
`CREATE_NO_WINDOW` is valid; zero console membership is not required. Background
Runtime children use explicit windowless spawning, while ordinary interactive
CLI spawning retains its existing console behavior. These checks do not replace
the logon and visual installer checks above.

Record visual console behavior, logon recovery, real OAuth consent and real tool calls on Windows. The automated fixtures exercise separate process and protocol boundaries.
