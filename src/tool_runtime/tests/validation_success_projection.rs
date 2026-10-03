use super::*;

fn fixture(
    tool: &str,
    adapter: &str,
    policy: ValidationSuccessPolicy,
    stdout: &str,
    stderr: &str,
) -> ToolResult {
    let test = matches!(adapter, "cargo_test" | "go_test");
    let minimum = if tool == "project_validate" && test {
        Some(1)
    } else {
        policy
            .min_tests
            .or_else(|| (policy.require_tests == Some(true)).then_some(1))
    };
    let projection = crate::tool_runtime::jobs::validation_job_projection_with_policy(
        Some(adapter),
        None,
        "completed",
        Some(0),
        stdout,
        stderr,
        false,
        None,
        minimum,
        policy.require_tests,
        policy.no_run,
    )
    .unwrap();
    let mut result = ToolResult::ok(json!({
        "project":"agent:fixture:validation", "command_summary":"validation fixture", "cwd":".",
        "execution_state":"completed", "exit_code":0, "duration_ms":5,
        "command_started":true,"command_completed":true,"passed":true,
        "promoted_to_job":false,"terminal":true,"effective_timeout_secs":60,"sync_wait_secs":10,
        "stdout_tail":stdout,"stderr_tail":stderr,"stdout_lines":stdout.lines().count(),"stderr_lines":stderr.lines().count(),
        "stdout_truncated":false,"stderr_truncated":false,
        "source_state":{"freshness":"unproven","observed_mutation_fence":"uncrossed"}
    }));
    crate::tool_runtime::validation::apply_validation_projection_fields(
        &mut result.output,
        &projection,
    );
    if tool == "project_validate" {
        result.output["backend"] = json!(if adapter.starts_with("go_") {
            "go"
        } else {
            "rust"
        });
        result.output["adapter"] = json!(adapter);
        result.output["action"] = json!(if test { "test" } else { "check" });
        result.output["validation_target_id"] = json!("target:0123456789abcdef01234567");
    }
    result
}

fn old_projection(tool: &str, result: &mut ToolResult) {
    sparsify_terminal_structured_execution_success(tool, result);
    sparsify_structured_validation_runtime_metadata(tool, result);
    crate::tool_runtime::jobs::sparsify_job_handoff_model_result(result);
}

fn project(tool: &'static str, policy: ValidationSuccessPolicy, result: &mut ToolResult) {
    registry::ResultProjection::project(
        Box::new(execution::ExecutionProjection {
            tool_name: tool,
            validation_policy: policy,
        }),
        result,
    );
}

fn validate(tool: &str, result: &ToolResult) -> Result<(), String> {
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &crate::tool_runtime::registry::output_schema_for_tool(tool),
    )
}

const CARGO: &str = "running 28 tests\ntest result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
const ZERO: &str =
    "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
const GO: &str = "{\"Action\":\"pass\",\"Package\":\"example\",\"Test\":\"TestOne\"}\n{\"Action\":\"pass\",\"Package\":\"example\",\"Test\":\"TestTwo\"}\n{\"Action\":\"pass\",\"Package\":\"example\"}\n";

