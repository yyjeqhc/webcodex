
#[test]
fn work_on_project_schema_and_registration() {
    let specs = registered_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert!(names.contains(&"work_on_project"), "missing from specs");

    // Workflow bootstrap mutates durable Session state without adding a new
    // interactive approval; its existing runtime:read authority is unchanged.
    let metadata = crate::tool_runtime::metadata::lookup_tool_metadata("work_on_project").unwrap();
    assert_eq!(
        metadata.effect,
        crate::tool_runtime::metadata::ToolEffect::Mutate
    );
    assert_eq!(
        metadata.risk,
        crate::tool_runtime::metadata::ToolRisk::WorkflowManage
    );
    assert_eq!(
        metadata.approval,
        crate::tool_runtime::metadata::ToolApprovalPolicy::None
    );
    assert_eq!(
        metadata.idempotency,
        crate::tool_runtime::metadata::ToolIdempotency::NonIdempotent
    );
    assert!(!metadata.destructive);
    assert!(!metadata.shell_like);
    assert!(metadata.requires_project);
    assert_eq!(
        metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require("runtime:read")
    );
    assert_eq!(
        crate::tool_runtime::tool_definition::runtime_tool_category("work_on_project"),
        "workflow"
    );
    let definition =
        crate::tool_runtime::tool_definition::lookup_tool_definition("work_on_project").unwrap();
    assert_eq!(
        definition.runner_capability,
        Some(crate::tool_runtime::RunnerCapabilityRequirement::GitOrShell)
    );
    assert!(!definition.requires_explicit_business_session());

    // Keep the model-facing schema simple enough for reliable host projection.
    // Project-source exclusivity remains authoritative in ToolCall/runtime parsing.
    let spec = spec_named(&specs, "work_on_project");
    assert_eq!(spec.input_schema["type"], "object");
    assert_eq!(required_fields(spec), vec!["instruction"]);
    assert_eq!(spec.input_schema["additionalProperties"], false);
    let props = spec.input_schema["properties"].as_object().unwrap();
    assert!(!props.contains_key("include_project_instructions"));
    assert!(!props.contains_key("include_workflow_guidance"));
    for field in [
        "project",
        "client_id",
        "path",
        "mode",
        "base_ref",
        "instruction",
        "include_extension_catalog",
        "session_id",
    ] {
        assert!(
            props.contains_key(field),
            "missing explicit {field} property"
        );
    }
    assert_eq!(props["project"]["minLength"], 1);
    assert!(
        props["path"].get("pattern").is_none(),
        "path portability is enforced by ToolCall/runtime parsing, not a POSIX-only schema pattern"
    );
    assert_eq!(props["instruction"]["minLength"], 1);
    assert_eq!(
        props["instruction"]["maxLength"],
        crate::tool_runtime::sessions::MAX_CODING_INSTRUCTION_CHARS
    );
    assert_eq!(props["session_id"]["type"], "string");
    assert_eq!(
        props["session_id"]["pattern"],
        "^(wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})|~s[1-9][0-9]{0,18})$"
    );
    assert_eq!(props["include_extension_catalog"]["type"], "boolean");
    assert_eq!(props["include_extension_catalog"]["default"], true);
    for keyword in [
        "oneOf",
        "anyOf",
        "allOf",
        "not",
        "dependentRequired",
        "if",
        "then",
        "else",
    ] {
        assert!(
            spec.input_schema.get(keyword).is_none(),
            "work_on_project model schema must not use top-level {keyword}"
        );
    }
    let schema_accepts = |value: Value| {
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &value,
            &spec.input_schema,
        )
        .is_ok()
    };
    assert!(schema_accepts(
        json!({"project": SAMPLE_PROJECT, "instruction": "do it"})
    ));
    assert!(!schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "instruction": "do it",
        "include_project_instructions": false
    })));
    assert!(!schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "instruction": "do it",
        "include_workflow_guidance": false
    })));
    assert!(schema_accepts(json!({
        "client_id": "special",
        "path": "/root/git/example",
        "instruction": "do it"
    })));
    assert!(
        schema_accepts(json!({"instruction": "runtime must select the source"})),
        "model schema intentionally advertises a safe source-selection superset"
    );
    assert!(schema_accepts(json!({
        "project": SAMPLE_PROJECT,
        "client_id": "special",
        "path": "/root/git/example",
        "instruction": "runtime must reject ambiguity"
    })));

    // The canonical entry must not expose internal diagnostic controls.
    for hidden in [
        "resume_session_id",
        "deny_write_tools",
        "deny_shell_tools",
        "execution_context",
        "detail",
        "temporary_project_name",
    ] {
        assert!(
            !props.contains_key(hidden),
            "work_on_project schema must not expose {hidden}"
        );
    }

    // Output schema describes the compact projection fields.
    let output = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let output_props = output["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    for field in [
        "session_id",
        "session_ref",
        "project",
        "resolved_project",
        "project_ref",
        "project_resolution",
        "continuation",
        "execution_context",
        "readiness",
        "workspace",
        "worktree",
        "repository",
        "instructions",
        "semantic_navigation",
        "extensions",
        "jobs",
        "blockers",
        "warnings",
        "suggested_call",
        "suggested_next_actions",
    ] {
        assert!(
            output_props.contains_key(field),
            "work_on_project output schema should include {field}"
        );
    }
    for hidden in [
        "workflow",
        "get_runtime_status",
        "connection_state",
        "authority",
        "read_tool_manifest",
        "recommended_flow",
        "startup_verdict",
        "git",
        "continuation_feedback",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            !output_props.contains_key(hidden),
            "work_on_project output schema must not include {hidden}"
        );
    }

    // ToolCall parsing maps the wrapper's session_id to the business accessor.
    let call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do the thing",
            "session_id": "wc_sess_target"
        }),
    )
    .unwrap();
    match &call {
        ToolCall::WorkOnProject {
            include_extension_catalog,
            session_id,
            ..
        } => {
            assert!(*include_extension_catalog);
            assert_eq!(session_id.as_deref(), Some("wc_sess_target"));
        }
        _ => panic!("expected WorkOnProject"),
    }
    assert_eq!(call.project(), Some(SAMPLE_PROJECT));
    assert_eq!(call.session_id(), Some("wc_sess_target"));

    let compact = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do the thing without repeating static context",
            "include_extension_catalog": false
        }),
    )
    .unwrap();
    match compact {
        ToolCall::WorkOnProject {
            include_extension_catalog,
            ..
        } => {
            assert!(!include_extension_catalog);
        }
        _ => panic!("expected WorkOnProject"),
    }
    for removed in ["include_project_instructions", "include_workflow_guidance"] {
        let mut args = json!({"project": SAMPLE_PROJECT, "instruction": "do it"});
        args[removed] = json!(false);
        assert!(ToolCall::from_tool_name("work_on_project", args).is_err());
    }

    let audit = super::super::tool_audit::session_log_arguments_for_tool_request(
        "work_on_project",
        &json!({
            "project": SAMPLE_PROJECT,
            "instruction": "do not persist this full instruction body",
            "include_extension_catalog": false
        }),
    );
    assert!(audit.get("include_project_instructions").is_none());
    assert!(audit.get("include_workflow_guidance").is_none());
    assert_eq!(audit["include_extension_catalog"], false);
    assert_eq!(audit["instruction_present"], true);
    assert!(audit["instruction_summary"].is_string());
    assert!(audit.get("instruction").is_none());
}

