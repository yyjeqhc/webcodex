use crate::{
    execution_purpose_for_validation_kind, validation_adapter_for_tool, CargoCheckOptions,
    CargoReadOnlyValidationOperation, CargoTestOptions, GoReadOnlyValidationOperation,
    GoTestOptions, ReadOnlyValidationOperation, ValidationCommandOptions,
    ValidationFailureEvidence,
};
use webcodex_core::runner_protocol::{GO_TEST_PACKAGE_MAX_BYTES, GO_TEST_PACKAGE_MAX_ITEMS};
use webcodex_core::validation_evidence::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, parse_go_test_diagnostics,
    PARSER_KIND, PARSER_VERSION,
};
use webcodex_core::workflow_session_contract::{ExecutionPurpose, EXECUTION_PURPOSE_VALUES};
use webcodex_tool_contracts::{is_known_tool_name, registered_tool_specs, ToolCall};
use webcodex_tool_runtime_contracts::{
    tool_audit::{
        is_structured_validation_target_identity, session_log_arguments_for_tool_request,
    },
    ToolCallAuditProjection,
};

#[test]
fn semantic_read_only_operations_preserve_legacy_leaf_plans_and_identity_profiles() {
    let cases = [
        (
            ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::FormatCheck),
            "cargo_fmt",
            ValidationCommandOptions {
                check: true,
                ..ValidationCommandOptions::default()
            },
        ),
        (
            ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Check(
                CargoCheckOptions {
                    all_targets: Some(false),
                    all_features: Some(true),
                    no_default_features: Some(true),
                    features: Some("feature-a,feature-b".to_string()),
                    package: None,
                    packages: Some(vec!["package-a".to_string(), "package-b".to_string()]),
                },
            )),
            "cargo_check",
            ValidationCommandOptions {
                all_targets: Some(false),
                all_features: Some(true),
                no_default_features: Some(true),
                features: Some("feature-a,feature-b".to_string()),
                cargo_packages: Some(vec!["package-a".to_string(), "package-b".to_string()]),
                ..ValidationCommandOptions::default()
            },
        ),
        (
            ReadOnlyValidationOperation::Cargo(CargoReadOnlyValidationOperation::Test(
                CargoTestOptions {
                    filter: Some("runtime".to_string()),
                    lib: Some(true),
                    all_targets: Some(true),
                    all_features: Some(false),
                    no_default_features: Some(true),
                    features: Some("feature-a".to_string()),
                    package: Some("webcodex".to_string()),
                    no_run: Some(true),
                },
            )),
            "cargo_test",
            ValidationCommandOptions {
                filter: Some("runtime".to_string()),
                lib: Some(true),
                all_targets: Some(true),
                all_features: Some(false),
                no_default_features: Some(true),
                features: Some("feature-a".to_string()),
                package: Some("webcodex".to_string()),
                no_run: Some(true),
                ..ValidationCommandOptions::default()
            },
        ),
        (
            ReadOnlyValidationOperation::Go(GoReadOnlyValidationOperation::Test(GoTestOptions {
                packages: Some(vec![
                    "./internal/control".to_string(),
                    "./internal/node".to_string(),
                ]),
            })),
            "go_test",
            ValidationCommandOptions {
                go_packages: Some(vec![
                    "./internal/control".to_string(),
                    "./internal/node".to_string(),
                ]),
                ..ValidationCommandOptions::default()
            },
        ),
    ];

    for (operation, tool_name, legacy_options) in cases {
        let compatibility = operation.compatibility_profile();
        assert_eq!(compatibility.tool_identity, tool_name);
        assert_eq!(
            compatibility.validation_identity,
            webcodex_tool_contracts::runtime_tool_session_evidence_policy(tool_name)
                .validation_identity
        );

        let semantic_plan = operation.build_readonly_plan().unwrap();
        let legacy_plan = validation_adapter_for_tool(tool_name)
            .unwrap()
            .build_readonly_plan(legacy_options)
            .unwrap();
        assert_eq!(semantic_plan, legacy_plan, "{tool_name}");
        assert_eq!(operation.adapter().tool_identity(), tool_name);
    }
}

