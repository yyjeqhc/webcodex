use super::*;
use crate::tool_runtime::TOOL_CALL_WRAPPER_FIELDS;

fn runtime_accepted_flattened_action_fields() -> std::collections::BTreeSet<String> {
    let mut fields = std::collections::BTreeSet::new();
    for spec in registered_tool_specs() {
        fields.extend(accepted_flattened_args_for_spec(&spec));
    }
    fields
}

fn flattened_schema_alternatives(schema: &Value) -> Vec<&Value> {
    schema["anyOf"].as_array().map_or_else(
        || vec![schema],
        |alternatives| alternatives.iter().collect(),
    )
}

#[test]
fn openapi_flattened_session_mode_includes_inspect() {
    let spec = build_openapi_spec();
    assert_eq!(
        spec["components"]["schemas"]["ToolCallRequest"]["properties"]["mode"]["enum"],
        json!(["normal", "inspect", "read_only"])
    );
}

#[test]
fn openapi_flattened_execution_context_is_strongly_typed() {
    let spec = build_openapi_spec();
    let execution_context =
        &spec["components"]["schemas"]["ToolCallRequest"]["properties"]["execution_context"];
    assert_eq!(execution_context["type"], "object");
    assert_eq!(execution_context["additionalProperties"], false);
    assert_eq!(
        execution_context["properties"]["default_shell"]["enum"],
        json!(["sh", "bash"])
    );
    assert_eq!(
        execution_context["properties"]["default_cwd"]["maxLength"],
        4096
    );
}

#[test]
fn explicit_resume_openapi_metadata_is_distinct_from_session_recording() {
    let spec = build_openapi_spec();
    let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    let resume = &properties["resume_session_id"];
    assert_eq!(resume["type"], "string");
    assert_eq!(resume["pattern"], "^wc_sess_[A-Za-z0-9_]+$");
    let resume_description = resume["description"].as_str().unwrap();
    for phrase in [
        "start_coding_task business input",
        "failure never falls back or creates",
        "without stable window identity",
        "mutually exclusive with new_session=true",
    ] {
        assert!(
            resume_description.to_lowercase().contains(phrase),
            "missing {phrase}: {resume_description}"
        );
    }
    assert_ne!(
        resume["description"],
        properties["session_id"]["description"]
    );
    assert_ne!(
        resume["description"],
        properties[TOOL_CALL_RECORDING_SESSION_ID_FIELD]["description"]
    );
    let wrapper_description = spec["components"]["schemas"]["ToolCallRequest"]["description"]
        .as_str()
        .unwrap();
    assert!(wrapper_description.contains("resume_session_id"));
    assert!(wrapper_description.contains("cannot establish a current binding"));
}

/// Recursively collect every `$ref` string found anywhere in a JSON value.
fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if k == "$ref" {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_string());
                    }
                }
                collect_refs(v, out);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_refs(v, out);
            }
        }
        _ => {}
    }
}

/// Resolve a local `#/components/schemas/<Name>` ref against the spec.
fn resolve_local_ref<'a>(spec: &'a Value, reference: &str) -> Option<&'a Value> {
    let rest = reference.strip_prefix("#/")?;
    let mut current = spec;
    for segment in rest.split('/') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Collect all operation ids in the spec (sorted, deduplicated).
fn operation_ids(spec: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for methods in spec["paths"].as_object().unwrap().values() {
        for op in methods.as_object().unwrap().values() {
            ids.push(op["operationId"].as_str().unwrap().to_string());
        }
    }
    ids.sort();
    ids
}

