use serde_json::{json, Value};

use super::common::{
    array_schema, continuation_feedback_schema, evidence_history_schema, evidence_integrity_schema,
    handoff_brief_schema, job_lifecycle_summary_schema, nullable_schema, open_object_schema,
    permission_summary_schema, schema_type, session_execution_context_schema, task_outcome_schema,
    wrapped_output_schema,
};
#[cfg(any(test, feature = "root-test-support"))]
use super::common::{
    authority_profile_schema, exploration_tool_name_schema, permission_decision_schema,
    session_hint_schema,
};
use super::files::{
    key_file_schema, path_kind_schema, project_type_schema, scan_schema, suggested_read_schema,
    top_level_entry_schema,
};
#[cfg(any(test, feature = "root-test-support"))]
use webcodex_core::runtime_contract::{
    BUILTIN_CODING_WORKFLOW_CONTRACT, BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS,
    BUILTIN_CODING_WORKFLOW_VERSION,
};

fn finish_changes_schema() -> Value {
    json!({
        "type": "object",
        "description": "show_changes output and hunk truncation metadata. The nested show_changes contract is formalized so structured recovery calls remain model-surface projectable; other closeout metadata stays additive.",
        "properties": {
            "show_changes": super::git::show_changes_output_value_schema(),
            "hunks_truncated": schema_type("boolean", "Whether the nested show_changes diff hunks were truncated by limits.")
        },
        "additionalProperties": true
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "work_on_project" => Some(work_on_project_output_schema()),
        "finish_coding_task" => Some(wrapped_output_schema(vec![
            ("goal_follow_up", super::goals::active_goal_context_schema()),
            (
                "summary_only",
                schema_type("boolean", "True only for compact summary_only output."),
            ),
            ("project", schema_type("string", "Full closeout original project input; omitted from summary_only.")),
            (
                "resolved_project",
                open_object_schema("Full closeout resolved project id, path, executor, and safe project metadata; omitted from summary_only."),
            ),
            ("session_id", schema_type("string", "Full closeout explicit task session id; omitted from summary_only.")),
            (
                "workspace_clean",
                nullable_schema("boolean", "Compact summary_only workspace cleanliness verdict; null means Git cleanliness is not applicable or was not observed."),
            ),
            (
                "workspace_conflicts",
                schema_type("integer", "Unresolved workspace conflict count."),
            ),
            (
                "hygiene_clean",
                schema_type("boolean", "Compact summary_only hygiene cleanliness verdict."),
            ),
            (
                "workspace",
                open_object_schema("Workspace cleanliness, changed file count, and warnings."),
            ),
            ("changes", finish_changes_schema()),
            (
                "validation",
                open_object_schema("Validation closeout evidence. Full closeout preserves bounded historical/resolved/unresolved evidence by stable identity and adds current_evidence for the current attempt after the latest trusted material content change. summary_only keeps final status/reason, historical and current success/failure counts, resolved/unresolved counts, current_status/stale_failure_count, and the zero-test integrity flag."),
            ),
            (
                "continuation_feedback",
                continuation_feedback_schema("Deterministic continuation feedback reused from the same projection as start/handoff. A read-only attempt summary plus validation delta over existing closeout evidence; it never re-runs validation, mutates the ledger, or replaces the closeout verdict."),
            ),
            (
                "handoff_brief",
                handoff_brief_schema("Full-closeout deterministic task handoff for a new window, new Agent, or human receiver; omitted from summary_only. It is a read-only projection over already-obtained Session, continuation, workspace, validation, Job, and guidance evidence; it is not Session replay and never restores hidden model context."),
            ),
            (
                "review_evidence",
                review_evidence_schema("Full-closeout ledger-derived non-cargo review evidence summary; omitted from summary_only after still participating internally in canonical task_outcome calculation. Counts successful read/search/diff/workspace/hygiene inspection tools. Does not include file contents, stdout/stderr, diff hunks, command text, tokens, secrets, or raw input payloads."),
            ),
            (
                "permissions",
                permission_summary_schema("Full-closeout deterministic bounded permission decision summary from the session ledger; omitted from summary_only. Counts high-risk auto-approved tools only; never includes stdout/stderr, env, tokens, secrets, or raw input content."),
            ),
            (
                "tool_failures",
                open_object_schema("Pre-declared result-expectation classification from the session ledger. Default success remains fail-closed; matched negative/observation outcomes are expected evidence. unexpected_count remains immutable raw failed-ToolCall evidence; non_actionable_unexpected_count identifies request-scoped validation evidence assertion failures, resolved/stale validation failures, or structurally proven not-started/non-effect attempts; actionable_unexpected_count is the conservative current blocker projection. Expectation mismatches and unexpected successes remain separate integrity evidence. Compact output includes counts only."),
            ),
            (
                "hygiene",
                nullable_schema("object", "workspace_hygiene_check output when requested; null otherwise."),
            ),
            (
                "handoff",
                nullable_schema("object", "session_handoff_summary output when requested; null otherwise."),
            ),
            (
                "jobs",
                job_lifecycle_summary_schema("Bounded job lifecycle summary for finish. active_jobs_present is emitted only for blocking_active_count > 0; stop_requested-only jobs use nonblocking jobs_terminal_pending. Never includes stdout/stderr or command text."),
            ),
            (
                "final_warnings",
                array_schema(open_object_schema("Finish warning."), "Bounded finish warnings."),
            ),
            (
                "facts",
                open_object_schema("Full-closeout canonical provenance facts; omitted from summary_only after contributing to the shared canonical outcome calculation."),
            ),
            (
                "hard_blockers",
                array_schema(schema_type("string", "Deterministic blocker identifier."), "Only confirmed command/safety/consistency blockers."),
            ),
            (
                "advisories",
                array_schema(schema_type("string", "Non-blocking advisory identifier."), "Context-dependent facts for Agent judgment."),
            ),
            (
                "warnings",
                array_schema(open_object_schema("Compact finish warning."), "Bounded compact summary_only warnings."),
            ),
            (
                "task_outcome",
                task_outcome_schema("Final task completion outcome with status pass/warn/fail, blocking, and task-only reasons. Resolved validation history and expected-failure audit metadata do not lower this status."),
            ),
            (
                "evidence_history",
                evidence_history_schema("Full-closeout validation evidence-history status; omitted from summary_only, whose task_outcome and compact validation state remain decision-complete."),
            ),
            (
                "evidence_integrity",
                evidence_integrity_schema("Expected-failure and validation-evidence integrity status: clean, warning, or error, with bounded reason identifiers."),
            ),
            (
                "informational_notes",
                array_schema(
                    schema_type("string", "Completed-state informational note."),
                    "Full-closeout bounded completed-state facts, omitted from summary_only and separate from executable suggested_next_actions.",
                ),
            ),
            (
                "presentation",
                open_object_schema("Optional parser-ready presentation follow-up. Present only when this exact Workflow Session has a startup Git baseline, durable successful first-class Edit evidence, and the current final workspace still differs from that baseline; contains exactly one present_work_result suggested_call and is preserved in full and summary_only closeout."),
            ),
            (
                "suggested_next_actions",
                array_schema(schema_type("string", "Short suggested action."), "Top-level full and summary_only final closeout actions derived from task outcome and evidence integrity. Preserves bounded finish actions and never duplicates the machine-readable presentation call."),
            ),
        ])),
        _ => None,
    }
}

#[cfg(any(test, feature = "root-test-support"))]
pub(super) fn coding_workflow_diagnostic_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "success": {"type": "boolean"},
            "output": {
                "oneOf": [
                    startup_brief_output_schema("minimal"),
                    startup_brief_output_schema("standard"),
                    full_startup_output_schema(),
                ]
            },
            "error": {
                "anyOf": [
                    {"type": "string"},
                    {"type": "null"}
                ]
            }
        },
        "required": ["success"],
        "additionalProperties": false,
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_brief_output_schema(detail: &str) -> Value {
    let mut schema = startup_brief_schema(detail);
    add_startup_model_metadata(&mut schema);
    schema
}

