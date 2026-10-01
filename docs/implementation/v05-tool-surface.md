# v0.5 tool surface: retire Actions, standardize canonical names

This is a breaking model/API contract change, not a published release. Existing
package versions and the separate v0.4.4 release branch are not changed here.
There is no production old-name map, alias registry, redirect, downgrade mode or
compatibility Cargo feature. Clients must refresh the advertised MCP schema and
use the new canonical names. Cached old tool invocations fail closed.

## Removed integration

GPT Actions/OpenAPI generation, its frozen direct/supported snapshots, the
`legacy-gpt-actions` feature (including forwarding features), the standalone CI
lane, `/api/actions/{tool_name}`, `/openapi.json`, and the old Action-only REST
attachment import route are removed. Per-tool Action descriptions/exposure and
the unused HTTP OpenAPI projection field are deleted with their consumers.
The Salvo OpenAPI feature and now-unused transitive packages are pruned; existing
remaining dependency versions are retained.

MCP, authenticated `/api/tools/call`, operational CLI/REST, Runner transports,
OAuth/PAT/Project authorization, Host attachment import, streaming artifact I/O,
ActionAudit records and explicitly saved Workflow Sessions remain. Import
security tests now use a **test-only** trusted-MCP fixture instead of retaining
a public compatibility adapter. Real MCP provenance/SSRF/permission tests remain.

## Naming and unchanged domains

Operation names follow `verb_object`; list/read/get/observe/wait retain their
different semantics. Native batches use plural objects. Provider validation
(`cargo_check`, `cargo_test`, `cargo_fmt`, `go_test`) and the documented portable
`project_build` / `project_validate` entries keep their domain vocabulary.
`plugin_tool`, `mcp_tool` and `call_runtime_tool` refer to actual separate callable
namespaces; ordinary tools are not collapsed into category mega-tools.

Canonical names change atomically across ToolDefinition, ToolCall serialization,
input/output schemas, manifest/intent/recommendation construction, generated
follow-ups, App client calls, CLI/environment/Desktop consumers and tests.
No new name reuses another retired tool's identity. In particular the broad
artifact action surface is `inspect_project_artifact`; the bounded byte reader
is `read_project_artifact_chunk`.

The original enum variant names and low-level Rust module/file names are not
runtime aliases. Explicit serde canonical names bind the existing operation to
its new spelling. Runner protocol operation tags, capability strings, stable
Store operation/idempotency keys, database records, HTTP paths and business
argument/result field names are separate contracts and are not globally renamed.
For example `ssh_resource` remains an argument field; `manage_ssh_resource` is
its tool. Native wire `job_write_input` remains distinct from the model tool
`write_job_input`, which keeps its existing **Direct** policy and effect hints.
This preserves transport semantics without accepting old model tool names.

Historical audit/measurement reports describe their original versions and are
not silently rewritten as observations of v0.5. Retained old recipe text is not
an executable alias; current parser/admission still rejects retired tool names.
No migration of saved Jobs, request intent, context, or input replay IDs occurs.

## Rename table

This table and the test-only JSON fixture are change documentation, never read
by the production tool dispatcher.