#[test]
fn execution_purpose_vocabulary_classification_and_validator_mapping_are_canonical() {
    let cases = [
        ("validation", ExecutionPurpose::Validation, true),
        ("test", ExecutionPurpose::Test, true),
        ("build", ExecutionPurpose::Build, true),
        ("format", ExecutionPurpose::Format, true),
        ("release", ExecutionPurpose::Release, true),
        ("diagnostic", ExecutionPurpose::Diagnostic, false),
        ("operation", ExecutionPurpose::Operation, false),
        ("other", ExecutionPurpose::Other, false),
    ];
    assert_eq!(
        EXECUTION_PURPOSE_VALUES,
        cases.map(|(value, _, _)| value).as_slice()
    );
    for (value, purpose, validation_like) in cases {
        assert_eq!(ExecutionPurpose::parse(value), Some(purpose), "{value}");
        assert_eq!(purpose.as_str(), value, "{value}");
        assert_eq!(purpose.is_validation_like(), validation_like, "{value}");
    }
    assert_eq!(ExecutionPurpose::parse("unknown"), None);

    for (tool, expected) in [
        ("cargo_check", ExecutionPurpose::Validation),
        ("cargo_test", ExecutionPurpose::Test),
        ("cargo_fmt", ExecutionPurpose::Format),
        ("go_test", ExecutionPurpose::Test),
    ] {
        let adapter = validation_adapter_for_tool(tool).expect("structured validation adapter");
        assert_eq!(
            execution_purpose_for_validation_kind(adapter.validation_kind()),
            expected,
            "{tool}"
        );
    }
}

#[test]
fn rust_profile_selects_cargo_fmt_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_fmt").expect("cargo_fmt adapter");
    assert_eq!(adapter.tool_identity(), "cargo_fmt");
    assert_eq!(adapter.validation_kind(), "format");
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                check: true,
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo fmt -- --check"
    );
}

#[test]
fn cargo_fmt_read_only_plan_excludes_mutating_format() {
    let adapter = validation_adapter_for_tool("cargo_fmt").expect("cargo_fmt adapter");
    let readonly = adapter
        .build_readonly_plan(ValidationCommandOptions {
            check: true,
            ..ValidationCommandOptions::default()
        })
        .unwrap();
    assert_eq!(readonly.compatibility_command, "cargo fmt -- --check");
    assert_eq!(readonly.structured_step.name, "format");
    assert_eq!(readonly.structured_step.program, "cargo");
    assert_eq!(readonly.structured_step.args, ["fmt", "--", "--check"]);
    assert!(readonly.structured_step.is_canonical());

    assert!(adapter
        .build_readonly_plan(ValidationCommandOptions::default())
        .is_err());
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions::default())
            .unwrap(),
        "cargo fmt"
    );
}

#[test]
fn rust_profile_selects_cargo_check_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_check").expect("cargo_check adapter");
    assert_eq!(adapter.tool_identity(), "cargo_check");
    assert_eq!(adapter.validation_kind(), "check");
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions::default())
            .unwrap(),
        "cargo check --all-targets"
    );
    let legacy_single = adapter
        .build_command(ValidationCommandOptions {
            package: Some("webcodex".to_string()),
            ..ValidationCommandOptions::default()
        })
        .unwrap();
    let canonical_single = adapter
        .build_command(ValidationCommandOptions {
            cargo_packages: Some(vec!["webcodex".to_string()]),
            ..ValidationCommandOptions::default()
        })
        .unwrap();
    assert_eq!(legacy_single, "cargo check --all-targets -p 'webcodex'");
    assert_eq!(legacy_single, canonical_single);
    assert!(adapter
        .build_command(ValidationCommandOptions {
            features: Some("feat\0x".to_string()),
            ..ValidationCommandOptions::default()
        })
        .is_err());
}

#[test]
fn cargo_check_builds_one_command_with_repeated_package_selectors() {
    let adapter = validation_adapter_for_tool("cargo_check").expect("cargo_check adapter");
    let command = adapter
        .build_command(ValidationCommandOptions {
            cargo_packages: Some(vec![
                "package-a".to_string(),
                "package-b".to_string(),
                "package-c".to_string(),
            ]),
            ..ValidationCommandOptions::default()
        })
        .unwrap();

    assert_eq!(
        command,
        "cargo check --all-targets -p 'package-a' -p 'package-b' -p 'package-c'"
    );
    assert_eq!(command.matches("cargo check").count(), 1);
}

