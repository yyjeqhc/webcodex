# Exact edit selector schema parity

## Reproduced problem and bounded change

During this development session on mini, one real `edit_project_files` batch
supplied both `occurrence` and `expected_match_count` on a `replace_exact`
edit. The published schema admitted that shape, while the existing typed parser
rejected it before writing. Correcting the call to use only
`expected_match_count` applied the intended number of original-source matches.

This PR expresses that existing mutual exclusion at
`ProjectTextEditInput::ReplaceExact` in
`crates/webcodex-tool-contracts/src/tool_inputs.rs`, using Schemars' existing
variant extension support. It does not introduce a schema registry, Host-specific
business DTO, editor mode, or automatic revision inference. The runtime validator
is unchanged and remains authoritative.

`occurrence` selects one match. `expected_match_count` requires exactly N
matches and replaces all of them. The existing canonical editor description is shortened without
dropping its revision, ambiguity, transaction, Runner recheck, dry-run, stale-read,
unknown-effect, review or validation rules. No description budget is increased.

## Regression and publication evidence

Before the implementation change, the new complete-request regression failed
because the schema accepted this otherwise valid edit:

```json
{
  "project": "demo",
  "changes": [{
    "kind": "edit",
    "path": "a.rs",
    "expected_read_revision": 123,
    "edits": [{
      "kind": "replace_exact",
      "old_text": "old",
      "new_text": "new",
      "occurrence": 1,
      "expected_match_count": 1
    }]
  }]
}
```

The paired single-selector requests remain valid. The new tests also cover:

- The 3 × 3 matrix of omitted, null and valid-integer selector values.
- A mixed batch with an adjacent `insert_before` edit and an exact two-match
  replacement; adding the conflicting selector to the second edit rejects it.
- Existing closed-object validation and parser behavior.
- Actual in-process HTTP `tools/list` and exact `read_tool_manifest` output,
  through Adaptive/CLI, full/compact configuration, and legacy/stateless MCP.
  CLI manifest validation parses the schema from the final model-visible text.

Published input schemas already require optional selectors to be omitted rather
than explicitly null. The parser's existing `Option` handling still treats null
as absence. Tests preserve that pre-existing distinction; this PR does not change
nullable input compatibility.

The required-only `not` predicate belongs to the exact-edit object, inside its
existing tagged union. The input normalization and compact description projection
preserve it; no adapter maintains a second copy of the constraint.

## Cost and limits

Measurements are serialized UTF-8/JSON bytes, not model Tokens. The new predicate
adds 57 bytes to compact editor input schemas. Full input schemas grow by the same 57 bytes; the canonical description shrinks
by 75 bytes, reducing the complete canonical tool projection by 18 bytes.
These are serialization-size changes, not measured Host call-count savings.

The reproduced malformed request now fails schema validation in every tested
publication projection. That establishes a concrete contract correction; it does
not establish a ChatGPT, Claude Code or Codex error-rate improvement. The HTTP
tests exercise the repository's service and adapters, not real external Hosts.

**Compatibility qualification before merge:** acceptance of the emitted `not`
keyword by supported Direct Hosts has not been independently exercised in this
change. A Host may preserve, ignore or reject a schema keyword. Existing runtime
validation still prevents the forbidden edit, but cannot by itself prove tool
registration compatibility. Keep this PR in draft until registration smoke
checks establish compatibility for supported Hosts, or existing evidence for
these exact publication paths is supplied. Do not introduce client-specific
schema copies speculatively.

## Revision safety decision

Automatic use of a Workflow Session's latest read remains rejected. Shared
windows or Agents can read the same path after another caller has prepared an
edit; a mutable latest-read slot would silently replace that caller's evidence.

The current `src/tool_runtime/read_revisions.rs` registry maps an explicit handle
to a full-file SHA and exact Project, path, Runner client/instance and root
identity. `src/tool_runtime/files/mutations/text_edits.rs` resolves that evidence
before dispatch, and the Runner still rechecks its SHA and transaction. An
expired, mismatched or stale handle cannot be replaced by an unrelated read.
No new caller-binding evidence was introduced, so there is no basis for safe
automatic inference in this PR.

Existing behavior already reduces some mechanical reads: successful edits
return final `read_revision` values that can fence the next edit. The
`apply_text_edits_success_mints_final_revisions_and_continues_without_reread`
regression covers continuation, path/project/owner mismatches and returned
revision handling. Existing mixed-batch, ambiguity, read-only, dropped-waiter,
rollback and Runner SHA-conflict tests remain in place.

## Validation

On mini: **336 tool-contract tests passed** (one existing ignored),
**429 MCP tests passed** (one existing ignored), and **44 editor-runtime tests
passed**. The new HTTP matrix checks 11 complete requests against both publication
paths in eight surface/protocol configurations. Formatting and diff checks passed.

Measured final canonical sizes are input schema **6,107 bytes**, description
**484 bytes**, and complete tool projection **29,976 bytes**. The 20,000-byte
input and 30,000-byte total regression bounds are unchanged. Initial verification
caught overlong explanatory text; the extra field prose was removed and existing
editor guidance tightened while retaining all pre-existing semantic assertions.
No budget test or runtime guard was weakened.

Reproduce:

```sh
cargo test --locked -p webcodex-tool-contracts --lib
cargo test --locked -p webcodex --lib mcp::tests -- --nocapture
cargo test --locked -p webcodex --lib tool_runtime::tests::apply_text_edits -- --nocapture
cargo fmt --all -- --check
git diff --check
```

The branch starts independently from main `2371ec67`; it does not include commits
from the telemetry, guidance or glob PRs. It changes no durable Job or file
protocol and needs no data migration.
