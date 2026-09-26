use super::*;
use webcodex_core::runner_protocol::RAW_SHELL_COMMAND_MAX_BYTES;
use webcodex_core::workflow_session_contract::EXECUTION_PURPOSE_VALUES;

macro_rules! assert_schema_fields {
    (
        $properties:expr,
        $context:expr,
        present: [$($present:expr),* $(,)?]
        $(, absent: [$($absent:expr),* $(,)?])?
        $(,)?
    ) => {{
        let properties = $properties;
        let context = $context;
        $(
            assert!(
                properties.contains_key($present),
                "{context}: missing schema field {}",
                $present
            );
        )*
        $(
            $(
                assert!(
                    !properties.contains_key($absent),
                    "{context}: unexpected schema field {}",
                    $absent
                );
            )*
        )?
    }};
}

#[test]
fn tool_specs_names_are_unique() {
    let specs = registered_tool_specs();
    let mut names = specs
        .iter()
        .map(|spec| spec.name.clone())
        .collect::<Vec<_>>();
    names.sort();
    let mut deduped = names.clone();
    deduped.dedup();
    assert_eq!(names, deduped, "tool names must be unique");
}

#[test]
fn tool_specs_names_are_snake_case() {
    for spec in registered_tool_specs() {
        assert!(!spec.name.contains('-'));
        assert!(
            spec.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "tool '{}' should be snake_case",
            spec.name
        );
    }
}

#[test]
fn tool_specs_derive_contract_fields_from_name() {
    for spec in registered_tool_specs() {
        assert_eq!(
            spec.output_schema,
            output_schema_for_tool(&spec.name),
            "{}",
            spec.name
        );
        assert_eq!(
            spec.annotations,
            tool_annotations(&spec.name),
            "{}",
            spec.name
        );
    }
}

#[test]
fn tool_specs_input_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.input_schema;
        assert!(schema.is_object(), "{}", spec.name);
        assert_eq!(schema["type"].as_str(), Some("object"), "{}", spec.name);
        assert!(schema["properties"].is_object(), "{}", spec.name);
        assert!(schema["required"].is_array(), "{}", spec.name);
        assert_eq!(schema["additionalProperties"], false, "{}", spec.name);
    }
}

#[test]
fn search_project_texts_query_schema_declares_bounded_advanced_inputs() {
    let specs = registered_tool_specs();
    let search = spec_named(&specs, "search_project_texts");
    let properties = search.input_schema["properties"]["queries"]["items"]["properties"]
        .as_object()
        .unwrap();

    assert_schema_fields!(
        properties,
        "search_project_texts query schema",
        present: ["include_globs", "exclude_globs", "result_mode", "timeout_secs"]
    );
    for field in ["include_globs", "exclude_globs"] {
        assert_eq!(properties[field]["type"], "array");
        assert_eq!(properties[field]["maxItems"], 32);
        assert_eq!(properties[field]["items"]["type"], "string");
        assert_eq!(properties[field]["items"]["minLength"], 1);
        assert_eq!(properties[field]["items"]["maxLength"], 256);
    }
    assert!(properties["timeout_secs"].get("minimum").is_none());
    assert!(properties["timeout_secs"].get("maximum").is_none());
    assert_eq!(properties["timeout_secs"]["type"], "integer");
    assert_eq!(properties["timeout_secs"]["default"], 30);
    assert!(properties["timeout_secs"]["description"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("clamp"));
    assert_eq!(
        properties["result_mode"]["enum"],
        json!(["matches", "files_with_matches", "count"])
    );
}

#[test]
fn read_files_snapshot_fence_schema_matches_read_revision_contract() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "read_files").input_schema;
    let fence = &schema["properties"]["items"]["items"]["properties"]["expected_read_revision"];
    assert_eq!(fence["type"], "integer");
    assert_eq!(fence["minimum"], 1);
    assert_eq!(fence["maximum"], 9_007_199_254_740_991_u64);
    let description = fence["description"].as_str().unwrap_or_default();
    assert!(description.contains("suggested_call"));
    assert!(description.contains("should not invent"));

    let request = |revision: Value| {
        json!({
            "project": "demo",
            "items": [{"path": "src/lib.rs", "expected_read_revision": revision}]
        })
    };
    assert!(
        test_support::validate_schema_instance(&request(json!(3_817_291_045_227_u64)), schema)
            .is_ok()
    );
    for invalid in [
        json!(0),
        json!(9_007_199_254_740_992_u64),
        json!("3817"),
        Value::Null,
    ] {
        assert!(test_support::validate_schema_instance(&request(invalid), schema).is_err());
    }
}

