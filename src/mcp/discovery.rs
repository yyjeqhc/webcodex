//! Compact MCP selection copy, applied only to owned tools/list projections.
//! Exact manifests and canonical ToolSpecs retain the operational contract.

use serde_json::Value;

use super::tools::RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN;

// MCP discovery targets, independent of execution semantics. Keep
// purpose, the nearest selection boundary, and essential continuation guidance.
pub(super) const TOOL_DESCRIPTION_MAX_CHARS: usize = 420;
pub(super) const INPUT_DESCRIPTION_MAX_CHARS: usize = 180;

pub(super) fn compact_tool(tool: &mut Value) {
    let name = tool["name"].as_str().unwrap_or_default().to_string();
    let workbench_app_only = tool.pointer("/_meta/ui/visibility")
        == Some(&serde_json::json!(["app"]))
        && super::workbench::CANONICAL_TOOLS.contains(&name.as_str());
    if let Some(description) = tool["description"].as_str() {
        let selection = match name.as_str() {
            "list_runners" if workbench_app_only => "List authorized Runner status and capabilities. Inventory is observation, not execution authority.",
            "list_project_files" if workbench_app_only => "List authorized project paths with bounded paging; file contents require exact resource reads.",
            "get_goal" if workbench_app_only => "Read one owned Goal, revision, completion criteria and associated work.",
            "create_goal" if workbench_app_only => "Create a caller-owned Goal with bounded completion criteria. Reuse the same key and unchanged input on uncertain results.",
            "update_goal" if workbench_app_only => "Edit an owned nonterminal Goal using its expected revision. Conflicts preserve the draft; reread before changing the revision. Replay only the same keyed input.",
            "read_agent_task" if workbench_app_only => "Read an owned canonical Task and its attempts; observation never authorizes redispatch.",
            "observe_coding_agent" if workbench_app_only => "Observe an authorized exact coding run. Uncertain execution requires observation, not redispatch.",
            "cancel_coding_agent" if workbench_app_only => "Request cancellation of an authorized exact run. Request acceptance is not terminal cancellation; observe the same run.",
            "get_session_assignment" if workbench_app_only => "Read an authorized exact Session todo and current assignment fence; no implicit Session selection.",
            "complete_session_message" if workbench_app_only => "Complete an exact authorized todo with the current assignment fence. Reuse the same completion key and answer after uncertainty; stale fences fail closed.",
            "reconcile_agent_task_coding_run" if workbench_app_only => "Reconcile an owned exact Task/Attempt with its original coding run. Never starts or retries execution.",
            "work_on_project" if tool.pointer("/inputSchema/properties/_wc").is_none() => "Start file, data, diagnostic or coding work in a Project; Git optional. Omit session_id for a fresh Session; supply exact session_id to resume. Read applicable AGENTS.md/CLAUDE.md with read_files. Reuse workspace branch/HEAD/status and sufficient catalogs; refresh stale/incomplete facts before dependent work.",
            "work_on_project" => "File/data/diagnostic/coding work; Git optional. Fresh: omit session_id; exact resumes. Fresh/uncertain: _wc.context=[\"project.instructions\",\"webcodex.workflow\"]; project.instructions includes AGENTS.md/CLAUDE.md. Reuse complete instructions, branch/HEAD/status, semantic navigation, sufficient catalogs; refresh stale/incomplete.",
            "present_pdf" => "View one authorized project PDF in a dedicated reader. Use for PDF viewing, present_work_result for coding progress. Supply project and relative path; unchanged/untracked files supported, no Git/Session required. Pins size/SHA (20 MiB max); private App reads fail on version changes. Explicitly reopen to select a new version. Requires MCP Apps Host.",
            "get_work_result_state" => "App-only Work Result read; reauthorizes exact Project/Session; no attention, Session recording or snapshot mutation. files: advertised immutable paths; diff, UTF-8 (32 KiB/page, 256 KiB/file), or PDF (128 KiB/page, 20 MiB/file). PDF bytes use private App metadata.",
            "plugin_tool" => "Use Runner-owned Plugins via opaque bindings. Catalog selects; describe when schema/binding is missing. Reuse retained exact binding for the same Project; reconcile unknown outcomes.",
            "call_runtime_tool" => "Call one admitted runtime tool with exact arguments; read_tool_manifest supplies its contract. Prefer its direct callable; ordinary tools may fall back here, but MCP App presentation requires a direct callable while Apps are enabled. Target validation and authority checks still apply.",
            "edit_project_files" => "Read with read_files; send one change per file. Existing edit/delete/rename require expected_read_revision; create needs content. Exact edits fail closed on ambiguity; replace_range uses 1-based inclusive lines from that snapshot. Batches preflight transactionally; Runner rechecks source. Stale: read_files recovery. outcome_unknown: observe before another write. Review and use task-appropriate validation.",
            "project_validate" => "Validate Rust/Go/Python/Node; no installs. Rust scope.exclude_packages needs all_packages=true, 1–8 exact Cargo members and project_workspace_exclusions_v1. Lint requires scripts.lint (no fallback); Vitest/Jest need project_validation_node_json_v1. Structured evidence proves counts. Scripts may write/network. Pending same Job; observe_jobs, never retry. Other scope/filter/policy: read_tool_manifest.",
            "project_build" => "Build Rust/Go/Python/Node; no installs. Rust profile/targets/artifact_policy=observe: no creation/correctness/freshness proof. Rust scope.exclude_packages needs all_packages=true + project_workspace_exclusions_v1. Rust dependency_resolution_policy=offline: Cargo only, not OS sandbox. Other scope/policy: read_tool_manifest. Scripts may write/network. Pending same Job: observe_jobs, never retry/redispatch.",
            "run_process" => "Run native executable literal argv. run_shell: shell grammar/short chains; run_script: scripts. Pending: same Job/continuation; never redispatch. Do independent work; when blocked: wait_for_job_readiness. Logs/details/recovery: observe_jobs. Diagnose failures; validate; outcome_unknown requires observation.",
            "run_shell" => "Run shell grammar/a short related command chain. run_process: literal argv; run_script: scripts. Pending: same Job/continuation; never redispatch. Do independent work; when blocked: wait_for_job_readiness. Logs/details/recovery: observe_jobs. Diagnose failures; validate; outcome_unknown requires observation.",
            "run_script" => "Run typed sh/bash/PowerShell/Python/JS/TS script data; prefer structured source editors. run_process: literal argv; run_shell: shell grammar. Pending: same Job/continuation; never redispatch. Do independent work; when blocked: wait_for_job_readiness. Logs/details/recovery: observe_jobs. Diagnose failures; validate; outcome_unknown requires observation.",
            "observe_jobs" => "Read known Job logs/details; do not list first. Reuse observation_ref or unchanged observation_token as after_observation_token. Finish independent work, then use bounded wait_for_job_readiness in this turn before observing results. Never redispatch or assume an automatic next turn.",
            "list_jobs" => "Recover lost Job identity or enumerate caller-visible background work. Exact project/session_id/status filters combine with AND before limit; no stdout/stderr bodies. Known pending Job: retain its continuation and continue independent work; do not list first or poll inventory. Read exact Job logs/details with observe_jobs.",
            "wait_for_job_readiness" => "Bounded current-turn join for 1..8 exact Jobs, 1..45s. Finish independent work first. Use any when one terminal Job unlocks work; all only when every dependency is needed. Budget the largest safe remaining wait. At deadline reassess work/set; never mechanically refill. Terminal is readiness, not success. Logs/details: observe_jobs. No automatic next turn.",
            "wait_for_job_terminal" => "Optional durable terminal attention for an explicitly selected continuation workflow. Not a blocking wait and does not start a model turn. Ordinary work uses wait_for_job_readiness in the current turn and observe_jobs for results. Reuse exact keyed waits; never redispatch the Job.",
            "present_docx" => "View one authorized project DOCX in a dedicated read-only reader. Supply project and relative path; unchanged/untracked files supported, no Git/Session required. Pins size/SHA (10 MiB max); private App reads fail on version changes. Common styles only; automatic Word pagination/editing unsupported. Rendering requires MCP Apps Host. Reopen to select a new version.",
            "present_agent_continuation" => "Present one exact Agent/Endpoint MCP App card directly. Setup: create_agent_identity -> rotate_agent_continuation_endpoint -> this tool. Give a normal final assistant response; card/tool result alone does not finalize the turn. Presentation is not wake readiness; later check list_agent_identities.production_auto_resume_available.",
            "present_job_terminal_continuation" => "Present an exact owned waiting wait_id as an MCP App card after independent work. Give a normal final assistant response; card/tool result alone does not finalize the turn. Reauthorizes wait/Job; never starts or retries execution. Triggered waits stay in this turn; auto-resume needs a verified unattended Host and exact binding.",
            "start_agent_task_attempt" => "Create one leased fenced Attempt for the explicit current assignee. Returns attempt_id, attempt_fence, and attempt_ref. Exact keyed retry returns that same Attempt. Does not dispatch CodingAgent, Job, Wake, or Endpoint work.",
            "start_agent_task_endpoint_continuation" => "Select the Endpoint continuation for one exact live AgentTaskAttempt. Pass attempt_ref or task, attempt, assignee, fence, and controller generation. Does not choose an Endpoint or grant CodingAgent authority. A stale ref fails closed and is not rewritten onto a later attempt or generation.",
            _ => description,
        };
        tool["description"] =
            Value::String(bound_description(selection, TOOL_DESCRIPTION_MAX_CHARS));
    }
    if let Some(schema) = tool.get_mut("inputSchema") {
        compact_input_descriptions(schema);
        compact_invocation_envelope(schema);
        if name == "edit_project_files" {
            compact_primary_editor_schema(schema);
        }
        if name == "get_work_result_state" {
            // The App consumes the canonical file contract. Discovery retains
            // its exact inline fields and constraints without repeating prose.
            if let Some(files) = schema.pointer_mut("/properties/files") {
                strip_schema_descriptions(files);
            }
        }
        if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
            for (field, property) in properties {
                if let (Some(description), Some(Value::String(copy))) = (
                    common_input_description(&name, field),
                    property.get_mut("description"),
                ) {
                    *copy = description.to_string();
                }
            }
        }
        if let Some(envelope) = schema.pointer_mut("/properties/_wc") {
            strip_wrapper_descriptions(envelope);
            envelope["description"] = Value::String(
                "Optional invocation sidecars; omit when unused. Never grants authority.".into(),
            );
        }
        compact_discovery_validation_annotations(schema);
        if workbench_app_only {
            // These exact schemas are consumed by the bundled controller, not
            // model selection. Keep every validation constraint without repeating
            // canonical field prose in the Host discovery transport.
            strip_schema_descriptions(schema);
        }
    }
}

