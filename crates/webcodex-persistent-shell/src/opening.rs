//! Local and external transport registration and synchronized initialization.

use super::*;

impl PersistentShellManager {
    pub fn open(&self, launch: ShellLaunch) -> Result<ShellSummary, ShellError> {
        ensure_local_shell_supported()?;
        validate_launch(&launch)?;
        self.ensure_idle_sweeper();
        self.sweep_idle();
        {
            let entries = lock_unpoison(&self.inner.entries);
            if entries.contains_key(&launch.identity.shell_id) {
                return Err(ShellError::new(
                    "persistent_shell_id_conflict",
                    "persistent shell id already exists",
                ));
            }
            let active = entries
                .values()
                .filter(|entry| lock_unpoison(&entry.state).is_active())
                .count();
            if active >= self.inner.max_shells.load(Ordering::SeqCst) {
                return Err(ShellError::new(
                    "persistent_shell_limit_reached",
                    format!(
                        "persistent shell limit reached ({})",
                        self.inner.max_shells.load(Ordering::SeqCst)
                    ),
                ));
            }
        }
        {
            let active = lock_unpoison(&self.inner.active_by_session);
            if active.contains_key(&launch.identity.workflow_session_id) {
                return Err(ShellError::new(
                    "persistent_shell_already_open",
                    "Workflow Session already has an active persistent shell",
                ));
            }
        }

        #[cfg(unix)]
        let process: Box<dyn ShellTransport> = Box::new(spawn_shell_process(&launch)?);
        #[cfg(windows)]
        let process: Box<dyn ShellTransport> = windows::spawn_shell_process(&launch)?;
        #[cfg(not(any(unix, windows)))]
        let process: Box<dyn ShellTransport> = spawn_shell_process(&launch)?;
        let timestamp = now_ts();

        let entry = Arc::new(ShellEntry {
            identity: launch.identity.clone(),
            dialect: launch.dialect,
            profile: launch.profile,
            initial_cwd: Mutex::new(launch.initial_cwd.clone()),
            initial_cwd_frozen: AtomicBool::new(true),
            current_cwd: Mutex::new(launch.initial_cwd),
            created_at: timestamp,
            last_activity_at: AtomicU64::new(timestamp.max(0) as u64),
            last_activity_instant: Mutex::new(Instant::now()),
            state: Mutex::new(ShellState::Opening),
            busy: AtomicBool::new(false),
            exit_code: Mutex::new(None),
            close_reason: Mutex::new(None),
            metadata: Mutex::new(process.metadata()),
            process,
        });

        let initialization = launch.initialization.unwrap_or_default();
        self.register_and_initialize(&entry, &initialization)
    }

    /// Open a persistent shell backed by an externally-provided transport
    /// (e.g. a remote SSH shell). The caller is responsible for spawning the
    /// transport and for the remote cwd/bootstrap; this method runs the same
    /// registration, initialization, and synchronization logic as [`open`], so
    /// the shared state machine, limits, idle sweeper, and lifecycle apply
    /// unchanged. `initial_cwd_seed` is provisional opening state only. A
    /// successful first control frame replaces it with the authoritative,
    /// absolute cwd reported by the transport and freezes that value.
    pub fn open_with_transport(
        &self,
        identity: ShellIdentity,
        dialect: String,
        profile: Option<String>,
        initial_cwd_seed: PathBuf,
        initialization: Option<String>,
        transport: Box<dyn ShellTransport>,
    ) -> Result<ShellSummary, ShellError> {
        self.ensure_idle_sweeper();
        self.sweep_idle();
        {
            let entries = lock_unpoison(&self.inner.entries);
            if entries.contains_key(&identity.shell_id) {
                return Err(ShellError::new(
                    "persistent_shell_id_conflict",
                    "persistent shell id already exists",
                ));
            }
            let active = entries
                .values()
                .filter(|entry| lock_unpoison(&entry.state).is_active())
                .count();
            if active >= self.inner.max_shells.load(Ordering::SeqCst) {
                return Err(ShellError::new(
                    "persistent_shell_limit_reached",
                    format!(
                        "persistent shell limit reached ({})",
                        self.inner.max_shells.load(Ordering::SeqCst)
                    ),
                ));
            }
        }
        {
            let active = lock_unpoison(&self.inner.active_by_session);
            if active.contains_key(&identity.workflow_session_id) {
                return Err(ShellError::new(
                    "persistent_shell_already_open",
                    "Workflow Session already has an active persistent shell",
                ));
            }
        }

        let timestamp = now_ts();
        let entry = Arc::new(ShellEntry {
            identity,
            dialect,
            profile,
            initial_cwd: Mutex::new(initial_cwd_seed.clone()),
            initial_cwd_frozen: AtomicBool::new(false),
            current_cwd: Mutex::new(initial_cwd_seed),
            created_at: timestamp,
            last_activity_at: AtomicU64::new(timestamp.max(0) as u64),
            last_activity_instant: Mutex::new(Instant::now()),
            state: Mutex::new(ShellState::Opening),
            busy: AtomicBool::new(false),
            exit_code: Mutex::new(None),
            close_reason: Mutex::new(None),
            metadata: Mutex::new(transport.metadata()),
            process: transport,
        });

        let initialization = initialization.unwrap_or_default();
        self.register_and_initialize(&entry, &initialization)
    }

