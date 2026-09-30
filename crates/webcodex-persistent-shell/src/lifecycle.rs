//! Identity lookup, terminal transitions, retention, and idle reclamation.

use super::*;

impl PersistentShellManager {
    pub fn status(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
    ) -> Result<ShellSummary, ShellError> {
        self.sweep_idle();
        let entry = self.lookup(shell_id, workflow_session_id, runtime_project_id)?;
        self.refresh_exit(&entry);
        Ok(entry.summary())
    }

    pub fn set_output_limit(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
        max_output_bytes: usize,
    ) -> Result<(), ShellError> {
        let entry = self.lookup(shell_id, workflow_session_id, runtime_project_id)?;
        lock_unpoison(entry.process.stdout()).set_max_bytes(max_output_bytes);
        lock_unpoison(entry.process.stderr()).set_max_bytes(max_output_bytes);
        Ok(())
    }

    pub fn close(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
        reason: &str,
    ) -> Result<ShellCloseResult, ShellError> {
        let entry = self.lookup(shell_id, workflow_session_id, runtime_project_id)?;
        self.refresh_exit(&entry);
        let already_closed = {
            let state = *lock_unpoison(&entry.state);
            matches!(
                state,
                ShellState::Closed | ShellState::Exited | ShellState::Poisoned | ShellState::Lost
            )
        };
        if !already_closed {
            self.transition_terminal(&entry, ShellState::Closed, None, Some(reason.to_string()));
            entry.process.shutdown();
        }
        Ok(ShellCloseResult {
            summary: entry.summary(),
            already_closed,
        })
    }

