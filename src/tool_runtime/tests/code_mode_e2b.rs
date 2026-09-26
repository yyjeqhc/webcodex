use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerRequest};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
};
use crate::tool_runtime::orchestration_host::{CanonicalOrchestrationHost, OrchestrationPolicy};
use crate::tool_runtime::permissions::{AuthorityMode, PermissionEvaluator};
use crate::tool_runtime::sessions::{CodingSessionRequest, SessionGuards, SessionTransport};
use crate::tool_runtime::{SessionMode, ToolCall, ToolResult, ToolRuntime};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

#[path = "code_mode_e2c.rs"]
mod e2c;

const E2B_MUTATION_ONLY_POLICY: OrchestrationPolicy = OrchestrationPolicy {
    frontend: "code_mode_e2b_test",
    policy_name: "Code Mode E2b test",
    admitted_tools: &["apply_text_edits"],
    denied_tools: &["code_mode_exec_mutating"],
    additional_forbidden_argument_fields: &[],
    child_return_timing: crate::tool_runtime::return_timing::ToolReturnTimingPolicy::unconstrained(
    ),
    max_mutation_calls: Some(1),
    validation_after_mutation: false,
};

const READ_ONLY_TEST_POLICY: OrchestrationPolicy = OrchestrationPolicy {
    frontend: "code_mode_e2b_read_test",
    policy_name: "Code Mode E2b read test",
    admitted_tools: &["read_files"],
    denied_tools: &[],
    additional_forbidden_argument_fields: &[],
    child_return_timing: crate::tool_runtime::return_timing::ToolReturnTimingPolicy::unconstrained(
    ),
    max_mutation_calls: None,
    validation_after_mutation: false,
};

#[derive(Debug, Clone)]
enum MutationFixtureReply {
    ApplyExact,
    Noop,
    ShaConflict { replacement: String },
    MultipleMatches,
}

fn init_git_repo(root: &Path) {
    let status = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success());
    for (key, value) in [
        ("user.email", "tests@example.invalid"),
        ("user.name", "WebCodex Tests"),
    ] {
        let status = std::process::Command::new("git")
            .args(["config", key, value])
            .current_dir(root)
            .status()
            .unwrap();
        assert!(status.success());
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn commit_file(root: &Path, path: &str, content: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, content).unwrap();
    git(root, &["add", "--", path]);
    git(root, &["commit", "-q", "-m", "fixture"]);
}

fn start_coding_session_with_baseline(
    runtime: &ToolRuntime,
    auth: &crate::auth::AuthContext,
    project: &str,
    baseline: String,
    instruction: &str,
) -> crate::tool_runtime::SessionSummary {
    let authority_fingerprint =
        crate::tool_runtime::workflow_session_authority_fingerprint(Some(auth)).unwrap();
    runtime
        .sessions
        .ensure_coding_session_with_git_baseline(
            CodingSessionRequest {
                project: project.to_string(),
                authority_fingerprint,
                resume_session_id: None,
                instruction: Some(instruction.to_string()),
                mode: SessionMode::Normal,
                guards: SessionGuards::default(),
                execution_context: None,
                project_instructions: None,
                transport: SessionTransport::Mcp,
                context_refreshed: true,
                write_scope_verified: true,
            },
            Some(baseline),
        )
        .unwrap()
        .summary
}

fn spawn_e2b_call(
    runtime: &ToolRuntime,
    project: &str,
    session_id: &str,
    source: &str,
    timeout_ms: Option<u64>,
) -> JoinHandle<ToolResult> {
    let runtime = runtime.clone();
    let project = project.to_string();
    let session_id = session_id.to_string();
    let source = source.to_string();
    let auth = bootstrap_auth_context();
    tokio::spawn(async move {
        runtime
            .dispatch_with_auth(
                ToolCall::CodeModeExecMutating {
                    project,
                    session_id,
                    source,
                    timeout_ms,
                },
                Some(&auth),
            )
            .await
    })
}

fn emitted_json(result: &ToolResult) -> Value {
    serde_json::from_str(
        result.output["content"][0]
            .as_str()
            .expect("Code Mode test must emit one JSON text value"),
    )
    .expect("Code Mode emitted text must be JSON")
}