#[test]
fn rust_profile_selects_cargo_test_adapter_and_preserves_command() {
    let adapter = validation_adapter_for_tool("cargo_test").expect("cargo_test adapter");
    assert_eq!(adapter.tool_identity(), "cargo_test");
    assert_eq!(adapter.validation_kind(), "test");
    assert!(adapter.reports_test_run_metadata());
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                filter: Some("tool_runtime".to_string()),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo test 'tool_runtime'"
    );
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                lib: Some(true),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo test --lib"
    );
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                lib: Some(false),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        adapter
            .build_command(ValidationCommandOptions::default())
            .unwrap()
    );
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                lib: Some(true),
                all_targets: Some(true),
                no_run: Some(true),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "cargo test --lib --all-targets --no-run"
    );
    assert!(adapter
        .build_command(ValidationCommandOptions {
            go_packages: Some(vec!["./pkg".to_string()]),
            ..ValidationCommandOptions::default()
        })
        .is_err());
}

#[test]
fn cargo_test_lib_audit_and_identity_canonicalize_false_to_omission() {
    let omitted = session_log_arguments_for_tool_request(
        "cargo_test",
        &serde_json::json!({"project": "agent:test:demo"}),
    );
    let explicit_false = session_log_arguments_for_tool_request(
        "cargo_test",
        &serde_json::json!({"project": "agent:test:demo", "lib": false}),
    );
    let explicit_true = session_log_arguments_for_tool_request(
        "cargo_test",
        &serde_json::json!({"project": "agent:test:demo", "lib": true}),
    );

    assert_eq!(
        omitted["validation_target_id"],
        explicit_false["validation_target_id"]
    );
    assert!(omitted.get("lib").is_none());
    assert!(explicit_false.get("lib").is_none());
    assert_eq!(explicit_true["lib"], true);
    assert_ne!(
        omitted["validation_target_id"],
        explicit_true["validation_target_id"]
    );
}

#[test]
fn rust_adapter_parser_entries_preserve_parser_v3_results() {
    assert_eq!(PARSER_VERSION, 3);
    let stderr = "error[E0308]: mismatched types\n --> src/lib.rs:12:5\n";
    let check = validation_adapter_for_tool("cargo_check").unwrap();
    assert_eq!(
        check.parse("", stderr, false),
        parse_cargo_check_diagnostics("", stderr, false)
    );
    assert_eq!(check.parse("", stderr, false).parser, PARSER_KIND);

    let stdout = "running 1 test\ntest demo ... FAILED\ntest result: FAILED. 0 passed; 1 failed; 0 ignored\n";
    let test = validation_adapter_for_tool("cargo_test").unwrap();
    assert_eq!(
        test.parse(stdout, "", false),
        parse_cargo_test_diagnostics(stdout, "", false)
    );
    assert_eq!(test.parse(stdout, "", false).parser, PARSER_KIND);
}

#[test]
fn go_profile_selects_only_go_test_and_preserves_json_command() {
    let adapter = validation_adapter_for_tool("go_test").expect("go_test adapter");
    assert_eq!(adapter.tool_identity(), "go_test");
    assert_eq!(adapter.validation_kind(), "test");
    assert!(adapter.reports_test_run_metadata());
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions::default())
            .unwrap(),
        "go test -json ./..."
    );
    assert_eq!(
        adapter
            .build_command(ValidationCommandOptions {
                go_packages: Some(vec![
                    "./internal/control".to_string(),
                    "./internal/node".to_string(),
                ]),
                ..ValidationCommandOptions::default()
            })
            .unwrap(),
        "go test -json './internal/control' './internal/node'"
    );
    assert!(adapter
        .build_command(ValidationCommandOptions {
            filter: Some("TestOne".to_string()),
            ..ValidationCommandOptions::default()
        })
        .is_err());
    assert!(validation_adapter_for_tool("go_check").is_none());

    let stdout = r#"{"Action":"fail","Package":"example.test/pkg","Test":"TestFailure"}"#;
    assert_eq!(
        adapter.parse(stdout, "ordinary stderr must be ignored", false),
        parse_go_test_diagnostics(stdout, false)
    );
}