#[cfg(any(test, feature = "root-test-support"))]
fn add_startup_model_metadata(schema: &mut Value) {
    let properties = schema
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .expect("startup output schema properties");
    properties.insert("session_hint".to_string(), session_hint_schema());
    properties.insert("permission".to_string(), permission_decision_schema());
}

fn startup_extensions_schema() -> Value {
    let annotations = json!({
        "type": "object",
        "properties": {
            "readOnlyHint": {"type": "boolean"},
            "destructiveHint": {"type": "boolean"},
            "idempotentHint": {"type": "boolean"},
            "openWorldHint": {"type": "boolean"}
        },
        "additionalProperties": false
    });
    let skill_entry = json!({
        "type": "object",
        "properties": {
            "skill_id": {"type": "string", "pattern": "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"},
            "name": {"type": "string"},
            "description": {"type": "string"},
            "source_scope": {"type": "string"},
            "trust": {"type": "string"},
            "name_conflict": {"type": "boolean"}
        },
        "required": ["skill_id", "name", "description", "source_scope", "trust", "name_conflict"],
        "additionalProperties": false
    });
    let plugin_entry = json!({
        "type": "object",
        "properties": {
            "plugin": {"type": "string"},
            "name": {"type": "string"},
            "tool": {"type": "string"},
            "title": {"type": "string"},
            "description": {"type": "string"},
            "annotations": annotations
        },
        "required": ["plugin", "name", "tool"],
        "additionalProperties": false
    });
    let family = |entry: Value, revision_pattern: &str| {
        json!({
            "type": "object",
            "properties": {
                "status": {"type": "string", "enum": ["available", "unavailable"]},
                "reason_code": {"type": "string"},
                "catalog_revision": {"type": "string", "pattern": revision_pattern},
                "total_count": {"type": "integer", "minimum": 0},
                "returned_count": {"type": "integer", "minimum": 0},
                "truncated": {"type": "boolean"},
                "entries": {"type": "array", "items": entry},
                "discovery_hint": {"type": "string"}
            },
            "required": ["status", "total_count", "returned_count", "truncated", "entries"],
            "additionalProperties": false
        })
    };
    json!({
        "type": "object",
        "description": "Bounded selection-only extension metadata. It grants zero additional authority and contains no Skill body, Plugin schema, binding, provider path, process, or execution data.",
        "properties": {
            "skills": family(skill_entry, "^wc_skillcat_[A-Za-z0-9_-]{43}$"),
            "plugins": family(plugin_entry, "^wc_plugcat_[A-Za-z0-9_-]{43}$")
        },
        "required": ["skills", "plugins"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_brief_schema(detail: &str) -> Value {
    json!({
        "type": "object",
        "description": "Deterministic, bounded model-facing coding startup brief shared by MCP, REST, and GPT Actions.",
        "properties": {
            "detail": {"type": "string", "const": detail},
            "session": startup_session_schema(),
            "project": startup_project_schema(),
            "project_resolution": project_resolution_schema(),
            "workspace": startup_workspace_schema(),
            "workflow": startup_workflow_schema(),
            "instructions": startup_instructions_schema(),
            "continuation": startup_continuation_schema(detail),
            "semantic_navigation": startup_semantic_navigation_schema(),
            "extensions": startup_extensions_schema(),
            "repository": startup_repository_schema(),
            "blockers": startup_issue_list_schema(true),
            "warnings": startup_issue_list_schema(false),
            "startup_verdict": startup_verdict_schema(),
            "deterministic": {"type": "boolean", "const": true},
            "llm_summary": {"type": "boolean", "const": false}
        },
        "required": [
            "detail",
            "session",
            "project",
            "project_resolution",
            "workspace",
            "workflow",
            "instructions",
            "continuation",
            "semantic_navigation",
            "repository",
            "blockers",
            "warnings",
            "startup_verdict",
            "deterministic",
            "llm_summary"
        ],
        "additionalProperties": false,
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn full_startup_output_schema() -> Value {
    let mut schema = json!({
        "type": "object",
        "description": "Full diagnostic startup output. Preserves the existing operator-facing blocks and embeds the shared model-facing startup_brief.",
        "properties": {
            "detail": {"type": "string", "const": "full"},
            "project": schema_type("string", "Original project input."),
            "project_resolution": project_resolution_schema(),
            "resolved_project": open_object_schema("Resolved project id, absolute execution path, executor, and diagnostic project metadata."),
            "session": open_object_schema("Full Workflow Session, guard, capability, context-refresh, exact-binding, and explicitly resumed session diagnostics."),
            "runtime_status": open_object_schema("Full runtime status diagnostics."),
            "connection_state": open_object_schema("Full layered connection diagnostics."),
            "authority": authority_profile_schema("Canonical authority profile for this task."),
            "rules": open_object_schema("Full deterministic rules source summary."),
            "git": open_object_schema("Full bounded Git/worktree summary including recent commits."),
            "semantic_navigation": semantic_navigation_schema(),
            "tool_manifest": open_object_schema("Bounded compact tool manifest."),
            "recommended_flow": open_object_schema("Deterministic recommended tool groups."),
            "startup_verdict": open_object_schema("Legacy full diagnostic startup checks and suggested actions."),
            "continuation_feedback": continuation_feedback_schema("Complete bounded continuation_feedback projection retained for full diagnostics."),
            "warnings": array_schema(open_object_schema("Full diagnostic startup warning."), "Bounded diagnostic warnings."),
            "startup_brief": startup_brief_schema("full"),
            "deterministic": {"type": "boolean", "const": true},
            "llm_summary": {"type": "boolean", "const": false}
        },
        "required": [
            "detail",
            "project",
            "project_resolution",
            "resolved_project",
            "session",
            "runtime_status",
            "connection_state",
            "authority",
            "rules",
            "git",
            "semantic_navigation",
            "tool_manifest",
            "recommended_flow",
            "startup_verdict",
            "continuation_feedback",
            "warnings",
            "startup_brief",
            "deterministic",
            "llm_summary"
        ],
        "additionalProperties": false,
    });
    add_startup_model_metadata(&mut schema);
    schema
}

fn project_resolution_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bounded, path-free project source resolution metadata.",
        "properties": {
            "source": {
                "type": "string",
                "enum": ["project", "path", "managed_worktree"]
            },
            "outcome": {
                "type": "string",
                "enum": [
                    "resolved_existing_project",
                    "reused_existing_registration",
                    "auto_registered",
                    "managed_worktree_created",
                    "managed_worktree_recovered"
                ]
            },
            "resolved_project": {
                "type": "string",
                "description": "Full runtime project id; never an absolute path."
            },
            "registered": {
                "type": "boolean",
                "description": "True only when this call permanently created a registration."
            },
            "worktree": {
                "type": "object",
                "description": "Path-free managed-worktree bootstrap metadata; present only for managed_worktree source resolution.",
                "properties": {
                    "managed": {"type": "boolean", "const": true},
                    "base_ref": {"type": "string"},
                    "base_sha": {"type": "string", "pattern": "^[0-9A-Fa-f]{40}([0-9A-Fa-f]{24})?$"},
                    "source_dirty": {"type": "boolean"}
                },
                "required": ["managed", "base_ref", "base_sha", "source_dirty"],
                "additionalProperties": false
            }
        },
        "required": ["source", "outcome", "resolved_project", "registered"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_session_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": {"type": "string", "pattern": "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"},
            "mode": {"type": "string", "enum": ["normal", "read_only"]},
            "execution_context": session_execution_context_schema(
                "Persistent execution defaults currently stored for this Workflow Session."
            ),
            "continuation": {"type": "string", "enum": ["created", "continued", "resumed_explicitly"]},
            "reused": {"type": "boolean"},
            "resume_requested": {"type": "boolean"},
            "explicit_resume_required_for_continuation": {"type": "boolean"}
        },
        "required": [
            "session_id",
            "mode",
            "execution_context",
            "continuation",
            "reused",
            "resume_requested",
            "explicit_resume_required_for_continuation"
        ],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_project_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "requested": {"type": "string"},
            "resolved_id": {"type": "string"},
            "project_ref": {"type": "string", "pattern": "^~p[1-9][0-9]*$"},
            "repository_identity": {
                "type": "string",
                "pattern": "^repository:v1:[0-9a-f]{64}$",
                "description": "Domain-separated identity of the currently resolved canonical repository root; never contains the path."
            },
            "canonical_repository_root_matches": {
                "anyOf": [
                    {"type": "boolean"},
                    {"type": "null"}
                ],
                "description": "true means the Session root identity is proved to match; null means this recovery path did not perform or cannot prove the comparison."
            }
        },
        "required": [
            "requested",
            "resolved_id",
            "repository_identity",
            "canonical_repository_root_matches"
        ],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_workspace_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["available", "clean", "dirty", "blocked", "unavailable"]},
            "git": {
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["clean", "dirty", "conflicted", "not_applicable", "unavailable"]},
                    "reason_code": nullable_schema("string", "Stable Git-state reason such as non_git_project or git_unavailable.")
                },
                "required": ["status", "reason_code"],
                "additionalProperties": false
            },
            "git_available": nullable_schema("boolean", "Whether bounded Git inspection was available."),
            "branch": nullable_schema("string", "Current branch when observed."),
            "head": nullable_schema("string", "Current full HEAD commit when observed."),
            "clean": nullable_schema("boolean", "Whether the worktree is clean when observed."),
            "conflicts": {"type": "integer", "minimum": 0},
            "modified": {"type": "integer", "minimum": 0},
            "untracked": {"type": "integer", "minimum": 0},
            "staged": {"type": "integer", "minimum": 0},
            "ahead": nullable_schema("integer", "Ahead count when a reliable source is available."),
            "behind": nullable_schema("integer", "Behind count when a reliable source is available.")
        },
        "required": [
            "status",
            "git",
            "git_available",
            "branch",
            "head",
            "clean",
            "conflicts",
            "modified",
            "untracked",
            "staged",
            "ahead",
            "behind"
        ],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_workflow_schema() -> Value {
    json!({
        "type": "object",
        "description": "WebCodex-owned shared workflow, selected tool strategy and optional review role. Separate from project instructions and Session authority.",
        "properties": {
            "contract": {"type": "string", "const": BUILTIN_CODING_WORKFLOW_CONTRACT},
            "version": {"type": "integer", "const": BUILTIN_CODING_WORKFLOW_VERSION},
            "authority": {"type": "string", "const": "model_guidance_only"},
            "role_selection": {"type": "string", "maxLength": 240},
            "guidance": {
                "type": "array",
                "description": "Default behavior for every coding/review task, including tasks without a named role. Guidance never grants authority.",
                "minItems": 1,
                "maxItems": BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS,
                "items": {"type": "string", "maxLength": 320}
            },
            "tool_strategy": {
                "type": "object",
                "description": "Only the selected request-local tool strategy. Model guidance, never tool admission, authority, or durable Session state.",
                "properties": {
                    "profile": crate::schema_generation::typed_host_schema::<crate::tool_inputs::CodingGuidanceProfile>(),
                    "guidance": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS,
                        "items": {"type": "string", "maxLength": 320}
                    }
                },
                "required": ["profile", "guidance"],
                "additionalProperties": false
            },
            "model_protocol": {
                "type": "object",
                "description": "Shared model-invocation guidance. It is not Session state, authority, or execution policy.",
                "properties": {
                    "handoff_recovery": {"type": "string", "maxLength": 720},
                    "session_recording": {"type": "string", "maxLength": 720},
                    "session_message_ack": {"type": "string", "maxLength": 720},
                    "session_message_resolution": {"type": "string", "maxLength": 480},
                    "context_sidecar": {"type": "string", "maxLength": 320},
                    "runner_targeting": {"type": "string", "maxLength": 320},
                    "persistent_shell": {"type": "string", "maxLength": 320},
                    "goal_workflow": {"type": "string", "maxLength": 720},
                    "goal_continuation": {"type": "string", "maxLength": 720},
                    "goal_checkpoint": {"type": "string", "maxLength": 480},
                    "normal_closeout": {"type": "string", "maxLength": 480}
                },
                "required": [
                    "handoff_recovery",
                    "session_recording",
                    "session_message_ack",
                    "session_message_resolution",
                    "context_sidecar",
                    "runner_targeting",
                    "persistent_shell",
                    "goal_workflow",
                    "goal_continuation",
                    "goal_checkpoint",
                    "normal_closeout"
                ],
                "additionalProperties": false
            },
            "roles": {
                "type": "object",
                "description": "Optional named review behavior. Ordinary implementation uses shared guidance and the selected tool strategy.",
                "properties": {
                    "independent_review": startup_workflow_role_schema()
                },
                "required": ["independent_review"],
                "additionalProperties": false
            }
        },
        "required": ["contract", "version", "authority", "role_selection", "guidance", "tool_strategy", "model_protocol", "roles"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_workflow_role_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "purpose": {"type": "string", "maxLength": 240},
            "guidance": {
                "type": "array",
                "minItems": 1,
                "maxItems": BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS,
                "items": {"type": "string", "maxLength": 320}
            }
        },
        "required": ["purpose", "guidance"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_instructions_schema() -> Value {
    json!({
        "type": "object",
        "description": "Runner-configured instructions followed by project-local repository instructions. Both are model guidance only and are separate from the WebCodex built-in workflow.",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["loaded", "reused", "changed", "not_found", "unavailable"]
            },
            "sources": {
                "type": "array",
                "maxItems": 21,
                "items": startup_instruction_source_schema(),
                "description": "Deterministic Runner-global sources followed by fixed project-local repository-rule sources."
            },
            "changed_sources": {
                "type": "array",
                "uniqueItems": true,
                // Old and new Runner identities (16 + 16), plus five fixed Project sources.
                "maxItems": 37,
                "items": instruction_source_path_schema()
            },
            "content_included": {"type": "boolean"},
            "truncated": {"type": "boolean"},
            "total_chars": {"type": "integer", "minimum": 0, "maximum": 32768}
        },
        "required": [
            "status",
            "sources",
            "changed_sources",
            "content_included",
            "truncated",
            "total_chars"
        ],
        "additionalProperties": false
    })
}

fn instruction_source_path_schema() -> Value {
    json!({
        "anyOf": [
            {
                "type": "string",
                "enum": [
                    "AGENTS.md",
                    "agents.md",
                    "CLAUDE.md",
                    ".codex/AGENTS.md",
                    ".github/copilot-instructions.md"
                ]
            },
            {
                "type": "string",
                "pattern": "^runner/[0-9]+/[^/\\\\\\u0000]{1,255}$"
            }
        ]
    })
}

fn startup_instruction_source_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "source_scope": {"type": "string", "enum": ["runner", "project"]},
            "path": instruction_source_path_schema(),
            "fingerprint": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
            "truncated": {"type": "boolean"},
            "headings": {
                "type": "array",
                "maxItems": 6,
                "items": {"type": "string", "maxLength": 160}
            },
            "content": {
                "anyOf": [
                    {
                        "type": "string",
                        "maxLength": 10240,
                        "description": "Bounded repository-rule body only when loaded or changed."
                    },
                    {"type": "null"}
                ]
            },
            "read_more": {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "start_line": {"type": "integer", "minimum": 1},
                            "limit": {"type": "integer", "minimum": 1, "maximum": 400}
                        },
                        "required": ["path", "start_line", "limit"],
                        "additionalProperties": false
                    },
                    {"type": "null"}
                ]
            }
        },
        "required": ["source_scope", "path", "fingerprint", "truncated", "headings", "content", "read_more"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_continuation_schema(detail: &str) -> Value {
    let exploration_limit = if detail == "minimal" { 3 } else { 12 };
    json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["available", "not_applicable", "unknown"]},
            "reason_code": nullable_schema("string", "Stable reason when continuation is not available."),
            "instruction": {
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["available", "not_observed"]},
                    "excerpt": {
                        "anyOf": [
                            {"type": "string", "maxLength": 768},
                            {"type": "null"}
                        ]
                    },
                    "truncated": {"type": "boolean"}
                },
                "required": ["status", "excerpt", "truncated"],
                "additionalProperties": false
            },
            "outcome": {
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["in_progress", "blocked", "clean", "unknown"]},
                    "reason_codes": {
                        "type": "array",
                        "maxItems": 8,
                        "items": {"type": "string", "maxLength": 96},
                        "description": "Bounded outcome reasons."
                    }
                },
                "required": ["status", "reason_codes"],
                "additionalProperties": false
            },
            "changed_paths": bounded_list_schema(
                json!({"type": "string", "maxLength": 192}),
                20
            ),
            "exploration": {
                "type": "object",
                "properties": {
                    "paths": bounded_list_schema(
                        json!({"type": "string", "maxLength": 512}),
                        exploration_limit
                    ),
                    "read_count": {"type": "integer", "minimum": 0},
                    "search_count": {"type": "integer", "minimum": 0},
                    "navigation_count": {"type": "integer", "minimum": 0},
                    "latest_tool": exploration_tool_name_schema(),
                    "complete": {"type": "boolean"}
                },
                "required": [
                    "paths",
                    "read_count",
                    "search_count",
                    "navigation_count",
                    "latest_tool",
                    "complete"
                ],
                "additionalProperties": false
            },
            "validation": startup_validation_schema(),
            "jobs": {
                "type": "object",
                "properties": {
                    "active_count": {"type": "integer", "minimum": 0},
                    "blocking_active_count": {"type": "integer", "minimum": 0},
                    "nonblocking_active_count": {"type": "integer", "minimum": 0},
                    "recovering_count": {"type": "integer", "minimum": 0},
                    "terminal_pending_count": {"type": "integer", "minimum": 0},
                    "latest_status": {"type": "string"}
                },
                "required": [
                    "active_count",
                    "blocking_active_count",
                    "nonblocking_active_count",
                    "recovering_count",
                    "terminal_pending_count",
                    "latest_status"
                ],
                "additionalProperties": false
            },
            "open_guidance": {
                "type": "object",
                "properties": {
                    "count": {"type": "integer", "minimum": 0},
                    "risk_count": {"type": "integer", "minimum": 0},
                    "todo_count": {"type": "integer", "minimum": 0},
                    "latest_kind": nullable_schema("string", "Latest open guidance kind when observed.")
                },
                "required": ["count", "risk_count", "todo_count", "latest_kind"],
                "additionalProperties": false
            },
            "suggested_next_actions": bounded_list_schema(
                json!({"type": "string", "maxLength": 384}),
                5
            )
        },
        "required": [
            "status",
            "reason_code",
            "instruction",
            "outcome",
            "changed_paths",
            "exploration",
            "validation",
            "jobs",
            "open_guidance",
            "suggested_next_actions"
        ],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_validation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "latest_status": {
                "type": "string",
                "enum": ["passed", "failed", "inconclusive", "expected", "not_run", "unknown", "unavailable"]
            },
            "open_failures": bounded_list_schema(startup_failure_schema(), 10),
            "delta": {
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["available", "unavailable"]},
                    "reason_code": nullable_schema("string", "Stable validation comparison reason."),
                    "new_failures": bounded_list_schema(startup_failure_schema(), 10),
                    "resolved_failures": bounded_list_schema(startup_failure_schema(), 10),
                    "still_failing": bounded_list_schema(startup_failure_schema(), 10)
                },
                "required": [
                    "status",
                    "reason_code",
                    "new_failures",
                    "resolved_failures",
                    "still_failing"
                ],
                "additionalProperties": false
            }
        },
        "required": ["latest_status", "open_failures", "delta"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_failure_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": {"type": "string", "enum": ["test", "diagnostic", "unknown"]},
            "name": {"type": "string", "maxLength": 160},
            "file": {
                "anyOf": [
                    {"type": "string", "maxLength": 160},
                    {"type": "null"}
                ]
            },
            "line": nullable_schema("integer", "Source line when available.")
        },
        "required": ["kind", "name", "file", "line"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn bounded_list_schema(items: Value, max_items: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "items": {"type": "array", "maxItems": max_items, "items": items},
            "total": {"type": "integer", "minimum": 0},
            "returned": {"type": "integer", "minimum": 0, "maximum": max_items},
            "truncated": {"type": "boolean"}
        },
        "required": ["items", "total", "returned", "truncated"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_semantic_navigation_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supported": {"type": "boolean", "description": "Whether LSP read-only navigation is advertised for this project."},
            "status": {
                "type": "string",
                "enum": [
                    "running",
                    "available",
                    "initializing",
                    "crashed",
                    "unavailable",
                    "not_applicable",
                    "agent_unavailable",
                    "agent_capability_unavailable",
                    "probe_timeout",
                    "probe_failed"
                ]
            },
            "available": nullable_schema("boolean", "Observed semantic-navigation availability. Null means the bounded startup status probe timed out or failed without an availability observation; this is advisory, not unavailability."),
            "provider": nullable_schema("string", "Semantic provider when applicable."),
            "capability": nullable_schema("string", "Bounded advertised capability summary."),
            "reason_code": nullable_schema("string", "Stable semantic-navigation reason.")
        },
        "required": ["supported", "status", "available", "provider", "capability", "reason_code"],
        "additionalProperties": false
    })
}

