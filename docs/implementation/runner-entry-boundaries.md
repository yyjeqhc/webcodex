# Runner entry-point boundaries

The Runner binary entry owns CLI parsing, startup and process exit. Implementation
modules no longer consume executable helpers or external-crate aliases from
`main.rs`.

| Owner | Responsibility |
| --- | --- |
| `execution_io` | Output decoding/draining, reader convergence, Runner-level process-tree coordination and validation evidence |
| `file_dispatch` | Typed file routing after the existing policy and exact-target checks |
| `projects::created_paths` | Rollback ownership for paths created by one Project operation |
| `transport::http_client` | Bounded HTTP exchanges, redacted errors and error classification |
| `transport::registration` | Registration facts, capability projection and registration recovery |
| `transport::poll_dispatch` | Bounded polling dispatch workers, admission and polling response handling |
| Existing `transport::bootstrap` / supervisor modules | Runtime resource assembly and transport lifecycle |

These are ordinary Rust modules within the existing Runner package, not a new
process framework. Native process groups and Windows Job Objects remain owned by
`webcodex-process`. Job registry/queues remain owned by `JobManager`; transport
continues to supply replaceable sinks.

The extraction preserves operation bodies, absolute deadlines, reader draining,
process-tree cleanup, shutdown/cancellation, provider metadata acknowledgements,
HTTP body bounds, and result-submission uncertainty. No wire labels, public tool
names, authentication checks or persistent formats are changed. Root test names
remain stable; shared fixture imports now live in `main_tests.rs`, not in the
production entry.

The existing `workspace_boundary_check.py` also checks implementation imports for
references through the binary crate root. It permits the implementation tree and
explicit test fixtures, but rejects dependencies on entry helpers and root aliases.
This lexical convention guard complements compilation and behavioral regression
tests; it is not a Rust parser or a proof of every possible dependency edge.

Validation should cover the Runner package plus the relevant serial real-process
lane (`runner-real-process-tests`). Linux results do not certify Windows/macOS
process behavior; those native lanes remain necessary for platform acceptance.