#[tokio::test]
async fn work_on_project_extension_catalog_is_defaulted_bounded_and_skips_all_extension_discovery_when_disabled(
) {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    write_project_skill(
        root.path(),
        "00-alpha",
        "duplicate-skill",
        "Alpha selection metadata",
        "ALPHA_PRIVATE_BODY_MUST_NOT_LEAK",
    );
    write_project_skill(
        root.path(),
        "01-beta",
        "duplicate-skill",
        "Beta selection metadata",
        "BETA_PRIVATE_BODY_MUST_NOT_LEAK",
    );
    for index in 2..26 {
        write_project_skill(
            root.path(),
            &format!("{index:02}-bulk"),
            &format!("bulk-{index:02}"),
            &format!("Bulk selection metadata {index:02} {}", "d".repeat(380)),
            "BULK_PRIVATE_BODY_MUST_NOT_LEAK",
        );
    }

    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-ext-skills", "demo", root.path()).await;
    let auth = bootstrap_auth_context();

    let (without_extensions, without_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-skills",
        work_on_project_call_with_extensions(&project, "without extensions", false),
        Some(&auth),
        "wop-ext-skills-off",
    )
    .await;
    assert!(without_extensions.success, "{:?}", without_extensions.error);
    assert!(without_extensions.output.get("extensions").is_none());
    assert!(!without_requests.iter().any(|kind| matches!(
        kind.as_str(),
        "file_skill_list_packages" | "file_skill_read_file"
    )));

    let (with_extensions, with_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-skills",
        work_on_project_call_with_extensions(&project, "with extensions", true),
        Some(&auth),
        "wop-ext-skills-on",
    )
    .await;
    assert!(with_extensions.success, "{:?}", with_extensions.error);
    assert!(with_requests
        .iter()
        .any(|kind| kind == "file_skill_list_packages"));
    assert!(with_requests
        .iter()
        .any(|kind| kind == "file_skill_read_file"));

    let skills = &with_extensions.output["extensions"]["skills"];
    assert_eq!(skills["status"], "available");
    assert_eq!(skills["total_count"], 26);
    assert_eq!(skills["truncated"], true);
    assert!(skills["returned_count"].as_u64().unwrap() < 26);
    let entries = skills["entries"].as_array().unwrap();
    assert!(
        entries.len() >= 2,
        "duplicate fixtures must fit the bounded prefix"
    );
    assert_eq!(entries[0]["name"], "duplicate-skill");
    assert_eq!(entries[0]["name_conflict"], true);
    assert_eq!(entries[0]["source_scope"], "project");
    assert_eq!(entries[0]["trust"], "project_content");
    assert_eq!(entries[1]["name"], "duplicate-skill");
    assert_eq!(entries[1]["name_conflict"], true);

    let plugins = &with_extensions.output["extensions"]["plugins"];
    assert_eq!(plugins["status"], "unavailable");
    assert_eq!(plugins["reason_code"], "plugin_runtime_unavailable");
    let serialized = with_extensions.output.to_string();
    for secret in [
        "ALPHA_PRIVATE_BODY_MUST_NOT_LEAK",
        "BETA_PRIVATE_BODY_MUST_NOT_LEAK",
        "BULK_PRIVATE_BODY_MUST_NOT_LEAK",
        "SKILL.md",
    ] {
        assert!(
            !serialized.contains(secret),
            "startup leaked Skill body/path marker: {secret}"
        );
    }
    let extension_bytes = serde_json::to_vec(&with_extensions.output["extensions"])
        .unwrap()
        .len();
    assert!(
        extension_bytes
            <= crate::tool_runtime::startup_catalog::STARTUP_EXTENSION_CATALOG_HARD_MAX_BYTES,
        "extension payload exceeded hard bound: {extension_bytes}"
    );
    let without_bytes = serde_json::to_vec(&without_extensions.output)
        .unwrap()
        .len();
    let with_bytes = serde_json::to_vec(&with_extensions.output).unwrap().len();
    assert!(with_bytes <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES);
    println!(
        "work_on_project_extension_catalog_bytes without={without_bytes} with={with_bytes} increase={}",
        with_bytes.saturating_sub(without_bytes)
    );
}