fn startup_repository_schema() -> Value {
    let bounded_repository_list = |items: Value, max_items: usize| {
        json!({
            "type": "object",
            "properties": {
                "items": {"type": "array", "maxItems": max_items, "items": items},
                "total": {"type": "integer", "minimum": 0},
                "returned": {"type": "integer", "minimum": 0, "maximum": max_items},
                "truncated": {"type": "boolean"}
            },
            "required": ["items", "total", "returned", "truncated"],
            "additionalProperties": false
        })
    };
    let bounded_roots = json!({
        "type": "object",
        "properties": {
            "source": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "tests": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "docs": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "examples": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "scripts": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "ci": bounded_repository_list(json!({"type": "string", "maxLength": 192}), 8),
            "classification_basis": schema_type("string", "Classification basis; conventional_directory_name.")
        },
        "required": ["source", "tests", "docs", "examples", "scripts", "ci", "classification_basis"],
        "additionalProperties": false
    });
    let scan = scan_schema();
    json!({
        "type": "object",
        "description": "Deterministic repository structure metadata. Reads directory entries, file types, and the git tracked index only; never reads ordinary file bodies, executes project code, follows symlinks, scans protected/sensitive/build/cache paths, or returns absolute roots or shell output.",
        "properties": {
            "status": {"type": "string", "enum": ["available", "unavailable"]},
            "reason_code": nullable_schema("string", "Stable reason when unavailable."),
            "project_types": bounded_repository_list(project_type_schema(), 8),
            "manifests": bounded_repository_list(path_kind_schema("Detected build or package manifest."), 12),
            "key_files": bounded_repository_list(key_file_schema(), 16),
            "roots": bounded_roots,
            "top_level": bounded_repository_list(top_level_entry_schema(), 24),
            "suggested_next_reads": bounded_repository_list(suggested_read_schema(), 8),
            "scan": scan,
            "warnings": {
                "type": "array",
                "uniqueItems": true,
                "maxItems": 8,
                "items": {
                    "type": "string",
                    "enum": ["symlinks_skipped", "unreadable_entries_skipped", "non_utf8_paths_skipped"]
                }
            }
        },
        "required": ["status", "reason_code"],
        "allOf": [
            {
                "if": {
                    "properties": {"status": {"const": "available"}},
                    "required": ["status"]
                },
                "then": {
                    "required": [
                        "project_types",
                        "manifests",
                        "key_files",
                        "roots",
                        "top_level",
                        "suggested_next_reads",
                        "scan",
                        "warnings"
                    ]
                }
            }
        ],
        "additionalProperties": false
    })
}

