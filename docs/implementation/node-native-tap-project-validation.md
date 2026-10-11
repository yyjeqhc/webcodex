# Node native TAP Project Validation

An additive `project_validate(action=test)` profile under [#962](https://github.com/yyjeqhc/webcodex/issues/962). It complements the previously merged Node script-check profile from #995; it **does not** execute general npm, Vitest or Jest test scripts.

## Selection and execution

- Runner resolves the canonical Project root and selects a regular, Project-local `package.json` via the existing Node manifest guard.
- The `scripts.test` value must be **exactly** `node --test` or `node --test --test-reporter=tap`. The manifest is used as an explicit opt-in signal; its script is never executed by `npm` or `node --run`.
- The Runner-owned command is always `node --test --test-reporter=tap`, with no caller-controlled argv, package/workspace scope, filter, dependency policy or environment additions.
- The same configured Node interpreter is probed and executed. The existing bounded version probe requires Node 22.3+, strips `NODE_OPTIONS`, and does not install or download any tooling.
- An independent, non-baseline Runner capability `project_validation_node_tap_v1` gates both planning of explicit Node requests and Job admission. Auto-detected Node jobs receive an admission check against the **actual** retained Node test adapter.
- Provenance binds the manifest content, command identity and Project root into durable source fences. The native Node path repeats the source recheck after interpreter preparation and before process handoff.

## Test evidence

The validation evidence parser accepts only a complete Node TAP v13 terminal summary with consistent `# tests`, `# pass`, `# fail`, `# skipped`, `# todo`, `# cancelled` and `# duration_ms` entries. Duplicate, incomplete, contradictory or truncated summaries are unproven. A process exit code of zero is **never sufficient** to count tests.

`tests_run_count` counts executed passed plus failed tests; skipped and TODO tests do not count. Cancellation, stale-state or contradictory process/evidence outcomes fail closed. The existing `require_tests` / `min_tests` policy remains effective. Even with `require_tests=false`, a missing or truncated native TAP summary cannot create a passing verdict.

Node tests themselves execute Project-authored code and can have filesystem/network side effects. TAP output is textual evidence, not a cryptographic attestation or OS sandbox.

## Excluded

- General npm/pnpm/yarn/bun scripts, Vitest, Jest, TypeScript compilation, arbitrary TAP reporters.
- Custom test filters or package/workspace scope.
- Any implicit dependency installation, isolation guarantee, or externally interpreted diagnostics.

## Regression requirements

Check native TAP pass/fail/skip/suite behavior, zero/contradictory/incomplete/duplicated/truncated report semantics, source drift after scheduling, explicit vs auto Node Runner capabilities, old-Runner denial, fixed argv, Node interpreter selection and environment sanitization, and ledger serialization of validation metadata. Tests for existing Node script-based `check` must remain unchanged.

## Root plan and accounting evidence

A complete native Node TAP report has a single unindented root `1..N` plan
immediately before the canonical eight-line accounting trailer. Indented plans
belong to nested suites and cannot substitute for a root plan. `N` counts
top-level assertions (including suite parents), not all nested tests from
the `# tests` footer. Top-level assertion ordinals must match this plan.

Duplicated/displaced root plans, repeated top-level numeric accounting fields,
bad assertion ordinals, contradictory counts and truncated reports cannot prove
test counts. Ordinary Node diagnostic comments (for example,
`# pass phase complete`) are not themselves reporter accounting entries.
Regression fixtures include real Node v26-shaped pass, failure, nested suites,
skip/TODO, a synthetic zero-test trailer and malformed reports. The existing
unavailable-count path remains authoritative even when `require_tests=false`.
This is an evidence-parser-only extension: no new npm/Jest/Vitest support,
Runner argv changes, source fences or public wire fields.