#[test]
fn openapi_operation_ids_are_minimal() {
    let spec = build_openapi_spec();
    let ids = operation_ids(&spec);
    let mut expected = GPT_ACTION_OPS
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn openapi_operations_have_consequential_flags() {
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    let mut count = 0;
    for methods in paths.values() {
        for op in methods.as_object().unwrap().values() {
            count += 1;
            let operation_id = op["operationId"].as_str().unwrap();
            assert!(
                op.get("x-openai-isConsequential")
                    .and_then(|v| v.as_bool())
                    .is_some(),
                "operation {} must have x-openai-isConsequential",
                operation_id
            );
        }
    }
    assert_eq!(count, 25);
}

#[test]
fn openapi_consequential_flags_match_operation_risk() {
    let spec = build_openapi_spec();
    let mut flags = std::collections::BTreeMap::new();
    for methods in spec["paths"].as_object().unwrap().values() {
        for op in methods.as_object().unwrap().values() {
            let operation_id = op["operationId"].as_str().unwrap().to_string();
            let consequential = op["x-openai-isConsequential"].as_bool().unwrap();
            flags.insert(operation_id, consequential);
        }
    }
    let readonly = [
        "listRuntimeTools",
        "listProjects",
        "getRuntimeStatus",
        "readProjectFile",
        "listProjectFiles",
        "searchProjectText",
        "getProjectGitStatus",
        "getProjectGitDiff",
        "getProjectGitDiffSummary",
        "getRuntimeJobStatus",
        "getRuntimeJobLog",
        "getRuntimeJobTail",
        "listRuntimeJobs",
        "validateProjectPatch",
        "registerProject",
        "createProject",
    ];
    let consequential = [
        "applyProjectPatch",
        "applyProjectPatchChecked",
        "runProjectShellCommand",
        "startProjectShellJob",
        "deleteProjectFiles",
        "gitRestorePaths",
        "discardUntrackedFiles",
        "callRuntimeTool",
    ];
    for id in readonly {
        assert_eq!(
            flags.get(id),
            Some(&false),
            "{} should be non-consequential",
            id
        );
    }
    for id in consequential {
        assert_eq!(flags.get(id), Some(&true), "{} should be consequential", id);
    }
    assert_eq!(flags.len(), 25);
}

#[test]
fn openapi_does_not_expose_any_legacy_or_non_gpt_action_paths() {
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    for legacy in LEGACY_FORBIDDEN_PATHS {
        assert!(
            !paths.contains_key(*legacy),
            "legacy/non-GPT-Actions path '{}' must not appear in openapi.json",
            legacy
        );
    }
}

#[test]
fn openapi_phase3_exposes_validate_patch_as_dedicated_action() {
    // Phase 3: validate_patch is now promoted to a dedicated GPT Action
    // (validateProjectPatch) so a custom GPT can dry-run patches without
    // callRuntimeTool. It is a read-only preflight that never modifies the
    // worktree.
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    assert!(
        paths.contains_key("/api/projects/validate_patch"),
        "validate_patch must now appear in /openapi.json as a dedicated read-only action"
    );
    assert_eq!(
        spec["paths"]["/api/projects/validate_patch"]["post"]["operationId"],
        "validateProjectPatch"
    );
    let desc = spec["paths"]["/api/projects/validate_patch"]["post"]["description"]
        .as_str()
        .unwrap();
    assert!(
        desc.to_lowercase().contains("read-only"),
        "validateProjectPatch description must be marked read-only: {}",
        desc
    );
}

#[test]
fn openapi_uses_bearer_auth() {
    let spec = build_openapi_spec();
    assert_eq!(
        spec["components"]["securitySchemes"]["bearerAuth"]["scheme"],
        "bearer"
    );
    let description = spec["components"]["securitySchemes"]["bearerAuth"]["description"]
        .as_str()
        .expect("bearerAuth description");
    assert!(
        description.contains("shared key"),
        "bearerAuth description must mention shared-key quick start: {description}"
    );
    assert!(
        description.contains("quick start"),
        "bearerAuth description must mention quick start: {description}"
    );
    assert!(
        description.contains("wc_pat"),
        "bearerAuth description must mention wc_pat managed tokens: {description}"
    );
    assert!(
        description.contains("managed mode"),
        "bearerAuth description must mention managed mode: {description}"
    );
    assert!(
        !description.contains("personal API token only") && !description.contains("wc_pat only"),
        "bearerAuth description must not regress to PAT-only guidance: {description}"
    );
}

#[test]
fn openapi_info_description_mentions_bearer_host_auth_modes() {
    let spec = build_openapi_spec();
    let description = spec["info"]["description"]
        .as_str()
        .expect("info description");
    for expected in [
        "static bearer/API-key hosts",
        "shared key",
        "quick start",
        "wc_pat",
        "managed mode",
    ] {
        assert!(
            description.contains(expected),
            "OpenAPI info.description must mention {expected:?}: {description}"
        );
    }
    assert!(
        !description.contains("personal API token only") && !description.contains("wc_pat only"),
        "OpenAPI info.description must not regress to PAT-only guidance: {description}"
    );
}

#[test]
fn openapi_top_level_security_uses_bearer() {
    let spec = build_openapi_spec();
    let security = spec["security"].as_array().expect("security array");
    assert!(!security.is_empty());
    assert!(security[0]["bearerAuth"].is_array());
}

#[test]
fn openapi_all_local_refs_resolve() {
    let spec = build_openapi_spec();
    let mut refs = Vec::new();
    collect_refs(&spec, &mut refs);
    assert!(!refs.is_empty(), "expected at least one $ref in the spec");
    for reference in &refs {
        assert!(
            reference.starts_with("#/"),
            "only local refs are allowed, found: {}",
            reference
        );
        let resolved = resolve_local_ref(&spec, reference)
            .unwrap_or_else(|| panic!("unresolved $ref target: {}", reference));
        assert!(
            resolved.is_object(),
            "$ref target '{}' should resolve to a schema object",
            reference
        );
    }
}

#[test]
fn openapi_schemas_define_all_referenced_names() {
    let spec = build_openapi_spec();
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("schemas object");
    // Every referenced schema name must exist as a key.
    let mut refs = Vec::new();
    collect_refs(&spec, &mut refs);
    for reference in &refs {
        if let Some(name) = reference.strip_prefix("#/components/schemas/") {
            assert!(
                schemas.contains_key(name),
                "referenced schema '{}' is not defined in components/schemas",
                name
            );
        }
    }
}

#[test]
fn openapi_paths_only_use_post_method() {
    // GPT Actions surface is POST-only. /openapi.json itself is served by
    // a separate GET route and must NOT appear inside the schema paths.
    let spec = build_openapi_spec();
    for (path, methods) in spec["paths"].as_object().unwrap() {
        let method_keys: Vec<&String> = methods.as_object().unwrap().keys().collect();
        assert_eq!(
            method_keys,
            vec!["post"],
            "path '{}' should only expose POST, got {:?}",
            path,
            method_keys
        );
    }
}

#[test]
fn openapi_has_no_duplicate_operation_ids() {
    let spec = build_openapi_spec();
    let mut ids = Vec::new();
    for methods in spec["paths"].as_object().unwrap().values() {
        for op in methods.as_object().unwrap().values() {
            ids.push(op["operationId"].as_str().unwrap().to_string());
        }
    }
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        ids.len(),
        sorted.len(),
        "duplicate operation ids detected: {:?}",
        ids
    );
}

#[test]
fn openapi_operation_descriptions_fit_chatgpt_limit() {
    let spec = build_openapi_spec();
    for (path, methods) in spec["paths"].as_object().unwrap() {
        for (method, op) in methods.as_object().unwrap() {
            let operation_id = op["operationId"].as_str().unwrap_or("<missing>");
            let desc = op["description"].as_str().unwrap_or("");
            assert!(
                desc.chars().count() <= 300,
                "{} {} operationId {} description has length {}",
                method,
                path,
                operation_id,
                desc.chars().count()
            );
        }
    }
}

#[test]
fn openapi_rejects_legacy_codex_paths_from_model_facing_spec() {
    let spec = build_openapi_spec();
    assert!(
        spec["paths"].get("/api/codex/run").is_none(),
        "legacy /api/codex/run must not be exposed as a GPT Action path"
    );
    let serialized = serde_json::to_string(&spec).unwrap();
    assert!(
        !serialized.contains("runCodexTask"),
        "legacy runCodexTask operation id must stay absent from OpenAPI"
    );
    assert!(
        !serialized.contains("CodexRunRequest"),
        "legacy CodexRunRequest schema must stay absent from OpenAPI"
    );
    // callRuntimeTool should be marked advanced/generic.
    let call_tool = &spec["paths"]["/api/tools/call"]["post"]["description"]
        .as_str()
        .unwrap();
    assert!(
        call_tool.contains("Advanced"),
        "callRuntimeTool description should mark it as advanced"
    );
    assert!(
        call_tool.contains("generic escape hatch")
            && call_tool.contains("Prefer dedicated actions"),
        "callRuntimeTool description should prefer dedicated tools: {call_tool}"
    );
    // getRuntimeJobStatus / getRuntimeJobLog should mention job_id polling.
    let status_desc = &spec["paths"]["/api/jobs/status"]["post"]["description"]
        .as_str()
        .unwrap();
    assert!(status_desc.contains("job_id"));
    let log_desc = &spec["paths"]["/api/jobs/log"]["post"]["description"]
        .as_str()
        .unwrap();
    assert!(log_desc.contains("job_id"));
}

