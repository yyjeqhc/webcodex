use crate::projects::ProjectConfig;
use crate::shell_protocol::ShellJobOpRequest;
use crate::ShellClientRegistry;
use salvo::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) async fn run_agent_project_command(
    depot: &Depot,
    proj: &ProjectConfig,
    cmd: &str,
    timeout_secs: u64,
    requested_by: &'static str,
    timeout_label: &'static str,
) -> (i32, String, String, u64) {
    let started = Instant::now();

    // ---- agent readiness check ----
    let client_id = match proj.agent_client_id() {
        Ok(c) => c.to_string(),
        Err(e) => return (-1, String::new(), e, 0),
    };

    let registry = match depot.obtain::<Arc<ShellClientRegistry>>() {
        Ok(r) => r.clone(),
        Err(_) => {
            return (
                -1,
                String::new(),
                "Shell client registry not configured".to_string(),
                0,
            )
        }
    };

    let clients = registry.list_clients().await;
    let client = clients.into_iter().find(|c| c.client_id == client_id);
    let client = match client {
        Some(c) => {
            if !c.connected {
                return (
                    -1,
                    String::new(),
                    format!("agent client {} is not connected", client_id),
                    0,
                );
            }
            c
        }
        None => {
            return (
                -1,
                String::new(),
                format!("agent client {} not found", client_id),
                0,
            );
        }
    };

    // Enforce shell-client ownership only when the request has a real user identity.
    if let Some(auth) = depot.obtain::<crate::auth::AuthContext>().ok() {
        if !auth.is_bootstrap {
            if let (Some(username), Some(owner)) =
                (auth.username.as_deref(), client.owner.as_deref())
            {
                if username != owner {
                    return (
                        -1,
                        String::new(),
                        format!(
                            "agent client {} is owned by {}, current user is not allowed",
                            client_id, owner
                        ),
                        0,
                    );
                }
            }
        }
    }
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
            },
            requested_by.to_string(),
        )
        .await
    {
        Ok(job) => job,
        Err(e) => return (-1, String::new(), e, started.elapsed().as_millis() as u64),
    };
    let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1) + 2);
    loop {
        if Instant::now() >= deadline {
            let _ = registry.stop_job(&job.job_id).await;
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
