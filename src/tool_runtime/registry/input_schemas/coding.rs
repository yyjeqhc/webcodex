use serde_json::{json, Value};

use super::sessions::{session_execution_context_schema, session_mode_schema};

#[allow(dead_code)]
pub(crate) fn start_coding_task_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "minLength": 1,
                "description": "Existing runtime project id. Use a full id from list_projects, such as agent:<client_id>:<project_id>. Mutually exclusive with the Runner path source and managed temporary-project source."
            },
            "client_id": {
                "type": "string",
                "minLength": 1,
                "description": "Runner client_id that owns path. For backward compatibility, client_id without path retains the existing Runner-managed temporary-project flow."
            },
            "path": {
                "type": "string",
                "minLength": 1,
                "description": "Existing absolute directory on the selected Runner; Git is not required. The Runner canonicalizes and policy-checks it, reuses a unique enabled registration, or registers it permanently. Mutually exclusive with project and temporary_project_name."
            },
            "temporary_project_name": {
                "type": "string",
                "maxLength": 120,
                "description": "Optional display name for a managed temporary project. The Runner generates the directory name; path-like and traversal-looking names are rejected."
            },
            "title": {
                "type": "string",
                "maxLength": 4000,
                "description": "Optional current user instruction. On creation it is retained as the root task title; on continuation it is appended to the existing ledger and never overwrites the root title."
            },
            "mode": session_mode_schema("Optional session mode. Defaults to normal. inspect blocks structured write tools and runs shell/job-like tools in the Linux Landlock inspect sandbox; read_only blocks both write-like and shell/job-like tools."),
            "deny_write_tools": {
                "type": "boolean",
                "description": "Optional task guard for the created session. Defaults to false unless mode=read_only."
            },
            "deny_shell_tools": {
                "type": "boolean",
                "description": "Optional task guard for the created session. Defaults to false unless mode=read_only."
            },
            "execution_context": session_execution_context_schema(
                "Optional complete Workflow Session execution defaults. On creation this becomes the initial context. On continuation or explicit resume, omission preserves the existing context while an explicit object replaces it atomically; `{}` clears all execution defaults."
            ),
            "detail": {
                "type": "string",
                "enum": ["minimal", "standard", "full"],
                "default": "standard",
                "description": "Startup projection detail. Every detail includes the bounded WebCodex built-in coding workflow separately from project instructions. minimal returns strict session/project/workspace/blocker essentials, instruction status without rule content, and at most 3 validated project-relative exploration paths; standard adds bounded repository rules, continuation evidence, semantic-navigation readiness, and actions; full preserves diagnostic runtime/authority/Git/manifest blocks and embeds the same startup_brief core."
            },
            "resume_session_id": {
                "type": "string",
                "pattern": "^wc_sess_[A-Za-z0-9_]+$",
                "description": "Optional explicit Workflow Session id. When present, start_coding_task resumes only that known active Session after exact project, lifecycle, access, authority, and capability checks; failure never creates a replacement. When omitted, start_coding_task always creates a fresh Workflow Session. Distinct from project-tool session_id and wrapper recording_session_id."
            }
        },
        "required": [],
        "oneOf": [
            {
                "required": ["project"],
                "not": {
                    "anyOf": [
                        {"required": ["client_id"]},
                        {"required": ["path"]},
                        {"required": ["temporary_project_name"]}
                    ]
                }
            },
            {
                "required": ["client_id", "path"],
                "not": {
                    "anyOf": [
                        {"required": ["project"]},
                        {"required": ["temporary_project_name"]}
                    ]
                }
            },
            {
                "required": ["client_id"],
                "not": {
                    "anyOf": [
                        {"required": ["project"]},
                        {"required": ["path"]}
                    ]
                }
            }
        ],
        "additionalProperties": false,
    })
}

pub(crate) fn work_on_project_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "minLength": 1,
                "description": "Existing runtime project id. Use project + instruction for an existing project; do not combine project with client_id or path."
            },
            "client_id": {
                "type": "string",
                "minLength": 1,
                "description": "Runner client_id for the path form. Use client_id + path + instruction together; do not combine with project."
            },
            "path": {
                "type": "string",
                "minLength": 1,
                "description": "Runner-owned absolute directory path for the path form. Use with client_id + instruction; do not combine with project. The Runner authoritatively resolves or permanently registers it before exact Workflow Session handling."
            },
            "instruction": {
                "type": "string",
                "minLength": 1,
                "maxLength": 4000,
                "description": "Required current user instruction. On a new task it becomes the root task title; when session_id is provided it is appended to the existing Workflow Session ledger and never overwrites the root title."
            },
            "include_project_instructions": {
                "type": "boolean",
                "default": true,
                "description": "Whether this bootstrap response should include bounded project-instruction bodies such as AGENTS.md. Defaults to true. Set false only when the caller's current model context already retains the applicable repository instructions. WebCodex does not infer that from session_id or transport identity. Instruction files are still re-observed and Workflow Session metadata is updated; this flag controls only model-facing instruction-content projection."
            },
            "include_workflow_guidance": {
                "type": "boolean",
                "default": true,
                "description": "Whether this bootstrap response should include the static built-in WebCodex coding-workflow guidance. Defaults to true. Set false only when the caller's current model context already retains that workflow guidance. WebCodex does not infer that from session_id or transport identity. This flag controls only model-facing workflow projection; it does not change Workflow Session state, authority, role selection, or execution semantics."
            },
            "session_id": {
                "type": "string",
                "pattern": "^wc_sess_[A-Za-z0-9_]+$",
                "description": "Optional explicit Workflow Session to continue exactly. It must match the project and be active and accessible; failure never guesses or creates a replacement Session. This business input is distinct from wrapper recording_session_id."
            }
        },
        "required": ["instruction"],
        "additionalProperties": false,
    })
}

pub(crate) fn finish_coding_task_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Required runtime project id. Use the same project used to start the task."
            },
            "session_id": {
                "type": "string",
                "description": "Required explicit wc_sess_* business Session id for the current coding task, obtained from its compatible Session bootstrap."
            },
            "include_diff": {
                "type": "boolean",
                "description": "Include bounded diff hunks in show_changes. Defaults to true."
            },
            "include_workspace": {
                "type": "boolean",
                "description": "Defaults to true. When include_handoff=true, controls whether the nested handoff summary includes its workspace block; the top-level finish workspace/show_changes check remains unchanged."
            },
            "include_hygiene": {
                "type": "boolean",
                "description": "Include workspace_hygiene_check output. Defaults to true."
            },
            "include_handoff": {
                "type": "boolean",
                "description": "Include session_handoff_summary output. Defaults to true."
            },
            "include_validation_summary": {
                "type": "boolean",
                "description": "Include deterministic validation-like session ledger event summary when available. Defaults to true; minimal diagnostics require bounded tails or safe result metadata."
            },
            "summary_only": {
                "type": "boolean",
                "description": "When true, return the minimal decision-complete closeout only: workspace cleanliness/conflicts, hygiene state, bounded Job counts, final validation state/counts, tool-failure actionability counts, canonical task_outcome, evidence_integrity, warnings, and suggested_next_actions. Omits project/session identity, permissions, review/work/change/handoff provenance, facts/evidence history/informational notes, command text, stdout/stderr, event history, tails, excerpts, and detailed validation history."
            }
        },
        "required": ["project", "session_id"],
        "additionalProperties": false,
    })
}
