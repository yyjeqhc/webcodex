# WebPi Release Readiness Checklist

## PUBLIC RELEASE IS FAIL-CLOSED

The current WebPi tree does **not** have an enabled public packaging/publication pipeline. `.github/workflows/release-build.yml` keeps its public release candidate path behind `if: ${{ false }}`, and `.github/workflows/release-image.yml` keeps public server-image publication behind the same fail-closed guard. Those guards are authoritative.

**Do not tag, publish npm, publish a GitHub Release, or publish GHCR from this checklist while those guards remain disabled.** Historical npm/Desktop/GHCR machinery that is still present in scripts or workflow bodies is retained implementation/reference material, not a supported WebPi release path. In particular, the current source tree does not ship a supported Desktop implementation.

An explicitly requested development/dogfood deployment of a reviewed WebPi commit is a different operation. It is governed by [`AGENTS.md`](../AGENTS.md), [Runner Release Process Notes](agent/release-process.md), WebPi service authority, and the post-deployment acceptance gate below. It does not imply that public artifact publication is enabled.

## 1. Source and workspace gate

Before any deployable WebPi candidate is considered:

- identify the exact Git commit and branch;
- require a clean, isolated candidate worktree rather than packaging an unrelated dirty checkout;
- require `git diff --check`;
- record the exact Server and Runner build identity;
- keep existing user work and unrelated dirty changes out of the candidate;
- do not create tags, push commits, rewrite history, deploy, publish, or touch credentials without the authority required for that operation.

For a future public release, the candidate must first land on `main` and obtain the exact-main CI evidence described by `.github/workflows/release-readiness.yml`. A local pass is useful evidence but is not a substitute for that immutable remote proof.

## 2. Current CI and readiness evidence

Current WebPi validation is split across normal CI, Windows-specific checks, extended-native checks and the release-readiness workflow. Review the workflow definitions at the exact candidate commit; do not assume historical release behavior.

Important current contracts:

- `.github/workflows/webpi-checks.yml` validates the WebPi Windows surface, builds `webpi`, `webpi-server`, and `webpi-runner`, runs Rust/Python/JavaScript regressions, the isolated HTTP/Pi acceptance fixture, formatting and whitespace checks.
- `.github/workflows/release-readiness.yml` is a non-publication gate. It proves the exact successful `main` CI attempt, invokes extended-native validation, runs WebSocket/polling E2E, coding-loop compare evaluation, and disposable server-image validation.
- `.github/workflows/release-build.yml` contains retained candidate-packaging machinery but its public candidate path is disabled.
- `.github/workflows/release-image.yml` contains retained container-publication machinery but its public publication path is disabled.

If the delivery or observation of a remote workflow becomes uncertain, keep observing the same durable request/run identity. Do not dispatch a second copy merely because a response was lost.

## 3. Focused runtime validation

During implementation and review, use the smallest sufficient focused suites for the changed contract, then broaden before deployment. Relevant areas include:

- tool schemas/OpenAPI/MCP/GPT Actions;
- file/search/edit stale-write and project/path fences;
- Git review and recovery;
- structured processes, Jobs and continuation;
- Goal/AgentTask/WorkflowSession state;
- authentication/authorization and credential redaction;
- plugin/Pi trust and reload;
- service lifecycle, deployment manifest and rollback identity;
- Windows/WSL cross-platform behavior.

A compile-only result, zero-test filter, HTTP 200, or process-start signal is not a complete validation result. Where a behavior defect is fixed, preserve a meaningful RED -> GREEN test when practical.

## 4. Product and documentation gate

Current user/model/operator surfaces must describe **WebPi** consistently:

- public programs are `webpi`, `webpi-server`, and `webpi-runner`;
- current environment/configuration is `WEBPI_*`;
- MCP and GPT Actions call the same WebPi ToolRuntime;
- supported installation/deployment guidance must match files and workflows that actually exist in the current tree;
- retired Desktop/public-release material must be explicitly marked historical or fail-closed rather than presented as a current recommendation.