async fn complete_mutation_fixture(
    runtime: &ToolRuntime,
    client_id: &str,
    request: RunnerRequest,
    reply: MutationFixtureReply,
) {
    assert_eq!(request.kind, "file_apply_text_edits");
    let payload: Value = serde_json::from_str(
        request
            .content
            .as_deref()
            .expect("apply_text_edits Runner request content"),
    )
    .unwrap();
    let change = &payload["changes"][0];
    let path = change["path"].as_str().expect("fixture edit path");
    let root = Path::new(request.cwd.as_deref().expect("fixture cwd"));
    let full = root.join(path);
    let stdout = match reply {
        MutationFixtureReply::ApplyExact => {
            let edit = &change["edits"][0];
            assert_eq!(edit["kind"], "replace_exact");
            let old_text = edit["old_text"].as_str().unwrap();
            let new_text = edit["new_text"].as_str().unwrap_or_default();
            let current = fs::read_to_string(&full).unwrap();
            assert_eq!(
                current.matches(old_text).count(),
                1,
                "fixture only supports one exact match"
            );
            let next = current.replacen(old_text, new_text, 1);
            let would_change = next != current;
            let dry_run = payload["dry_run"].as_bool().unwrap_or(false);
            let changed = would_change && !dry_run;
            let old_sha256 = crate::tool_runtime::files::sha256_hex_bytes(current.as_bytes());
            let new_sha256 = crate::tool_runtime::files::sha256_hex_bytes(next.as_bytes());
            if changed {
                fs::write(&full, next).unwrap();
            }
            json!({
                "dry_run": dry_run,
                "applied_count": 1,
                "changed": changed,
                "would_change": would_change,
                "files": [{
                    "index": 0, "kind": "edit", "path": path, "to_path": null,
                    "old_sha256": old_sha256, "new_sha256": new_sha256,
                    "changed": changed, "would_change": would_change, "edits": []
                }],
                "changed_paths": if changed { vec![path] } else { Vec::<&str>::new() },
            })
        }
        MutationFixtureReply::Noop => {
            let sha256 = crate::tool_runtime::files::sha256_hex_bytes(&fs::read(&full).unwrap());
            json!({
                "dry_run": payload["dry_run"].as_bool().unwrap_or(false),
                "applied_count": 1,
                "changed": false,
                "would_change": false,
                "files": [{
                    "index": 0, "kind": "edit", "path": path, "to_path": null,
                    "old_sha256": sha256, "new_sha256": sha256,
                    "changed": false, "would_change": false, "edits": []
                }],
                "changed_paths": [],
            })
        }
        MutationFixtureReply::ShaConflict { replacement } => {
            fs::write(&full, replacement).unwrap();
            let expected = change["expected_sha256"]
                .as_str()
                .expect("read_revision must translate to expected_sha256");
            let current = crate::tool_runtime::files::sha256_hex_bytes(&fs::read(&full).unwrap());
            assert_ne!(expected, current);
            json!({
                "error": "expected_sha256 mismatch. No files were modified",
                "error_kind": "sha256_conflict",
                "change_index": 0,
                "changed": false,
                "state_changed": false,
                "rollback_complete": true,
                "conflict_recovery": {
                    "schema_version": 1,
                    "conflict_kind": "sha256_mismatch",
                    "expected_sha256": expected,
                    "current_sha256": current,
                    "occurrence_selector_supported": false,
                    "direct_retry_safe": false,
                    "reread_required": true,
                    "recovery_action": "reread_file"
                }
            })
        }
        MutationFixtureReply::MultipleMatches => json!({
            "error": "replace_exact matched multiple locations. No files were modified",
            "error_kind": "edit_conflict",
            "change_index": 0,
            "edit_index": 0,
            "kind": "replace_exact",
            "path": path,
            "changed": false,
            "state_changed": false,
            "conflict_recovery": {
                "schema_version": 1,
                "conflict_kind": "multiple_matches",
                "match_count": 2,
                "occurrence_selector_supported": true,
                "direct_retry_safe": true,
                "reread_required": false,
                "candidate_ranges": [
                    {"occurrence": 1, "start_line": 1, "end_line": 1},
                    {"occurrence": 2, "start_line": 3, "end_line": 3}
                ],
                "candidates_truncated": false,
                "recovery_action": "select_occurrence_or_refine_match"
            }
        }),
    };
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &stdout.to_string(),
        "",
    )
    .await;
}

async fn service_e2b_call(
    runtime: &ToolRuntime,
    client_id: &str,
    task: &JoinHandle<ToolResult>,
    mut mutation_replies: VecDeque<MutationFixtureReply>,
) -> usize {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut mutation_requests = 0usize;
    while !task.is_finished() {
        assert!(Instant::now() < deadline, "E2b test call did not finish");
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            if request.kind == "file_apply_text_edits" {
                mutation_requests += 1;
                let reply = mutation_replies
                    .pop_front()
                    .expect("unexpected extra mutation Runner request");
                complete_mutation_fixture(runtime, client_id, request, reply).await;
            } else {
                complete_agent_request_by_running_locally(runtime, client_id, request).await;
            }
        } else {
            tokio::task::yield_now().await;
        }
    }
    assert!(
        mutation_replies.is_empty(),
        "not all expected mutation requests were observed"
    );
    mutation_requests
}

async fn service_tool_task(runtime: &ToolRuntime, client_id: &str, task: &JoinHandle<ToolResult>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "tool task did not finish for {client_id}"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        } else {
            tokio::task::yield_now().await;
        }
    }
}

async fn e2b_fixture(
    client_id: &str,
    initial: &str,
) -> (tempfile::TempDir, ToolRuntime, String, String) {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/example.rs"), initial).unwrap();
    let runtime = test_runtime();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client_id,
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: true,
            ..Default::default()
        },
    )
    .await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("E2b integration".to_string()));
    (root, runtime, project, session.session_id)
}

