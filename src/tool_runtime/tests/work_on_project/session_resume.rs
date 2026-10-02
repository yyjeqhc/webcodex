//! Session-first checkout continuation through the same API/MCP kernel.

use super::*;

async fn register_project_with_ref(
    runtime: &ToolRuntime,
    client: &str,
    id: &str,
    root: &Path,
) -> String {
    let project = register_runner_project_at_path(runtime, client, id, root).await;
    let mut summary = named_registered_project(client, id, id, &root.to_string_lossy(), 2);
    summary.root_fingerprint = Some(format!("wc_projroot_{}", "a".repeat(64)));
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        client,
        "inst",
        vec![summary],
    )
    .await;
    project
}

async fn kernel_startup(
    runtime: &ToolRuntime,
    client_id: &str,
    arguments: Value,
    auth: &crate::auth::AuthContext,
    transport: ToolTransport,
    recorder: Option<&str>,
) -> (ToolResult, Vec<String>) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let recorder = recorder.map(str::to_string);
        async move {
            let outcome = runtime
                .call_tool_with_invocation_metadata(
                    ToolCallRequest {
                        tool_name: "work_on_project".into(),
                        arguments,
                    },
                    ToolCallContext {
                        transport,
                        session_id: recorder.as_deref(),
                        auth: Some(&auth),
                        window: None,
                        record_oauth_scope_denials: false,
                        host_file_import_trust: HostFileImportTrust::default(),
                    },
                    ToolInvocationMetadata {
                        context_request: vec!["project.instructions".into()],
                        ..Default::default()
                    },
                    ToolProtocolCapabilities {
                        context_sidecar: true,
                        ..Default::default()
                    },
                )
                .await;
            if outcome.success {
                let result = outcome.result.as_ref().unwrap();
                assert_eq!(
                    outcome.correlation.resolved_project.as_deref(),
                    result.output["resolved_project"].as_str(),
                    "session-only startup must retain canonical activity attribution"
                );
            }
            outcome.result.unwrap_or_else(|| {
                ToolResult::err(format!("kernel rejection: {:?}", outcome.error_status))
            })
        }
    });
    record_startup_requests(runtime, client_id, task).await
}

fn resume_arguments(selector: &str) -> Value {
    json!({
        "session_id": selector,
        "instruction": "continue exact work",
        "include_extension_catalog": false,
    })
}

#[test]
fn session_only_resume_schema_and_parser_preserve_source_exclusivity() {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "work_on_project")
        .unwrap();
    for selector in ["wc_sess_1111111111111111", "~s42"] {
        for mode in [None, Some("checkout")] {
            let mut args = resume_arguments(selector);
            if let Some(mode) = mode {
                args["mode"] = json!(mode);
            }
            crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
                &args,
                &spec.input_schema,
            )
            .unwrap();
            let call = ToolCall::from_tool_name("work_on_project", args).unwrap();
            assert_eq!(call.session_id(), Some(selector));
            assert!(call.project().is_none());
        }
    }
    for args in [
        json!({"instruction": "missing source"}),
        json!({"session_id": null, "instruction": "missing source"}),
        json!({"session_id": "  ", "instruction": "empty selector"}),
        json!({"session_id": "~s42"}),
        json!({"session_id": "~s42", "project": "", "instruction": "empty explicit source"}),
        json!({"session_id": "~s42", "project": null, "instruction": "null explicit source"}),
        json!({"session_id": "~s42", "mode": "worktree", "instruction": "must name source"}),
        json!({"session_id": "~s42", "base_ref": "main", "instruction": "not a checkout ref"}),
        json!({"session_id": "~s42", "client_id": "runner", "instruction": "missing path"}),
        json!({"session_id": "~s42", "path": "/repo", "instruction": "missing client"}),
        json!({"session_id": "~s42", "project": "demo", "client_id": "runner", "instruction": "ambiguous"}),
    ] {
        assert!(
            ToolCall::from_tool_name("work_on_project", args.clone()).is_err(),
            "accepted invalid source: {args}"
        );
    }
}

