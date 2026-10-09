use super::*;
use crate::webcodex::settings::{self, RunnerPaths, SettingsUpdate};
use std::collections::VecDeque;
use std::future::ready;

fn fixture() -> (
    tempfile::TempDir,
    StoredRuntime,
    settings::PendingSettingsEdit,
    String,
) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("runner.toml");
    let original =
        "server_url='http://localhost:1234'\nclient_id='local'\n# preserved\n".to_string();
    std::fs::write(&path, &original).unwrap();
    let runtime = StoredRuntime {
        server_url: "http://localhost:1234".into(),
        runner_config: Some(path),
        runner_client_id: Some("local".into()),
        server_env_file: None,
        user_token_file: None,
        project_id: None,
        runtime_project_id: None,
    };
    let edit = settings::stage_paths_update(
        &runtime,
        SettingsUpdate {
            target: settings::target(&runtime).unwrap(),
            expected: RunnerPaths {
                instruction_files: vec![],
                skill_roots: vec![],
            },
            paths: RunnerPaths {
                instruction_files: vec![root
                    .path()
                    .join("instructions.md")
                    .to_string_lossy()
                    .into_owned()],
                skill_roots: vec![],
            },
        },
    )
    .unwrap();
    (root, runtime, edit, original)
}
fn check(generation: u64) -> Value {
    serde_json::json!({"success":true,"output":{"valid":true,"restart_required":false,"current_generation":generation}})
}
fn reload(generation: u64) -> Value {
    serde_json::json!({"success":true,"output":{"restart_required":false,"current_generation":generation}})
}

#[tokio::test]
async fn extension_paths_check_then_reload_once_with_the_observed_generation() {
    let (_root, _runtime, edit, _) = fixture();
    let mut responses = VecDeque::from([Ok(check(4)), Ok(reload(5))]);
    let mut calls = Vec::new();
    apply_with_control("local", &edit, |tool, params| {
        calls.push((tool, params));
        ready(responses.pop_front().unwrap())
    })
    .await
    .unwrap();
    assert!(edit.candidate_unchanged().unwrap());
    assert_eq!(
        calls.iter().map(|row| row.0).collect::<Vec<_>>(),
        ["check_runner_config", "reload_runner_config"]
    );
    assert_eq!(
        calls[1].1,
        serde_json::json!({"client_id":"local","expected_generation":4})
    );
}

#[tokio::test]
async fn offline_check_or_definite_rejection_restores_only_the_unchanged_candidate() {
    for responses in [
        vec![Err(desktop_state_unavailable("offline"))],
        vec![
            Ok(check(4)),
            Ok(serde_json::json!({"success":false,"output":{"execution_state":"not_started"}})),
        ],
        vec![Ok(
            serde_json::json!({"success":true,"output":{"valid":true,"current_generation":4,"restart_required":true}}),
        )],
    ] {
        let (_root, runtime, edit, original) = fixture();
        let mut responses = VecDeque::from(responses);
        assert!(
            apply_with_control("local", &edit, |_, _| ready(responses.pop_front().unwrap()))
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(runtime.runner_config.unwrap()).unwrap(),
            original
        );
        assert!(responses.is_empty());
    }
}

#[tokio::test]
async fn uncertain_reload_preserves_candidate_without_replay_even_if_generation_advanced() {
    for observed in [
        Ok(check(4)),
        Ok(check(5)),
        Err(desktop_state_unavailable("offline")),
    ] {
        let (_root, _runtime, edit, _) = fixture();
        let mut responses = VecDeque::from([
            Ok(check(4)),
            Err(desktop_state_unavailable("response lost")),
            observed,
        ]);
        let mut calls = Vec::new();
        let error = apply_with_control("local", &edit, |tool, _| {
            calls.push(tool);
            ready(responses.pop_front().unwrap())
        })
        .await
        .unwrap_err();
        assert_eq!(error.code, "runner_config_reconcile_required");
        assert!(edit.candidate_unchanged().unwrap());
        assert_eq!(
            calls,
            [
                "check_runner_config",
                "reload_runner_config",
                "check_runner_config"
            ]
        );
    }
}

#[tokio::test]
async fn successful_reload_with_restart_only_changes_is_not_rolled_back() {
    let (_root, _runtime, edit, _) = fixture();
    let mut responses = VecDeque::from([
        Ok(check(4)),
        Ok(serde_json::json!({
            "success":true,"output":{"current_generation":5,"restart_required":true}
        })),
    ]);
    let error = apply_with_control("local", &edit, |_, _| ready(responses.pop_front().unwrap()))
        .await
        .unwrap_err();
    assert_eq!(error.code, "runner_config_reconcile_required");
    assert!(edit.candidate_unchanged().unwrap());
    assert!(responses.is_empty());
}

#[tokio::test]
async fn external_config_edit_before_reload_is_never_replaced_or_applied() {
    let (_root, runtime, edit, _) = fixture();
    let path = runtime.runner_config.unwrap();
    let mut calls = 0;
    let error = apply_with_control("local", &edit, |tool, _| {
        calls += 1;
        assert_eq!(tool, "check_runner_config");
        std::fs::write(&path, "# external change").unwrap();
        ready(Ok(check(4)))
    })
    .await
    .unwrap_err();
    assert_eq!(error.code, "runner_config_concurrent_change");
    assert_eq!(calls, 1);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "# external change");
}

#[tokio::test]
async fn job_concurrency_save_rejects_unowned_runner_without_changing_the_file() {
    let (root, runtime, _edit, _) = fixture();
    let before = std::fs::read_to_string(runtime.runner_config.as_ref().unwrap()).unwrap();
    let app = AppState::new(root.path().to_path_buf(), root.path().join("resources")).unwrap();
    app.core.lock().await.as_mut().unwrap().config.runtime = Some(runtime.clone());
    let result = app
        .save_runner_job_concurrency(settings::JobConcurrencyUpdate {
            target: settings::target(&runtime).unwrap(),
            expected: None,
            limit: 12,
        })
        .await;
    assert_eq!(result.unwrap_err().code, "runner_not_owned");
    assert_eq!(
        std::fs::read_to_string(runtime.runner_config.as_ref().unwrap()).unwrap(),
        before
    );
}
