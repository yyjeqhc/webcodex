| Route | Handler | GPT Action? | Audit required? | Audit status | Reason |
|---|---|---:|---:|---|---|
| `/api/health` | `health` | No | No | Not audited | Infra health check, not part of GPT task execution flow. |
| `/api/channels` | `list_channels` | No | No | Not audited | Message UI/support API, not Codex action business traffic. |
| `/api/messages` GET | `list_messages` | No | No | Not audited | Chat/message API, outside action session scope. |
| `/api/messages` POST | `create_message` | No | No | Not audited | Chat/message API, outside action session scope. |
| `/api/messages/{id}` GET | `get_message` | No | No | Not audited | Chat/message API, outside action session scope. |
| `/api/messages/{id}` DELETE | `delete_message` | No | No | Not audited | Chat/message API, outside action session scope. |
| `/api/files/{file_id}` | `download_file` | No | No | Not audited | File transfer API, not a GPT business action. |
| `/api/files` | `upload_file` | No | No | Not audited | File transfer API, not a Codex business action route. |
| `/api/desktop/task_op` | `desktop_task_op` | Yes | Yes | Audited | Aggregate desktop task action route used by GPT tools. |
| `/api/codex/action_sessions` | `action_sessions::codex_action_sessions` | Yes | No | Not audited | Audit viewer/control API; intentionally not self-audited to avoid recursive noise. |
| `/api/desktop/tasks` GET | `list_desktop_tasks` | No | No | Not audited | UI/admin CRUD surface; GPT business path is `/api/desktop/task_op`. |
| `/api/desktop/tasks` POST | `create_desktop_task` | No | No | Not audited | UI/admin CRUD surface; aggregate action route already covered. |
| `/api/desktop/tasks/claim_next` | `claim_next_desktop_task` | No | No | Not audited | Worker orchestration route, not GPT action-facing. |
| `/api/desktop/tasks/{id}` | `get_desktop_task_detail` | No | No | Not audited | UI detail route, not GPT action-facing. |
| `/api/desktop/tasks/{id}/claim` | `claim_desktop_task` | No | No | Not audited | Worker orchestration route, not GPT action-facing. |
| `/api/desktop/tasks/{id}/event` | `append_desktop_task_event` | No | No | Not audited | Worker event ingestion route, not GPT action-facing. |
| `/api/agent/run` | `agent::run_agent` | No | No | Not audited | Separate agent playground subsystem. |
| `/api/agent/specs` GET | `agent::list_agent_specs` | No | No | Not audited | Agent playground config API. |
| `/api/agent/specs` POST | `agent::save_agent_spec` | No | No | Not audited | Agent playground config API. |
| `/api/agent/specs/{id}` GET | `agent::get_agent_spec` | No | No | Not audited | Agent playground config API. |
| `/api/agent/specs/{id}` DELETE | `agent::delete_agent_spec` | No | No | Not audited | Agent playground config API. |
| `/api/codex/context` | `codex::codex_context` | Legacy/No | No | Not audited | Legacy single-read route. GPT Actions compact schema uses `/api/codex/context_batch`, which is audited. |
| `/api/codex/projects` | `codex::codex_projects` | Yes | Yes | Audited | Lightweight project discovery endpoint exposed in compact schema. |
| `/api/codex/context_batch` | `codex::codex_context_batch` | Yes | Yes | Audited | Primary GPT context read route. |
| `/api/codex/apply_patch` | `codex::codex_apply_patch` | Legacy/No | No | Not audited | Legacy patch route superseded by `/api/codex/edit` in current GPT Actions schema. |
| `/api/codex/edit` | `codex::codex_edit` | Yes | Yes | Audited | Primary GPT edit route. |
| `/api/codex/artifact` | `codex::codex_artifact` | Yes | Yes | Audited | GPT artifact/save route. |
| `/api/codex/git` | `codex::codex_git` | Yes | Yes | Audited | GPT git operation route. |
| `/api/codex/command` | `codex::codex_command` | Yes | Yes | Audited | Direct configured command execution route, exposed in compact schema. |
| `/api/codex/command_request` | `codex::codex_command_request` | Legacy/No | No | Not audited | Legacy helper route covered conceptually by `/api/codex/command_request_op`. |
| `/api/codex/command_request_op` | `codex::codex_command_request_op` | Yes | Yes | Audited | Primary aggregate command request workflow route. |
| `/api/codex/job` | `codex::codex_job` | Yes | Yes | Audited | Primary aggregate job workflow route. |
| `/api/codex/command_request_raw` | `codex::codex_command_request_raw` | Legacy/No | No | Not audited | Legacy helper route superseded by `/api/codex/command_request_op`. |
| `/api/codex/command_requests` | `codex::codex_command_requests` | Legacy/No | No | Not audited | Legacy helper route superseded by `/api/codex/command_request_op`. |
| `/api/codex/command_request_batch` | `codex::codex_command_request_batch` | Legacy/No | No | Not audited | Legacy helper route superseded by `/api/codex/command_request_op`. |
| `/api/codex/command_approve` | `codex::codex_command_approve` | Legacy/No | No | Not audited | Legacy helper route superseded by `/api/codex/command_request_op`. |
| `/api/codex/command_reject` | `codex::codex_command_reject` | Legacy/No | No | Not audited | Legacy helper route superseded by `/api/codex/command_request_op`. |
| `/api/codex/check` | `codex::codex_check` | Yes | Yes | Audited | GPT project check route. |
| `/api/codex/report` | `codex::codex_report` | Yes | Yes | Audited | GPT report write route. |

## Coverage Notes

- Current audited GPT business routes are also reflected in `src/action_sessions.rs` via `AUDITED_ACTION_ROUTES`.
- `context_batch` covers the current GPT read path; `context` remains available for legacy/manual callers but is not part of the compact GPT tool surface.
- `edit` covers the current GPT write path; `apply_patch` remains a legacy/manual endpoint.
- `command_request_op` covers list/create/approve/reject/goal/trusted raw command workflows; older helper routes remain uninstrumented by design.
- `action_sessions` itself is intentionally excluded to avoid recursive self-auditing noise.
