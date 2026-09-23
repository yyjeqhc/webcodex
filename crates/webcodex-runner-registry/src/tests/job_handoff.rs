use super::*;
use crate::state::{JobLifecycleState, ShellJobVisibility};

#[tokio::test]
async fn job_handoff_promotion_is_authorized_atomic_and_never_replaces_execution() {
    for state in ["nonterminal", "terminal", "cleanup"] {
        let registry = RunnerRegistry::default();
        register_with_instance(&registry, "handoff-fixture", "fixture-instance").await;
        let job = registry
            .start_job_with_metadata(
                ShellJobOpRequest {
                    login: false,
                    op: "start".into(),
                    client_id: Some("handoff-fixture".into()),
                    cwd: None,
                    command: Some("echo fixture-only".into()),
                    timeout_secs: Some(60),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "alice".into(),
                ShellJobStartMetadata {
                    project_id: Some("agent:handoff-fixture:repo".into()),
                    visibility: ShellJobVisibility::HiddenUntilHandoff,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        {
            let mut inner = registry.inner.lock().await;
            let record = inner.jobs_by_id.get_mut(&job.job_id).unwrap();
            if state == "terminal" {
                record.lifecycle = JobLifecycleState::Completed;
            }
            if state == "cleanup" {
                record.visibility = ShellJobVisibility::CleanupPending;
            }
        }
        let alice = auth_context(Some("alice"), false);
        let foreign = auth_context(Some("bob"), false);
        assert!(registry
            .promote_hidden_job(Some(&foreign), &job.job_id)
            .await
            .is_err());
        assert!(registry
            .get_job_for_auth(Some(&alice), &job.job_id)
            .await
            .is_err());
        for _ in 0..2 {
            let promoted = registry.promote_hidden_job(Some(&alice), &job.job_id).await;
            match state {
                "nonterminal" => {
                    let promoted = promoted.unwrap();
                    assert_eq!(promoted.job_id, job.job_id);
                    assert!(promoted.observation_token.is_some());
                    assert!(registry
                        .get_job_for_auth(Some(&alice), &job.job_id)
                        .await
                        .is_ok());
                }
                "terminal" => {
                    assert_eq!(promoted.unwrap().status, "completed");
                    assert!(registry
                        .get_job_for_auth(Some(&alice), &job.job_id)
                        .await
                        .is_err());
                }
                "cleanup" => {
                    assert!(promoted.is_err());
                    assert!(registry
                        .get_job_for_auth(Some(&alice), &job.job_id)
                        .await
                        .is_err());
                }
                _ => unreachable!(),
            }
            assert!(registry
                .get_job_for_auth(Some(&foreign), &job.job_id)
                .await
                .is_err());
            assert_eq!(registry.inner.lock().await.jobs_by_id.len(), 1);
        }
    }
}
