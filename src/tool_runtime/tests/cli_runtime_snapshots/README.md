# Runtime-produced CLI snapshots

`cli_runtime_projection.rs` starts one in-memory Runtime and one temporary Git
checkout per related test group. It reuses the local Runner request responder:
file listing and reading use the real local filesystem, search executes the
Runtime-generated bounded search program, and Shell/Process/Job logs come from
commands that actually ran. The native argv cases reuse the bounded local command
helper and report a completed process only after obtaining its actual exit status;
spawn/capture/timeout failures remain fixture failures. Typed completion evidence
is supplied through the existing Runner result helper, not a fabricated ToolResult.
Every sample then enters the real MCP CLI adapter through
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

## Native response replay for projection comparisons

The inventory sample now also uses the in-memory Runtime fixture. For workspace
symbols, the native lane captures one actual Adaptive MCP response in
`workspace_symbols.runtime.json` when an installed language server is available.
`projects.runtime.json` uses the same native capture path. A capture contains the
canonical tool name, exact response, native binary hashes and transport label;
it never includes authentication headers or temporary credential configuration.

The ignored test
`mcp::tests::cli::text_projection::cli_runtime_replay_captured_native_response`
passes that exact response through the compiled CLI projection. It requires both
`WEBCODEX_CLI_RUNTIME_CAPTURE` and `WEBCODEX_CLI_RUNTIME_REPLAY_OUTPUT`; missing
input is a failure, not a successful no-op. It accepts only inventory/workspace
symbol MCP ToolResults up to 2 MiB, never normalizes identity or clocks, preserves
failure semantics and requires a new output file. Normal CI reports this test
as ignored; native replay coverage is reported only after explicit execution.

For a comparison, first compile the replay test before changing production
projection code and preserve the printed test executable in a dedicated temporary
path. Rebuild after the projection change, then run the native smoke once:

```bash
python3 scripts/e2e_cli_runtime_output.py --bin-dir target/dogfood --output-dir /tmp/webcodex-cli-native
python3 scripts/measure_cli_runtime_projection.py \
  --before-test-bin /tmp/webcodex-cli-replay-before \
  --after-test-bin /tmp/webcodex-cli-replay-after \
  --output-dir /tmp/webcodex-cli-replay-comparison \
  /tmp/webcodex-cli-native/projects.runtime.json \
  /tmp/webcodex-cli-native/workspace_symbols.runtime.json
```

Use the actual preserved executable paths; the replay helper starts neither a
Server nor a Runner. It verifies unchanged capture/binary hashes and writes exact
before/after text, Unicode scalar/UTF-8 byte counts and a separate JSON-response
size. These measurements compare two compiled CLI projections of the same real
Runtime producer output. They are not two end-to-end executions, Host token
measurements or remote latency measurements.

If the native lane skips the language server, omit the missing symbol capture
and report that no successful symbol comparison was measured. If indexing fails,
keep the actual failed response and identify its numbers as failure diagnostics.
Do not replace a missing or failed native symbol sample with constructed JSON.

Job batch total counts can be omitted only when an explicit nontruncated batch
contains exactly the requested and returned number of visible items.
Classification counts still state successes/failures/terminal changes. Log totals,
returned log lines and retention cursors describe different facts and remain
visible, including the unknown values in full observations.
