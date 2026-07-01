//! Checkpoint tests for tool_runtime.

use super::super::cargo::*;
use super::super::codex::*;
use super::super::files::*;
use super::super::git::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[tokio::test]
async fn checkpoint_create_lists_and_shows_metadata() {
    if !python3_available() {
        eprintln!("skipping checkpoint_create_lists_and_shows_metadata: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project: project.clone(),
            title: Some("before refactor".to_string()),
            note: Some("last known good".to_string()),
            include_untracked: Some(false),
            session_id: None,
        },
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
async fn checkpoint_restore_tracked_changes() {
    if !python3_available() {
        eprintln!("skipping checkpoint_restore_tracked_changes: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project: project.clone(),
            title: Some("safe".to_string()),
            note: None,
            include_untracked: Some(false),
            session_id: None,
        },
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
    if !python3_available() {
        eprintln!("skipping checkpoint_restore_rejects_head_mismatch: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project: project.clone(),
            title: None,
            note: None,
            include_untracked: Some(false),
            session_id: None,
        },
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
    if !python3_available() {
        eprintln!("skipping checkpoint_untracked_text_file_roundtrip: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project: project.clone(),
            title: None,
            note: None,
            include_untracked: Some(true),
            session_id: None,
        },
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
    if !python3_available() {
        eprintln!("skipping checkpoint_skips_large_or_binary_untracked: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project,
            title: None,
            note: None,
            include_untracked: Some(true),
            session_id: None,
        },
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
async fn checkpoint_does_not_persist_inside_worktree() {
    if !python3_available() {
        eprintln!("skipping checkpoint_does_not_persist_inside_worktree: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project,
            title: None,
            note: None,
            include_untracked: Some(false),
            session_id: None,
        },
    )
    .await;
    assert!(created.success, "{:?}", created.error);
    let storage_path = PathBuf::from(created.output["storage_path"].as_str().unwrap())
        .canonicalize()
        .unwrap();
    let root = root.canonicalize().unwrap();
    assert!(!storage_path.starts_with(&root));
    assert!(storage_path.starts_with(runtime.checkpoint_state_dir()));
}

#[tokio::test]
async fn checkpoint_session_guards() {
    if !python3_available() {
        eprintln!("skipping checkpoint_session_guards: python3 unavailable");
        return;
    }
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
        ToolCall::WorkspaceCheckpointCreate {
            project: project.clone(),
            title: Some("safe".to_string()),
            note: None,
            include_untracked: Some(false),
            session_id: Some(read_only_session.clone()),
        },
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
async fn checkpoint_create_requires_agent_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false;
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::WorkspaceCheckpointCreate {
                project: agent_test_project_id("oe"),
                title: None,
                note: None,
                include_untracked: None,
                session_id: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support shell"), "{}", err);
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
