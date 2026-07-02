# WebCodex Desktop Sessions

[English](DESKTOP_SESSIONS.md) | [简体中文](DESKTOP_SESSIONS.zh-CN.md)

This document describes a future WebCodex direction. It is a strategic design note, not a release checklist and not a claim that these capabilities are implemented today.

## Product goal

WebCodex Desktop Sessions should not mean unrestricted remote desktop control. The goal is a controlled, auditable, replayable desktop engineering session:

```text
observe -> decide/propose -> authorize -> act -> verify -> record -> replay/report
```

This extends WebCodex from an AI coding runtime toward an AI engineering workstation runtime. The runtime should be able to connect code repositories, artifacts, commands, Git diffs, desktop observations, screenshots, and final reports under one task session.

## Non-goals

Desktop Sessions are not intended to be:

- a generic RPA platform;
- unrestricted remote desktop control;
- a public computer-use bot;
- a consumer screen-sharing tool;
- a replacement for existing project, Git, shell, MCP, or GPT Action tools;
- an excuse to give an agent unrestricted access to a user's primary desktop.

Computer use should be a visual/desktop execution backend for WebCodex, not the entire product.

## Why desktop sessions matter

MCP and ordinary runtime tools are valuable, but many engineering tasks still have no stable API, CLI, or MCP surface:

```text
Windows installer testing
GUI application smoke tests
browser workflows that depend on login state
IDE/editor-assisted debugging
OBS or web build platform operation
Electron/Qt application testing
remote desktop distribution testing
game or AI game debugging
control panels, drivers, settings, and setup wizards
```

The differentiator should not be that WebCodex can click coordinates. The differentiator should be that every visual action has a boundary, an intent, a result, and an evidence trail.

## Session lifecycle

A Desktop Session should be modeled as a WebCodex task session with visual execution events:

1. **Observe:** capture screen/window state, window list, process metadata, and relevant artifacts.
2. **Decide/propose:** describe the intended action in terms of target window, UI element, and expected result.
3. **Authorize:** require policy or user approval for input, destructive, submission, payment, or privacy-sensitive actions.
4. **Act:** perform a bounded desktop action such as focusing a window, setting the clipboard, pressing a key, or clicking a window-relative coordinate.
5. **Verify:** capture after-state and compare it to the expected result.
6. **Record:** store action metadata, screenshots, logs, artifacts, and policy decisions in the session timeline.
7. **Replay/report:** produce a reviewable report with evidence, not just a textual claim that the task succeeded.

This mirrors the existing WebCodex coding loop: inspect, edit, diff, validate, record, and report.

## Permission model

Desktop permissions should be separate from shell/job permissions. A useful model has two axes.

Desktop capability levels:

```text
L0 artifact transfer
L1 screenshot, window list, process list, screen metadata
L2 clipboard get/set
L3 keyboard input and hotkeys
L4 mouse click, drag, scroll
L5 autonomous visual loop
```

Execution capability remains separate:

```text
shell disabled
diagnostic-only shell
bounded shell
privileged shell prohibited by default
```

This separation matters because GUI actions and shell commands have different risk shapes. A click can submit a form, send a message, delete data, or expose private information even when no shell command is run.

## Artifact bus

Artifact flow should be a first-class runtime capability, not a desktop-only feature:

```text
ChatGPT upload
  -> WebCodex artifact
  -> agent workspace / desktop session
  -> generated logs, screenshots, builds, reports
  -> WebCodex artifact
  -> user download
```

This supports both coding and desktop scenarios, and it is the main bridge from "coding agent" to "engineering workstation" tasks. Upload/download allows the user to provide data, images, documents, installers, configuration files, screenshots, or logs; the agent can then analyze, transform, test, or package them; and the runtime can return generated reports, plots, repaired files, build outputs, logs, and visual evidence.

Common flows include:

