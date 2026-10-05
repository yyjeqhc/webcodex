# First Run and device invitations

This first-round change improves Desktop onboarding using the existing
Environment, pairing and native service lifecycle. It does not change their
authorization, configuration format, credential issuance or ownership rules.

## Source inventory and gaps

At upstream `390b7bbe`, FirstRun already supplies an explicit Runner role,
protected pairing input, saved enrollment reuse and setup progress. Create/Join
support an empty initial Project; Environment add-project reuses the saved Runner.
The missing presentation is two ordinary primary/join paths, advanced alternatives,
and optional initial directory selection.

NativeEnvironment::invite already checks the saved local Server environment and
upgrade exclusion, and invokes the existing Server pairing authorization. It
issues a ten-minute single-use code. Desktop needs only a target-bound IPC and
explicit Add device dialog. The separately reviewed prerequisite [#915](https://github.com/yyjeqhc/webcodex/pull/915) adds a locked expected-ID check to that same implementation; Desktop uses it to reject changed targets before issuance. ProjectsPanel/RunnerDevices already show authorized
devices and their Projects with exact identities.

## Boundaries

Ordinary new Create means Server plus local Runner; ordinary Join means an
additional Runner. Empty Projects do not disable the Runner. Server-only,
viewer-only and temporary Quick Share remain advanced options. Saved environments
retain their roles. Tunnel setup continues through the existing Connection UI.

Invitations are transient responses, never Desktop preferences or state. User
input for the advertised Server URL is instruction text, not an authorization
target. No firewall, listener or service is changed. Core's invitation result
contains only a code: ten-minute validity is guidance, not an exact server expiry
or proof of redemption. Device connection is observed separately through the
existing authorized fleet view. Failed/uncertain requests are never retried
automatically.

Configuration inventory/backup, update UX, runtime packages and native installer
acceptance belong to later independently reviewed rounds. Automated UI/adapter
tests and source builds do not establish native installation, logout, reboot,
upgrade or rollback acceptance.

## Implemented UX

First Run foregrounds **Create main node** and **Join main node**. New ordinary
requests always enable Runner, with or without an initial canonical Project path.
Advanced retains Server-only, viewer-only and Quick Share. Reopened environments
keep the saved role, credentials and service scope. A non-secret seven-field
projection reads the existing Environment record or interrupted setup journal;
no new configuration authority or persistent UI intent is written.

Projects offers **Add device** even without observed devices. Explicit creation
returns a target-bound transient invitation; display, copy, expiry and context
changes never persist it. The advertised address can be corrected without issuing
another code. Core/Server authorization, operation admission and existing fleet
identities remain authoritative. No invitation failure changes runtime readiness
or service ownership.

## Verification — 2026-10-05

Host: Ubuntu 24.04.4 LTS, x86_64. All network fixtures below use loopback and
private temporary state. None install, stop, restart or deploy user services.

| Check | Actual result | Evidence boundary |
| --- | --- | --- |
| FirstRun, AddDevice, AddLocalProject, Workspace, locale, presentation and address Vitest suites | 234 tests passed in 7 files | Desktop behavior, saved/pending roles, optional projects, authorized identity routing, invitation privacy and failures |
| Desktop native invitation adapter | 7 tests passed | IPC input restrictions, matching/late environment IDs, Core authority, single admission, private transient response and unchanged configuration/services/readiness |
| Desktop operation completion | 6 tests passed | Invitations cannot reclaim or terminalize runtime startup on failure |
| Core invitation prerequisite | 4 tests passed | Expected ID checked under existing lock before credentials/HTTP; original CLI, TTL and Server authorization preserved |
| Desktop typecheck and Vite build | Passed | Frontend source only; build still reports its large-chunk advisory |
| Desktop dogfood build | Passed | Linux native compilation/linking only; reports existing dead-code warnings for AppState::new and DesktopCore configure_remote_setup/store_identity |
| Environment dogfood cargo check | Passed | Focused production library compilation |
| Desktop and Environment cargo fmt; git diff --check; form-control CSS contract | Passed | Static gates for touched surfaces |
| Chrome for Testing 153.0.8010.12 at 360px | 26 checks passed across all seven locales | Primary cards, long selected project path and masked invitation layouts without horizontal overflow; modal Tab focus trap and Escape focus restoration. Temporary browser fixture, not a native WebView or live pairing |

Commands use the Desktop working directory for Vitest/typecheck/build, and
`--profile dogfood --locked` for Cargo tests/builds. Rust test filters are
`state::environment_invitation::tests`, `operation_completion`, and
`native::tests::invitation` in their respective packages. Accidental direct frontend
invocations from the repository root lacked Desktop's jsdom/package configuration
and failed with `localStorage` undefined or a missing root package.json; the configured Desktop run above passed.
A direct rustfmt invocation without the crate edition was also corrected to the
package-aware Cargo formatting command. These were invocation errors, not fixes
or weakened product assertions.

## Remaining acceptance

No Windows/macOS native build or installation, Linux installer installation,
logout/reboot, actual service ownership/upgrade/rollback, additional-machine
pairing, or ChatGPT-to-Tunnel-to-Project read was exercised. The existing
[deployment validation matrix](../unified-deployment-validation.md) remains
pending. Core provides no exact invitation expiry instant or redeemed-device
association; the UI deliberately makes neither claim. Device naming continues
to use the existing identity contract. Configuration inventory/backup, update UX
and native acceptance remain later independently reviewed rounds.
