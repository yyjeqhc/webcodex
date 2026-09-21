---
name: debug
description: >
  Use when diagnosing a failure — a failing or flaky test, a stack trace, a crash, a CI or
  lint error — "why is this failing", "find the bug", "debug this", "it crashes when",
  "works on my machine". Not for writing new features or fixing a bug whose cause is
  already known (build).
allowed-tools: read, grep, find, ls, bash
---

# Debug — Hypothesis Before Fix

Diagnose methodically. Each speculative edit moves the system from a known state to an
unknown one; the harder the bug, the stricter the loop.

## The loop
1. **Reproduce.** Make the failure happen on demand — ideally as a failing test. Can't
   reproduce → don't ship a speculative fix: add instrumentation to capture it next time,
   or ask for environment/input/steps, and say plainly that it isn't reproduced.
2. **Isolate.** Shrink to the smallest input that triggers it — binary-search the input;
   `git bisect` the history if it used to work.
3. **Hypothesize testably.** First read the error message word by word — it usually names
   the file, line, and cause. Then write 2–3 hypotheses in the form "the bug is at
   file:line because <observed evidence> implies <cause>"; test the cheapest first.
4. **Probe — in a workspace you own.** One smallest experiment per hypothesis: a log line,
   an assertion, a narrowed test. One change at a time. Probing edits code, so do it in a
   disposable copy, never the caller's checkout: `npx -p principal-pi-skills principal-pi-workspace create`
   prints a throwaway worktree holding their exact working state (minus anything git
   ignores). Work there, then `remove` it. That keeps "probes are temporary" structurally,
   which nobody manages by hand under a failing build.
5. **Prove the fix at the root cause — still in the workspace.** The stack trace points at
   the symptom; trace the wrong value upstream and fix there. The regression test fails
   before the fix and passes after; re-run the full suite and the original reproduction.
   Intermittent bug → loop the test (e.g. 100×) before declaring victory. For async or
   eventually-consistent behavior, wait on the observable condition with a deadline; a fixed
   sleep is not condition-based evidence and only moves the race.
   **In a workflow you report the fix and do not leave it behind** — `build` implements it
   once, in the caller's checkout, where they can watch it land. **Asked directly to fix it,
   fix it.** "Diagnose this and fix it" is a request, not a handoff; withholding a repair the
   user asked for because a later phase might apply it is the ceremony this framework
   refuses.
   No workspace available → return a read-only diagnosis that says so, with the fix marked
   unproven, or `BLOCKED` if nothing can be told apart without running code. Never run the
   experiment in the caller's tree instead.

## Error-handling rule
When the user says "make it not crash" / "stop it taking down the server", they are asking
you to make the failure **survivable and detectable** — never to suppress it. A fix where
the operation silently looks like it succeeded (order unmarked, null nobody checks) is a
worse bug than the crash. A silent `catch {}` / `return null` / `pass` trades a loud crash
for silent corruption. **Absence of success is not a failure signal**: leaving a record
unmarked so "the caller can tell" is a swallow — nobody can distinguish *failed* from *not
attempted yet*.

A caught error must do four things: **preserve the failure semantics** (the caller can
still tell it failed — a raised domain error, a checked result, a rejected promise);
**keep state consistent wherever state was changed** (a half-written record is marked
failed or rolled back); **log once, at the boundary that owns it** — handler, job runner,
entry point, not at every frame on the way up; and **sanitize the log** — no credentials,
tokens, PII, request/response bodies or raw provider errors; log the operation, the ids and
the error type.

Three shapes satisfy that and are routinely mistaken for swallows. **Pure or library code**
may return a typed error or checked result and log nothing — a parser returning
`Err(ParseError)` is complete, and a library that logs has stolen the caller's decision
about where output goes. **A transaction** may roll back and rethrow; that is the state
consistency and the semantics both. And **do not invent a status field where no durable
record exists** — if nothing durable was written there is nothing to mark, so raise or
return and stop.

Test the failure path too — a happy-path test cannot tell a fix from a swallow. A typical
catch at a boundary:
`catch (e) { log.error({ op, id, err: e.name }); markFailed(record); return { ok: false, error: e }; }`
— and the caller checks it. `catch {}`, `catch (e) { return null; }`, or an empty
`catch (e) { return; }` is the bug, not the fix.

## When stuck (~1 hour, or the user says "still failing")
Never repeat a suggestion that was already tried — each next step must produce NEW
information. For environment-specific failures (CI-only, prod-only, "works on my
machine"): capture artifacts from where it fails (logs, recordings, core state), reproduce
that environment locally, isolate the difference, or bisect. More sleeps and bigger
timeouts are not diagnostics. If genuinely out of moves, stop: re-read the original report
— are you debugging what was actually reported? — and hand off the note below filled in as
far as you verifiably got, ending with the one question that would branch the search.

## Right-sizing
An obvious one-line bug with an obvious cause doesn't need the full loop. But after two
speculative edits with no traction, you are in a hard bug: return to step 1.

## Delegated mode (running as a subagent)
Run the loop with the tools you have; return the note in one response, filled to wherever
you verifiably got. A confirmed root cause without a fix is a valid result — say so.

## Output — debugging note
```
## Bug: <one line>
Reproduction: <command or test that triggers it, or "NOT REPRODUCED: <why>">
Isolated to: <smallest input / commit range>
Hypotheses tested: <each → confirmed / rejected, with evidence>
Boundary evidence: <smallest input + system boundary where the bad value first appears>
Wait condition: <observable condition + deadline> | not applicable
Root cause: <file:line + why>
Fix: <the minimal change, at the cause not the symptom — proposed, not applied>
Regression test: <name; failed before fix, passes after>
Suite: <result verbatim>
Workspace: disposable | none (read-only diagnosis) — <path removed, or why none>
Blocked: <the ONE question that would unblock the diagnosis> | none
Next: build | plan | done | blocked
```

`Next:` is exactly one of those four bare words — the caller routes on it mechanically, so
`build (nontrivial)` matches nothing. **build** the fix needs implementing · **plan** it is
a design flaw · **done** nothing more is needed · **blocked** you need the answer in
`Blocked:` first. Never `blocked` alongside a confident root cause.

## Checks
| If you are about to… | Instead |
|---|---|
| Make a speculative edit "to see if it helps" | That's guess-and-check. Reproduce and hypothesize first. |
| Fix at the exact line the stack trace names | That's the symptom location; trace the bad value upstream. |
| Wrap the failing call in try/catch to make the error go away | You're hiding the bug. Diagnose first. |
| Declare an intermittent bug fixed after one green run | Loop the test before declaring victory. |