#[tokio::test]
async fn work_on_project_extension_catalog_includes_runner_local_configured_skill() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        "wop-ext-configured-skill",
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            skill_runtime: true,
            ..Default::default()
        },
    )
    .await;
    let auth = bootstrap_auth_context();
    let configured_id = "wc_skill_IiIiIiIiIiIiIiIiIiIiIg".to_string();
    let configured_revision = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let (result, requests) = dispatch_startup_with_configured_skill_catalog(
        &runtime,
        "wop-ext-configured-skill",
        work_on_project_call_with_extensions(&project, "discover configured Skill", true),
        &auth,
        RunnerSkillDescriptor::Configured {
            skill_id: configured_id.clone(),
            name: "operator-live-guidance".to_string(),
            description: "Configured live Skill metadata".to_string(),
            definition_revision: configured_revision.to_string(),
        },
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(requests.iter().any(|kind| kind == "skill"));
    let skills = &result.output["extensions"]["skills"];
    assert_eq!(skills["status"], "available");
    assert_eq!(skills["total_count"], 1);
    assert_eq!(skills["returned_count"], 1);
    assert_eq!(skills["truncated"], true);
    assert!(skills["discovery_hint"].is_string());
    let entry = &skills["entries"][0];
    assert_eq!(entry["skill_id"], configured_id);
    assert_eq!(entry["name"], "operator-live-guidance");
    assert_eq!(entry["description"], "Configured live Skill metadata");
    assert_eq!(entry["source_scope"], "runner");
    assert_eq!(entry["trust"], "operator_configured_guidance");
    assert_eq!(entry["name_conflict"], false);
    assert!(!result.output.to_string().contains(configured_revision));
}

