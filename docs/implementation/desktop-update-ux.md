# Desktop unified update workflow

Desktop consumes the shared `webcodex-environment::unified_update::UpdateView` through the read-only native `get_local_update_status` command. The wrapper adds local process observations, the last explicit file-inspection timestamp, and an exact installation confirmation. The complete native response is limited to 64 KiB, including the adapter fields.

## Identity and local scope

About presents Desktop, CLI, Server and Runner separately. Each component has installed-file, running-process and verified-candidate columns. Polling does not probe executable files. **Inspect installed files** runs the existing bounded build-info and before/after byte checks against the selected Runtime source and the Desktop executable path. Valid file identities survive protocol differences; missing, unreadable or changed bytes remain unknown. The retained file column is explicitly labelled as the last requested inspection, with its timestamp.

Running Desktop identity comes from the actual Desktop build. Running Server identity is queried only for the matching saved Create role and local authenticated Server URL. Runner observations use only the matching saved local Runner ID. CLI has no inferred long-running process identity. Network failures, absent identity and unowned native services remain unknown. Remote service roles are shown outside the local service scope. Server/Runner version strings require canonical plain SemVer; arbitrary suffixes and non-hex commit strings are discarded.

The confirmation lists only fixed Server, Runner and Tunnel categories with the actual saved native User/System service scope. Native service inspection must establish ownership and a known process state. Tunnel profile inspection is bounded to sixteen profiles; an incomplete inventory cannot advertise installation readiness. Service names, definitions, arguments, paths, profile identifiers and private diagnostics do not cross this projection. Core rechecks actual scope and ownership before any effect.

Source, dirty, custom Runtime and unmanaged installations retain manual replacement. Readiness requires both the shared verified download flag and a current managed native installation context. A read-only projection cannot grant installation authority to a source/custom installation.

## Recorded stages and explicit actions

The UI distinguishes metadata checking, download, verification, readiness for confirmation, active-task blockers, recorded preparation, shutdown, snapshot readiness, installer handoff, verification, completion, restoration and manual recovery. Saved phases describe records, not proof of a currently active executor. An installer handoff takes precedence over an older snapshot-ready record and does not imply completion or continuing installer activity.

Core terminal records remain visible after release/download cache clearing. A restart notice additionally requires explicitly inspected installed Desktop metadata matching the committed version/source, plus an older observed Desktop process. Unknown installed files alone cannot establish that notice. A prior terminal record does not block discovery/download of a newer release. Repeating a restored candidate first requires an explicit successful local-status review. Neither polling nor task completion retries download or invokes installation.

The confirmation binds the candidate version, source manifest, installer checksum, target Environment, selection revision and operation. A fresh prepare following a terminal record carries no old operation ID. Stale same-version confirmations are disabled in the UI and rejected again in native/Core paths. Core performs its existing candidate, Environment, owner and operation checks under the effect lock. Installation and Desktop exit still require explicit confirmation.

Keyboard navigation stays in the confirmation; Escape and dismissal return focus to its opening control. Full revisions wrap in narrow component rows. New scope/stage/inspection messages and the existing update actions/errors are translated across English, Simplified Chinese, Traditional Chinese, German, French, Japanese and Korean.

## Validation and native boundary

Run from the repository root:

```sh
npm run build --prefix apps/desktop
npm test --prefix apps/desktop
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --profile dogfood --lib state::updates::view::tests
cargo fmt --check --manifest-path apps/desktop/src-tauri/Cargo.toml
git diff --check
```

For the focused presentation gate, run from `apps/desktop`:

```sh
npx vitest run src/features/settings/UpdateWorkflow.test.tsx
```

Recorded source validation on 2026-10-06 with shared prerequisite `fbe7100c`: the final Desktop suite passed 492 tests across 19 files and its form-control CSS contract; the focused workflow suite passed 74 tests; the native adapter suite passed 12 tests. Typechecking, the Vite production build, Rust formatting and diff-whitespace checks passed. The native adapter rejects saved Environment record changes as well as Desktop selection changes before returning a confirmation.

After integration with the guarded native handoff repair and shared platform fix `40d927db`, the native state adapter again passed 12 tests and the native installer adapter passed 4 tests. The final Desktop Rust check passed with two existing dead-code warnings in unchanged setup helpers. The frontend source was unchanged by that integration. Desktop and the separate Linux CLI both depend on the exact prepared-operation receipt repair.

The focused frontend suite covers all nine retained Core phases, cache clearing, unknown handoff/restoration executors, exact-target invalidation, active-task refresh without effects, review before repeating a restored candidate, late observations after Environment changes, separate component identities, service scopes, inspection timestamps, keyboard behavior and all seven confirmation languages. Native adapter tests cover identity canaries, Environment/revision/manifest fences, terminal-operation handling, byte-change rejection, owned-service scope projection, manual-installation eligibility and the full-response bound.

These checks establish source and adapter behavior. The Vite production build is a frontend artifact build, not a signed Desktop package. It retains the existing large-chunk advisory. Tests do not establish real OS authorization, package installation, service restart, logout/reboot, native handoff, upgrade or rollback acceptance. This work does not deploy packages or control existing host services.

## Follow-up response ordering and integration

Revalidation against `upstream/main` `6713748b` reproduced a stale response:
a newer explicit installed-file inspection completed before an older inspection,
which then replaced its version, timestamp and Desktop restart requirement. A
deferred-response regression failed on the original implementation. An independent
inspection sequence now rejects older inspections while preserving the current
inspection when ordinary status polling finishes first; unmount invalidates it.

The updated workflow suite passed 77 tests; five related workflow, First Run,
Add device and workspace suites passed 276 tests total. Type checking, frontend
production build and the CSS contract passed. Integrated native update adapters
passed 24 focused tests and Desktop path-error projection passed two. The existing
Vite large-chunk advisory remains. The shared cross-process pending-state repair
and Windows invitation fixture correction are included through their prerequisite
branches. These are Linux source/fixture checks, not native installed-system or
OS reboot acceptance.
