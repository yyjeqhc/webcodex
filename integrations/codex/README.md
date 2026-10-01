# Optional local Codex external observations (proposal #631)

This integration records bounded **external reports** in an explicitly selected
WebCodex Workflow Session. It does not install Hooks, change trust, create a
Session/Goal, execute commands, or migrate a conversation. The Server now projects
these claims read-only in `read_session_handoff`. The optional read-only local
consumer and optional automatic entry below can inspect that brief and the optional
read-only Goal recovery context described below; export remains follow-up work. This
does not establish real two-sided UI acceptance.

## Server contract

- `record_external_observation(project, session_id, adapter_id, event_id, observed_tool,
  exit_code?)`: exact authorized Project and Workflow Session; `session:collaborate`
  plus normal Project authority. A closed Session rejects new writes.
- `list_external_observations(project, session_id)`: authorized read of the same
  reports, available through normal MCP tool discovery/gateway and REST.

The write tool is intentionally adapter/API ingress and is hidden from the model
tool surface; `list_external_observations` remains model-visible for continuity
recovery. A model therefore cannot manufacture an external report through ordinary
MCP discovery while an authenticated local adapter can still submit one through the
supported Runtime API.

Both identities are required, not inferred from the connection, directory, window
or credentials. `adapter_id` and `event_id` are lowercase SHA-256 strings; they are
correlation keys, not authenticated provenance or authorization tokens. Any authorized
caller can submit claims, so output always identifies `provenance=external_report`.
Tool names are bounded ASCII identifiers; raw commands, arguments, stdout/stderr,
paths and transcript bodies are not inputs. Missing exit codes remain `unknown`;
provided codes become `reported_success` / `reported_failure`, never native Job or
validation verdicts. Reports do not update Goal progress or mark work complete.

A transaction commits the report and its deduplication identity together in the
existing Server database. Exact replay returns the stored observation, including
its original server timestamp; a changed tool/exit code conflicts. The key is scoped
to one Session and adapter. A write failure does not prove that no write occurred:
reconcile by submitting the **same** identity/payload, never rerunning work.

The prototype admits at most 256 observations per Session and 65,536 total. It
fails closed at capacity; it does not evict replay keys or silently claim full
history. There is no automatic garbage collection yet. This conservative capacity
policy and the final API shape need maintainer review before broad rollout. The
reported local operation is never represented as a native WebCodex tool/Job/validation
event, and the explicit ingestion/read calls are deliberately excluded from the
business Workflow Session event ledger so adapter traffic cannot evict native
execution evidence. The first adapter also has no durable source sequence: list
results therefore expose `coverage.complete=false` and do not claim complete capture
or source execution ordering. `read_session_handoff` includes the last five
retained reports in `handoff_brief.external_observations`, with exact source IDs,
unknown count, truncation and explicit incomplete coverage. Use
`list_external_observations` to inspect all retained reports (at most 256).
Session lifecycle/authority remain owned by the existing Session store;
the SQLite table is only external evidence, not a second task state machine.

## Optional adapter (macOS / Linux / Windows, Python 3.10+)

After the user confirms the exact project and work, prepare one private configuration
**outside the project** with the exact canonical project root, existing Workflow
Session and local Codex conversation. Use normal supported host configuration and
trust approval; this repository does not install or approve the Hook automatically.
No production service or global configuration changes are necessary to review it.

```json
{
  "server_url": "https://webcodex.example.com",
  "authorization_file": "/private/operator/webcodex-authorization",
  "project": "agent:my-runner:my-project",
  "project_root": "/absolute/canonical/project",
  "workflow_session_id": "wc_sess_EXACT_EXISTING_SESSION",
  "local_session_id": "EXACT_LOCAL_CODEX_CONVERSATION",
  "state_dir": "/private/operator/observation-outbox"
}
```