#[test]
fn validation_success_receipts_sizes_and_schema() {
    let default = ValidationSuccessPolicy::default();
    for (label, tool, adapter, policy, stdout, stderr, expected) in [
        (
            "cargo_check clean",
            "cargo_check",
            "cargo_check",
            default,
            "",
            "",
            json!({}),
        ),
        (
            "cargo_check warnings",
            "cargo_check",
            "cargo_check",
            default,
            "",
            "warning: unused variable\n --> src/lib.rs:1:1\n",
            json!({"warnings_count":1}),
        ),
        (
            "cargo_test ordinary",
            "cargo_test",
            "cargo_test",
            default,
            CARGO,
            "",
            json!({"tests_run_count":28}),
        ),
        (
            "cargo_test min",
            "cargo_test",
            "cargo_test",
            ValidationSuccessPolicy {
                min_tests: Some(20),
                ..default
            },
            CARGO,
            "",
            json!({"tests_run_count":28,"test_count_assertion":{"minimum_tests":20}}),
        ),
        (
            "cargo_test zero",
            "cargo_test",
            "cargo_test",
            ValidationSuccessPolicy {
                require_tests: Some(false),
                ..default
            },
            ZERO,
            "",
            json!({"tests_run_count":0,"require_tests":false}),
        ),
        (
            "cargo_test compile",
            "cargo_test",
            "cargo_test",
            ValidationSuccessPolicy {
                no_run: Some(true),
                ..default
            },
            "",
            "",
            json!({"no_run":true}),
        ),
        (
            "go_test ordinary",
            "go_test",
            "go_test",
            default,
            GO,
            "",
            json!({"tests_run_count":2}),
        ),
        (
            "project Rust check",
            "project_validate",
            "cargo_check",
            default,
            "",
            "",
            json!({}),
        ),
        (
            "project Rust test",
            "project_validate",
            "cargo_test",
            default,
            CARGO,
            "",
            json!({"tests_run_count":28}),
        ),
        (
            "project Go check",
            "project_validate",
            "go_vet",
            default,
            "",
            "",
            json!({}),
        ),
        (
            "project Go test",
            "project_validate",
            "go_test",
            default,
            GO,
            "",
            json!({"tests_run_count":2}),
        ),
    ] {
        let mut result = fixture(tool, adapter, policy, stdout, stderr);
        let canonical = serde_json::to_value(&result).unwrap();
        let before = serde_json::to_vec(&result).unwrap().len();
        let mut old = fixture(tool, adapter, policy, stdout, stderr);
        // #765's synchronous copy list omitted these canonical parser fields.
        old.output.as_object_mut().unwrap().remove("require_tests");
        old.output.as_object_mut().unwrap().remove("no_run");
        old_projection(tool, &mut old);
        let old_bytes = serde_json::to_vec(&old).unwrap().len();
        project(tool, policy, &mut result);
        validate(tool, &result).unwrap_or_else(|e| panic!("{label}: {e}: {}", result.output));
        let bytes = serde_json::to_vec(&result).unwrap().len();
        println!("{label}: canonical={before} previous={old_bytes} final={bytes}");
        println!("{label} receipt: {}", result.output);
        assert!(
            bytes + 150 < old_bytes,
            "soft ergonomics regression: {label}"
        );
        assert!(bytes < 800, "soft fixture budget: {label}");
        for (key, value) in expected.as_object().unwrap() {
            assert_eq!(&result.output[key], value, "{label}: {key}");
        }
        for key in [
            "tests_detected",
            "tests_passed",
            "tests_failed",
            "zero_tests_run",
            "errors_count",
        ] {
            assert!(result.output.get(key).is_none(), "{label}: {key}");
        }
        assert_eq!(
            result.output["source_state"],
            canonical["output"]["source_state"]
        );
        if tool == "project_validate" {
            for key in ["adapter", "validation_target_id"] {
                assert_eq!(result.output[key], canonical["output"][key]);
            }
            assert!(result.output.get("action").is_none());
            assert!(result.output.get("backend").is_none());
        }
        if !stderr.is_empty() {
            assert_eq!(
                result.output["diagnostics"]["diagnostics"],
                canonical["output"]["diagnostics"]["diagnostics"]
            );
        }
        // Neither API ToolResult nor MCP structuredContent can accept duplicate fragments.
        for (key, value) in [
            ("tests_passed", json!(28)),
            ("tests_failed", json!(0)),
            ("tests_detected", json!(true)),
            ("zero_tests_run", json!(false)),
            ("errors_count", json!(0)),
        ] {
            let saved = result.output.clone();
            result.output[key] = value;
            assert!(
                validate(tool, &result).is_err(),
                "{label} accepts duplicate {key}"
            );
            result.output = saved;
        }
    }
}

