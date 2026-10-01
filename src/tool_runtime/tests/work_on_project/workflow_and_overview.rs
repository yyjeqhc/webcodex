
#[tokio::test]
async fn work_on_project_sizes_and_runner_request_reduction_are_stable() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "Keep the focused startup safe.");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "wop-size",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("wop-size", "demo");
    let auth = auth_context(None, true);

    let (fresh, fresh_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-size",
        work_on_project_call(&project, "fresh lightweight startup", None),
        Some(&auth),
        "wop-size-fresh",
    )
    .await;
    assert!(fresh.success, "{:?}", fresh.error);
    let session_id = fresh.output["session_id"].as_str().unwrap().to_string();

    let (reused, reused_requests) = dispatch_recording_startup_requests(
        &runtime,
        "wop-size",
        work_on_project_call(&project, "unchanged continuation", Some(&session_id)),
        Some(&auth),
        "wop-size-reused",
    )
    .await;
    assert!(reused.success, "{:?}", reused.error);
    assert_eq!(reused.output["instructions"]["status"], "reused");
    assert!(reused.output["instructions"]
        .get("content_included")
        .is_none());

    let (standard, standard_requests) = dispatch_recording_coding_workflow_diagnostic(
        &runtime,
        "wop-size",
        &project,
        "same fixture standard startup",
        StartupDetail::Standard,
        Some(&auth),
    )
    .await;
    assert!(standard.success, "{:?}", standard.error);
    assert_eq!(standard.output["repository"]["status"], "available");

    for output in [&fresh.output, &reused.output] {
        for omitted in [
            "repository",
            "execution_context",
            "jobs",
            "blockers",
            "deterministic",
            "llm_summary",
        ] {
            assert!(
                output.get(omitted).is_none(),
                "work_on_project boring default field {omitted} should be omitted: {output}"
            );
        }
    }

    let fresh_overviews = fresh_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    let reused_overviews = reused_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    let standard_overviews = standard_requests
        .iter()
        .filter(|kind| kind.as_str() == "file_project_overview")
        .count();
    assert_eq!(fresh_overviews, 0);
    assert_eq!(reused_overviews, 0);
    assert_eq!(standard_overviews, 1);
    assert_eq!(
        standard_requests.len(),
        fresh_requests.len() + 1,
        "the identical advanced fixture should add only the overview request"
    );
    assert_eq!(
        reused_requests.len() + 1,
        fresh_requests.len(),
        "fresh startup pays exactly one Git baseline probe; exact continuation must preserve the durable baseline without re-probing it"
    );
    let fresh_bytes = serde_json::to_vec(&fresh.output).unwrap().len();
    let reused_bytes = serde_json::to_vec(&reused.output).unwrap().len();
    let standard_bytes = serde_json::to_vec(&standard.output).unwrap().len();
    eprintln!(
        "work_on_project_fixture_bytes fresh={fresh_bytes} unchanged_continuation={reused_bytes} standard={standard_bytes}; runner_requests fresh={} unchanged_continuation={} standard={}",
        fresh_requests.len(),
        reused_requests.len(),
        standard_requests.len()
    );
    let hard_max = crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES;
    assert!(fresh_bytes < hard_max);
    assert!(reused_bytes < hard_max);
    assert!(standard_bytes <= hard_max);
    assert!(fresh_bytes < standard_bytes);
    assert!(reused_bytes < standard_bytes);
    // The compact primary projection excludes static instruction bodies and workflow
    // guidance; keep it tightly bounded while leaving modest protocol headroom.
    assert!(
        fresh_bytes <= 4832,
        "fresh work_on_project projection regressed above the sparse context budget: {fresh_bytes} bytes"
    );
    assert!(
        reused_bytes <= 4932,
        "unchanged work_on_project projection regressed above the sparse continuation budget: {reused_bytes} bytes"
    );
}

