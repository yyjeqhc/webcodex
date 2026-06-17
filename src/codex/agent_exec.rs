use crate::projects::ProjectConfig;
use crate::shell_client::{assert_shell_client_owner, requested_by_from_auth};
use crate::shell_protocol::ShellJobOpRequest;
use crate::ShellClientRegistry;
use salvo::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) async fn resolve_agent_project_client(
    depot: &Depot,
    proj: &ProjectConfig,
) -> Result<(String, Arc<ShellClientRegistry>), String> {
    let client_id = proj.agent_client_id()?.to_string();
    let registry = depot
        .obtain::<Arc<ShellClientRegistry>>()
        .map_err(|_| "Shell client registry not configured".to_string())?
        .clone();

    let client = registry.get_client_view(&client_id).await;
    let client = match client {
        Some(c) if c.connected => c,
        Some(_) => return Err(format!("agent client {} is not connected", client_id)),
        None => return Err(format!("agent client {} not found", client_id)),
    };

    let auth = depot.obtain::<crate::auth::AuthContext>().ok();
    assert_shell_client_owner(auth, &client_id, client.owner.as_deref())?;

    Ok((client_id, registry))
}

pub(super) async fn run_agent_project_command(
    depot: &Depot,
    proj: &ProjectConfig,
    cmd: &str,
    timeout_secs: u64,
    _requested_by: &'static str,
    timeout_label: &'static str,
) -> (i32, String, String, u64) {
    let started = Instant::now();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let requested_by = requested_by_from_auth(auth.as_ref());

    let (client_id, registry) = match resolve_agent_project_client(depot, proj).await {
        Ok(v) => v,
        Err(e) => return (-1, String::new(), e, 0),
    };
    let job = match registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some(client_id),
                cwd: Some(proj.path.clone()),
                command: Some(cmd.to_string()),
                timeout_secs: Some(timeout_secs),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            requested_by.clone(),
        )
        .await
    {
        Ok(job) => job,
        Err(e) => return (-1, String::new(), e, started.elapsed().as_millis() as u64),
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1) + 2);
    loop {
        if Instant::now() >= deadline {
            let _ = registry.stop_job(&job.job_id, requested_by.clone()).await;
            let (_, stdout, stderr, _, _) = registry
                .job_log(&job.job_id, Some(1), Some(1), None)
                .await
                .unwrap_or_else(|_| (job.clone(), Some(String::new()), Some(String::new()), 1, 1));
            let stderr = stderr.unwrap_or_default();
            let timeout = format!("{} timed out after {} seconds", timeout_label, timeout_secs);
            return (
                -1,
                stdout.unwrap_or_default(),
                if stderr.trim().is_empty() {
                    timeout
                } else {
                    format!("{}\n{}", stderr, timeout)
                },
                started.elapsed().as_millis() as u64,
            );
        }
        let sleep_ms = match registry.get_job(&job.job_id).await {
            Ok(info) => {
                if matches!(
                    info.status.as_str(),
                    "completed" | "failed" | "stopped" | "timeout" | "lost"
                ) {
                    let (_, stdout, stderr, _, _) = registry
                        .job_log(&job.job_id, Some(1), Some(1), None)
                        .await
                        .unwrap_or_else(|_| {
                            (info.clone(), Some(String::new()), Some(String::new()), 1, 1)
                        });
                    let code = info.exit_code.unwrap_or_else(|| {
                        if info.status == "completed" {
                            0
                        } else {
                            -1
                        }
                    });
                    let mut stderr = stderr.unwrap_or_default();
                    if let Some(error) = info.error {
                        if !error.trim().is_empty() {
                            if !stderr.trim().is_empty() {
                                stderr.push('\n');
                            }
                            stderr.push_str(&error);
                        }
                    }
                    return (
                        code,
                        stdout.unwrap_or_default(),
                        stderr,
                        info.duration_ms
                            .unwrap_or_else(|| started.elapsed().as_millis() as u64),
                    );
                }
                if info.status == "queued" {
                    100
                } else {
                    250
                }
            }
            Err(e) => return (-1, String::new(), e, started.elapsed().as_millis() as u64),
        };
        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
    }
}