#[test]
fn batch_inspection_result_budget_schema_defers_bounds_to_runtime_clamp() {
    let specs = registered_tool_specs();
    for name in ["read_files", "search_project_texts"] {
        let schema = &spec_named(&specs, name).input_schema;
        let budget = &schema["properties"]["max_result_bytes"];
        assert_eq!(budget["type"], "integer", "{name}");
        assert_eq!(budget["default"], 64 * 1024, "{name}");
        assert_eq!(budget["minimum"], 0, "{name}");
        assert!(budget.get("maximum").is_none(), "{name}");
        assert!(budget["description"]
            .as_str()
            .unwrap_or_default()
            .contains("runtime-clamped"));

        let request = |max_result_bytes: Value| match name {
            "read_files" => json!({
                "project": "demo",
                "items": [{"path": "src/lib.rs"}],
                "max_result_bytes": max_result_bytes
            }),
            _ => json!({
                "project": "demo",
                "queries": [{"pattern": "needle"}],
                "max_result_bytes": max_result_bytes
            }),
        };
        for bytes in [0, 1, 256 * 1024, 512 * 1024, 1024 * 1024] {
            assert!(
                test_support::validate_schema_instance(&request(json!(bytes)), schema).is_ok(),
                "{name}: recognized integer budget should reach runtime normalization"
            );
        }
        assert!(test_support::validate_schema_instance(&request(json!(-1)), schema).is_err());
        assert!(test_support::validate_schema_instance(&request(json!("65536")), schema).is_err());
        assert!(test_support::validate_schema_instance(&request(json!(1.5)), schema).is_err());
    }
}

#[test]
fn git_diff_hunks_page_budget_schema_defers_bounds_to_runtime_clamp() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "git_diff_hunks").input_schema;
    let page = &schema["properties"]["max_page_bytes"];
    assert_eq!(page["type"], "integer");
    assert_eq!(
        page["default"],
        webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert_eq!(
        webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES,
        webcodex_core::runtime_contract::MAX_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert_eq!(page["minimum"], 0);
    assert!(page.get("maximum").is_none());
    let description = page["description"].as_str().unwrap().to_ascii_lowercase();
    assert!(description.contains("producer page"));
    assert!(description.contains("final serialized model result"));
    assert!(description.contains("runtime-clamped"));
    let default_kib = webcodex_core::runtime_contract::DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES / 1024;
    let min_kib = webcodex_core::runtime_contract::MIN_GIT_DIFF_HUNKS_PAGE_BYTES / 1024;
    let max_kib = webcodex_core::runtime_contract::MAX_GIT_DIFF_HUNKS_PAGE_BYTES / 1024;
    assert!(description.contains(&format!("{default_kib} kib")));
    assert!(description.contains(&format!("{min_kib}..{max_kib} kib")));
    for bytes in [0, 1, 16 * 1024, 64 * 1024, 192 * 1024, 300_000] {
        assert!(test_support::validate_schema_instance(
            &json!({"project":"demo","max_page_bytes":bytes}),
            schema,
        )
        .is_ok());
    }
    assert!(test_support::validate_schema_instance(
        &json!({"project":"demo","max_page_bytes":-1}),
        schema,
    )
    .is_err());
    for invalid in [json!("65536"), json!(1.5)] {
        assert!(test_support::validate_schema_instance(
            &json!({"project":"demo","max_page_bytes":invalid}),
            schema,
        )
        .is_err());
    }
}

#[test]
fn git_diff_hunks_schema_leaves_committed_range_relationships_to_runtime() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "git_diff_hunks").input_schema;
    let base = "a".repeat(40);
    let head = "b".repeat(40);

    for request in [
        json!({"project":"demo","base_commit":base,"head_commit":head}),
        json!({"project":"demo","base_commit":base,"head_commit":head,"cached":false}),
        json!({"project":"demo","cached":false}),
        json!({"project":"demo","cached":true}),
    ] {
        assert!(test_support::validate_schema_instance(&request, schema).is_ok());
    }
    // committed-range pairing/cached conflicts are semantic runtime rules, not
    // structural JSON Schema rules. Runtime tests cover both failure modes.
    assert!(test_support::validate_schema_instance(
        &json!({"project":"demo","base_commit":base,"head_commit":head,"cached":true}),
        schema,
    )
    .is_ok());
    assert!(test_support::validate_schema_instance(
        &json!({"project":"demo","base_commit":base,"cached":false}),
        schema,
    )
    .is_ok());
    assert!(test_support::validate_schema_instance(
        &json!({"project":"demo","base_commit":base,"head_commit":head,"unknown":false}),
        schema,
    )
    .is_err());
}

#[test]
fn list_project_files_paging_schema_keeps_cardinality_bounded() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "list_project_files").input_schema;
    let properties = &schema["properties"];
    assert_eq!(properties["limit"]["default"], 200);
    assert!(properties["limit"]["description"]
        .as_str()
        .unwrap()
        .contains("1..500"));
    assert_eq!(properties["offset"]["minimum"], 0);
    assert_eq!(properties["offset"]["default"], 0);
    assert!(properties["offset"]["description"]
        .as_str()
        .unwrap()
        .contains("next_offset"));
    assert!(test_support::validate_schema_instance(
        &json!({"project":"demo","limit":200,"offset":500}),
        schema,
    )
    .is_ok());
}

