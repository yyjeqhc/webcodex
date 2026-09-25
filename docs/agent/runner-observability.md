# Runner observability

Issue [#579](https://github.com/yyjeqhc/webcodex/issues/579) converges public Runner
observations on one vocabulary. Implementation baseline:
`5e6abf79b651c27d142299813a16a54d6ecadb73`.

| Previous observation | Canonical observation |
| --- | --- |
| `runtime_status.agents` | `runtime_status.runners` |
| `agents.clients` and `agents.summary.clients` | `runners.clients` |
| `list_runners.agents`, `list_runners.clients`, and `list_runners.summary.clients` | `list_runners.runners` |
| `agent_instance_id` | `runner_instance_id` |
| `agent_protocol_generation` | `runner_protocol_generation` |
| `mismatched_agents_count`, `source_mismatched_agents_count` | `mismatched_runners_count`, `source_mismatched_runners_count` |
| Admin `agents_total`, `agents_online` | `runners_total`, `runners_online` |
| CLI summary/server status `agents` | `runners` |

Full, compact, summary, and focused status retain their existing selection and
count semantics. Summary contains aggregates only. Compact clients retain health
facts formerly held by the duplicate health list: last-seen age, enabled project
count, inventory state, pending requests, active Jobs, and concurrency. Full-only
owner, policy, and capability details remain outside compact output. Focused
status describes only the selected Runner and includes its inventory state.

Admin, Console, CLI JSON/text, pairing owner checks, provider discovery, coding
startup health checks, and platform readiness readers consume this same tree.
An online peer does not make an unavailable owning Runner healthy. Project and
Job ownership, stale-instance fences, authorization, and readiness rules remain
authoritative. Runtime status uses `projects.runner_registered`; the separate
`list_projects.source` classification retains its existing `agent_registered` value.

Registration and transport DTOs, `/api/agents/*`, build-info
`agent_protocol_generation`, credential names, persisted records, diagnostic
trace fields, reason codes, canonical `agent:<client_id>:<project_id>` IDs, Coding
Agents, and durable Agent APIs retain their established contracts. This is not
a Runner wire-generation migration and does not add permanent observation aliases.

## Upgrade status

The owner clarified that this migration keeps `DESKTOP_RUNTIME_CONTRACT` at
`[1, 1]`. Its scope is the raw `runtime_status` / `list_runners` observations and
their first-party consumers, not the management command contracts Desktop
deserializes. Runner registration, transport, and build-info wire fields and
Runner wire generation remain unchanged.

Older CLI versions read `/agents/clients` or `/agents/summary/clients`; against
the new Server they may report an empty fleet or an unavailable Runner. Preserving
mixed-version compatibility for these raw observation clients is not required
by #579. The management-generation check is not an observation-schema version.

Atomic migration means updating the in-repository producers and first-party
consumers together in one change: Server, Console/Admin, CLI readers, scripts,
fixtures, E2Es, and documentation. The consumer list is not exhaustive; search
for additional observation readers without renaming real Durable Agent concepts.
Do not publish the intermediate Server-only commits independently. No permanent
dual projection is retained. Temporary fallback reads require a demonstrated
in-repository migration need; none is needed by this candidate.

Desktop continues to deserialize `RunnerStatusOutput.config` and
`runtime.checked`, `runtime.reachable`, and `runtime.client_online`. The CLI
reads the canonical Runner observation internally and preserves these fields.
`ServerStatusOutput` retains reachability, probe URL, PID, revision, management
contract, protocol compatibility, and build fields. Its Desktop DTO does not
deserialize the renamed auxiliary `agents` / `runners` statistics. The Desktop
`OpsProject.agent_status` field is also preserved.

The owner identified this cleanup as non-blocking for 0.4.2 and recommended
picking it up after that release to avoid late stabilization conflicts. Keep
the candidate separate from release hardening; recheck the integration baseline
and affected consumers when resuming after the release. This note does not
assert that 0.4.2 has already been released.

See [testing guidance](../TESTING.md#runner-observability-contract) for behavior
tests and the CI guard. Native Windows readiness tests and Linux socket-activation
E2E are separate evidence; passing one does not establish the other.