    pub fn close_session(&self, workflow_session_id: &str, reason: &str) -> usize {
        let entries = lock_unpoison(&self.inner.entries)
            .values()
            .filter(|entry| entry.identity.workflow_session_id == workflow_session_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut closed = 0;
        for entry in entries {
            self.refresh_exit(&entry);
            if lock_unpoison(&entry.state).is_active() {
                self.transition_terminal(
                    &entry,
                    ShellState::Closed,
                    None,
                    Some(reason.to_string()),
                );
                entry.process.shutdown();
                closed += 1;
            }
        }
        closed
    }

    pub fn close_project(&self, runtime_project_id: &str, reason: &str) -> usize {
        let entries = lock_unpoison(&self.inner.entries)
            .values()
            .filter(|entry| entry.identity.runtime_project_id == runtime_project_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut closed = 0;
        for entry in entries {
            self.refresh_exit(&entry);
            if lock_unpoison(&entry.state).is_active() {
                self.transition_terminal(
                    &entry,
                    ShellState::Closed,
                    None,
                    Some(reason.to_string()),
                );
                entry.process.shutdown();
                closed += 1;
            }
        }
        closed
    }

    pub fn close_all(&self, reason: &str) -> usize {
        let entries = lock_unpoison(&self.inner.entries)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut closed = 0;
        for entry in entries {
            self.refresh_exit(&entry);
            if lock_unpoison(&entry.state).is_active() {
                self.transition_terminal(
                    &entry,
                    ShellState::Closed,
                    None,
                    Some(reason.to_string()),
                );
                entry.process.shutdown();
                closed += 1;
            }
        }
        closed
    }

    pub fn active_count(&self) -> usize {
        lock_unpoison(&self.inner.active_by_session).len()
    }

    /// The opaque transport binding metadata captured when the shell was
    /// opened, if the transport reported any. Used by callers to validate that
    /// the bindings an open shell depends on are still current (e.g. a named
    /// SSH resource at the configuration generation it was opened against).
    pub fn metadata(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
    ) -> Result<Option<TransportMetadata>, ShellError> {
        let entry = self.lookup(shell_id, workflow_session_id, runtime_project_id)?;
        let metadata = lock_unpoison(&entry.metadata).clone();
        Ok(metadata)
    }

    pub fn sweep_idle(&self) -> usize {
        let idle = Duration::from_secs(self.inner.idle_timeout_secs.load(Ordering::SeqCst));
        let entries = lock_unpoison(&self.inner.entries)
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut closed = 0;
        for entry in entries {
            self.refresh_exit(&entry);
            let should_attempt_close = lock_unpoison(&entry.state).is_active()
                && lock_unpoison(&entry.last_activity_instant).elapsed() >= idle;
            if !should_attempt_close {
                continue;
            }
            if entry
                .busy
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }
            let should_close = lock_unpoison(&entry.state).is_active()
                && lock_unpoison(&entry.last_activity_instant).elapsed() >= idle;
            if !should_close {
                entry.busy.store(false, Ordering::SeqCst);
                continue;
            }
            self.transition_terminal(
                &entry,
                ShellState::Closed,
                None,
                Some("idle_timeout".to_string()),
            );
            entry.process.shutdown();
            closed += 1;
        }
        closed
    }

    pub(super) fn lookup(
        &self,
        shell_id: &str,
        workflow_session_id: &str,
        runtime_project_id: &str,
    ) -> Result<Arc<ShellEntry>, ShellError> {
        let entry = lock_unpoison(&self.inner.entries)
            .get(shell_id)
            .cloned()
            .ok_or_else(|| {
                ShellError::new(
                    "persistent_shell_not_found",
                    "persistent shell was not found or belongs to another runtime",
                )
            })?;
        entry.validate_identity(workflow_session_id, runtime_project_id)?;
        Ok(entry)
    }

    pub(super) fn ensure_idle_sweeper(&self) {
        if !self.inner.sweeper_started.swap(true, Ordering::SeqCst) {
            spawn_idle_sweeper(Arc::downgrade(&self.inner));
        }
    }

    pub(super) fn refresh_exit(&self, entry: &Arc<ShellEntry>) {
        if !lock_unpoison(&entry.state).is_active() {
            return;
        }
        if let Some(status) = entry.process.try_wait() {
            self.transition_terminal(
                entry,
                ShellState::Exited,
                status.code(),
                Some("shell_process_exited".to_string()),
            );
            entry.process.terminate_remaining_group_after_exit();
        }
    }

    pub(super) fn transition_terminal(
        &self,
        entry: &Arc<ShellEntry>,
        state: ShellState,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) {
        {
            let mut current = lock_unpoison(&entry.state);
            if !current.is_active() && *current != ShellState::Opening {
                return;
            }
            *current = state;
        }
        entry.busy.store(false, Ordering::SeqCst);
        *lock_unpoison(&entry.exit_code) = exit_code;
        *lock_unpoison(&entry.close_reason) = reason;
        // A terminal shell can never be reused; release its transport-binding
        // metadata so a stale binding cannot keep validating a dead entry.
        *lock_unpoison(&entry.metadata) = None;
        entry.touch();
        let mut active = lock_unpoison(&self.inner.active_by_session);
        if active
            .get(&entry.identity.workflow_session_id)
            .is_some_and(|id| id == &entry.identity.shell_id)
        {
            active.remove(&entry.identity.workflow_session_id);
        }
        drop(active);
        let mut order = lock_unpoison(&self.inner.terminal_order);
        if !order.iter().any(|id| id == &entry.identity.shell_id) {
            order.push_back(entry.identity.shell_id.clone());
        }
        drop(order);
        self.prune_terminal_records();
    }

    pub(super) fn prune_terminal_records(&self) {
        let limit = self.inner.max_terminal_records.load(Ordering::SeqCst);
        loop {
            let remove = {
                let mut order = lock_unpoison(&self.inner.terminal_order);
                (order.len() > limit).then(|| order.pop_front()).flatten()
            };
            let Some(shell_id) = remove else {
                break;
            };
            let mut entries = lock_unpoison(&self.inner.entries);
            if entries
                .get(&shell_id)
                .is_some_and(|entry| !lock_unpoison(&entry.state).is_active())
            {
                entries.remove(&shell_id);
            }
        }
    }
}

fn spawn_idle_sweeper(weak: Weak<ManagerInner>) {
    let _ = thread::Builder::new()
        .name("wc-persistent-shell-idle".to_string())
        .spawn(move || loop {
            let Some(inner) = weak.upgrade() else {
                break;
            };
            if inner.stop_sweeper.load(Ordering::SeqCst) {
                break;
            }
            let interval = Duration::from_secs(
                (inner.idle_timeout_secs.load(Ordering::SeqCst) / 4).clamp(1, 30),
            );
            drop(inner);
            thread::sleep(interval);
            let Some(inner) = weak.upgrade() else {
                break;
            };
            if inner.stop_sweeper.load(Ordering::SeqCst) {
                break;
            }
            PersistentShellManager { inner }.sweep_idle();
        });
}
