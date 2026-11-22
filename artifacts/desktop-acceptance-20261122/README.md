# Desktop acceptance — 2026-11-22 UTC

This report keeps source versions separate. The screenshots use the mini computer's UTC+8 local time; backend receipts use UTC. Builds and acceptance evidence are local development work on PR #1435, not a release or notarization claim.

## Latest installed macOS result

Clean `aa0b3add37b0a6f6a87b16021d0c977ed09ce202` was built with the official local helper, verified and installed at `/Applications/WebCodex Desktop.app`. The GUI, CLI, Server and Runner all report the same clean source and build time `1795369641`. The DMG SHA-256 is `044535f1141530c8166126a2d428dc1c0234e3716c469d608e040cbdaf1f321b` (55,889,903 bytes). All four strict signature checks and the App deep check passed; App/Runner retain the existing local development certificate, while CLI/Server have valid ad-hoc linker signatures.

One native Start created a single Desktop Server/Runner pair. The first Quick generation reached production running status in approximately 9.8 seconds and Discovery/registration availability about 2.5 seconds later, with no reconnect, external probe, DNS or proxy change. Fresh Edge ChatGPT and Chrome Gemini conversations both read the same main-repository proof file and ran the same marker command through that pair.

Gemini's first account-link attempt failed after WebCodex issued an authorization code. Its exact cause was not retained. One ordinary retry from Gemini's visible retry prompt, using the same URL and unchanged background services, completed OAuth and synchronized 30 tools. This failure and recovery are retained below; the result is not described as first-attempt OAuth success.

| Final identity | Verified value |
| --- | --- |
| Desktop Server | PID `20203`, instance `65ac62d6-469f-4d3b-b0fd-7675a72be711` |
| Desktop Runner | PID `20210`, client `device-47cbdf021f154085`, instance `a6d9a69f-bdf4-44d7-82b8-899c21747aac` |
| Cloudflare child | PID `20246`, generation `1`, Server-owned |
| Gemini MCP URL | `https://reprints-too-hugh-argue.trycloudflare.com/mcp` |
| Final GUI | PID `36628`, opened normally in Finder after quitting PID `20029` |
| Existing inventory | Seven registered projects retained; no running or queued tasks at final observation |

Seven installed connection-page locales were visually checked. Native GUI Quit/reopen preserved Server, Runner and Cloudflare process identities, creation times, generation and public URL; real ChatGPT activity returned on the new GUI. The final app is left on Chinese Home with both connections ready. The pre-existing control Runner PID 1578 remains unchanged.

Full package hashes, version pins, copied browser results, exact traces, both OAuth attempts, native close/reopen observations, CI and 19 cropped screenshots are indexed in [aa0-browser-e2e.json](aa0-browser-e2e.json).

## Acceptance matrix