#[test]
fn openapi_call_runtime_tool_lists_accepted_tool_names() {
    use crate::tool_runtime::tool_definition::model_visible_tool_definitions;

    let spec = build_openapi_spec();
    let tool_desc = &spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        [TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .unwrap();
    for definition in model_visible_tool_definitions() {
        let name = definition.name;
        assert!(
            tool_desc.contains(name),
            "ToolCallRequest.tool description should list accepted tool name '{}'",
            name
        );
    }
    assert!(tool_desc.contains("document_diagnostics"));
    assert!(tool_desc.contains("hover"));
    assert!(tool_desc.contains("workspace_symbols"));
    let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    for field in ["project", "path", "limit", "session_id"] {
        assert!(
            properties.contains_key(field),
            "callRuntimeTool must expose flattened document_diagnostics field {field}"
        );
    }
    for field in ["line", "column", "query"] {
        assert!(
            properties.contains_key(field),
            "callRuntimeTool must expose flattened LSP field {field}"
        );
    }
    let operation_ids = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|path| {
            path.as_object()
                .into_iter()
                .flat_map(|methods| methods.values())
        })
        .filter_map(|operation| operation["operationId"].as_str())
        .collect::<Vec<_>>();
    assert!(!operation_ids.contains(&"hover"));
    assert!(!operation_ids.contains(&"workspaceSymbols"));
    assert_eq!(operation_ids.len(), 25);
}

