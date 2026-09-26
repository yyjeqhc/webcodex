# MCP conformance baseline

WebPi tracks MCP wire compatibility with the upstream
[`modelcontextprotocol/conformance`](https://github.com/modelcontextprotocol/conformance)
referee in addition to its focused Rust tests. The baseline is evidence about the
checks that actually ran; it is not a blanket "MCP compliant" badge.

## Pinned referee and profiles

`scripts/mcp_conformance.sh` pins the upstream referee to the immutable commit in
`tests/fixtures/mcp/conformance/baseline.json`. The script rejects an external
checkout at a different commit, archives the pinned Git object into a fresh
script-owned build directory, and runs `npm ci` plus the referee build from that
archive every time. Existing `dist/` output and working-tree modifications are
therefore never trusted as pinned evidence. Updating the pin is a reviewed
test-contract change, not a floating dependency update.

The ordinary baseline runs two dated server profiles independently:

- `2026-07-28`, using the stateless per-request protocol and that revision's
  frozen requirements;
- `2025-11-25`, using the stateful lifecycle and its reconstructed frozen
  requirements.

Focused repository tests continue to cover `2025-06-18`; the conformance harness
is not used to invent a frozen requirements set that it does not publish.

Run the same baseline locally with:

```bash
bash scripts/mcp_conformance.sh
```

To reuse an already checked-out referee without changing the pin:

```bash
WEBPI_MCP_CONFORMANCE_HARNESS_DIR=/path/to/conformance \
  bash scripts/mcp_conformance.sh 2026-07-28
```

The supplied checkout must resolve to the exact pinned commit. A checkout supplied
through `WEBPI_MCP_CONFORMANCE_HARNESS_DIR` is read-only from this script's
perspective: invalid or mismatched checkouts are rejected rather than deleted or
rewritten, and local working-tree changes are ignored by the pinned `git archive`.
Reports are written to `target/mcp-conformance/reports` by default. An explicitly
supplied report directory must be empty or carry the WebPi ownership marker
before old profile output is removed. CI uploads the report directory even when
the gate fails so the raw evidence remains inspectable.

## What is exercised

The script starts one ignored, test-only Rust fixture on `127.0.0.1:0`. The
fixture composes the same Salvo `AuthMiddleware` and `/mcp` handlers used by the
production HTTP surface, with temporary database/runtime state and authentication
disabled in the isolated test config. The referee therefore reaches real WebPi parsing, dispatch and
rendering without production credentials or an external deployment.

No conformance-only tool is added to the production registry. Some upstream
scenarios intentionally expect named reference-fixture tools. When WebPi does
not expose such a tool, that result is a coverage gap until a test-only adapter is
provided; it must not be relabeled as a product protocol failure merely to make a
summary green.

Authenticated runtime behavior, including project-scoped ProjectGrant access, remains covered by focused synthetic-credential integration tests. The upstream server CLI does not provide a generic
way to inject WebPi authorization headers, so an authentication-blocked
scenario is inconclusive rather than a pass.

## Coverage-aware report gate

The upstream runner saves one `checks.json` file per scenario. After each profile,
`scripts/mcp_conformance_report.py` verifies the raw reports against the exact
required server-scenario list and the reviewed baseline.

Every `FAILURE`, `WARNING`, or `SKIPPED` check from a **scored required**
scenario must have an exact `<scenario>, <check_id>` classification in
`tests/fixtures/mcp/conformance/baseline.json`. Each classification also pins the
observed status and a SHA-256 fingerprint of the stable failure evidence
(`errorMessage` plus structured details/metadata when present, falling back to
the check's name/description). If the same check ID starts failing for a different reason or
with a different severity, the gate requires review instead of reusing the old
classification. Whole-scenario wildcards are rejected.

Classifications that rely on an optional capability also bind to the fixture's
actual capability advertisement. Before the referee run, the script probes the
same HTTP endpoint with `server/discover` for `2026-07-28` and `initialize` for
`2025-11-25`, keeps the relevant tools/resources/prompts/logging/completions
capabilities, and compares them with the profile snapshot in the baseline. If the
server starts advertising a previously absent capability, those optional
classifications require review even when the old check ID still fails.

The referee's `not_scored` server scenarios are still required to run and are
retained as informational protocol evidence. Their protocol verdicts do not
consume expected-failure classifications, but infrastructure failures such as a
scenario timeout or connection failure remain gate-blocking. Supported scored
classifications are:

| Classification | Meaning |
|---|---|
| `genuine_protocol_failure` | The intended WebPi behavior was reached and a protocol requirement is known to be violated. |
| `missing_harness_fixture` | The upstream scenario depends on reference-fixture behavior that the product endpoint does not expose. The product behavior remains untested by that check. |
| `optional_capability_not_implemented` | A capability is genuinely optional and is not advertised/implemented. The entry must cite capability/spec evidence. |
| `harness_limitation_pending` | The pinned referee itself marks or demonstrates a limitation/pending check. |
| `inconclusive_infrastructure` | Authentication blocking, timeout, harness exception, or other infrastructure failure. This is recorded but **never satisfies the CI gate**. |

The gate also fails when:

- no applicable `SUCCESS`/`FAILURE`/`WARNING` checks were emitted;
- an individual required scenario emits no verdict (for example an empty or
  INFO-only report);
- a scored or `not_scored` scenario report expected by the frozen requirements is
  missing, duplicated, or an unknown scenario is unexpectedly present;
- a check contains an unknown status;
- a new non-success check in a scored required scenario has no exact classification;
- a classified check changes status/evidence, disappears, or now passes;
- the referee exits abnormally, its exit code disagrees with the scored FAILURE
  set, or any scored/not-scored scenario emits `scenario-timeout`,
  `wire-schema-harness-error`, one of the pinned referee's generic top-level
  scenario-error checks, the outer `Failed to run scenario` shape, or an exact
  transport-exception form emitted by the pinned Node runner;
- the server capability advertisement changes while optional-capability baseline
  entries depend on the previous advertisement;
- an explicitly classified inconclusive infrastructure result is present.

For the pinned referee, exit `0` means no scored FAILURE and exit `1` means at
least one scored FAILURE; other exit codes are treated as runner/infrastructure
failures. This is stricter than trusting the process exit code alone because the
raw reports still need complete coverage and reviewed classifications.

The report gate's own regression suite is dependency-free:

```bash
python3 scripts/tests/test_mcp_conformance_report.py
```

It covers all-skipped output, missing/empty/INFO-only scenarios, unknown statuses,
abnormal referee exits, scored and not-scored infrastructure/transport failures,
changed status/evidence for an existing check ID, capability-advertisement drift,
ambiguous duplicate non-success check IDs, harness-pin mismatch, placeholder evidence, new scored
failures, informational non-success reporting, stale classifications, exact-path
JSON-RPC request-ID normalization, wire-schema payload drift, ordinary lookalike
objects, empty structured evidence, and rejection of broad masks.

## Raw evidence and review policy

Each profile retains:

- the referee's raw per-scenario `checks.json` files;
- the referee stdout/stderr log;
- metadata containing the WebPi source SHA, referee SHA, profile, upstream
  referee exit code, observed server capability projection, exact scored scenario
  list, and frozen `not_scored` server scenarios with their upstream reasons;
- the coverage-aware WebPi summary.

Baseline updates should be narrow. Add a classification only after reproducing
and understanding the exact check, then record its observed status and evidence
fingerprint. Fingerprints retain semantic diagnostics but normalize two narrowly
scoped referee noise sources: the numeric value (not presence/type) of the two
known SEP-2575 millisecond-epoch response IDs, and the duplicated offending
message only inside the structured `wire-schema-valid` violation list. For a
`ListToolsResult/tools/<index>/...` diagnostic, that volatile array ordinal is
resolved through the offending response to the referenced tool name before hashing,
so unrelated tool insertion/reordering does not rename the finding; moving the defect
to a different tool still changes the fingerprint. Wire-schema origin/context/errors/
specVersion remain hashed, and ordinary objects with similar field names are not
normalized. A changed fingerprint is a review prompt, not something
to refresh mechanically. Remove the entry as soon as the check passes. Do not replace
multiple check IDs with a scenario-wide exception. The SEP-2164 `data.uri`
finding is kept as an advisory SHOULD/WARNING rather than described as a MUST
violation.

This baseline deliberately records already-demonstrated 2026 result-shape findings without rewriting production behavior merely to satisfy the harness. Their focused Rust observations and planned follow-up belong in the baseline fixture; fixes belong in a separate protocol-behavior change so this baseline remains an independent measuring instrument.