| Source | Machine and package | Observed result | Remaining boundary |
| --- | --- | --- | --- |
| Official v0.5.0, `354541e` | mini, separate official ARM64 app | Native projectless local setup; OpenAI Tunnel; Edge custom app invoked Runtime status and empty project inventory | Tunnel secrets were configured through bundled CLI/private temporary file, not typed into the inspected native form |
| `7ef8601` | mini, official helper local package | Fresh local Server/Runner; Chinese first-run and two-field ChatGPT form; actual Gemini failure and legacy migration conflict captured | Those findings drove subsequent fixes; this is not final dual-browser acceptance |
| `7ef8601` | MSI, official helper Windows package | Three console observation periods totaling 35 minutes / 165,399 samples / zero newly visible console windows; seven languages; normal service startup and GUI close/reopen | Native Runner restart failed and exposed a lifecycle race |
| `a819e298` | mini, installed candidate | Native legacy adoption preserved seven project registrations and existing credentials; both real browser clients read the same repository file and ran the marker command through one Desktop pair | Subsequent source fixes and package tests are recorded separately |
| `5c4acefe` | mini, installed candidate | Fresh real ChatGPT calls succeeded | Three new Quick generations failed public readiness near the previous 60-second deadline; do not count Gemini as passed on this version |
| `5c4acefe` | MSI, official NSIS install | Actual installer and four installed binaries matched; normal GUI launch; 22,100 console samples / zero new visible console windows | User shut down MSI before Runtime Start, Restart, Gemini forwarding/Discovery or GUI close/reopen on this package |
| `9f642542` | mini, installed official-helper package | Both real browser clients succeeded through one pair; real Gemini OAuth; Quick ready in approximately 8.6 seconds; Chinese/German UI; native GUI Quit/reopen retained all background identities and URL | Home/Connections still failed to query ChatGPT activity after reopening; defect explicitly recorded and repaired in 2bb |
| `2bb0297e` | mini, installed activity fix | Fresh ChatGPT file/command calls; Chinese/German Home and project states; GUI cold reopen restored real activity | Gemini passed forwarding but final admission failed with configuration-lock error; retained as a failed generation |
| `aa0b3add` | mini, final admission repair | Both real browser clients succeeded through one exact pair; first Quick generation ready in approximately 9.8 seconds; seven-language connection UI; cold GUI reopen retained activity, background identities and URL; first OAuth attempt failed, one same-URL retry succeeded | No later package was installed on MSI; real-account Named Tunnel remains outside this run |

## Shared-runtime browser proof

The read-only task uses `target/desktop-acceptance-20261122/connection-proof.txt`, followed by `run_process` with executable `/usr/bin/printf` and the sole argument `desktop-shared-runtime-ok`. Browser responses report Server commit/dirty flag/PID and Runner client/instance. Matching those identities establishes the same Desktop environment; the pre-existing control Runner remains separate and unchanged.

The test file is:

```text
WebCodex Desktop acceptance 2026-11-22
Expected marker: desktop-shared-runtime-ok
This file exists only for read-only ChatGPT and Gemini connection checks.
```

Gemini custom app setup used the signed-in Chrome UI, Google consent, the native Desktop credential-copy confirmation, a password paste into WebCodex login, WebCodex OAuth consent and tool synchronization. Credentials were not read from the clipboard or included in evidence. Chrome's password-save prompt was declined. Runtime mutations were not part of the browser task; only exact project/file/command access was used.

ChatGPT's successful command trace does not retain a numeric exit code. Its browser response and trace confirm the marker and tool success; zero is not inferred. For Gemini the separately observed exact Workflow Session includes structured command completion and exit code zero. Browser copies verify the marker, not byte-exact newline behavior.

### Final aa0 browser and OAuth receipts

ChatGPT's fresh prompt was sent at 18:03:34Z. The exact main-repository/OpenAI-window trace records `read_files` at 18:03:50.792Z (`daec8da7-126c-4af4-9e1d-c60189e4f843`) and `run_process` at 18:03:59.782Z (`47045d0e-5b75-4613-843f-544531a0492c`), both successful. The copied reply identifies clean aa0, Server 20203 and Runner instance `a6d9a69f-bdf4-44d7-82b8-899c21747aac`, and returns the expected file and marker.

Gemini's new conversation is `https://gemini.google.com/app/e3459a447e01e2e5`; its exact Workflow Session is `wc_sess_LhuFbVo8341Rg1zj`. A fresh prompt at 18:12:54Z led to `work_on_project` at 18:13:39.427Z, `read_files` at 18:13:44.982Z (`9c9950ce-87a5-45a3-b391-adccb31391d6`) and `run_process` at 18:14:47.150Z (`6c9d7e31-1637-42a0-9cd6-28fc49810a36`). The command parameters were expanded in the actual browser and approved with Allow once. The separate Session execution entry records `completed` / `succeeded`, exit 0 and 107 ms, without a Job handoff. The browser reply reports the same aa0 Server and Runner identities as ChatGPT.

