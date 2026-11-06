use super::*;
use crate::state::{JobLifecycleState, ShellJobVisibility};

#[tokio::test]
async fn job_handoff_promotion_is_authorized_atomic_and_never_replaces_execution() {
    for state in ["nonterminal", "terminal", "cleanup"] {
        let registry = RunnerRegistry::default();
        register_with_instance(&registry, "handoff-fixture", "fixture-instance").await;
        let mut request = shell_job_start_request("handoff-fixture", "echo fixture-only", Some(60));
        request.context.project_id = Some("agent:handoff-fixture:repo".into());
        request.context.visibility = ShellJobVisibility::HiddenUntilHandoff;
        let job = registry
            .start_job(request, "alice".into(), None, None)
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

#[tokio::test]
async fn retained_terminal_logs_preserve_identity_authorization_and_do_not_publish_active_jobs() {
    let registry = RunnerRegistry::default();
    register_with_instance(&registry, "log-fixture", "instance").await;
    let mut request = shell_job_start_request("log-fixture", "echo fixture", Some(60));
    request.context.project_id = Some("agent:log-fixture:repo".into());
    request.context.visibility = ShellJobVisibility::HiddenUntilHandoff;
    let job = registry
        .start_job(request, "alice".into(), None, None)
        .await
        .unwrap();
    let alice = auth_context(Some("alice"), false);
    let bob = auth_context(Some("bob"), false);
    assert!(registry
        .retain_terminal_job_logs(Some(&alice), &job.job_id)
        .await
        .is_err());
    registry
        .inner
        .lock()
        .await
        .jobs_by_id
        .get_mut(&job.job_id)
        .unwrap()
        .lifecycle = JobLifecycleState::Completed;
    assert!(registry
        .retain_terminal_job_logs(Some(&bob), &job.job_id)
        .await
        .is_err());
    for _ in 0..2 {
        let retained = registry
            .retain_terminal_job_logs(Some(&alice), &job.job_id)
            .await
            .unwrap();
        assert_eq!(retained.job_id, job.job_id);
        assert_eq!(retained.status, "completed");
        assert!(retained.observation_token.is_some());
        assert!(registry
            .get_job_for_auth(Some(&alice), &job.job_id)
            .await
            .is_ok());
        assert!(registry
            .get_job_for_auth(Some(&bob), &job.job_id)
            .await
            .is_err());
        assert_eq!(registry.inner.lock().await.jobs_by_id.len(), 1);
    }
}
