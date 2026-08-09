# WebCodex 0.3.4

[English](RELEASE_NOTES_v0.3.4.md) | [简体中文](RELEASE_NOTES_v0.3.4.zh-CN.md)

WebCodex 0.3.4 is an execution reliability and ergonomics release. It makes
model-facing command execution safer to retry, less dependent on shell quoting,
able to continue durably without rerunning work, easier to observe in batches,
more responsive over polling, and more predictable on Windows.

## Highlights

- **Truthful execution lifecycle and retry safety.** Structured execution state
  now distinguishes work that definitely did not start, work that may have
  started with an unknown outcome, timed-out work, and completed work. Models no
  longer need to infer retry safety from prose error strings.
- **Structured process and script execution.** `run_process` carries executable
  and argv as typed data, while `run_script` carries bounded script content,
  script argv, and stdin separately. Neither path reconstructs shell text, and
  both fail closed instead of silently falling back to shell execution when the
  required Runner capability is unavailable.
- **Same-execution sync-to-Job handoff.** A structured process or script that
  outlives the synchronous grace continues as the same durable Job with the same
  execution identity and timeout budget. Handoff does not cancel, restart, or
  replay the child process.
- **Batch Job observation.** `observe_jobs` can inspect up to eight existing Jobs
  in one bounded call, using one shared wall-clock wait and returning fresh
  sibling observations together after a meaningful change.
- **More responsive polling and practical Job concurrency.** Polling can keep
  making progress while an ordinary request is still running, without replaying
  execution. Runner Job execution now defaults to four concurrent Jobs, supports
  an effective `max_concurrent_jobs` range of 1 through 64, preserves FIFO
  promotion of the original queued Job, and exposes bounded running/queued/limit
  observability.
- **Deterministic Windows output normalization.** Local Windows process output is
  presented as bounded valid UTF-8 across UTF-8, BOM-declared UTF-16, and the
  active OEM code page. Streaming preserves split UTF-8 scalars, UTF-16 units,
  and OEM DBCS characters across chunk boundaries; PowerShell 5.1 script
  `param(...)` semantics and exactly-once timeout/stop behavior remain intact.

## Compatibility and behavior changes

There is no intentional breaking protocol change in 0.3.4. The new execution
capabilities and observability fields are additive and fail closed when an older
Runner does not advertise them.

Operators should note one intentional runtime default change: Runner Job
execution concurrency is now four. Set `max_concurrent_jobs = 1` when strict
serialization is required. The setting remains restart-required, and values are
normalized to the effective range 1 through 64. Polling dispatch capacity is a
separate fixed bound of two and is not derived from Job concurrency.

Clients that cache MCP/OpenAPI tool schemas should refresh them after upgrading
because the structured execution and Job observation surface changed.

## Structured execution details

`run_process` is for native executable/argv execution and never reconstructs a
shell command. On Windows, native `.exe`, `.com`, and extensionless PE images are
accepted; `.cmd` and `.bat` remain shell/script concerns and are rejected before
spawn on this typed path.

`run_script` currently supports explicit `sh`, `bash`, and `powershell`
interpreters through Runner-owned temporary script files. Named SSH Session
resources do not gain typed process/script transfer in this release; use
`run_shell` when shell or remote SSH shell semantics are intentionally required.

If a capable Runner cannot finish a structured execution during the bounded
synchronous grace, WebCodex exposes the already-running execution as a durable
Job. The public timeout remains one total budget rather than being reset at
handoff.

## Job observation and concurrency

`observe_jobs` accepts 1 through 8 existing Job ids and optionally their prior
observation tokens. A requested wait is one batch deadline, not a per-Job wait.
The call is observation-only: it does not launch, retry, stop, schedule, or
subscribe to Jobs.

Runner Job execution uses the existing bounded inventory and queue. The default
concurrency is four; eligible queued Jobs are promoted FIFO with their original
`job_id` and request, exactly once. `list_agents` and `runtime_status` expose the
effective static limit and authorization-filtered running/queued counts without
deriving an `available_slots` or `saturated` claim.

## Windows execution behavior

For local model-facing process output, Windows decoding follows a deterministic
order: UTF-8 BOM, wholly valid UTF-8, BOM-declared UTF-16LE/BE, then the active
Windows OEM code page. CRLF is presented as LF while lone CR is preserved, and
output is bounded again after transcoding so encoding expansion cannot bypass the
model-facing byte cap.

The 0.3.4 acceptance cycle exercised native Windows/MSVC execution including an
OEM CP936 environment, UTF-8 and UTF-16 output, PowerShell 5.1, split UTF-8/OEM
streaming, timeout/stop exactly-once behavior, and Runner service-context polling.

Remote SSH streams retain their existing remote UTF-8/lossy contract, and
persistent-shell framing is unchanged. Windows x64 remains a supported CLI +
Runner platform connecting to a remote Linux Server; long-running local Windows
Server operation and productized Windows SCM installation remain outside the
supported deployment model.

## Upgrade notes

1. Upgrade `webcodex`, `webcodex-server`, and `webcodex-runner` together from the
   same immutable v0.3.4 revision.
2. Verify all installed binaries report `0.3.4`, the same concrete commit, and
   `dirty=false`.
3. Refresh cached MCP/OpenAPI schemas so clients discover `run_process`,
   `run_script`, `observe_jobs`, and the current Job/runtime output fields.
4. If you set `max_concurrent_jobs`, restart the Runner after changing it; the
   setting is intentionally not hot-reloadable.
5. On Windows, continue using the CLI/Runner path against a remote Linux Server;
   the packaged `webcodex-server.exe` does not make long-running Windows Server
   runtime a supported deployment.

## Binary packaging

The planned v0.3.4 release artifacts are:

- `webcodex-v0.3.4-linux-x64.tar.gz`
- `webcodex-v0.3.4-linux-arm64.tar.gz`
- `webcodex-v0.3.4-darwin-arm64.tar.gz`
- `webcodex-v0.3.4-win32-x64.tar.gz`

Each publishable artifact must be built natively from the exact immutable
`v0.3.4` tag and contain `webcodex`, `webcodex-server`, and `webcodex-runner`
(`.exe` on Windows). Real SHA-256 checksums are generated from the final native
archives after tagging; they are not committed as placeholder release metadata.

## Known limitations

- `observe_jobs` batches observation only; it does not add batch Job launch or a
  new scheduler.
- Structured process/script execution over named SSH Session resources is not
  part of this release; intentional SSH shell work continues through `run_shell`.
- Remote SSH stream decoding and persistent-shell framing are unchanged by the
  Windows local-output normalization work.
- Windows supports client/Runner operation, not the long-running local Server,
  productized SCM installation, Windows ARM64, or UNC project roots.
- macOS x64 and other unpublished targets remain outside the release artifact
  matrix.

## Release validation

The execution A–F cycle completed cross-platform acceptance on Linux and real
Windows/MSVC, including MSI Runner service-context execution. The final release
candidate still follows the repository release gates: formatting, workspace
compilation and tests, focused runtime/schema coverage, native builds on all four
release hosts, npm self-tests and staged install smoke, provenance/build-identity
checks, Markdown link validation, checksum verification, and clean-worktree
review.

## Next steps

The execution A–F cycle is complete. Further execution work in this cycle is
maintenance/stabilization unless a new concrete reliability or product need is
demonstrated; 0.3.4 does not begin a new execution feature phase.
