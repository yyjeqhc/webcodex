# Rust build selection and artifact observation

[简体中文](rust-build-selection-artifacts.zh-CN.md)

This additive [#1273](https://github.com/yyjeqhc/webcodex/issues/1273) stage of
[#962](https://github.com/yyjeqhc/webcodex/issues/962) extends `project_build`.
It does not add cross-compilation, caller argv, output-path selection or tool
installation. Existing requests keep their command and behavior.

## Request and execution

`profile` is an optional Cargo profile name (1–64 ASCII letters, digits,
underscores or hyphens, not beginning with a hyphen), including `dev`, `release`
and project-defined profiles such as `dogfood`. Cargo reports missing profiles.
`targets` optionally selects `library: true` and/or `binaries: ["server"]`
(at most eight bounded names); binary names are sorted/deduplicated for canonical
argv. Omit targets to keep Cargo defaults. Empty selectors are invalid.

`artifact_policy: "observe"` explicitly requests bounded metadata for selected
Rust library/binary outputs. These three fields support Rust and auto-to-Rust;
other recipes return `build_options_unsupported`. They require the additive
`project_build_rust_options_v1` capability before planning and at Job admission.

The Runner translates intent to fixed `cargo build --profile NAME --lib --bin
NAME` arguments, composing existing package/locked/offline policies. Requested
options participate in canonical invocation/provenance and queue/pre-spawn
replanning. Rustup auto-installation is disabled on the new execution path.

## Observation and limits

Observation uses offline `cargo metadata --no-deps --format-version 1`, then the
planned build with `--message-format=json-render-diagnostics`, within the original
durable Job and one shared timeout/cancellation budget. Both processes use the
existing prepared environment and owned process-tree/pipe lifecycle. Metadata
and build output obey existing stream byte limits; truncation cannot qualify.

Metadata selects registered-Project packages, explicit package scope or Cargo
default workspace members, applying admitted workspace exclusions even when an excluded
member builds as a dependency. Compiler output is accepted only for selected package
identities and library/binary targets, followed by a successful `build-finished`.
The Runner rechecks Project ownership and directory identity and uses the existing
artifact inspection primitives for Project-relative paths, byte counts, SHA-256 and MIME. A private exact Cargo-output reader accepts generated `target/` files; generic artifact tools and non-Cargo build readers retain their bulk-exclusion policy, and secrets remain forbidden. The
existing 16-file/256-MiB aggregate contract remains authoritative, including any
stricter reader bound. Nothing reads an output outside the registered Project;
an external Cargo target directory cannot qualify for this observation policy.

Cached artifacts are observations of existing files, not proof this invocation
created them, they are correct, or their source is current. Required-target,
missing-file, malformed/incomplete-output, redirect, resource or read failures
produce no partial success list. A successful native build can therefore have
an explicit artifact-observation failure while retaining native exit zero.
Cargo may legitimately update its lockfile; that is not mistaken for a stale
pre-execution plan during post-build output reads.

Pending and recovery retain the same Job and canonical artifact metadata. Lost
or unknown outcomes never authorize another build. Artifact observation neither
changes build effects/permissions nor provides a filesystem/network sandbox.

## Verification

Focused Core, Tool Contracts, Runner, Registry and Server tests cover default
omission, closed/bounded selectors, provenance, old Runner refusal, output
completeness, Project ownership, sparse results and same-Job handoff. The explicit
`runner-real-process-tests` lane includes
`runner_real_process_project_build_rust_options_observe_selected_cached_artifacts`:
a library and chosen binary under `dogfood`, an intentionally broken unselected
binary, independently verified bytes/digests, cached reruns, workspace exclusions
and missing-profile failure. The same lane includes
`runner_real_process_structured_process_preserves_fractional_shared_deadline`
for subsecond process execution and cleanup without resetting the shared deadline. Native platform/build results are recorded in the PR; this document is
the contract and does not itself claim live deployment or model acceptance.
