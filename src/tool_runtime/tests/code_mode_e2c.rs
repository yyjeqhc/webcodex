//! E2c uses the real canonical host/kernel, guarded-edit path, and Job registry.
//! Runner replies are controlled barriers, not concurrent timing assumptions.
use super::super::validation_handoff::{cargo_test_update, completed_progress, running_progress};
use super::*;
use crate::tool_runtime::code_mode::{code_mode_orchestration_policy, CodeModeCallableStage};

const EDIT: &str = r#"
const read = await tools.read_files({items:[{path:"src/example.rs"}]});
const revision = read.output.items[0].output.read_revision;
const edit = await tools.apply_text_edits({changes:[{
  kind:"edit",path:"src/example.rs",expected_read_revision:revision,
  edits:[{kind:"replace_exact",old_text:"before",new_text:"after"}]
}]});
"#;

async fn fixture(client: &str) -> (tempfile::TempDir, ToolRuntime, String, String) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/example.rs"), "before\n").unwrap();
    let runtime = test_runtime().with_validation_sync_wait(Duration::from_millis(20));
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client,
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            async_shell_jobs: true,
            structured_validation_argv: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: true,
            ..Default::default()
        },
    )
    .await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    (root, runtime, project, session.session_id)
}

async fn reach_validation(
    runtime: &ToolRuntime,
    client: &str,
    task: &JoinHandle<ToolResult>,
    reply: MutationFixtureReply,
) -> RunnerRequest {
    let mut reply = Some(reply);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(
            !task.is_finished() && Instant::now() < deadline,
            "cell ended before validation dispatch"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client).await {
            match request.kind.as_str() {
                "file_apply_text_edits" => {
                    complete_mutation_fixture(
                        runtime,
                        client,
                        request,
                        reply.take().expect("second mutation dispatched"),
                    )
                    .await
                }
                "start_validation_job" => {
                    assert!(reply.is_none(), "validation dispatched before edit");
                    assert!(
                        serde_json::to_value(&request).unwrap()["job_context"]["validation"]
                            ["source_fence"]["quiescent"]
                            .as_bool()
                            .unwrap()
                    );
                    return request;
                }
                _ => complete_agent_request_by_running_locally(runtime, client, request).await,
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
}

async fn validation_reply(
    runtime: &ToolRuntime,
    client: &str,
    request: &RunnerRequest,
    exit: Option<i32>,
) {
    let tool = serde_json::to_value(request).unwrap()["job_context"]["validation"]["tool"]
        .as_str()
        .unwrap()
        .to_string();
    let status = match exit {
        None => "running",
        Some(0) => "completed",
        Some(_) => "failed",
    };
    let stdout = if tool == "cargo_test" {
        "running 1 test\ntest example ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
    } else {
        "Checking example\nFinished check\n"
    };
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            client,
            &request.request_id,
            request.job_id.as_deref().unwrap(),
            status,
            stdout,
            if exit.is_some_and(|code| code != 0) {
                "error: validation failed\n"
            } else {
                ""
            },
            exit,
            if exit.is_some() {
                completed_progress()
            } else {
                running_progress(if tool == "cargo_test" {
                    "test"
                } else {
                    "check"
                })
            },
            exit.is_some(),
        ))
        .await
        .unwrap();
}

async fn canonical_call(runtime: &ToolRuntime, name: &str, args: Value) -> ToolResult {
    let auth = bootstrap_auth_context();
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: name.into(),
                arguments: args,
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: None,
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(outcome.error_status.is_none(), "{outcome:?}");
    outcome.result.unwrap()
}

fn spawn_e2c_mcp_call(
    runtime: &ToolRuntime,
    project: &str,
    session_id: &str,
    source: &str,
) -> JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    let project = project.to_string();
    let session_id = session_id.to_string();
    let source = source.to_string();
    tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        let outcome = runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "code_mode_exec_mutating".to_string(),
                    arguments: json!({
                        "project": project,
                        "session_id": session_id,
                        "source": source,
                        "timeout_ms": 5_000,
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session_id),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await;
        assert!(outcome.error_status.is_none(), "{outcome:?}");
        outcome.result.expect("outer E2c ToolResult")
    })
}

