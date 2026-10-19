use crate::project_format::*;
use crate::runner_operation::{RunnerInvocationMetadata, RunnerOperation};
use crate::runner_protocol::{RunnerCapabilities, RunnerCapabilityId, RunnerRequest};

fn request() -> ProjectFormatRequest {
    ProjectFormatRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectFormatAdapter::Auto,
        files: vec!["src/main.rs".into()],
    }
}

fn plan() -> ProjectFormatPlan {
    ProjectFormatPlan {
        request: request(),
        recipe_root: ".".into(),
        root_digest: "a".repeat(64),
        manifest_digest: "b".repeat(64),
        profile: ProjectFormatProfile::Rust {
            edition: "2021".into(),
        },
        files: vec![ProjectFormatFileWitness {
            path: "src/main.rs".into(),
            bytes: 12,
            sha256: "c".repeat(64),
            identity_digest: "d".repeat(64),
        }],
    }
}

#[test]
fn project_format_request_is_closed_bounded_and_single_language() {
    assert!(request().validate().is_ok());
    for files in [
        vec![],
        vec!["x.rs"; 9],
        vec!["x.rs", "x.rs"],
        vec!["x.rs", "x.py"],
        vec!["../x.rs"],
        vec!["/x.rs"],
        vec!["C:/x.rs"],
        vec!["a\\x.rs"],
        vec!["./x.rs"],
        vec!["a//x.rs"],
        vec!["x\n.rs"],
        vec!["x\0.rs"],
        vec!["x.js"],
        vec!["x.RS"],
    ] {
        let mut invalid = request();
        invalid.files = files.into_iter().map(str::to_string).collect();
        assert!(invalid.validate().is_err());
    }
    let mut invalid = request();
    invalid.files = vec![format!("{}.rs", "x".repeat(1024))];
    assert!(invalid.validate().is_err());
    invalid = request();
    invalid.adapter = ProjectFormatAdapter::Python;
    assert!(invalid.validate().is_err());
    for field in ["argv", "exec", "command", "script", "scope", "all_files"] {
        let mut json = serde_json::to_value(request()).unwrap();
        json[field] = serde_json::json!("injected");
        assert!(serde_json::from_value::<ProjectFormatRequest>(json).is_err());
    }
}

#[test]
fn project_format_plan_fences_order_bytes_profile_and_recipe_root() {
    let valid = plan();
    let digest = valid.digest().unwrap();
    for change in 0..4 {
        let mut updated = valid.clone();
        match change {
            0 => updated.files[0].sha256 = "d".repeat(64),
            1 => updated.files[0].bytes += 1,
            2 => updated.manifest_digest = "d".repeat(64),
            _ => {
                updated.profile = ProjectFormatProfile::Rust {
                    edition: "2024".into(),
                }
            }
        }
        assert_ne!(updated.digest().unwrap(), digest);
    }
    for change in 0..6 {
        let mut invalid = valid.clone();
        match change {
            0 => invalid.files[0].bytes = usize::MAX,
            1 => invalid.recipe_root = "nested".into(),
            2 => invalid.files[0].sha256 = "invalid".into(),
            3 => invalid.files[0].path = "other.rs".into(),
            4 => invalid.files.clear(),
            _ => {
                invalid.profile = ProjectFormatProfile::Python {
                    target_version: "py311".into(),
                }
            }
        }
        assert!(invalid.digest().is_err());
    }
    let encoded = serde_json::to_string(&valid).unwrap();
    assert!(encoded.len() < PROJECT_FORMAT_PLAN_MAX_BYTES);
    assert!(!encoded.contains("content"));
}

#[test]
fn project_format_commands_have_only_fixed_stdin_profiles() {
    let rust = plan().profile.process("src/main.rs").unwrap();
    assert_eq!(rust.executable, "rustfmt");
    assert_eq!(
        rust.args,
        [
            "--emit=stdout",
            "--edition=2021",
            "--config",
            "skip_children=true"
        ]
    );
    let python = ProjectFormatProfile::Python {
        target_version: "py311".into(),
    };
    let python = python.process("package/code.pyi").unwrap();
    assert_eq!(
        python.args,
        [
            "-I",
            "-B",
            "-m",
            "ruff",
            "format",
            "--no-cache",
            "--config",
            "pyproject.toml",
            "--stdin-filename",
            "package/code.pyi",
            "-"
        ]
    );
}

#[test]
fn project_format_wire_roundtrip_rejects_execution_and_oversized_payloads() {
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "r".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::PlanProjectFormat(request()),
    )
    .unwrap();
    assert_eq!(wire.kind, "plan_project_format");
    assert!(
        matches!(wire.decode_operation().unwrap(), RunnerOperation::PlanProjectFormat(decoded) if decoded == request())
    );
    for field in ["command", "stdin", "cwd", "path", "job_id", "content"] {
        let mut invalid = wire.clone();
        match field {
            "command" => invalid.command = "echo x".into(),
            "stdin" => invalid.stdin = Some("input".into()),
            "cwd" => invalid.cwd = Some("/tmp".into()),
            "path" => invalid.path = Some("x.rs".into()),
            "job_id" => invalid.job_id = Some("job".into()),
            _ => invalid.content = Some(" ".repeat(PROJECT_FORMAT_PLAN_MAX_BYTES + 1)),
        }
        assert!(invalid.decode_operation().is_err(), "{field}");
    }
}