    fn register_and_initialize(
        &self,
        entry: &Arc<ShellEntry>,
        initialization: &str,
    ) -> Result<ShellSummary, ShellError> {
        {
            let mut entries = lock_unpoison(&self.inner.entries);
            let mut active = lock_unpoison(&self.inner.active_by_session);
            if active.contains_key(&entry.identity.workflow_session_id) {
                entry.process.shutdown();
                return Err(ShellError::new(
                    "persistent_shell_already_open",
                    "Workflow Session already has an active persistent shell",
                ));
            }
            let active_count = entries
                .values()
                .filter(|existing| lock_unpoison(&existing.state).is_active())
                .count();
            if active_count >= self.inner.max_shells.load(Ordering::SeqCst) {
                entry.process.shutdown();
                return Err(ShellError::new(
                    "persistent_shell_limit_reached",
                    format!(
                        "persistent shell limit reached ({})",
                        self.inner.max_shells.load(Ordering::SeqCst)
                    ),
                ));
            }
            active.insert(
                entry.identity.workflow_session_id.clone(),
                entry.identity.shell_id.clone(),
            );
            entries.insert(entry.identity.shell_id.clone(), Arc::clone(entry));
        }

        let init_token = command_token();
        entry.process.set_expected_token(&init_token);
        let mut completion = CompletionProgress::default();
        if let Err(error) = entry.process.write_command(initialization, &init_token) {
            self.transition_terminal(
                entry,
                ShellState::Poisoned,
                None,
                Some("initialization_write_failed".to_string()),
            );
            entry.process.shutdown();
            return Err(error);
        }
        match entry.process.wait_for_completion(
            &init_token,
            OPEN_INITIALIZATION_TIMEOUT,
            &mut completion,
        ) {
            WaitOutcome::Frame(frame) if frame.status == 0 => {
                if !entry.process.reported_cwd_is_absolute(&frame.cwd) {
                    self.transition_terminal(
                        entry,
                        ShellState::Poisoned,
                        None,
                        Some("initialization_cwd_unobservable".to_string()),
                    );
                    entry.process.shutdown();
                    return Err(ShellError::new(
                        "shell_reset_required",
                        "persistent shell initialization did not report an absolute cwd",
                    ));
                }
                let promoted = {
                    let mut state = lock_unpoison(&entry.state);
                    if *state == ShellState::Opening {
                        *state = ShellState::Running;
                        true
                    } else {
                        false
                    }
                };
                if !promoted {
                    entry.process.shutdown();
                    return Err(ShellError::new(
                        "persistent_shell_stale",
                        "persistent shell was closed while it was opening",
                    ));
                }
                let authoritative_cwd = frame.cwd;
                {
                    let mut initial_cwd = lock_unpoison(&entry.initial_cwd);
                    if entry
                        .initial_cwd_frozen
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        *initial_cwd = authoritative_cwd.clone();
                    }
                }
                *lock_unpoison(&entry.current_cwd) = authoritative_cwd;
                entry.touch();
                Ok(entry.summary())
            }
            WaitOutcome::Frame(frame) => {
                self.transition_terminal(
                    entry,
                    ShellState::Poisoned,
                    Some(frame.status),
                    Some("initialization_failed".to_string()),
                );
                entry.process.shutdown();
                Err(ShellError::new(
                    "persistent_shell_initialization_failed",
                    format!(
                        "persistent shell initialization exited with {}",
                        frame.status
                    ),
                ))
            }
            WaitOutcome::Exited(status) => {
                let code = status.code();
                self.transition_terminal(
                    entry,
                    ShellState::Exited,
                    code,
                    Some("shell_exited_during_initialization".to_string()),
                );
                entry.process.shutdown();
                Err(ShellError::new(
                    "persistent_shell_exited",
                    "persistent shell exited during initialization",
                ))
            }
            WaitOutcome::TimedOut | WaitOutcome::ControlLost => {
                self.transition_terminal(
                    entry,
                    ShellState::Poisoned,
                    None,
                    Some("initialization_sync_lost".to_string()),
                );
                entry.process.shutdown();
                Err(ShellError::new(
                    "shell_reset_required",
                    "persistent shell initialization did not reach a synchronized state",
                ))
            }
        }
    }
}

