# WebPi independent deployment progress — 2026-09-19

## Verified target and preserved work

The current WEBCODEX Runner successfully registered and accessed `E:\WebPi\webpi-core`. Its registered Project is `agent:desktop-4d5te6a-8b40cbe5f9914ca3:webpi-core-42107258`. This management Project identity is distinct from the WebPi runtime's own `webpi-local` Runner.

Both the source working copy (`E:\WebCodex Desktop\webpi-core`) and independent deployment have HEAD `3cb489dce8557a94a3e56c51ed71c634e3169939`, branch `webpi`, and pre-existing uncommitted changes. Fifty-five relevant source paths were compared. Twenty-four reviewed changed/new files were synchronized using exact source and destination SHA-256 preconditions, preserving unrelated files. No Git reset, clean, commit, push, tag, or release occurred.

Rollback evidence and previous files are in:

```text
.webpi-state/deployment-backups/prepare-20260919-195733-4269ba/
```

The backup includes a per-file source/previous-hash receipt, previous source files, previous compiled plugin/SDK output, and this independent installation's original local environment/manifest. Do not blindly restore its old permissive authentication configuration. No credentials or database were copied from the other working copy or from the WebCodex Desktop installation.

## Configuration and build completed

The independent installation now has `https://webpi.piforme.vip` configured as its public origin in both its environment and manifest. Shared-key quick-start, anonymous access, OAuth shared-key bridging, and MCP query-token access are disabled by the hardened standalone configuration. The existing nonempty bootstrap key and Action PAT remain this installation's own credentials.

The Node runtime, Pi package and file-linked plugin SDK resolve inside `E:\WebPi\webpi-core`, not into the other working copy. The Pi bridge was rebuilt in the independent directory. Rust source and runtime binaries were unchanged in this continuation; there was no claim of a new native runtime build. The observed native identity remains version 0.4.1, git commit `3cb489dce855`, `git_dirty=true`.

## Validation performed in the independent directory

| Check | Result |
| --- | --- |
| Pi bridge TypeScript build | Passed |
| Pi bridge/admin/filesystem/approval/runtime tests | 32 passed, 0 failed, 0 skipped |
| Python deployment/isolation/security tests | 34 passed |
| Real HTTP test with fresh temporary state and a newly paired managed PAT | 14 checks passed; 12 negative authentication cases returned 401, OpenAPI returned a schema, and the valid PAT succeeded |
| Temporary real-HTTP test cleanup | Listener closed and fixture removed |
| WebPi's actual native Plugin gateway candidate check | `ready=true`, `phase=ready`, 23 candidate tools |

The real HTTP fixture did not replace or restart the live production service. The candidate check is not evidence of successful activation.

## Current blockers and observed live state

The management Runner remains non-elevated. Read-only Windows process-image inspection confirmed that the running CLI, Server and Runner executables belong to `E:\WebPi\webpi-core\target\dogfood`. At observation time these were PIDs 27776, 27428 and 1584 respectively, with an old Python supervisor (22020). PIDs are observations, not safe commands to reuse later without identity revalidation.

The attempted Plugin `reload` was blocked by the tool safety check. It was not retried through a different interface. A subsequent read-only query confirmed the live provider still has 16 tools, rather than the 23-tool checked candidate.

Live status after preparation:

```text
auth_config_hardened=true
live_unknown_bearer_status=200
live_auth_hardened=false
restart_required_for_auth=true
```

Therefore the new authentication configuration is not active in the running Server. Do not enable unattended web-agent self-extension yet.

Cloudflared was observed stopped, installed as an automatic LocalSystem service using a token file. No Cloudflare credential was read or displayed. The last public checks of `/openapi.json` and `/api/actions/runtime_status` returned HTTP 530. Public deployment and post-restart end-to-end acceptance remain incomplete.

## First required local user action

Stop the old elevated WebPi foreground lifecycle from the local terminal that launched it (Ctrl+C), or use the administrator Task Manager to inspect and stop only the process tree belonging to `E:\WebPi\webpi-core`. Do not stop processes by basename alone, do not reuse an observed PID without checking its current executable path, and do not stop WebCodex Desktop's separate runtime.

The updated WebPi should subsequently run as an ordinary user, not as administrator. Before starting Cloudflared, require successful local authentication acceptance with the installation's Action PAT. After starting the existing Cloudflared service with the necessary local administrator permission, require public acceptance against the configured origin. Keep the native application's authentication enabled; do not treat 530, 403, HTML, or successful schema access as authenticated execution.

Normal-user commands after the old service is stopped (not claimed executed here):

```powershell
Set-Location 'E:\WebPi\webpi-core'
.\webpi.cmd run
```

Acceptance commands in a separate normal-user terminal:

```powershell
Set-Location 'E:\WebPi\webpi-core'
.\webpi.cmd verify --with-action-token
.\webpi.cmd verify --base-url https://webpi.piforme.vip --expect-public-origin https://webpi.piforme.vip --with-action-token
```

The public command is only meaningful after the tunnel is running. Custom GPT editor authentication and any Cloudflare account/UI decisions remain user-controlled. Never paste a credential into chat.