#[tokio::test]
async fn e2b_read_guarded_edit_post_read_preserves_canonical_mutation_truth() {
    let (root, runtime, project, session_id) =
        e2b_fixture("e2b-read-edit-read", "fn value() -> i32 { 1 }\n").await;
    let source = r#"
        const before = await tools.read_files({items:[{path:"src/example.rs"}]});
        const revision = before.output.items[0].output.read_revision;
        const edit = await tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs", expected_read_revision:revision,
            edits:[{kind:"replace_exact", old_text:"{ 1 }", new_text:"{ 2 }"}]
        }]});
        const after = await tools.read_files({items:[{path:"src/example.rs"}]});
        text(JSON.stringify({
            edit_success: edit.success,
            changed: edit.output.state_changed,
            after: after.output.items[0].output.text
        }));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, Some(5_000));
    let mutation_requests = service_e2b_call(
        &runtime,
        "e2b-read-edit-read",
        &task,
        VecDeque::from([MutationFixtureReply::ApplyExact]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(mutation_requests, 1);
    let emitted = emitted_json(&result);
    assert_eq!(emitted["edit_success"], true);
    assert_eq!(emitted["changed"], true);
    assert!(emitted["after"].as_str().unwrap().contains("{ 2 }"));
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "fn value() -> i32 { 2 }\n"
    );
    let receipt = &result.output["effect_receipt"];
    assert_eq!(receipt["consequential_calls"], 1);
    assert_eq!(receipt["known_results"], 1);
    assert_eq!(receipt["outcome_unknown"], 0);
    assert_eq!(receipt["children"][0]["tool"], "apply_text_edits");
    assert_eq!(receipt["children"][0]["state_changed"], true);
    assert!(
        runtime
            .sessions
            .summary(&session_id, None)
            .unwrap()
            .repository_edit_observed
    );
}

#[tokio::test]
async fn e2b_noop_mutation_is_known_false_without_edit_provenance() {
    let (_root, runtime, project, session_id) = e2b_fixture("e2b-noop", "same\n").await;
    let source = r#"
        const before = await tools.read_files({items:[{path:"src/example.rs"}]});
        const revision = before.output.items[0].output.read_revision;
        const edit = await tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs", expected_read_revision:revision,
            edits:[{kind:"replace_exact", old_text:"same", new_text:"same"}]
        }]});
        text(JSON.stringify({success:edit.success, changed:edit.output.state_changed}));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    service_e2b_call(
        &runtime,
        "e2b-noop",
        &task,
        VecDeque::from([MutationFixtureReply::Noop]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "known_result"
    );
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["state_changed"],
        false
    );
    assert!(
        !runtime
            .sessions
            .summary(&session_id, None)
            .unwrap()
            .repository_edit_observed
    );
    assert!(result.output.get("state_changed").is_none());
}

#[tokio::test]
async fn e2b_dry_run_is_known_false_and_never_creates_edit_provenance() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-dry-run", "before\n").await;
    let source = r#"
        const before = await tools.read_files({items:[{path:"src/example.rs"}]});
        const revision = before.output.items[0].output.read_revision;
        const edit = await tools.apply_text_edits({
            dry_run:true,
            changes:[{
                kind:"edit", path:"src/example.rs", expected_read_revision:revision,
                edits:[{kind:"replace_exact", old_text:"before", new_text:"after"}]
            }]
        });
        text(JSON.stringify({success:edit.success, changed:edit.output.state_changed, would_change:edit.output.would_change}));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    service_e2b_call(
        &runtime,
        "e2b-dry-run",
        &task,
        VecDeque::from([MutationFixtureReply::ApplyExact]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let emitted = emitted_json(&result);
    assert_eq!(emitted["success"], true);
    assert_eq!(emitted["changed"], false);
    assert_eq!(emitted["would_change"], true);
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "known_result"
    );
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["state_changed"],
        false
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "before\n"
    );
    assert!(
        !runtime
            .sessions
            .summary(&session_id, None)
            .unwrap()
            .repository_edit_observed
    );
}

#[tokio::test]
async fn e2b_stale_revision_preserves_newer_workspace_and_recovery() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-stale", "old\n").await;
    let source = r#"
        const before = await tools.read_files({items:[{path:"src/example.rs"}]});
        const revision = before.output.items[0].output.read_revision;
        const edit = await tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs", expected_read_revision:revision,
            edits:[{kind:"replace_exact", old_text:"old", new_text:"bad"}]
        }]});
        text(JSON.stringify({
            success:edit.success,
            state_changed:edit.output.state_changed,
            error_kind:edit.output.error_kind,
            recovery:edit.output.recovery ?? null
        }));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    service_e2b_call(
        &runtime,
        "e2b-stale",
        &task,
        VecDeque::from([MutationFixtureReply::ShaConflict {
            replacement: "newer\n".to_string(),
        }]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "newer\n"
    );
    let emitted = emitted_json(&result);
    assert_eq!(emitted["success"], false);
    assert_eq!(emitted["state_changed"], false);
    assert_eq!(emitted["error_kind"], "stale_file_revision");
    assert!(emitted["recovery"].is_object(), "{emitted}");
    assert!(
        !runtime
            .sessions
            .summary(&session_id, None)
            .unwrap()
            .repository_edit_observed
    );
}

