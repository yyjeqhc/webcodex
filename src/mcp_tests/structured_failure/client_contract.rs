//! Actual Server adapter/Runner receipt contract, not a claim about a live
//! ChatGPT/Codex client's deferred tool loader or Apps cache.
use super::*;

#[tokio::test]
async fn client_contract_edit_success_keeps_effects_and_safe_numeric_fences_across_routes() {
    for (modern, client) in [
        (false, "generic-test-client"),
        (true, "generic-test-client"),
        (true, "openai-mcp"),
    ] {
        for gateway in [false, true] {
            for dry_run in [false, true] {
                let (_tmp, db) = test_db();
                let runtime = Arc::new(test_runtime());
                register_failure_runner(&runtime).await;
                let revision = seed_failure_edit_revision(&runtime).await;
                let service = Service::new(build_test_router(
                    test_config(Some("secret")),
                    db,
                    runtime.clone(),
                ));
                let arguments = json!({"project":"agent:failure-runner:probe","dry_run":dry_run,"changes":[{
                    "kind":"edit","path":"probe.txt","expected_read_revision":revision as f64,
                    "edits":[{"kind":"replace_exact","old_text":"before","new_text":"after"}]
                }]});
                let params = if gateway {
                    adaptive_runtime_gateway_params("edit_project_files", arguments)
                } else {
                    json!({"name":"edit_project_files","arguments":arguments})
                };
                let (response, ()) = tokio::join!(
                    http_call(&service, params, client_meta(client, "fixture"), modern),
                    async {
                        let request = wait_for_failure_request(&runtime).await;
                        assert_eq!(request.kind, "file_apply_text_edits");
                        let output = json!({"dry_run":dry_run,"execution_state":"completed","applied_count":if dry_run{0}else{1},"planned_count":1,
                            "changed":!dry_run,"state_changed":!dry_run,"would_change":true,
                            "changed_paths":if dry_run {vec![]}else{vec!["probe.txt"]},
                            "files":[{"index":0,"kind":"edit","path":"probe.txt","to_path":null,
                                "old_sha256":"a".repeat(64),"new_sha256":if dry_run{"a".repeat(64)}else{"b".repeat(64)},
                                "changed":!dry_run,"would_change":true,
                                "edits":[{"index":0,"kind":"replace_exact","old_start_line":1,"old_end_line":1,"new_line_count":1}]}]
                        });
                        runtime
                            .runner_registry
                            .complete(RunnerResultRequest {
                                client_id: "failure-runner".into(),
                                runner_instance_id: "inst".into(),
                                request_id: request.request_id,
                                exit_code: Some(0),
                                stdout: Some(output.to_string()),
                                stderr: Some(String::new()),
                                stdout_truncated: false,
                                stderr_truncated: false,
                                duration_ms: Some(1),
                                error: None,
                            })
                            .await
                            .unwrap();
                    }
                );
                assert_eq!(response.0, StatusCode::OK, "{}", response.1);
                let result = &response.1["result"];
                assert_eq!(
                    result["isError"], false,
                    "modern={modern} gateway={gateway} dry_run={dry_run}: {result}"
                );
                assert_eq!(result["structuredContent"]["success"], true, "{result}");
                let output = &result["structuredContent"]["output"];
                assert_eq!(output["changed"], !dry_run);
                if dry_run {
                    assert!(
                        output["files"][0]["read_revision"].is_null(),
                        "dry-run does not mint future revision authority: {output}"
                    );
                    assert_eq!(output["state_changed"], false);
                    assert_eq!(output["would_change"], true);
                } else {
                    assert!(output["files"][0]["read_revision"].is_u64(), "{output}");
                    assert!(
                        output.get("state_changed").is_none(),
                        "success consumers use changed, not a duplicate effect field"
                    );
                }
                assert!(
                    runtime
                        .runner_registry
                        .poll(RunnerPollRequest {
                            client_id: "failure-runner".into(),
                            runner_instance_id: "inst".into()
                        })
                        .await
                        .unwrap()
                        .is_none(),
                    "adapter must never replay an edit"
                );
            }
        }
    }
}