#[test]
fn execution_timeout_schemas_keep_runtime_bounds_and_hide_sync_wait_tuning() {
    let specs = registered_tool_specs();
    for (name, default) in [
        ("cargo_check", 600),
        ("cargo_test", 1800),
        ("go_test", 1800),
    ] {
        let spec = spec_named(&specs, name);
        let timeout = &spec.input_schema["properties"]["timeout_secs"];
        assert_eq!(timeout["type"], "integer", "{name}");
        assert_eq!(timeout["minimum"], 1, "{name}");
        assert!(timeout.get("maximum").is_none(), "{name}");
        assert_eq!(timeout["default"], default, "{name}");
        let desc = timeout["description"].as_str().unwrap_or("");
        assert!(desc.contains("3600") && desc.to_ascii_lowercase().contains("job"));
        assert!(
            spec.input_schema["properties"]
                .get("sync_wait_secs")
                .is_none(),
            "{name} must hide sync_wait_secs from model discovery"
        );
    }

    let cargo_fmt = spec_named(&specs, "cargo_fmt");
    let timeout = &cargo_fmt.input_schema["properties"]["timeout_secs"];
    assert_eq!(timeout["type"], "integer");
    assert_eq!(timeout["minimum"], 1);
    assert!(timeout.get("maximum").is_none());
    assert_eq!(timeout["default"], 120);
    assert!(cargo_fmt.input_schema["properties"]
        .get("sync_wait_secs")
        .is_none());

    for name in [
        "run_process",
        "run_script",
        "run_shell",
        "run_skill_resource",
    ] {
        let spec = spec_named(&specs, name);
        assert!(
            spec.input_schema["properties"]
                .get("sync_wait_secs")
                .is_none(),
            "{name} must hide sync_wait_secs from model discovery"
        );
    }

    let run_shell = spec_named(&specs, "run_shell");
    let timeout = &run_shell.input_schema["properties"]["timeout_secs"];
    assert_eq!(timeout["type"], "integer");
    assert_eq!(timeout["minimum"], 1);
    assert!(timeout.get("maximum").is_none());
    assert_eq!(timeout["default"], 60);
    let timeout_desc = timeout["description"].as_str().unwrap_or_default();
    assert!(timeout_desc.contains("shared structured-execution ceiling"));
    assert!(!timeout_desc.contains("120 are accepted and clamped"));

    let search = spec_named(&specs, "search_project_texts");
    assert!(
        search.input_schema["properties"]["queries"]["items"]["properties"]["timeout_secs"]
            .get("maximum")
            .is_none()
    );
}

#[test]
fn cargo_check_schema_exposes_bounded_multi_package_selection() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "cargo_check").input_schema;
    let packages = &schema["properties"]["packages"];

    assert_eq!(packages["type"], "array");
    assert_eq!(packages["minItems"], 1);
    assert!(packages["maxItems"].as_u64().is_some());
    assert_eq!(packages["items"]["minLength"], 1);
    assert_eq!(packages["items"]["maxLength"], 500);
    assert!(packages["description"]
        .as_str()
        .is_some_and(|description| description.contains("package")));
}

#[test]
fn cargo_test_schema_explains_execution_proof_policy() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "cargo_test");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    let filter = properties["filter"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(filter.contains("substring"), "{filter}");
    assert!(filter.contains("cargo test FILTER"), "{filter}");
    assert!(filter.contains("--exact"), "{filter}");
    assert!(filter.contains("full qualified name"), "{filter}");
    let lib = &properties["lib"];
    assert_eq!(lib["type"], "boolean");
    let lib_description = lib["description"].as_str().unwrap_or_default();
    assert!(lib_description.contains("--lib"), "{lib_description}");
    assert!(lib_description.contains("Omission"), "{lib_description}");
    assert!(lib_description.contains("false"), "{lib_description}");
    let require_tests = properties["require_tests"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(require_tests.contains("Omission"), "{require_tests}");
    assert!(require_tests.contains("non-zero"), "{require_tests}");
    assert!(
        require_tests.contains("false is an explicit opt-out"),
        "{require_tests}"
    );
    assert!(
        require_tests.contains("true requires proof"),
        "{require_tests}"
    );

    let no_run = properties["no_run"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(no_run.contains("compile-only"), "{no_run}");
    assert!(
        no_run.contains("does not require executed-test-count proof"),
        "{no_run}"
    );
    assert!(spec
        .description
        .contains("Normal execution requires non-zero"));
    assert!(spec.description.contains("require_tests=false opts out"));
    assert!(spec.description.contains("no_run=true is compile-only"));
    assert!(spec.description.contains("Rust substring"));
    assert!(spec.description.contains("--exact"));
    assert!(spec.description.contains("lib=true"));
    assert!(spec.description.contains("--lib"));
    assert!(spec
        .description
        .contains("zero-test results are not validation proof"));
}

#[test]
fn raw_shell_tools_expose_the_shared_authored_command_bound() {
    let specs = registered_tool_specs();
    for name in ["run_shell", "run_job", "session_shell_exec"] {
        let spec = spec_named(&specs, name);
        let command = &spec.input_schema["properties"]["command"];
        assert_eq!(command["maxLength"], RAW_SHELL_COMMAND_MAX_BYTES, "{name}");
        let description = command["description"].as_str().unwrap_or_default();
        assert!(description.contains("65536") || description.contains("65,536"));
    }
}

#[test]
fn execution_purpose_schemas_share_canonical_vocabulary_and_validators_do_not_accept_it() {
    let specs = registered_tool_specs();
    for name in [
        "run_process",
        "run_script",
        "run_shell",
        "run_job",
        "session_shell_exec",
    ] {
        let purpose = &spec_named(&specs, name).input_schema["properties"]["purpose"];
        assert_eq!(purpose["enum"], json!(EXECUTION_PURPOSE_VALUES), "{name}");
        assert!(purpose["description"]
            .as_str()
            .is_some_and(|description| description.contains("caller-declared evidence intent")));
    }
    for name in ["cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        let properties = spec_named(&specs, name).input_schema["properties"]
            .as_object()
            .unwrap();
        assert!(
            !properties.contains_key("purpose"),
            "{name} validation purpose must be Runtime-derived"
        );
    }
}

#[test]
fn run_process_schema_is_small_bounded_and_has_no_shell_or_environment_input() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "run_process");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "executable"])
    );
    assert_schema_fields!(
        properties,
        "run_process input schema",
        present: ["project", "executable", "args", "cwd", "stdin", "timeout_secs", "purpose", "session_id"],
        absent: ["shell", "env", "environment"]
    );
    assert_eq!(properties["executable"]["minLength"], 1);
    assert_eq!(properties["executable"]["maxLength"], 1024);
    assert_eq!(properties["args"]["type"], "array");
    assert_eq!(properties["args"]["maxItems"], 256);
    assert_eq!(properties["args"]["items"]["type"], "string");
    assert_eq!(properties["args"]["items"]["maxLength"], 8192);
    assert_eq!(properties["args"]["default"], json!([]));
    assert_eq!(
        properties["stdin"]["anyOf"],
        json!([{"type": "string", "maxLength": 65_536}, {"type": "null"}])
    );
    assert_eq!(properties["cwd"]["maxLength"], 1024);
    assert_eq!(properties["timeout_secs"]["minimum"], 1);
    assert!(properties["timeout_secs"].get("maximum").is_none());
    assert_eq!(properties["timeout_secs"]["default"], 60);
    assert_eq!(spec.input_schema["additionalProperties"], false);
}

