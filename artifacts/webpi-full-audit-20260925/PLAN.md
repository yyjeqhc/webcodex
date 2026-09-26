# Execution Plan

## P0 — Preserve and baseline
1. Freeze runtime/Git/build identity and distinguish pre-existing dirty work from this goal.
2. Baseline Rust/Python/Node/frontend tests, public/loopback health, tool discovery and permission surfaces.
3. Inventory Pi resources/packages/extensions in Windows host and Ubuntu 26.04 WSL2 without approving/importing unknown code.

## P1 — Deep review
4. Correctness: file/search/edit/Git/process/jobs/artifacts/browser/computer/MCP/plugin/Pi flows, error/recovery/idempotency/outcome_unknown.
5. Security: auth scopes, project/path fences, credential redaction, OAuth/MCP exposure, plugin trust, command/script dialect boundaries, SSRF/path traversal.
6. Task quality/efficiency: bootstrap context size, tool selection, redundant round-trips, paging/retention, job continuation, token/result budgets, representative end-to-end task corpus.
7. Performance/resources: startup, discovery, search/read/write, tool dispatch, job observation, gateway limits, cold/hot cache, memory/CPU where measurable.
8. Product identity: WebPi public surface vs intentional compatibility identifiers; docs/frontend/CLI/OpenAPI/runtime/error text.
9. Operability: diagnostics, source alignment, service lifecycle, deploy/rollback receipts, tunnel probe, incident recovery.
10. Test/release: gaps in TDD, flaky/race coverage, Windows/WSL cross-platform tests, supply-chain and artifact provenance.

## P2 — Goal/Todo adapter design
11. Compare native Goal/AgentTask/WorkflowSession/Job semantics with pi-goal-x cachefix and rpiv-todo.
12. Build a thin WebPi-native planning layer: Goal is objective; AgentTask is durable executable work; dependencies/progress/presentation live in an adapter only where native semantics lack them; WorkflowSession and Job remain execution evidence.
13. Keep task truth single-sourced, fail closed on cycles/stale revisions, one active work item by default, never complete while required validation is red.

## P3 — First repair/optimization release
14. For each verified defect: reproduce -> minimal RED test where applicable -> fix -> targeted GREEN -> relevant broader regression -> diff review.
15. Run complete smoke matrix and representative quality/performance benchmark before/after.
16. Split coherent commits/PR(s), preserving unrelated pre-existing work; require clean build revision for release.
17. Produce rollback-capable artifacts; deploy only through WebPi service authority, then verify readiness/source alignment/auth/tool/public behavior.

## P4 — Web GPT plugin migration
18. Freeze deployed capability/permission/error contract.
19. Use current supported webpage plugin/MCP architecture; map skills/instructions/resources/tools to the deployed WebPi backend instead of implementing a second backend.
20. Test installation/auth, discovery, tool choice/arguments, confirmations, errors, artifact flows, long jobs, Pi resources and project isolation in the real web client.

## P5 — Second iteration and final release
21. Review migration deltas for UX, latency, quality, missing capabilities and naming.
22. Repeat strict TDD/smoke/performance/quality gates and independent diff review.
23. Create second PR/release, deploy, run post-deploy full smoke, document exact revision/hashes/rollback and unresolved risks.

## Stop/approval boundaries
- Unknown extension/package install or trust elevation requires explicit user approval after exact candidate/fingerprint review.
- Secrets are never requested in chat.
- Runtime permission denial is not bypassed through another interface.
- Current missing `communication:*` and `service:*` scopes are tracked blockers for native Goal creation and deployment respectively; development/review continues until those exact boundaries are reached.
