---
description: Read-only WebPi review across specification, correctness, security, recovery, and tests.
---
Review this WebPi change/task without editing: $@

Inspect the real diff plus the minimum surrounding implementation/tests needed to judge it.
Review two axes separately:

Specification:
- requested user outcome and acceptance criteria;
- compatibility and documented behavior;
- WebPi product naming/user guidance where applicable.

Quality:
- correctness and edge cases;
- auth/scope/project/path boundaries;
- observation vs control authority;
- secret/data leakage;
- stale identity, replay, TOCTOU, and outcome-unknown recovery;
- Plugin/Pi discovery vs approval/execution;
- test strength and missing failure-path coverage;
- maintainability/performance only when material.

Report only actionable findings, ordered by severity. Each finding needs evidence/location, failure condition, impact, and minimal remediation. Do not report style preference as a defect. If no material finding exists, state what was checked and what remains unverified.
