# Linux terminal unified update adapter

The `environment update` command delegates discovery, cache/download validation,
installation classification, candidate preparation and reconciliation to the shared
Environment updater. The existing Core journal, owner receipt and installer broker
remain authoritative. No Desktop window, UI preferences, new pending transaction
or remote-device update mechanism is introduced.

Linux effects require explicit confirmation and stdin TTY admission before any
store/cache resolution or program probe. Core's existing system-service authorization
also selects normal terminal sudo under that TTY. The Environment owner does not
become root; literal argv elevates only the trusted installed installer helper.
Both the literal system-tool path chain and its resolved target must be root-owned
and protected against untrusted writes. No shell, password input collector or GUI
authorization fallback is added.

`status` and `check` preserve missing Environment/cache roots. Status does not discover
releases. Check does not download. Download is awaited with bounded cancellation.
Application revalidates the selected candidate and task state; service state and
ownership stay under Core's original prepare/finish contract. Original stopped
components stay stopped. Additional Runner installations still require the existing
official unified package; no standalone distribution is converted automatically.

Terminal apply/resume/rollback results explicitly reconcile only the exact pending
Environment, manifest and operation. Missing or superseding identity, unconfirmed
installed bytes and ambiguous nonterminal handoff fail closed. Restoration is limited
to Core-proven preparation before replacement, or an already restored operation.
Completion retries are limited to an already committed lease release. Other phases
retain the existing manual recovery route, without redispatch or competing rollback.
The public JSON wrapper is schema 1, at most 64 KiB, with static error categories and
whitelisted identities; raw configuration, service arguments, receipt bodies and
arbitrary child output are excluded.

## Actual local verification

Recorded 2026-10-06 on Linux x64 using a dirty source development build from
`upstream/main` `ed22e5c0` plus the shared adapters. No release/native package was installed.

| Check | Result |
| --- | --- |
| `cargo test --locked --offline -p webcodex-cli environment::update --profile dogfood` | 7 passed: parser targets, TTY/platform admission, absent readonly status, secret-safe errors, exact acknowledgement, literal argv and writable-alias rejection. |
| CLI `environment::` focused regression after guarded-handoff integration | 18 passed: the seven terminal-update tests plus eleven existing/guarded setup, identity, scope, secret and installer authorization tests. |
| `cargo build --locked --offline -p webcodex-cli --profile dogfood` | Passed on Linux x64. |
| Built CLI, isolated absent roots, `status --json` | Passed with no Environment/cache creation, no release discovery and bounded single-document stdout. |
| Built CLI, non-TTY apply/resume/rollback with explicit version/operation and `--yes` | Expected `interactive_terminal_required`, exit 1, bounded JSON stdout, no stderr or storage creation. No service-changing call was executed. |
| CLI `guarded_` compatibility regression | 3 passed, including additive Windows build-info and original non-Windows bytes (overlaps two environment tests above). |
| Rust formatting and diff checks | Passed. |

Shared Core tests independently cover exact terminal reconciliation, stale target
rejection, transaction privacy, candidate/receipt validation and original recovery
boundaries. They use disposable fixtures; they are not real installation acceptance.

Not executed: actual sudo authorization, SSH TTY installation, DEB/RPM replacement,
real system/user services, task-window interruption, logout/reboot, native upgrade or
rollback, and secret/configuration preservation on a real package upgrade. macOS and
Windows headless application remains unsupported by this command. Existing native
Desktop/manual paths and explicit low-level commands remain available.

## Integrated repair and isolated restart validation

The integration from main `6713748b` includes the original Windows invitation
fixture repair and the shared updater's cross-process durable-state correction.
Revised locked dogfood suites passed on Linux x64: unified updater 63, Core
upgrade status 11, original upgrade tests 9, CLI Environment 18 and guarded CLI
compatibility 3 (the guarded group overlaps two Environment tests). The CLI
dogfood build passed. Repeated independent CLI status processes preserved absent
roots; non-TTY apply/resume/rollback returned the expected refusal without writes.

The existing `scripts/e2e_job_reconciliation_ws.sh` ran against development source
`33532357073dde64fdb2761acb1526d7cebf042f` with `CARGO_NET_OFFLINE=true` and the
dogfood profile. All **102 assertions passed**. Five Server restarts cover active
Jobs, completion while offline, `run_process`, `run_script`, and `cargo_check`
handoffs. The original Runner instance and Job identities survived, observation
epochs refreshed without duplicate logs, and commands were never redispatched.
Only disposable loopback processes and generated projects were used. The cleanup
removed the temporary state and listener; no existing host service was controlled.

The built programs report version `0.5.0`, source `33532357073d`, dirty `false`:

| Development binary | SHA-256 |
| --- | --- |
| CLI | `403af6812833210f32644d7ee89ddb2a318667619fced19fb32a454d70942f55` |
| Server | `77bb9a88412da4385f93c16172d2060a2768fef79e38355d372126923c40d4f2` |
| Runner | `0f8f678fa9a31f71cb25017d2f59a3c5f7fbd7ce8601178d13c4d6a669493e2a` |

These are source-build process regression results, not release packages or native
installation, OS reboot, system authorization, installer handoff or rollback
acceptance. Those outstanding checks and their existing recovery boundaries remain
unchanged. No raw process output, project source, credentials or private configuration
is included in this report.
