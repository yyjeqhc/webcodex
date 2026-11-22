# Desktop acceptance — 2026-11-22

## Release baseline on mini

The official v0.5.0 macOS ARM64 release was downloaded and its SHA-256 verified (`c891c76ee3a63685acd79aec828eb289c9d182c0ca3728fc664b31161484a0ac`). All three bundled runtimes report source commit `354541eaa729a1c4bb6f35d4add46a95db4fbca4`.

The release was copied to a separate application, then launched with an isolated XDG configuration and Desktop data directory. The native first-run UI successfully configured a projectless local Server and Runner. The existing remote-control Runner was preserved.

- `baseline-after-configure.jpg`: native UI after local setup.
- `baseline-tunnel-ready.jpg`: native UI after the embedded OpenAI tunnel became ready.

The account-owned tunnel was configured through the bundled CLI using a temporary private credentials file; the file was deleted after use. The native connection form was inspected, but this is not evidence that credentials were typed through that form. Native Restart Server applied the connection. A transient busy error occurred on the first attempt; the second succeeded. The final tunnel status reported local MCP ready, tunnel ready, matching configured/applied revision, and no restart required.

Browser tool invocation and the final candidate installation are separate acceptance steps and were not yet completed at this checkpoint.

## Windows console regression

On MSI, a real process test established a parent with no console and exercised production Git and SSH launch paths. Restoring the previous Git launcher reproduced a visible console; restoring only the previous SSH capability probe reproduced the same regression. The repaired paths passed the native checks.

The focused Windows verification passed 44 tests, with one existing ignored test: five Git/SSH/process-tree cases, seven user-task cases, one persistent PowerShell case, twenty structured-process cases, and eleven upgrade cases. These verify source-level behavior; final package installation remains a separate acceptance step.
