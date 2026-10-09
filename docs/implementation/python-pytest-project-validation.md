# Python/pytest project validation

`project_validate(action=test, adapter=python)` plans a bounded pytest run on the
owning Runner. `auto` uses the existing nearest `pyproject.toml` recipe root;
explicit Python also works without a manifest. Ambiguous recipes, escaping paths,
package scope (including `scope.all_packages`) and dependency policy fail closed.
Python check/format actions use the separate [Ruff project contract](python-ruff-project-validation.md). `cwd`
selects a recipe root, not a free pytest positional argument. Ordinary recipe
workflows retain their existing Python unittest/Ruff/mypy choices.

The canonical step is `python -m pytest --color=no -rA`, optionally followed by
one `-k` expression from `test.filter`. Filters preserve meaningful whitespace,
are bounded to 200 UTF-8 bytes, and reject controls and option-shaped prefixes.
There are no raw flags, executable, shell, install or environment inputs.

Before tests start, Runner resolves an existing configured/profile/PATH Python
interpreter using its existing script runtime selection. Configured Python wins;
otherwise Unix tries python3 then python and Windows tries python then python3.
Virtual environments are selected through the existing profile/PATH configuration,
without creating or searching for environments. A five-second Runner-owned probe
checks Python 3 and pytest availability with null stdin/output and managed process
tree cleanup. Probe and actual spawn use the same resolved executable and profile
environment, including configured PYTHONPATH. `PYTEST_ADDOPTS` is removed because
pytest parses it as extra command-line argv before root/config discovery; structured
validation owns that argv. Missing tooling returns definite not-started validation
unavailability. No installation or fallback occurs.

The parser accepts a complete final pytest terminal summary only. Missing,
truncated, duplicate, malformed or contradictory summaries cannot prove counts.
Executed-test evidence counts passed plus failed tests; skipped, xfailed and
xpassed items conservatively do not count toward positive proof. This first slice
does not claim hook-level accounting for every plugin outcome. Quiet/custom
plugins that suppress the standard final summary leave count evidence unproven.
Native process outcome must also agree: exit 0 has no failed tests, exit 1 needs
failed-test evidence, and exit 5 cannot have a positive count. Interrupted,
internal/usage errors and cancelled/timeout executions prove no completed count.

Default `require_tests=true` requires at least one proven executed test.
`min_tests` is a postcondition, independently of native process success.
`require_tests=false` preserves native exit 0 success when no minimum is specified,
even if counts are unknown or proven zero. Session records that explicit opt-out
native-success policy; unknown counts remain unproven and source freshness stays
independent. Native pytest exit 5 remains a failed execution, even with that
opt-out. Unknown counts never become zero.

Python uses the existing StartValidation Job, timeout, source fence, synchronous
cleanup guard and same-execution observation/reconciliation/cancellation paths.
Pending returns the existing Job continuation and never authorizes redispatch.
Python success retains the rich canonical receipt including backend, adapter,
target and policy; existing Cargo/Go sparse receipts are unchanged.

`project_validation_python_pytest_v1` is additive and false when omitted. The
running binary advertises it independently of local Python/pytest installation.
Explicit Python test planning and every pytest Job admission check that capability;
Auto may discover Python on the Runner but cannot enqueue it without current
support. Admission and post-queue replan fences cover pytest configuration from
the selected Python cwd through the registered Project root. If no recognized
Project-local config stops pytest's upward discovery, an ambient parent
pytest/config/setup marker fails closed instead of influencing the Job.

Focused validation commands:

```sh
cargo test --locked -p webcodex-core --lib project_validation
cargo test --locked -p webcodex-core --lib pytest
cargo test --locked -p webcodex-validation --lib
cargo test --locked -p webcodex-runner --bin webcodex-runner project_validation
cargo test --locked -p webcodex-runner-registry --lib project_validation
cargo test --locked -p webcodex --lib validation_handoff
cargo test --locked -p webcodex-tool-contracts --all-features --lib
```

The opt-in real-process fixture test requires an explicitly provisioned development
Python environment with pytest; the test never installs anything. Run with
`WEBCODEX_TEST_PYTEST_PYTHON=/absolute/venv/bin/python` and:

```sh
cargo test --locked -p webcodex-runner --bin webcodex-runner \
  --features runner-real-process-tests \
  runner_real_process_project_validation_python_pytest_fixtures -- --ignored --test-threads=1
```