Do not mechanically rename stable compatibility identifiers. The identity contract in [`WEBPI_IDENTITY.md`](WEBPI_IDENTITY.md) intentionally retains internal `webcodex*` Rust/package names, the `@yyjeqhc/webcodex-plugin-sdk` package id, `webcodex-plugin-v1`, `webcodex-runner/1`, `wc_*` durable ids and other documented persistence/wire identifiers.

Run the WebPi brand/document-contract tests and the repository-local Markdown link checker. Any remaining historical `WebCodex` mention should be explainable as an explicit upstream/reference/compatibility or retired-history statement.

## 5. E2E and task-quality gate

For a candidate intended for deployment, exercise a representative coding loop rather than only isolated endpoints:

1. bootstrap project/session context;
2. inspect/read/search;
3. perform one safe edit in an isolated fixture;
4. demonstrate a failing validation when the scenario requires it;
5. fix and obtain GREEN validation;
6. review bounded Git/workspace changes;
7. finish with validation evidence and no unresolved unknown outcome.

Track result size, redundant Runner/tool round trips, approval interruptions, retries and elapsed time where they are meaningful. A performance change is accepted only against a comparable baseline without reducing correctness or evidence quality.

The release-readiness workflow owns the remote WebSocket/polling zero-config E2E and compare-eval evidence for a future exact-main release candidate. Local invocations remain diagnostic evidence, not publication authority.

## 6. Security and leakage gate

Confirm that:

- no secret, `.env`, credential file, token, private key or Authorization value was printed or committed;
- account/admin/pairing/token-management authority is not accidentally exposed through model-facing MCP/GPT tools;
- project/path/symlink/reparse and stale-revision fences remain fail-closed;
- shell dialects are explicit, and structured process tools are preferred unless real shell semantics are required;
- plugin/Pi package trust still follows discover -> inspect -> exact approval -> reload -> describe -> call;
- deployment/restart/rollback uses its normal service scopes and supervisor protocol rather than an alternate bypass;
- uncertain execution remains `unknown`/attention-required until reconciled.

## 7. Development/dogfood deployment gate

When the operator has explicitly requested deployment of a reviewed commit and the caller has WebPi service authority:

1. isolate a clean candidate revision;
2. run the relevant focused suites plus the complete pre-deploy smoke/regression matrix;
3. review the exact candidate diff independently;
4. record candidate commit/build identity and rollback target;
5. run WebPi deployment preflight and require source/revision/generation alignment;
6. deploy through WebPi service lifecycle authority only;
7. treat a lost lifecycle response as uncertain and reconcile the same operation identity;
8. verify the new exact build identity after the service reaches ready state.

If `service:restart`/`service:deploy` is not granted, stop at the deployment boundary. Do not substitute a shell/service-manager path to evade the denial.

## 8. Post-deployment acceptance smoke

After a new WebPi Server/Runner/runtime build is actually deployed:

1. verify compact `runtime_status`, readiness and exact source/build alignment;
2. verify loopback behavior before public exposure;
3. verify the public origin/tunnel and protected-route authentication behavior;
4. refresh MCP/GPT schemas if tool contracts changed;
5. run focused tool discovery;
6. enumerate the expected Runner/project;
7. run a read-only coding flow (`work_on_project`, read/search, bounded `show_changes`, hygiene);
8. run one small reversible edit in an explicitly safe fixture and review the diff;
9. run representative Job continuation and, where applicable, Pi/plugin capability discovery;
10. record the exact deployed revision, smoke evidence and rollback instructions.

A static HTTP 200, an HTML error page, unauthenticated response, or merely observing that a process is alive is not acceptance.

## 9. Re-enabling public publication

Public publication may become a supported WebPi capability only through a separate reviewed change that:

- removes the fail-closed workflow guards intentionally;
- defines the WebPi artifact/package/container names and ownership;
- removes or productizes the retired Desktop dependency instead of silently inheriting it;
- validates the exact native platform matrix and provenance;
- verifies immutable tag/source/version identity;
- defines draft/publication/rollback/reconciliation behavior;
- passes anonymous availability checks for any public package/container;
- updates this checklist and its contract tests in the same change.

Until that work is complete, retained release scripts and disabled workflow bodies are **not** instructions to publish WebPi.
