use crate::activity::{sanitize_message, ActivityEventKind, ActivityLevel, ActivityLog};
use crate::error::{DesktopError, DesktopResult};
use crate::platform;
use crate::process::reclaim_owned_tree;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const LOG_LINES: usize = 80;
const LOG_LINE_BYTES: usize = 2048;
const MACHINE_LINE_BYTES: usize = 16 * 1024;
const GRACEFUL_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const STREAM_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProcessKind {
    LocalServer,
    LocalRunner,
    QuickShare,
    RegularTunnel,
}

impl ProcessKind {
    fn source(self) -> &'static str {
        match self {
            Self::LocalServer => "service",
            Self::LocalRunner => "runner",
            Self::QuickShare => "quick_share",
            Self::RegularTunnel => "regular_tunnel",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessPhase {
    Starting,
    Running,
    Stopping,
    Exited,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProcessSnapshot {
    pub kind: ProcessKind,
    pub phase: ProcessPhase,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub owned_by_desktop: bool,
}

struct ManagedProcess {
    child: Child,
    owned_tree: platform::OwnedProcessTree,
    phase: ProcessPhase,
    exit_code: Option<i32>,
    logs: Arc<Mutex<VecDeque<String>>>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

pub struct ProcessSupervisor {
    processes: HashMap<ProcessKind, ManagedProcess>,
    activity: ActivityLog,
}

impl ProcessSupervisor {
    pub fn new(activity: ActivityLog) -> Self {
        Self {
            processes: HashMap::new(),
            activity,
        }
    }

    pub async fn spawn_owned(
        &mut self,
        kind: ProcessKind,
        mut command: Command,
        machine_stdout: bool,
    ) -> DesktopResult<Option<mpsc::UnboundedReceiver<Value>>> {
        self.refresh();
        if self.processes.get(&kind).is_some_and(|process| {
            matches!(
                process.phase,
                ProcessPhase::Starting | ProcessPhase::Running | ProcessPhase::Stopping
            )
        }) {
            return Err(DesktopError::new(
                "process_already_running",
                format!("Desktop already owns an active {kind:?} process"),
                "Stop the existing Desktop-owned process first.",
            ));
        }
        #[cfg(target_os = "macos")]
        if self.processes.contains_key(&kind) {
            // refresh() may have reaped a root that exited while one of its
            // descendants is still alive. Preserve the recorded process-group
            // authority until that terminal generation is fully reclaimed.
            self.stop(kind).await;
        }
        #[cfg(not(target_os = "macos"))]
        self.processes.remove(&kind);

        if matches!(kind, ProcessKind::QuickShare | ProcessKind::RegularTunnel) {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        platform::configure_child(&mut command);
        let mut child = command.spawn().map_err(|error| {
            DesktopError::new(
                "process_start_failed",
                format!("Could not start the {kind:?} process"),
                "Check the configured WebCodex binaries and retry.",
            )
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
        let pid = child.id().ok_or_else(|| {
            let _ = child.start_kill();
            DesktopError::new(
                "process_start_failed",
                format!("Could not establish ownership for the {kind:?} process"),
                "Retry the operation.",
            )
        })?;
        let owned_tree = platform::OwnedProcessTree::from_spawned_root(pid).ok_or_else(|| {
            let _ = child.start_kill();
            DesktopError::new(
                "process_start_failed",
                format!(
                    "Could not establish a safe process-tree identity for the {kind:?} process"
                ),
                "Retry the operation.",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            DesktopError::new(
                "process_start_failed",
                "Could not capture process output",
                "Retry the operation.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            DesktopError::new(
                "process_start_failed",
                "Could not capture process diagnostics",
                "Retry the operation.",
            )
        })?;

        let logs = Arc::new(Mutex::new(VecDeque::new()));
        let (machine_tx, machine_rx) = if machine_stdout {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };
        let stdout_logs = Arc::clone(&logs);
        let stdout_task = tokio::spawn(drain_stream(
            stdout,
            stdout_logs,
            machine_tx,
            machine_stdout,
        ));
        let stderr_logs = Arc::clone(&logs);
        let stderr_task = tokio::spawn(drain_stream(stderr, stderr_logs, None, false));
        self.activity.push(
            ActivityEventKind::ProcessStarted,
            kind.source(),
            ActivityLevel::Info,
            format!("Desktop started the process (PID {pid})"),
        );
        self.processes.insert(
            kind,
            ManagedProcess {
                child,
                owned_tree,
                phase: ProcessPhase::Starting,
                exit_code: None,
                logs,
                stdout_task,
                stderr_task,
            },
        );
        Ok(machine_rx)
    }

    pub fn refresh(&mut self) {
        for (kind, process) in &mut self.processes {
            if !matches!(
                process.phase,
                ProcessPhase::Starting | ProcessPhase::Running
            ) {
                continue;
            }
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    process.exit_code = status.code();
                    process.phase = if status.success() {
                        ProcessPhase::Exited
                    } else {
                        ProcessPhase::Failed
                    };
                    self.activity.push(
                        ActivityEventKind::ProcessExited,
                        kind.source(),
                        if status.success() {
                            ActivityLevel::Info
                        } else {
                            ActivityLevel::Error
                        },
                        format!("Desktop-owned process exited with status {status}"),
                    );
                }
                Ok(None) => process.phase = ProcessPhase::Running,
                Err(_) => {
                    process.phase = ProcessPhase::Failed;
                    self.activity.push(
                        ActivityEventKind::ProcessObservationFailed,
                        kind.source(),
                        ActivityLevel::Error,
                        "Desktop could not observe the child process state",
                    );
                }
            }
        }
    }

    pub fn snapshot(&mut self, kind: ProcessKind) -> Option<ProcessSnapshot> {
        self.refresh();
        self.processes.get(&kind).map(|process| ProcessSnapshot {
            kind,
            phase: process.phase,
            pid: process.child.id(),
            exit_code: process.exit_code,
            owned_by_desktop: true,
        })
    }

    pub fn logs(&self, kind: ProcessKind) -> Vec<String> {
        self.processes
            .get(&kind)
            .map(|process| {
                process
                    .logs
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn stop(&mut self, kind: ProcessKind) {
        let Some(mut process) = self.processes.remove(&kind) else {
            return;
        };
        if matches!(
            process.phase,
            ProcessPhase::Starting | ProcessPhase::Running
        ) {
            process.phase = ProcessPhase::Stopping;
            self.activity.push(
                ActivityEventKind::ProcessStopping,
                kind.source(),
                ActivityLevel::Info,
                "Stopping the Desktop-owned process",
            );
            let mut graceful =
                if matches!(kind, ProcessKind::QuickShare | ProcessKind::RegularTunnel) {
                    drop(process.child.stdin.take());
                    matches!(
                        tokio::time::timeout(GRACEFUL_STOP_TIMEOUT, process.child.wait()).await,
                        Ok(Ok(_))
                    )
                } else {
                    false
                };
            #[cfg(target_os = "macos")]
            if graceful && platform::owned_tree_is_running(process.owned_tree) {
                // EOF can let the root exit before a descendant. Exact ownership
                // is complete only when the process group itself is gone.
                graceful = false;
            }
            if !graceful {
                reclaim_owned_tree(
                    &mut process.child,
                    process.owned_tree,
                    tokio::time::Instant::now() + GRACEFUL_STOP_TIMEOUT,
                )
                .await;
            }
        }
        #[cfg(target_os = "macos")]
        if platform::owned_tree_is_running(process.owned_tree) {
            // A root may already be terminal when stop() is called while one of
            // its descendants is still alive. The recorded process-group identity
            // remains the authority for that cleanup.
            reclaim_owned_tree(
                &mut process.child,
                process.owned_tree,
                tokio::time::Instant::now() + GRACEFUL_STOP_TIMEOUT,
            )
            .await;
        }
        finish_drain_task(process.stdout_task).await;
        finish_drain_task(process.stderr_task).await;
        self.activity.push(
            ActivityEventKind::ProcessStopped,
            kind.source(),
            ActivityLevel::Info,
            "Desktop-owned process stopped",
        );
    }

    pub async fn stop_all(&mut self) {
        for kind in [
            ProcessKind::QuickShare,
            ProcessKind::RegularTunnel,
            ProcessKind::LocalRunner,
            ProcessKind::LocalServer,
        ] {
            self.stop(kind).await;
        }
    }
}

async fn finish_drain_task(mut task: JoinHandle<()>) {
    if tokio::time::timeout(STREAM_DRAIN_TIMEOUT, &mut task)
        .await
        .is_err()
    {
        task.abort();
        let _ = task.await;
    }
}

async fn drain_stream<R>(
    mut reader: R,
    logs: Arc<Mutex<VecDeque<String>>>,
    machine_tx: Option<mpsc::UnboundedSender<Value>>,
    machine_only: bool,
) where
    R: AsyncRead + Unpin,
{
    let mut buffer = [0_u8; 4096];
    let mut line = Vec::with_capacity(4096);
    let line_limit = if machine_only {
        MACHINE_LINE_BYTES
    } else {
        LOG_LINE_BYTES
    };
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                process_line(&line, &logs, machine_tx.as_ref(), machine_only);
                line.clear();
            } else if line.len() < line_limit {
                line.push(*byte);
            }
        }
    }
    if !line.is_empty() {
        process_line(&line, &logs, machine_tx.as_ref(), machine_only);
    }
}

fn process_line(
    line: &[u8],
    logs: &Arc<Mutex<VecDeque<String>>>,
    machine_tx: Option<&mpsc::UnboundedSender<Value>>,
    machine_only: bool,
) {
    let text = String::from_utf8_lossy(line).trim().to_string();
    if text.is_empty() {
        return;
    }
    if machine_only {
        if let (Some(tx), Ok(value)) = (machine_tx, serde_json::from_str::<Value>(&text)) {
            let _ = tx.send(value);
        }
        return;
    }
    let mut logs = logs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    logs.push_back(sanitize_message(&text));
    while logs.len() > LOG_LINES {
        logs.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn supervisor_only_stops_children_it_owns() {
        let activity = ActivityLog::default();
        let mut supervisor = ProcessSupervisor::new(activity);
        supervisor.stop(ProcessKind::LocalRunner).await;
        assert!(supervisor.snapshot(ProcessKind::LocalRunner).is_none());
    }

    #[test]
    fn process_kind_has_no_generic_process_surface() {
        let kinds = [
            ProcessKind::LocalServer,
            ProcessKind::LocalRunner,
            ProcessKind::QuickShare,
            ProcessKind::RegularTunnel,
        ];
        assert_eq!(kinds.len(), 4);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn regular_tunnel_stop_closes_stdin_for_canonical_graceful_shutdown() {
        let marker = std::env::temp_dir().join(format!(
            "webcodex-desktop-regular-tunnel-eof-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let escaped_marker = marker.to_string_lossy().replace('\'', "''");
        let mut command = Command::new("powershell.exe");
        command
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(format!(
                "$null = [Console]::In.ReadToEnd(); Set-Content -LiteralPath '{escaped_marker}' -Value 'eof'"
            ));

        let activity = ActivityLog::default();
        let mut supervisor = ProcessSupervisor::new(activity);
        supervisor
            .spawn_owned(ProcessKind::RegularTunnel, command, false)
            .await
            .expect("start EOF fixture");
        supervisor.stop(ProcessKind::RegularTunnel).await;

        assert!(
            marker.is_file(),
            "regular tunnel child must observe stdin EOF before the graceful stop completes"
        );
        let _ = std::fs::remove_file(marker);
    }
}
