# Desktop acceptance — 2026-11-22

## Release baseline on mini

The official v0.5.0 macOS ARM64 release was downloaded and its SHA-256 verified (`c891c76ee3a63685acd79aec828eb289c9d182c0ca3728fc664b31161484a0ac`). All three bundled runtimes report source commit `354541eaa729a1c4bb6f35d4add46a95db4fbca4`.

The release was copied to a separate application, then launched with an isolated XDG configuration and Desktop data directory. The native first-run UI successfully configured a projectless local Server and Runner. The existing remote-control Runner was preserved.

- `baseline-after-configure.jpg`: native UI after local setup.
- `baseline-tunnel-ready.jpg`: native UI after the embedded OpenAI tunnel became ready.

The account-owned tunnel was configured through the bundled CLI using a temporary private credentials file; the file was deleted after use. The native connection form was inspected, but this is not evidence that credentials were typed through that form. Native Restart Server applied the connection. A transient busy error occurred on the first attempt; the second succeeded. The final tunnel status reported local MCP ready, tunnel ready, matching configured/applied revision, and no restart required.

An account-owned custom WebCodex app was then created through the signed-in Edge UI. Its real ChatGPT conversation successfully invoked `get_runtime_status` and `list_projects` against this isolated release environment; the expected empty project inventory was returned. The baseline services were stopped and uninstalled before testing the candidate. The separate application and data were retained.

## Windows console regression

On MSI, a real process test established a parent with no console and exercised production Git and SSH launch paths. Restoring the previous Git launcher reproduced a visible console; restoring only the previous SSH capability probe reproduced the same regression. The repaired paths passed the native checks.

The focused Windows verification passed 44 tests, with one existing ignored test: five Git/SSH/process-tree cases, seven user-task cases, one persistent PowerShell case, twenty structured-process cases, and eleven upgrade cases.

The clean `7ef86017942eb3d2cd012d22e1bac044cc9cf0f8` candidate was built with the official local helper and installed on MSI. The installed binaries matched that source identity. Real UI coverage included Git project registration, normal application close/reopen, all seven languages, and service operations. A visible-console monitor collected 165,399 observations across 35 minutes and detected no new visible console windows. The existing Windows Terminal was recorded separately as the baseline.

A native Runner restart exposed a separate lifecycle race: the scheduler reported Ready and started a replacement before the previous worker's kernel process handle was signaled. The new worker exited during that overlap. The repair waits for both owned host and worker handles, then verifies the one returned replacement task instance. The native PowerShell lifecycle fixture passed 15 cases plus the existing process-identity fixture. A final installed-package restart remains a separate acceptance step.

## Installed candidate on mini

The same clean `7ef86017942e` source was built with the official macOS helper, the existing persistent local signing identity, Node 22, and the dogfood runtime profile. The DMG SHA-256 was `75fb5491be30cf919d0ab4b0217cb0051dc7ccff320a7373476232fd18f0a143`. All bundled runtimes reported the expected clean source. The previous application and default Desktop data were backed up, including a consistent SQLite backup verified with `integrity_check`.

Fresh-install native UI evidence:

- `candidate-fresh-welcome-zh-CN.jpg`: initial welcome and one-button local setup.
- `candidate-chatgpt-minimal-form-zh-CN.jpg`: the ChatGPT form requires only Tunnel ID and API key.
- `candidate-cloudflare-forwarding-failure-zh-CN.jpg`: the actual first Gemini Quick Tunnel failure.

The fresh setup created one Server and one Runner automatically. Reusing the baseline Tunnel in a different local environment correctly encountered protected binding history, but the UI initially collapsed the specific reason to an unhelpful generic state. The new diagnostic path preserves the safe machine reason and explains that the original environment or a new Tunnel is required. No protected tunnel history was removed.

The first Gemini Quick Tunnel attempt timed out on this network. A bounded, independently owned connector diagnostic reproduced UDP/QUIC startup failure with the default automatic protocol policy; explicit HTTP/2 registered an edge connection in 6.8 seconds. The repair explicitly selects HTTP/2 only for Quick Tunnel onboarding, preserves the existing Named Tunnel policy, and requires both a Quick URL and an established edge connection before the existing end-to-end forwarding probe. Diagnostic processes were cleaned up, and no private connector output is included here.

The isolated candidate services were stopped and uninstalled before reopening the default Desktop data. The native application restored the user's original local runtime, existing ChatGPT connection, and seven registered projects. Its migration preflight then exposed a separate case: two historical registered project directories no longer exist. The initial implementation rejected the complete inventory with `migration_target_conflict`. `candidate-legacy-migration-conflict-zh-CN.jpg` records the actual error; `candidate-legacy-migration-conflict-de-DE.jpg` also verifies the long translated layout on macOS. The migration repair now preserves each previously registered absolute path when the directory is missing, while retaining the original identity and available-directory checks. All four focused legacy-upgrade tests passed. Real migration in the final installed candidate and shared-runtime browser acceptance remain separate pending steps at this checkpoint.

## Scope of this checkpoint

CI for `dd0ad5f4b5286cf8d408265441c09e8470dc6964` completed successfully across Windows, macOS, Linux, native/Desktop checks, and MCP contract checks (run `45030432720`). The native discoveries above are additional changes under validation. These observations are not a claim that the final ChatGPT and Gemini browser acceptance has passed.

The [Windows installed-candidate report](../../docs/ux/windows-acceptance-20261122/evidence-7ef8601.json) links all seven-language screenshots and records the native process timelines. The [Windows lifecycle fixture report](../../docs/ux/windows-acceptance-20261122/native-lifecycle-e1656c3.json) records the production and test source hashes. Both distinguish the installed candidate from the subsequent fixes.