Configuration and authorization files must be private regular single-link files;
authorization contains the deployment's existing full Authorization header value.
The pre-created outbox directory must also be private. Keep all three outside the
project so reading an untrusted checkout cannot change the target or obtain credentials.
On macOS/Linux, files must be owned by the current user with mode 0600 and the state
directory with mode 0700. On Windows, files/directories must be owned by the current
user, must not be reparse points, and their DACL may grant access only to the
current user, SYSTEM, and the local Administrators group. The adapter refuses symlink/reparse
targets and redirects on every platform; remote transport requires HTTPS and HTTP is
accepted only for loopback. It uses the configured origin directly, not ambient proxy
settings.

On Windows, create the operator files as the current user and remove inherited broad
access before use. One PowerShell pattern is:

```powershell
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
icacls C:\private\operator\observation.json /inheritance:r
icacls C:\private\operator\observation.json /setowner "$me"
icacls C:\private\operator\observation.json /grant:r "${me}:(F)" "SYSTEM:(F)" "*S-1-5-32-544:(F)"
icacls C:\private\operator\webcodex-authorization /inheritance:r
icacls C:\private\operator\webcodex-authorization /setowner "$me"
icacls C:\private\operator\webcodex-authorization /grant:r "${me}:(F)" "SYSTEM:(F)" "*S-1-5-32-544:(F)"
icacls C:\private\operator\observation-outbox /inheritance:r
icacls C:\private\operator\observation-outbox /setowner "$me"
icacls C:\private\operator\observation-outbox /grant:r "${me}:(OI)(CI)(F)" "SYSTEM:(OI)(CI)(F)" "*S-1-5-32-544:(OI)(CI)(F)"
```

Use canonical absolute Windows paths in JSON (for example
`C:\\private\\operator\\observation.json`). The command examples below use
`python3`; use the equivalent installed interpreter such as `py -3` on Windows.

Configure a `PostToolUse` command through the client's normal supported Hooks flow:

```text
python3 /absolute/path/external_observation_hook.py --config /private/operator/observation.json
```

Expected Hook input: `hook_event_name`, `session_id`, `cwd`, `tool_use_id`,
`tool_name`. Other fields, including tool input/output, are ignored. Input is bounded
to 128 KiB. Exact conversation/root matching happens before sending. The generic
adapter deliberately reports **unknown for every tool**, because free-text output
is not an authoritative execution receipt. Additional structured receipt recognition
must have separate, tool-specific tests.

A report is written/fsynced to the private outbox before transmission. On a missing
or mismatched acknowledgement it stays pending. Later invocations drain pending
reports within one bounded deadline, or the operator can explicitly run:

```text
python3 /absolute/path/external_observation_hook.py --config /private/operator/observation.json --flush
```

This retries observation storage only. It never runs business commands. A changed
configuration cannot retarget old pending reports. Conflicting/corrupt files remain
for diagnosis. At capacity or a concurrent adapter lock the command returns a visible
nonzero incomplete result; it does not claim the new event was captured. Completion
of the original native tool is independent of recording success. A client that does
not expose Hook failure cannot provide a complete capture guarantee.

## Read an existing Session when local Codex takes over

From the configured project directory, run:

```text
python3 /absolute/path/read_handoff.py --config /private/operator/observation.json
```

This uses the same private operator configuration and calls the hidden
`get_session_handoff_state` API ingress for its exact Project and Workflow Session.
That ingress reuses the canonical `read_session_handoff` projection, but it is
non-meaningful adapter traffic and deliberately does not expose its business
Session through generic recorder semantics. It therefore does not append Workflow
Session tool-call telemetry or refresh Goal liveness. Standard request/audit
telemetry may still be retained outside the Workflow Session.

The reader rejects a different current project, response identity, missing evidence
basis, a missing/invalid external-report section, or a nondeterministic/LLM summary.
The JSON output retains the bounded `handoff_brief`, including incomplete basis,
unknown reports and incomplete source coverage. A successful read means only that
the brief was obtained; inspect current project rules, files, Git status, Jobs and
unknown operations before continuing. The command does not select a Session by
directory or recency, bind the new local conversation, mark work complete, replay
an operation, install a Hook, or write a handoff file.