#[tokio::test]
async fn e2b_ambiguous_exact_match_fails_closed_and_consumes_mutation_attempt() {
    let (root, runtime, project, session_id) =
        e2b_fixture("e2b-ambiguous", "dup\nother\ndup\n").await;
    let source = r#"
        const first = await tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs",
            edits:[{kind:"replace_exact", old_text:"dup", new_text:"one"}]
        }]});
        let second_error = null;
        try {
            await tools.apply_text_edits({changes:[{
                kind:"edit", path:"src/example.rs",
                edits:[{kind:"replace_exact", old_text:"other", new_text:"two"}]
            }]});
        } catch (error) { second_error = String(error); }
        text(JSON.stringify({first_success:first.success, first_kind:first.output.error_kind, second_error}));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    let count = service_e2b_call(
        &runtime,
        "e2b-ambiguous",
        &task,
        VecDeque::from([MutationFixtureReply::MultipleMatches]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(
        count, 1,
        "second mutation must be rejected before Runner dispatch"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "dup\nother\ndup\n"
    );
    let emitted = emitted_json(&result);
    assert_eq!(emitted["first_success"], false);
    assert_eq!(emitted["first_kind"], "multiple_matches");
    assert!(emitted["second_error"]
        .as_str()
        .unwrap()
        .contains("at most 1 mutation attempt"));
}

#[tokio::test]
async fn e2b_js_failure_after_successful_write_keeps_known_true_receipt() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-after-edit", "before\n").await;
    let source = r#"
        const before = await tools.read_files({items:[{path:"src/example.rs"}]});
        const revision = before.output.items[0].output.read_revision;
        await tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs", expected_read_revision:revision,
            edits:[{kind:"replace_exact", old_text:"before", new_text:"after"}]
        }]});
        throw new Error("E2B_AFTER_EDIT");
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    service_e2b_call(
        &runtime,
        "e2b-after-edit",
        &task,
        VecDeque::from([MutationFixtureReply::ApplyExact]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "after\n"
    );
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "known_result"
    );
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["state_changed"],
        true
    );
    let message = result.output["message"].as_str().unwrap();
    assert!(message.contains("Do not blindly rerun"), "{message}");
    assert!(!message.contains("retry_same"), "{message}");
}

