# Python Ruff project validation (#962 P1)

`project_validate(action=check|format_check, adapter=python|auto)` resolves Ruff on
its owning Runner. Python `test` remains pytest. Rust/Go and ordinary recipe
workflows keep their existing semantics. This slice adds no public Ruff tool,
installer, environment creation, package scope, dependency policy or free flags.

Planning requires a Project-local `pyproject.toml` with a `[tool.ruff]` table and
an explicit `target-version` (`py37` through `py314`). The installed Ruff must
support that target. Any `extend` entry is rejected, including an empty one.
The manifest must resolve inside the registered Project and be at most 1 MiB.
The planner parses and hashes the same bounded bytes, including the resolved
relative manifest path. The existing admission and post-queue replans compare
manifest/invocation/root digests, exact step, cwd and target identity. Ruff uses
an explicit config, so ancestor and nested Ruff configs are not selected.

The protocol owns one exact argv per operation:

```text
python -I -B -m ruff check --no-fix --no-fix-only --no-cache --no-respect-gitignore --output-format json-lines --config pyproject.toml .
python -I -B -m ruff format --check --no-cache --no-respect-gitignore --config pyproject.toml .
```

Check overrides config-driven fixes and fix-only behavior. Format only checks;
there is no write/diff/output-file mode. Both disable Ruff cache and Python
bytecode writes, and disable gitignore discovery. `RUFF_OUTPUT_FILE` is removed
from probe and spawn environments. There are no model-supplied argv/env values.

Runner selects the existing configured/profile/PATH Python through its script
interpreter resolver. A configured Python is authoritative: invalid configured
paths fail rather than choosing a PATH alternative. Without a configured Python,
the existing platform candidate order applies. A five-second managed probe checks
Python 3 and the Ruff module in the exact selected interpreter, cwd and profile
environment. Ruff probe and spawn use `-I -B`: Python isolated mode intentionally
ignores cwd, `PYTHONPATH`, and user site packages, while preserving installed
interpreter site packages (including the selected venv). Pytest preserves its
configured `PYTHONPATH` and historical interpreter fallback candidates without
`-I`. Ruff also sets `PYTHONDONTWRITEBYTECODE=1`. Missing interpreter/module returns definite
not-started unavailability. No fallback follows a failed probe. Probe descendants
are cleaned up through the existing managed process-tree path.

`project_validation_python_ruff_v1` is separate from pytest and defaults false
when omitted. The Runner binary advertises protocol support independently of
installed tools. Explicit Python test planning requires the pytest capability;
check/format planning requires Ruff. Job admission checks the actual planned
adapter and step again, including auto selection and Runner replacement.

`python:ruff:check` and `python:ruff:format` have separate target identities and
evidence profiles in the existing registry. They have no static command adapter:
command planning requires Runner manifest resolution. Check diagnostics accept
bounded JSON lines and retain only validated rule codes with a fixed message;
paths, source, messages and fix payloads are not copied into safe diagnostics.
Malformed/truncated excerpts do not prove complete counts. Empty output does not
fabricate a zero diagnostic count. Format uses native process status and leaves
human-output file counts unproven. Neither profile borrows Rust, Go or pytest
counts. Existing source fences, timeout, cancellation, same-Job observation,
reconciliation and no-replay behavior apply.

## Validation limits

Ruff 0.16.9 was exercised on Linux aarch64 using a SHA-256-verified
official PyPI wheel unpacked into an isolated temporary Python environment.
Both exact canonical command forms were verified: lint and format returned
exit code 1 for deliberately failing source, JSON Lines carried lint rule codes,
and project settings `fix=true` and `fix-only=true` did not modify source.
No Ruff cache, Python bytecode, or project-local `ruff.py` execution was observed,
even with `PYTHONPATH` pointing at the project. The temporary wheel, environment,
and test files were removed; the system Python was not modified.

That evidence is for Ruff **0.16.9**, not a promise that every older or future
version supports these flags and targets. Unsupported versions or configuration
must fail without dropping safety flags, installation, or fallback.

Local Rust tests separately cover configuration authority, canonical argv,
identity/evidence, capability selection and admission, queue-time replanning,
missing tooling, and workflow-session evidence retention. Stub interpreter
tests verify process selection and environment wiring; they are not substitutes
for executing an actual Ruff binary. The opt-in real-process module-isolation
regression requires an existing Python 3 interpreter via
`WEBCODEX_TEST_RUFF_PYTHON` and the `runner-real-process-tests` feature; run
it explicitly with `--ignored`. The contract is Ruff's requested read-only
check/format behavior in a trusted interpreter, not an OS-level filesystem sandbox.
