# Runtime WebUI v2 migration / parity matrix

Runtime WebUI v2 keeps the existing Runtime Console HTTP authority boundary and replaces only the browser presentation/runtime implementation. The production information architecture is **Work / Projects / Runtime**. Raw Runtime entities remain available as evidence below those three user-facing concepts.

| Current Runtime capability | Runtime WebUI v2 location | Authoritative endpoint | Migration status |
| --- | --- | --- | --- |
| Project inventory | Projects | `runtime-console/projects` | production target |
| Server-side Project search | Projects search | `runtime-console/projects` | production target |
| Runner filter | Projects filter | `runtime-console/projects` | production target |
| Exact Project refs / canonical id | Project details / Evidence | `runtime-console/projects` | production target |
| Project branch/path summary | Project details | `runtime-console/project-git` + Project projection | production target; bounded lookup |
| Session inventory | Work; Project -> Active Sessions | `runtime-console/overview`, `workflow-sessions` | production target |
| Session search | Work | loaded inventory + exact locator | production target |
| Exact Session locator | Work search | `workflow-session-locate` | production target |
| Session detail | Work + Session Context/Evidence | `workflow-session` | production target |
| Session messages | Work transcript | `workflow-session-messages` / observe | production target |
| Work/session composer | Work composer | `workflow-session-post-message` | production target |
| Replace / withdraw retained message | Work transcript actions | replace / withdraw message endpoints | production target |
| Validation/report state | Work + Context | canonical Session `overview.validation` / `reported_progress` | production target |
| Current activity / Job evidence | Work execution state | Session `current_activity`, `running_call`, `running_jobs` | production target |
| Grouped activity evidence | Work progress disclosure | Session `activity` | production target |
| Session -> linked Windows | Session Evidence | `workflow-session` | production target |
| Window inventory | Runtime -> Window Activity | `runtime-console/windows` | production target |
| Window detail | Runtime -> Window Activity | `runtime-console/window` | production target |
| Window -> linked Sessions | Runtime -> Window Activity | `runtime-console/window` | production target |
| Window activity | Runtime -> Window Activity -> Recent activity | `runtime-console/window` | production target |
| Runtime / Runner status | Runtime -> Overview | `runtime-console/overview`, `runner` | production target |
| Jobs | Work execution state + Runtime metrics | Session projection + Runtime overview | production target |
| Durable Agent inventory / diagnostics | Runtime -> Overview -> Agents | communication read endpoints | retained as secondary Runtime evidence |
| Current auth / visibility semantics | All views | unchanged AuthMiddleware + Runtime Console project/runtime authority checks | must remain unchanged |
| Credential retention | Connection gate / Lock | browser tab `sessionStorage` only | production target |
| Draft retention | Work composer | per-Project/Session `sessionStorage` | production target |
| Localization | Whole Runtime UI | existing en / zh-CN dictionary | production target |
| Appearance | Whole Runtime UI | existing light / dark / system preference | production target |
| Refresh / polling | View state | existing HTTP endpoints | production target; view-scoped and lifecycle-aware |
| Loading / empty / stale / denied | Every data surface | adapter state, never synthetic data | production target |
| Runtime extension / instruction management | Desktop owns this surface | backend endpoints remain available | intentionally not promoted into Runtime WebUI v2 |
| Admin console | `/admin` | existing Admin API | out of scope; legacy Admin build remains |

## Projection constraints

- Work is a deterministic presentation of already-authoritative Session / Job / validation evidence. It does not invent an execution phase, success result, command, participant, or ownership relation.
- Window correlation is evidence only. The UI models Project ↔ Session ↔ Window as many-to-many and never treats a Window as the owner of a Session.
- Stale data is explicitly marked. A failed refresh may retain the last authorized successful snapshot; a 401/403/404 authority transition must not retain revoked detail as current.
- Per-row enrichment (for example Project Git or Session Window counts) is bounded, cancellable, and independently authorized.
- Polling is view-scoped. Closed/inactive Session detail is not continuously polled after a fresh terminal observation.
- Actor identity is optional in the presentation model so future Coordinator / Worker / Reviewer activity can be represented without changing the Work timeline schema.
