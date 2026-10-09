use super::*;

const COMMAND_CANARY: &str = "private-initialization-command-canary";
const CWD_CANARY: &str = "private-cwd-canary";
const STDOUT_CANARY: &str = "private-stdout-canary";
const STDERR_CANARY: &str = "private-stderr-canary";

#[derive(Default)]
struct Calls {
    writes: AtomicUsize,
    waits: AtomicUsize,
    interrupts: AtomicUsize,
    shutdowns: AtomicUsize,
    token: Mutex<String>,
}

struct IncompleteOpeningTransport {
    timed_out: bool,
    control_received: bool,
    stdout_synced: bool,
    stderr_synced: bool,
    calls: Arc<Calls>,
    stdout: Arc<Mutex<BoundedBuffer>>,
    stderr: Arc<Mutex<BoundedBuffer>>,
}

impl ShellTransport for IncompleteOpeningTransport {
    fn set_expected_token(&self, token: &str) {
        *lock_unpoison(&self.calls.token) = token.to_string();
    }

    fn write_command(&self, command: &str, token: &str) -> Result<(), ShellError> {
        assert_eq!(command, COMMAND_CANARY);
        assert_eq!(token, *lock_unpoison(&self.calls.token));
        self.calls.writes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn wait_for_completion(
        &self,
        token: &str,
        timeout: Duration,
        progress: &mut CompletionProgress,
    ) -> WaitOutcome {
        assert_eq!(timeout, OPEN_INITIALIZATION_TIMEOUT);
        self.calls.waits.fetch_add(1, Ordering::SeqCst);
        progress.control = self.control_received.then(|| ControlFrame {
            token: token.to_string(),
            status: 0,
            cwd: PathBuf::from(CWD_CANARY),
        });
        progress.stdout_synced = self.stdout_synced;
        progress.stderr_synced = self.stderr_synced;
        if self.timed_out {
            WaitOutcome::TimedOut
        } else {
            WaitOutcome::ControlLost
        }
    }

    fn try_wait(&self) -> Option<ExitStatus> {
        None
    }

    fn interrupt(&self) {
        self.calls.interrupts.fetch_add(1, Ordering::SeqCst);
    }

    fn shutdown(&self) {
        self.calls.shutdowns.fetch_add(1, Ordering::SeqCst);
    }

    fn terminate_remaining_group_after_exit(&self) {
        panic!("an initialization synchronization failure is not a confirmed process exit");
    }

    fn stdout(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stdout
    }

    fn stderr(&self) -> &Arc<Mutex<BoundedBuffer>> {
        &self.stderr
    }
}

fn check_incomplete_opening(timed_out: bool, control: bool, stdout: bool, stderr: bool) {
    let manager = PersistentShellManager::new(ShellLimits::default());
    let calls = Arc::new(Calls::default());
    let mut stdout_buffer = BoundedBuffer::new(1024);
    stdout_buffer.append(STDOUT_CANARY.as_bytes());
    let mut stderr_buffer = BoundedBuffer::new(1024);
    stderr_buffer.append(STDERR_CANARY.as_bytes());
    let identity = ShellIdentity {
        shell_id: "opening-diagnostic-shell".to_string(),
        workflow_session_id: "opening-diagnostic-session".to_string(),
        runtime_project_id: "opening-diagnostic-project".to_string(),
        executor: "agent".to_string(),
        client_id: None,
    };
    let error = manager
        .open_with_transport(
            identity.clone(),
            "powershell".to_string(),
            None,
            PathBuf::from(CWD_CANARY),
            Some(COMMAND_CANARY.to_string()),
            Box::new(IncompleteOpeningTransport {
                timed_out,
                control_received: control,
                stdout_synced: stdout,
                stderr_synced: stderr,
                calls: Arc::clone(&calls),
                stdout: Arc::new(Mutex::new(stdout_buffer)),
                stderr: Arc::new(Mutex::new(stderr_buffer)),
            }),
        )
        .unwrap_err();

    assert_eq!(error.code, "shell_reset_required");
    let reason = if timed_out {
        "timed out"
    } else {
        "control synchronization was lost"
    };
    assert!(error.message.contains(reason), "{}", error.message);
    for evidence in [
        format!("control_received={control}"),
        format!("stdout_synced={stdout}"),
        format!("stderr_synced={stderr}"),
    ] {
        assert!(error.message.contains(&evidence), "{}", error.message);
    }
    let token = lock_unpoison(&calls.token).clone();
    assert!(!token.is_empty());
    for private_value in [
        COMMAND_CANARY,
        CWD_CANARY,
        STDOUT_CANARY,
        STDERR_CANARY,
        &token,
    ] {
        assert!(!error.message.contains(private_value));
    }

    let summary = manager
        .status(
            &identity.shell_id,
            &identity.workflow_session_id,
            &identity.runtime_project_id,
        )
        .unwrap();
    assert_eq!(summary.state, ShellState::Poisoned);
    assert_eq!(
        summary.close_reason.as_deref(),
        Some("initialization_sync_lost")
    );
    assert_eq!(manager.active_count(), 0);
    assert_eq!(calls.shutdowns.load(Ordering::SeqCst), 1);

    let exec_error = manager
        .exec(
            &identity.shell_id,
            &identity.workflow_session_id,
            &identity.runtime_project_id,
            "must-not-be-dispatched",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(exec_error.code, "persistent_shell_stale");
    assert_eq!(calls.writes.load(Ordering::SeqCst), 1);
    assert_eq!(calls.waits.load(Ordering::SeqCst), 1);
    assert_eq!(calls.interrupts.load(Ordering::SeqCst), 0);
    assert_eq!(calls.shutdowns.load(Ordering::SeqCst), 1);
    assert_eq!(*lock_unpoison(&calls.token), token);
}

#[test]
fn initialization_timeout_reports_only_bounded_synchronization_evidence() {
    for (control, stdout, stderr) in [(false, false, false), (true, true, false)] {
        check_incomplete_opening(true, control, stdout, stderr);
    }
}

#[test]
fn initialization_control_loss_reports_only_bounded_synchronization_evidence() {
    for (control, stdout, stderr) in [(false, true, true), (true, false, true)] {
        check_incomplete_opening(false, control, stdout, stderr);
    }
}
