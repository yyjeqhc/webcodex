# Runner test environment isolation and CI evidence

## Scope

Follow-up to the maintenance review after #773. Baseline is
`b967a1556ac58b50e32c8399196979fdbae1713c` (#774).

One reviewed historical CI Job (run `36516341399`, Job `109239344597`,
source `d4626687`) terminated the Runner libtest executable with SIGSEGV.
The log has no crash backtrace, so its last completed test does not identify the
culprit. Environment mutation is a demonstrated shared-process risk, not a proven
root cause of that crash. This change does not claim a reproduced SIGSEGV fix.

## Environment ownership

Runner test code no longer calls process-global `std::env::set_var/remove_var`.
The old EnvGuard restore-on-drop helper is removed. A test mutex coordinates only
participants; it cannot serialize concurrent native library environment readers.

Tests whose subject is inherited environment (CLI precedence, shell credentials,
non-Unicode values and provider env mapping) re-execute their exact named test in
a child with `Command::env/env_remove`. An explicit case selector allows a matrix
without mutating environment between cases. Assertions run in the child; the
parent requires both the completion sentinel and one passed test. The helper
never retries, passes EOF stdin, drains both output streams with bounded retained
diagnostics, owns the process tree, and uses one absolute 60-second deadline.
Missing/incorrect test selection cannot silently succeed with zero tests.

Pyright fixtures instead pass their executable directly into the same internal
adapter implementation. Executable override validation is tested from explicit
input values. No fixture can temporarily replace another test's PATH or Pyright
override. Public validation arguments and production resolver precedence stay
unchanged; no new user configuration or dependency is added.

## CI result facts

Linux Rust matrix jobs use `scripts/ci_rust_test.py` around the existing command.
It executes once with unchanged Cargo selectors, features and libtest concurrency,
streams output to the normal CI log and returns the command's exit code. The JSON
artifact contains bounded test names/statuses, complete suite summaries, reported
build durations, command elapsed time, source SHA, platform, shard and run attempt.
It distinguishes a signal crash, compile failure, test failure, other command
failure and a successful command without test summary. No culprit is inferred
from output order and no per-test duration is invented from interleaved output.

Raw stdout/stderr, assertion bodies, arbitrary paths and environment values are
not copied into the JSON artifact. Original GitHub logs remain the detailed source;
the report is a diagnostic index, not a replacement test runner or retry policy.
Artifacts are named by shard/SHA/attempt and retained seven days. If cancellation
kills the wrapper before it can finish, an artifact may be absent; absence is not
success. CI's existing failing-command gate remains authoritative.

## Validation and limitations

Run the same ordinary lane as the historical failure:

```sh
python3 -m unittest scripts.tests.test_ci_rust_test
cargo test --locked -p webcodex-runner -p webcodex-lsp --features runner-real-process-tests
```

The real-process feature compiles its gated tests but does not run ignored tests
unless explicitly requested. Native Windows/macOS execution, sanitizer builds,
core dumps, stress-test failure probabilities and production crashes are not
established by a Linux ordinary-lane success. Preserve first-failure evidence and
pursue a backtrace if the historical crash recurs rather than weakening a test.
