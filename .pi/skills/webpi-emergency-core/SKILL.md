---
name: webpi-emergency-core
description: Use when WebPi is acting as an emergency/fallback agent, especially when work may be handed off from the Ubuntu Pi agent. Enforces isolation, explicit handoff, scoped execution, and evidence-bound completion using WebPi-native controls.
---

# WebPi Emergency Core

Use this skill when WebPi is the fallback agent or when a task is explicitly handed off from another agent.

## Isolation boundary

- Treat WebPi and the Ubuntu Pi installation as independent agents.
- WebPi owns its Windows project state, `.webpi-state`, WebPi Workflow Sessions, Goals, Jobs, validation evidence, and project-local `.pi` resources.
- Ubuntu Pi owns `/home/piagent/.pi`, its session logs, package state, and `/home/piagent/workspaces`.
- Do not read, write, copy, synchronize, or mutate Ubuntu Pi state unless the user explicitly requests a handoff or forensic read.
- A handoff does not imply shared memory, shared approvals, shared package state, or shared execution authority.

## Explicit handoff

When the user explicitly hands off a task from Ubuntu Pi to WebPi:

1. Identify the exact task artifact or project path the user wants handed off.
2. Read only the minimum handoff material needed to reconstruct the task.
3. Re-establish WebPi's own Workflow Session and validation state; never reuse Ubuntu Pi's session authority or approvals.
4. Record unresolved assumptions and user-confirmation boundaries before consequential changes.
5. Do not write back to the Ubuntu Pi workspace unless the user explicitly requests a return handoff.

## WebPi-native control plane

Prefer WebPi's canonical controls over Pi extensions for:

- files and guarded edits;
- process and Job execution;
- Git operations;
- Goals, AgentTasks, and Workflow Sessions;
- project trust and extension fingerprint approval;
- validation, receipts, and change review.

Do not install a second owner for permissions, goals, memory, session replacement, provider/model routing, or terminal UI when WebPi already owns that responsibility.

## Completion contract

For non-trivial work, keep a compact contract with:

- goal and success criteria;
- exact project/scope;
- allowed mutations;
- destructive or external side-effect boundaries;
- required validation;
- any independent-review requirement;
- unresolved unknowns or required user confirmation.

A task is not complete merely because an agent says it is complete. Completion must be supported by current artifacts and validation evidence tied to the current file/version state.

## Independent review

When an independent reviewer is warranted, use a separate WebPi AgentTask/reviewer path that reads the current artifacts and produces a receipt. The primary agent must not impersonate an independent reviewer. Reviewer failure, uncertainty, stale artifacts, or budget exhaustion is not a pass.

## Pi package/extension policy

- Prefer passive skills and narrow tools that add capability without taking over the model loop.
- Skip or adapt packages that depend on Pi-owned provider/model hooks, turn/message hooks, TUI widgets, session switching, global goal ownership, global memory ownership, or broad automatic command interception.
- Install WebPi Pi resources project-locally by default so they remain independent of other Pi installations.
- For executable extensions: discover candidate → inspect source/side effects → observe exact candidateId + SHA-256 → approve exact fingerprint → reload → list → describe schema → call only after validation.
- Never inherit approvals from another agent installation.

## Rollback discipline

Every added WebPi package/extension should have a clear removal path. After removal, confirm resource inventory, extension list, trust/approval state where relevant, and Git/worktree residue.