| Before v0.5 | Canonical v0.5 name |
|---|---|
| `agent_wait_state` | `get_agent_wait_state` |
| `project_artifact` | `inspect_project_artifact` |
| `read_project_artifact` | `read_project_artifact_chunk` |
| `artifact_upload_begin` | `begin_artifact_upload` |
| `artifact_upload_chunk` | `upload_artifact_chunk` |
| `artifact_upload_finish` | `finish_artifact_upload` |
| `artifact_upload_abort` | `abort_artifact_upload` |
| `browser_observe` | `observe_browser` |
| `browser_act` | `control_browser` |
| `workspace_checkpoint_create` | `create_workspace_checkpoint` |
| `workspace_checkpoint_list` | `list_workspace_checkpoints` |
| `workspace_checkpoint_show` | `read_workspace_checkpoint` |
| `workspace_checkpoint_restore` | `restore_workspace_checkpoint` |
| `workspace_checkpoint_delete` | `delete_workspace_checkpoint` |
| `code_mode_exec` | `execute_code_mode` |
| `code_mode_exec_effectful` | `execute_effectful_code_mode` |
| `code_mode_exec_mutating` | `execute_mutating_code_mode` |
| `coding_agent_start` | `start_coding_agent` |
| `coding_agent_observe` | `observe_coding_agent` |
| `coding_agent_cancel` | `cancel_coding_agent` |
| `agent_continuation_bind` | `bind_agent_continuation` |
| `agent_continuation_recover_endpoint` | `recover_agent_continuation_endpoint` |
| `agent_continuation_state` | `get_agent_continuation_state` |
| `agent_continuation_wake_acquire` | `acquire_agent_continuation_wake` |
| `agent_continuation_wake_prepare` | `prepare_agent_continuation_wake` |
| `agent_continuation_wake_finish` | `finish_agent_continuation_wake` |
| `agent_continuation_unbind` | `unbind_agent_continuation` |
| `computer_observe` | `observe_computer` |
| `computer_control` | `control_computer` |
| `computer_save_snapshot` | `save_computer_snapshot` |
| `runtime_status` | `get_runtime_status` |
| `current_window_activity` | `read_current_window_activity` |
| `tool_manifest` | `read_tool_manifest` |
| `project_overview` | `read_project_overview` |
| `search_and_read` | `search_and_read_project_texts` |
| `git_review_summary` | `read_git_review_summary` |
| `show_changes` | `read_workspace_changes` |
| `git_commit_paths` | `commit_git_paths` |
| `git_status` | `get_git_status` |
| `git_diff_hunks` | `read_git_diff_hunks` |
| `git_log` | `read_git_log` |
| `goal_plan_sync` | `sync_goal_plan` |
| `workspace_hygiene_check` | `check_workspace_hygiene` |
| `git_restore_paths` | `restore_git_paths` |
| `job_write_input` | `write_job_input` |
| `session_shell_exec` | `execute_session_shell` |
| `session_shell_status` | `get_session_shell_status` |
| `job_terminal_continuation_bind` | `bind_job_terminal_continuation` |
| `job_terminal_continuation_state` | `get_job_terminal_continuation_state` |
| `job_terminal_continuation_prepare` | `prepare_job_terminal_continuation` |
| `job_terminal_continuation_finish` | `finish_job_terminal_continuation` |
| `job_terminal_continuation_unbind` | `unbind_job_terminal_continuation` |
| `job_tail` | `read_job_tail` |
| `lsp_status` | `get_lsp_status` |
| `document_symbols` | `list_document_symbols` |
| `document_diagnostics` | `read_document_diagnostics` |
| `hover` | `read_symbol_hover` |
| `workspace_symbols` | `list_workspace_symbols` |
| `goto_definition` | `find_definition` |
| `call_hierarchy` | `read_call_hierarchy` |
| `memory_search` | `search_memory` |
| `memory_read` | `read_memory` |
| `memory_set` | `set_memory` |
| `memory_delete` | `delete_memory` |
| `memory_scope_list` | `list_memory_scopes` |
| `memory_scope_purge` | `purge_memory_scope` |
| `runner_config_check` | `check_runner_config` |
| `runner_config_reload` | `reload_runner_config` |
| `work_result_state` | `get_work_result_state` |
| `work_result_activity_detail` | `read_work_result_activity_detail` |
| `work_result_send_message` | `send_work_result_message` |
| `changes_file_diff` | `read_changed_file_diff` |
| `session_summary` | `read_session_summary` |
| `validation_summary` | `read_validation_summary` |
| `session_discussion_summary` | `read_session_discussion_summary` |
| `session_handoff_summary` | `read_session_handoff_summary` |
| `session_handoff_state` | `get_session_handoff_state` |
| `skill_load` | `load_skill` |
| `skill_list` | `list_skills` |
| `skill_read_file` | `read_skill_file` |
| `skill_versions` | `list_skill_versions` |
| `skill_install` | `install_skill` |
| `skill_activate` | `activate_skill` |
| `skill_remove_revision` | `remove_skill_revision` |
| `ssh_resource` | `manage_ssh_resource` |

## Validation strategy

