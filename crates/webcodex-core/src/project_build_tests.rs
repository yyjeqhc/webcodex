use crate::project_build::*;
use crate::runner_operation::{
    RunnerInvocationMetadata, RunnerJobBuildOperation, RunnerJobOperation, RunnerOperation,
};
use crate::runner_protocol::{RunnerRequest, ShellJobContext, ShellJobStructuredExecutionMetadata};

fn request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: Some("src".into()),
        adapter: ProjectBuildAdapter::Auto,
        scope: None,
    }
}

fn digest(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

#[test]
fn project_build_protocol_is_closed_declarative_and_roundtrips() {
    let input = request();
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "request".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::PlanProjectBuild(input.clone()),
    )
    .unwrap();
    assert_eq!(wire.kind, "plan_project_build");
    assert!(wire.command.is_empty());
    assert!(wire.process.is_none() && wire.script.is_none() && wire.cwd.is_none());
    let payload: ProjectBuildRequest =
        serde_json::from_str(wire.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload, input);

    for field in ["program", "args", "shell", "script", "executable", "argv"] {
        let mut json = serde_json::to_value(&input).unwrap();
        json[field] = serde_json::json!("arbitrary");
        assert!(serde_json::from_value::<ProjectBuildRequest>(json).is_err());
    }
    for cwd in [
        "/tmp",
        "../outside",
        "C:/secret",
        "\\\\host\\share",
        "a/../b",
    ] {
        let mut invalid = input.clone();
        invalid.cwd = Some(cwd.into());
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn project_build_scope_and_canonical_processes_are_bounded() {
    let mut rust = request();
    rust.scope = Some(ProjectBuildScope {
        packages: vec!["package-b".into(), "package-a".into(), "package-a".into()],
    });
    assert_eq!(
        canonical_project_build_process("rust", &rust).unwrap().args,
        ["build", "-p", "package-a", "-p", "package-b"]
    );

    let mut go = request();
    go.scope = Some(ProjectBuildScope {
        packages: vec!["./cmd/...".into(), "./internal".into()],
    });
    assert_eq!(
        canonical_project_build_process("go", &go).unwrap().args,
        ["build", "./cmd/...", "./internal"]
    );

    for packages in [
        Vec::<String>::new(),
        (0..9).map(|index| format!("package-{index}")).collect(),
        vec!["x".repeat(257)],
        vec!["bad\npackage".into()],
    ] {
        let mut invalid = request();
        invalid.scope = Some(ProjectBuildScope { packages });
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn start_build_wire_roundtrip_preserves_typed_process_and_provenance() {
    let request = ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Rust,
        scope: None,
    };
    let process = canonical_project_build_process("rust", &request).unwrap();
    let provenance = ProjectBuildProvenance {
        request,
        backend: "rust".into(),
        recipe_root: ".".into(),
        root_digest: digest('1'),
        manifest_digest: digest('2'),
        invocation_digest: project_build_invocation_digest(&process),
    };
    let structured = ShellJobStructuredExecutionMetadata {
        execution_source: "project_build".into(),
        language: None,
        script_bytes: None,
        arg_count: process.args.len(),
        stdin_present: false,
        validation_identity: None,
        assertion_name: None,
        validation_tool: None,
    };
    assert!(structured.is_valid());
    let context = ShellJobContext {
        runtime_project_id: Some("agent:runner:demo".into()),
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: Some(".".into()),
        cwd: Some("/tmp/demo".into()),
        purpose: Some("build".into()),
        shell: Some("direct_argv".into()),
        command_preview: "cargo build".into(),
        validation_steps: Vec::new(),
        validation: None,
        structured_execution: Some(structured),
    };
    let operation = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
        job_id: "job".into(),
        cwd: context.cwd.clone(),
        process: process.clone(),
        provenance: provenance.clone(),
        timeout_secs: 60,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "request".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::Job(operation.clone()),
    )
    .unwrap();
    assert_eq!(wire.kind, "start_build_job");
    assert_eq!(wire.process, Some(process));
    assert!(wire.command.is_empty() && wire.stdin.is_none());
    let decoded = wire.decode_operation().unwrap();
    let RunnerOperation::Job(RunnerJobOperation::StartBuild(decoded)) = decoded else {
        panic!("expected typed StartBuild");
    };
    assert_eq!(decoded.provenance, provenance);
    assert_eq!(
        RunnerJobOperation::StartBuild(decoded).expected_structured_execution(),
        operation.expected_structured_execution()
    );

    let mut process_drift = wire.clone();
    process_drift
        .process
        .as_mut()
        .unwrap()
        .args
        .push("--release".into());
    assert!(process_drift
        .decode_operation()
        .unwrap_err()
        .contains("does not match build provenance"));

    let mut recovery_drift = wire;
    recovery_drift
        .job_context
        .as_mut()
        .unwrap()
        .structured_execution
        .as_mut()
        .unwrap()
        .execution_source = "run_process".into();
    assert!(recovery_drift
        .decode_operation()
        .unwrap_err()
        .contains("recovery metadata does not match typed operation"));
}
