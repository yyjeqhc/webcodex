# Runtime Join and local update validation

Additional Linux machines use the existing Join + Runner setup, hidden pairing
input, saved identity, project registration and Environment service manager.
Package flavor remains separate from role. No central Server is created by Join,
including when the initial project is skipped.

The CLI keeps the verified installed package flavor, architecture and format for
candidate selection and apply. Manual/source/bare Runner installations cannot
gain Runtime eligibility from a missing Desktop. Linux effects retain the TTY,
explicit confirmation, original owner and system sudo boundary. Recovery uses
existing Core operations and terminal outcomes; it does not redispatch an
uncertain installer. Remote components are never updated by this entry point.

## Integrated checks, 2026-10-06

After merging upstream `8719afd7` and the package contract/packaging branches:

- `cargo test --profile dogfood -p webcodex-cli environment::`: 32 passed.
- `cargo test -p webcodex-cli environment::`: the same 32 passed with the
  default unoptimized profile and normal thread stack.
- CLI dogfood build, root formatting and diff checks passed.
- The built CLI returned bounded JSON for absent-environment status and rejected
  nonTTY apply/resume/rollback before creating Environment/cache roots. An invalid
  201-character label was rejected without echoing the value or creating roots.
- Named projectless Join passes the existing display label to Core, keeps Runner
  enabled and excludes Server/project setup steps. Shared Unicode/no-NUL bounds
  are checked before opening the store; saved resume keeps its original request.
- A focused Runner catalog fixture passed after checking JSON field names rather
  than substrings of an opaque revision hash. Private-path assertions remain,
  and a revision containing private-field words exercises that distinction.
- Runtime/metadata/plan/collection/publication tests: 81 passed on the integrated
  packaging branch. Documentation links passed; no new publication was executed.

The first public CLI regression exposed a default-profile future stack overflow;
its adapter allocation fix is inherited from the package contract PR. The Runner
fixture correction and Join follow-ups are commits in this existing feature PR.

These checks are source/fixture/built-binary checks. No real SSH/sudo authorization,
live pairing Server, DEB/RPM installation, services, session/logout/reboot,
upgrade/rollback, Tunnel/ChatGPT project read, deployment or Release was executed.
Native evidence remains separate in the deployment-validation matrix. Complete
secret backup and restore remain deferred pending the requested design review.