#[test]
fn project_format_capability_and_mutation_truth_default_closed() {
    let old: RunnerCapabilities = serde_json::from_str("{}").unwrap();
    assert!(!old.supports(RunnerCapabilityId::ProjectFormat));
    assert!(!old.supports(RunnerCapabilityId::ProjectFormatGo));
    let original: RunnerCapabilities =
        serde_json::from_str(r#"{"project_format_v1":true}"#).unwrap();
    assert!(original.supports(RunnerCapabilityId::ProjectFormat));
    assert!(!original.supports(RunnerCapabilityId::ProjectFormatGo));
    assert_eq!(ProjectFormatMutationReport::default().state_changed(), None);
    for (report, changed) in [
        (ProjectFormatMutationReport::Unknown, None),
        (ProjectFormatMutationReport::Unchanged, Some(false)),
        (ProjectFormatMutationReport::Changed, Some(true)),
    ] {
        let retained: ProjectFormatMutationReport =
            serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
        assert_eq!(retained.state_changed(), changed);
    }
}

#[test]
fn project_format_go_selection_and_wire_preserve_closed_profiles() {
    let mut request = request();
    request.files = vec!["main.go".into(), "internal/code.go".into()];
    for adapter in [ProjectFormatAdapter::Auto, ProjectFormatAdapter::Go] {
        request.adapter = adapter;
        assert!(request.validate().is_ok());
        assert!(request.requires_go());
        let decoded: ProjectFormatRequest =
            serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
        assert_eq!(decoded, request);
    }
    let profile = ProjectFormatProfile::Go {};
    assert_eq!(
        serde_json::to_value(&profile).unwrap(),
        serde_json::json!({"backend":"go"})
    );
    assert_eq!(profile.adapter(), ProjectFormatAdapter::Go);
    let command = profile.process("internal/code.go").unwrap();
    assert_eq!(command.executable, "gofmt");
    assert!(command.args.is_empty());
    assert!(profile.process("main.rs").is_err());
    for files in [
        vec!["main.go", "main.py"],
        vec!["main.go", "main.go"],
        vec!["../main.go"],
        vec!["dir\\main.go"],
    ] {
        request.files = files.into_iter().map(str::to_string).collect();
        assert!(request.validate().is_err());
    }
    request.files = vec!["main.go".into()];
    request.adapter = ProjectFormatAdapter::Rust;
    assert!(request.validate().is_err());
    request.adapter = ProjectFormatAdapter::Go;
    request.files = vec!["main.rs".into()];
    assert!(request.validate().is_err());
    assert!(serde_json::from_value::<ProjectFormatProfile>(
        serde_json::json!({"backend":"go","args":["-w"]})
    )
    .is_err());
}

#[test]
fn project_format_pf01_job_wire_is_closed_and_preserves_identity() {
    use crate::runner_operation::{RunnerJobFormatOperation, RunnerJobOperation};
    let context = crate::runner_protocol::ShellJobContext {
        runtime_project_id: Some("agent:runner:demo".into()),
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: Some(".".into()),
        cwd: Some("/project".into()),
        purpose: Some("format".into()),
        shell: Some("direct_argv".into()),
        command_preview: "project_format selected files".into(),
        validation_steps: vec![],
        validation: None,
        structured_execution: Some(structured_metadata()),
    };
    let format_op = RunnerJobFormatOperation {
        job_id: "job-format".into(),
        cwd: context.cwd.clone(),
        plan: plan(),
        timeout_secs: 60,
        context,
    };
    let operation = RunnerOperation::Job(RunnerJobOperation::StartFormat(format_op.clone()));
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "r".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 1,
        },
        operation,
    )
    .unwrap();
    assert_eq!(wire.kind, "start_format_job");
    assert!(
        matches!(wire.decode_operation().unwrap(), RunnerOperation::Job(RunnerJobOperation::StartFormat(decoded)) if decoded == format_op)
    );
    for field in ["command", "stdin", "content", "timeout_secs"] {
        let mut forged = wire.clone();
        match field {
            "command" => forged.command = "arbitrary".into(),
            "stdin" => forged.stdin = Some("source must remain private".into()),
            "content" => forged.content = Some("{}".into()),
            _ => forged.timeout_secs = 121,
        }
        assert!(forged.decode_operation().is_err(), "{field}");
    }
}

#[test]
fn project_format_pf12_mutation_report_cannot_borrow_exit_success() {
    use crate::runner_protocol::ShellCommandExecutionState as State;
    assert_eq!(ProjectFormatMutationReport::default().state_changed(), None);
    for report in [
        ProjectFormatMutationReport::Changed,
        ProjectFormatMutationReport::Unchanged,
    ] {
        assert!(validate_mutation_report(
            "project_format",
            "completed",
            Some(0),
            Some(State::Completed),
            Some(report),
            None
        )
        .is_ok());
        for (kind, status, exit, state, error) in [
            (
                "project_build",
                "completed",
                Some(0),
                Some(State::Completed),
                None,
            ),
            (
                "project_format",
                "failed",
                Some(0),
                Some(State::Completed),
                None,
            ),
            (
                "project_format",
                "completed",
                Some(1),
                Some(State::Completed),
                None,
            ),
            (
                "project_format",
                "completed",
                Some(0),
                Some(State::OutcomeUnknown),
                None,
            ),
            (
                "project_format",
                "completed",
                Some(0),
                Some(State::Completed),
                Some("format_write_conflict"),
            ),
        ] {
            assert!(
                validate_mutation_report(kind, status, exit, state, Some(report), error).is_err()
            );
        }
    }
    assert!(validate_mutation_report(
        "project_format",
        "completed",
        Some(0),
        Some(State::Completed),
        None,
        None
    )
    .is_ok());
    assert!(validate_mutation_report(
        "project_format",
        "failed",
        Some(1),
        Some(State::OutcomeUnknown),
        None,
        Some("format_process_failed")
    )
    .is_ok());
}