fn startup_issue_list_schema(blockers: bool) -> Value {
    let values = if blockers {
        json!([
            "workspace_conflicts",
            "project_unavailable",
            "write_scope_missing",
            "runner_unavailable",
            "runtime_unavailable",
            "active_jobs_blocking"
        ])
    } else {
        json!([
            "dirty_worktree",
            "git_unavailable",
            "semantic_navigation_unavailable",
            "rules_unavailable",
            "repository_overview_unavailable",
            "runtime_status_unavailable",
            "active_jobs_present"
        ])
    };
    json!({
        "type": "array",
        "uniqueItems": true,
        "maxItems": 8,
        "items": {"type": "string", "enum": values}
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn startup_verdict_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["pass", "warn", "fail"]},
            "blocking": {"type": "boolean"},
            "suggested_next_actions": {
                "type": "array",
                "maxItems": 5,
                "items": {"type": "string", "maxLength": 384}
            }
        },
        "required": ["status", "blocking", "suggested_next_actions"],
        "additionalProperties": false
    })
}

#[cfg(any(test, feature = "root-test-support"))]
fn semantic_navigation_schema() -> Value {
    json!({
        "type": "object",
        "description": "Always-present bounded Rust/Go semantic-navigation capability summary. Derived only from a typed agent status probe; never contains transport envelopes, process output, paths, environment variables, or symbol/location data.",
        "additionalProperties": false,
        "properties": {
            "supported": schema_type("boolean", "True when the Project is Runner-backed, the owning Runner is connected, and it advertises lsp_read_only_navigation."),
            "available": nullable_schema("boolean", "Observed semantic-navigation availability. True means supported Rust/Go navigation has an available executable or an existing running/initializing server slot; false is a positive unavailable observation; null means a bounded startup status probe timed out or failed before availability could be observed; this is advisory, not unavailability."),
            "recommended": schema_type("boolean", "True only for available or running status."),
            "status": {
                "type": "string",
                "enum": [
                    "running",
                    "available",
                    "initializing",
                    "crashed",
                    "unavailable",
                    "not_applicable",
                    "agent_unavailable",
                    "agent_capability_unavailable",
                    "probe_timeout",
                    "probe_failed"
                ]
            },
            "language": {
                "anyOf": [
                    { "type": "string", "enum": ["rust", "go"] },
                    { "type": "null" }
                ]
            },
            "server": {
                "anyOf": [
                    { "type": "string", "enum": ["rust-analyzer", "gopls"] },
                    { "type": "null" }
                ]
            },
            "position_encoding": {
                "anyOf": [
                    { "type": "string", "enum": ["utf-8", "utf-16", "utf-32"] },
                    { "type": "null" }
                ]
            },
            "tools": {
                "type": "array",
                "maxItems": 7,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["lsp_status", "document_symbols", "goto_definition", "find_references", "document_diagnostics", "hover", "workspace_symbols"]
                }
            },
            "preferred_flow": {
                "type": "array",
                "maxItems": 6,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["document_symbols", "goto_definition", "find_references", "hover", "read_files", "search_project_texts"]
                }
            },
            "limitations": {
                "type": "array",
                "maxItems": 5,
                "uniqueItems": true,
                "items": {
                    "type": "string",
                    "enum": ["rust_only", "go_only", "read_only", "workspace_only", "no_dependency_navigation", "full_text_sync_only"]
                }
            },
            "reason_code": {
                "anyOf": [
                    {
                        "type": "string",
                        "enum": [
                            "project_not_agent_backed",
                            "rust_not_detected",
                            "agent_not_connected",
                            "lsp_capability_not_advertised",
                            "server_crashed",
                            "server_unavailable",
                            "status_probe_timed_out",
                            "status_probe_failed",
                            "malformed_agent_result"
                        ]
                    },
                    { "type": "null" }
                ]
            }
        },
        "required": [
            "supported",
            "available",
            "recommended",
            "status",
            "language",
            "server",
            "position_encoding",
            "tools",
            "preferred_flow",
            "limitations",
            "reason_code"
        ]
    })
}