async fn direct_edit(
    runtime: &ToolRuntime,
    client: &str,
    project: &str,
    session: &str,
    old: &str,
    new: &str,
) {
    let rt = runtime.clone();
    let args = json!({"project":project,"session_id":session,"changes":[{
        "path":"src/example.rs","old_text":old,"new_text":new
    }]});
    let task = tokio::spawn(async move { canonical_call(&rt, "apply_text_edits", args).await });
    assert_eq!(
        service_e2b_call(
            runtime,
            client,
            &task,
            VecDeque::from([MutationFixtureReply::ApplyExact])
        )
        .await,
        1
    );
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["state_changed"], true);
}

fn validator_receipt(result: &ToolResult) -> &Value {
    result.output["effect_receipt"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|child| matches!(child["tool"].as_str(), Some("cargo_check" | "cargo_test")))
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_guarded_edit_then_check_or_test_keeps_execution_and_source_truth_separate() {
    for (tool, noop, exit) in [
        ("cargo_check", false, 0),
        ("cargo_test", false, 0),
        ("cargo_check", true, 0),
        ("cargo_check", false, 101),
    ] {
        let client = format!("e2c-known-{tool}-{noop}-{exit}");
        let (root, runtime, project, session) = fixture(&client).await;
        let source = format!("{EDIT} const validation = await tools.{tool}({{}}); text({{changed:edit.output.state_changed,success:validation.success,passed:validation.output.passed,source:validation.output.source_state}});");
        let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
        let request = reach_validation(
            &runtime,
            &client,
            &task,
            if noop {
                MutationFixtureReply::Noop
            } else {
                MutationFixtureReply::ApplyExact
            },
        )
        .await;
        validation_reply(&runtime, &client, &request, Some(exit)).await;
        let request_json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            request_json["job_context"]["validation"]["sync_wait_secs"], 5,
            "E2c API validation handoff must be owned by trusted orchestration policy"
        );
        let result = task.await.unwrap();
        assert!(
            result.success,
            "JS completion is independent of child business failure: {result:?}"
        );
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::to_value(&result).unwrap(),
            &crate::tool_runtime::registry::output_schema_for_tool("code_mode_exec_mutating"),
        )
        .expect("E2c receipt must match canonical schema");
        let emitted = emitted_json(&result);
        assert_eq!(emitted["changed"], !noop);
        assert_eq!(emitted["success"], exit == 0);
        if exit == 0 {
            assert!(
                emitted.get("passed").is_none(),
                "preserve canonical sparse terminal success"
            );
        } else {
            assert_eq!(emitted["passed"], false);
        }
        assert_eq!(emitted["source"]["freshness"], "unproven");
        let receipt = validator_receipt(&result);
        assert_eq!(receipt["outcome"], "known_result");
        assert_eq!(receipt["success"], exit == 0);
        assert_eq!(
            receipt["source_state"]["observed_mutation_fence"],
            "uncrossed"
        );
        assert!(receipt.get("job_id").is_none());
        assert_eq!(result.output["effect_receipt"]["known_results"], 2);
        assert_eq!(result.output["effect_receipt"]["outcome_unknown"], 0);
        assert_eq!(
            fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
            if noop { "before\n" } else { "after\n" }
        );
        let summary = runtime.sessions.summary(&session, Some(100)).unwrap();
        let validation = runtime
            .validation_summary_for_session_with_jobs(&summary, 20, None)
            .await;
        assert_eq!(
            validation["current_evidence"]["status"],
            if exit == 0 { "unproven" } else { "failed" },
            "{validation}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_mcp_direct_and_host_code_mode_keep_internal_five_second_handoff() {
    for (suffix, host_code_mode) in [("direct", false), ("host-code-mode", true)] {
        let client = format!("e2c-mcp-{suffix}");
        let (_root, runtime, project, session) = fixture(&client).await;
        let runtime = if host_code_mode {
            runtime.with_mcp_host_policy(
                crate::mcp_host::McpHostConfig {
                    profile: crate::mcp_host::McpHostProfile::HostCodeMode,
                    host_budget_secs: None,
                }
                .runtime_policy(),
            )
        } else {
            runtime
        };
        let source = format!(
            "{EDIT} const validation = await tools.cargo_check({{timeout_secs:600}}); text({{job_id:validation.output?.job_id??null}});"
        );
        let task = spawn_e2c_mcp_call(&runtime, &project, &session, &source);
        let request =
            reach_validation(&runtime, &client, &task, MutationFixtureReply::ApplyExact).await;
        let request_json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            request_json["job_context"]["validation"]["sync_wait_secs"], 5,
            "{suffix}"
        );
        assert_eq!(
            request_json["job_context"]["validation"]["effective_timeout_secs"], 600,
            "{suffix}: return policy must not shorten execution lifetime"
        );
        validation_reply(&runtime, &client, &request, None).await;

        let result = task.await.unwrap();
        assert!(result.success, "{suffix}: {result:?}");
        let receipt = validator_receipt(&result);
        assert_eq!(receipt["outcome"], "job_handoff", "{suffix}");
        assert_eq!(
            receipt["job_id"],
            request.job_id.as_deref().unwrap(),
            "{suffix}"
        );
    }
}

