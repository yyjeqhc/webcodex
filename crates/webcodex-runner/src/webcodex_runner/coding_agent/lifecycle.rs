//! Cancellation, restart recovery, and durable-before-live terminal transitions.

use super::protocol::bounded_text;
use super::store::{durable_record_from_snapshot, DurableDispatchPhase, STORE_RETENTION_SECS};
use super::*;

impl CodingAgentManager {
    pub(super) fn request_cancel(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
    ) -> CodingAgentRunSnapshot {
        let prompt_gate = entry.prompt_dispatch.lock().unwrap();
        let current = entry.snapshot();
        if current.state.terminal() {
            return current;
        }
        entry.cancel_requested.store(true, Ordering::Release);
        entry.changed.notify_all();
        if *prompt_gate == PromptDispatchGateState::PrePrompt {
            self.finish_pre_prompt_cancelled(run_id, entry);
        }
        entry.snapshot()
    }

    pub(super) fn finish_pre_prompt_cancelled(&self, run_id: &str, entry: &Arc<RunEntry>) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: None,
            message: Some("ACP prompt was not dispatched; CodingAgentRun was cancelled before prompt dispatch".to_string()),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: None,
            status: Some("cancelled".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Cancelled,
            CodingAgentExecutionState::NotStarted,
            terminal,
            event,
        );
    }

    pub(super) fn pre_prompt_interrupted(&self, run_id: &str, entry: &Arc<RunEntry>) -> bool {
        if !self.accepting.load(Ordering::Acquire) && !entry.snapshot().state.terminal() {
            let _ = self.request_cancel(run_id, entry);
        }
        entry.snapshot().state.terminal()
    }

    pub(super) fn setup_timeout(&self, run_id: &str, entry: &Arc<RunEntry>) {
        self.setup_failure(
            run_id,
            entry,
            "coding_agent_setup_timeout",
            "CodingAgentRun total deadline expired before ACP prompt dispatch",
        );
    }

    pub(super) fn recover(&self) -> Result<(), String> {
        let mut map = self.runs.lock().unwrap();
        for mut record in self.store.scan()? {
            if record.dispatch_phase != DurableDispatchPhase::Terminal {
                let message = if record.dispatch_phase
                    == DurableDispatchPhase::PromptDispatchMayHaveOccurred
                {
                    "Runner restarted after prompt dispatch uncertainty barrier"
                } else {
                    "Runner restarted before prompt dispatch barrier"
                };
                let state = if record.dispatch_phase
                    == DurableDispatchPhase::PromptDispatchMayHaveOccurred
                {
                    CodingAgentRunState::Lost
                } else {
                    CodingAgentRunState::Failed
                };
                let execution = if state == CodingAgentRunState::Lost {
                    CodingAgentExecutionState::OutcomeUnknown
                } else {
                    CodingAgentExecutionState::NotStarted
                };
                record.state = state.clone();
                record.execution_state = execution;
                record.dispatch_phase = DurableDispatchPhase::Terminal;
                record.updated_at = now();
                record.terminal = Some(CodingAgentTerminal {
                    stop_reason: None,
                    error_code: Some(
                        if state == CodingAgentRunState::Lost {
                            "runner_restart_uncertain"
                        } else {
                            "runner_restart_not_started"
                        }
                        .to_string(),
                    ),
                    message: Some(message.to_string()),
                    completed_at: now(),
                });
                self.store.write(&record)?;
            }
            map.insert(
                record.run_id.clone(),
                Arc::new(RunEntry::new(record.snapshot(0))),
            );
        }
        Ok(())
    }

    pub(super) fn cleanup_expired(&self) {
        let cutoff = now().saturating_sub(STORE_RETENTION_SECS);
        let expired = {
            let map = self.runs.lock().unwrap();
            map.iter()
                .filter(|(_, entry)| {
                    let snapshot = entry.snapshot();
                    snapshot.state.terminal() && snapshot.updated_at < cutoff
                })
                .map(|(run_id, _)| run_id.clone())
                .collect::<Vec<_>>()
        };
        if expired.is_empty() {
            return;
        }
        let mut map = self.runs.lock().unwrap();
        for run_id in expired {
            map.remove(&run_id);
            self.store.remove(&run_id);
        }
    }

    pub(super) fn setup_failure(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        code: &str,
        message: &str,
    ) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(bounded_text(message)),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("failed".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::NotStarted,
            terminal,
            event,
        );
    }

    pub(super) fn finish_failed(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        code: &str,
        message: String,
    ) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(bounded_text(&message)),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("failed".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Failed,
            CodingAgentExecutionState::Completed,
            terminal,
            event,
        );
    }

    pub(super) fn finish_terminal(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        state: CodingAgentRunState,
        stop_reason: &str,
        message: Option<&str>,
    ) {
        let terminal = CodingAgentTerminal {
            stop_reason: Some(stop_reason.to_string()),
            error_code: if state == CodingAgentRunState::Failed {
                Some(stop_reason.to_string())
            } else {
                None
            },
            message: message.map(bounded_text),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: message.map(bounded_text),
            label: Some(stop_reason.to_string()),
            status: Some(format!("{state:?}").to_ascii_lowercase()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            state,
            CodingAgentExecutionState::Completed,
            terminal,
            event,
        );
    }

    pub(super) fn mark_lost(&self, run_id: &str, entry: &Arc<RunEntry>, code: &str) {
        let terminal = CodingAgentTerminal {
            stop_reason: None,
            error_code: Some(code.to_string()),
            message: Some(
                "ACP prompt outcome is unknown; prompt must not be redispatched".to_string(),
            ),
            completed_at: now(),
        };
        let event = CodingAgentEvent {
            sequence: 0,
            kind: CodingAgentEventKind::Terminal,
            text: terminal.message.clone(),
            label: terminal.error_code.clone(),
            status: Some("lost".to_string()),
            usage: None,
        };
        self.commit_terminal_transition(
            run_id,
            entry,
            CodingAgentRunState::Lost,
            CodingAgentExecutionState::OutcomeUnknown,
            terminal,
            event,
        );
    }

    fn commit_terminal_transition(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        desired_state: CodingAgentRunState,
        desired_execution_state: CodingAgentExecutionState,
        desired_terminal: CodingAgentTerminal,
        desired_event: CodingAgentEvent,
    ) {
        let Some(_terminal_transition) = entry.begin_terminal_transition() else {
            return;
        };
        let current = entry.snapshot();
        let mut candidate = current.clone();
        candidate.state = desired_state.clone();
        candidate.execution_state = desired_execution_state;
        candidate.updated_at = desired_terminal.completed_at;
        candidate.terminal = Some(desired_terminal);
        let durable =
            durable_record_from_snapshot(run_id, &candidate, DurableDispatchPhase::Terminal)
                .and_then(|record| self.store.write(&record));
        match durable {
            Ok(()) => entry.publish_terminal(candidate, desired_event),
            Err(error) => {
                let pre_prompt = desired_execution_state == CodingAgentExecutionState::NotStarted;
                let completed_at = now();
                let terminal = CodingAgentTerminal {
                    stop_reason: None,
                    error_code: Some(CODING_AGENT_TERMINAL_PERSISTENCE_UNCERTAIN.to_string()),
                    message: Some(TERMINAL_PERSISTENCE_UNCERTAIN_MESSAGE.to_string()),
                    completed_at,
                };
                let mut uncertain = current;
                uncertain.state = if pre_prompt {
                    CodingAgentRunState::Failed
                } else {
                    CodingAgentRunState::Lost
                };
                uncertain.execution_state = if pre_prompt {
                    CodingAgentExecutionState::NotStarted
                } else {
                    CodingAgentExecutionState::OutcomeUnknown
                };
                uncertain.updated_at = completed_at;
                uncertain.terminal = Some(terminal.clone());
                let status = if pre_prompt { "failed" } else { "lost" };
                tracing::error!(
                    run_id = %run_id,
                    desired_state = ?desired_state,
                    desired_execution_state = ?desired_execution_state,
                    error = %error,
                    "ACP terminal durable commit failed; publishing conservative persistence uncertainty"
                );
                entry.publish_terminal(
                    uncertain,
                    CodingAgentEvent {
                        sequence: 0,
                        kind: CodingAgentEventKind::Terminal,
                        text: terminal.message.clone(),
                        label: terminal.error_code.clone(),
                        status: Some(status.to_string()),
                        usage: None,
                    },
                );
            }
        }
    }

    pub(super) fn persist_phase(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        phase: DurableDispatchPhase,
        state: CodingAgentRunState,
        execution_state: CodingAgentExecutionState,
        terminal: Option<CodingAgentTerminal>,
    ) -> Result<(), String> {
        entry.update_snapshot(|snapshot| {
            snapshot.state = state.clone();
            snapshot.execution_state = execution_state;
            snapshot.terminal = terminal.clone();
        });
        self.persist_from_entry(run_id, entry, phase)
    }

    pub(super) fn persist_from_entry(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        phase: DurableDispatchPhase,
    ) -> Result<(), String> {
        let snapshot = entry.snapshot();
        let record = durable_record_from_snapshot(run_id, &snapshot, phase)?;
        self.store.write(&record)
    }
}