#[tokio::test]
async fn session_only_resume_reuses_exact_session_across_transports_without_window() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    fs::write(
        root.path().join("AGENTS.md"),
        "Keep exact Session identity.\n",
    )
    .unwrap();
    let db = std::sync::Arc::new(crate::Database::open(&state.path().join("refs.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(db);
    let project = register_project_with_ref(&runtime, "resume", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_startup_without_window(
        &runtime,
        "resume",
        work_on_project_call(&project, "root task", None),
        Some(&auth),
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session_id"].as_str().unwrap();
    let session_ref = first.output["session_ref"].as_str().unwrap();
    let project_ref = first.output["project_ref"].as_str().unwrap();
    for transport in [ToolTransport::Mcp, ToolTransport::Api] {
        for selector in [session_id, session_ref] {
            let (resumed, requests) = kernel_startup(
                &runtime,
                "resume",
                resume_arguments(selector),
                &auth,
                transport,
                None,
            )
            .await;
            assert!(resumed.success, "{:?}", resumed);
            assert_eq!(resumed.output["session_id"], session_id);
            assert_eq!(resumed.output["session_ref"], session_ref);
            assert_eq!(resumed.output["project_ref"], project_ref);
            assert_eq!(resumed.output["project"], project);
            assert_eq!(resumed.output["continuation"], "resumed_explicitly");
            assert_eq!(
                resumed.output["context_projection"]["materials"][0]["status"],
                "available"
            );
            assert!(!requests
                .iter()
                .any(|kind| kind == "resolve_or_register_project"));
        }
    }
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    assert_eq!(
        runtime
            .sessions
            .summary(session_id, Some(1))
            .unwrap()
            .title
            .as_deref(),
        Some("root task")
    );
    assert_eq!(instruction_events(&runtime, session_id).len(), 5);
}

#[tokio::test]
async fn session_only_resume_switches_projects_but_never_retargets_or_inherits_recorder() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    init_git_repo(a.path());
    init_git_repo(b.path());
    let db = std::sync::Arc::new(crate::Database::open(&state.path().join("refs.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(db);
    let project_a = register_project_with_ref(&runtime, "resume-a", "demo", a.path()).await;
    let project_b = register_project_with_ref(&runtime, "resume-b", "demo", b.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_startup_without_window(
        &runtime,
        "resume-a",
        work_on_project_call(&project_a, "A", None),
        Some(&auth),
    )
    .await;
    let second = dispatch_startup_without_window(
        &runtime,
        "resume-b",
        work_on_project_call(&project_b, "B", None),
        Some(&auth),
    )
    .await;
    assert!(first.success && second.success);
    let session_a = first.output["session_id"].as_str().unwrap();
    let ref_a = first.output["session_ref"].as_str().unwrap();
    let session_b = second.output["session_id"].as_str().unwrap();
    let ref_b = second.output["session_ref"].as_str().unwrap();
    for (runner, selector, session, project) in [
        ("resume-a", ref_a, session_a, &project_a),
        ("resume-b", ref_b, session_b, &project_b),
        ("resume-a", ref_a, session_a, &project_a),
    ] {
        let (result, _) = kernel_startup(
            &runtime,
            runner,
            resume_arguments(selector),
            &auth,
            ToolTransport::Mcp,
            None,
        )
        .await;
        assert!(result.success, "{:?}", result);
        assert_eq!(result.output["session_id"], session);
        assert_eq!(result.output["project"], *project);
    }
    let before_a = instruction_events(&runtime, session_a).len();
    let before_b = instruction_events(&runtime, session_b).len();
    let mut mismatch = resume_arguments(ref_a);
    mismatch["project"] = second.output["project_ref"].clone();
    for (arguments, recorder) in [(mismatch, None), (resume_arguments(ref_a), Some(ref_b))] {
        let (result, requests) = kernel_startup(
            &runtime,
            "resume-a",
            arguments,
            &auth,
            ToolTransport::Mcp,
            recorder,
        )
        .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "session_project_mismatch");
        assert!(
            requests.is_empty(),
            "mismatch must fail before Runner work: {requests:?}"
        );
    }
    let (missing_target, requests) = kernel_startup(
        &runtime,
        "resume-a",
        json!({"instruction": "recorder is not a target"}),
        &auth,
        ToolTransport::Mcp,
        Some(ref_a),
    )
    .await;
    assert!(!missing_target.success);
    assert!(requests.is_empty());
    assert_eq!(instruction_events(&runtime, session_a).len(), before_a);
    assert_eq!(instruction_events(&runtime, session_b).len(), before_b);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project_a)),
        1
    );
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project_b)),
        1
    );
    assert_eq!(
        runtime.sessions.session_project(session_a).flatten(),
        Some(project_a)
    );
    assert_eq!(
        runtime.sessions.session_project(session_b).flatten(),
        Some(project_b)
    );
}