#[test]
fn validation_success_exceptional_evidence_is_not_compacted() {
    let default = ValidationSuccessPolicy::default();
    for (path, value) in [
        ("/source_state/freshness", json!("stale")),
        ("/source_state/observed_mutation_fence", json!("crossed")),
        ("/source_state/observed_mutation_fence", json!("unknown")),
        ("/stdout_truncated", json!(true)),
        ("/diagnostics/diagnostics_truncated", json!(true)),
        ("/diagnostics/truncated", json!(true)),
        ("/diagnostics/invalid_diagnostics_omitted", json!(1)),
        ("/diagnostics/failed_test_details_truncated", json!(true)),
        ("/diagnostics/test_summary/ignored", json!(1)),
        ("/diagnostics/test_summary/passed", json!(27)),
        ("/tests_run_count", Value::Null),
        ("/tests_failed", json!(1)),
        ("/passed", json!(false)),
        ("/execution_state", json!("outcome_unknown")),
        ("/exit_code", json!(1)),
    ] {
        let mut result = fixture("cargo_test", "cargo_test", default, CARGO, "");
        *result.output.pointer_mut(path).unwrap() = value;
        let before = result.output.clone();
        sparsify_structured_validation_success_evidence("cargo_test", default, &mut result);
        assert_eq!(before, result.output, "{path}");
    }
    let mut zero = fixture("cargo_test", "cargo_test", default, ZERO, "");
    let before = zero.output.clone();
    sparsify_structured_validation_success_evidence("cargo_test", default, &mut zero);
    assert_eq!(
        before, zero.output,
        "default zero remains rich/inconclusive; canonical success is not rewritten"
    );
    for (tool, adapter, stdout, stderr) in [
        ("cargo_check", "cargo_check", "", "error: compiler failure"),
        (
            "cargo_test",
            "cargo_test",
            "test result: FAILED. 1 passed; 1 failed; 0 ignored\n",
            "",
        ),
    ] {
        let mut result = fixture(tool, adapter, default, stdout, stderr);
        result.success = false;
        result.error = Some("validation failed".into());
        result.output["passed"] = json!(false);
        result.output["exit_code"] = json!(1);
        result.output["failure_kind"] = json!("validation_failed");
        let before = result.output.clone();
        project(tool, default, &mut result);
        assert_eq!(before, result.output);
        validate(tool, &result).unwrap();
    }
    let policy = ValidationSuccessPolicy {
        require_tests: Some(true),
        ..default
    };
    let mut zero = fixture("cargo_test", "cargo_test", policy, ZERO, "");
    assert_eq!(zero.output["passed"], false);
    zero.success = false;
    zero.error = Some("minimum not met".into());
    zero.output["failure_kind"] = json!("validation_failed");
    let before = zero.output.clone();
    project("cargo_test", policy, &mut zero);
    assert_eq!(before, zero.output);
    validate("cargo_test", &zero).unwrap();
}

#[test]
fn validation_success_keeps_canonical_audit_and_measures_final_receipt() {
    let policy = ValidationSuccessPolicy {
        min_tests: Some(20),
        ..Default::default()
    };
    let mut result = fixture("cargo_test", "cargo_test", policy, CARGO, "");
    let audit = crate::tool_runtime::tool_audit::canonical_execution_audit_result_for_tool(
        "cargo_test",
        &result.output,
    );
    assert_eq!(audit["tests_passed"], 28);
    assert_eq!(audit["test_count_assertion"]["status"], "passed");
    assert_eq!(audit["test_count_assertion"]["actual_tests_run"], 28);
    assert!(audit.get("stdout_tail").is_none());
    let mut timer=crate::tool_runtime::model_ergonomics_telemetry::ModelErgonomicsTimer::start_with_arguments("cargo_test",&json!({})).unwrap();
    timer.capture_canonical_result(&result);
    let completion = timer.finish();
    project("cargo_test", policy, &mut result);
    let api = completion.record_for_tool_result(&result).unwrap();
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(
        api,
        completion.record_for_structured_content(&wire).unwrap()
    );
    assert_eq!(
        api.serialized_result_bytes,
        Some(serde_json::to_vec(&wire).unwrap().len() as u64)
    );
    validate("cargo_test", &result).unwrap();
}

