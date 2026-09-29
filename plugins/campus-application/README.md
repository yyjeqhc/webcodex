# campus-application Native Tool Plugin

`campus-application` is a first-party WebCodex Native Tool Plugin for planning safe resume-form filling with WebCodex Browser Use. It turns bounded Browser semantic snapshots into structured fill plans, supports repeated and dynamically-added resume sections, and can advance explicit multi-step application flows. It never submits an application.

The Plugin is deliberately split from Browser execution:

- Browser observation supplies semantic controls and opaque element identities.
- `campus-application` maps those controls to a structured resume profile and proposes bounded actions.
- the caller executes ordinary Browser actions and takes a fresh snapshot after every structural or page-step mutation.

For a built-in Browser `plan_fill` result, send up to 32 same-page field actions
through `browser_act(action="batch")`, using the opaque elements from the same
snapshot. Batch preserves order and stops on stale authority, rejection or
uncertainty; inspect its completion counts and follow observation recovery rather
than replaying effects. Take a fresh verification snapshot after filling. Keep
section-expansion and page-step mutations separate. The plugin remains a planner;
final submission remains `ready_for_review` and is never automatic.
- final submit controls become a `ready_for_review` boundary with no click action.

## Install, build, and test

```bash
npm ci
npm run typecheck
npm test
```

Node.js 18 or newer is required. Repository CI uses the local `../../npm/plugin-sdk` checkout so this Plugin continuously tests the current WebCodex SDK.

## Configure a resume profile

The repository contains only fictional example data. Copy `profile.example.json` to a private Runner-local directory as `profile.json` and edit that copy, for example:

```text
~/.config/webcodex/campus-application/profile.json
```

`profile.json` is intentionally ignored by Git.

By default the Plugin reads `profile.json` from the provider's configured `cwd`. An explicit `WEBCODEX_CAMPUS_APPLICATION_PROFILE` environment variable may instead point to another profile file.

The structured profile covers identity/contact data, links, repeated education/experience/projects/campus experience, skills/languages, job preferences, application text, and attachments.

`attachments.resume_path` is passed to the later Browser `upload_file` action. It must be valid relative to the WebCodex project that the caller authorizes for upload; it is not resolved relative to the Plugin profile directory.

## Configure one Runner

Build the Plugin, create a private data directory containing `profile.json`, then add a provider entry to the Runner's startup-bound `runner.toml`:

```toml
[[plugins.providers]]
id = "campus-application"
name = "Campus Application"
command = "node"
args = ["/absolute/path/to/webcodex/plugins/campus-application/dist/plugin.js"]
cwd = "/absolute/path/to/private/campus-application-data"
timeout_secs = 30
```

After restarting or reloading the Runner:

```text
webcodex plugin check --runner <runner> --plugin campus-application
webcodex plugin reload --runner <runner>
webcodex plugin describe --runner <runner> --plugin campus-application --tool plan_fill
```

Invocation remains on the canonical `plugin_tool describe -> call` path.

## Tools

`profile_get` returns the configured structured resume resource plus the bounded canonical fill view.

`analyze_form` consumes a bounded Browser semantic snapshot and reports mapped controls, indexed resume paths, blockers, site classification, and the stable form-structure signature.

`plan_fill` returns the next safe planning phase:

- `expand_sections`: exactly one add-section click, then a mandatory fresh snapshot;
- `fill_fields`: ordinary `input_text`, `select_option`, `set_value`, `upload_file`, or grouped `click` actions;
- `advance_step`: exactly one next/continue click when explicit step/progress context is visible, then a mandatory fresh snapshot;
- `ready_for_review`: a final submission control is present; no submit click is returned.

Repeated controls map to exact source paths such as `education[1].school`, `experience[1].start_date`, or `projects[1].technologies`.

The Plugin keeps a bounded 64-entry in-process mapping cache keyed by a 24-hex-character structure signature. The signature excludes volatile element/group identities, current values, checked state, passive AX text, submit buttons, and native picker affordances. Cache state is discarded when the Plugin process reloads.

## Local fixtures

The repository includes deterministic local-only forms covering Greenhouse-like and Lever-like single-page forms, a Chinese campus form, repeated sections, dynamically-added sections, and a four-step wizard.

Run them with:

```bash
npm run fixtures
```

Then open `http://127.0.0.1:32124/`. Fixture submit events are intercepted locally; no real application is sent.

## Safety boundary

This Plugin is a planner, not an autonomous submission service. It does not call Browser tools itself, access external recruiting APIs, or submit forms. Browser authority stays with WebCodex and the caller, and final submission remains a review boundary.
