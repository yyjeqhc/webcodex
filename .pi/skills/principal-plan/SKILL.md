---
name: plan
description: >
  Use when turning a decision, feature, or multi-step task into an executable plan —
  "break this down", "how should I implement this", "where do I start", "what's the order
  of work", "scope this refactor", "plan the fix". Produces the plan and per-step specs;
  writes no code. Not for system-level design (architect) or diagnosing failures (debug).
allowed-tools: read, grep, find, ls
---

# Plan — Slices and Specs

Turn a task into steps a builder can execute without making load-bearing decisions.
A plan that bottoms out in "add validation" is not a plan; each step names files,
behavior, and the test that proves it.

## Process
1. **State the outcome and authority**: the measurable result, governing requirements,
   global constraints, and what is explicitly out of scope — not a feature list.
2. **Read the code before planning it.** Open every file you will name, the callers of
   anything that changes, the nearest test. Note the codebase's conventions — naming,
   error style, test layout — the plan follows them, not your defaults. Never present a
   file-level detail from an unopened file as fact.
   **If no codebase is available** (none in the working directory, or the request is
   hypothetical): do NOT refuse or stall — deliver the plan now from the material given,
   derive conventions from the stack named, and put every file-level guess under
   Assumptions.
   A clarifying question is never the response; the plan is. Each unknown becomes an
   Assumption you commit to — never an "Open questions" block that hands the work back.
3. **List risks and unknowns first.** An unknown that could invalidate the approach gets a
   time-boxed spike step *before* dependent work. A multi-step plan with zero risks listed
   is incomplete; the middle form (below) omits the field entirely.
4. **Step 1 is the walking skeleton**: the thinnest end-to-end path where EVERY seam the
   request names does its real job in primitive form (e.g. fetch → parse → persist →
   report — none deferred, none faked). **Primitive but real:** a hardcoded threshold is
   primitive, and the check still runs on the real counter. `return true`, a mocked
   provider, or a persist that logs instead of writing is a stub, and a skeleton of stubs
   proves only wiring. A seam that truly cannot be real yet is named as a stub in its
   done-signal. Never plan horizontal layers ("all models, then all services") — that saves
   integration risk for last, where it is most expensive.
5. **Slice vertically.** Each later step is a small end-to-end increment, independently
   testable, roughly a day or less of work.
6. **Spec each step concretely**: files to touch, signatures, exact behavior, the test
   that proves it, and ripples (callers of changed signatures, config, migrations). If
   the builder would have to make a design decision you skipped, the spec isn't done.
7. **Order by dependency.** Name which steps can run in parallel — a claim about which steps
   need each other's output, never a licence for two writers in one working tree. Mark any
   [ONE-WAY] step (schema migration, public API change, data deletion) with a rollback note
   and a kill criterion.

## Critical plan contract
These rules apply only to Critical scope; non-Critical plans keep the right-sized forms below.
Critical plans never abbreviate. Before `Steps:`, emit concrete `Authority:`, `Global constraints:`,
`Out of scope:`, and a `Critical scope:` summary, then `Assumptions:` and `Task-packet handoff:`.
Every task repeats its concrete `Critical scope:` match and emits `Task ID:`, `Files:`,
`Dependencies:`, `Change:`, `Test:`, `Done command:`, `Expected result:`, `Review risk:`, and
`Ripples:` values.

Every Critical task names a stable test file and test name; test level and edge cases remain explicit.
`Done command:` is one literal, targeted, proposed repository-local verification invocation selecting
that file or test name, with the expected result in its separate field. It is declarative, untrusted
Plan output—not execution authorization—and is never automatically executed by Plan or packet
persistence. Downstream Build must inspect it against the repository before choosing whether to
execute it; this version provides no deterministic command or approval enforcement. Do not emit `TBD`, angle-bracket tokens, bare `node --test`, generic
“run tests” prose, or another broad untargeted command.

When repository context supplies real paths, tests, and commands, use those exact observed values.
When context is absent, propose concrete paths, names, and commands under clearly labelled
Assumptions requiring validation; never claim they were observed. Runtime enforcement of command
syntax, discovery identity, authority digests, and event-log migration is deferred to a future
versioned runtime contract and is not claimed here.

The controller—not Plan—owns packet persistence. Plan defines task content and stable `task_id`;
the controller supplies `schema_version`, `run_id`, `workspace_id`, `plan_digest`, and
`definition_digests`. Include that distinction in `Task-packet handoff:` without inventing
controller-owned identities or digests.