#[tokio::test]
async fn coding_workflow_standard_repository_overview_timeout_is_nonblocking() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite overview timeout");
    // Tight overview timeout so the probe expires quickly.
    let runtime = ToolRuntime::new_for_tests()
        .with_repository_overview_probe_timeout(std::time::Duration::from_millis(50));
    let project =
        register_runner_project_at_path(&runtime, "wop-timeout", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        let project = project.clone();
        async move {
            runtime
                .start_coding_workflow_for_test(
                    project,
                    None,
                    None,
                    Some("start despite overview timeout".to_string()),
                    SessionMode::Normal,
                    false,
                    false,
                    StartupDetail::Standard,
                    None,
                    None,
                    Some(&auth),
                    None,
                    None,
                    crate::tool_runtime::sessions::SessionTransport::Mcp,
                )
                .await
        }
    });

    // Service the git/instruction probes but never the overview request.
    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut overview_request = None;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "overview-timeout startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?}"
        );
        let Some(request) = probe_patch_agent_request(&runtime, "wop-timeout").await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        if request.kind == "file_project_overview" {
            overview_request = Some(request.request_id.clone());
            // Intentionally never complete it; the probe must time out.
            continue;
        }
        let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
        complete_patch_agent_request(
            &runtime,
            "wop-timeout",
            &request.request_id,
            exit_code,
            &stdout,
            &stderr,
        )
        .await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);

    // Overview unavailable with the deterministic reason; session still works.
    assert_eq!(result.output["repository"]["status"], "unavailable");
    assert_eq!(
        result.output["repository"]["reason_code"],
        "unsupported_or_unavailable"
    );
    assert!(result.output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning == "repository_overview_unavailable"));
    let session_id = result.output["session"]["session_id"].as_str().unwrap();
    assert!(session_id.starts_with("wc_sess_"));
    let summary = runtime.sessions.summary(session_id, Some(20)).unwrap();
    assert_eq!(summary.project.as_deref(), Some(project.as_str()));

    // The timed-out overview request was cancelled server-side.
    if let Some(request_id) = overview_request {
        let expired = runtime
            .runner_registry
            .complete(crate::runner_protocol::RunnerResultRequest {
                client_id: "wop-timeout".to_string(),
                runner_instance_id: "inst".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("{}".to_string()),
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .expect_err("timed-out overview probe must remove pending waiter");
        assert!(
            expired.contains("unknown or expired shell request"),
            "{expired}"
        );
    }
}

/// Drive a coding-workflow diagnostic to completion, completing the
/// `file_project_overview` probe with `overview_stdout` (exit code 0, no error)
/// while servicing every other agent request locally. Returns the startup
/// result and the overview request id that was answered.
async fn dispatch_coding_workflow_diagnostic_with_overview_stdout(
    runtime: &ToolRuntime,
    client_id: &str,
    project: &str,
    instruction: &str,
    detail: StartupDetail,
    overview_stdout: String,
    auth: Option<&crate::auth::AuthContext>,
) -> (crate::tool_runtime::ToolResult, Option<String>) {
    use crate::tool_runtime::sessions::SessionTransport;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.cloned();
        let project = project.to_string();
        let instruction = instruction.to_string();
        async move {
            runtime
                .start_coding_workflow_for_test(
                    project,
                    None,
                    None,
                    Some(instruction),
                    SessionMode::Normal,
                    false,
                    false,
                    detail,
                    None,
                    None,
                    auth.as_ref(),
                    None,
                    None,
                    SessionTransport::Mcp,
                )
                .await
        }
    });

    let deadline = std::time::Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
    let mut overview_request_id = None;
    while !task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "overview startup did not finish within {CODING_WORKFLOW_FIXTURE_TIMEOUT:?} for client {client_id}"
        );
        let Some(request) = probe_patch_agent_request(runtime, client_id).await else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            continue;
        };
        if request.kind == "file_project_overview" {
            overview_request_id = Some(request.request_id.clone());
            complete_patch_agent_request(
                runtime,
                client_id,
                &request.request_id,
                0,
                &overview_stdout,
                "",
            )
            .await;
            continue;
        }
        let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
        complete_patch_agent_request(
            runtime,
            client_id,
            &request.request_id,
            exit_code,
            &stdout,
            &stderr,
        )
        .await;
    }
    let result = task.await.unwrap();
    (result, overview_request_id)
}

/// Build a structurally-valid root overview (depth 2 / limit 120) so a test can
/// mutate one field and observe fail-closed behavior. The fixture repo must
/// already be seeded and committed.
fn valid_agent_overview_stdout(
    runtime: &ToolRuntime,
    client_id: &str,
    root: &std::path::Path,
) -> String {
    let _ = (runtime, client_id);
    let overview = crate::project_overview::build_project_overview(root, ".", Some(2), Some(120))
        .expect("valid agent overview fixture");
    overview.to_string()
}