#[test]
fn run_script_schema_is_typed_bounded_and_hides_execution_infrastructure() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "run_script");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "language", "script"])
    );
    assert_schema_fields!(
        properties,
        "run_script input schema",
        present: ["project", "language", "script", "args", "stdin", "cwd", "timeout_secs", "purpose", "session_id"],
        absent: ["command", "executable", "interpreter", "interpreter_path", "interpreter_args", "shell", "env", "environment", "temp_file", "profile", "pty", "allow_cross_project_session"]
    );
    assert_eq!(
        properties["language"]["enum"],
        json!([
            "sh",
            "bash",
            "powershell",
            "python",
            "javascript",
            "typescript"
        ])
    );
    let language_description = properties["language"]["description"]
        .as_str()
        .expect("run_script language description");
    for phrase in [
        "temporary .py file",
        ".mjs ESM",
        ".mts ESM",
        "erasable type stripping",
        "Node.js 22.6.0 or newer",
        "callers cannot provide a runtime path or runtime flags",
    ] {
        assert!(
            language_description.contains(phrase),
            "run_script language description is missing {phrase:?}: {language_description}"
        );
    }
    assert_eq!(properties["script"]["minLength"], 1);
    assert_eq!(properties["script"]["maxLength"], 512 * 1024);
    assert_eq!(properties["args"]["type"], "array");
    assert_eq!(properties["args"]["maxItems"], 256);
    assert_eq!(properties["args"]["items"]["type"], "string");
    assert_eq!(properties["args"]["items"]["maxLength"], 8192);
    assert_eq!(properties["args"]["default"], json!([]));
    assert_eq!(
        properties["stdin"]["anyOf"],
        json!([{"type": "string", "maxLength": 65_536}, {"type": "null"}])
    );
    assert_eq!(properties["cwd"]["maxLength"], 1024);
    assert_eq!(properties["timeout_secs"]["minimum"], 1);
    assert!(properties["timeout_secs"].get("maximum").is_none());
    assert_eq!(properties["timeout_secs"]["default"], 60);
    assert_eq!(spec.input_schema["additionalProperties"], false);
}

#[test]
fn execution_timeout_schema_descriptions_keep_form_specific_lifetime_ceilings() {
    let specs = registered_tool_specs();
    for name in ["run_process", "run_script", "run_detached_process"] {
        let timeout = &spec_named(&specs, name).input_schema["properties"]["timeout_secs"];
        assert_eq!(timeout["minimum"], 1, "{name}");
        assert_eq!(timeout["default"], 60, "{name}");
        assert!(timeout.get("maximum").is_none(), "{name}");
        let description = timeout["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} timeout description"));
        assert!(
            description.contains("execution lifetime"),
            "{name}: {description}"
        );
        assert!(description.contains("604800"), "{name}: {description}");
        assert!(description.contains("7 days"), "{name}: {description}");
    }

    for name in ["run_shell", "run_skill_resource", "cargo_check"] {
        let timeout = &spec_named(&specs, name).input_schema["properties"]["timeout_secs"];
        let description = timeout["description"]
            .as_str()
            .unwrap_or_else(|| panic!("{name} timeout description"));
        assert!(description.contains("3600"), "{name}: {description}");
        assert!(
            !description.contains("604800"),
            "{name} must retain the one-hour ceiling: {description}"
        );
    }
}