#[tokio::test]
async fn e2b_promise_all_never_dispatches_two_mutations() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-two-edits", "one\n").await;
    let source = r#"
        await Promise.all([
            tools.apply_text_edits({changes:[{kind:"edit",path:"src/example.rs",edits:[{kind:"replace_exact",old_text:"one",new_text:"first"}]}]}),
            tools.apply_text_edits({changes:[{kind:"edit",path:"src/example.rs",edits:[{kind:"replace_exact",old_text:"one",new_text:"second"}]}]})
        ]);
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, None);
    let count = service_e2b_call(
        &runtime,
        "e2b-two-edits",
        &task,
        VecDeque::from([MutationFixtureReply::ApplyExact]),
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(count, 1);
    assert!(matches!(
        fs::read_to_string(root.path().join("src/example.rs"))
            .unwrap()
            .as_str(),
        "first\n" | "second\n"
    ));
    assert_eq!(result.output["effect_receipt"]["consequential_calls"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_timeout_after_mutation_dispatch_reconciles_known_true_result() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-timeout-known", "before\n").await;
    let source = r#"
        tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs",
            edits:[{kind:"replace_exact", old_text:"before", new_text:"after"}]
        }]});
        while (true) {}
    "#;
    // Keep the frontend deadline above host scheduling jitter so this test
    // exercises timeout only after the mutation has entered canonical dispatch.
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, Some(1000));
    let request = wait_for_patch_agent_request(&runtime, "e2b-timeout-known").await;
    assert_eq!(request.kind, "file_apply_text_edits");
    tokio::time::sleep(Duration::from_millis(100)).await;
    complete_mutation_fixture(
        &runtime,
        "e2b-timeout-known",
        request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    assert!(result.output["recovery"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("reduce_or_bound_code_mode_work")));
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "known_result"
    );
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["state_changed"],
        true
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "after\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_mutation_stall_beyond_bounded_drain_returns_outcome_unknown() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-timeout-unknown", "before\n").await;
    let source = r#"
        tools.apply_text_edits({changes:[{
            kind:"edit", path:"src/example.rs",
            edits:[{kind:"replace_exact", old_text:"before", new_text:"after"}]
        }]});
        while (true) {}
    "#;
    // Keep the frontend deadline above host scheduling jitter so the child
    // reaches canonical dispatch before bounded drain is exercised.
    let task = spawn_e2b_call(&runtime, &project, &session_id, source, Some(1000));
    let request = wait_for_patch_agent_request(&runtime, "e2b-timeout-unknown").await;
    assert_eq!(request.kind, "file_apply_text_edits");
    let result = tokio::time::timeout(Duration::from_secs(8), task)
        .await
        .expect("E2b must return after the bounded five-second reconciliation")
        .unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(result.output["effect_receipt"]["outcome_unknown"], 1);
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    let recovery_actions = result.output["recovery"]["actions"].as_array().unwrap();
    assert!(recovery_actions.contains(&json!("reduce_or_bound_code_mode_work")));
    assert!(recovery_actions.contains(&json!("reconcile_effect_state_before_retry")));
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "outcome_unknown"
    );
    assert!(result.output["effect_receipt"]["children"][0]
        .get("state_changed")
        .is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "before\n"
    );
    runtime
        .runner_registry
        .cancel_request(&request.request_id)
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_same_project_mutation_fence_serializes_independent_hosts() {
    let (root, runtime, project, first_session_id) = e2b_fixture("e2b-fence-same", "one\n").await;
    let second_session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("second E2b host".to_string()));
    let first_host = Arc::new(CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&bootstrap_auth_context()),
        project.clone(),
        first_session_id,
        ToolTransport::Mcp,
        Some("e2b-fence-first".to_string()),
        E2B_MUTATION_ONLY_POLICY,
    ));
    let second_host = Arc::new(CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&bootstrap_auth_context()),
        project.clone(),
        second_session.session_id,
        ToolTransport::Mcp,
        Some("e2b-fence-second".to_string()),
        E2B_MUTATION_ONLY_POLICY,
    ));

    let first = tokio::spawn({
        let host = Arc::clone(&first_host);
        async move {
            host.invoke_tool(
                1,
                "apply_text_edits".to_string(),
                json!({"changes":[{"kind":"edit","path":"src/example.rs","edits":[{"kind":"replace_exact","old_text":"one","new_text":"first"}]}]}),
            )
            .await
        }
    });
    runtime
        .orchestration_mutation_fences
        .wait_for_acquire_attempt_for_test()
        .await;
    runtime
        .orchestration_mutation_fences
        .wait_for_acquired_for_test()
        .await;
    let first_request = wait_for_patch_agent_request(&runtime, "e2b-fence-same").await;
    assert_eq!(first_request.kind, "file_apply_text_edits");

    let second = tokio::spawn({
        let host = Arc::clone(&second_host);
        async move {
            host.invoke_tool(
                1,
                "apply_text_edits".to_string(),
                json!({"changes":[{"kind":"edit","path":"src/example.rs","edits":[{"kind":"replace_exact","old_text":"first","new_text":"second"}]}]}),
            )
            .await
        }
    });
    runtime
        .orchestration_mutation_fences
        .wait_for_acquire_attempt_for_test()
        .await;
    assert!(
        probe_patch_agent_request(&runtime, "e2b-fence-same")
            .await
            .is_none(),
        "second same-Project mutation crossed canonical dispatch while first held the fence"
    );

    complete_mutation_fixture(
        &runtime,
        "e2b-fence-same",
        first_request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    assert!(first.await.unwrap().unwrap().success);
    runtime
        .orchestration_mutation_fences
        .wait_for_acquired_for_test()
        .await;
    let second_request = wait_for_patch_agent_request(&runtime, "e2b-fence-same").await;
    complete_mutation_fixture(
        &runtime,
        "e2b-fence-same",
        second_request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    assert!(second.await.unwrap().unwrap().success);
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "second\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_mutation_fence_is_project_scoped_not_process_global() {
    let (_root_one, runtime, project_one, _session_one) =
        e2b_fixture("e2b-fence-p1", "one\n").await;
    let root_two = tempfile::tempdir().unwrap();
    fs::create_dir_all(root_two.path().join("src")).unwrap();
    fs::write(root_two.path().join("src/example.rs"), "two\n").unwrap();
    let project_two = register_runner_project_at_path_with_capabilities(
        &runtime,
        "e2b-fence-p2",
        "demo-two",
        root_two.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: true,
            ..Default::default()
        },
    )
    .await;
    let session_two = runtime
        .sessions
        .start_session(Some(project_two.clone()), Some("P2 E2b host".to_string()));
    let project_one_guard = runtime
        .orchestration_mutation_fences
        .hold_project_for_test(&project_one)
        .await;
    let host_two = Arc::new(CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&bootstrap_auth_context()),
        project_two,
        session_two.session_id,
        ToolTransport::Mcp,
        Some("e2b-fence-p2-parent".to_string()),
        E2B_MUTATION_ONLY_POLICY,
    ));
    let second_project = tokio::spawn({
        let host = Arc::clone(&host_two);
        async move {
            host.invoke_tool(
                1,
                "apply_text_edits".to_string(),
                json!({"changes":[{"kind":"edit","path":"src/example.rs","edits":[{"kind":"replace_exact","old_text":"two","new_text":"changed"}]}]}),
            )
            .await
        }
    });
    runtime
        .orchestration_mutation_fences
        .wait_for_acquire_attempt_for_test()
        .await;
    runtime
        .orchestration_mutation_fences
        .wait_for_acquired_for_test()
        .await;
    let request = wait_for_patch_agent_request(&runtime, "e2b-fence-p2").await;
    complete_mutation_fixture(
        &runtime,
        "e2b-fence-p2",
        request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    assert!(second_project.await.unwrap().unwrap().success);
    drop(project_one_guard);
    assert_eq!(
        fs::read_to_string(root_two.path().join("src/example.rs")).unwrap(),
        "changed\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_orchestration_does_not_acquire_project_mutation_fence() {
    let (_root, runtime, project, session_id) = e2b_fixture("e2b-fence-read", "readable\n").await;
    let project_guard = runtime
        .orchestration_mutation_fences
        .hold_project_for_test(&project)
        .await;
    let host = Arc::new(CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&bootstrap_auth_context()),
        project,
        session_id,
        ToolTransport::Mcp,
        Some("e2b-read-with-write-fence-held".to_string()),
        READ_ONLY_TEST_POLICY,
    ));
    let read = tokio::spawn({
        let host = Arc::clone(&host);
        async move {
            host.invoke_tool(
                1,
                "read_files".to_string(),
                json!({"items":[{"path":"src/example.rs"}]}),
            )
            .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "e2b-fence-read").await;
    assert_eq!(request.kind, "file_read");
    complete_agent_request_by_running_locally(&runtime, "e2b-fence-read", request).await;
    let response = tokio::time::timeout(Duration::from_secs(2), read)
        .await
        .expect("read-only orchestration must not wait on the mutation fence")
        .unwrap()
        .unwrap();
    assert!(response.success);
    drop(project_guard);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_validation_does_not_acquire_project_mutation_fence() {
    let root = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_validation_sync_wait(Duration::from_millis(20));
    let client_id = "e2b-fence-e2a";
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client_id,
        "demo",
        root.path(),
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
    )
    .await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("E2a fence isolation".to_string()),
    );
    let project_guard = runtime
        .orchestration_mutation_fences
        .hold_project_for_test(&project)
        .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::CodeModeExecEffectful {
                        project,
                        session_id,
                        source: "const r=await tools.cargo_check({timeout_secs:600}); text(JSON.stringify({job:r.output?.job_id??null}));".to_string(),
                        timeout_ms: Some(5_000),
                    },
                    Some(&bootstrap_auth_context()),
                )
                .await
        }
    });
    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "Checking fence isolation\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("E2a validation must not wait on the mutation fence")
        .unwrap();
    assert!(result.success, "{result:?}");
    drop(project_guard);
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished fence isolation\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_nested_edit_drives_real_final_changes_baseline_to_full_final_workspace() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "base readme\n");
    commit_file(root.path(), "other.txt", "base other\n");
    let baseline = git(root.path(), &["rev-parse", "HEAD^{tree}"]);
    let runtime = test_runtime();
    let auth = bootstrap_auth_context();
    let client_id = "e2b-final-changes";
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client_id,
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: true,
            ..Default::default()
        },
    )
    .await;
    let session = start_coding_session_with_baseline(
        &runtime,
        &auth,
        &project,
        baseline,
        "E2b Final Changes integration",
    );

    // This non-Code-Mode workspace change proves Final Changes freezes the
    // complete baseline→final workspace, not merely the nested edit provenance.
    fs::write(root.path().join("other.txt"), "outside code mode\n").unwrap();
    let source = r#"
        const before=await tools.read_files({items:[{path:"README.md"}]});
        const revision=before.output.items[0].output.read_revision;
        const edit=await tools.apply_text_edits({changes:[{
            kind:"edit",path:"README.md",expected_read_revision:revision,
            edits:[{kind:"replace_exact",old_text:"base readme",new_text:"E2b readme"}]
        }]});
        text(JSON.stringify({success:edit.success,changed:edit.output.state_changed}));
    "#;
    let e2b = spawn_e2b_call(&runtime, &project, &session.session_id, source, None);
    service_e2b_call(
        &runtime,
        client_id,
        &e2b,
        VecDeque::from([MutationFixtureReply::ApplyExact]),
    )
    .await;
    let e2b_result = e2b.await.unwrap();
    assert!(e2b_result.success, "{e2b_result:?}");
    assert_eq!(
        e2b_result.output["effect_receipt"]["children"][0]["state_changed"],
        true
    );
    let summary = runtime.sessions.summary(&session.session_id, None).unwrap();
    assert!(summary.repository_edit_observed);

    let finish = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: Some(false),
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    service_tool_task(&runtime, client_id, &finish).await;
    let finish = finish.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);
    assert_eq!(
        finish.output["presentation"]["suggested_call"]["tool"],
        "present_work_result"
    );

    let present = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .present_work_result(project, session_id, Some(&auth))
                .await
        }
    });
    service_tool_task(&runtime, client_id, &present).await;
    let present = present.await.unwrap();
    assert!(present.success, "{:?}", present.error);
    let files = present.output["work_result"]["final_changes"]["files"]
        .as_array()
        .expect("Final Changes files");
    for expected in ["README.md", "other.txt"] {
        assert!(
            files.iter().any(|file| file["path"] == expected),
            "baseline→final snapshot omitted {expected}: {}",
            present.output
        );
    }
    assert_eq!(
        present.output["work_result"]["final_changes"]["files_changed"],
        2
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_outer_only_noop_and_prestart_failure_do_not_create_final_changes_eligibility() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "base\n");
    let baseline = git(root.path(), &["rev-parse", "HEAD^{tree}"]);
    let runtime = test_runtime();
    let auth = bootstrap_auth_context();
    let client_id = "e2b-final-ineligible";
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        client_id,
        "demo",
        root.path(),
        RunnerCapabilities {
            shell: true,
            git: true,
            file_read: true,
            file_write: true,
            internal_posix_script: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: true,
            ..Default::default()
        },
    )
    .await;

    let outer_only = start_coding_session_with_baseline(
        &runtime,
        &auth,
        &project,
        baseline.clone(),
        "outer-only E2b",
    );
    let read = spawn_e2b_call(
        &runtime,
        &project,
        &outer_only.session_id,
        "const r=await tools.read_files({items:[{path:'README.md'}]}); text(String(r.success));",
        None,
    );
    service_e2b_call(&runtime, client_id, &read, VecDeque::new()).await;
    assert!(read.await.unwrap().success);
    let summary = runtime
        .sessions
        .summary(&outer_only.session_id, None)
        .unwrap();
    assert!(!summary.repository_edit_observed);
    assert!(!runtime
        .final_changes_presentation_needed(&project, &summary)
        .await
        .unwrap());

    let noop = start_coding_session_with_baseline(
        &runtime,
        &auth,
        &project,
        baseline.clone(),
        "no-op E2b",
    );
    let noop_task = spawn_e2b_call(
        &runtime,
        &project,
        &noop.session_id,
        "const r=await tools.apply_text_edits({changes:[{kind:'edit',path:'README.md',edits:[{kind:'replace_exact',old_text:'base',new_text:'base'}]}]}); text(String(r.output.state_changed));",
        None,
    );
    service_e2b_call(
        &runtime,
        client_id,
        &noop_task,
        VecDeque::from([MutationFixtureReply::Noop]),
    )
    .await;
    assert!(noop_task.await.unwrap().success);
    let summary = runtime.sessions.summary(&noop.session_id, None).unwrap();
    assert!(!summary.repository_edit_observed);
    assert!(!runtime
        .final_changes_presentation_needed(&project, &summary)
        .await
        .unwrap());

    let prestart = start_coding_session_with_baseline(
        &runtime,
        &auth,
        &project,
        baseline,
        "prestart failure E2b",
    );
    let prestart_result = spawn_e2b_call(
        &runtime,
        &project,
        &prestart.session_id,
        "await tools.apply_text_edits({changes:[]});",
        None,
    )
    .await
    .unwrap();
    assert!(prestart_result.success, "{prestart_result:?}");
    assert!(prestart_result.output.get("effect_receipt").is_none());
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    let summary = runtime
        .sessions
        .summary(&prestart.session_id, None)
        .unwrap();
    assert!(!summary.repository_edit_observed);
    assert!(!runtime
        .final_changes_presentation_needed(&project, &summary)
        .await
        .unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn direct_apply_text_edits_does_not_acquire_experimental_orchestration_fence() {
    let (root, runtime, project, session_id) = e2b_fixture("e2b-direct-unlocked", "before\n").await;
    let project_guard = runtime
        .orchestration_mutation_fences
        .hold_project_for_test(&project)
        .await;
    let direct = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ApplyTextEdits {
                        project,
                        changes: vec![edit_change(
                            "src/example.rs",
                            &"a".repeat(64),
                            vec![text_edit(
                                crate::tool_runtime::ApplyTextEditKind::ReplaceExact,
                                Some("before"),
                                Some("direct"),
                                None,
                            )],
                        )],
                        dry_run: None,
                        session_id: Some(session_id),
                    },
                    Some(&bootstrap_auth_context()),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "e2b-direct-unlocked").await;
    assert_eq!(request.kind, "file_apply_text_edits");
    complete_mutation_fixture(
        &runtime,
        "e2b-direct-unlocked",
        request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    let result = tokio::time::timeout(Duration::from_secs(2), direct)
        .await
        .expect("direct apply_text_edits must ignore the E2b orchestration fence")
        .unwrap();
    assert!(result.success, "{result:?}");
    drop(project_guard);
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "direct\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e1_and_e2a_remain_unable_to_dispatch_apply_text_edits_after_e2b() {
    let (_root, runtime, project, _session_id) =
        e2b_fixture("e2b-stage-isolation", "before\n").await;
    for tool_name in ["code_mode_exec", "code_mode_exec_effectful"] {
        let session = runtime.sessions.start_session(
            Some(project.clone()),
            Some(format!("isolation {tool_name}")),
        );
        let result = runtime
            .dispatch_with_auth(
                match tool_name {
                    "code_mode_exec" => ToolCall::CodeModeExec {
                        project: project.clone(),
                        session_id: session.session_id,
                        source: "try { await tools.apply_text_edits({changes:[{kind:'create',path:'forbidden.txt',content:'x'}]}); } catch (e) { text(String(e)); }".to_string(),
                        timeout_ms: Some(2_000),
                    },
                    _ => ToolCall::CodeModeExecEffectful {
                        project: project.clone(),
                        session_id: session.session_id,
                        source: "try { await tools.apply_text_edits({changes:[{kind:'create',path:'forbidden.txt',content:'x'}]}); } catch (e) { text(String(e)); }".to_string(),
                        timeout_ms: Some(2_000),
                    },
                },
                Some(&bootstrap_auth_context()),
            )
            .await;
        assert!(result.success, "{tool_name}: {result:?}");
        assert!(result.output["content"][0]
            .as_str()
            .unwrap()
            .contains("is not a function"));
        assert!(probe_patch_agent_request(&runtime, "e2b-stage-isolation")
            .await
            .is_none());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2b_parent_omits_retired_continuity_overlays_after_nested_edit() {
    use crate::tool_runtime::kernel::{ToolInvocationMetadata, ToolProtocolCapabilities};

    let (root, runtime, project, session_id) =
        e2b_fixture("e2b-session-continuity", "before\n").await;
    let runtime_for_call = runtime.clone();
    let project_for_call = project.clone();
    let session_for_call = session_id.clone();
    let task = tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        runtime_for_call
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "code_mode_exec_mutating".to_string(),
                    arguments: json!({
                        "project": project_for_call,
                        "session_id": session_for_call,
                        "source": "const e=await tools.apply_text_edits({changes:[{kind:'edit',path:'src/example.rs',edits:[{kind:'replace_exact',old_text:'before',new_text:'after'}]}]}); text(String(e.output.state_changed));"
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session_for_call),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                ToolInvocationMetadata::default(),
                ToolProtocolCapabilities {

                    ..Default::default()
                },
            )
            .await
    });
    let request = wait_for_patch_agent_request(&runtime, "e2b-session-continuity").await;
    complete_mutation_fixture(
        &runtime,
        "e2b-session-continuity",
        request,
        MutationFixtureReply::ApplyExact,
    )
    .await;
    let outcome = task.await.unwrap();
    assert!(outcome.success, "{outcome:?}");
    let result = outcome.result.expect("E2b continuity ToolResult");
    assert!(result.success, "{result:?}");
    assert!(result.output.get("session_context_revision").is_none());
    assert!(result.output.get("session_continuity").is_none());
    assert_eq!(
        fs::read_to_string(root.path().join("src/example.rs")).unwrap(),
        "after\n"
    );
}

#[tokio::test]
async fn e2b_denies_shell_other_mutation_nested_jobs_and_recursion_before_business_dispatch() {
    let (_root, runtime, project, session_id) = e2b_fixture("e2b-denials", "x\n").await;
    for tool in [
        "run_shell",
        "run_process",
        "observe_jobs",
        "wait_for_job_terminal",
        "apply_patch",
        "write_file",
        "git_commit",
        "plugin_tool",
        "code_mode_exec",
        "code_mode_exec_effectful",
        "code_mode_exec_mutating",
    ] {
        let source =
            format!("try {{ await tools.{tool}({{}}); }} catch (error) {{ text(String(error)); }}");
        let task = spawn_e2b_call(&runtime, &project, &session_id, &source, None);
        let count = service_e2b_call(&runtime, "e2b-denials", &task, VecDeque::new()).await;
        let result = task.await.unwrap();
        assert!(result.success, "{tool}: {:?}", result.error);
        assert_eq!(count, 0, "{tool}");
        assert!(
            result.output["content"][0]
                .as_str()
                .unwrap()
                .contains("is not a function"),
            "{tool}: {}",
            result.output
        );
    }
}

#[tokio::test]
async fn e2b_missing_outer_write_scope_rejects_before_nested_dispatch() {
    let (_root, runtime, project, session_id) = e2b_fixture("oauth-client", "x\n").await;
    let auth = oauth_bridge_auth_context(
        "scope-hash",
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_SESSION_COLLABORATE,
        ],
    );
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "code_mode_exec_mutating".to_string(),
                arguments: json!({
                    "project": project,
                    "session_id": session_id,
                    "source": "await tools.apply_text_edits({changes:[{kind:'create',path:'x.txt',content:'x'}]})"
                }),
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
    assert!(outcome.result.is_none());
    assert!(matches!(
        outcome.error_status,
        Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InsufficientScope { .. })
    ));
    assert!(probe_patch_agent_request(&runtime, "oauth-client")
        .await
        .is_none());
}

