//! Checkpoint tests for tool_runtime.

use super::super::types::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn checkpoint_create_call(
    project: String,
    title: Option<&str>,
    note: Option<&str>,
    include_untracked: Option<bool>,
) -> ToolCall {
    checkpoint_create_call_with_session(project, title, note, include_untracked, None)
}

fn checkpoint_create_call_with_session(
    project: String,
    title: Option<&str>,
    note: Option<&str>,
    include_untracked: Option<bool>,
    session_id: Option<String>,
) -> ToolCall {
    checkpoint_create_call_with_metadata(
        project,
        title,
        note,
        include_untracked,
        None,
        &[],
        None,
        session_id,
    )
}

fn checkpoint_create_call_with_metadata(
    project: String,
    title: Option<&str>,
    note: Option<&str>,
    include_untracked: Option<bool>,
    kind: Option<&str>,
    labels: &[&str],
    validation: Option<CheckpointValidationInput>,
    session_id: Option<String>,
) -> ToolCall {
    ToolCall::WorkspaceCheckpointCreate {
        project,
        title: title.map(str::to_string),
        note: note.map(str::to_string),
        include_untracked,
        kind: kind.map(str::to_string),
        labels: labels.iter().map(|label| (*label).to_string()).collect(),
        validation,
        session_id,
    }
}

fn checkpoint_validation(
    status: Option<&str>,
    commands: &[&str],
    summary: Option<&str>,
) -> CheckpointValidationInput {
    CheckpointValidationInput {
        status: status.map(str::to_string),
        commands: commands
            .iter()
            .map(|command| (*command).to_string())
            .collect(),
        summary: summary.map(str::to_string),
    }
}

fn checkpoint_json_count(path: &Path) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                checkpoint_json_count(&path)
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                1
            } else {
                0
            }
        })
        .sum()
}

fn assert_invalid_checkpoint_metadata(result: &ToolResult, expected: &str) {
    assert!(!result.success, "{:?}", result.output);
    assert_eq!(result.output["error_kind"], "invalid_checkpoint_metadata");
    let error = result.error.as_deref().unwrap_or_default();
    assert!(
        error.contains(expected),
        "expected error to contain {expected:?}, got {error:?}"
    );
}

