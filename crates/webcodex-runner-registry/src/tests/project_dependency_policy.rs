use super::*;
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildProvenance, ProjectBuildRequest,
};
use webcodex_core::project_validation::{
    ProjectDependencyMode, ProjectDependencyPolicy, ProjectDependencyResolutionPolicy,
    ProjectValidationAction, ProjectValidationAdapter, ProjectValidationProvenance,
    ProjectValidationRequest, ProjectValidationScope, ProjectValidationTestOptions,
};
use webcodex_core::runner_protocol::RunnerCapabilityId;

fn locked_policy() -> ProjectDependencyPolicy {
    ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    }
}

fn registration(supported: bool) -> RunnerRegisterRequest {
    let mut capabilities =
        crate::test_support::current_runner_capabilities(RunnerCapabilities::default());
    capabilities.project_build_v1 = true;
    capabilities.project_validation_v1 = true;
    capabilities.project_dependency_policy_v1 = supported;
    current_runner_registration(RunnerRegisterRequest {
        computer_session_availability: None,
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: "policy-runner".into(),
        runner_instance_id: "inst".into(),
        runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities,
        policy: None,
    })
}

fn locked_build_request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Rust,
        scope: None,
        dependency_policy: Some(locked_policy()),
    }
}

fn locked_validation_request() -> ProjectValidationRequest {
    ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: None,
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Rust,
        scope: None,
        dependency_policy: Some(locked_policy()),
        dependency_resolution_policy: None,
        test: None,
    }
}

fn locked_build_plan() -> ProjectBuildPlan {
    let request = locked_build_request();
    let process = canonical_project_build_process("rust", &request).unwrap();
    ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request,
            backend: "rust".into(),
            recipe_root: ".".into(),
            root_digest: "1".repeat(64),
            manifest_digest: "2".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    }
}

fn locked_validation_metadata() -> ShellJobValidationMetadata {
    let request = locked_validation_request();
    let step = ShellJobValidationStep {
        name: "check".into(),
        program: "cargo".into(),
        args: vec!["check".into(), "--locked".into(), "--all-targets".into()],
        env: Vec::new(),
    };
    let validation = ShellJobValidationMetadata {
        project_validation: Some(ProjectValidationProvenance {
            request,
            backend: "rust".into(),
            recipe_root: ".".into(),
            root_digest: "a".repeat(64),
            manifest_digest: "b".repeat(64),
            invocation_digest: "c".repeat(64),
        }),
        source_fence: None,
        tool: "project_validate".into(),
        kind: "check".into(),
        steps: vec![step],
        effective_timeout_secs: 600,
        sync_wait_secs: 10,
        adapter: "cargo_check".into(),
        validation_target_id: Some("target:0123456789abcdef01234567".into()),
        minimum_tests: None,
        require_tests: None,
        no_run: None,
    };
    assert!(validation.is_valid());
    validation
}

fn job_start_request(execution: JobStartExecution, purpose: &str) -> JobStartRequest {
    JobStartRequest {
        cwd: Some("/tmp/demo".into()),
        timeout_secs: 60,
        context: JobStartContext {
            project_id: Some("agent:policy-runner:demo".into()),
            project_cwd: Some(".".into()),
            purpose: Some(purpose.into()),
            shell: Some("direct_argv".into()),
            visibility: ShellJobVisibility::Public,
            ..Default::default()
        },
        ..JobStartRequest::new("policy-runner", execution)
    }
}

#[tokio::test]
async fn dependency_policy_is_fenced_before_project_build_planning() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        registry.register(registration(supported)).await.unwrap();
        let access = auth_context(None, true);
        let result = registry
            .enqueue_project_build_plan(
                "policy-runner".into(),
                locked_build_request(),
                Some(&access),
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().starts_with("capability_unavailable:"),
                "{error}"
            );
            assert!(
                error.to_string().contains("Runner `policy-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_dependency_policy_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
        }
    }
}