#[tokio::test]
async fn coding_workflow_standard_repository_overview_rejects_malformed_runner_responses() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite malformed overview");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-malformed", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let valid = valid_agent_overview_stdout(&runtime, "wop-malformed", root.path());
    let valid_value: serde_json::Value = serde_json::from_str(&valid).unwrap();

    let cases: Vec<(&str, serde_json::Value)> = vec![
        ("absolute path", {
            let mut v = valid_value.clone();
            v["top_level"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "/etc/passwd", "kind": "file"}));
            v
        }),
        ("parent traversal", {
            let mut v = valid_value.clone();
            v["manifests"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "../outside/Cargo.toml", "kind": "rust_manifest"}));
            v
        }),
        ("request boundary mismatch", {
            let mut v = valid_value.clone();
            v["scan"]["max_depth"] = json!(4);
            v["scan"]["limit"] = json!(500);
            v["path"] = json!("src");
            v
        }),
        ("unknown project type", {
            let mut v = valid_value.clone();
            v["project_types"]
                .as_array_mut()
                .unwrap()
                .push(json!({"kind": "cobol", "evidence": []}));
            v
        }),
        ("unknown key-file kind", {
            let mut v = valid_value.clone();
            v["key_files"]
                .as_array_mut()
                .unwrap()
                .push(json!({"path": "README.md", "kind": "mystery", "reason": "x"}));
            v
        }),
        ("unknown warning", {
            let mut v = valid_value.clone();
            v["warnings"]
                .as_array_mut()
                .unwrap()
                .push(json!("nuclear_launch_detected"));
            v
        }),
        ("returned_entry_count as string", {
            let mut v = valid_value.clone();
            v["scan"]["returned_entry_count"] = json!("plenty");
            v
        }),
        ("warnings as object", {
            let mut v = valid_value.clone();
            v["warnings"] = json!({"note": "not an array"});
            v
        }),
        ("duplicate top-level paths", {
            let mut v = valid_value.clone();
            let top = v["top_level"].as_array_mut().unwrap();
            top.push(top[0].clone());
            v
        }),
    ];

    for (label, payload) in cases {
        let stdout = payload.to_string();
        let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
            &runtime,
            "wop-malformed",
            &project,
            label,
            StartupDetail::Standard,
            stdout,
            Some(&auth),
        )
        .await;
        assert!(
            result.success,
            "{label}: task must still succeed: {:?}",
            result.error
        );
        assert!(
            overview_id.is_some(),
            "{label}: overview probe must be issued"
        );
        let repository = &result.output["repository"];
        assert_eq!(
            repository["status"], "unavailable",
            "{label}: malformed Runner response must fail closed"
        );
        assert_eq!(
            repository["reason_code"], "unsupported_or_unavailable",
            "{label}: deterministic reason code"
        );
        // No raw stdout, stderr, error text, or absolute paths leak into the
        // model-facing compact output.
        let serialized = result.output.to_string();
        assert!(
            !serialized.contains("runner_secret"),
            "{label}: extra Runner field leaked"
        );
        assert!(
            !serialized.contains("/etc/passwd") && !serialized.contains("/absolute/leak"),
            "{label}: absolute path leaked"
        );
        assert!(
            !serialized.contains("nuclear_launch_detected") && !serialized.contains("cobol"),
            "{label}: malformed enum leaked"
        );
        assert!(
            !serialized.contains("../outside"),
            "{label}: traversal path leaked"
        );
        assert!(
            serialized.len() <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES,
            "{label}: compact output exceeded 30 KiB"
        );
        // The deterministic unavailable warning is surfaced.
        assert!(
            result.output["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|warning| warning == "repository_overview_unavailable"),
            "{label}: repository_overview_unavailable warning missing"
        );
        // A session is still created despite the malformed overview.
        let session_id = result.output["session"]["session_id"].as_str().unwrap();
        assert!(
            session_id.starts_with("wc_sess_"),
            "{label}: session not created"
        );
    }
}

