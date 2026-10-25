# Runtime-produced CLI snapshots

`cli_runtime_projection.rs` starts one in-memory Runtime and one temporary Git
checkout per related test group. It reuses the local Runner request responder:
file listing and reading use the real local filesystem, search executes the
Runtime-generated bounded search program, and Shell/Job logs come from commands
that actually ran. Every sample then enters the real MCP CLI adapter through
`cli_call_for_test`; these snapshots are not hand-written ToolResult fixtures.
No standalone Server process is started for each sample. The local process
fixture is Unix-gated; Windows protocol/rendering coverage stays in the existing
platform-neutral suites, and is not claimed by these POSIX execution samples.

Prerequisites are a POSIX shell, Git, ripgrep (`rg`) and Python 3 on `PATH`.
The fixture fails immediately with an explicit diagnostic when `rg --version`
cannot run; it never skips the search sample or substitutes constructed output.
The Linux Server Rust CI shard installs ripgrep explicitly. A grep fallback is
valid Runtime behavior, but its backend evidence is a different observation and
must not silently replace the fixed ripgrep snapshot baseline. The native Runner
lane below also requires Bash and the separately built Server/Runner binaries.

Run and explicitly refresh with:

```bash
cargo test --locked -p webcodex --lib cli_runtime_ -- --nocapture
WEBCODEX_UPDATE_CLI_SNAPSHOTS=1 cargo test --locked -p webcodex --lib cli_runtime_ -- --nocapture
```

Review refreshed snapshots before committing. Each observation reports its raw
model-visible UTF-8 text bytes, Unicode scalar count and serialized response JSON
bytes separately. These are local adapter measurements, not Host token counts or
remote end-to-end latency measurements. The normalized snapshot includes an
`isError` line which is not part of the measured text.

Normalization is deliberately restricted to the fixture root, the one known Job
ID, read-revision identities, the observation-token nonce, and named clock fields.
Observation version/cursors, requested/effective timeouts, exit codes, file paths,
line ranges, truncation and failure evidence remain visible. Complete recovery
calls are parsed before normalization; the read continuation is invoked with its
original unmodified arguments and version fence.

Coverage includes live/recursive tracked listings, directory folding, contextual
and compound search, successful/failed Shell commands, versioned reads, edit
preflight rejection and recovery, bounded-read truncation, Job start/running/
completed observations, unknown-Job partial failure, a real process execution
timeout, and a deliberately lost transport completion after a real side effect.
The transport-loss case injects the fault at RunnerRegistry rather than inventing
an already projected failure result. Native successful edit planning/application
is not simulated here: it requires the separate real Runner integration lane.

## Native Runner integration lane

The opt-in `scripts/e2e_cli_runtime_output.py` reuses `InteractiveSmoke.prepare`
from the existing interactive MCP harness. One disposable loopback Server and
one WebSocket Runner serve the entire sequence. Temporary users, credentials,
projects, XDG/data directories and subprocess handles belong only to the fixture;
its `finally` cleanup stops only those two children. It does not build or install
binaries, change production configuration, or restart existing services.

After a separate dogfood build, run:

```bash
python3 scripts/e2e_cli_runtime_output.py --bin-dir target/dogfood --output-dir /tmp/webcodex-cli-output-after
```

The output directory must be new. It holds exact final response JSON/text and
`measurements.json`, with text UTF-8 bytes, Unicode scalar counts and raw HTTP
JSON-response bytes kept separate. Temporary authentication secrets are checked
against every persisted tool response; credentials and configuration are never
part of the ledger. Binary SHA-256 values identify the measured executables.
Use the same fixture and conditions on each source revision for a before/after
comparison; do not infer tokens or remote latency from these byte counts.

This lane adds actual native Runner dry-run and write success, unchanged-file
hash oracles for dry-run/stale rejection, explicit Workflow Session/cwd/Bash
execution, and a fixed-PAT scope denial checked before any filesystem effect.
It also captures project inventory, directory/search output and Job lifecycle.
An existing `rust-analyzer` triggers one bounded workspace-symbol request with
Cargo offline; otherwise the ledger explicitly records a skip. A readiness or
indexing failure remains a failed observation, not a successful symbol sample.
No language server or dependency is installed by the fixture.