#[test]
fn cargo_fmt_conditional_timeout_schema_matches_contract() {
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "cargo_fmt").input_schema;
    let validates = |value: &Value| test_support::validate_schema_instance(value, schema).is_ok();
    let check_description = schema["properties"]["check"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(check_description.contains("pure read-only"));
    assert!(check_description.contains("ensure formatting"));
    let timeout_description = schema["properties"]["timeout_secs"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(timeout_description.contains("precheck plus any required mutation"));

    assert!(validates(
        &json!({"project": "demo", "check": true, "timeout_secs": 3600})
    ));
    assert!(!validates(
        &json!({"project": "demo", "check": true, "timeout_secs": 3600, "sync_wait_secs": 1})
    ));
    assert!(validates(
        &json!({"project": "demo", "check": true, "timeout_secs": 3601})
    ));
    assert!(validates(
        &json!({"project": "demo", "check": false, "timeout_secs": 120})
    ));
    assert!(!validates(
        &json!({"project": "demo", "check": false, "timeout_secs": 120, "sync_wait_secs": 1})
    ));
    assert!(!validates(
        &json!({"project": "demo", "timeout_secs": 120, "sync_wait_secs": 1})
    ));
    assert!(validates(
        &json!({"project": "demo", "check": false, "timeout_secs": 121})
    ));
    assert!(validates(&json!({"project": "demo", "timeout_secs": 121})));
    for structurally_valid in [
        json!({"project": "demo", "check": true, "result_expectation": "failure"}),
        json!({"project": "demo", "check": false, "result_expectation": "failure"}),
        json!({"project": "demo", "result_expectation": "observe"}),
    ] {
        assert!(validates(&structurally_valid));
    }
    // The recorder wrapper validator owns the action-dependent rule instead of
    // encoding it as allOf/if/then in the Host schema.
    assert!(ToolCall::from_tool_name(
        "cargo_fmt",
        json!({"project": "demo", "check": true, "result_expectation": "failure"}),
    )
    .is_ok());
    assert!(ToolCall::from_tool_name(
        "cargo_fmt",
        json!({"project": "demo", "check": false, "result_expectation": "failure"}),
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "cargo_fmt",
        json!({"project": "demo", "result_expectation": "observe"}),
    )
    .is_err());
}

#[test]
fn tool_specs_required_fields_match_declared_properties() {
    for spec in registered_tool_specs() {
        let properties = spec.input_schema["properties"].as_object().unwrap();
        let required = spec.input_schema["required"].as_array().unwrap();
        for field in required {
            let field = field.as_str().unwrap();
            assert!(properties.contains_key(field), "{}: {field}", spec.name);
        }
    }
}

#[test]
fn tool_specs_output_schemas_are_objects() {
    for spec in registered_tool_specs() {
        let schema = &spec.output_schema;
        assert_eq!(schema["type"].as_str(), Some("object"), "{}", spec.name);
        assert!(schema["properties"].is_object(), "{}", spec.name);
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|required| required.iter().any(|value| value == "success")),
            "{} output schema must require success",
            spec.name
        );
    }
}

#[test]
fn tool_specs_optional_fields_are_not_required() {
    let specs = registered_tool_specs();
    let run_shell = spec_named(&specs, "run_shell");
    let required = required_fields(run_shell);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"command".to_string()));
    assert!(!required.contains(&"timeout_secs".to_string()));
    assert!(!required.contains(&"cwd".to_string()));
    assert_eq!(
        run_shell.input_schema["properties"]["timeout_secs"]["minimum"],
        1
    );
    assert!(run_shell.input_schema["properties"]["timeout_secs"]
        .get("maximum")
        .is_none());
    assert_eq!(
        run_shell.input_schema["properties"]["timeout_secs"]["default"],
        60
    );

    let read_files = spec_named(&specs, "read_files");
    let required = required_fields(read_files);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"items".to_string()));
    assert!(!required.contains(&"with_line_numbers".to_string()));

    let search = spec_named(&specs, "search_project_texts");
    let required = required_fields(search);
    assert!(required.contains(&"project".to_string()));
    assert!(required.contains(&"queries".to_string()));
    let query_required = search.input_schema["properties"]["queries"]["items"]["required"]
        .as_array()
        .unwrap();
    assert!(query_required.iter().any(|value| value == "pattern"));
}

#[test]
fn git_log_head_commit_schema_requires_exact_40_hex() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "git_log");
    let head = &spec.input_schema["properties"]["head_commit"];
    assert_eq!(head["type"], "string");
    assert_eq!(head["minLength"], 40);
    assert_eq!(head["maxLength"], 40);
    assert_eq!(head["pattern"], "^[0-9A-Fa-f]{40}$");
    assert!(!required_fields(spec).contains(&"head_commit".to_string()));
}