#[tokio::test]
async fn coding_workflow_standard_overview_strips_unknown_runner_fields_and_stays_bounded() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load despite extra runner fields");
    let runtime = ToolRuntime::new_for_tests();
    let project = register_runner_project_at_path(&runtime, "wop-strip", "demo", root.path()).await;
    let auth = auth_context(None, true);

    let valid = valid_agent_overview_stdout(&runtime, "wop-strip", root.path());
    let mut payload: serde_json::Value = serde_json::from_str(&valid).unwrap();
    // A malicious/defensive Runner adds an oversized `scan` extra field and
    // top-level unknowns (including an absolute path). The contract must not
    // fail on mere extras — it must strip them and keep the formal fields only,
    // so the model output stays small and free of leaked content.
    payload["scan"]["padding"] = json!("X".repeat(40_000));
    payload["scan"]["nested"] = json!({"deep": json!(["Y".repeat(10_000), 1, 2])});
    payload["runner_secret"] = json!("/absolute/leak");

    let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
        &runtime,
        "wop-strip",
        &project,
        "strip extras",
        StartupDetail::Standard,
        payload.to_string(),
        Some(&auth),
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert!(overview_id.is_some(), "overview probe must be issued");
    let repository = &result.output["repository"];
    assert_eq!(
        repository["status"], "available",
        "extras must be stripped, not rejected"
    );

    // scan keeps exactly the 5 fixed formal fields; padding/nested dropped.
    let scan = &repository["scan"];
    assert!(scan.is_object());
    let mut scan_keys: Vec<&str> = scan
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    scan_keys.sort_unstable();
    assert_eq!(
        scan_keys,
        [
            "limit",
            "max_depth",
            "returned_entry_count",
            "truncated",
            "truncation_reason"
        ],
        "scan must keep only the fixed fields: {scan:?}"
    );
    assert_eq!(scan["max_depth"], 2);
    assert_eq!(scan["limit"], 120);

    let serialized = result.output.to_string();
    assert!(!serialized.contains("padding"), "scan padding leaked");
    assert!(
        !serialized.contains("runner_secret"),
        "extra runner field leaked"
    );
    assert!(
        !serialized.contains("/absolute/leak"),
        "absolute path leaked"
    );
    assert!(
        serialized.len() <= crate::tool_runtime::startup_brief::STANDARD_STARTUP_HARD_MAX_BYTES,
        "compact output exceeded 30 KiB after stripping"
    );
}

#[tokio::test]
async fn coding_workflow_standard_and_full_accept_valid_repository_overview() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "rules load with valid overview");
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_runner_project_at_path(&runtime, "wop-valid-overview", "demo", root.path()).await;

    let auth = auth_context(None, true);
    for detail in [StartupDetail::Standard, StartupDetail::Full] {
        let stdout = valid_agent_overview_stdout(&runtime, "wop-valid-overview", root.path());
        let (result, overview_id) = dispatch_coding_workflow_diagnostic_with_overview_stdout(
            &runtime,
            "wop-valid-overview",
            &project,
            "valid overview",
            detail,
            stdout,
            Some(&auth),
        )
        .await;
        assert!(result.success, "{detail:?}: {:?}", result.error);
        assert!(
            overview_id.is_some(),
            "{detail:?}: overview probe must be issued"
        );
        let brief = crate::tool_runtime::startup_brief::startup_brief_from_output(&result.output)
            .expect("advanced startup brief");
        let semantic = &brief["semantic_navigation"];
        assert_eq!(semantic["status"], "probe_failed");
        assert_eq!(semantic["available"], Value::Null);
        assert_eq!(semantic["reason_code"], "malformed_agent_result");
        assert!(!brief["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning == "semantic_navigation_unavailable"));
        if detail == StartupDetail::Full {
            // Full diagnostics retain the original probe status and reason.
            assert_eq!(
                result.output["semantic_navigation"]["status"],
                "probe_failed"
            );
            assert_eq!(
                result.output["semantic_navigation"]["available"],
                Value::Null
            );
            assert_eq!(
                result.output["semantic_navigation"]["reason_code"],
                "malformed_agent_result"
            );
        }
        let repository = &brief["repository"];
        assert_eq!(
            repository["status"], "available",
            "{detail:?}: valid response must be accepted"
        );
        // scan projection keeps only the fixed fields, no extras.
        let scan = &repository["scan"];
        assert!(scan.is_object());
        assert_eq!(scan.as_object().unwrap().len(), 5);
        assert_eq!(scan["max_depth"], 2);
        assert_eq!(scan["limit"], 120);
        // Rust is detected via the committed Cargo.toml fixture.
        let types = repository["project_types"]["items"].as_array().unwrap();
        assert!(types.iter().any(|kind| kind["kind"] == "rust"));
        assert!(repository["manifests"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|manifest| manifest["path"] == "Cargo.toml"));
        assert!(!repository["key_files"]["items"]
            .as_array()
            .unwrap()
            .is_empty());
        let serialized = repository.to_string();
        assert!(!serialized.contains(&root.path().to_string_lossy().to_string()));

        let schema =
            crate::tool_runtime::registry::coding_workflow_diagnostic_output_schema_for_test();
        let instance = json!({"success": true, "output": result.output});
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&instance, &schema)
            .unwrap_or_else(|error| {
                panic!("{detail:?} coding workflow diagnostic must match strict schema: {error}")
            });
    }
}