#[tokio::test]
async fn work_on_project_plugin_extension_uses_project_catalog_without_binding_or_schema_leakage() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        "wop-ext-plugin",
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            native_tool_plugins: true,
            ..Default::default()
        },
    )
    .await;
    let auth = bootstrap_auth_context();
    let bindings_before = runtime.plugin_gateway.binding_count();
    let (result, requests) = dispatch_startup_with_plugin_catalog(
        &runtime,
        "wop-ext-plugin",
        work_on_project_call_with_extensions(&project, "discover plugin", true),
        &auth,
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(requests.iter().any(|kind| kind == "plugin_project_catalog"));
    assert_eq!(runtime.plugin_gateway.binding_count(), bindings_before);

    let plugins = &result.output["extensions"]["plugins"];
    assert_eq!(plugins["status"], "available");
    assert_eq!(
        plugins["catalog_revision"],
        format!("wc_plugcat_{}", webcodex_core::compact::encode([0xaa; 32]))
    );
    assert_eq!(plugins["total_count"], 1);
    assert_eq!(plugins["returned_count"], 1);
    assert_eq!(plugins["truncated"], false);
    let entry = &plugins["entries"][0];
    assert_eq!(entry["plugin"], "repo-context");
    assert_eq!(entry["tool"], "repo_context");
    assert_eq!(entry["annotations"]["readOnlyHint"], true);
    assert_eq!(entry["annotations"]["destructiveHint"], false);
    let serialized = result.output["extensions"].to_string();
    for forbidden in [
        root.path().to_string_lossy().as_ref(),
        "inputSchema",
        "outputSchema",
        "provider_instance_id",
        "binding",
        "command",
        "argv",
        "cwd",
        "env",
        "stderr",
        "pid",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "startup leaked {forbidden}: {serialized}"
        );
    }
}