fn compact_primary_editor_schema(schema: &mut Value) {
    // `tools/list` is only the model-selection copy. The direct editor already
    // carries one bounded top-level purpose, while the discriminated `changes`
    // variants repeat explanatory prose across edit/create/delete/rename and
    // nested exact/range edit forms. Keep every structural constraint, bound,
    // required field and discriminator, but omit that duplicated nested copy.
    // Canonical/full discovery and ToolRuntime parsing retain the exact schema.
    if let Some(changes) = schema.pointer_mut("/properties/changes/items") {
        strip_schema_descriptions(changes);
    }
}

fn strip_schema_descriptions(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.remove("description");
    for keyword in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
    ] {
        if let Some(children) = object.get_mut(keyword).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                strip_schema_descriptions(child);
            }
        }
    }
    for keyword in [
        "items",
        "prefixItems",
        "allOf",
        "anyOf",
        "oneOf",
        "additionalItems",
        "additionalProperties",
        "unevaluatedItems",
        "unevaluatedProperties",
        "propertyNames",
        "contains",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get_mut(keyword) {
            if let Some(children) = child.as_array_mut() {
                for child in children {
                    strip_schema_descriptions(child);
                }
            } else {
                strip_schema_descriptions(child);
            }
        }
    }
}

fn strip_wrapper_descriptions(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    object.remove("description");
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            strip_wrapper_descriptions(property);
        }
    }
    if let Some(items) = object.get_mut("items") {
        strip_wrapper_descriptions(items);
    }
}