#[tokio::test]
async fn work_on_project_guidance_profile_is_request_local_and_not_durable() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "profile-independent project rule");
    let state = tempfile::tempdir().unwrap();
    let ledger = state.path().join("sessions.json");
    let runtime = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    let project =
        register_runner_project_at_path(&runtime, "wop-profile", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let first = dispatch_coding_call_in_window(
        &runtime,
        "wop-profile",
        work_on_project_call(&project, "root objective", None),
        Some(&auth),
        "same-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert!(first.output.get("workflow").is_none());
    let session_id = first.output["session_id"].as_str().unwrap().to_string();
    let before =
        serde_json::to_value(runtime.sessions.summary(&session_id, Some(50)).unwrap()).unwrap();
    let mut cases = vec![Some("direct"), Some("host_code_mode")];
    #[cfg(feature = "experimental-code-mode")]
    cases.push(Some("code_mode"));
    cases.push(None); // Same Window/Session must not remember the last profile.
    let mut baseline = None;
    let mut requests_baseline = None;
    for profile in cases {
        let mut args = json!({"project": project, "instruction": "continue the task", "session_id": session_id, "include_extension_catalog": false});
        if let Some(profile) = profile {
            args["guidance_profile"] = json!(profile);
        }
        let call = ToolCall::from_tool_name("work_on_project", args).unwrap();
        let audit =
            crate::tool_runtime::tool_audit::ToolCallAuditProjection::session_log_arguments(&call);
        assert!(
            audit.get("guidance_profile").is_none(),
            "presentation selection must not leak into Session event arguments"
        );
        let (result, mut requests) = dispatch_recording_startup_requests(
            &runtime,
            "wop-profile",
            call,
            Some(&auth),
            "same-window",
        )
        .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_id"], session_id);
        assert_eq!(result.output["continuation"], "resumed_explicitly");
        assert!(result.output.get("workflow").is_none());
        let schema = crate::tool_runtime::registry::output_schema_for_tool("work_on_project");
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &json!({"success":true,"output":result.output}),
            &schema,
        )
        .unwrap();
        assert!(serde_json::to_vec(&result.output).unwrap().len() < 30 * 1024);
        if let Some(expected) = &baseline {
            assert_eq!(
                &result.output, expected,
                "guidance profile must not change the primary work_on_project projection"
            );
        } else {
            baseline = Some(result.output);
        }
        requests.sort();
        if let Some(expected) = &requests_baseline {
            assert_eq!(
                &requests, expected,
                "profile must not change startup probes"
            );
        } else {
            requests_baseline = Some(requests);
        }
    }
    let after =
        serde_json::to_value(runtime.sessions.summary(&session_id, Some(50)).unwrap()).unwrap();
    for field in [
        "session_id",
        "project",
        "title",
        "mode",
        "guards",
        "execution_context",
    ] {
        assert_eq!(before[field], after[field], "{field}");
    }
    assert_eq!(
        runtime
            .sessions
            .active_session_count_for_test(Some(&project)),
        1
    );
    runtime.sessions.flush_persistence();
    let persisted = fs::read_to_string(&ledger).unwrap();
    for absent in [
        "guidance_profile",
        "tool_strategy",
        "webcodex.coding_workflow",
        "code_mode",
    ] {
        assert!(
            !persisted.contains(absent),
            "profile must not become durable business state: {absent}"
        );
    }
    let restored = ToolRuntime::new_for_tests().with_session_ledger(&ledger);
    assert_eq!(
        restored
            .sessions
            .summary(&session_id, Some(50))
            .unwrap()
            .title
            .as_deref(),
        Some("root objective")
    );
}