#[tokio::test]
async fn node_project_check_requires_additive_capability_before_planning() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_validation_node_script_check_v1 = supported;
        registry.register(runner).await.unwrap();
        let access = auth_context(None, true);
        let mut request = locked_validation_request();
        request.adapter = ProjectValidationAdapter::Node;
        request.dependency_policy = None;
        let result = registry
            .enqueue_project_validation_plan("policy-runner".into(), request, Some(&access))
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("project_validation_node_script_check_v1"),
                "{error}"
            );
            assert!(registry
                .poll(RunnerPollRequest {
                    client_id: "policy-runner".into(),
                    runner_instance_id: "inst".into(),
                })
                .await
                .unwrap()
                .is_none());
        }
    }
}

#[tokio::test]
async fn node_auto_job_admission_rechecks_actual_step_capability() {
    for hint in [
        ProjectValidationAdapter::Auto,
        ProjectValidationAdapter::Node,
    ] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_validation_node_script_check_v1 = false;
        registry.register(runner).await.unwrap();
        let mut metadata = locked_validation_metadata();
        let step = ShellJobValidationStep {
            name: "check".into(),
            program: "node".into(),
            args: vec!["--run".into(), "lint".into()],
            env: vec![],
        };
        let validation = &mut metadata;
        let provenance = validation.project_validation.as_mut().unwrap();
        provenance.backend = "node".into();
        provenance.request.adapter = hint;
        provenance.request.dependency_policy = None;
        validation.adapter = "node:script:check".into();
        validation.steps = vec![step.clone()];

        assert!(validation.is_valid());
        let error = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("project_validation_node_script_check_v1"),
            "{error}"
        );
        assert!(registry.list_jobs(Some(10)).await.is_empty());
    }
}