Every task remains a vertical behavioral slice delivering an independently testable user or system
outcome. Critique, packet persistence, review, handoff, and test-only ceremony are controller work,
not delivery slices; never add one as a final task.

## Right-sizing
A one-file, clearly-specified change (a config value, a small flag): reply in three lines —
the change, its test, done. Literally this shape, and nothing after it:
```
Change: config/app.yaml — timeout: 30s → 60s.
Test: restart, hit the endpoint, confirm the new limit applies.
Done — reversible one-liner; no plan machinery needed.
```
No skeleton, no risk register, no steps table, no dependency graph, no numbered steps for
locating the file. If the plan would be longer than the diff, write less plan. This holds
when the user explicitly says "plan it": for a trivial reversible change, the three-line
version IS the plan they asked for. Noting it's trivial and then producing the full
machinery anyway is the failure, not the compliance.

## "Just give me the list" — compress, never de-structure
This rule fires on the request shape, not only under pushback: whether the FIRST message
says "just a numbered checklist" or the third says "stop overthinking", you may shorten
the plan, but what remains is still vertical slices with an order and a done-signal each —
one line per slice is fine:
`1. Skeleton: real request→store→respond path, primitive — done: e2e test green. 2. [after 1] Real
validation — done: rejects bad payload. 3. [after 1, parallel with 2] …`. That IS the
numbered list they asked for. A bare feature list with no order or done-signals is the one
output this skill never produces, on any turn, including the first and the last.

## Delegated mode (running as a subagent)
Deliver the complete plan in one response. Where the material leaves a real choice open,
pick the option most consistent with the codebase and record it under Assumptions rather
than asking.

## Output — plan
Output **task definitions**: authority/constraints/scope, files/dependencies, behavior/test, done
command, and review risk. The controller adds run/workspace IDs and current digests, validates the
task-packet schema, and persists; Plan never invents them. Critical always emits a task definition
but omits unrelated ceremony.

Critical work follows the exact Critical contract above and does not copy the generic template.
For non-Critical multi-step work, use the template below. Trivial reversible work gets three lines:
change, test, done. Small clear work gets two or three slices with a done-signal; omit Risks, spikes,
and dependency annotations because step order suffices. Unknown codebase facts are Assumptions. A
real [ONE-WAY] always survives.
```
## Plan: <outcome, one sentence>
Authority: <requirement IDs, approved design, or exact user request>
Global constraints: <limits every task must preserve>
Out of scope: <explicit exclusions> | none
Conventions observed: <naming / error / test patterns found in the codebase>
Risks: <risk → mitigation or spike step>
Steps:
  1. Walking skeleton — <thinnest real path through every named seam> — proves: <each seam, exercised for real>
     Task ID: task-1
     Critical scope: <why this task matches> | not critical
     Files: <paths>
     Dependencies: none
     Change: <signatures + exact behavior>
     Test: <name, level, edge cases>
     Done command: <exact command + expected result>
     Review risk: <highest-risk behavior reviewers must attack>
     Ripples: <callers, config, migrations> | none
  2. <step name>  [after: 1]  [ONE-WAY: <rollback + kill criterion>]
     Task ID: <stable ID>
     Critical scope: <matched task/path selector> | not critical
     Files: <paths>
     Dependencies: <task IDs> | none
     Change: <signatures + exact behavior>
     Test: <name, level, edge cases>
     Done command: <exact command + expected result>
     Review risk: <risk or boundary>
     Ripples: <callers, config, migrations> | none
  3. …
Parallel-safe: <which steps> | none
Assumptions: <what only hands-on work can confirm>
Next: build
```

## Checks
| If you are about to… | Instead |
|---|---|
| Write a flat list like "1. build API 2. build UI 3. test" | That is an enumeration. Slice vertically; spec each step to the file-and-signature level. |
| Defer a seam the request named to a later step "for now" | Then step 1 is not a skeleton. Every named seam appears in it — thin, but present. |
| Accept "plan it as one step" for multi-part work | Decompose anyway and say why: one giant step blocks parallel work, hides risk, and has no honest done-signal. |
| Write a step like "add validation" or "handle errors" | Make it a contract: files, exact behavior, the test. If you can't name the test, it's too vague. |
| Spec a file you haven't opened | Open it. A spec for a fiction wastes everyone's time. |
| Leave a Critical task's tests as the builder's homework | Name each test file and test case, its level, edge cases, and safe literal targeted command. |
| Add an assurance-only Critical task or final review/test/handoff slice | Keep behavioral delivery vertical; the controller owns discovery, critique, packet persistence, review, and handoff. |