#[test]
fn go_test_schema_and_audit_projection_are_bounded_and_explicit() {
    let specs = registered_tool_specs();
    let spec = specs.iter().find(|spec| spec.name == "go_test").unwrap();
    let packages = &spec.input_schema["properties"]["packages"];
    assert_eq!(packages["minItems"], 1);
    assert_eq!(packages["maxItems"], GO_TEST_PACKAGE_MAX_ITEMS);
    assert_eq!(packages["items"]["maxLength"], GO_TEST_PACKAGE_MAX_BYTES);

    let raw = serde_json::json!({
        "project": "agent:test:demo",
        "cwd": "internal/control",
        "packages": ["./internal/control", "./internal/node"],
        "timeout_secs": 90,
        "unrecognized_private_field": "NEVER_PERSIST_GO_TEST_UNKNOWN"
    });
    let raw_audit = session_log_arguments_for_tool_request("go_test", &raw);
    assert_eq!(raw_audit, serde_json::json!({}));
    assert!(!raw_audit
        .to_string()
        .contains("NEVER_PERSIST_GO_TEST_UNKNOWN"));

    let call = ToolCall::from_tool_name(
        "go_test",
        serde_json::json!({
            "project": "agent:test:demo",
            "cwd": "internal/control",
            "packages": ["./internal/control", "./internal/node"],
            "timeout_secs": 90
        }),
    )
    .unwrap();
    let typed_audit = call.session_log_arguments();
    let target_id = typed_audit["validation_target_id"]
        .as_str()
        .expect("go_test audit projection should include validation_target_id");
    assert!(
        is_structured_validation_target_identity(target_id),
        "unexpected go_test validation target identity: {target_id}"
    );
    let mut audit_without_target = typed_audit;
    audit_without_target
        .as_object_mut()
        .unwrap()
        .remove("validation_target_id");
    assert_eq!(
        audit_without_target,
        serde_json::json!({
            "project": "agent:test:demo",
            "cwd": "internal/control",
            "packages_present": true,
            "package_count": 2,
            "timeout_secs": 90
        })
    );
}

#[test]
fn go_test_adapter_maps_only_structured_test_failures() {
    let adapter = validation_adapter_for_tool("go_test").unwrap();
    let failed = adapter.parse(
        r#"{"Action":"fail","Package":"p.example/pkg","Test":"TestFail"}"#,
        "",
        false,
    );
    assert_eq!(
        adapter.map_failure_kind(ValidationFailureEvidence {
            success: false,
            reported_failure_kind: Some("command_exit_nonzero"),
            exit_code: Some(1),
            diagnostics: Some(&failed),
            stdout_excerpt: "",
            stderr_excerpt: "",
        }),
        "test_failure"
    );

    let unavailable = adapter.parse("not json", "compile failed in ordinary stderr", false);
    assert_eq!(
        adapter.map_failure_kind(ValidationFailureEvidence {
            success: false,
            reported_failure_kind: Some("command_exit_nonzero"),
            exit_code: Some(1),
            diagnostics: Some(&unavailable),
            stdout_excerpt: "",
            stderr_excerpt: "compile failed in ordinary stderr",
        }),
        "process_exit"
    );
    assert_eq!(
        adapter.map_failure_kind(ValidationFailureEvidence {
            success: false,
            reported_failure_kind: Some("timeout"),
            exit_code: None,
            diagnostics: Some(&failed),
            stdout_excerpt: "",
            stderr_excerpt: "",
        }),
        "timeout"
    );
    assert_eq!(
        adapter.map_failure_kind(ValidationFailureEvidence {
            success: true,
            reported_failure_kind: None,
            exit_code: Some(0),
            diagnostics: Some(&adapter.parse(
                r#"{"Action":"pass","Package":"p.example/pkg","Test":"TestPass"}"#,
                "",
                false,
            )),
            stdout_excerpt: "",
            stderr_excerpt: "",
        }),
        "unknown"
    );
}

#[test]
fn validation_profiles_reuse_existing_runtime_tool_schemas() {
    let specs = registered_tool_specs();
    for tool_name in ["cargo_fmt", "cargo_check", "cargo_test", "go_test"] {
        assert!(is_known_tool_name(tool_name));
        assert_eq!(
            specs.iter().filter(|spec| spec.name == tool_name).count(),
            1,
            "{tool_name} must retain exactly one existing runtime schema"
        );
    }
    assert!(validation_adapter_for_tool("validation_profile").is_none());
    assert!(is_known_tool_name("go_test"));
    assert!(!is_known_tool_name("validation_profile"));
    assert!(!is_known_tool_name("validation_adapter"));
}
