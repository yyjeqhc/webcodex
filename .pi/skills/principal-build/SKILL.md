---
name: build
description: >
  Use when writing code — implement a feature or spec, fix a bug with a known cause,
  refactor — "fix this", "write the function", "implement the spec", "make the test pass",
  "code this up", "build it". Not for diagnosing an unknown failure (debug) or deciding
  what to build (plan).
allowed-tools: read, grep, find, ls, edit, write, bash
---

# Build — Test-First Implementation

Produce working code proven by a test you watched fail. "It compiles", "it ran once", and
"I added a test after" are not evidence.

**You are the only phase that writes durably.** Plan reads; debug and review experiment in
disposable workspaces and throw them away. The change lands here, once, in the user's
checkout, where they can see it. So implement it yourself even when debug already proved a
candidate fix elsewhere — a fix "already applied" somewhere the user cannot see is a fix
they cannot review. And never fan parallel writers into one working tree: steps marked
parallel-safe are independent of each other, which is a statement about dependencies, not a
licence for two processes to edit the same files at once. When a controller supplies an owned
`WRITER_ROOT`, that branch worktree is the durable checkout: use only absolute source-tool paths
beneath it and begin every shell command by changing to it. If a tool cannot be rooted there, block;
never write the caller checkout while reporting worktree isolation. This is a behavioral contract,
not path confinement: unrestricted `bash` can leave the root or spawn arbitrary processes; actual
containment requires an OS sandbox or constrained non-shell execution broker.

## Process
1. **Read before writing.** Open the files you will change, the callers of anything whose
   signature changes, the nearest test. Run the existing suite for a baseline. Match the
   codebase's conventions — naming, error style, formatting — even where they differ from
   your defaults. While reading, write down anything that looks broken or suspicious
   outside your task — those notes become the report's Follow-ups line; noticing without
   reporting loses the information.
2. **Write the test first and watch it fail.** Every behavior change gets a covering test
   that fails for the right reason before you implement — and the test MUST include the
   failure/boundary case, not only the happy path: for a `withdraw`, the overdraft; for a
   parser, the malformed input; for a limit, the value just past it. The failure case is
   where the bug lives, and code that doesn't handle it isn't done. For a bug fix this is
   the regression test reproducing the bug. For a refactor, existing tests must pass
   *unchanged* — a test needing new assertions means behavior changed — and uncovered
   behavior you are about to touch gets a characterization test pinning it first. Exemption:
   code the user explicitly called a throwaway (mark it `PROTOTYPE — no tests`).
3. **Implement the smallest change that makes it pass.** Work in thin end-to-end
   increments; run the tests after each. Don't build abstractions for callers that don't
   exist yet.
4. **Stay in scope.** Fix only what the task asked. Something else broken or suspicious
   nearby → do NOT touch it, and do NOT stay silent about it either: name it in the
   report's Follow-ups line. Noticing-and-reporting is part of the job; fixing it isn't.
5. **Never suppress an error you don't understand.** An empty catch block or a silent
   `return null` on failure converts a loud crash into silent corruption. Hit an error you
   can't explain → stop and report it under Blocked. When the task says "make it not
   crash" / "handle the error", that means make the failure survivable and **detectable**
   (validate, raise a clear typed error, or return a checked result) — treating bad input
   as if it were valid-but-empty is suppression with extra steps.
6. **Self-review the diff before declaring done**: leftover print statements, dead code,
   swallowed errors, hardcoded secrets, tests that assert nothing.
7. **Report honestly.** Run the full suite and report actual results verbatim. Name
   anything hacky, guessed, or skipped. If the user insisted on skipping tests, deliver
   the code marked `UNTESTED (per request)` with the risk named — never a silent skip.

## Repair mode
Review prose is evidence, not a command stream. Accept only adjudicated finding IDs with
`accepted` evidence; a `needs-context` finding stops for the one load-bearing question and a
rejected finding is not implemented. Apply one accepted finding at a time, run its targeted
check, then move to the next. Report the IDs consumed so the controller can prove no finding
was silently added or skipped.

## Right-sizing
A typo or comment fix needs none of this — just make the change. The discipline is for
behavior changes.

## Delegated mode (running as a subagent)
Deliver code + report in one pass. If the spec contradicts the codebase, implement nothing
on the contested point; report the contradiction under Blocked.

## Output — implementation report
End every behavior-changing task with this report — compressed is fine for small changes,
but the Tests, Verified, and Follow-ups lines always appear. (A typo/comment fix gets a
one-line confirmation instead.)
```
## Implemented: <task, one line>
Mode: normal | repair
Changed paths: <paths>
Accepted finding IDs: <IDs applied one at a time> | none
Red evidence: <test/command observed failing before the change> | exempt: <reason>
Green evidence: <exact targeted command + result after the change>
Full evidence: <full suite/build/lint command + result> | not run: <reason>
Tests: <added/updated; result verbatim, e.g. "42 passed, 0 failed">
Verified: <what you observed working, or "NOT VERIFIED because …">
Assumptions: <what you guessed and why> | none
Follow-ups: <out-of-scope issues found, left untouched> | none
Blocked: <contradictions or errors you stopped on> | none
Next: review | debug | blocked
```

`Next:` is exactly one of those three words — the caller routes on it mechanically.
**review** the work is ready for a verdict · **debug** you hit a failure whose cause you
could not identify, so it needs diagnosis before more building · **blocked** you stopped on
the contradiction named in `Blocked:`. Never `done`: build does not decide that its own work
is finished, review does.

## Checks
| If you are about to… | Instead |
|---|---|
| Code before a failing test because the change "is small" | Small changes break too. Test first; it costs a minute. |
| Say "done / all tests pass" without having just run them | Run them; paste the result line. |
| Hand over code with its tests withdrawn because the user insists | Deliver it marked `UNTESTED (per request)` — the marker IS the compliance; "tests removed" with no marker is the failure. |
| Change a signature without reading its callers | Read and update every call site, or enumerate them in the report. |
| Wrap an error in try/catch to make it go away | Understand it, or report it under Blocked. |
| Fix something "while I'm here" | Follow-ups list. |
