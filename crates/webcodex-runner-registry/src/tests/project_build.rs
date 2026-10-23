use super::*;
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildProvenance, ProjectBuildRequest,
};
use webcodex_core::runner_operation::{RunnerJobOperation, RunnerOperation};

fn build_request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Rust,
        scope: None,
        dependency_policy: None,
        dependency_resolution_policy: None,
    }
}

fn build_plan() -> ProjectBuildPlan {
    let request = build_request();
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

fn go_build_request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Go,
        scope: None,
        dependency_policy: None,
        dependency_resolution_policy: None,
    }
}

fn go_build_plan() -> ProjectBuildPlan {
    let request = go_build_request();
    let process = canonical_project_build_process("go", &request).unwrap();
    ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request,
            backend: "go".into(),
            recipe_root: ".".into(),
            root_digest: "3".repeat(64),
            manifest_digest: "4".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    }
}

fn build_job_request(plan: ProjectBuildPlan) -> JobStartRequest {
    JobStartRequest {
        cwd: Some("/tmp/demo".into()),
        timeout_secs: 60,
        context: JobStartContext {
            project_id: Some("agent:build-runner:demo".into()),
            project_cwd: Some(".".into()),
            purpose: Some("build".into()),
            shell: Some("direct_argv".into()),
            ..Default::default()
        },
        ..JobStartRequest::new("build-runner", JobStartExecution::Build(plan))
    }
}

fn registration(project_build: bool) -> RunnerRegisterRequest {
    let mut capabilities =
        crate::test_support::current_runner_capabilities(RunnerCapabilities::default());
    capabilities.project_build_v1 = project_build;
    current_runner_registration(RunnerRegisterRequest {
        capabilities,
        ..runner_registration("build-runner", "inst", Vec::new())
    })
}