#[test]
fn tool_specs_covers_expected_tool_set() {
    let names = registered_tool_names();
    for expected in [
        "list_tools",
        "list_projects",
        "list_runners",
        "runtime_status",
        "create_agent_task",
        "list_agent_tasks",
        "read_agent_task",
        "assign_agent_task",
        "start_agent_task_attempt",
        "start_agent_task_coding_run",
        "reconcile_agent_task_coding_run",
        "heartbeat_agent_task_attempt",
        "complete_agent_task_attempt",
        "run_process",
        "run_script",
        "run_shell",
        "run_job",
        "stop_job",
        "read_files",
        "git_status",
        "git_diff_hunks",
        "git_log",
        "show_changes",
        "workspace_hygiene_check",
        #[cfg(feature = "workspace-checkpoints")]
        "workspace_checkpoint_create",
        #[cfg(feature = "workspace-checkpoints")]
        "workspace_checkpoint_list",
        #[cfg(feature = "workspace-checkpoints")]
        "workspace_checkpoint_show",
        #[cfg(feature = "workspace-checkpoints")]
        "workspace_checkpoint_restore",
        #[cfg(feature = "workspace-checkpoints")]
        "workspace_checkpoint_delete",
        "apply_patch",
        "apply_unified_diff",
        "delete_project_files",
        "git_restore_paths",
        "discard_untracked",
        "project_overview",
        "list_project_tracked_files",
        "list_jobs",
        "write_project_file",
        "save_project_artifact",
        "read_project_artifact_metadata",
        "read_project_artifact",
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
        "register_project",
        "create_project",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
}

#[test]
fn bootstrap_agent_conversation_activation_key_is_inbox_only_contract() {
    let specs = registered_tool_specs();
    let bootstrap = spec_named(&specs, "bootstrap_agent_conversation");
    let activation = &bootstrap.input_schema["properties"]["activation_idempotency_key"];
    let description = activation["description"].as_str().unwrap();
    for required in [
        "Inbox-style Wake",
        "OMIT this field",
        "agent_task_attempt",
        "attention_event",
        "agent_wait_events",
        "Endpoint carrier",
    ] {
        assert!(
            description.contains(required),
            "missing {required}: {description}"
        );
    }
    assert!(!required_fields(bootstrap)
        .iter()
        .any(|field| field == "activation_idempotency_key"));
    for required in [
        "Inbox-style Wake",
        "agent_task_attempt",
        "attention_event",
        "agent_wait_events",
        "omit it",
        "Endpoint carrier",
    ] {
        assert!(
            bootstrap.description.contains(required),
            "tool description missing {required}: {}",
            bootstrap.description
        );
    }
}

#[test]
fn heartbeat_agent_task_attempt_active_turn_proof_is_paired_and_server_timed() {
    let specs = registered_tool_specs();
    let heartbeat = spec_named(&specs, "heartbeat_agent_task_attempt");
    assert_eq!(heartbeat.input_schema["additionalProperties"], false);
    let required = required_fields(heartbeat);
    for field in [
        "task_id",
        "attempt_id",
        "assignee_agent_id",
        "attempt_fence",
        "attempt_controller_generation",
    ] {
        assert!(
            required.contains(&field.to_string()),
            "missing required {field}"
        );
    }
    for optional in ["active_turn_wake_id", "active_turn_consume_token"] {
        assert!(!required.contains(&optional.to_string()));
    }
    assert_eq!(
        heartbeat.input_schema["properties"]["active_turn_wake_id"]["pattern"],
        "^wc_wake_[A-Za-z0-9_-]{16}$"
    );
    assert_eq!(
        heartbeat.input_schema["properties"]["active_turn_consume_token"]["pattern"],
        "^wc_wake_consume_[A-Za-z0-9_-]{21}[AQgw]$"
    );
    let properties = heartbeat.input_schema["properties"].as_object().unwrap();
    for forbidden in [
        "lease_ms",
        "lease_duration_ms",
        "duration",
        "duration_ms",
        "lease_expires_at_unix_ms",
        "expires_at_unix_ms",
    ] {
        assert!(
            !properties.contains_key(forbidden),
            "caller-controlled lease field {forbidden}"
        );
    }

    let base = json!({
        "task_id": "wc_agent_task_ERERERERERERERER".to_string(),
        "attempt_id": "wc_agent_task_attempt_IiIiIiIiIiIiIiIi".to_string(),
        "assignee_agent_id": "wc_dagent_MzMzMzMzMzMzMzMz".to_string(),
        "attempt_fence": "wc_agent_task_fence_RERERERERERERERERERERA".to_string(),
        "attempt_controller_generation": 7,
    });
    assert!(test_support::validate_schema_instance(&base, &heartbeat.input_schema).is_ok());

    let mut wake_only = base.clone();
    wake_only["active_turn_wake_id"] = json!("wc_wake_VVVVVVVVVVVVVVVV".to_string());
    assert!(test_support::validate_schema_instance(&wake_only, &heartbeat.input_schema).is_ok());

    let mut token_only = base.clone();
    token_only["active_turn_consume_token"] =
        json!("wc_wake_consume_ZmZmZmZmZmZmZmZmZmZmZg".to_string());
    assert!(test_support::validate_schema_instance(&token_only, &heartbeat.input_schema).is_ok());

    let mut paired = base.clone();
    paired["active_turn_wake_id"] = json!("wc_wake_VVVVVVVVVVVVVVVV".to_string());
    paired["active_turn_consume_token"] =
        json!("wc_wake_consume_ZmZmZmZmZmZmZmZmZmZmZg".to_string());
    assert!(test_support::validate_schema_instance(&paired, &heartbeat.input_schema).is_ok());

    for forbidden in ["lease_ms", "duration_ms", "expires_at_unix_ms"] {
        let mut invalid = paired.clone();
        invalid[forbidden] = json!(30 * 60_000);
        assert!(test_support::validate_schema_instance(&invalid, &heartbeat.input_schema).is_err());
    }
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_exec_schema_keeps_authority_outer_bound_and_source_bounded() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "code_mode_exec");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        properties,
        "code_mode_exec input schema",
        present: ["project", "session_id", "source", "timeout_ms"],
        absent: [
            "recording_session_id",
            "ack_session_context_revision",
            "ack_session_message_ids",
            "ack_ref",
            "context_request",
            "session_message_resolution",
        ]
    );
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "session_id", "source"])
    );
    assert_eq!(properties["source"]["maxLength"], 65_536);
    let valid = json!({
        "project": "agent:special:demo",
        "session_id": format!("wc_sess_{}", "1".repeat(32)),
        "source": "text({hello: 'world'});",
        "timeout_ms": 5_000,
    });
    assert!(test_support::validate_schema_instance(&valid, &spec.input_schema).is_ok());
    let mut override_attempt = valid.clone();
    override_attempt["recording_session_id"] = json!(format!("wc_sess_{}", "2".repeat(32)));
    assert!(test_support::validate_schema_instance(&override_attempt, &spec.input_schema).is_err());
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_effectful_schema_keeps_authority_outer_bound_and_deadline_explicit() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "code_mode_exec_effectful");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        properties,
        "code_mode_exec_effectful input schema",
        present: ["project", "session_id", "source", "timeout_ms"],
        absent: [
            "recording_session_id",
            "ack_session_context_revision",
            "ack_session_message_ids",
            "ack_ref",
            "context_request",
            "session_message_resolution",
        ]
    );
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "session_id", "source"])
    );
    assert_eq!(properties["source"]["maxLength"], 65_536);
    assert!(properties["timeout_ms"]["description"]
        .as_str()
        .unwrap_or_default()
        .contains("decision deadline"));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_mutating_schema_keeps_authority_outer_bound_and_mutation_scope_narrow() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "code_mode_exec_mutating");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert_schema_fields!(
        properties,
        "code_mode_exec_mutating input schema",
        present: ["project", "session_id", "source", "timeout_ms"],
        absent: [
            "recording_session_id",
            "ack_session_context_revision",
            "ack_session_message_ids",
            "ack_ref",
            "context_request",
            "session_message_resolution",
            "state_changed",
        ]
    );
    assert_eq!(
        spec.input_schema["required"],
        json!(["project", "session_id", "source"])
    );
    assert_eq!(properties["source"]["maxLength"], 65_536);
    let source_description = properties["source"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(source_description.contains("at most one canonical apply_text_edits attempt"));
    assert!(
        source_description.contains("cargo_check/cargo_test only after a successful known edit")
    );
    assert!(source_description.contains("source_state"));
    assert!(source_description.contains("never wait inside JS"));
}