#[tokio::test]
async fn e2b_child_permission_denial_is_canonical_and_non_effectful() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("example.rs"), "x\n").unwrap();
    let runtime = test_runtime()
        .with_permission_evaluator(PermissionEvaluator::with_mode(AuthorityMode::Restricted));
    let project =
        register_runner_project_at_path(&runtime, "e2b-child-permission", "demo", root.path())
            .await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let host = CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&bootstrap_auth_context()),
        project,
        session.session_id.clone(),
        ToolTransport::Mcp,
        Some("e2b-child-permission-parent".to_string()),
        E2B_MUTATION_ONLY_POLICY,
    );
    let nested = host
        .invoke_tool(
            1,
            "apply_text_edits".to_string(),
            json!({"changes":[{"kind":"create","path":"blocked.txt","content":"x"}]}),
        )
        .await
        .expect("permission denial is a canonical child ToolResult");
    assert!(!nested.success);
    assert_eq!(nested.output["error_kind"], "permission_denied");
    assert!(!root.path().join("blocked.txt").exists());
    assert!(probe_patch_agent_request(&runtime, "e2b-child-permission")
        .await
        .is_none());
    assert_eq!(host.effect_receipt().consequential_calls, 0);
}

#[tokio::test]
async fn e2b_runner_capability_failure_never_claims_mutation() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("example.rs"), "dup\ndup\n").unwrap();
    let runtime = test_runtime();
    let project = register_runner_project_at_path_with_capabilities(
        &runtime,
        "e2b-missing-line-scope-cap",
        "demo",
        root.path(),
        RunnerCapabilities {
            file_read: true,
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: false,
            ..Default::default()
        },
    )
    .await;
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let source = r#"
        const before=await tools.read_files({items:[{path:"example.rs"}]});
        const revision=before.output.items[0].output.read_revision;
        const r=await tools.apply_text_edits({changes:[{
            kind:"edit",path:"example.rs",expected_read_revision:revision,
            edits:[{kind:"replace_exact",old_text:"dup",new_text:"x",line_scope:{start_line:1,end_line:1}}]
        }]});
        text(JSON.stringify({success:r.success, kind:r.output.failure_kind, changed:r.output.state_changed}));
    "#;
    let task = spawn_e2b_call(&runtime, &project, &session.session_id, source, None);
    let count = service_e2b_call(
        &runtime,
        "e2b-missing-line-scope-cap",
        &task,
        VecDeque::new(),
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(count, 0);
    let emitted = emitted_json(&result);
    assert_eq!(emitted["success"], false);
    assert_eq!(emitted["kind"], "capability_unavailable");
    assert_eq!(emitted["changed"], false);
    assert_eq!(
        fs::read_to_string(root.path().join("example.rs")).unwrap(),
        "dup\ndup\n"
    );
    assert!(result.output.get("effect_receipt").is_none());
}