#[test]
fn openapi_read_files_is_available_through_strict_flattened_runtime_fields() {
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let description = tool_call["properties"][TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("read_file"));
    assert!(description.contains("read_files"));
    assert!(description.contains("observe_jobs"));

    let items = &tool_call["properties"]["items"];
    let alternatives = flattened_schema_alternatives(items);
    let read_files = alternatives
        .iter()
        .copied()
        .find(|alternative| alternative["items"]["required"] == json!(["path"]))
        .expect("flattened items must retain the read_files array shape");
    let observe_jobs = alternatives
        .iter()
        .copied()
        .find(|alternative| alternative["items"]["required"] == json!(["job_id"]))
        .expect("flattened items must retain the observe_jobs array shape");
    for alternative in [read_files, observe_jobs] {
        assert_eq!(alternative["type"], "array");
        assert_eq!(alternative["minItems"], 1);
        assert_eq!(alternative["maxItems"], 8);
        assert_eq!(alternative["items"]["type"], "object");
        assert_eq!(alternative["items"]["additionalProperties"], false);
    }
    assert!(read_files["items"]["properties"].get("path").is_some());
    assert!(observe_jobs["items"]["properties"].get("job_id").is_some());
    assert_eq!(tool_call["additionalProperties"], false);

    let examples = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
        ["application/json"]["examples"];
    assert_eq!(examples["readFiles"]["value"]["tool"], "read_files");
    assert_eq!(
        examples["readFiles"]["value"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn openapi_dynamic_flattened_arg_collisions_are_independent_of_tool_order() {
    let mut forward = json!({"ToolCallRequest": {"properties": {}}});
    let mut reverse = json!({"ToolCallRequest": {"properties": {}}});
    let specs = registered_tool_specs();
    let mut reversed_specs = specs.clone();
    reversed_specs.reverse();

    insert_tool_call_request_flattened_arg_properties_for_specs(&mut forward, specs);
    insert_tool_call_request_flattened_arg_properties_for_specs(&mut reverse, reversed_specs);

    assert_eq!(
        forward["ToolCallRequest"]["properties"],
        reverse["ToolCallRequest"]["properties"]
    );
}

#[test]
fn openapi_search_project_texts_is_available_through_strict_flattened_runtime_fields() {
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let description = tool_call["properties"][TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("search_project_text"));
    assert!(description.contains("search_project_texts"));

    let queries = &tool_call["properties"]["queries"];
    assert_eq!(queries["type"], "array");
    assert_eq!(queries["minItems"], 1);
    assert_eq!(queries["maxItems"], 8);
    assert_eq!(queries["items"]["additionalProperties"], false);
    assert_eq!(queries["items"]["required"], json!(["pattern"]));
    assert_eq!(
        queries["items"]["properties"]["result_mode"]["enum"],
        json!(["matches", "files_with_matches", "count"])
    );
    assert_eq!(tool_call["additionalProperties"], false);

    let examples = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
        ["application/json"]["examples"];
    assert_eq!(
        examples["searchProjectTexts"]["value"]["tool"],
        "search_project_texts"
    );
    assert_eq!(
        examples["searchProjectTexts"]["value"]["queries"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn openapi_key_actions_have_examples() {
    let spec = build_openapi_spec();
    // getRuntimeJobStatus, getRuntimeJobLog, and callRuntimeTool must ship
    // with at least one request example so GPT has a concrete template to
    // follow. Phase 3 dedicated actions are also required to carry examples.
    for (path, label) in [
        ("/api/jobs/status", "getRuntimeJobStatus"),
        ("/api/jobs/log", "getRuntimeJobLog"),
        ("/api/projects/read_file", "readProjectFile"),
        ("/api/projects/git_status", "getProjectGitStatus"),
        ("/api/projects/git_diff", "getProjectGitDiff"),
        ("/api/projects/git_diff_summary", "getProjectGitDiffSummary"),
        ("/api/projects/list_files", "listProjectFiles"),
        ("/api/projects/search_text", "searchProjectText"),
        ("/api/projects/validate_patch", "validateProjectPatch"),
        ("/api/projects/apply_patch", "applyProjectPatch"),
        (
            "/api/projects/apply_patch_checked",
            "applyProjectPatchChecked",
        ),
        ("/api/projects/run_shell", "runProjectShellCommand"),
        ("/api/projects/delete_files", "deleteProjectFiles"),
        ("/api/projects/git_restore_paths", "gitRestorePaths"),
        ("/api/projects/discard_untracked", "discardUntrackedFiles"),
        ("/api/projects/run_job", "startProjectShellJob"),
        ("/api/projects/register", "registerProject"),
        ("/api/projects/create", "createProject"),
        ("/api/jobs/list", "listRuntimeJobs"),
        ("/api/jobs/tail", "getRuntimeJobTail"),
        ("/api/tools/call", "callRuntimeTool"),
    ] {
        let examples =
            &spec["paths"][path]["post"]["requestBody"]["content"]["application/json"]["examples"];
        assert!(
            examples.is_object(),
            "{} request should declare examples",
            label
        );
        assert!(
            !examples.as_object().unwrap().is_empty(),
            "{} request should declare at least one example",
            label
        );
    }
}

#[test]
fn openapi_dedicated_actions_have_expected_routes_and_operation_ids() {
    // Complete path -> operationId map for the GPT Actions surface. Every
    // operation in GPT_ACTION_OPS must appear here, so adding an action
    // without pinning its route fails the length check below.
    let spec = build_openapi_spec();
    let expected = [
        ("/api/tools/list", "listRuntimeTools"),
        ("/api/projects/list", "listProjects"),
        ("/api/projects/register", "registerProject"),
        ("/api/projects/create", "createProject"),
        ("/api/runtime/status", "getRuntimeStatus"),
        ("/api/jobs/status", "getRuntimeJobStatus"),
        ("/api/jobs/log", "getRuntimeJobLog"),
        ("/api/jobs/list", "listRuntimeJobs"),
        ("/api/jobs/tail", "getRuntimeJobTail"),
        ("/api/projects/read_file", "readProjectFile"),
        ("/api/projects/git_status", "getProjectGitStatus"),
        ("/api/projects/git_diff", "getProjectGitDiff"),
        ("/api/projects/git_diff_summary", "getProjectGitDiffSummary"),
        ("/api/projects/list_files", "listProjectFiles"),
        ("/api/projects/search_text", "searchProjectText"),
        ("/api/projects/validate_patch", "validateProjectPatch"),
        ("/api/projects/apply_patch", "applyProjectPatch"),
        (
            "/api/projects/apply_patch_checked",
            "applyProjectPatchChecked",
        ),
        ("/api/projects/run_shell", "runProjectShellCommand"),
        ("/api/projects/delete_files", "deleteProjectFiles"),
        ("/api/projects/git_restore_paths", "gitRestorePaths"),
        ("/api/projects/discard_untracked", "discardUntrackedFiles"),
        ("/api/artifacts/import", "importConversationFilesToProject"),
        ("/api/projects/run_job", "startProjectShellJob"),
        ("/api/tools/call", "callRuntimeTool"),
    ];
    assert_eq!(
        expected.len(),
        GPT_ACTION_OPS.len(),
        "route map must cover every GPT Action operation"
    );
    for (path, operation_id) in expected {
        assert_eq!(
            spec["paths"][path]["post"]["operationId"], operation_id,
            "GPT Action path '{}' must map to operationId '{}'",
            path, operation_id
        );
    }
}

#[test]
fn openapi_mutation_actions_describe_execution_risk_and_auth() {
    // Phase 3 mutation actions (applyProjectPatch, applyProjectPatchChecked,
    // runProjectShellCommand, deleteProjectFiles, gitRestorePaths,
    // discardUntrackedFiles) are executable actions with side effects; their
    // descriptions must call out the execution risk/side effects and the
    // Bearer-auth requirement so GPT callers understand they are not
    // read-only inspection.
    let spec = build_openapi_spec();
    for path in [
        "/api/projects/apply_patch",
        "/api/projects/apply_patch_checked",
        "/api/projects/run_shell",
        "/api/projects/delete_files",
        "/api/projects/git_restore_paths",
        "/api/projects/discard_untracked",
        "/api/projects/run_job",
        "/api/projects/register",
        "/api/projects/create",
    ] {
        let desc = spec["paths"][path]["post"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
            desc.to_lowercase().contains("side effect"),
            "{} description should mention side effects, got: {}",
            path,
            desc
        );
        assert!(
            desc.to_lowercase().contains("bearer auth"),
            "{} description should mention Bearer auth, got: {}",
            path,
            desc
        );
    }
    // The patch/shell/cleanup/write mutations must also mention the agent
    // shell capability. startProjectShellJob requires the async shell job
    // capability (checked separately below), not the plain shell capability.
    for path in [
        "/api/projects/apply_patch",
        "/api/projects/apply_patch_checked",
        "/api/projects/run_shell",
        "/api/projects/delete_files",
        "/api/projects/git_restore_paths",
        "/api/projects/discard_untracked",
    ] {
        let desc = spec["paths"][path]["post"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
            desc.to_lowercase().contains("agent shell capability"),
            "{} description should mention the agent shell capability, got: {}",
            path,
            desc
        );
    }
    // startProjectShellJob requires the async shell job capability, not
    // the plain shell capability. Pin its capability wording so GPT callers
    // understand the different requirement.
    {
        let desc = spec["paths"]["/api/projects/run_job"]["post"]["description"]
            .as_str()
            .unwrap_or("");
        assert!(
                desc.to_lowercase().contains("async shell job"),
                "startProjectShellJob description should mention the async shell job capability, got: {}",
                desc
            );
    }
}

#[test]
fn openapi_readonly_actions_describe_readonly() {
    // Every read-only dedicated action must mark itself read-only (or
    // "never writes") in its description so GPT callers can tell them
    // apart from mutations. This covers all 14 read-only operations;
    // callRuntimeTool is excluded because it is a generic escape hatch
    // that can dispatch either read-only or mutating tools.
    let spec = build_openapi_spec();
    for path in [
        "/api/tools/list",
        "/api/projects/list",
        "/api/runtime/status",
        "/api/jobs/status",
        "/api/jobs/log",
        "/api/jobs/list",
        "/api/jobs/tail",
        "/api/projects/read_file",
        "/api/projects/git_status",
        "/api/projects/git_diff",
        "/api/projects/git_diff_summary",
        "/api/projects/list_files",
        "/api/projects/search_text",
        "/api/projects/validate_patch",
    ] {
        let desc = spec["paths"][path]["post"]["description"]
            .as_str()
            .unwrap_or("");
        let lower = desc.to_lowercase();
        assert!(
            lower.contains("read-only") || lower.contains("never writes"),
            "{} description should be marked read-only or never writes, got: {}",
            path,
            desc
        );
    }
}

#[test]
fn openapi_request_body_schemas_have_additional_properties_false() {
    // Every requestBody schema referenced by an operation must declare
    // `additionalProperties: false` at the top level so GPT Actions
    // rejects unknown fields rather than silently dropping them. Inner
    // properties (e.g. ToolCallRequest.params) may still allow arbitrary
    // keys; this guard only pins the top-level request object.
    let spec = build_openapi_spec();
    let schemas = spec["components"]["schemas"]
        .as_object()
        .expect("schemas object");
    for (path, methods) in spec["paths"].as_object().unwrap() {
        for (method, op) in methods.as_object().unwrap() {
            let request_schema_ref =
                op["requestBody"]["content"]["application/json"]["schema"]["$ref"].as_str();
            let schema_name = match request_schema_ref {
                Some(r) => r.strip_prefix("#/components/schemas/").unwrap_or(r),
                None => continue,
            };
            let schema = schemas.get(schema_name).unwrap_or_else(|| {
                panic!(
                    "{} {} references unknown schema '{}'",
                    method, path, schema_name
                )
            });
            assert_eq!(
                schema["additionalProperties"],
                Value::Bool(false),
                "{} {} requestBody schema '{}' must have additionalProperties=false",
                method,
                path,
                schema_name
            );
        }
    }
}

#[test]
fn openapi_file_search_shell_schemas_include_ergonomics_fields() {
    let spec = build_openapi_spec();
    let schemas = &spec["components"]["schemas"];
    let read_props = schemas["ReadProjectFileRequest"]["properties"]
        .as_object()
        .unwrap();
    assert!(read_props.contains_key("with_line_numbers"));

    let search_props = schemas["SearchProjectTextRequest"]["properties"]
        .as_object()
        .unwrap();
    assert!(search_props.contains_key("context_before"));
    assert!(search_props.contains_key("context_after"));
    assert!(search_props.contains_key("include_globs"));
    assert!(search_props.contains_key("exclude_globs"));
    assert!(search_props.contains_key("result_mode"));
    assert!(search_props.contains_key("timeout_secs"));
    assert_eq!(search_props["include_globs"]["maxItems"], 32);
    assert_eq!(search_props["include_globs"]["items"]["maxLength"], 256);
    assert_eq!(
        search_props["result_mode"]["enum"],
        json!(["matches", "files_with_matches", "count"])
    );
    let flattened_props = schemas["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    assert_eq!(flattened_props["include_globs"]["maxItems"], 32);
    assert_eq!(flattened_props["include_globs"]["items"]["maxLength"], 256);
    assert_eq!(
        flattened_props["result_mode"]["enum"],
        json!(["matches", "files_with_matches", "count"])
    );
    assert!(
        flattened_schema_alternatives(&flattened_props["timeout_secs"])
            .iter()
            .all(|schema| schema["type"] == "integer")
    );
    // Search timeout is server-clamped; the dedicated SearchProjectTextRequest
    // schema must not reject out-of-range integers with minimum/maximum.
    // ToolCallRequest.timeout_secs is a shared flattened field also used by
    // cargo_*/run_shell (which declare 1..120); do not require it to omit bounds.
    assert!(search_props["timeout_secs"].get("minimum").is_none());
    assert!(search_props["timeout_secs"].get("maximum").is_none());
    let search_timeout_desc = search_props["timeout_secs"]["description"]
        .as_str()
        .unwrap_or("");
    assert!(
        search_timeout_desc.to_ascii_lowercase().contains("clamp"),
        "SearchProjectTextRequest.timeout_secs should document clamp: {search_timeout_desc}"
    );

    let run_shell_description = schemas["RunShellRequest"]["description"]
        .as_str()
        .unwrap_or("");
    assert!(run_shell_description.contains("shell command"));
    let op_description = spec["paths"]["/api/projects/run_shell"]["post"]["description"]
        .as_str()
        .unwrap_or("");
    assert!(op_description.contains("failure_kind"));
    assert!(op_description.contains("tool_failure"));
}

#[test]
fn openapi_dedicated_project_action_schemas_include_optional_session_id() {
    let spec = build_openapi_spec();
    let schemas = &spec["components"]["schemas"];
    for name in [
        "ReadProjectFileRequest",
        "RunShellRequest",
        "WriteProjectFileRequest",
        "ProjectIdRequest",
        "ProjectGitDiffRequest",
        "SearchProjectTextRequest",
        "ApplyPatchRequest",
        "ApplyPatchCheckedRequest",
        "ValidatePatchRequest",
        "DeleteProjectFilesRequest",
        "GitRestorePathsRequest",
        "DiscardUntrackedRequest",
        "StartProjectShellJobRequest",
        "ListProjectFilesRequest",
    ] {
        let schema = &schemas[name];
        assert!(
            schema["properties"].get("session_id").is_some(),
            "{name} missing optional session_id property"
        );
        assert_eq!(
            schema["properties"]["session_id"]["description"], SESSION_ID_FIELD_DESCRIPTION,
            "{name} session_id description should match dedicated action guidance"
        );
        let required = schema["required"].as_array().unwrap();
        assert!(
            !required.iter().any(|field| field == "session_id"),
            "{name} must not require session_id"
        );
    }
}

#[test]
fn openapi_call_runtime_tool_params_is_explicit_object() {
    // callRuntimeTool's ToolCallRequest must declare `params` as a property
    // that is an OpenAPI 3.1 object accepting arbitrary tool arguments.
    // GPT Actions sometimes mishandles free-form object params, which is
    // why dedicated typed actions are preferred; this test pins the schema
    // so `params` stays present and object-typed for advanced callers.
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let properties = tool_call["properties"].as_object().unwrap();
    assert!(
        properties.contains_key(TOOL_CALL_PARAMS_FIELD),
        "ToolCallRequest must declare a `params` property"
    );
    let params = &properties[TOOL_CALL_PARAMS_FIELD];
    assert_eq!(params["type"], "object", "params must be type object");
    assert_eq!(params["nullable"], true, "params must allow null");
    assert_eq!(
        params["additionalProperties"], true,
        "params must allow arbitrary object properties"
    );
    let description = tool_call["description"].as_str().unwrap_or("");
    assert!(
            description.contains(TOOL_CALL_RECORDING_SESSION_ID_FIELD)
                && description.contains("flattened top-level fields"),
            "ToolCallRequest should document GPT Action flattened fields and recorder metadata: {description}"
        );
    for phrase in [
        "record this wrapper call in the session ledger",
        "process-local cache can be restored after restart",
        "hashed durable projection",
        "missing identity never falls back",
        "session_id or recording_session_id still wins",
    ] {
        assert!(
            description.contains(phrase),
            "ToolCallRequest should document {phrase}: {description}"
        );
    }
    let properties = tool_call["properties"].as_object().unwrap();
    let recording_desc = properties[TOOL_CALL_RECORDING_SESSION_ID_FIELD]["description"]
        .as_str()
        .unwrap_or("");
    assert!(
        recording_desc.contains("session ledger"),
        "recording_session_id should mention session ledger: {recording_desc}"
    );
    let session_desc = properties["session_id"]["description"]
        .as_str()
        .unwrap_or("");
    assert!(
        session_desc.contains("explicit tool session")
            && session_desc.contains("wins over current-session binding"),
        "session_id should describe explicit session priority: {session_desc}"
    );
    let start_example = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
        ["application/json"]["examples"]["trackedSession"]["value"];
    assert_eq!(start_example["mode"], "read_only");
    assert_eq!(start_example["title"], "implement show_changes follow-up");
    // `tool` remains required; `params` is optional (advanced callers may
    // omit it for argument-less tools).
    let required = tool_call["required"].as_array().unwrap();
    assert!(required
        .iter()
        .any(|v| v.as_str() == Some(TOOL_CALL_TOOL_FIELD)));
}

#[test]
fn openapi_call_runtime_tool_documents_arguments_alias() {
    // Phase 2: ToolCallRequest must document the `arguments` compatibility
    // alias and state that `params` wins. Both must be object-typed.
    let spec = build_openapi_spec();
    let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        properties.contains_key(TOOL_CALL_ARGUMENTS_FIELD),
        "ToolCallRequest must declare an `arguments` alias property"
    );
    let arguments = &properties[TOOL_CALL_ARGUMENTS_FIELD];
    assert_eq!(arguments["type"], "object", "arguments must be type object");
    assert_eq!(arguments["nullable"], true, "arguments must allow null");
    assert_eq!(
        arguments["additionalProperties"], true,
        "arguments must allow arbitrary object properties"
    );
    let desc_blob =
        serde_json::to_string(&spec["components"]["schemas"]["ToolCallRequest"]).unwrap();
    assert!(
        desc_blob.contains("params") && desc_blob.contains("precedence"),
        "ToolCallRequest description must document params precedence over arguments"
    );
}

#[test]
fn openapi_call_runtime_tool_declares_flattened_action_fields() {
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let properties = tool_call["properties"].as_object().unwrap();
    let accepted_fields = runtime_accepted_flattened_action_fields();

    for field in &accepted_fields {
        assert!(
            properties.contains_key(field),
            "ToolCallRequest.properties.{field} must exist for flattened GPT Action calls"
        );
    }
    for field in properties.keys() {
        if TOOL_CALL_WRAPPER_FIELDS.contains(&field.as_str()) {
            continue;
        }
        assert!(
            accepted_fields.contains(field),
            "ToolCallRequest.properties.{field} is not accepted by any runtime ToolSpec"
        );
    }

    assert!(properties.contains_key(TOOL_CALL_PARAMS_FIELD));
    assert!(properties.contains_key(TOOL_CALL_ARGUMENTS_FIELD));
    assert_eq!(
        properties[ALLOW_CROSS_PROJECT_SESSION_FIELD]["type"],
        "boolean"
    );
    assert_eq!(properties[TOOL_EXPECTED_FAILURE_FIELD]["type"], "boolean");
    assert_eq!(
        properties[TOOL_EXPECTED_FAILURE_KIND_FIELD]["type"],
        "string"
    );
    assert_eq!(properties[TOOL_ASSERTION_NAME_FIELD]["type"], "string");
    let required = tool_call["required"].as_array().unwrap();
    assert_eq!(required, &vec![json!(TOOL_CALL_TOOL_FIELD)]);
    assert_eq!(tool_call["additionalProperties"], false);

    let desc_blob = serde_json::to_string(tool_call).unwrap();
    assert!(
        desc_blob.contains("top-level fields") && desc_blob.contains("params/arguments"),
        "ToolCallRequest must document flattened GPT Action compatibility"
    );
}

#[test]
fn openapi_flattened_arg_table_does_not_overwrite_existing_properties() {
    let mut schemas = json!({
        "ToolCallRequest": {
            "properties": {
                "project": {
                    "type": "string",
                    "description": "Existing project-specific schema."
                },
                "args": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "description": "Existing args-specific schema."
                }
            }
        }
    });

    insert_tool_call_request_flattened_arg_properties(&mut schemas);

    let properties = schemas["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    assert_eq!(
        properties["project"]["description"],
        "Existing project-specific schema."
    );
    assert_eq!(properties["args"]["items"]["type"], "integer");
    assert_eq!(
        properties["command"]["description"],
        FLATTENED_TOOL_ARG_DESCRIPTION
    );
    assert_eq!(
        properties["paths"]["description"],
        FLATTENED_TOOL_ARG_DESCRIPTION
    );
}

#[test]
fn openapi_tool_call_request_exposes_canonical_closeout_and_detail_fields() {
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let properties = tool_call["properties"].as_object().unwrap();

    for field in [
        "include_validation",
        "include_workspace",
        "include_checkpoints",
        "detail",
        "compact",
        "include_recommended_flows",
        "include_risk_summary",
    ] {
        assert!(
            properties.contains_key(field),
            "ToolCallRequest.properties.{field} must exist for flattened GPT Action calls"
        );
    }

    assert_eq!(properties["include_validation"]["type"], "boolean");
    assert_eq!(properties["include_workspace"]["type"], "boolean");
    assert_eq!(properties["include_checkpoints"]["type"], "boolean");
    assert_eq!(properties["detail"]["type"], "string");
    assert_eq!(
        properties["detail"]["enum"],
        json!(["minimal", "standard", "full"])
    );
    assert_eq!(properties["compact"]["type"], "boolean");
    assert_eq!(properties["include_recommended_flows"]["type"], "boolean");
    assert_eq!(properties["include_risk_summary"]["type"], "boolean");
    for removed_startup_field in [
        "compact_startup",
        "include_tool_manifest",
        "tool_manifest_intent",
        "tool_manifest_categories",
        "tool_manifest_limit",
    ] {
        assert!(
            !properties.contains_key(removed_startup_field),
            "removed startup compatibility field {removed_startup_field} must stay absent"
        );
    }
    assert_eq!(
        tool_call["additionalProperties"], false,
        "ToolCallRequest must keep explicit flattened fields with additionalProperties=false"
    );

    let count = operation_ids(&spec).len();
    assert_eq!(count, 25, "GPT Actions operation count must stay 25");
}

#[test]
fn openapi_call_runtime_tool_declares_checkpoint_flattened_fields() {
    // Regression: GPT Action wrapper rejected checkpoint note,
    // include_untracked, checkpoint_id, confirm, and include_diff_stat
    // because ToolCallRequest.properties did not declare them while
    // additionalProperties stayed false. Each flattened field must be
    // an explicit top-level property so GPT Actions accept it.
    let spec = build_openapi_spec();
    let properties = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "note",
        "include_untracked",
        "checkpoint_id",
        "confirm",
        "include_diff_stat",
    ] {
        assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened checkpoint GPT Action calls"
            );
    }
    assert_eq!(properties["note"]["type"], "string");
    assert_eq!(properties["include_untracked"]["type"], "boolean");
    assert_eq!(properties["checkpoint_id"]["type"], "string");
    assert_eq!(properties["confirm"]["type"], "boolean");
    assert_eq!(properties["include_diff_stat"]["type"], "boolean");
    assert_eq!(
        spec["components"]["schemas"]["ToolCallRequest"]["additionalProperties"],
        false
    );
    let count: usize = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|m| m.as_object().unwrap().len())
        .sum();
    assert_eq!(count, 25, "operation count must stay 25");
}

#[test]
fn openapi_call_runtime_tool_declares_apply_text_edits_flattened_fields() {
    // Regression: GPT Action wrapper rejected apply_text_edits changes and
    // dry_run because ToolCallRequest.properties did not declare them.
    // `changes` must mirror the runtime input schema
    // (bounded array of typed items), not a bare free-form object.
    let spec = build_openapi_spec();
    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let properties = tool_call["properties"].as_object().unwrap();
    for field in ["changes", "dry_run"] {
        assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened apply_text_edits GPT Action calls"
            );
    }
    assert_eq!(properties["dry_run"]["type"], "boolean");
    let changes = &properties["changes"];
    assert_eq!(
        changes["type"], "array",
        "changes must be an array, not a bare object"
    );
    assert_eq!(changes["minItems"], 1);
    assert_eq!(changes["maxItems"], 16);
    let items = &changes["items"];
    assert_eq!(items["type"], "object");
    assert_eq!(items["additionalProperties"], false);
    let kind_enum = &items["properties"]["kind"]["enum"]
        .as_array()
        .expect("changes.items.kind must be an enum");
    for variant in ["edit", "create", "delete", "rename"] {
        assert!(
            kind_enum.iter().any(|v| v == variant),
            "changes.items.kind enum must include {variant}"
        );
    }
    assert_eq!(
        tool_call["additionalProperties"], false,
        "additionalProperties must stay false"
    );
    let count: usize = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|m| m.as_object().unwrap().len())
        .sum();
    assert_eq!(count, 25, "operation count must stay 25");
}