#[tokio::test]
async fn work_on_project_plugin_extension_fails_closed_without_plugin_inspect_scope() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    write_project_skill(
        root.path(),
        "alpha",
        "alpha",
        "Visible Skill metadata",
        "PRIVATE_SCOPE_TEST_BODY",
    );
    let runtime = ToolRuntime::new_for_tests();
    let auth = open_auth_context();
    let project = register_runner_project_at_path_with_auth(
        &runtime,
        "wop-ext-scope",
        "demo",
        root.path(),
        &auth,
    )
    .await;
    assert!(!auth.has_scope(crate::auth::SCOPE_PLUGIN_INSPECT));
    let (result, requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-ext-scope",
        work_on_project_call_with_extensions(&project, "scope bounded startup", true),
        Some(&auth),
        "wop-ext-scope-window",
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["extensions"]["skills"]["status"], "available");
    assert_eq!(
        result.output["extensions"]["plugins"]["status"],
        "unavailable"
    );
    assert_eq!(
        result.output["extensions"]["plugins"]["reason_code"],
        "plugin_inspect_scope_unavailable"
    );
    assert!(!requests.iter().any(|kind| kind == "plugin_gateway"));
    assert!(!result
        .output
        .to_string()
        .contains("PRIVATE_SCOPE_TEST_BODY"));
}

#[test]
fn work_on_project_tool_call_enforces_authoritative_source_contract() {
    assert!(
        ToolCall::from_tool_name("work_on_project", json!({})).is_err(),
        "project source and instruction are required"
    );
    assert!(
        ToolCall::from_tool_name("work_on_project", json!({"project": SAMPLE_PROJECT})).is_err(),
        "instruction is required"
    );
    let project_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({"project": SAMPLE_PROJECT, "instruction": "do it"}),
    )
    .unwrap();
    assert_eq!(project_call.project(), Some(SAMPLE_PROJECT));
    let path_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "special",
            "path": "/root/git/example",
            "instruction": "do it"
        }),
    )
    .unwrap();
    assert!(path_call.project().is_none());
    let worktree_call = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "special",
            "path": "/root/git/example",
            "mode": "worktree",
            "base_ref": "origin/main",
            "instruction": "do it in isolation"
        }),
    )
    .unwrap();
    match worktree_call {
        ToolCall::WorkOnProject { mode, base_ref, .. } => {
            assert_eq!(mode.as_deref(), Some("worktree"));
            assert_eq!(base_ref.as_deref(), Some("origin/main"));
        }
        _ => panic!("expected WorkOnProject"),
    }
    for invalid in [
        json!({"instruction": "no source"}),
        json!({"project": SAMPLE_PROJECT, "client_id": "special", "instruction": "mixed"}),
        json!({"project": SAMPLE_PROJECT, "path": "/root/git/example", "instruction": "mixed"}),
        json!({"project": SAMPLE_PROJECT, "client_id": "special", "path": "/root/git/example", "instruction": "mixed"}),
        json!({"client_id": "special", "instruction": "missing path"}),
        json!({"path": "/root/git/example", "instruction": "missing client"}),
    ] {
        assert!(
            ToolCall::from_tool_name("work_on_project", invalid.clone()).is_err(),
            "authoritative parser accepted invalid source form: {invalid}"
        );
    }
    // The schema declares additionalProperties: false so advanced
    // internal diagnostic controls are not part of the canonical entry surface.
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "work_on_project")
        .unwrap();
    assert_eq!(spec.input_schema["additionalProperties"], false);
    let props = spec.input_schema["properties"].as_object().unwrap();
    for hidden in [
        "resume_session_id",
        "deny_write_tools",
        "deny_shell_tools",
        "execution_context",
        "detail",
        "temporary_project_name",
    ] {
        assert!(
            !props.contains_key(hidden),
            "work_on_project schema must not expose {hidden}"
        );
    }
}

