# Runtime WebUI v2 migration / parity matrix

Runtime WebUI v2 keeps the existing Runtime Console HTTP authority boundary and replaces only the browser presentation/runtime implementation. The production information architecture is **Work / Projects / Runtime**. Raw Runtime entities remain available as evidence below those three user-facing concepts.

| Current Runtime capability | Runtime WebUI v2 location | Authoritative endpoint | Migration status |
| --- | --- | --- | --- |
| Project inventory | Projects | `runtime-console/projects` | implemented |
| Server-side Project search | Projects search | `runtime-console/projects` | implemented |
| Runner filter | Projects filter | `runtime-console/projects` | implemented |
| Exact Project refs / canonical id | Project details / Evidence | `runtime-console/projects` | implemented |
| Project branch/path summary | Project details | `runtime-console/project-git` + Project projection | implemented; bounded lookup |
| Session inventory | Work; Project -> Active Sessions | `runtime-console/overview`, `workflow-sessions` | implemented |
| Session search | Work | loaded inventory + exact locator | implemented |
| Exact Session locator | Work search | `workflow-session-locate` | implemented |
| Session detail | Work + Session Context/Evidence | `workflow-session` | implemented |
| Session messages | Work transcript | `workflow-session-messages` / observe | implemented |
| Work/session composer | Work composer | `workflow-session-post-message` | implemented |
| Replace / withdraw retained message | Work transcript actions | replace / withdraw message endpoints | implemented |
| Validation/report state | Work + Context | canonical Session `overview.validation` / `reported_progress` | implemented |
| Current activity / Job evidence | Work execution state | Session `current_activity`, `running_call`, `running_jobs` | implemented |
| Grouped activity evidence | Work progress disclosure | Session `activity` | implemented |
| Session -> linked Windows | Session Evidence | `workflow-session` | implemented |
| Window inventory | Runtime -> Window Activity | `runtime-console/windows` | implemented |
| Window detail | Runtime -> Window Activity | `runtime-console/window` | implemented |
| Window -> linked Sessions | Runtime -> Window Activity | `runtime-console/window` | implemented |
| Window activity | Runtime -> Window Activity -> Recent activity | `runtime-console/window` | implemented |
| Runtime / Runner status | Runtime -> Overview | `runtime-console/overview`, `runner` | implemented |
| Jobs | Work execution state + Runtime metrics | Session projection + Runtime overview | implemented |
| Durable Agent inventory / diagnostics | Runtime -> Overview -> Agents | communication read endpoints | implemented as secondary Runtime evidence |
| Current auth / visibility semantics | All views | unchanged AuthMiddleware + Runtime Console project/runtime authority checks | preserved; unchanged |
| Credential retention | Connection gate / Lock | browser tab `sessionStorage` only | implemented |
| Draft retention | Work composer | per-Project/Session `sessionStorage` | implemented |
| Localization | Whole Runtime UI | existing en / zh-CN dictionary | implemented |
| Appearance | Whole Runtime UI | existing light / dark / system preference | implemented |
| Refresh / polling | View state | existing HTTP endpoints | implemented; view-scoped and lifecycle-aware |
| Loading / empty / stale / denied | Every data surface | adapter state, never synthetic data | implemented |
| Runtime extension / instruction management | Desktop owns this surface | backend endpoints remain available | intentionally not promoted into Runtime WebUI v2 |
| Admin console | `/admin` | existing Admin API | out of scope; legacy Admin build remains |

## Projection constraints

- Work is a deterministic presentation of already-authoritative Session / Job / validation evidence. It does not invent an execution phase, success result, command, participant, or ownership relation.
- Window correlation is evidence only. The UI models Project ↔ Session ↔ Window as many-to-many and never treats a Window as the owner of a Session.
- Stale data is explicitly marked. A failed refresh may retain the last authorized successful snapshot; a 401/403/404 authority transition must not retain revoked detail as current.
- Per-row enrichment (for example Project Git or Session Window counts) is bounded, cancellable, and independently authorized.
- Polling is view-scoped. Closed/inactive Session detail is not continuously polled after a fresh terminal observation.
- Actor identity is optional in the presentation model so future Coordinator / Worker / Reviewer activity can be represented without changing the Work timeline schema.
