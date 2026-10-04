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
    if let Some(description) = tool["description"].as_str() {
        let selection = match name.as_str() {
            "work_on_project" if tool.pointer("/inputSchema/properties/_wc").is_none() => "Start file, data, diagnostic or coding work in a Project; Git optional. Omit session_id for a fresh Session; supply exact session_id to resume. Read applicable AGENTS.md/CLAUDE.md with read_files. Reuse workspace branch/HEAD/status and sufficient catalogs; refresh stale/incomplete facts before dependent work.",
            "work_on_project" => "Start file, data, diagnostic or coding work; Git optional. Fresh: omit session_id; exact id resumes. Fresh/uncertain model context: request _wc.context=[\"project.instructions\",\"webcodex.workflow\"]; project.instructions contains applicable AGENTS.md/CLAUDE.md. Reuse complete instruction bodies, workspace branch/HEAD/status, semantic navigation and sufficient catalogs; refresh stale/incomplete.",
            "get_work_result_state" => "App-only Work Result read; reauthorizes exact Project/Session; no attention, Session recording or snapshot mutation. files: advertised immutable paths; diff or UTF-8 content; max 32 KiB/page, 256 KiB/file.",
            "plugin_tool" => "Use Runner-owned Plugins via opaque bindings. Catalog selects; describe when schema/binding is missing. Reuse retained exact binding for the same Project; reconcile unknown outcomes.",
            "call_runtime_tool" => "Call one admitted runtime tool with its exact arguments. Use read_tool_manifest to discover the contract. Prefer an available direct callable; ordinary direct tools may fall back here when unavailable, but MCP App presentation tools must use their direct callable while Apps are enabled. Target validation and authority checks still apply.",
            "edit_project_files" => "Read with read_files; send one change per file. Existing edit/delete/rename require expected_read_revision; create needs content. Exact edits fail closed on ambiguity; replace_range uses 1-based inclusive lines from that snapshot. Batches preflight transactionally; Runner rechecks source. Stale: read_files recovery. outcome_unknown: observe before another write. Review and use task-appropriate validation.",
            "run_process" => "Run one native executable with literal argv. Use run_shell for shell grammar/short related chains; run_script for program-like scripts. Pending keeps the same Job and returned continuation; never redispatch. Continue independent work first; when blocked use wait_for_job_readiness. Read logs/details with observe_jobs. Diagnose failures and use task-appropriate validation; outcome_unknown requires observation.",
            "run_shell" => "Run shell grammar or a short related command chain. Use run_process for literal argv; run_script for program-like scripts. Pending keeps the same Job and returned continuation; never redispatch. Continue independent work first; when blocked use wait_for_job_readiness. Read logs/details with observe_jobs. Diagnose failures and use task-appropriate validation; outcome_unknown requires observation.",
            "run_script" => "Run typed sh/bash/PowerShell/Python/JS/TS script data. Use run_process for literal argv or run_shell for shell grammar; prefer structured editors for source edits. Pending keeps the same Job/continuation; never redispatch. Continue independent work; when blocked use wait_for_job_readiness. Read logs/details with observe_jobs. Diagnose failures and validate; outcome_unknown requires observation.",
            "observe_jobs" => "Read logs/details for known Jobs; do not list first. Reuse observation_ref or pass observation_token unchanged as after_observation_token. Continue independent work first. When blocked, use bounded wait_for_job_readiness in the current turn, then observe needed results. Never redispatch execution or assume an automatic next turn.",
            "wait_for_job_readiness" => "Bounded current-turn join for 1..8 exact Jobs, 1..45s. Finish independent work first. Use any when one terminal Job unlocks work; all only when every dependency is needed. Budget the largest safe remaining wait. At deadline reassess work/set; never mechanically refill. Terminal is readiness, not success. Logs/details: observe_jobs. No automatic next turn.",
            "wait_for_job_terminal" => "Optional durable terminal attention for an explicitly selected continuation workflow. Not a blocking wait and does not start a model turn. Ordinary work uses wait_for_job_readiness in the current turn and observe_jobs for results. Reuse exact keyed waits; never redispatch the Job.",
            "present_agent_continuation" => "Present one exact Agent/Endpoint generation as the persistent MCP App continuation card. Pass agent_continuation_ref or the exact tuple. New window setup: create_agent_identity -> rotate_agent_continuation_endpoint -> present_agent_continuation, then yield/end promptly. Presentation success is not wake readiness; later verify list_agent_identities.production_auto_resume_available.",
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
            "Extension catalog; default true on fresh/first Project; false only while a complete/sufficient catalog for that Project is retained.",
        ("work_on_project", "session_id") =>
            "Omit fresh; exact active id/ref resumes its bound Project.",
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