#[test]
fn coding_agent_start_keeps_recorder_provenance_out_of_business_input() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "coding_agent_start");
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert!(!properties.contains_key("recording_session_id"));
    assert!(ToolCall::from_tool_name(
        "coding_agent_start",
        json!({
            "project": "demo",
            "provider_id": "codex",
            "idempotency_key": "start-1",
            "instruction": "inspect",
            "recording_session_id": "wc_sess_0123456789abcdef0123456789abcdef"
        }),
    )
    .is_err());
}

#[test]
fn agent_continuation_bind_requires_canonical_view_fence_without_model_exposure() {
    let specs = crate::registry::agent_continuation_app_tool_specs();
    let bind = specs
        .iter()
        .find(|spec| spec.name == "agent_continuation_bind")
        .unwrap();
    assert_eq!(
        bind.input_schema["properties"]["binding_id"]["pattern"],
        "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"
    );
    let mut args = json!({
        "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq".to_string(),
        "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7".to_string(),
        "expected_controller_generation": 1,
        "binding_id": format!("wc_host_binding_{}", webcodex_core::compact::encode([0xa0; 16])),
    });
    assert!(test_support::validate_schema_instance(&args, &bind.input_schema).is_ok());
    for invalid in [
        String::new(),
        format!("wc_binding_{}", "a".repeat(32)),
        format!("wc_host_binding_{}", "A".repeat(32)),
        format!("wc_host_binding_{}", "a".repeat(31)),
        format!("wc_host_binding_{}", "a".repeat(33)),
    ] {
        args["binding_id"] = json!(invalid);
        assert!(test_support::validate_schema_instance(&args, &bind.input_schema).is_err());
    }
    args.as_object_mut().unwrap().remove("binding_id");
    assert!(test_support::validate_schema_instance(&args, &bind.input_schema).is_err());
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == bind.name));
}