```text
upload experiment CSVs or result archives -> analyze data -> download plots and report
upload screenshots or test images -> inspect visual evidence -> download annotated output
run a local web UI -> capture screenshots -> adjust layout -> save before/after evidence
upload installer or sample files -> run smoke test -> download logs and screenshots
upload documents or configs -> transform or repair -> download the corrected artifact
```

This is where WebCodex can exceed ordinary coding agents. A coding agent can usually edit files and run tests, but it often cannot move real task inputs and outputs through a session, inspect generated images, or attach screenshot evidence to a final report.

Initial artifact categories should include:

```text
upload installers
upload test images
upload Excel/PDF/docx files
upload configuration files
download logs
download screenshots
download build outputs
download test reports
download repaired files
download before/after UI evidence
```

Artifacts should eventually carry stable metadata such as id, type, source, session id, project id, creator, SHA-256, size, retention policy, preview support, and download routing.

## Evidence and replay

Screenshots should be evidence, not just model input. A critical desktop event should be able to produce records shaped like:

```text
before.png
action.json
after.png
observation.md
```

A conceptual action record could look like:

```json
{
  "action": "click",
  "target_window": "Chrome",
  "coordinate_space": "window",
  "x": 812,
  "y": 436,
  "intent": "Click the Login button",
  "timestamp": "..."
}
```

This is a proposed record shape, not a committed API. The invariant is more important than the schema: the user should be able to answer what the agent saw, what it intended to do, what input it sent, what changed, and whether the result was verified.

## Windows MVP

Windows is a good first desktop provider because many installer, GUI application, enterprise software, and game testing workflows depend on it. The architecture should not be Windows-only, but the first implementation can focus there.

Initial capabilities should be conservative:

```text
screenshot
window_list
focus_window
mouse_click, mouse_drag, scroll
keyboard_type, key_combo
clipboard_get, clipboard_set
artifact upload/download
screen_trace, action_trace
```

`window_list`, `focus_window`, and window-relative coordinates are important. Full-screen absolute coordinates are too brittle when resolution, DPI, window placement, or layout changes.

The provider abstraction should stay open to future implementations:

```text
windows-desktop-provider
linux-x11-provider
linux-wayland-limited-provider
macos-provider
browser-provider
vnc-provider
rdp-provider
```

## Safety policy

Desktop Sessions must be designed as high-risk capabilities from the start. Default policy should favor observation and explicit authorization:

- default to screenshot/window observation only;
- require approval before keyboard, clipboard, or mouse input;
- never auto-type passwords;
- require confirmation for payment, send, delete, publish, submit, install, or system-setting actions;
- support sensitive window and process deny lists;
- support masked screenshot regions and retention limits;
- block sensitive paths from artifact upload by default;
- provide an emergency stop path;
- keep before/after evidence for critical actions;
- recommend virtual machines, test accounts, temporary desktops, and dedicated OS users.

A Desktop Session should not default to controlling a user's primary personal desktop.

## Roadmap

A practical roadmap should keep risk low early:

1. **Artifact bus and observation:** uploads/downloads, screenshots, window list, process list, screen metadata.
2. **Human-approved input actions:** focus, clipboard, keyboard, hotkeys, click, drag, scroll under session policy.
3. **Evidence and replay:** before/after screenshots, action trace, session timeline, downloadable report.
4. **Short GUI workflows:** open a page, upload a file, click build, download result, capture evidence.
5. **Vertical engineering workflows:** Windows GUI smoke tests, installer validation, OBS/web build triage, desktop app packaging validation, browser release workflows, game/AI game testing.

## Relationship to existing WebCodex sessions

Desktop Sessions should reuse the existing WebCodex ideas instead of creating a separate automation product:

- project ids and agent identity define where work happens;
- session ledgers record desktop events alongside file, Git, shell, and artifact events;
- task guards and risk metadata define what the agent may do;
- artifacts carry screenshots, logs, installers, reports, and generated files;
- `show_changes` and final reports can include both code diff evidence and desktop verification evidence.

The long-term position is: WebCodex should not be an AI mouse controller. It should be an engineering runtime where desktop execution is authorized, scoped, auditable, and replayable.