#[tokio::test]
async fn runner_instruction_refresh_keeps_local_changes_and_observes_suppressed_removals() {
    let root = tempfile::tempdir().unwrap();
    seed_coding_repository(root.path(), "LOCAL_INITIAL_RULE");
    let runtime = ToolRuntime::new_for_tests();
    register_agent_with_projects(
        &runtime,
        "instruction-refresh",
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            lsp_read_only_navigation: true,
            internal_posix_script: true,
            instruction_runtime: true,
            ..Default::default()
        },
        vec![registered_project("demo", &root.path().to_string_lossy())],
    )
    .await;
    let project = crate::tool_runtime::runner_project_runtime_id("instruction-refresh", "demo");
    let auth = auth_context(None, true);
    let mut global: RunnerInstructionSnapshotResponse = serde_json::from_str(
        &runner_instruction_snapshot_stdout(&"g".repeat(32 * 1024), 7),
    )
    .unwrap();
    for index in 1..16 {
        let mut file = global.files[0].clone();
        file.path = format!("runner/{index}/extra.md");
        file.content.clear();
        file.chars = 0;
        file.truncated = true;
        global.files.push(file);
    }
    let global = serde_json::to_string(&global).unwrap();
    let (first, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "initial", None),
        Some(&auth),
        "instruction-window",
        &global,
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let id = first.output["session_id"].as_str().unwrap();
    let sources = first.output["instructions"]["sources"].as_array().unwrap();
    assert!(sources.iter().all(|source| source.get("content").is_none()));
    assert!(sources.iter().any(|source| source["path"] == "AGENTS.md"));
    assert_eq!(sources.len(), 17);
    assert!(sources[0].get("read_more").is_none());
    assert!(serde_json::to_vec(&first.output).unwrap().len() <= 30 * 1024);

    std::fs::write(root.path().join("AGENTS.md"), "LOCAL_UPDATED_RULE").unwrap();
    let (failed, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "global refresh fails", Some(id)),
        Some(&auth),
        "instruction-window",
        "invalid snapshot",
    )
    .await;
    assert!(failed.success, "{:?}", failed.error);
    assert_eq!(failed.output["instructions"]["status"], "unavailable");
    assert!(failed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("AGENTS.md")));
    let summary = runtime
        .sessions
        .summary(id, None)
        .unwrap()
        .project_instructions
        .unwrap();
    let updated = summary
        .files
        .iter()
        .find(|file| file.path == "AGENTS.md")
        .unwrap()
        .fingerprint
        .clone();

    std::fs::remove_file(root.path().join("AGENTS.md")).unwrap();
    let (suppressed, requests) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "observe deletion silently", Some(id)),
        Some(&auth),
        "instruction-window",
        "invalid snapshot",
    )
    .await;
    assert!(suppressed.success, "{:?}", suppressed.error);
    assert!(requests
        .iter()
        .any(|kind| kind == RUNNER_INSTRUCTION_REQUEST_KIND));
    assert!(suppressed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source.get("content").is_none()));
    let summary = runtime
        .sessions
        .summary(id, None)
        .unwrap()
        .project_instructions
        .unwrap();
    assert!(summary.files.iter().all(|file| file.fingerprint != updated));
    assert!(summary.files.iter().all(|file| file.path != "AGENTS.md"));

    let empty = serde_json::to_string(&RunnerInstructionSnapshotResponse {
        format: RUNNER_INSTRUCTION_RESPONSE_FORMAT.into(),
        generation: 7,
        scan_complete: true,
        files: Vec::new(),
    })
    .unwrap();
    let (removed, _) = dispatch_recording_startup_requests_with_runner_instructions(
        &runtime,
        "instruction-refresh",
        work_on_project_call(&project, "global removed", Some(id)),
        Some(&auth),
        "instruction-window",
        &empty,
    )
    .await;
    assert!(removed.success, "{:?}", removed.error);
    assert!(removed.output["instructions"]["changed_sources"]
        .as_array()
        .unwrap()
        .contains(&json!("runner/0/AGENTS.md")));
    assert!(removed.output["instructions"]["sources"]
        .as_array()
        .unwrap()
        .iter()
        .all(|source| source["source_scope"] != "runner"));
}