#[test]
fn job_terminal_continuation_app_contract_is_exact_wait_plus_private_view_fence_only() {
    let specs = crate::registry::job_terminal_continuation_app_tool_specs();
    assert_eq!(specs.len(), 5);
    let names = specs
        .iter()
        .map(|spec| spec.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "job_terminal_continuation_bind",
            "job_terminal_continuation_state",
            "job_terminal_continuation_prepare",
            "job_terminal_continuation_finish",
            "job_terminal_continuation_unbind",
        ]
    );
    let binding_id = format!(
        "wc_host_binding_{}",
        webcodex_core::compact::encode([0xa1; 16])
    );
    let wait_id = format!("wc_job_wait_{}", webcodex_core::compact::encode([0xa2; 12]));
    let attempt_id = format!(
        "wc_job_delivery_{}",
        webcodex_core::compact::encode([0xa3; 12])
    );
    for spec in &specs {
        let properties = spec.input_schema["properties"].as_object().unwrap();
        assert_eq!(
            properties["wait_id"]["pattern"], "^wc_job_wait_[A-Za-z0-9_-]{16}$",
            "{}",
            spec.name
        );
        assert_eq!(
            properties["binding_id"]["pattern"], "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$",
            "{}",
            spec.name
        );
        for forbidden in [
            "client_window",
            "openai_session",
            "peer_id",
            "principal_digest",
            "session_id",
            "job_id",
            "project",
        ] {
            assert!(
                !properties.contains_key(forbidden),
                "{} unexpectedly accepts {forbidden}",
                spec.name
            );
        }
        assert!(!registered_tool_specs()
            .iter()
            .any(|registered| registered.name == spec.name));
    }

    let bind = specs
        .iter()
        .find(|spec| spec.name == "job_terminal_continuation_bind")
        .unwrap();
    assert!(test_support::validate_schema_instance(
        &json!({"wait_id": wait_id, "binding_id": binding_id}),
        &bind.input_schema,
    )
    .is_ok());
    let finish = specs
        .iter()
        .find(|spec| spec.name == "job_terminal_continuation_finish")
        .unwrap();
    assert_eq!(
        finish.input_schema["properties"]["attempt_id"]["pattern"],
        "^wc_job_delivery_[A-Za-z0-9_-]{16}$"
    );
    assert!(test_support::validate_schema_instance(
        &json!({
            "wait_id": format!("wc_job_wait_{}", webcodex_core::compact::encode([0xa2; 12])),
            "binding_id": format!("wc_host_binding_{}", webcodex_core::compact::encode([0xa1; 16])),
            "attempt_id": attempt_id,
            "outcome": "dispatch_accepted"
        }),
        &finish.input_schema,
    )
    .is_ok());

    let registered = registered_tool_specs();
    let present = spec_named(&registered, "present_job_terminal_continuation");
    assert_eq!(present.input_schema["required"], json!(["wait_id"]));
    assert_eq!(
        present.input_schema["properties"]["wait_id"]["pattern"],
        "^wc_job_wait_[A-Za-z0-9_-]{16}$"
    );
}

#[test]
fn skill_runtime_and_management_schemas_preserve_typed_bounds() {
    let run = input_schema_for_tool("run_skill_resource");
    assert_eq!(run["properties"]["path"]["pattern"], "^scripts/.+$");
    let args_description = run["properties"]["args"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(args_description.contains("package/script execution identity"));
    assert!(args_description.contains("requested Project cwd"));
    assert!(!args_description.contains("stdin-reading"));
    let cwd_description = run["properties"]["cwd"]["description"]
        .as_str()
        .unwrap_or_default();
    assert!(cwd_description.contains("business cwd"));
    assert!(!cwd_description.contains("run_process"));

    let list = input_schema_for_tool("skill_list");
    assert_eq!(list["properties"]["project"]["minLength"], 1);
    assert_eq!(list["properties"]["query"]["maxLength"], 200);
    assert_eq!(list["properties"]["limit"]["maximum"], 64);
    assert_eq!(
        list["properties"]["expected_catalog_revision"]["pattern"],
        "^wc_skillcat_[A-Za-z0-9_-]{43}$"
    );

    let read = input_schema_for_tool("skill_read_file");
    assert_eq!(read["properties"]["project"]["minLength"], 1);
    assert_eq!(
        read["properties"]["skill_id"]["pattern"],
        "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"
    );
    assert_eq!(read["properties"]["path"]["maxLength"], 512);
    assert_eq!(read["properties"]["start_line"]["minimum"], 1);
    assert_eq!(read["properties"]["limit"]["maximum"], 400);
    assert_eq!(
        read["properties"]["expected_definition_revision"]["pattern"],
        "^[0-9a-f]{64}$"
    );

    let versions = input_schema_for_tool("skill_versions");
    assert_eq!(versions["properties"]["skill_key"]["maxLength"], 96);
    assert_eq!(
        versions["properties"]["skill_key"]["pattern"],
        "^[A-Za-z0-9._-]+$"
    );
    assert_eq!(versions["properties"]["limit"]["maximum"], 64);

    let install = input_schema_for_tool("skill_install");
    assert_eq!(install["properties"]["artifact_path"]["maxLength"], 1024);
    assert_eq!(
        install["properties"]["expected_artifact_sha256"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    assert_eq!(install["properties"]["idempotency_key"]["maxLength"], 128);
    assert_eq!(install["properties"]["activate"]["default"], false);

    for name in ["skill_activate", "skill_remove_revision"] {
        let schema = input_schema_for_tool(name);
        assert_eq!(schema["properties"]["skill_key"]["maxLength"], 96, "{name}");
        assert_eq!(
            schema["properties"]["package_revision"]["pattern"], "^wc_skillpkg_[A-Za-z0-9_-]{43}$",
            "{name}"
        );
        assert_eq!(
            schema["properties"]["expected_state_revision"]["pattern"],
            "^wc_skillstate_[A-Za-z0-9_-]{43}$",
            "{name}"
        );
        assert_eq!(
            schema["properties"]["idempotency_key"]["maxLength"], 128,
            "{name}"
        );
    }
}
#[test]
fn process_alias_and_python_are_host_visible_without_opening_objects() {
    for name in ["run_process", "run_detached_process"] {
        let schema = input_schema_for_tool(name);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["argv"]["type"], "array");
        assert_eq!(
            schema["properties"]["argv"]["maxItems"],
            schema["properties"]["args"]["maxItems"]
        );
        assert!(schema["properties"].get("arguments").is_none());
    }
    let schema = input_schema_for_tool("run_script");
    let language = schema["properties"]["language"].to_string();
    assert!(language.contains("python"));
    assert!(!language.contains("python3"));
    let shell = input_schema_for_tool("run_shell");
    assert_eq!(shell["properties"]["login"]["type"], "boolean");
}