#[tokio::test]
async fn e2c_expired_read_revision_rejected_before_write_dispatch_blocks_ignored_failure_validation(
) {
    let client = "e2c-stale-pre-dispatch";
    let (root, mut runtime, project, session) = fixture(client).await;
    let rt = runtime.clone();
    let args = json!({"project":project,"session_id":session,"items":[{"path":"src/example.rs"}]});
    let read = tokio::spawn(async move { canonical_call(&rt, "read_files", args).await });
    service_tool_task(&runtime, client, &read).await;
    let revision = read.await.unwrap().output["items"][0]["output"]["read_revision"]
        .as_u64()
        .unwrap();
    // Simulate a lost Control read-registry epoch using a real old token. A new
    // read alone does NOT expire old snapshots: Runner hash guards still apply.
    runtime.read_revisions =
        Arc::new(crate::tool_runtime::read_revisions::ReadRevisionRegistry::new());
    fs::write(root.path().join("src/example.rs"), "newer\n").unwrap();
    let source = EDIT.replace("expected_read_revision:revision", &format!("expected_read_revision:{revision}"))
        + "try {await tools.cargo_check({});} catch(e) {text({edit_success:edit.success,state:edit.output.execution_state,blocked:String(e)});}";
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    assert_eq!(
        service_e2b_call(&runtime, client, &task, VecDeque::new()).await,
        0
    );
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    let emitted = emitted_json(&result);
    assert_eq!(emitted["edit_success"], false);
    assert_eq!(emitted["state"], "not_started");
    assert!(emitted["blocked"]
        .as_str()
        .unwrap()
        .contains("successful canonical apply_text_edits"));
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "neither edit nor validator may reach Runner"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "newer\n"
    );
}

#[tokio::test]
async fn e2c_runner_stale_guard_also_blocks_validation_after_known_failed_edit() {
    let client = "e2c-runner-stale";
    let (root, runtime, project, session) = fixture(client).await;
    let source = format!("{EDIT} try {{await tools.cargo_check({{}});}} catch(e) {{text({{kind:edit.output.error_kind,changed:edit.output.state_changed,blocked:String(e)}});}}");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    assert_eq!(
        service_e2b_call(
            &runtime,
            client,
            &task,
            VecDeque::from([MutationFixtureReply::ShaConflict {
                replacement: "newer\n".into()
            }])
        )
        .await,
        1
    );
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(emitted_json(&result)["kind"], "stale_file_revision");
    assert_eq!(emitted_json(&result)["changed"], false);
    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "newer\n"
    );
}