OAuth was not uniformly successful on the first attempt. The first client registered at 18:04:32Z; Allow issued a valid S256 authorization code at 18:07:43Z. At the retained observation that code was unused and there were no corresponding access or refresh records. Gemini displayed an account-link retry error. The existing Server did not retain a token-endpoint HTTP error record, so this cannot distinguish a callback/network failure, a request that never arrived, or a client pre-validation rejection. It does not establish a configuration-lock failure or an ingress restart.

One normal retry used the same generation-1 URL and valid browser login session. The second client registered at 18:10:07Z, Allow issued its code at 18:11:00Z, and a transaction at 18:11:04Z consumed it and created matching access and refresh tokens. Google then displayed 30 synchronized tools. Both attempts and the same-runtime continuity are preserved in the final JSON; no secret token, authorization code or credential is included.

The actual aa0 first-start timing was below 60 seconds and does not by itself prove deadline extension or lock contention occurred. The respective deterministic regressions establish those boundaries.

## First-run, migration and connection fixes

The default first-run path prepares one local Server and Runner, then presents ChatGPT and Gemini as separate connections. ChatGPT's ordinary form needs Tunnel ID and Tunnel key. Gemini reuses its configured Cloudflare profile or prepares Quick, enables dynamic registration, checks public forwarding and Discovery/DCR, then provides the complete MCP address and login credential guidance.

The baseline's Tunnel had a protected binding to its isolated local Server port. Reusing it in a different environment correctly failed; the repaired UI preserves the exact safe reason and explains the recovery. Protected history was not removed. A separate account-owned Tunnel is used for the actual candidate.

The original default Desktop data were backed up before candidate installation, including a consistent SQLite backup with integrity verification. Native legacy adoption retained all seven registered projects, their existing identities, and credentials. Two historical registered absolute directories no longer exist; retaining those entries no longer blocks the entire inventory migration. The selected target and conflicting identities remain validated.

The installed `a819` legacy migration proof and successful browser checkpoint are retained in [a819-browser-e2e.json](a819-browser-e2e.json). Early conflict screenshots remain historical evidence rather than a claim that the final UI still fails.

## Cloudflare readiness

The first candidate's default protocol failed to establish the connector on this network. An independently owned diagnostic established an edge connection with HTTP/2; Quick onboarding now explicitly uses HTTP/2. The existing Named policy is preserved.

Later `5c4` attempts showed a second problem: a Quick connector and local Server could be ready while the newly assigned hostname remained unusable through native resolution/public TLS near the 60-second deadline. A diagnostic later observed the same exact route become reachable. This evidence does not distinguish authoritative publication from negative caching and does not establish a global Cloudflare outage. No DNS, system proxy or browser proxy configuration was changed.

Only embedded Quick forwarding now has a 120-second bounded deadline. Each probe and retry remains inside the absolute deadline. Public TLS, expected profile/Server identity, revision, process generation and nonce are still required, with mismatches rejected immediately. Standalone Quick and Named retain 60 seconds.

The focused ingress suite at the 9f checkpoint passed 16 tests. Deterministic paused-time tests separately cover availability at 75 seconds, cancellation at 120 seconds and immediate mismatch rejection. The real `9f` run became ready in approximately 8.6 seconds without an external probe or DNS/proxy override. It did not exercise the extended part of the deadline.

The installed `2bb` first generation then exposed a different failure: `cloudflare_configuration_busy` after the exact forwarding proof had succeeded. The existing status projection also takes the environment setup lock while inspecting services; a one-shot acquisition at final admission could collide with normal reads. The actual lock holder at the failure instant was not retained, and the old error mapping also lost the underlying lock error. The approximately 83.65-second total startup interval does not isolate forwarding duration.