fn work_on_project_instruction_source_schema() -> Value {
    let mut schema = startup_instruction_source_schema();
    schema["required"] = json!(["source_scope", "path", "fingerprint"]);
    schema
}

/// Compact startup projection for `work_on_project`. Carries only the fields a
/// coding model immediately needs after starting or continuing a task. It never
/// returns the full runtime/connection/authority/binding/manifest diagnostics
/// and never fabricates empty state when the underlying startup result omitted
/// a field.
fn work_on_project_output_schema() -> Value {
    let compact_workspace = json!({
        "type": "object",
        "description": "Sparse workspace state. status is always present; null/default facts are omitted, branch/head are included when observed, git_available is emitted only when false, and conflicts only when non-zero.",
        "properties": {
            "status": {"type": "string", "enum": ["available", "clean", "dirty", "blocked", "unavailable"]},
            "git": {
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["clean", "dirty", "conflicted", "not_applicable", "unavailable"]},
                    "reason_code": nullable_schema("string", "Stable Git-state reason such as non_git_project or git_unavailable.")
                },
                "required": ["status", "reason_code"],
                "additionalProperties": false
            },
            "git_available": nullable_schema("boolean", "Emitted when bounded Git inspection is explicitly unavailable; omission means no exceptional Git-unavailable fact."),
            "branch": nullable_schema("string", "Current branch when observed."),
            "head": nullable_schema("string", "Current full HEAD commit when observed."),
            "clean": nullable_schema("boolean", "Legacy compatibility field; normal clean/dirty state is represented by status and may omit this field."),
            "conflicts": {"type": "integer", "minimum": 1}
        },
        "required": ["status", "git"],
        "additionalProperties": true
    });
    let compact_instructions = json!({
        "type": "object",
        "description": "Compact Runner-global plus project-local instruction projection, separate from the WebCodex built-in workflow. status reports Workflow Session delta; content_included reports this call's caller-explicit model-facing body projection. False/null/empty body-projection defaults are omitted.",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["loaded", "reused", "changed", "not_found", "unavailable"]
            },
            "sources": {
                "type": "array",
                "maxItems": 21,
                "items": work_on_project_instruction_source_schema(),
                "description": "Runner-global sources precede project-local sources. source_scope/path/fingerprint are always present; false/null/empty body-projection defaults are omitted."
            },
            "changed_sources": {
                "type": "array",
                "uniqueItems": true,
                // Old and new Runner identities (16 + 16), plus five fixed Project sources.
                "maxItems": 37,
                "items": instruction_source_path_schema()
            },
            "content_included": {"type": "boolean", "description": "Emitted only when bounded instruction bodies are included for this call; omission means false. This is independent of status=reused."},
            "truncated": {"type": "boolean", "description": "Emitted only when true."},
            "total_chars": {"type": "integer", "minimum": 1, "maximum": 32768, "description": "Emitted only with truncated=true to quantify the observed instruction extent."}
        },
        "required": ["status", "sources"],
        "additionalProperties": true
    });
    let compact_semantic_navigation = json!({
        "type": "object",
        "properties": {
            "supported": {"type": "boolean"},
            "available": nullable_schema("boolean", "Observed semantic-navigation availability. Null means the bounded startup status probe timed out or failed without an availability observation; this does not lower coding readiness."),
            "status": {
                "type": "string",
                "enum": [
                    "running",
                    "available",
                    "initializing",
                    "crashed",
                    "unavailable",
                    "not_applicable",
                    "agent_unavailable",
                    "agent_capability_unavailable",
                    "probe_timeout",
                    "probe_failed"
                ]
            },
            "capability": nullable_schema("string", "Bounded advertised capability summary."),
            "reason_code": nullable_schema("string", "Stable semantic-navigation reason.")
        },
        "required": ["supported", "available", "status", "capability", "reason_code"],
        "additionalProperties": true
    });
    let compact_jobs = json!({
        "type": "object",
        "description": "Sparse noteworthy Job state. The whole object is omitted when all lifecycle counts are zero and no latest status was observed; inside the object zero counts and latest_status=not_observed are omitted.",
        "properties": {
            "active_count": {"type": "integer", "minimum": 1},
            "blocking_active_count": {"type": "integer", "minimum": 1},
            "nonblocking_active_count": {"type": "integer", "minimum": 1},
            "recovering_count": {"type": "integer", "minimum": 1},
            "terminal_pending_count": {"type": "integer", "minimum": 1},
            "latest_status": {"type": "string"}
        },
        "minProperties": 1,
        "additionalProperties": true
    });
    let compact_repository = startup_repository_schema();
    let output_properties = vec![
        (
            "session_id",
            schema_type("string", "Explicit Workflow Session id for exact continuation or recording on later calls."),
        ),
        (
            "project",
            schema_type("string", "Project selector used to start or resume this task. For direct Project input this preserves the caller's accepted selector, including a Server-issued project_ref; for Runner path input it is the resolved canonical runtime Project id."),
        ),
        (
            "resolved_project",
            schema_type("string", "Resolved full runtime project id from the permission check and exact project resolution."),
        ),
        (
            "project_ref",
            schema_type("string", "Server-issued short Project selector scoped to the authenticated caller. Convenience only: every use re-resolves and re-authorizes the canonical Runtime Project."),
        ),
        (
            "project_resolution",
            {
                let mut schema = project_resolution_schema();
                schema["description"] = json!("Non-default project-source resolution metadata. Omitted when an ordinary project input resolves to an existing registration without mutation.");
                schema
            },
        ),
        (
            "continuation",
            schema_type("string", "created, continued, or resumed_explicitly."),
        ),
        (
            "goal_context",
            super::goals::active_goal_context_schema(),
        ),
        (
            "execution_context",
            session_execution_context_schema("Persistent execution defaults currently stored for this Workflow Session. Omitted when empty."),
        ),
        (
            "readiness",
            json!({
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["pass", "warn", "fail"]},
                    "blocking": {"type": "boolean"}
                },
                "required": ["status", "blocking"],
                "additionalProperties": false
            }),
        ),
        (
            "workspace",
            compact_workspace,
        ),
        (
            "worktree",
            json!({
                "type": "object",
                "description": "Compact path-free managed-worktree source projection. Omitted for ordinary checkout mode.",
                "properties": {
                    "managed": {"type": "boolean", "const": true},
                    "base_ref": {"type": "string"},
                    "base_sha": {"type": "string", "pattern": "^[0-9A-Fa-f]{40}([0-9A-Fa-f]{24})?$"},
                    "source_dirty": {"type": "boolean"}
                },
                "required": ["managed", "base_ref", "base_sha", "source_dirty"],
                "additionalProperties": false
            }),
        ),
        (
            "repository",
            {
                let mut schema = compact_repository;
                schema["description"] = json!("Unexpected or noteworthy repository-overview state. Omitted for work_on_project's normal intentional no-overview path.");
                schema
            },
        ),
        ("instructions", compact_instructions),
        ("semantic_navigation", compact_semantic_navigation),
        ("extensions", startup_extensions_schema()),
        ("jobs", compact_jobs),
        (
            "blockers",
            {
                let mut schema = startup_issue_list_schema(true);
                schema["description"] = json!("Blocking startup issues; omitted when empty.");
                schema
            },
        ),
        (
            "warnings",
            {
                let mut schema = startup_issue_list_schema(false);
                schema["description"] = json!("Non-blocking startup warnings; omitted when empty. Deliberately disabled current-window binding is not a warning.");
                schema
            },
        ),
        (
            "suggested_call",
            json!({
                "type": "object",
                "description": "Parser-ready recovery observation emitted when work_on_project can identify one exact safe next call.",
                "properties": {
                    "tool": {"type": "string", "const": "list_runners"},
                    "arguments": {
                        "type": "object",
                        "properties": {
                            "include_projects": {"type": "boolean", "const": false},
                            "summary_only": {"type": "boolean", "const": true}
                        },
                        "required": ["include_projects", "summary_only"],
                        "additionalProperties": false
                    }
                },
                "required": ["tool", "arguments"],
                "additionalProperties": false
            }),
        ),
        (
            "suggested_next_actions",
            array_schema(schema_type("string", "Short suggested action."), "Bounded non-default suggested next actions. Omitted when there is nothing more informative than beginning the requested task."),
        ),
    ];
    wrapped_output_schema(output_properties)
}

fn review_evidence_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "description": description,
        "additionalProperties": true,
        "properties": {
            "available": schema_type("boolean", "True when review evidence summary is available."),
            "source": schema_type("string", "Review evidence source, usually session_ledger."),
            "total": schema_type("integer", "Total successful review evidence tool calls counted."),
            "read_only_inspection_count": schema_type("integer", "Successful read-only inspection tool calls counted."),
            "search_count": schema_type("integer", "Successful search tool calls counted."),
            "diff_review_count": schema_type("integer", "Successful diff review tool calls counted."),
            "workspace_review_count": schema_type("integer", "Successful workspace review tool calls counted."),
            "hygiene_review_count": schema_type("integer", "Successful hygiene review tool calls counted."),
            "tools": {
                "type": "array",
                "maxItems": 20,
                "description": "Bounded unique review evidence tool names only; never file contents, diff hunks, stdout/stderr, command text, tokens, secrets, or raw input payloads.",
                "items": schema_type("string", "Review evidence tool name.")
            }
        }
    })
}