When the exact Workflow Session already has one caller-owned active Goal linked by an
explicit Workflow Session↔Goal correlation, the Server may also return sibling
`goal_context`. This projection is separate from the 8 KiB `handoff_brief` and contains
bounded Goal identity/lifecycle/revision, objective, current plan and checkpoint
context. Zero correlated active Goals omit it. Multiple active Goals return only a
bounded `selection_required` candidate set and never choose or expose one Goal as the
default. Reading this context does not create or associate a Goal, checkpoint/update/
complete progress, refresh Goal liveness, schedule work, or grant Goal/Session/Project
authority.

`read_handoff.py` itself remains a deliberate recovery command.
It requires an already selected Workflow Session and the normal authorized
credential; a new local Codex conversation must not inherit an old conversation's
write binding merely because it can read the brief. Real client and MCP roundtrip
acceptance is still required before general rollout.

## Optional automatic recovery entry

`session_recovery.py` offers the same read-only evidence on `SessionStart` and
`UserPromptSubmit`. Register this command for those two events through the
client's supported Hook configuration and trust UI:

```text
python3 /absolute/path/session_recovery.py --registry /private/operator/recovery.json hook
```

Prepare a private registry, authorization file containing the complete
Authorization value, and an existing private state directory, all outside the
projects they serve. Apply the same Unix mode or Windows ACL/reparse-point rules
described above:

```json
{
  "version": 1,
  "server_url": "http://127.0.0.1:18880",
  "authorization_file": "/private/operator/authorization",
  "state_dir": "/private/operator/recovery-state",
  "bindings": []
}
```

After the user confirms the project and work, the local Agent can discover
existing Sessions and establish a read association without asking the user to
copy identifiers or install per-project Hooks:

```text
python3 /absolute/path/session_recovery.py --registry /private/operator/recovery.json discover --project-root /absolute/project
python3 /absolute/path/session_recovery.py --registry /private/operator/recovery.json associate --project-root /absolute/project --session wc_sess_SELECTED --entry-root /absolute/project
```

Discovery returns exact-root candidates; it does not choose the newest Session,
create one, or bind a writer. Match the user's work intent and ask only if it is
ambiguous. Additional `--entry-root` values allow explicitly confirmed alternate
local checkouts to read the same remote work. `--local-session` restricts an
association to one local conversation; otherwise a new conversation at the same
entry can recover it. Several matching Sessions require selection; an explicit
conversation association takes precedence. Root canonical paths and filesystem
identities are checked, nested repositories do not inherit a parent entry, and each
recovery rechecks that the current Server Project still maps the associated canonical
Project id to that exact root.

Each entry rereads the server. A changed brief is offered as Hook context; an
unchanged prompt gets a short snapshot reference. `SessionStart` reintroduces the
brief after compaction. Large briefs use a bounded private snapshot instead of
filling model context. Snapshots prove that evidence was offered, not that the
Host or Agent consumed it. Offline and invalid responses remain failures and do
not present cached evidence as current. Unknown outcomes and incomplete coverage
are preserved. No observations, Goals, business operations or historical events
are written or replayed by this entry.

## Verification boundary

Python unit tests use synthetic Hook payloads and controlled senders, including
uncertain delivery/retry, changed associations, private-file checks, redaction,
and the read-only consumer's exact identity checks. CI runs this suite on both
Linux and Windows; Windows coverage includes native ACL, non-blocking lock, atomic
replace and reparse-point checks.
Rust tests cover transactional replay/reopen/conflict/capacity, authenticated
runtime dispatch, and the actual `/api/tools/call` non-recording handoff path. These
do **not** establish real Codex Hook lifecycle or ChatGPT
browser acceptance of this proposed adapter. The separately deployed downstream
prototype's acceptance is not acceptance of these new endpoints.

Before rollout: in an isolated approved fixture, install/trust through the normal
client UI, bind the exact independent local conversation, perform a minimal real
read, verify one report through actual MCP, and retain `unknown` when no receipt is
available. Verify the wrong-project and offline cases separately. Do not exercise
this candidate against a production server that lacks the new endpoints.