#[cfg(any(unix, windows))]
fn ensure_local_shell_supported() -> Result<(), ShellError> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_local_shell_supported() -> Result<(), ShellError> {
    Err(persistent_shell_unsupported_error())
}

#[cfg(not(any(unix, windows)))]
fn persistent_shell_unsupported_error() -> ShellError {
    ShellError::new(
        "persistent_shell_unsupported",
        "persistent local shell is not supported on this platform",
    )
}

fn validate_launch(launch: &ShellLaunch) -> Result<(), ShellError> {
    for (field, value) in [
        ("shell_id", launch.identity.shell_id.as_str()),
        (
            "workflow_session_id",
            launch.identity.workflow_session_id.as_str(),
        ),
        (
            "runtime_project_id",
            launch.identity.runtime_project_id.as_str(),
        ),
        ("executor", launch.identity.executor.as_str()),
        ("dialect", launch.dialect.as_str()),
        ("program", launch.program.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(ShellError::new(
                "persistent_shell_invalid_open",
                format!("{field} must be non-empty and contain no control characters"),
            ));
        }
    }
    #[cfg(unix)]
    if !matches!(launch.dialect.as_str(), "sh" | "bash") {
        return Err(ShellError::new(
            "persistent_shell_dialect_unsupported",
            "Unix persistent shells support only sh or bash",
        ));
    }
    #[cfg(windows)]
    if launch.dialect != "powershell" {
        return Err(ShellError::new(
            "persistent_shell_dialect_unsupported",
            "Windows persistent shells require a configured PowerShell program",
        ));
    }
    if launch.args.iter().any(|arg| arg.contains('\0')) {
        return Err(ShellError::new(
            "persistent_shell_invalid_open",
            "persistent shell startup args cannot contain NUL",
        ));
    }
    #[cfg(windows)]
    if launch.args.iter().any(|arg| {
        matches!(
            arg.to_ascii_lowercase().as_str(),
            "-command" | "-encodedcommand" | "-file"
        )
    }) {
        return Err(ShellError::new(
            "persistent_shell_invalid_open",
            "Windows persistent shell startup args cannot contain PowerShell command/file payload switches",
        ));
    }
    #[cfg(unix)]
    if launch.args.iter().any(|arg| arg == "-c") {
        return Err(ShellError::new(
            "persistent_shell_invalid_open",
            "Unix persistent shell startup args cannot contain -c",
        ));
    }
    if !launch.initial_cwd.is_dir() {
        return Err(ShellError::new(
            "persistent_shell_invalid_cwd",
            "persistent shell cwd must be an existing directory",
        ));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn spawn_shell_process(_launch: &ShellLaunch) -> Result<Box<dyn ShellTransport>, ShellError> {
    Err(persistent_shell_unsupported_error())
}
