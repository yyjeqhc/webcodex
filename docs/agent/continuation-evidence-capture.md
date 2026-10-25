# Bounded continuation evidence capture

The opt-in `scripts/continuation_capture.py` collector reads durable metadata for
one exact, existing Goal, controller Agent and correlated Workflow Session.
It separates resource preparation from observation: launch it after the coordinator
has those identifiers, rather than spending an observation timeout waiting for a
human to create resources. Missing resources fail immediately. It does not create
resources, invoke tools, send messages, dispatch/retry Wakes or change readiness.

This implements the evidence-collection follow-up in
[#1284](https://github.com/yyjeqhc/webcodex/issues/1284), related to
[#1159](https://github.com/yyjeqhc/webcodex/issues/1159) and
[#1051](https://github.com/yyjeqhc/webcodex/issues/1051). It is not a new Host
acceptance result or a replacement for the
[continuation acceptance procedure](../implementation/mcp-app-auto-resume-restoration.md).

## Coordinator procedure

1. Verify the authorized test runtime's actual Server, Runner, Desktop and Tunnel
   executables, clean source/build, listener, data root, Tunnel binding and active
   work. Reuse that environment and its existing credentials. Never fall back to
   stable. Record the result in a private deployment attestation as described below.
2. Obtain the exact identifiers from the existing authorized preparation result.
   Do not ask the user to copy identifiers, click diagnostics or present cards again
   merely for collection. Collection uses local SQLite and needs no PAT. If exact
   identifiers are unavailable, report that limitation; do not select the newest
   Goal or discover unrelated work by scanning.
3. Create a new empty private output directory. Start observation only now. The
   budget begins at invocation, not at a future checkpoint or user action. Attach
   before the first checkpoint when feasible; otherwise retain the initial state
   and report the missing prefix. A completed Goal supports retrospective capture.
4. For a future behavior change that requires real Host acceptance, give the user
   one prepared prompt and request one final observation: whether a fresh reply was
   already complete on return, without clicks/confirmation. Collect Server evidence
   independently. Reuse applicable existing evidence when runtime behavior is
   unchanged; do not repeat manual trials for collector-only changes.
5. Preserve all receipts and failures privately. Publish a bounded, redacted summary
   with capture source, historical execution source when independently known, and
   separately attributed user observation. Do not upload raw receipts by default.

Example (replace placeholders with verified local paths and exact identifiers):

```bash
mkdir -m 700 /private/test-evidence/new-capture
python3 scripts/continuation_capture.py \
  --database /private/test-runtime/data/webcodex.db \
  --source-record /private/test-runtime/current-deployment.json \
  --expected-source FULL_VERIFIED_40_HEX_COMMIT \
  --goal-id EXACT_GOAL_ID --agent-id EXACT_AGENT_ID --session-id EXACT_SESSION_ID \
  --output /private/test-evidence/new-capture \
  --watch-seconds 420 --interval-seconds 5
```

Use `--watch-seconds 0` (the default) for one retrospective snapshot. Python 3.11+
with SQLite JSON/window-function support is required; no third-party package is
needed. Run the focused suite with:

```bash
python3 -m unittest scripts.tests.test_continuation_capture scripts.tests.test_continuation_snapshot
```

## Source attestation and bounds

The coordinator's private JSON record has this shape:

```json
{
  "source": "<verified 40 lowercase hexadecimal commit>",
  "components": {
    "server": {"source": "<same commit>", "sha256": "<verified binary SHA-256>"},
    "runner": {"source": "<same commit>", "sha256": "<verified binary SHA-256>"},
    "desktop": {"source": "<same commit>", "sha256": "<verified binary SHA-256>"},
    "tunnel": {"source": "<same commit>", "sha256": "<verified binary SHA-256>"}
  }
}
```

The collector validates this attestation and fences its identity/content before
and after each snapshot. It does **not** probe processes, authenticate the record,
or detect a process restart that leaves the record unchanged. The coordinator
must verify the live chain separately. The attested source describes the runtime
at capture, not necessarily the source that executed historical work. Never
rewrite an earlier trial's build from this record.

The source file must be a current-user-owned regular single-link file with no
group/other permissions, in a private directory. The output must be an empty
current-user-owned private directory. Symlinked path components and unsafe file
types are rejected. Receipts use exclusive creation and mode `0600`; existing
receipts are never overwritten. The collector does not delete them on cancellation.

Each snapshot uses SQLite `mode=ro`, `query_only`, a transaction, a three-second
query budget, a 100-ms lock timeout, and database identity checks. It reads no
message bodies, titles, summaries, claim fences, consume tokens or credentials
into the receipt. It validates exact owner/correlation, lifecycle, generation and
consume metadata, retaining legitimate generation changes and revoked attempts.
There is no database migration, repair or lifecycle mutation. Normal SQLite WAL
coordination still applies; this is not an OS sandbox or an offline forensic copy.

Limits are 128 rows per evidence table, 64 Goal steps, a 256-KiB plan inspection
bound, 128-KiB snapshot output and 256-KiB per receipt. Observation is at most
900 seconds with intervals of 1–30 seconds (default 5). Both time and poll count
are bounded: at most 901 snapshots plus one end receipt, under 226 MiB at the
largest allowed settings. A final in-progress snapshot can take up to its query
budget beyond the observation deadline. Schema gaps, identity/source changes,
regression, excessive evidence and clock anomalies stop collection. Exit codes:
`0` collection finished, `1` incomplete, `130` cancelled. A storage failure may
prevent an incomplete receipt; absence of a valid end receipt is incomplete.

## Reading the result

- `before_first_checkpoint` describes the initial database state only. It does
  not prove that the browser was visible, mounted or continuously observed.
  `late_or_snapshot_only` is explicitly partial/retrospective; never backfill it
  into a prospective capture or erase a previous watcher timeout.
- Each sample reports the current Endpoint lease/lifecycle and exact Goal
  attention/Wake/attempt records. An expired lease does not identify browser
  throttling, unmount, network loss or Host rejection as its cause.
- `delivery_unknown_do_not_resend` preserves uncertainty. This collector cannot
  authorize or perform retry. Multiple prepared attempts require inspection;
  distinct attention epochs and pre-dispatch revoked attempts are retained.
- `exact_consume_recorded` requires matching durable Wake/attempt/Endpoint/
  generation metadata. It does not establish a fresh model turn, user inactivity,
  prompt adoption or production readiness. Those require separate evidence.
- Server intervals cover meaningful activity → Wake, Wake → prepare, prepare →
  delivery, delivery → consume, and Wake → consume when their timestamps exist.
  They are not browser scheduling latency. Missing timestamps stay null; negative
  intervals are findings. `sqlite_snapshot_count` counts collector reads, not App
  polls or model requests. Model tokens and monetary cost remain unknown.

Automatic coverage includes delayed preparation, cancellation, clock drift,
source/database replacement, ownership/correlation, malformed/oversized evidence,
WAL reads, generation changes, uncertain dispatch and exact consumption. These
SQLite/fake-clock cases do not substitute for a real browser lifecycle trial.
