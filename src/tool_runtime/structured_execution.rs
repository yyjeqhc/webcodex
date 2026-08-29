use crate::auth::AuthContext;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellJobInfo, STRUCTURED_EXECUTION_TIMEOUT_DEFAULT_SECS, STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS,
    STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS,
};
use std::sync::Arc;
use std::time::Duration;

pub(crate) const STRUCTURED_EXECUTION_SYNC_WAIT_SECS: u64 = 10;
pub(crate) const STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StructuredExecutionBudget {
    pub(crate) effective_timeout_secs: u64,
    pub(crate) sync_wait_secs: u64,
}

impl StructuredExecutionBudget {
    pub(crate) fn resolve(timeout_secs: Option<u64>) -> Result<Self, String> {
        Self::resolve_with_sync_wait(timeout_secs, None)
    }

    pub(crate) fn resolve_with_sync_wait(
        timeout_secs: Option<u64>,
        sync_wait_secs: Option<u64>,
    ) -> Result<Self, String> {
        let effective_timeout_secs =
            timeout_secs.unwrap_or(STRUCTURED_EXECUTION_TIMEOUT_DEFAULT_SECS);
        if !(STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS..=STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS)
            .contains(&effective_timeout_secs)
        {
            return Err(format!(
                "timeout_secs must be between {STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS} and {STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS}"
            ));
        }
        let sync_wait_secs = match sync_wait_secs {
            Some(sync_wait_secs) => {
                if !(1..=STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS).contains(&sync_wait_secs) {
                    return Err(format!(
                        "sync_wait_secs must be between 1 and {STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS}"
                    ));
                }
                if sync_wait_secs > effective_timeout_secs {
                    return Err(format!(
                        "sync_wait_secs ({sync_wait_secs}) must not exceed effective timeout_secs ({effective_timeout_secs})"
                    ));
                }
                sync_wait_secs
            }
            None => STRUCTURED_EXECUTION_SYNC_WAIT_SECS.min(effective_timeout_secs),
        };
        Ok(Self {
            effective_timeout_secs,
            sync_wait_secs,
        })
    }
}

pub(crate) enum HiddenStructuredJobWait {
    Terminal {
        job: ShellJobInfo,
        stdout: String,
        stderr: String,
    },
    Continued {
        job: ShellJobInfo,
        execution_state: &'static str,
        command_started: bool,
    },
}

pub(crate) async fn await_hidden_structured_job(
    clients: Arc<ShellClientRegistry>,
    job_id: String,
    sync_wait: Duration,
    auth: Option<AuthContext>,
) -> Result<HiddenStructuredJobWait, String> {
    let mut guard = HiddenJobCleanupGuard::new(clients.clone(), job_id.clone(), auth.clone());
    let deadline = std::time::Instant::now() + sync_wait;
    loop {
        if let Ok(job) = clients
            .get_hidden_job_for_auth(auth.as_ref(), &job_id)
            .await
        {
            if crate::tool_runtime::jobs::is_terminal_job_status(&job.status) {
                let terminal = hidden_terminal_snapshot(&clients, auth.as_ref(), &job_id).await?;
                guard.disarm();
                return Ok(terminal);
            }
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let promoted = clients.promote_hidden_job(&job_id).await?;
    if crate::tool_runtime::jobs::is_terminal_job_status(&promoted.status) {
        let terminal = hidden_terminal_snapshot(&clients, auth.as_ref(), &job_id).await?;
        guard.disarm();
        return Ok(terminal);
    }
    let (execution_state, command_started) = match promoted.status.as_str() {
        "queued" | "agent_queued" | "started" => ("queued", false),
        "running" => ("running", true),
        "stop_requested" if promoted.started_at.is_some() => ("running", true),
        "stop_requested" => ("queued", false),
        // Recovery is a real retained Job contract, but it is not proof that
        // the execution is presently running. Preserve uncertainty explicitly.
        "recovering" => ("outcome_unknown", true),
        _ => ("outcome_unknown", true),
    };
    guard.disarm();
    Ok(HiddenStructuredJobWait::Continued {
        job: promoted,
        execution_state,
        command_started,
    })
}

async fn hidden_terminal_snapshot(
    clients: &ShellClientRegistry,
    auth: Option<&AuthContext>,
    job_id: &str,
) -> Result<HiddenStructuredJobWait, String> {
    let (job, stdout, stderr, _, _) = clients.hidden_job_log_for_auth(auth, job_id, None).await?;
    Ok(HiddenStructuredJobWait::Terminal {
        job,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

/// Cancels an initially hidden Job if the initiating future is dropped before
/// it can return either a terminal projection or a public continuation.
struct HiddenJobCleanupGuard {
    clients: Arc<ShellClientRegistry>,
    job_id: String,
    auth: Option<AuthContext>,
    armed: bool,
}

impl HiddenJobCleanupGuard {
    fn new(clients: Arc<ShellClientRegistry>, job_id: String, auth: Option<AuthContext>) -> Self {
        Self {
            clients,
            job_id,
            auth,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for HiddenJobCleanupGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let clients = self.clients.clone();
        clients.record_hidden_cleanup_intent(self.job_id.clone(), self.auth.clone());
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                clients.process_hidden_cleanup_intents().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_execution_budget_preserves_default_and_validates_explicit_sync_wait() {
        let default = StructuredExecutionBudget::resolve_with_sync_wait(None, None).unwrap();
        assert_eq!(default.effective_timeout_secs, 60);
        assert_eq!(default.sync_wait_secs, 10);

        let short = StructuredExecutionBudget::resolve_with_sync_wait(Some(5), None).unwrap();
        assert_eq!(short.effective_timeout_secs, 5);
        assert_eq!(short.sync_wait_secs, 5);

        for wait in [1, 45, 60] {
            let budget =
                StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(wait)).unwrap();
            assert_eq!(budget.effective_timeout_secs, 600);
            assert_eq!(budget.sync_wait_secs, wait);
        }

        assert!(StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(0)).is_err());
        assert!(StructuredExecutionBudget::resolve_with_sync_wait(Some(600), Some(61)).is_err());
        let conflict = StructuredExecutionBudget::resolve_with_sync_wait(Some(5), Some(6))
            .expect_err("explicit sync wait above total timeout must be rejected");
        assert!(conflict.contains("must not exceed effective timeout_secs"));
    }
}