The subsequent repair waits up to five seconds, only for the lock API's explicit `setup_busy` result and only in embedded mode. Standalone retains immediate failure because of its separate lease/control budget; Windows retains its previous conditional behavior. Storage errors fail immediately in a distinct category. After acquiring the lock, the same final configuration, generation, lifecycle, owner and database fences execute synchronously without an await. The wait creates no background retry task. The final focused ingress suite passed 22/22 with no failures or ignored tests (Job `wc_job_PGv0Xeoz0kL-P6aF`). Six real setup-lock regressions cover release before deadline, the five-second ceiling and immediate standalone behavior, non-busy storage failure, changed configuration/revision, a replaced attempt, and cancellation without later admission. These tests enter the post-forwarding commit boundary explicitly and do not claim to simulate real TLS or OAuth.

See [5c4-browser-and-network-evidence.json](5c4-browser-and-network-evidence.json), [9f-browser-e2e.json](9f-browser-e2e.json) and [2bb-activity-and-admission-evidence.json](2bb-activity-and-admission-evidence.json).

## Activity-state repair

Real browser calls on `9f` succeeded, but Home and Connections still claimed no ChatGPT activity after GUI reopen. The exact selected project had recent `openai-session` records readable through the installed CLI. The persistent-environment refresh path had never initialized the in-memory adapter's binary cache, and the observation code silently skipped its query.

The repair captures the selected adapter/project/source/revision, resolves the existing selected binaries and queries outside the main state lock, then checks operation generation and target identity immediately before applying the result. Errors remain errors, successful empty reads are explicit, and older or different-target results cannot overwrite current evidence. The existing Windows CLI no-window launcher is retained.

The UI distinguishes never queried, pending, failed and successfully empty activity. A failed refresh preserves the previous verified timestamp. Home labels its in-memory lifecycle/event history as Desktop events; project rows with no supplied session summary show Not checked instead of falsely asserting no activity. New copy covers all seven supported locales without broadening queries or permissions.

The final Rust activity filter passed 5/5, including actual CLI fixtures for a reopened environment with no binary cache, a successful empty history and a command failure, plus identity fences and source filtering. The two relevant frontend files first passed 202/204; the two new interaction tests then passed after correcting their composite-span text match. The final missing-summary row assertion passed separately. TypeScript, form-control CSS, seven-language copy integrity and diff checks passed. Per-run evidence is kept separate.

Installed `2bb` native verification passed this cold-reopen scenario. The old GUI PID 89774 exited normally and Finder reopened PID 99704; Server 89977 and Runner 89984 retained their original creation times and instances. Chinese and German Home showed real ChatGPT activity again with seven projects, while the empty Desktop-events label correctly described the new app session. The prior failed Gemini generation stayed generation 1/error; GUI reopen did not retry it. The first post-start timestamp was independently traced to real `read_workbench_state` calls in the existing OpenAI window, not the observer CLI's own query.

Final aa0 native acceptance repeats the full cold-reopen scenario after both browser clients succeeded. At 18:21:54Z the old GUI 20029 was absent, while Server 20203, Runner 20210, Cloudflare 20246 and the control Runner 1578 retained all pre-quit PID/birth/parent/path identities. Finder opened GUI 36628 at 18:22:12Z. The 18:22:32Z observation confirmed the same background processes, runtime instances, generation 1 and public URL, with registration/OAuth/tool flags true and seven projects.

German Home in the new GUI showed both connections ready and a real recent ChatGPT activity time; its empty Desktop-events label correctly described the new GUI session. Chinese Home was then checked and left selected. No Start or Reconnect action was used during the reopen check.

The final installed Gemini connection page was also checked in simplified Chinese, traditional Chinese, English, German, French, Japanese and Korean at 1180 × 760. A second reviewer inspected all seven saved images: no visible overlapping text, horizontal overflow or clipped controls; German's longer label caused the URL to wrap to four lines, and all primary buttons retained readable white text. This scope is the seven connection pages, plus Chinese/German Home, rather than an exhaustive review of every page.

## Windows acceptance

Acceptance around `7ef8601` included three native Windows observation periods totaling 35 minutes and 165,399 samples with no newly visible console windows. This also covered all seven supported languages, normal Desktop Server/Runner startup, and reopening the desktop window without duplicate services. Runner restart failed during that acceptance: the replacement started before the previous owned worker exited. The repair waits for the owned host and worker kernel handles and verifies the returned Scheduler instance.