#[test]
fn openapi_tools_list_response_includes_names_count_categories_flows() {
    // Phase 2: ToolsListResponse must declare names/count (required) and
    // categories/recommended_flows (optional), while keeping `tools` for
    // backward compatibility.
    let spec = build_openapi_spec();
    let resp = &spec["components"]["schemas"]["ToolsListResponse"];
    let required = resp["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "tools"));
    assert!(required.iter().any(|v| v == "names"));
    assert!(required.iter().any(|v| v == "count"));
    let props = resp["properties"].as_object().unwrap();
    assert!(props.contains_key("tools"));
    assert!(props.contains_key("names"));
    assert!(props.contains_key("count"));
    assert!(props.contains_key("categories"));
    assert!(props.contains_key("recommended_flows"));
    assert!(props.contains_key("total_count"));
    assert!(props.contains_key("filtered_count"));
    assert!(props.contains_key("truncated"));
    assert!(props.contains_key("hint"));
    assert!(props.contains_key("recommended_next"));
    assert_eq!(
        spec["paths"]["/api/tools/list"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/ToolsListRequest"
    );
    let req_props = spec["components"]["schemas"]["ToolsListRequest"]["properties"]
        .as_object()
        .unwrap();
    for field in ["category", "features", "summary_only", "limit"] {
        assert!(
            req_props.contains_key(field),
            "ToolsListRequest must expose bounded field {field}"
        );
    }
    assert_eq!(
        spec["components"]["schemas"]["ToolsListRequest"]["properties"]["limit"]["maximum"],
        100
    );
}

#[test]
fn openapi_tool_spec_includes_output_schema() {
    let spec = build_openapi_spec();
    let tool_spec = &spec["components"]["schemas"]["ToolSpec"];
    let required = tool_spec["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v == "inputSchema"));
    assert!(required.iter().any(|v| v == "outputSchema"));
    assert!(required.iter().any(|v| v == "annotations"));
    let props = tool_spec["properties"].as_object().unwrap();
    assert!(props["inputSchema"].is_object());
    assert!(props["outputSchema"].is_object());
    assert!(props["annotations"].is_object());
    assert_eq!(props["annotations"]["additionalProperties"], true);
}

#[test]
fn openapi_runtime_only_tools_do_not_get_dedicated_paths() {
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    for forbidden in [
        "/api/projects/cargo_fmt",
        "/api/projects/cargo_check",
        "/api/projects/cargo_test",
        "/api/projects/go_test",
        "/api/projects/git_diff_hunks",
        "/api/projects/show_changes",
        "/api/projects/workspace_checkpoint_create",
        "/api/projects/workspace_checkpoint_list",
        "/api/projects/workspace_checkpoint_show",
        "/api/projects/workspace_checkpoint_restore",
        "/api/projects/workspace_checkpoint_delete",
        "/api/projects/project_overview",
        "/api/projects/write_file",
    ] {
        assert!(
            !paths.contains_key(forbidden),
            "{} must remain runtime-only via callRuntimeTool",
            forbidden
        );
    }
}

#[test]
fn openapi_operation_count_is_twenty_five_after_retiring_legacy_edits() {
    // Phase 3 promoted 10 core runtime tools to dedicated GPT Actions,
    // then later phases promoted run_job plus project onboarding actions.
    // The single-purpose legacy edit tools are retired, while retained editing
    // tools stay on the generic runtime surface. The current GPT Action count
    // is 25 after removing legacy `/api/codex/*` routes.
    // The surface must stay <= 30.
    let spec = build_openapi_spec();
    let count: usize = spec["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|m| m.as_object().unwrap().len())
        .sum();
    assert_eq!(
        count, 25,
        "GPT Actions schema must be 25 operations after retiring legacy edits"
    );
    assert!(count <= 30, "GPT Actions schema must stay <= 30 operations");
}

#[test]
fn openapi_artifact_upload_tools_remain_generic_and_under_action_limit() {
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    for path in [
        "/api/projects/artifact_upload_begin",
        "/api/projects/artifact_upload_chunk",
        "/api/projects/artifact_upload_finish",
        "/api/projects/artifact_upload_abort",
    ] {
        assert!(
            !paths.contains_key(path),
            "{path} must remain runtime-only via callRuntimeTool"
        );
    }

    let ids = operation_ids(&spec);
    for id in &ids {
        assert!(
            !id.contains("ArtifactUpload") && !id.contains("artifactUpload"),
            "artifact upload must not be promoted to a dedicated GPT Action: {id}"
        );
    }
    let count = ids.len();
    assert_eq!(count, 25, "GPT Actions operation count must stay 25");
    assert!(count <= 30, "GPT Actions operation count must stay <= 30");

    let tool_call = &spec["components"]["schemas"]["ToolCallRequest"];
    let tool_desc = tool_call["properties"][TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .unwrap();
    for tool in [
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        assert!(
            tool_desc.contains(tool),
            "callRuntimeTool must document runtime tool {tool}"
        );
    }
    let properties = tool_call["properties"].as_object().unwrap();
    for field in [
        "project",
        "path",
        "content_base64",
        "upload_id",
        "offset",
        "expected_bytes",
        "expected_sha256",
        "mime_type",
        "overwrite",
        "allow_missing",
    ] {
        assert!(
            properties.contains_key(field),
            "ToolCallRequest.properties.{field} must exist for flattened artifact upload calls"
        );
    }
    let path_desc = properties["path"]["description"].as_str().unwrap();
    assert!(
        path_desc.contains("must exactly match the path used by artifact_upload_begin")
            && path_desc.contains("bind upload_id"),
        "path flattened description must explain upload path binding: {path_desc}"
    );
    let upload_id_desc = properties["upload_id"]["description"].as_str().unwrap();
    assert!(
        upload_id_desc.contains("same path from artifact_upload_begin is also required"),
        "upload_id flattened description must mention repeated path: {upload_id_desc}"
    );
    for field in [
        "category",
        "intent",
        "features",
        "summary_only",
        "limit",
        "include_recommended_flows",
        "include_risk_summary",
    ] {
        assert!(
                properties.contains_key(field),
                "ToolCallRequest.properties.{field} must exist for flattened list_tools/tool_manifest calls"
            );
    }
}

#[test]
fn openapi_retained_edit_tools_remain_runtime_only() {
    // Retained edit tools (write_project_file) remain reachable via
    // callRuntimeTool / MCP tools/call, but should not be promoted to
    // dedicated GPT Actions. The removed legacy edit tools are absent.
    let spec = build_openapi_spec();
    let paths = spec["paths"].as_object().unwrap();
    assert!(
        !paths.contains_key("/api/projects/write_file"),
        "write_file must remain runtime-only through callRuntimeTool"
    );
    assert!(
        !paths.contains_key("/api/projects/replace_in_file"),
        "replace_in_file must not have a dedicated path"
    );
    assert!(
        paths.contains_key("/api/projects/run_job"),
        "run_job remains a dedicated execution action"
    );
    assert_eq!(
        spec["paths"]["/api/projects/run_job"]["post"]["operationId"],
        "startProjectShellJob"
    );
    assert!(
        LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/write_file"),
        "write_file must stay in the forbidden guard"
    );
    assert!(
        !LEGACY_FORBIDDEN_PATHS.contains(&"/api/projects/run_job"),
        "run_job must not be in the forbidden guard now that it is a dedicated action"
    );
    let tool_desc = spec["components"]["schemas"]["ToolCallRequest"]["properties"]
        [TOOL_CALL_TOOL_FIELD]["description"]
        .as_str()
        .unwrap();
    for tool in ["write_project_file", "apply_text_edits"] {
        assert!(
            tool_desc.contains(tool),
            "callRuntimeTool must document runtime tool {tool}"
        );
    }
    // `replace_in_file` was removed entirely, so it is neither documented in
    // the model-facing ToolCallRequest description nor a known tool.
    assert!(
        !tool_desc.contains("replace_in_file"),
        "callRuntimeTool must not document the removed replace_in_file tool"
    );
}

#[test]
fn openapi_call_runtime_tool_examples_cover_alias_and_no_params() {
    // Phase 2: callRuntimeTool examples should demonstrate the arguments
    // alias and the argument-less (params omitted) shapes so a custom GPT
    // has concrete templates for both.
    let spec = build_openapi_spec();
    let examples = &spec["paths"]["/api/tools/call"]["post"]["requestBody"]["content"]
        ["application/json"]["examples"];
    let keys: Vec<&str> = examples
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert!(
        keys.iter()
            .any(|k| examples[*k]["value"]["arguments"].is_object()),
        "callRuntimeTool examples should include an arguments-alias variant"
    );
    assert!(
        keys.iter().any(|k| {
            let v = &examples[*k]["value"];
            v["tool"].as_str() == Some("list_tools") && v.get("params").is_none()
        }),
        "callRuntimeTool examples should include an argument-less variant"
    );
}

#[test]
fn openapi_spec_serializes_as_valid_json() {
    // Building the spec must not panic and must produce a JSON object with
    // the top-level OpenAPI 3.1 keys ChatGPT expects.
    let spec = build_openapi_spec();
    assert_eq!(spec["openapi"], "3.1.0");
    assert!(spec["info"]["title"].is_string());
    assert!(spec["info"]["version"].is_string());
    assert!(spec["servers"].is_array());
    assert!(spec["paths"].is_object());
    assert!(spec["components"]["schemas"].is_object());
    assert!(spec["security"].is_array());
}

#[test]
fn openapi_exposes_get_runtime_status_action() {
    let spec = build_openapi_spec();
    assert_eq!(
        spec["paths"]["/api/runtime/status"]["post"]["operationId"],
        "getRuntimeStatus"
    );
    let description = spec["paths"]["/api/runtime/status"]["post"]["description"]
        .as_str()
        .unwrap();
    assert!(description.contains("observability"));
    assert!(description.contains("stale_count"));
    assert!(!description.contains("offline_count"));
}
