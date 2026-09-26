---
name: webpi-verification-before-completion
description: Verify a WebPi task immediately before declaring completion. Use after the last implementation change, before handoff/release/deployment, or whenever prior green tests may be stale.
---
# WebPi Verification Before Completion

Completion requires **fresh evidence after the final relevant change**.

## Verification ladder

Choose the smallest ladder that matches risk:

### Documentation / prompt / static config
- parse/lint or resource-loader smoke when applicable;
- exact diff review;
- brand/secret/path policy checks relevant to the content.

### Local code change
- focused regression test reproducing the changed behavior;
- nearest package/module suite;
- formatter/typecheck/static diagnostics as appropriate;
- final diff review.

### Shared runtime / auth / tool contract
- focused TDD;
- broader Server/tool-contract/CLI regression covering the changed surface;
- negative authorization/failure-path smoke where relevant.

### Production/deployment
- clean source revision and intended workspace state;
- build identity + exact hashes;
- rollback evidence;
- runtime readiness/doctor;
- loopback smoke;
- public-origin/auth/security smoke where applicable;
- source alignment after deployment.

## Do not count as proof

- HTTP 200 with an error/HTML body;
- a process that merely started;
- a generated file that the runtime never consumed;
- an old test run from before the final edit;
- a successful retry that hides an unresolved first outcome;
- source code inspection alone when behavior is runnable.

## Before saying "done"

1. Inspect current Git/workspace state.
2. Confirm no unrelated/secret/generated files slipped into the change.
3. Re-run the required fresh validation.
4. Inspect the final diff/change set.
5. State any remaining unverified assumption explicitly.

Report only verified facts:

```text
Changed:
Fresh validation:
Negative/failure-path evidence:
Remaining risk:
User action:
```