In addition to parser/catalog/follow-up tests, compare the full pre/post catalog:
all category, visibility, Direct rank/reason, effects, permissions and execution
policies must be unchanged after replacing only each tool identity. Input and
output structural schemas keep their business property names, bounds, required
fields and closedness. Dynamic suggested-call targets use the new names.

Run first-party frontend/App tests and rebuild their tracked bundles from source;
no handcrafted generated JavaScript. Exercise ordinary Runtime/MCP tests and
native wire producer/consumer tests rather than making old model names accepted
to satisfy stale fixtures. Per-tool exact naming tests and category-driven
membership checks protect future changes from depending on noun-first prefixes.

## Executed verification

Implementation source is frozen at `3768ce23ceed46215df0db0f2b2a3700c6093394`
(tree `a50db3b235a4c26265c51e7e9aaeea05a14dc916`). The subsequent evidence
update changes this document only.

The complete 181-definition comparison covers feature-gated and hidden tools:
85 identities change, 96 remain, and all 26 definition-owned Direct policies
retain their rank/reason. Metadata differs only in the canonical `name` field;
category, visibility, scope/permission, effect and execution policies are equal.
Structural schema comparison preserves property keys, required fields, bounds,
risk vocabulary and closedness. Differences are 249 exact tool-name targets and
195 projections of the shared ACK instruction's renamed recovery target.
These are descriptor counts, not claims about Host token savings or latency.

| Local verification | Result |
|---|---:|
| Default Server library | 3,155 passed; 3 existing ignored |
| Tool contracts, all features | 278 passed |
| CLI, Environment, Store, Runtime-contract and Workflow Session libraries | 1,066 passed; 3 existing ignored |
| Computer native-request/receipt focused selection | 42 passed, included in Server total |
| Workspace-checkpoints feature, actual checkpoint selection | 24 passed |
| Linux Desktop library, final complete rerun | 270 passed; 4 existing ignored |
| Shipped MCP App JavaScript tests | 328 passed |
| Frontend | 157 tests passed; typecheck and both production bundles built |
| Codex integration Python suite | 57 tests, 3 skipped |
| General Python tooling suite | 380 tests, 1 skipped |
| Workspace all-targets compilation | Passed |
| Formatting, shell/Python syntax, 21-package boundaries, Markdown links | Passed |

The first checkpoint filter (`workspace_checkpoint`) matched zero test names and
is not execution evidence. It was corrected to `checkpoint` with a required-test
assertion, yielding the 24 executed tests above. Development failures from stale
fixtures and accidental native-vocabulary substitutions were fixed at the owning
producer/consumer boundary, not by accepting old model names or weakening checks.

One same-source Desktop full run failed the existing owned-user-service tracing
fixture at `assert!(f.store.lock().is_ok())` (269 passed, 1 failed). That tracing
module was unchanged. Its targeted run and then the original full-suite command
passed without changing code, timeouts, or parallelism. Retain the initial failure
as evidence of an intermittent fixture result; its cause is not established and
this change does not claim to fix it. Existing diagnostic/dead-code warnings were
not hidden by adjusting compiler settings.

Clean dogfood Server and Runner binaries both report `3768ce23ceed, dirty=false`.
Two disposable real MCP/WebSocket smoke scripts passed:

- `e2e_job_input_ws.py`: new Direct input and exact gateway replay, short Project
  and Session references, batch read/edit and revision rejection, compound search,
  native PNG delivery, keyed input/EOF, cancellation/timeout, Server-only recovery,
  and Runner replacement. Old names are absent/rejected; the three retired HTTP
  routes return 404. No production process or configuration is involved.
- `e2e_session_continuity_ws.py`: saved notes survive a temporary Server restart,
  exact authorized Session recovery works, both model API modes run through the
  renamed ACP tools, and same-key replay/context-change fences remain intact.
  Exactly two requests reached a loopback fake model endpoint; no paid API ran.

No native Windows/macOS run, full Runner execution suite, actual V8-backed
experimental Code Mode execution, packaged release, or live Host schema refresh
is claimed. Feature-gated Code Mode tool contracts are covered by the all-features
contract tests; default workspace compilation does not prove its optional V8
runtime. The source's package versions remain unchanged; this is not a v0.5 tag.
No push, PR, production deployment or release is performed by this migration.