#[tokio::test]
async fn work_on_project_distinguishes_unavailable_from_inconclusive_lsp_probes() {
    use crate::lsp_bridge::{LspAvailabilityStatus, LspServerStatusEntry, LspStatusResult};
    use std::time::{Duration, Instant};

    for (status, reason) in [
        ("not_observed", None),
        ("probe_failed", Some("status_probe_failed")),
        ("unavailable", Some("server_unavailable")),
    ] {
        let root = tempfile::tempdir().unwrap();
        init_git_repo(root.path());
        commit_file(root.path(), "README.md", "# fixture\n", "seed");
        let runtime = ToolRuntime::new_for_tests()
            .with_semantic_navigation_probe_timeout(Duration::from_secs(60));
        let project =
            register_runner_project_at_path(&runtime, "wop-probe-state", "demo", root.path()).await;
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        work_on_project_call(&project, "continue normal coding", None),
                        Some(&auth_context(None, true)),
                    )
                    .await
            }
        });
        let deadline = Instant::now() + CODING_WORKFLOW_FIXTURE_TIMEOUT;
        let mut probe_count = 0;
        while !task.is_finished() {
            assert!(Instant::now() < deadline, "startup stuck for {status}");
            let Some(request) = probe_patch_agent_request(&runtime, "wop-probe-state").await else {
                tokio::time::sleep(Duration::from_millis(2)).await;
                continue;
            };
            if request.kind != AGENT_LSP_REQUEST_KIND {
                complete_agent_request_by_running_locally(&runtime, "wop-probe-state", request)
                    .await;
                continue;
            }
            probe_count += 1;
            if status == "not_observed" {
                // Leave only the optional probe unanswered. Mandatory completion
                // must cancel it without waiting for the provider timeout.
                continue;
            }
            let envelope = if status == "unavailable" {
                RunnerLspResultEnvelope::ok(LspStatusResult {
                    project: "demo".to_string(),
                    detected_languages: vec!["rust".to_string()],
                    servers: vec![LspServerStatusEntry {
                        language: "rust".to_string(),
                        server: "rust-analyzer".to_string(),
                        available: false,
                        running: false,
                        status: LspAvailabilityStatus::Unavailable,
                        source: None,
                        position_encoding: None,
                    }],
                    warnings: vec![],
                })
            } else {
                RunnerLspResultEnvelope::err("lsp_protocol_error", "probe did not conclude")
            };
            complete_patch_agent_request(
                &runtime,
                "wop-probe-state",
                &request.request_id,
                0,
                &envelope.to_stdout_json(),
                "",
            )
            .await;
        }
        let result = task.await.unwrap();
        assert!(result.success, "{status}: {result:?}");
        assert_eq!(probe_count, 1);
        let semantic = &result.output["semantic_navigation"];
        assert_eq!(semantic["supported"], true);
        assert_eq!(semantic["status"], status);
        assert_eq!(semantic["reason_code"], json!(reason));
        if status == "unavailable" {
            assert_eq!(semantic["available"], false);
            assert_eq!(result.output["readiness"]["status"], "warn");
            assert_eq!(result.output["readiness"]["blocking"], false);
            assert_eq!(
                result.output["warnings"],
                json!(["semantic_navigation_unavailable"])
            );
        } else {
            assert_eq!(semantic["available"], Value::Null);
            assert!(result.output.get("readiness").is_none(), "{result:?}");
            assert!(result.output.get("warnings").is_none(), "{result:?}");
        }
        assert!(semantic.get("action_required").is_none());
        assert!(result.output.get("blockers").is_none());
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::to_value(&result).unwrap(),
            &crate::tool_runtime::registry::output_schema_for_tool("work_on_project"),
        )
        .unwrap();
    }
}
