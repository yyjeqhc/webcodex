---
name: webpi-code-simplifier
description: Review and simplify the current WebPi code change without changing behavior. Use after feature/bugfix implementation when the diff is correct but may contain avoidable complexity, duplication, awkward naming, or unnecessary structure. Restrict edits to changed files/nearby changed logic and re-run focused validation.
---
# WebPi Code Simplifier

Use this skill **after correctness is established**, not as a substitute for debugging or feature design.

## Goal

Make the current change easier to read and maintain while preserving observable behavior, authority boundaries, compatibility, and test meaning.

## Scope

1. Inspect `git status` and the current diff first.
2. Focus on files/logic changed by the current task. Read nearby code only for context.
3. Do not turn a local simplification pass into an unrelated refactor.
4. Preserve public API, wire/data compatibility, security checks, error semantics, and user-visible behavior unless the task explicitly requires otherwise.
5. Preserve comments that explain non-obvious invariants; remove comments only when the code itself now makes the same fact obvious.

## What to look for

- duplicated branches or validation;
- nested control flow that can be made linear without hiding failure cases;
- unnecessary temporary state or abstraction;
- names that obscure the actual invariant;
- repeated constants/messages that should share one helper;
- error paths that re-run validation or can throw while constructing an error;
- defensive code that is now unreachable because a stronger validated boundary already exists;
- inconsistent WebPi product naming in new user-facing text.

Do **not** optimize for line count. A shorter implementation is worse if it hides authority, rollback, or failure semantics.

## WebPi-specific checks

Before changing a simplification candidate, ask whether it would weaken any of these:

- project/sensitive-path confinement;
- scope separation or observation-vs-control authority;
- stale identity / expected revision fences;
- `outcome_unknown` reconciliation;
- exact extension fingerprint approval;
- create-only / rollback-capable artifact semantics;
- clean build/source-alignment evidence.

If yes, keep the explicit structure.

## Workflow

```text
inspect diff
→ identify one simplification with a concrete readability/maintainability benefit
→ make the smallest edit
→ run the nearest focused test/diagnostic
→ continue only if the diff is still easier to reason about
→ run the relevant broader gate
→ inspect final diff again
```

Use LSP/definition/reference tools when a rename or helper extraction could affect callers. Prefer exact WebPi edits over broad rewrites.

## Output

Report:

- what was simplified and why it is clearer;
- behavior/compatibility invariants preserved;
- validation run;
- any complexity deliberately left in place because it represents a real safety boundary.