#[tokio::test]
async fn checkpoint_create_lists_and_shows_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-create", "agent-proj", root).await;

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-create",
        checkpoint_create_call(
            project.clone(),
            Some("before refactor"),
            Some("last known good"),
            Some(false),
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(checkpoint_id.starts_with("wc_ckpt_"));
    assert_eq!(created.output["project"], project);
    assert_eq!(created.output["title"], "before refactor");
    assert!(created.output["tracked_diff_bytes"].as_u64().unwrap() > 0);
    assert!(created.output.get("tracked_diff").is_none());

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointList {
                project: project.clone(),
                limit: Some(20),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    let checkpoints = listed.output["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0]["checkpoint_id"], checkpoint_id);
    assert_eq!(checkpoints[0]["title"], "before refactor");
    assert!(checkpoints[0].get("tracked_diff").is_none());

    let shown = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointShow {
                project: project.clone(),
                checkpoint_id,
                include_diff_stat: Some(true),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(shown.success, "{:?}", shown.error);
    assert!(shown.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "a.txt" && file["kind"] == "tracked"));
    assert!(shown.output["diff_stat"]["tracked"]
        .as_str()
        .unwrap()
        .contains("a.txt"));
    assert!(shown.output.get("tracked_diff").is_none());
    assert!(shown.output.get("staged_diff").is_none());
}

#[tokio::test]
async fn checkpoint_create_records_kind_labels_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-metadata", "agent-proj", root).await;
    let validation = checkpoint_validation(
        Some("passed"),
        &["cargo fmt --check", "cargo test checkpoint"],
        Some("checkpoint validation passed"),
    );

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-metadata",
        checkpoint_create_call_with_metadata(
            project.clone(),
            Some("last known good"),
            Some("tests passed"),
            Some(false),
            Some("last_known_good"),
            &["policy-layer", "tests-passed"],
            Some(validation),
            None,
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    assert_eq!(created.output["kind"], "last_known_good");
    assert_eq!(
        created.output["labels"],
        json!(["policy-layer", "tests-passed"])
    );
    assert_eq!(created.output["validation"]["status"], "passed");
    assert_eq!(
        created.output["validation"]["commands"],
        json!(["cargo fmt --check", "cargo test checkpoint"])
    );
    assert_eq!(
        created.output["validation"]["summary"],
        "checkpoint validation passed"
    );

    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let shown = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointShow {
                project,
                checkpoint_id,
                include_diff_stat: Some(false),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(shown.success, "{:?}", shown.error);
    assert_eq!(shown.output["kind"], "last_known_good");
    assert_eq!(
        shown.output["labels"],
        json!(["policy-layer", "tests-passed"])
    );
    assert_eq!(
        shown.output["validation"],
        json!({
            "status": "passed",
            "commands": ["cargo fmt --check", "cargo test checkpoint"],
            "summary": "checkpoint validation passed",
        })
    );
}

#[tokio::test]
async fn checkpoint_list_includes_kind_labels_validation_status() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-list", "agent-proj", root).await;

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-list",
        checkpoint_create_call_with_metadata(
            project.clone(),
            Some("checked"),
            None,
            Some(false),
            Some("last_known_good"),
            &["policy-layer", "tests-passed"],
            Some(checkpoint_validation(
                Some("passed"),
                &["cargo fmt --check", "cargo test checkpoint"],
                None,
            )),
            None,
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let checkpoint_id = created.output["checkpoint_id"].as_str().unwrap();

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointList {
                project,
                limit: Some(20),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    let checkpoints = listed.output["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 1);
    let item = &checkpoints[0];
    assert_eq!(item["checkpoint_id"], checkpoint_id);
    assert_eq!(item["kind"], "last_known_good");
    assert_eq!(item["labels"], json!(["policy-layer", "tests-passed"]));
    assert_eq!(item["validation_status"], "passed");
    assert!(item.get("validation").is_none(), "{item:?}");
}

#[tokio::test]
async fn checkpoint_defaults_metadata_for_old_or_minimal_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-defaults", "agent-proj", root).await;

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-defaults",
        checkpoint_create_call(project.clone(), Some("minimal"), None, Some(false)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    assert_eq!(created.output["kind"], "snapshot");
    assert_eq!(created.output["labels"], json!([]));
    assert_eq!(created.output["validation"]["status"], "unknown");
    assert_eq!(created.output["validation"]["commands"], json!([]));
    assert!(created.output["validation"]["summary"].is_null());

    let minimal_checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let shown = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointShow {
                project: project.clone(),
                checkpoint_id: minimal_checkpoint_id,
                include_diff_stat: Some(false),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(shown.success, "{:?}", shown.error);
    assert_eq!(shown.output["kind"], "snapshot");
    assert_eq!(shown.output["labels"], json!([]));
    assert_eq!(shown.output["validation"]["status"], "unknown");

    let storage_path = PathBuf::from(created.output["storage_path"].as_str().unwrap());
    let storage_dir = storage_path.parent().unwrap();
    let mut legacy: Value =
        serde_json::from_str(&fs::read_to_string(&storage_path).unwrap()).unwrap();
    let legacy_id = "wc_ckpt_legacy_missing_metadata";
    legacy["checkpoint_id"] = json!(legacy_id);
    legacy["title"] = json!("legacy checkpoint");
    legacy["created_at"] = json!(created.output["created_at"].as_i64().unwrap_or_default() + 1);
    let legacy_obj = legacy.as_object_mut().unwrap();
    legacy_obj.remove("kind");
    legacy_obj.remove("labels");
    legacy_obj.remove("validation");
    fs::write(
        storage_dir.join(format!("{legacy_id}.json")),
        serde_json::to_vec_pretty(&legacy).unwrap(),
    )
    .unwrap();

    let legacy_shown = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointShow {
                project: project.clone(),
                checkpoint_id: legacy_id.to_string(),
                include_diff_stat: Some(false),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(legacy_shown.success, "{:?}", legacy_shown.error);
    assert_eq!(legacy_shown.output["kind"], "snapshot");
    assert_eq!(legacy_shown.output["labels"], json!([]));
    assert_eq!(
        legacy_shown.output["validation"],
        json!({"status": "unknown", "commands": [], "summary": Value::Null})
    );

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointList {
                project,
                limit: Some(20),
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    let legacy_item = listed.output["checkpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["checkpoint_id"] == legacy_id)
        .unwrap();
    assert_eq!(legacy_item["kind"], "snapshot");
    assert_eq!(legacy_item["labels"], json!([]));
    assert_eq!(legacy_item["validation_status"], "unknown");
}

#[tokio::test]
async fn checkpoint_rejects_invalid_kind_or_labels() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-invalid", "agent-proj", tmp.path()).await;
    let long_label = "a".repeat(65);
    let bootstrap = auth_context(None, true);

    let invalid_kind = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                Some("experimental"),
                &[],
                None,
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&invalid_kind, "kind must be one of");

    let invalid_label = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                None,
                &["bad label"],
                None,
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&invalid_label, "may only contain ASCII");

    let overlong_label = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project,
                None,
                None,
                None,
                None,
                &[long_label.as_str()],
                None,
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&overlong_label, "exceeds 64 characters");
    assert_eq!(checkpoint_json_count(state.path()), 0);
}

#[tokio::test]
async fn checkpoint_validation_metadata_is_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-bounds", "agent-proj", tmp.path()).await;
    let bootstrap = auth_context(None, true);
    let too_many_commands = (0..21).map(|idx| format!("echo {idx}")).collect::<Vec<_>>();
    let too_many_command_refs = too_many_commands
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let long_command = "x".repeat(201);
    let long_summary = "s".repeat(501);

    let too_many = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                None,
                &[],
                Some(checkpoint_validation(
                    Some("passed"),
                    &too_many_command_refs,
                    None,
                )),
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&too_many, "at most 20 entries");

    let command_too_long = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                None,
                &[],
                Some(checkpoint_validation(
                    Some("passed"),
                    &[long_command.as_str()],
                    None,
                )),
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&command_too_long, "exceeds 200 characters");

    let summary_too_long = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                None,
                &[],
                Some(checkpoint_validation(
                    Some("passed"),
                    &[],
                    Some(long_summary.as_str()),
                )),
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&summary_too_long, "exceeds 500 characters");

    let secret_command = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project.clone(),
                None,
                None,
                None,
                None,
                &[],
                Some(checkpoint_validation(
                    Some("passed"),
                    &["echo password=abc123"],
                    None,
                )),
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&secret_command, "contains secret-like text");

    let secret_summary = runtime
        .dispatch_with_auth(
            checkpoint_create_call_with_metadata(
                project,
                None,
                None,
                None,
                None,
                &[],
                Some(checkpoint_validation(
                    Some("passed"),
                    &[],
                    Some("client_secret=abc123"),
                )),
                None,
            ),
            Some(&bootstrap),
        )
        .await;
    assert_invalid_checkpoint_metadata(&secret_summary, "contains secret-like text");
    assert_eq!(checkpoint_json_count(state.path()), 0);
}