#[tokio::test]
async fn project_build_rust_options_require_capability_before_planning_and_admission() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    registry.register(registration(true)).await.unwrap();
    let mut selected = build_plan();
    selected.provenance.request.profile = Some("dogfood".into());
    selected.process =
        canonical_project_build_process("rust", &selected.provenance.request).unwrap();
    selected.provenance.invocation_digest = project_build_invocation_digest(&selected.process);
    let error = registry
        .enqueue_project_build_plan(
            "build-runner".into(),
            selected.provenance.request.clone(),
            Some(&access),
        )
        .await
        .unwrap_err();
    assert!(error.contains("project_build_rust_options_v1"), "{error}");
    let error = registry
        .start_job(
            build_job_request(selected.clone()),
            "tool_runtime".into(),
            Some(&access),
            None,
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("project_build_rust_options_v1"),
        "{error}"
    );
    assert!(registry.list_jobs(Some(10)).await.is_empty());
    let mut newer = registration(true);
    newer.capabilities.project_build_rust_options_v1 = true;
    registry.register(newer).await.unwrap();
    assert!(registry
        .enqueue_project_build_plan(
            "build-runner".into(),
            selected.provenance.request.clone(),
            Some(&access)
        )
        .await
        .is_ok());
}
#[tokio::test]
async fn missing_build_capabilities_preserve_stage_order_without_admission() {
    for (base, node, planning_feature, job_feature) in [
        (false, false, "project_build_v1", "project_build_node_v1"),
        (false, true, "project_build_v1", "project_build_v1"),
        (
            true,
            false,
            "project_build_node_v1",
            "project_build_node_v1",
        ),
    ] {
        let registry = RunnerRegistry::default();
        let access = auth_context(None, true);
        let mut registration = registration(base);
        registration.capabilities.project_build_node_v1 = node;
        registry.register(registration).await.unwrap();

        let mut plan = build_plan();
        plan.provenance.request.adapter = ProjectBuildAdapter::Node;
        plan.provenance.backend = "node".into();
        plan.process = canonical_project_build_process("node", &plan.provenance.request).unwrap();
        plan.provenance.invocation_digest = project_build_invocation_digest(&plan.process);
        let expected_error = |feature| {
            format!(
            "capability_unavailable: Runner `build-runner` does not support {feature}; upgrade that Runner to use this operation"
        )
        };

        let error = registry
            .enqueue_project_build_plan(
                "build-runner".into(),
                plan.provenance.request.clone(),
                Some(&access),
            )
            .await
            .unwrap_err();
        assert_eq!(error, expected_error(planning_feature));

        let error = registry
            .start_job(build_job_request(plan), "test".into(), Some(&access), None)
            .await
            .unwrap_err();
        assert_eq!(error.failure_kind(), "capability_unavailable");
        assert_eq!(error.to_string(), expected_error(job_feature));
        assert!(registry.list_jobs(Some(10)).await.is_empty());
        assert!(registry
            .poll(RunnerPollRequest {
                client_id: "build-runner".into(),
                runner_instance_id: "inst".into(),
            })
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn project_build_plan_requires_capability_and_stays_planning_only() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    registry.register(registration(false)).await.unwrap();

    let error = registry
        .enqueue_project_build_plan("build-runner".into(), build_request(), Some(&access))
        .await
        .unwrap_err();
    assert!(
        error.to_string().starts_with("capability_unavailable:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("Runner `build-runner`"),
        "{error}"
    );
    assert!(error.to_string().contains("project_build_v1"), "{error}");
    assert!(error.to_string().contains("upgrade that Runner"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .is_none());

    registry.register(registration(true)).await.unwrap();
    let (request_id, _rx) = registry
        .enqueue_project_build_plan("build-runner".into(), build_request(), Some(&access))
        .await
        .unwrap();
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "plan_project_build");
    assert!(matches!(
        polled.decode_operation().unwrap(),
        RunnerOperation::PlanProjectBuild(request) if request == build_request()
    ));
}

#[tokio::test]
async fn project_build_scope_capability_error_identifies_runner_without_dispatch() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    registry.register(registration(true)).await.unwrap();

    let mut request = build_request();
    request.scope = Some(webcodex_core::project_build::ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    let error = registry
        .enqueue_project_build_plan("build-runner".into(), request, Some(&access))
        .await
        .unwrap_err();
    assert!(
        error.to_string().starts_with("capability_unavailable:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("Runner `build-runner`"),
        "{error}"
    );
    assert!(
        error.to_string().contains("project_all_packages_v1"),
        "{error}"
    );
    assert!(error.to_string().contains("upgrade that Runner"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn project_build_workspace_exclusions_capability_gates_planning_and_admission() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    let mut registration = registration(true);
    registration.capabilities.project_all_packages_v1 = true;
    registration.capabilities.project_workspace_exclusions_v1 = false;
    registry.register(registration.clone()).await.unwrap();

    let mut request = build_request();
    request.scope = Some(webcodex_core::project_build::ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: vec!["crate-a".into()],
    });
    let mut empty_exclusions = request.clone();
    empty_exclusions
        .scope
        .as_mut()
        .unwrap()
        .exclude_packages
        .clear();
    assert!(registry
        .enqueue_project_build_plan("build-runner".into(), empty_exclusions, Some(&access),)
        .await
        .is_ok());

    let error = registry
        .enqueue_project_build_plan("build-runner".into(), request.clone(), Some(&access))
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("project_workspace_exclusions_v1"));
    assert!(registry.list_jobs(Some(10)).await.is_empty());

    let mut plan = build_plan();
    plan.provenance.request = request.clone();
    plan.process = canonical_project_build_process("rust", &plan.provenance.request).unwrap();
    plan.provenance.invocation_digest = project_build_invocation_digest(&plan.process);
    let error = registry
        .start_job(build_job_request(plan), "test".into(), Some(&access), None)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("project_workspace_exclusions_v1"));
    assert!(registry.list_jobs(Some(10)).await.is_empty());

    registration.capabilities.project_workspace_exclusions_v1 = true;
    registry.register(registration).await.unwrap();
    assert!(registry
        .enqueue_project_build_plan("build-runner".into(), request, Some(&access))
        .await
        .is_ok());
}

#[tokio::test]
async fn project_build_job_handoff_emits_typed_start_build() {
    let registry = RunnerRegistry::default();
    registry.register(registration(true)).await.unwrap();

    let plan = build_plan();
    let job = registry
        .start_job(build_job_request(plan.clone()), "test".into(), None, None)
        .await
        .unwrap();
    assert_eq!(job.kind, "project_build");

    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.kind, "start_build_job");
    let RunnerOperation::Job(RunnerJobOperation::StartBuild(build)) =
        polled.decode_operation().unwrap()
    else {
        panic!("expected typed StartBuild");
    };
    assert_eq!(build.process, plan.process);
    assert_eq!(build.provenance, plan.provenance);
    assert_eq!(
        build.context.runtime_project_id.as_deref(),
        Some("agent:build-runner:demo")
    );
}

#[tokio::test]
async fn project_build_base_capability_error_identifies_runner_without_job_admission() {
    let registry = RunnerRegistry::default();
    registry.register(registration(false)).await.unwrap();

    let error = registry
        .start_job(build_job_request(build_plan()), "test".into(), None, None)
        .await
        .unwrap_err();
    assert!(
        error.to_string().starts_with("capability_unavailable:"),
        "{error}"
    );
    assert!(
        error.to_string().contains("Runner `build-runner`"),
        "{error}"
    );
    assert!(error.to_string().contains("project_build_v1"), "{error}");
    assert!(error.to_string().contains("upgrade that Runner"), "{error}");
    assert!(registry.list_jobs(Some(10)).await.is_empty());
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn go_project_build_job_handoff_requires_single_module_capability() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        let mut registration = registration(true);
        registration.capabilities.project_go_single_module_v1 = supported;
        registry.register(registration).await.unwrap();

        let plan = go_build_plan();
        let result = registry
            .start_job(build_job_request(plan), "test".into(), None, None)
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
                error.to_string().contains("Runner `build-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_go_single_module_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn python_project_build_capability_is_checked_for_planning_and_auto_discovered_admission() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    registry.register(registration(true)).await.unwrap();
    let mut request = build_request();
    request.adapter = ProjectBuildAdapter::Python;
    let error = registry
        .enqueue_project_build_plan("build-runner".into(), request.clone(), Some(&access))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("project_build_python_v1"),
        "{error}"
    );
    // Auto planning can discover Python. Its typed StartBuild still cannot be
    // submitted to an older Runner advertising only generic project_build_v1.
    request.adapter = ProjectBuildAdapter::Auto;
    let process = canonical_project_build_process("python", &request).unwrap();
    let plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request,
            backend: "python".into(),
            recipe_root: ".".into(),
            root_digest: "1".repeat(64),
            manifest_digest: "2".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    };
    let error = registry
        .start_job(build_job_request(plan), "test".into(), None, None)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("project_build_python_v1"),
        "{error}"
    );
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into()
        })
        .await
        .unwrap()
        .is_none());
    let mut registered = registration(true);
    registered.capabilities.project_build_python_v1 = true;
    registry.register(registered).await.unwrap();
    let mut request = build_request();
    request.adapter = ProjectBuildAdapter::Python;
    let (_id, _rx) = registry
        .enqueue_project_build_plan("build-runner".into(), request, Some(&access))
        .await
        .unwrap();
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into()
        })
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn node_project_build_job_handoff_requires_additive_capability() {
    for supported in [false, true] {
        let registry = RunnerRegistry::default();
        let mut registration = registration(true);
        registration.capabilities.project_build_node_v1 = supported;
        registry.register(registration).await.unwrap();

        let mut plan = build_plan();
        plan.provenance.request.adapter = ProjectBuildAdapter::Node;
        plan.provenance.backend = "node".into();
        plan.process = canonical_project_build_process("node", &plan.provenance.request).unwrap();
        plan.provenance.invocation_digest = project_build_invocation_digest(&plan.process);
        let result = registry
            .start_job(build_job_request(plan), "test".into(), None, None)
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
                error.to_string().contains("Runner `build-runner`"),
                "{error}"
            );
            assert!(
                error.to_string().contains("project_build_node_v1"),
                "{error}"
            );
            assert!(error.to_string().contains("upgrade that Runner"), "{error}");
            assert!(registry.list_jobs(Some(10)).await.is_empty());
        }
    }
}

#[tokio::test]
async fn node_project_build_planning_requires_additive_capability() {
    let registry = RunnerRegistry::default();
    registry.register(registration(true)).await.unwrap();
    let mut request = build_request();
    request.adapter = ProjectBuildAdapter::Node;
    let access = auth_context(None, true);
    let error = registry
        .enqueue_project_build_plan("build-runner".into(), request, Some(&access))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("project_build_node_v1"),
        "{error}"
    );
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into()
        })
        .await
        .unwrap()
        .is_none());
}