fn compact_invocation_envelope(schema: &mut Value) {
    let Some(envelope) = schema
        .pointer_mut("/properties/_wc")
        .filter(|value| value.is_object())
    else {
        return;
    };
    strip_wrapper_descriptions(envelope);
    if let Some(properties) = envelope
        .get_mut("properties")
        .and_then(Value::as_object_mut)
    {
        if properties.contains_key("reply") {
            properties.insert("reply".to_string(), serde_json::json!({"type": "object"}));
        }
        if properties.contains_key("control") {
            properties.insert("control".to_string(), serde_json::json!({"type": "object"}));
        }
    }
}

fn compact_discovery_validation_annotations(schema: &mut Value) {
    // Only copied opaque IDs in protocol wrappers, at these exact schema
    // positions and with these exact patterns. Business IDs, fences, resource
    // paths and all bounds stay intact. Full discovery and runtime validation
    // use their original schemas/parsers, never this owned presentation copy.
    for (pointer, pattern) in [
        (
            "/properties/_wc/properties/record",
            RECORDING_SESSION_SELECTOR_SCHEMA_PATTERN,
        ),
        (
            "/properties/_wc/properties/ack/items",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
        (
            "/properties/_wc/properties/resolve/properties/message_id",
            "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$",
        ),
    ] {
        if let Some(property) = schema.pointer_mut(pointer).and_then(Value::as_object_mut) {
            if property.get("type").and_then(Value::as_str) == Some("string")
                && property.get("pattern").and_then(Value::as_str) == Some(pattern)
            {
                property.remove("pattern");
            }
        }
    }
}

fn common_input_description(tool: &str, field: &str) -> Option<&'static str> {
    // Root arguments only, in audited groups with the same semantics. In
    // particular run_shell cwd/timeouts can refer to a named SSH resource;
    // project, client_id and idempotency_key also differ between direct tools.
    Some(match (tool, field) {
        ("run_process", "cwd") =>
            "Project-relative cwd; omit, empty or '.' for root. No named Session SSH resources.",
        ("run_skill_resource", "cwd") =>
            "Project-relative cwd; omit, empty or '.' for root. Skill resolution does not change cwd.",
        ("run_process", "timeout_secs") =>
            "Total runtime seconds; default 60, clamped to 604800 (7 days).",
        ("run_skill_resource", "timeout_secs") =>
            "Total runtime seconds; default 60, clamped to 3600.",
        ("cargo_check" | "cargo_test", "timeout_secs") =>
            "Total validation runtime seconds, clamped to 3600. Defaults vary per tool.",
        ("run_process" | "run_shell", "assertion_name") =>
            "Validation label; reuse after a fix to correlate evidence. Inert unless execution is validation-like.",
        ("work_on_project", "guidance_profile") =>
            "Guidance only; explicit value wins; no authority.",
        ("work_on_project", "include_extension_catalog") =>
            "Catalog true fresh/first Project; false only with its complete/sufficient retained catalog.",
        ("work_on_project", "session_id") =>
            "Omit fresh; exact active id/ref resumes its bound Project.",
        ("present_agent_continuation", "agent_continuation_ref") =>
            "Server ~ac ref pins Agent/Endpoint/generation, never authority. Use ref or full tuple, never both.",
        ("present_agent_continuation", "agent_id") =>
            "Exact Agent id, with endpoint_id and expected_controller_generation; omit with ref. No fallback.",
        ("present_agent_continuation", "endpoint_id") =>
            "Exact Endpoint id in the tuple; omit with ref. Rechecks ownership.",
        ("present_agent_continuation", "expected_controller_generation") =>
            "Exact Endpoint generation in tuple; omit with ref. Stale input fails closed; never follows rotation.",
        _ => return None,
    })
}