#[tokio::test]
async fn checkpoint_restore_tracked_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-restore", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-restore",
        checkpoint_create_call(project.clone(), Some("safe"), None, Some(false)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("restore checkpoint".to_string()),
        SessionMode::Normal,
        crate::tool_runtime::sessions::SessionGuards::default(),
    );

    fs::write(root.join("a.txt"), "polluted\n").unwrap();
    let restored = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-restore",
        ToolCall::WorkspaceCheckpointRestore {
            project,
            checkpoint_id: checkpoint_id.clone(),
            confirm: true,
            session_id: Some(session.session_id.clone()),
        },
    )
    .await;
    assert!(restored.success, "{:?}", restored.error);
    assert_eq!(restored.output["restored"], true);
    assert_eq!(restored.output["checkpoint_id"], checkpoint_id);
    assert_eq!(restored.output["session_recorded"], true);
    assert_eq!(
        fs::read_to_string(root.join("a.txt")).unwrap(),
        "checkpoint\n"
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "workspace_checkpoint_restore");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    assert!(event.write_like);
    assert!(!event.read_like);
}

#[tokio::test]
async fn checkpoint_restore_ignores_metadata_and_still_restores() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-restore-metadata", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-restore-metadata",
        checkpoint_create_call_with_metadata(
            project.clone(),
            Some("last known good"),
            None,
            Some(false),
            Some("last_known_good"),
            &["policy-layer", "tests-passed"],
            Some(checkpoint_validation(
                Some("passed"),
                &["cargo fmt --check", "cargo test checkpoint"],
                Some("validation passed"),
            )),
            None,
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();

    fs::write(root.join("a.txt"), "polluted\n").unwrap();
    let restored = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-restore-metadata",
        ToolCall::WorkspaceCheckpointRestore {
            project: project.clone(),
            checkpoint_id: checkpoint_id.clone(),
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(restored.success, "{:?}", restored.error);
    assert_eq!(restored.output["restored"], true);
    assert_eq!(
        fs::read_to_string(root.join("a.txt")).unwrap(),
        "checkpoint\n"
    );

    commit_file(root, "b.txt", "new head\n", "advance head");
    fs::write(root.join("a.txt"), "polluted again\n").unwrap();
    let head_mismatch = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-restore-metadata",
        ToolCall::WorkspaceCheckpointRestore {
            project,
            checkpoint_id,
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(!head_mismatch.success);
    assert_eq!(head_mismatch.output["error_kind"], "head_mismatch");
    assert_eq!(
        fs::read_to_string(root.join("a.txt")).unwrap(),
        "polluted again\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b.txt")).unwrap(),
        "new head\n"
    );
}