#[test]
fn work_on_project_projection_fails_closed_when_required_field_is_missing() {
    let mut output = valid_work_on_project_projection_input();
    output["session"]
        .as_object_mut()
        .unwrap()
        .remove("session_id");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
    assert!(result.output["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("session_id")));
}

#[test]
fn work_on_project_projection_preserves_session_ref() {
    let mut output = valid_work_on_project_projection_input();
    output["session"]["session_ref"] = json!("~s12");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_ref"], "~s12");
}

#[test]
fn work_on_project_projection_emits_typed_window_session_correlation() {
    let mut correlation = crate::tool_runtime::ToolCallCorrelation::default();
    let result =
        crate::tool_runtime::coding_task::project_work_on_project_output_with_correlation_for_test(
            SAMPLE_PROJECT.to_string(),
            valid_work_on_project_projection_input(),
            &mut correlation,
        );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        correlation.resolved_project.as_deref(),
        Some("agent:wop:demo")
    );
    assert_eq!(correlation.workflow_sessions.len(), 1);
    let link = &correlation.workflow_sessions[0];
    assert_eq!(link.session_id, "wc_sess_0123456789abcdef");
    assert_eq!(link.project.as_deref(), Some("agent:wop:demo"));
    assert_eq!(
        link.relation,
        crate::tool_runtime::WorkflowSessionCorrelationRelation::WorkOnProject
    );
}

#[test]
fn work_on_project_projection_fails_closed_for_wrong_field_type() {
    let mut output = valid_work_on_project_projection_input();
    output["workspace"]["conflicts"] = json!("0");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
}

#[test]
fn work_on_project_projection_keeps_safe_coding_agent_discovery_and_rejects_private_fields() {
    let mut input = valid_work_on_project_projection_input();
    input["coding_agent_providers"] = json!([{"provider_id":"pi", "name":"Pi Agent"}]);
    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        input.clone(),
    );
    assert!(result.success);
    assert_eq!(
        result.output["coding_agent_providers"],
        input["coding_agent_providers"]
    );
    input["coding_agent_providers"][0]["provider_instance_id"] = json!("private");
    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        input,
    );
    assert!(!result.success);
}

#[test]
fn work_on_project_projection_fails_closed_for_noncanonical_workflow() {
    let mut output = valid_work_on_project_projection_input();
    output["workflow"]["version"] =
        json!(crate::tool_runtime::startup_brief::BUILTIN_CODING_WORKFLOW_VERSION + 1);

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["field"], "workflow");
    assert_eq!(result.output["state_changed"], true);
}

#[test]
fn work_on_project_projection_does_not_default_missing_instruction_sources() {
    let mut output = valid_work_on_project_projection_input();
    output["instructions"]
        .as_object_mut()
        .unwrap()
        .remove("sources");

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        output,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["error_kind"],
        "work_on_project_projection_failed"
    );
    assert_eq!(result.output["state_changed"], true);
    assert!(result.output["detail"]
        .as_str()
        .is_some_and(|detail| detail.contains("sources")));
}