#[test]
fn validation_success_schema_rejects_duplicate_parser_and_assertion_fragments() {
    let policy = ValidationSuccessPolicy {
        min_tests: Some(20),
        ..Default::default()
    };
    let mut result = fixture("cargo_test", "cargo_test", policy, CARGO, "");
    project("cargo_test", policy, &mut result);
    let compact = result.output.clone();
    for (key, value) in [
        ("actual_tests_run", json!(28)),
        ("status", json!("passed")),
        ("reason_code", json!("minimum_satisfied")),
        ("evidence_reason_code", json!("complete_summary")),
    ] {
        result.output = compact.clone();
        result.output["test_count_assertion"][key] = value;
        assert!(validate("cargo_test", &result).is_err(), "{key}");
    }
    for value in [
        json!({"diagnostics":[]}),
        json!({"available":true}),
        json!({"parser":"structured_validation_parser"}),
        json!({"truncated":false}),
    ] {
        result.output = compact.clone();
        result.output["diagnostics"] = value;
        assert!(validate("cargo_test", &result).is_err());
    }
    let default = ValidationSuccessPolicy::default();
    let mut project_result = fixture("project_validate", "cargo_check", default, "", "");
    project("project_validate", default, &mut project_result);
    let compact = project_result.output.clone();
    for (key, value) in [
        ("backend", json!("rust")),
        ("action", json!("check")),
        ("detected_backend", json!("rust")),
    ] {
        project_result.output = compact.clone();
        project_result.output[key] = value;
        assert!(
            validate("project_validate", &project_result).is_err(),
            "{key}"
        );
    }

    // A rich successful receipt (for example unknown source coverage) still
    // carries source_state as independent correctness evidence. The strict
    // published schema must not accept a presentation that drops it.
    let mut rich = fixture("cargo_test", "cargo_test", default, CARGO, "");
    rich.output["source_state"]["observed_mutation_fence"] = json!("unknown");
    project("cargo_test", default, &mut rich);
    assert!(rich.output.get("tests_detected").is_some());
    validate("cargo_test", &rich).unwrap();
    rich.output.as_object_mut().unwrap().remove("source_state");
    assert!(validate("cargo_test", &rich).is_err());

    let mut project_rich = fixture("project_validate", "cargo_check", default, "", "");
    project_rich.output["source_state"]["observed_mutation_fence"] = json!("unknown");
    project("project_validate", default, &mut project_rich);
    validate("project_validate", &project_rich).unwrap();
    let canonical_rich = project_rich.output.clone();
    for key in [
        "source_state",
        "backend",
        "action",
        "adapter",
        "validation_target_id",
    ] {
        project_rich.output = canonical_rich.clone();
        project_rich.output.as_object_mut().unwrap().remove(key);
        assert!(
            validate("project_validate", &project_rich).is_err(),
            "rich project_validate success accepted missing {key}"
        );
    }
}

#[test]
fn validation_success_registry_capture_preserves_cargo_postconditions() {
    for policy in [
        ValidationSuccessPolicy::default(),
        ValidationSuccessPolicy {
            require_tests: Some(true),
            ..Default::default()
        },
        ValidationSuccessPolicy {
            require_tests: Some(false),
            ..Default::default()
        },
        ValidationSuccessPolicy {
            no_run: Some(true),
            ..Default::default()
        },
        ValidationSuccessPolicy {
            min_tests: Some(3),
            ..Default::default()
        },
    ] {
        let mut args = json!({"project":"agent:fixture:validation"});
        if let Some(value) = policy.require_tests {
            args["require_tests"] = json!(value);
        }
        if let Some(value) = policy.no_run {
            args["no_run"] = json!(value);
        }
        if let Some(value) = policy.min_tests {
            args["min_tests"] = json!(value);
        }
        let call = ToolCall::from_tool_name("cargo_test", args.clone()).unwrap();
        for stdout in [CARGO, ZERO, ""] {
            let mut actual = fixture("cargo_test", "cargo_test", policy, stdout, "");
            let mut expected = ToolResult {
                success: actual.success,
                output: actual.output.clone(),
                error: actual.error.clone(),
            };
            project("cargo_test", policy, &mut expected);
            ModelFacingProjectionPlan::capture(&call).project(&mut actual);
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "captured policy changed for {args}: {stdout:?}",
            );
        }
    }
}