#[tokio::test]
async fn session_only_resume_denials_never_create_fallback_or_start_runner_work() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let db = std::sync::Arc::new(crate::Database::open(&state.path().join("refs.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_project_reference_database(db);
    let project = register_project_with_ref(&runtime, "resume-deny", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_startup_without_window(
        &runtime,
        "resume-deny",
        work_on_project_call(&project, "root task", None),
        Some(&auth),
    )
    .await;
    assert!(first.success);
    let session = first.output["session_id"].as_str().unwrap();
    let selector = first.output["session_ref"].as_str().unwrap();
    let mut foreign = auth_context(Some("other-owner"), false);
    // This caller can see the Project, but does not own the Workflow Session.
    foreign.scopes = vec!["admin".into()];
    foreign.role = Some("admin".into());
    assert_ne!(
        crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
        crate::tool_runtime::workflow_session_authority_fingerprint(Some(&foreign)).unwrap(),
    );
    assert!(runtime
        .resolve_project_input_for_auth(&project, Some(&foreign))
        .await
        .is_ok());
    for (selector, caller, kind) in [
        ("wc_sess_1111111111111111", &auth, "unknown_session_id"),
        ("~s99999999", &auth, "unknown_session_ref"),
        ("~sbad", &auth, "unknown_session_ref"),
        (selector, &foreign, "unknown_session_ref"),
        (session, &foreign, "session_authority_denied"),
    ] {
        let (result, requests) = kernel_startup(
            &runtime,
            "resume-deny",
            resume_arguments(selector),
            caller,
            ToolTransport::Mcp,
            None,
        )
        .await;
        assert!(!result.success, "{selector} unexpectedly resumed");
        if kind == "unknown_session_ref" {
            // Selector failures are kernel InvalidArguments, not business results.
            assert!(result.output.is_null());
            assert!(
                result
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("InvalidArguments")
                        && error.contains("unknown_session_ref:")),
                "{:?}",
                result
            );
        } else {
            assert_eq!(result.output["error_kind"], kind, "{:?}", result);
        }
        assert!(requests.is_empty());
    }
    let no_scope = auth_context(Some("no-runtime-scope"), false);
    let (denied, requests) = kernel_startup(
        &runtime,
        "resume-deny",
        resume_arguments(session),
        &no_scope,
        ToolTransport::Api,
        None,
    )
    .await;
    assert!(!denied.success);
    assert!(requests.is_empty());

    // Typed internal callers cannot turn session-only work into registration.
    for (mode, base_ref, client_id, path) in [
        (Some("worktree"), None, None, None),
        (None, Some("main"), None, None),
        (None, None, Some("resume-deny"), None),
        (None, None, None, Some("/missing/source")),
    ] {
        let mut call = work_on_project_call("", "must not create", Some(session));
        if let ToolCall::WorkOnProject {
            mode: m,
            base_ref: b,
            client_id: c,
            path: p,
            ..
        } = &mut call
        {
            *m = mode.map(str::to_string);
            *b = base_ref.map(str::to_string);
            *c = client_id.map(str::to_string);
            *p = path.map(str::to_string);
        }
        let (result, requests) = dispatch_recording_startup_requests(
            &runtime,
            "resume-deny",
            call,
            Some(&auth),
            "denied-window",
        )
        .await;
        assert!(!result.success);
        assert_eq!(result.output["state_changed"], false);
        assert!(requests.is_empty());
    }
    assert_eq!(instruction_events(&runtime, session).len(), 1);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    runtime.sessions.close_session(session).unwrap();
    let (closed, requests) = kernel_startup(
        &runtime,
        "resume-deny",
        resume_arguments(selector),
        &auth,
        ToolTransport::Mcp,
        None,
    )
    .await;
    assert!(!closed.success);
    assert_eq!(closed.output["error_kind"], "session_closed");
    assert!(requests.is_empty());
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        0
    );
}

#[tokio::test]
async fn session_only_resume_reauthorizes_current_project_and_rejects_unscoped_session() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let auth = auth_context(None, true);
    let project = register_project_with_ref(&runtime, "resume-hidden", "demo", root.path()).await;
    let first = dispatch_startup_without_window(
        &runtime,
        "resume-hidden",
        work_on_project_call(&project, "root task", None),
        Some(&auth),
    )
    .await;
    assert!(first.success);
    let session = first.output["session_id"].as_str().unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "resume-hidden",
        "inst",
        Vec::new(),
    )
    .await;
    let (missing_project, requests) = kernel_startup(
        &runtime,
        "resume-hidden",
        resume_arguments(session),
        &auth,
        ToolTransport::Mcp,
        None,
    )
    .await;
    assert!(!missing_project.success);
    assert!(requests.is_empty());
    assert_eq!(instruction_events(&runtime, session).len(), 1);
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );

    let unscoped = runtime
        .sessions
        .start_session_with_options(
            crate::tool_runtime::sessions::SessionCreateOptions::new(
                None,
                Some("unscoped".into()),
                SessionMode::Normal,
                SessionGuards::default(),
            )
            .with_owner_authority_fingerprint(Some(
                crate::tool_runtime::workflow_session_authority_fingerprint(Some(&auth)).unwrap(),
            )),
        )
        .unwrap();
    let (result, requests) = kernel_startup(
        &runtime,
        "resume-hidden",
        resume_arguments(&unscoped.session_id),
        &auth,
        ToolTransport::Mcp,
        None,
    )
    .await;
    assert!(!result.success);
    assert!(requests.is_empty());
    assert!(runtime
        .sessions
        .session_project(&unscoped.session_id)
        .unwrap()
        .is_none());
}