#[tokio::test]
async fn e2c_unknown_mutation_blocks_validation_even_when_js_catches_the_rejection() {
    let client = "e2c-unknown";
    let (_root, runtime, project, session) = fixture(client).await;
    let source =
        format!("{EDIT} try {{await tools.cargo_check({{}});}} catch(e) {{text(String(e));}}");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    loop {
        let request = wait_for_patch_agent_request(&runtime, client).await;
        if request.kind == "file_apply_text_edits" {
            complete_patch_agent_request(&runtime, client, &request.request_id, 0, "{}", "").await;
            break;
        }
        complete_agent_request_by_running_locally(&runtime, client, request).await;
    }
    let result = task.await.unwrap();
    assert_eq!(
        result.output["effect_receipt"]["outcome_unknown"], 1,
        "{result:?}"
    );
    assert_eq!(result.output["effect_receipt"]["known_results"], 0);
    assert!(result.output["effect_receipt"]["children"][0]
        .get("success")
        .is_none());
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "unknown edit cannot dispatch validation"
    );
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    assert!(result.output["recovery"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("reconcile_effect_state_before_retry")));
    assert!(
        !runtime
            .validation_sources
            .capture(&project)
            .unwrap()
            .quiescent
    );
}

#[tokio::test]
async fn e2c_validator_requires_edit_and_second_mutation_is_pre_dispatch_rejected() {
    let client = "e2c-order";
    let (_root, runtime, project, session) = fixture(client).await;
    for tool in ["cargo_check", "cargo_test"] {
        let task = spawn_e2b_call(
            &runtime,
            &project,
            &session,
            &format!("await tools.{tool}({{}});"),
            None,
        );
        let result = task.await.unwrap();
        assert!(!result.success);
        assert_eq!(
            result.output["child_failure"]["failure_kind"],
            "composition_policy_denied"
        );
        assert!(probe_patch_agent_request(&runtime, client).await.is_none());
    }
    let source = format!("{EDIT} await tools.cargo_check({{}}); await tools.apply_text_edits({{changes:[{{path:'src/example.rs',old_text:'after',new_text:'twice'}}]}});");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    let request = reach_validation(&runtime, client, &task, MutationFixtureReply::ApplyExact).await;
    validation_reply(&runtime, client, &request, Some(0)).await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.output["child_failure"]["failure_kind"],
        "mutation_budget_exceeded"
    );
    assert_eq!(result.output["effect_receipt"]["known_results"], 2);
    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_concurrent_canonical_write_from_another_session_crosses_running_validation() {
    let client = "e2c-concurrent";
    let (_root, runtime, project, session) = fixture(client).await;
    let source = format!("{EDIT} const r=await tools.cargo_check({{}}); text({{success:r.success,source:r.output.source_state}});");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    let request = reach_validation(&runtime, client, &task, MutationFixtureReply::ApplyExact).await;
    let other = runtime.sessions.start_session(Some(project.clone()), None);
    direct_edit(
        &runtime,
        client,
        &project,
        &other.session_id,
        "after",
        "concurrent",
    )
    .await;
    // Revert bytes before terminal: endpoint content equality cannot hide crossing.
    direct_edit(
        &runtime,
        client,
        &project,
        &other.session_id,
        "concurrent",
        "after",
    )
    .await;
    validation_reply(&runtime, client, &request, Some(0)).await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(emitted_json(&result)["success"], true);
    assert_eq!(
        validator_receipt(&result)["source_state"]["freshness"],
        "stale"
    );
    assert_eq!(
        validator_receipt(&result)["source_state"]["observed_mutation_fence"],
        "crossed"
    );
    let summary = runtime.sessions.summary(&session, Some(100)).unwrap();
    let validation = runtime
        .validation_summary_for_session_with_jobs(&summary, 20, None)
        .await;
    assert_eq!(
        validation["current_evidence"]["status"], "stale",
        "{validation}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_job_handoff_has_exact_continuation_and_later_write_invalidates_terminal_view() {
    let client = "e2c-job";
    let (_root, runtime, project, session) = fixture(client).await;
    let source = format!("{EDIT} await tools.cargo_check({{}}); try {{await tools.cargo_test({{}});}} catch(e) {{text(String(e));}}");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, None);
    let request = reach_validation(&runtime, client, &task, MutationFixtureReply::ApplyExact).await;
    validation_reply(&runtime, client, &request, None).await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    let receipt = validator_receipt(&result);
    assert_eq!(receipt["outcome"], "job_handoff");
    let job_id = request.job_id.as_deref().unwrap();
    assert_eq!(receipt["job_id"], job_id);
    assert_eq!(
        receipt["continuation"]["arguments"]["items"][0]["job_id"],
        job_id
    );
    assert!(receipt.get("success").is_none());
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "no second validator/redispatch"
    );
    let listed_active = runtime
        .list_jobs_for_auth_with_filters(
            Some(20),
            None,
            Some(project.clone()),
            Some(session.clone()),
            None,
        )
        .await;
    assert!(listed_active.success, "{listed_active:?}");
    let listed_job = listed_active.output["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["job_id"] == job_id)
        .unwrap();
    assert_eq!(
        listed_job["validation"]["source_state"]["freshness"],
        "unproven"
    );
    assert_eq!(
        listed_job["validation"]["source_state"]["observed_mutation_fence"],
        "uncrossed"
    );
    assert!(listed_job["validation"]["source_state"]
        .get("start_fence")
        .is_none());
    assert!(listed_job.get("command_summary").is_none());
    assert!(listed_job.get("stdout_tail").is_none());
    let active = runtime
        .active_jobs_summary(Some(&project), Some(&session), None, 20)
        .await;
    let active_job = active["recent"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["job_id"] == job_id)
        .unwrap();
    assert_eq!(
        active_job["validation"]["source_state"],
        listed_job["validation"]["source_state"]
    );

    validation_reply(&runtime, client, &request, Some(0)).await;
    let continuation = &receipt["continuation"];
    let observed = canonical_call(
        &runtime,
        continuation["tool"].as_str().unwrap(),
        continuation["arguments"].clone(),
    )
    .await;
    assert!(observed.success, "{observed:?}");
    let summary = runtime.sessions.summary(&session, Some(100)).unwrap();
    let before = runtime
        .validation_summary_for_session_with_jobs(&summary, 20, None)
        .await;
    assert_eq!(before["current_evidence"]["status"], "unproven", "{before}");
    let other = runtime.sessions.start_session(Some(project.clone()), None);
    direct_edit(
        &runtime,
        client,
        &project,
        &other.session_id,
        "after",
        "later",
    )
    .await;
    let terminal = runtime
        .job_status_for_auth(job_id.to_string(), false, None)
        .await;
    assert_eq!(terminal.output["validation"]["passed"], true);
    assert_eq!(
        terminal.output["validation"]["source_state"]["freshness"], "stale",
        "{terminal:?}"
    );
    let listed_stale = runtime
        .list_jobs_for_auth_with_filters(
            Some(20),
            None,
            Some(project.clone()),
            Some(session.clone()),
            None,
        )
        .await;
    let stale_job = listed_stale.output["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|job| job["job_id"] == job_id)
        .unwrap();
    assert_eq!(
        stale_job["validation"]["source_state"]["freshness"],
        "stale"
    );
    assert_eq!(
        stale_job["validation"]["source_state"]["observed_mutation_fence"],
        "crossed"
    );
    assert!(stale_job["validation"]["source_state"]
        .get("start_fence")
        .is_none());

    let after = runtime
        .validation_summary_for_session_with_jobs(&summary, 20, None)
        .await;
    assert_eq!(after["current_evidence"]["status"], "stale", "{after}");
    let handoff = canonical_call(
        &runtime,
        "session_handoff_summary",
        json!({
            "session_id": session,
            "project": project,
            "include_workspace": false,
            "include_validation": true,
            "diagnostic": true,
        }),
    )
    .await;
    assert!(handoff.success, "{handoff:?}");
    assert_eq!(
        handoff.output["validation"]["current_evidence"]["status"], "stale",
        "{handoff:#?}"
    );
    assert_eq!(
        handoff.output["continuation_feedback"]["attempt"]["validation"]["status"], "stale",
        "{handoff:#?}"
    );

    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_external_write_is_not_covered_and_never_gets_current_proof() {
    let client = "e2c-external";
    let (root, runtime, project, session) = fixture(client).await;
    let task = spawn_e2b_call(
        &runtime,
        &project,
        &session,
        &format!("{EDIT} await tools.cargo_check({{}});"),
        None,
    );
    let request = reach_validation(&runtime, client, &task, MutationFixtureReply::ApplyExact).await;
    fs::write(root.path().join("src/example.rs"), "outside Control\n").unwrap();
    validation_reply(&runtime, client, &request, Some(0)).await;
    let result = task.await.unwrap();
    assert!(result.success);
    assert_eq!(
        validator_receipt(&result)["source_state"]["freshness"],
        "unproven"
    );
    assert_eq!(
        validator_receipt(&result)["source_state"]["observed_mutation_fence"],
        "uncrossed",
        "scope excludes filesystem truth, not a snapshot"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2c_frontend_timeout_after_validation_dispatch_preserves_edit_and_exact_job() {
    let client = "e2c-timeout";
    let (_root, runtime, project, session) = fixture(client).await;
    let source = format!("{EDIT} tools.cargo_check({{}}); while(true){{}}");
    let task = spawn_e2b_call(&runtime, &project, &session, &source, Some(1000));
    let request = reach_validation(&runtime, client, &task, MutationFixtureReply::ApplyExact).await;
    validation_reply(&runtime, client, &request, None).await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["state_changed"],
        true
    );
    assert_eq!(
        validator_receipt(&result)["job_id"],
        request.job_id.as_deref().unwrap()
    );
    assert_eq!(validator_receipt(&result)["outcome"], "job_handoff");
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    assert!(result.output["recovery"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("observe_existing_job_continuations")));
    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
    validation_reply(&runtime, client, &request, Some(0)).await;
}

#[tokio::test]
async fn e2c_outer_requires_both_write_and_job_scopes_before_running_js() {
    let (_root, runtime, project, session) = fixture("oauth-client").await;
    for missing in [crate::auth::SCOPE_PROJECT_WRITE, crate::auth::SCOPE_JOB_RUN] {
        let scopes: Vec<_> = [
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_SESSION_COLLABORATE,
            crate::auth::SCOPE_PROJECT_WRITE,
            crate::auth::SCOPE_JOB_RUN,
        ]
        .into_iter()
        .filter(|scope| *scope != missing)
        .collect();
        let auth = oauth_bridge_auth_context("scope-hash", &scopes);
        let outcome = runtime.call_tool_with_context(ToolCallRequest {
            tool_name:"code_mode_exec_mutating".into(),arguments:json!({"project":project,"session_id":session,"source":"text('must not run')"}),
        }, ToolCallContext {transport:ToolTransport::Mcp,session_id:None,auth:Some(&auth),window:None,
            record_oauth_scope_denials:true,host_file_import_trust:HostFileImportTrust::Untrusted}).await;
        assert!(
            matches!(
                outcome.error_status,
                Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InsufficientScope { .. })
            ),
            "missing {missing}: {outcome:?}"
        );
        assert!(outcome.result.is_none());
        assert!(probe_patch_agent_request(&runtime, "oauth-client")
            .await
            .is_none());
    }
    let policy = code_mode_orchestration_policy(CodeModeCallableStage::GuardedEdit);
    assert!(policy.validation_after_mutation);
    assert_eq!(policy.max_mutation_calls, Some(1));
}