#[tokio::test]
async fn checkpoint_restore_requires_confirm() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    let project =
        register_agent_project_at_path(&runtime, "ckpt-confirm", "agent-proj", tmp.path()).await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointRestore {
                project,
                checkpoint_id: "wc_ckpt_missing".to_string(),
                confirm: false,
                session_id: None,
            },
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "confirm_required");
    assert_eq!(result.output["restored"], false);
}

#[tokio::test]
async fn checkpoint_restore_rejects_head_mismatch() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-head", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-head",
        checkpoint_create_call(project.clone(), None, None, Some(false)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    commit_file(root, "b.txt", "new head\n", "advance head");

    let restored = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-head",
        ToolCall::WorkspaceCheckpointRestore {
            project,
            checkpoint_id,
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(!restored.success);
    assert_eq!(restored.output["error_kind"], "head_mismatch");
    assert_eq!(
        fs::read_to_string(root.join("b.txt")).unwrap(),
        "new head\n"
    );
}

#[tokio::test]
async fn checkpoint_untracked_text_file_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("notes.txt"), "safe untracked\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-untracked", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-untracked",
        checkpoint_create_call(project.clone(), None, None, Some(true)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    assert!(created.output["untracked_files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["path"] == "notes.txt"));
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();

    fs::remove_file(root.join("notes.txt")).unwrap();
    let restored = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-untracked",
        ToolCall::WorkspaceCheckpointRestore {
            project,
            checkpoint_id,
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(restored.success, "{:?}", restored.error);
    assert_eq!(
        fs::read_to_string(root.join("notes.txt")).unwrap(),
        "safe untracked\n"
    );
}

#[tokio::test]
async fn checkpoint_skips_large_or_binary_untracked() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("big.txt"), vec![b'a'; 256 * 1024 + 1]).unwrap();
    fs::write(root.join("binary.bin"), b"abc\0def").unwrap();
    fs::write(root.join(".env.local"), "TOKEN=secret\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-skip", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-skip",
        checkpoint_create_call(project, None, None, Some(true)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let untracked = created.output["untracked_files"].as_array().unwrap();
    assert!(!untracked.iter().any(|file| file["path"] == "big.txt"));
    assert!(!untracked.iter().any(|file| file["path"] == "binary.bin"));
    assert!(!untracked.iter().any(|file| file["path"] == ".env.local"));
    let skipped = created.output["skipped_files"].as_array().unwrap();
    assert!(skipped
        .iter()
        .any(|file| file["path"] == "big.txt" && file["reason"] == "too_large"));
    assert!(skipped
        .iter()
        .any(|file| { file["path"] == "binary.bin" && file["reason"] == "binary_or_non_utf8" }));
    assert!(skipped.iter().any(|file| {
        file["path"] == ".env.local" && file["reason"] == "sensitive_or_invalid_path"
    }));
}

#[tokio::test]
async fn checkpoint_restore_rejects_malicious_untracked_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-malicious", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-malicious",
        checkpoint_create_call(project.clone(), None, None, Some(false)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let storage_path = PathBuf::from(created.output["storage_path"].as_str().unwrap());
    let storage_dir = storage_path.parent().unwrap();
    let mut checkpoint: Value =
        serde_json::from_str(&fs::read_to_string(&storage_path).unwrap()).unwrap();

    let traversal_id = "wc_ckpt_reject_traversal";
    checkpoint["checkpoint_id"] = json!(traversal_id);
    checkpoint["untracked_files"] = json!([{"path": "../escape.txt", "content": "escape\n"}]);
    fs::write(
        storage_dir.join(format!("{traversal_id}.json")),
        serde_json::to_vec_pretty(&checkpoint).unwrap(),
    )
    .unwrap();
    let traversal = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-malicious",
        ToolCall::WorkspaceCheckpointRestore {
            project: project.clone(),
            checkpoint_id: traversal_id.to_string(),
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(!traversal.success);
    assert_eq!(traversal.output["error_kind"], "invalid_checkpoint");
    assert!(!tmp.path().join("escape.txt").exists());

    let sensitive_id = "wc_ckpt_reject_sensitive";
    checkpoint["checkpoint_id"] = json!(sensitive_id);
    checkpoint["untracked_files"] =
        json!([{"path": "secrets/agent-token.txt", "content": "secret\n"}]);
    fs::write(
        storage_dir.join(format!("{sensitive_id}.json")),
        serde_json::to_vec_pretty(&checkpoint).unwrap(),
    )
    .unwrap();
    let sensitive = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-malicious",
        ToolCall::WorkspaceCheckpointRestore {
            project,
            checkpoint_id: sensitive_id.to_string(),
            confirm: true,
            session_id: None,
        },
    )
    .await;
    assert!(!sensitive.success);
    assert_eq!(sensitive.output["error_kind"], "invalid_checkpoint");
    assert!(!root.join("secrets/agent-token.txt").exists());
}

#[tokio::test]
async fn checkpoint_does_not_persist_inside_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-path", "agent-proj", root).await;
    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-path",
        checkpoint_create_call(project, None, None, Some(false)),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    assert_eq!(created.output["status_summary"]["clean"], true);
    assert_eq!(created.output["tracked_diff_bytes"], 0);
    let storage_path = PathBuf::from(created.output["storage_path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    let root = root.canonicalize().unwrap();
    assert!(!storage_path.starts_with(&root));
    assert!(storage_path.starts_with(runtime.checkpoint_state_dir()));
}

#[tokio::test]
async fn checkpoint_session_guards() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project = register_agent_project_at_path(&runtime, "ckpt-guard", "agent-proj", root).await;
    let bootstrap = auth_context(None, true);

    let read_only = runtime
        .dispatch_with_auth(
            ToolCall::StartSession {
                project: Some(project.clone()),
                title: Some("read only".to_string()),
                mode: SessionMode::ReadOnly,
                deny_write_tools: false,
                deny_shell_tools: false,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(read_only.success, "{:?}", read_only.error);
    let read_only_session = read_only.output["session_id"].as_str().unwrap().to_string();

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-guard",
        checkpoint_create_call_with_session(
            project.clone(),
            Some("safe"),
            None,
            Some(false),
            Some(read_only_session.clone()),
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    assert_eq!(created.output["session_recorded"], true);
    let checkpoint_id = created.output["checkpoint_id"]
        .as_str()
        .unwrap()
        .to_string();

    let listed = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointList {
                project: project.clone(),
                limit: Some(10),
                session_id: Some(read_only_session.clone()),
            },
            Some(&bootstrap),
        )
        .await;
    assert!(listed.success, "{:?}", listed.error);
    assert_eq!(listed.output["session_recorded"], true);

    let shown = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointShow {
                project: project.clone(),
                checkpoint_id: checkpoint_id.clone(),
                include_diff_stat: Some(false),
                session_id: Some(read_only_session.clone()),
            },
            Some(&bootstrap),
        )
        .await;
    assert!(shown.success, "{:?}", shown.error);
    assert_eq!(shown.output["session_recorded"], true);

    let restore_denied = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointRestore {
                project: project.clone(),
                checkpoint_id: checkpoint_id.clone(),
                confirm: true,
                session_id: Some(read_only_session.clone()),
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!restore_denied.success);
    assert_eq!(restore_denied.output["error_kind"], "session_guard_denied");
    assert_eq!(restore_denied.output["session_recorded"], true);

    let delete_denied = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointDelete {
                project: project.clone(),
                checkpoint_id: checkpoint_id.clone(),
                confirm: true,
                session_id: Some(read_only_session.clone()),
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!delete_denied.success);
    assert_eq!(delete_denied.output["error_kind"], "session_guard_denied");
    assert_eq!(delete_denied.output["session_recorded"], true);

    let summary = runtime
        .sessions
        .summary(&read_only_session, Some(20))
        .unwrap();
    let restore_event = finished_event(&summary, "workspace_checkpoint_restore");
    assert_eq!(restore_event.status.as_deref(), Some("failed"));
    assert!(restore_event.write_like);
}

#[tokio::test]
async fn checkpoint_session_input_summary_does_not_leak_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let root = tmp.path();
    init_git_repo(root);
    commit_file(root, "a.txt", "base\n", "base commit");
    fs::write(root.join("a.txt"), "checkpoint\n").unwrap();
    let runtime = test_runtime().with_checkpoint_state_dir(state.path());
    let project =
        register_agent_project_at_path(&runtime, "ckpt-summary", "agent-proj", root).await;
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("checkpoint metadata".to_string()),
        SessionMode::Normal,
        crate::tool_runtime::sessions::SessionGuards::default(),
    );

    let created = dispatch_checkpoint_with_local_agent(
        &runtime,
        "ckpt-summary",
        checkpoint_create_call_with_metadata(
            project,
            Some("last known good"),
            Some("tests passed"),
            Some(false),
            Some("last_known_good"),
            &["policy-layer", "tests-passed"],
            Some(checkpoint_validation(
                Some("passed"),
                &["cargo fmt --check", "cargo test checkpoint"],
                Some("validation passed"),
            )),
            Some(session.session_id.clone()),
        ),
    )
    .await;
    assert!(created.success, "{:?}", created.error);

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let started = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_started" && event.tool_name == "workspace_checkpoint_create"
        })
        .unwrap();
    let input_summary = started.input_summary.as_ref().unwrap();
    assert_eq!(input_summary["kind"], "last_known_good");
    assert_eq!(input_summary["label_count"], 2);
    assert_eq!(input_summary["validation_status"], "passed");
    assert!(
        input_summary.get("validation").is_none(),
        "{input_summary:?}"
    );
    assert!(input_summary.get("labels").is_none(), "{input_summary:?}");
    let serialized = serde_json::to_string(input_summary).unwrap();
    assert!(!serialized.contains("cargo fmt --check"), "{serialized}");
    assert!(
        !serialized.contains("cargo test checkpoint"),
        "{serialized}"
    );
    assert!(!serialized.contains("validation passed"), "{serialized}");
}

#[tokio::test]
async fn checkpoint_create_requires_agent_file_read_capability() {
    let runtime = runtime_with_agent_project("oe");
    let caps = ShellClientCapabilities::default();
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            checkpoint_create_call(agent_test_project_id("oe"), None, None, None),
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support file_read"), "{}", err);
}

#[tokio::test]
async fn checkpoint_restore_requires_agent_file_write_capability() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.file_read = true;
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointRestore {
                project: agent_test_project_id("oe"),
                checkpoint_id: "wc_ckpt_missing".to_string(),
                confirm: true,
                session_id: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support file_write"), "{}", err);
}

#[tokio::test]
async fn checkpoint_metadata_tools_enforce_agent_owner_boundary() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false;
    register_agent(&runtime, "oe", Some("alice"), caps).await;
    let bob = auth_context(Some("bob"), false);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointList {
                project: agent_test_project_id("oe"),
                limit: Some(5),
                session_id: None,
            },
            Some(&bob),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("owned by alice"), "{}", err);
    assert!(err.contains("belongs to bob"), "{}", err);
}