The ordinary DOS and extended Windows path spellings of the same executable were also reconciled without relaxing SID, arguments, task definition, working directory or process ownership checks. The final path fixture passed 54 cases and lifecycle fixture 15. Read-only observation of the actual previous task confirmed matching identity. These checks do not substitute for a final GUI restart.

Clean `5c4acefe` was built by the official Windows helper, installed through the normal NSIS flow and opened using the desktop shortcut. All four installed executables matched those extracted from the actual installer. A separate observation period covering verification, installation and GUI launch recorded 22,100 samples and no newly visible console windows.

The user then requested imminent MSI shutdown. Runtime Start, Server/Runner Restart, Gemini Quick forwarding/Discovery and close/reopen were not performed on `5c4`; neither `9f` nor later revisions were installed there. All subsequent work used mini. The original MSI working tree and control Runner were preserved.

- [7ef installed-candidate report and locale screenshots](../../docs/ux/windows-acceptance-20261122/evidence-7ef8601.json)
- [Lifecycle fixture report](../../docs/ux/windows-acceptance-20261122/native-lifecycle-e1656c3.json)
- [Final 5c4 install/shutdown checkpoint](../../docs/ux/windows-acceptance-20261122/evidence-final-5c4acefe-shutdown.json)

## Official release baseline and rollback

The official v0.5.0 ARM64 release SHA-256 is `c891c76ee3a63685acd79aec828eb289c9d182c0ca3728fc664b31161484a0ac`, with bundled runtime source `354541eaa729a1c4bb6f35d4add46a95db4fbca4`. It ran as a separate app with isolated XDG/config/data directories and did not replace the pre-existing control Runner. The native welcome/setup and Tunnel-ready screens are retained as `baseline-after-configure.jpg` and `baseline-tunnel-ready.jpg`.

The Tunnel was configured with the bundled CLI through a private temporary credential file, deleted after use. Native Restart Server applied it; the first attempt had a transient busy result and the next succeeded. The signed-in Edge custom app invoked real Runtime status and an empty project inventory. Baseline-owned services were stopped/uninstalled before candidate work; the separate app and data were retained.

The original application/data and versioned candidate app backups remain available. Installs used the official helper and existing persistent local development signing identity; strict code-signature checks and four-executable source/hash comparisons were performed. These are self-signed local builds, not Apple-notarized release artifacts.

## CI and limits

Final source `aa0b3add37b0a6f6a87b16021d0c977ed09ce202` passed [CI run 45051418815](https://github.com/yyjeqhc/webcodex/actions/runs/45051418815): all 18 created jobs completed successfully. The original watch Job `wc_job_jdUBUyJNG41e53HA` completed with exit 0 at 18:13:24Z. Windows Desktop's frontend, Rust, exact runtime build/stage, unsigned NSIS build and newly built installer smoke steps all actually executed successfully; these remain CI evidence, separate from the stopped MSI acceptance.

Earlier clean-source CI checkpoints `2bb0297e` ([45048521136](https://github.com/yyjeqhc/webcodex/actions/runs/45048521136)) and `9f642542` ([45044395818](https://github.com/yyjeqhc/webcodex/actions/runs/45044395818)) also passed all 18 created jobs. The final documentation/evidence commit follows the tested source and does not change the installed binary identity.

Quick Tunnel addresses may change when the tunnel is recreated, requiring Gemini app address/authorization updates. Normal GUI Quit/reopen retaining the background connector was tested explicitly; that is different from stopping or recreating the runtime. Named Tunnel retains its fixed-address configuration path, but no real Named account acceptance was performed. Previous seven-language Windows screenshots belong to `7ef`. The final aa0 macOS connection-page screenshots cover all seven languages; Home and the cold-reopen activity checks cover Chinese and German. Copy integrity checks also cover all seven locales.

This work updates the existing PR. It does not merge it or publish a release.