#[test]
fn work_on_project_projection_is_sparse_for_defaults_and_keeps_noteworthy_state() {
    let default_result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        valid_work_on_project_projection_input(),
    );
    assert!(default_result.success, "{:?}", default_result.error);
    for omitted in [
        "project_resolution",
        "execution_context",
        "readiness",
        "repository",
        "jobs",
        "blockers",
        "warnings",
        "suggested_next_actions",
        "deterministic",
        "llm_summary",
    ] {
        assert!(
            default_result.output.get(omitted).is_none(),
            "boring default field {omitted} should be omitted: {}",
            default_result.output
        );
    }
    assert_eq!(default_result.output["workspace"]["status"], "clean");
    assert_eq!(default_result.output["workspace"]["git"]["status"], "clean");
    assert!(default_result.output["workspace"]["git"]["reason_code"].is_null());
    assert!(default_result.output["workspace"]["branch"].is_string());
    assert!(default_result.output["workspace"]["head"].is_string());
    for omitted in ["git_available", "clean", "conflicts"] {
        assert!(default_result.output["workspace"].get(omitted).is_none());
    }
    assert_eq!(
        default_result.output["instructions"]["content_included"],
        true
    );
    for omitted in ["changed_sources", "truncated", "total_chars"] {
        assert!(default_result.output["instructions"].get(omitted).is_none());
    }

    let mut noteworthy = valid_work_on_project_projection_input();
    noteworthy["session"]["execution_context"] = json!({"default_cwd": "src"});
    noteworthy["project_resolution"] = json!({
        "source": "path",
        "outcome": "auto_registered",
        "resolved_project": "agent:wop:demo",
        "registered": true,
    });
    noteworthy["workspace"]["status"] = json!("blocked");
    noteworthy["workspace"]["conflicts"] = json!(2);
    noteworthy["repository"] = json!({
        "status": "unavailable",
        "reason_code": "probe_failed",
    });
    noteworthy["continuation"]["jobs"]["active_count"] = json!(1);
    noteworthy["continuation"]["jobs"]["latest_status"] = json!("running");
    noteworthy["blockers"] = json!(["workspace_conflicts"]);
    noteworthy["warnings"] = json!(["active_jobs_present"]);
    noteworthy["startup_verdict"] = json!({
        "status": "fail",
        "blocking": true,
        "suggested_next_actions": ["inspect or await blocking active jobs"],
    });
    let noteworthy_result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        noteworthy,
    );
    assert!(noteworthy_result.success, "{:?}", noteworthy_result.error);
    assert_eq!(
        noteworthy_result.output["project_resolution"]["outcome"],
        "auto_registered"
    );
    assert_eq!(
        noteworthy_result.output["execution_context"]["default_cwd"],
        "src"
    );
    assert_eq!(noteworthy_result.output["readiness"]["status"], "fail");
    assert_eq!(
        noteworthy_result.output["repository"]["reason_code"],
        "probe_failed"
    );
    assert_eq!(noteworthy_result.output["workspace"]["conflicts"], 2);
    assert_eq!(noteworthy_result.output["jobs"]["active_count"], 1);
    assert_eq!(noteworthy_result.output["jobs"]["latest_status"], "running");
    assert_eq!(
        noteworthy_result.output["blockers"],
        json!(["workspace_conflicts"])
    );
    assert_eq!(
        noteworthy_result.output["warnings"],
        json!(["active_jobs_present"])
    );
    assert_eq!(
        noteworthy_result.output["suggested_next_actions"],
        json!(["inspect or await blocking active jobs"])
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({ "success": true, "output": noteworthy_result.output });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("noteworthy sparse output must match schema: {error}"));
}

#[test]
fn work_on_project_projection_preserves_observed_tracking_and_dirty_paths() {
    let mut input = valid_work_on_project_projection_input();
    input["workspace"]["status"] = json!("dirty");
    input["workspace"]["git"]["status"] = json!("dirty");
    input["workspace"]["clean"] = json!(false);
    input["workspace"]["upstream_status"] = json!("available");
    input["workspace"]["upstream_reason_code"] = Value::Null;
    input["workspace"]["upstream"] = json!("origin/main");
    input["workspace"]["ahead"] = json!(2);
    input["workspace"]["behind"] = json!(1);
    input["workspace"]["changed_paths"] = json!(["src/a.rs", "src/b.rs"]);
    input["workspace"]["changed_paths_total"] = json!(3);
    input["workspace"]["changed_paths_truncated"] = json!(true);

    let result = crate::tool_runtime::coding_task::project_work_on_project_output(
        SAMPLE_PROJECT.to_string(),
        input,
    );
    assert!(result.success, "{:?}", result.error);
    let workspace = &result.output["workspace"];
    assert_eq!(workspace["status"], "dirty");
    assert_eq!(workspace["upstream_status"], "available");
    assert_eq!(workspace["upstream"], "origin/main");
    assert_eq!(workspace["ahead"], 2);
    assert_eq!(workspace["behind"], 1);
    assert_eq!(workspace["changed_paths"], json!(["src/a.rs", "src/b.rs"]));
    assert_eq!(workspace["changed_paths_total"], 3);
    assert_eq!(workspace["changed_paths_truncated"], true);
    assert!(workspace.get("clean").is_none());

    let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
    let instance = json!({"success": true, "output": result.output});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
        .unwrap_or_else(|error| panic!("tracking startup projection must match schema: {error}"));
}