fn compact_input_descriptions(schema: &mut Value) {
    // Traverse schema positions only: const/default/enum/examples may contain
    // business data named "description" that must never be rewritten.
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    if let Some(Value::String(description)) = object.get_mut("description") {
        *description = bound_description(description, INPUT_DESCRIPTION_MAX_CHARS);
    }
    for keyword in [
        "properties",
        "patternProperties",
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
    ] {
        if let Some(children) = object.get_mut(keyword).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                compact_input_descriptions(child);
            }
        }
    }
    for keyword in [
        "items",
        "prefixItems",
        "allOf",
        "anyOf",
        "oneOf",
        "additionalItems",
        "additionalProperties",
        "unevaluatedItems",
        "unevaluatedProperties",
        "propertyNames",
        "contains",
        "not",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = object.get_mut(keyword) {
            if let Some(children) = child.as_array_mut() {
                for child in children {
                    compact_input_descriptions(child);
                }
            } else {
                compact_input_descriptions(child);
            }
        }
    }
}

pub(super) fn bound_description(description: &str, max_chars: usize) -> String {
    let description = description.trim();
    if description.chars().count() <= max_chars {
        return description.to_string();
    }
    // Prefer complete sentences; periods in foo.rs, v0.4.0, and context keys
    // are not sentence boundaries. Fall back to a Unicode-safe word prefix.
    let prefix: String = description.chars().take(max_chars - 1).collect();
    if let Some(end) = prefix
        .char_indices()
        .filter_map(|(index, ch)| {
            let end = index + ch.len_utf8();
            (matches!(ch, '.' | '!' | '?')
                && description[end..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace))
            .then_some(end)
        })
        .next_back()
    {
        return prefix[..end].to_string();
    }
    let end = prefix
        .rfind(char::is_whitespace)
        .filter(|index| *index > 0)
        .unwrap_or(prefix.len());
    format!("{}…", prefix[..end].trim_end())
}