#[tokio::test]
async fn project_validation_base_capability_error_identifies_runner_without_dispatch() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    let mut registration = registration(true);
    registration.capabilities.project_validation_v1 = false;
    registry.register(registration).await.unwrap();

    let mut request = locked_validation_request();
    request.dependency_policy = None;
    let error = registry
        .enqueue_project_validation_plan("policy-runner".into(), request, Some(&access))
        .await
        .unwrap_err();
    assert!(
        error.to_string().starts_with("capability_unavailable:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("Runner `policy-runner`"),
        "{error}"
    );
    assert!(
        error.to_string().contains("project_validation_v1"),
        "{error}"
    );
    assert!(error.to_string().contains("upgrade that Runner"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "policy-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn project_validation_base_capability_error_identifies_runner_without_job_admission() {
    let registry = RunnerRegistry::default();
    let mut registration = registration(true);
    registration.capabilities.project_validation_v1 = false;
    registry.register(registration).await.unwrap();

    let error = registry
        .start_job(
            job_start_request(
                JobStartExecution::Validation {
                    plan: JobValidationPlan::Metadata(locked_validation_metadata()),
                    ssh_resource: None,
                },
                "validation",
            ),
            "test".into(),
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().starts_with("capability_unavailable:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("Runner `policy-runner`"),
        "{error}"
    );
    assert!(
        error.to_string().contains("project_validation_v1"),
        "{error}"
    );
    assert!(error.to_string().contains("upgrade that Runner"), "{error}");
    assert!(registry.list_jobs(Some(10)).await.is_empty());
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "policy-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn project_validation_optional_capability_errors_identify_runner_without_dispatch() {
    let cases = [
        (
            RunnerCapabilityId::ProjectValidationPythonRuff,
            ProjectValidationRequest {
                action: ProjectValidationAction::Check,
                adapter: ProjectValidationAdapter::Python,
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectValidationPythonRuff,
            ProjectValidationRequest {
                action: ProjectValidationAction::FormatCheck,
                adapter: ProjectValidationAdapter::Python,
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectValidationPythonPytest,
            ProjectValidationRequest {
                action: ProjectValidationAction::Test,
                adapter: ProjectValidationAdapter::Python,
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectAllPackages,
            ProjectValidationRequest {
                scope: Some(ProjectValidationScope {
                    packages: Vec::new(),
                    all_packages: true,
                    exclude_packages: Vec::new(),
                }),
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectWorkspaceExclusions,
            ProjectValidationRequest {
                scope: Some(ProjectValidationScope {
                    packages: Vec::new(),
                    all_packages: true,
                    exclude_packages: vec!["crate".into()],
                }),
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectDependencyPolicy,
            locked_validation_request(),
        ),
        (
            RunnerCapabilityId::ProjectDependencyResolutionOffline,
            ProjectValidationRequest {
                dependency_policy: None,
                dependency_resolution_policy: Some(ProjectDependencyResolutionPolicy::Offline),
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectValidationTestOptions,
            ProjectValidationRequest {
                action: ProjectValidationAction::Test,
                test: Some(ProjectValidationTestOptions::default()),
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
        (
            RunnerCapabilityId::ProjectValidationPackageScope,
            ProjectValidationRequest {
                scope: Some(ProjectValidationScope {
                    packages: vec!["crate".into()],
                    all_packages: false,
                    exclude_packages: Vec::new(),
                }),
                dependency_policy: None,
                dependency_resolution_policy: None,
                ..locked_validation_request()
            },
        ),
    ];

    for (capability, request) in cases {
        let registry = RunnerRegistry::default();
        let access = auth_context(None, true);
        let mut registration = registration(true);
        if capability == RunnerCapabilityId::ProjectWorkspaceExclusions {
            registration.capabilities.project_all_packages_v1 = true;
        }
        registration.capabilities.set(capability, false);
        registry.register(registration).await.unwrap();

        let error = registry
            .enqueue_project_validation_plan("policy-runner".into(), request, Some(&access))
            .await
            .unwrap_err();
        assert!(
            error.to_string().starts_with("capability_unavailable:"),
            "{error}"
        );
        assert!(
            error.to_string().contains("Runner `policy-runner`"),
            "{error}"
        );
        assert!(
            error.to_string().contains(capability.as_wire_name()),
            "{error}"
        );
        assert!(error.to_string().contains("upgrade that Runner"), "{error}");
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "policy-runner".into(),
                runner_instance_id: "inst".into(),
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn offline_project_build_requires_additive_capability_at_planning_and_job_admission() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_dependency_resolution_offline_v1 = supported;
        registry.register(runner).await.unwrap();
        let access = auth_context(None, true);
        let mut request = locked_build_request();
        request.dependency_policy = None;
        request.dependency_resolution_policy = Some(ProjectDependencyResolutionPolicy::Offline);
        let planned = registry
            .enqueue_project_build_plan("policy-runner".into(), request.clone(), Some(&access))
            .await;
        if supported {
            assert!(planned.is_ok(), "{planned:?}");
        } else {
            let error = planned.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("project_dependency_resolution_offline_v1"),
                "{error}"
            );
            assert!(registry
                .poll(RunnerPollRequest {
                    client_id: "policy-runner".into(),
                    runner_instance_id: "inst".into(),
                })
                .await
                .unwrap()
                .is_none());
        }
        let process = canonical_project_build_process("rust", &request).unwrap();
        let mut plan = locked_build_plan();
        plan.provenance.request = request;
        plan.provenance.invocation_digest = project_build_invocation_digest(&process);
        plan.process = process;
        assert!(plan.is_valid());
        let result = registry
            .start_job(
                job_start_request(JobStartExecution::Build(plan), "build"),
                "test".into(),
                None,
                None,
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("project_dependency_resolution_offline_v1"),
                "{error}"
            );
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn project_build_capability_diagnostic_follows_runner_authorization() {
    let registry = RunnerRegistry::default();
    let mut registration = registration(false);
    registration.owner = Some("alice".into());
    registry.register(registration).await.unwrap();

    let access = RunnerAccess {
        global_visibility: true,
        owner_bypass: false,
        username: Some("bob".into()),
        group: None,
    };
    let error = registry
        .enqueue_project_build_plan(
            "policy-runner".into(),
            locked_build_request(),
            Some(&access),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("owned by alice"), "{error}");
    assert!(
        !error.to_string().contains("capability_unavailable"),
        "{error}"
    );
    assert!(!error.to_string().contains("project_build_v1"), "{error}");
    assert!(
        !error.to_string().contains("upgrade that Runner"),
        "{error}"
    );

    let error = registry
        .start_job(
            job_start_request(JobStartExecution::Build(locked_build_plan()), "build"),
            "test".into(),
            Some(&access),
            None,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("owned by alice"), "{error}");
    assert!(
        !error.to_string().contains("capability_unavailable"),
        "{error}"
    );
    assert!(!error.to_string().contains("project_build_v1"), "{error}");
    assert!(
        !error.to_string().contains("upgrade that Runner"),
        "{error}"
    );
}

#[tokio::test]
async fn dependency_policy_is_fenced_before_project_validation_planning() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        registry.register(registration(supported)).await.unwrap();
        let access = auth_context(None, true);
        let result = registry
            .enqueue_project_validation_plan(
                "policy-runner".into(),
                locked_validation_request(),
                Some(&access),
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().starts_with("capability_unavailable:"),
                "{error}"
            );
            assert!(
                error.to_string().contains("Runner `policy-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_dependency_policy_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
        }
    }
}

#[tokio::test]
async fn dependency_policy_is_fenced_again_at_project_build_job_admission() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        registry.register(registration(supported)).await.unwrap();
        let plan = locked_build_plan();
        let result = registry
            .start_job(
                job_start_request(JobStartExecution::Build(plan), "build"),
                "test".into(),
                None,
                None,
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().starts_with("capability_unavailable:"),
                "{error}"
            );
            assert!(
                error.to_string().contains("Runner `policy-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_dependency_policy_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn dependency_policy_is_fenced_again_at_project_validation_job_admission() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        registry.register(registration(supported)).await.unwrap();
        let result = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(locked_validation_metadata()),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error.to_string().starts_with("capability_unavailable:"),
                "{error}"
            );
            assert!(
                error.to_string().contains("Runner `policy-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_dependency_policy_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn offline_dependency_resolution_capability_is_rechecked_at_job_admission() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_dependency_resolution_offline_v1 = supported;
        registry.register(runner).await.unwrap();
        let mut metadata = locked_validation_metadata();
        let validation = &mut metadata;
        let provenance = validation.project_validation.as_mut().unwrap();
        provenance.request.dependency_policy = None;
        provenance.request.dependency_resolution_policy =
            Some(ProjectDependencyResolutionPolicy::Offline);
        let step = ShellJobValidationStep {
            name: "check".into(),
            program: "cargo".into(),
            args: vec!["check".into(), "--offline".into(), "--all-targets".into()],
            env: Vec::new(),
        };
        validation.steps = vec![step.clone()];

        assert!(validation.is_valid());
        let result = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await;
        if supported {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let error = result.unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("project_dependency_resolution_offline_v1"),
                "{error}"
            );
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn project_validation_python_preplan_selects_capability_by_action() {
    for action in [
        ProjectValidationAction::Test,
        ProjectValidationAction::Check,
        ProjectValidationAction::FormatCheck,
    ] {
        for supported in [false, true] {
            let registry = RunnerRegistry::default();
            let mut runner = registration(true);
            runner.capabilities.project_validation_python_pytest_v1 =
                if action == ProjectValidationAction::Test {
                    supported
                } else {
                    !supported
                };
            runner.capabilities.project_validation_python_ruff_v1 =
                if action == ProjectValidationAction::Test {
                    !supported
                } else {
                    supported
                };
            registry.register(runner).await.unwrap();
            let result = registry
                .enqueue_project_validation_plan(
                    "policy-runner".into(),
                    ProjectValidationRequest {
                        action,
                        adapter: ProjectValidationAdapter::Python,
                        dependency_policy: None,
                        dependency_resolution_policy: None,
                        ..locked_validation_request()
                    },
                    Some(&auth_context(None, true)),
                )
                .await;
            assert_eq!(
                result.is_ok(),
                supported,
                "{action:?}, supported={supported}"
            );
            let queued = registry
                .poll(RunnerPollRequest {
                    client_id: "policy-runner".into(),
                    runner_instance_id: "inst".into(),
                })
                .await
                .unwrap();
            assert_eq!(queued.is_some(), supported);
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn rust_clippy_old_runner_is_fenced_at_planning_and_job_admission() {
    for hint in [
        ProjectValidationAdapter::Rust,
        ProjectValidationAdapter::Auto,
    ] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_validation_rust_clippy_v1 = false;
        registry.register(runner).await.unwrap();
        let mut request = locked_validation_request();
        request.action = ProjectValidationAction::Lint;
        request.adapter = hint;
        let access = auth_context(None, true);
        if hint == ProjectValidationAdapter::Rust {
            let error = registry
                .enqueue_project_validation_plan(
                    "policy-runner".into(),
                    request.clone(),
                    Some(&access),
                )
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("project_validation_rust_clippy_v1"),
                "{error}"
            );
        } // Auto planning stays available for existing Node lint; resolved Clippy is gated below.
        let mut metadata = locked_validation_metadata();
        let validation = &mut metadata;
        validation.project_validation.as_mut().unwrap().request = request;
        validation.adapter = "cargo_clippy".into();
        validation.kind = "lint".into();
        validation.steps[0].name = "lint".into();
        validation.steps[0].args = [
            "clippy",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "clippy::correctness",
        ]
        .map(str::to_string)
        .to_vec();

        assert!(validation.is_valid());
        let error = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("project_validation_rust_clippy_v1"),
            "{error}"
        );
        assert!(registry.list_jobs(Some(10)).await.is_empty());
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "policy-runner".into(),
                runner_instance_id: "inst".into()
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn rust_clippy_offline_requires_dedicated_capability_before_planning_and_admission() {
    for hint in [
        ProjectValidationAdapter::Rust,
        ProjectValidationAdapter::Auto,
    ] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_validation_rust_clippy_v1 = true;
        runner.capabilities.project_dependency_resolution_offline_v1 = true;
        runner
            .capabilities
            .project_validation_rust_clippy_offline_v1 = false;
        registry.register(runner).await.unwrap();
        let request = ProjectValidationRequest {
            action: ProjectValidationAction::Lint,
            adapter: hint,
            dependency_resolution_policy: Some(ProjectDependencyResolutionPolicy::Offline),
            ..locked_validation_request()
        };
        let error = registry
            .enqueue_project_validation_plan(
                "policy-runner".into(),
                request.clone(),
                Some(&auth_context(None, true)),
            )
            .await
            .unwrap_err();
        assert!(
            error.contains("project_validation_rust_clippy_offline_v1"),
            "{error}"
        );
        let mut metadata = locked_validation_metadata();
        metadata.project_validation.as_mut().unwrap().request = request;
        metadata.adapter = "cargo_clippy".into();
        metadata.kind = "lint".into();
        metadata.steps[0].name = "lint".into();
        metadata.steps[0].args = [
            "clippy",
            "--locked",
            "--offline",
            "--all-targets",
            "--",
            "-D",
            "clippy::correctness",
        ]
        .map(str::to_string)
        .to_vec();
        assert!(metadata.is_valid());
        let error = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("project_validation_rust_clippy_offline_v1"),
            "{error}"
        );
        assert!(registry.list_jobs(Some(10)).await.is_empty());
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "policy-runner".into(),
                runner_instance_id: "inst".into()
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn rust_clippy_activity_preserves_terminal_result_and_rejects_wrong_phase_or_source() {
    use webcodex_core::runner_protocol::{
        ShellJobActivity, ShellJobActivityPhase, ShellJobActivitySource, ShellJobActivityState,
        ShellJobValidationProgress,
    };
    for (phase, source, exit_code, valid) in [
        (
            ShellJobActivityPhase::ValidationCheck,
            ShellJobActivitySource::ValidationPlan,
            0,
            true,
        ),
        (
            ShellJobActivityPhase::ValidationCheck,
            ShellJobActivitySource::ValidationPlan,
            101,
            true,
        ),
        (
            ShellJobActivityPhase::ValidationTest,
            ShellJobActivitySource::ValidationPlan,
            0,
            false,
        ),
        (
            ShellJobActivityPhase::ProcessRunning,
            ShellJobActivitySource::RunnerExecution,
            0,
            false,
        ),
    ] {
        let registry = RunnerRegistry::default();
        let mut runner = registration(true);
        runner.capabilities.project_validation_rust_clippy_v1 = true;
        registry.register(runner).await.unwrap();
        let mut metadata = locked_validation_metadata();
        let request = &mut metadata.project_validation.as_mut().unwrap().request;
        request.action = ProjectValidationAction::Lint;
        request.dependency_policy = None;
        metadata.adapter = "cargo_clippy".into();
        metadata.kind = "lint".into();
        metadata.steps[0].name = "lint".into();
        metadata.steps[0].args = ["clippy", "--all-targets", "--", "-D", "clippy::correctness"]
            .map(str::to_string)
            .to_vec();
        assert!(metadata.is_valid());
        let job = registry
            .start_job(
                job_start_request(
                    JobStartExecution::Validation {
                        plan: JobValidationPlan::Metadata(metadata),
                        ssh_resource: None,
                    },
                    "validation",
                ),
                "test".into(),
                None,
                None,
            )
            .await
            .unwrap();
        let mut update = RunnerJobUpdateRequest {
            client_id: "policy-runner".into(),
            runner_instance_id: "inst".into(),
            job_id: job.job_id.clone(),
            request_id: None,
            update_seq: Some(1),
            status: "running".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: Some(ShellJobValidationProgress {
                completed: 0,
                current_step: Some("lint".into()),
                failed_step: None,
            }),
            test_count_evidence: None,
            format_mutation: None,
            build_artifacts: None,
            activity: Some(ShellJobActivity {
                state: ShellJobActivityState::Working,
                phase,
                source,
            }),
            finished: false,
        };
        let running = registry.update_job(update.clone()).await.unwrap();
        if !valid {
            assert_eq!(running.status, "failed");
            assert!(running
                .error
                .as_deref()
                .is_some_and(|error| error.contains("job_activity_invalid")));
            assert_eq!(running.activity, None);
            continue;
        }
        assert_eq!(running.status, "running", "{running:?}");
        assert_eq!(running.activity, update.activity);
        assert!(running.error.is_none());
        update.update_seq = Some(2);
        update.status = if exit_code == 0 {
            "completed"
        } else {
            "failed"
        }
        .into();
        update.exit_code = Some(exit_code);
        update.duration_ms = Some(4);
        update.stderr_chunk = Some(
            if exit_code == 0 {
                "warning: fixture warning\n"
            } else {
                "error: fixture correctness failure\n"
            }
            .into(),
        );
        update.validation_progress = Some(ShellJobValidationProgress {
            completed: usize::from(exit_code == 0),
            current_step: None,
            failed_step: (exit_code != 0).then(|| "lint".into()),
        });
        update.activity = None;
        update.finished = true;
        let terminal = registry.update_job(update).await.unwrap();
        assert_eq!(
            terminal.status,
            if exit_code == 0 {
                "completed"
            } else {
                "failed"
            }
        );
        assert_eq!(terminal.exit_code, Some(exit_code));
        assert_eq!(terminal.duration_ms, Some(4));
        assert!(terminal.error.is_none(), "{terminal:?}");
        assert_eq!(terminal.activity, None);
        let (retained, _, stderr, _, _) = registry
            .job_log(&job.job_id, None, None, Some(10))
            .await
            .unwrap();
        assert_eq!(retained.exit_code, Some(exit_code));
        assert!(stderr.as_deref().unwrap().contains(if exit_code == 0 {
            "fixture warning"
        } else {
            "fixture correctness failure"
        }));
    }
}
