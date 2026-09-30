//! One ACP turn, prompt-dispatch fencing, and owned process I/O cleanup.

use super::protocol::{
    bounded_json_summary, error_frame, normalize_update, notification_frame, permission_event,
    remaining_run_budget, request_frame, result_frame, wait_outbound_write, AcpOutboundWriter,
    OutboundInterruption, OutboundWriteOutcome, ReaderEvent,
};
use super::store::DurableDispatchPhase;
use super::*;

impl CodingAgentManager {
    pub(super) fn terminate_run_io(
        &self,
        child: &mut ManagedChild,
        outbound: &mut AcpOutboundWriter,
    ) {
        let _ = child.terminate_tree();
        outbound.close();
        let deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        let _ = outbound.wait_finished_until(deadline);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            let _ = child.wait_tree_exit(remaining);
        }
        let _ = child.try_wait();
        let _ = self.worker_threads.reap_finished();
    }

    fn cleanup_run_io(&self, child: &mut ManagedChild, outbound: &mut AcpOutboundWriter) {
        outbound.close();
        let graceful_deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        loop {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= graceful_deadline {
                let _ = child.terminate_tree();
                break;
            }
            thread::sleep(ACP_POLL);
        }
        let forced_deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        let _ = outbound.wait_finished_until(forced_deadline);
        let remaining = forced_deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            let _ = child.wait_tree_exit(remaining);
        }
        let _ = child.try_wait();
        let _ = self.worker_threads.reap_finished();
    }

    fn write_post_prompt_frame(
        &self,
        run_id: &str,
        entry: &Arc<RunEntry>,
        child: &mut ManagedChild,
        outbound: &mut AcpOutboundWriter,
        frame: std::io::Result<Vec<u8>>,
        deadline: Instant,
        observe_cancel: bool,
        uncertainty_code: &str,
    ) -> bool {
        let frame = match frame {
            Ok(frame) => frame,
            Err(_) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                return false;
            }
        };
        let pending = match outbound.start_frame(frame) {
            Ok(pending) => pending,
            Err(_) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                return false;
            }
        };
        let cancelled = observe_cancel.then_some(&entry.cancel_requested);
        match wait_outbound_write(pending, deadline, cancelled, Some(&self.accepting)) {
            OutboundWriteOutcome::Written => true,
            OutboundWriteOutcome::Failed(_)
            | OutboundWriteOutcome::Interrupted(
                OutboundInterruption::Cancelled
                | OutboundInterruption::Shutdown
                | OutboundInterruption::Deadline,
            ) => {
                self.terminate_run_io(child, outbound);
                self.mark_lost(run_id, entry, uncertainty_code);
                false
            }
        }
    }

    pub(super) fn run_turn(
        self: Arc<Self>,
        request: webcodex_core::coding_agent::CodingAgentStartRequest,
        provider: Arc<ProviderEntry>,
        environment: Vec<(String, std::ffi::OsString)>,
        entry: Arc<RunEntry>,
    ) {
        let run_deadline = Instant::now() + Duration::from_secs(request.timeout_secs);
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            return;
        }
        let mut command = Command::new(&provider.config.executable);
        command.args(&provider.config.args).env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
        command.current_dir(&request.project_root);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match ManagedChild::spawn(&mut command) {
            Ok(child) => child,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_spawn_failed",
                    &error.to_string(),
                );
                return;
            }
        };
        if self.pre_prompt_should_stop(&request.run_id, &entry, run_deadline) {
            let _ = child.terminate_tree();
            let _ = child.wait();
            return;
        }
        let stdin = match child.child_mut().stdin.take() {
            Some(stdin) => stdin,
            None => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_stdio_unavailable",
                    "ACP stdin unavailable",
                );
                let _ = child.terminate_tree();
                return;
            }
        };
        let stdout = match child.child_mut().stdout.take() {
            Some(stdout) => stdout,
            None => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_stdio_unavailable",
                    "ACP stdout unavailable",
                );
                let _ = child.terminate_tree();
                return;
            }
        };
        let stderr = child.child_mut().stderr.take();
        if let Some(stderr) = stderr {
            let _ = thread::Builder::new()
                .name("wc-acp-stderr".to_string())
                .spawn(move || {
                    let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
                });
        }
        let mut outbound = match AcpOutboundWriter::spawn(stdin, &self.worker_threads) {
            Ok(outbound) => outbound,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_writer_unavailable",
                    &error.to_string(),
                );
                let _ = child.terminate_tree();
                let _ = child.wait_tree_exit(ACP_IO_CLEANUP_TIMEOUT);
                let _ = child.try_wait();
                return;
            }
        };
        let (tx, rx) = mpsc::sync_channel(32);
        let _reader = match thread::Builder::new()
            .name("wc-acp-stdout".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => {
                            let _ = tx.send(ReaderEvent::Eof);
                            break;
                        }
                        Ok(_) if line.len() <= ACP_MESSAGE_MAX_BYTES => {
                            let value = serde_json::from_str::<Value>(&line)
                                .map(ReaderEvent::Message)
                                .unwrap_or(ReaderEvent::Malformed);
                            if tx.send(value).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            let _ = tx.send(ReaderEvent::TooLarge);
                            break;
                        }
                        Err(_) => {
                            let _ = tx.send(ReaderEvent::Io);
                            break;
                        }
                    }
                }
            }) {
            Ok(handle) => handle,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_reader_unavailable",
                    &error.to_string(),
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };

        let Some((session_id, next_id)) = self.initialize_session(
            &request,
            &provider,
            &entry,
            &mut child,
            &mut outbound,
            &rx,
            run_deadline,
        ) else {
            return;
        };

        #[cfg(test)]
        {
            let barrier = self.prompt_dispatch_test_barrier.lock().unwrap().clone();
            if let Some(barrier) = barrier {
                barrier.wait();
            }
        }
        let prompt_id = next_id;
        let prompt_frame = match request_frame(
            prompt_id,
            "session/prompt",
            json!({
                "sessionId": session_id,
                "prompt": [{"type":"text","text":request.instruction}]
            }),
        ) {
            Ok(frame) => frame,
            Err(error) => {
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_failed",
                    &error.to_string(),
                );
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        let mut prompt_gate = entry.prompt_dispatch.lock().unwrap();
        if entry.snapshot().state.terminal()
            || entry.cancel_requested.load(Ordering::Acquire)
            || !self.accepting.load(Ordering::Acquire)
        {
            if !entry.snapshot().state.terminal() {
                entry.cancel_requested.store(true, Ordering::Release);
                self.finish_pre_prompt_cancelled(&request.run_id, &entry);
            }
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        if remaining_run_budget(run_deadline).is_none() {
            self.setup_timeout(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        // Irreversible uncertainty barrier: durable state is committed before the
        // first byte of session/prompt can be handed to the sole stdin writer.
        if let Err(error) = self.persist_phase(
            &request.run_id,
            &entry,
            DurableDispatchPhase::PromptDispatchMayHaveOccurred,
            CodingAgentRunState::Running,
            CodingAgentExecutionState::OutcomeUnknown,
            None,
        ) {
            self.setup_failure(
                &request.run_id,
                &entry,
                "coding_agent_dispatch_barrier_failed",
                &error,
            );
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        #[cfg(test)]
        {
            let delay = *self.prompt_after_barrier_test_delay.lock().unwrap();
            if let Some(delay) = delay {
                thread::sleep(delay);
            }
        }
        if remaining_run_budget(run_deadline).is_none() {
            self.setup_timeout(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }
        // Shutdown intent may become visible while the durable uncertainty barrier
        // is being written. The gate is still pre-prompt, so overwrite the durable
        // barrier with truthful cancelled/not_started state before writer handoff.
        if !self.accepting.load(Ordering::Acquire) {
            entry.cancel_requested.store(true, Ordering::Release);
            self.finish_pre_prompt_cancelled(&request.run_id, &entry);
            drop(prompt_gate);
            self.terminate_run_io(&mut child, &mut outbound);
            return;
        }

        // Mark possible dispatch before the writer can consume any prompt byte.
        // If the bounded queue handoff itself fails, no prompt byte was writable,
        // so restore the in-memory gate while it is still exclusively held.
        *prompt_gate = PromptDispatchGateState::PromptDispatchMayHaveOccurred;
        let prompt_pending = match outbound.start_frame(prompt_frame) {
            Ok(pending) => pending,
            Err(error) => {
                *prompt_gate = PromptDispatchGateState::PrePrompt;
                self.setup_failure(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_failed",
                    &error,
                );
                drop(prompt_gate);
                self.terminate_run_io(&mut child, &mut outbound);
                return;
            }
        };
        // The authoritative possible-dispatch boundary is the successful writer
        // handoff. Never retain this gate while waiting on ChildStdin backpressure.
        drop(prompt_gate);
        match wait_outbound_write(
            prompt_pending,
            run_deadline,
            Some(&entry.cancel_requested),
            Some(&self.accepting),
        ) {
            OutboundWriteOutcome::Written => {
                entry.update_snapshot(|snapshot| {
                    snapshot.execution_state = CodingAgentExecutionState::Started
                });
                let _ = self.persist_from_entry(
                    &request.run_id,
                    &entry,
                    DurableDispatchPhase::PromptDispatchMayHaveOccurred,
                );
            }
            OutboundWriteOutcome::Failed(_)
            | OutboundWriteOutcome::Interrupted(
                OutboundInterruption::Cancelled
                | OutboundInterruption::Shutdown
                | OutboundInterruption::Deadline,
            ) => {
                self.terminate_run_io(&mut child, &mut outbound);
                self.mark_lost(
                    &request.run_id,
                    &entry,
                    "coding_agent_prompt_write_uncertain",
                );
                return;
            }
        }

        let mut cancel_sent = false;
        let mut cancel_deadline = None;
        loop {
            if entry.cancel_requested.load(Ordering::Acquire)
                || Instant::now() >= run_deadline
                || !self.accepting.load(Ordering::Acquire)
            {
                if !cancel_sent {
                    let deadline =
                        *cancel_deadline.get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                    if !self.write_post_prompt_frame(
                        &request.run_id,
                        &entry,
                        &mut child,
                        &mut outbound,
                        notification_frame("session/cancel", json!({"sessionId":session_id})),
                        deadline,
                        false,
                        "coding_agent_cancel_write_uncertain",
                    ) {
                        return;
                    }
                    cancel_sent = true;
                }
            }
            if cancel_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                self.terminate_run_io(&mut child, &mut outbound);
                self.mark_lost(
                    &request.run_id,
                    &entry,
                    "coding_agent_cancel_terminal_missing",
                );
                return;
            }
            match rx.recv_timeout(ACP_POLL) {
                Ok(ReaderEvent::Message(message)) => {
                    if message.get("method").and_then(Value::as_str) == Some("session/update") {
                        if let Some(event) = normalize_update(&message) {
                            entry.push_event(event);
                        }
                        continue;
                    }
                    if message.get("method").and_then(Value::as_str)
                        == Some("session/request_permission")
                    {
                        let Some(id) = message.get("id").and_then(Value::as_u64) else {
                            self.terminate_run_io(&mut child, &mut outbound);
                            self.mark_lost(
                                &request.run_id,
                                &entry,
                                "coding_agent_permission_id_invalid",
                            );
                            return;
                        };
                        let params = message.get("params").cloned().unwrap_or(Value::Null);
                        if serde_json::from_value::<RequestPermissionRequest>(params.clone())
                            .is_err()
                        {
                            self.terminate_run_io(&mut child, &mut outbound);
                            self.mark_lost(
                                &request.run_id,
                                &entry,
                                "coding_agent_permission_invalid",
                            );
                            return;
                        }
                        entry.push_event(permission_event(&params));
                        entry.update_snapshot(|snapshot| {
                            snapshot.state = CodingAgentRunState::WaitingPermission
                        });
                        let permission_deadline =
                            (Instant::now() + self.permission_timeout).min(run_deadline);
                        while Instant::now() < permission_deadline
                            && !entry.cancel_requested.load(Ordering::Acquire)
                            && self.accepting.load(Ordering::Acquire)
                        {
                            thread::sleep(ACP_POLL);
                        }
                        let lifecycle_interrupted = entry.cancel_requested.load(Ordering::Acquire)
                            || Instant::now() >= run_deadline
                            || !self.accepting.load(Ordering::Acquire);
                        let response_deadline = if lifecycle_interrupted {
                            *cancel_deadline
                                .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE)
                        } else {
                            (Instant::now() + ACP_CANCEL_GRACE).min(run_deadline)
                        };
                        // P1 never selects an allow/reject option. Cancelled is
                        // the only fail-closed ACP outcome emitted by WebCodex.
                        if !self.write_post_prompt_frame(
                            &request.run_id,
                            &entry,
                            &mut child,
                            &mut outbound,
                            result_frame(id, json!({"outcome":{"outcome":"cancelled"}})),
                            response_deadline,
                            !lifecycle_interrupted,
                            "coding_agent_permission_response_uncertain",
                        ) {
                            return;
                        }
                        entry.update_snapshot(|snapshot| {
                            snapshot.state = CodingAgentRunState::Running
                        });
                        if (entry.cancel_requested.load(Ordering::Acquire)
                            || Instant::now() >= run_deadline
                            || !self.accepting.load(Ordering::Acquire))
                            && !cancel_sent
                        {
                            let deadline = *cancel_deadline
                                .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                notification_frame(
                                    "session/cancel",
                                    json!({"sessionId":session_id}),
                                ),
                                deadline,
                                false,
                                "coding_agent_cancel_write_uncertain",
                            ) {
                                return;
                            }
                            cancel_sent = true;
                        }
                        continue;
                    }
                    if message.get("method").is_some() {
                        let deadline = *cancel_deadline
                            .get_or_insert_with(|| Instant::now() + ACP_CANCEL_GRACE);
                        if let Some(id) = message.get("id").and_then(Value::as_u64) {
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                error_frame(id, -32601, "unsupported ACP client request"),
                                deadline,
                                true,
                                "coding_agent_transport_lost",
                            ) {
                                return;
                            }
                        }
                        if !cancel_sent {
                            if !self.write_post_prompt_frame(
                                &request.run_id,
                                &entry,
                                &mut child,
                                &mut outbound,
                                notification_frame(
                                    "session/cancel",
                                    json!({"sessionId":session_id}),
                                ),
                                deadline,
                                false,
                                "coding_agent_cancel_write_uncertain",
                            ) {
                                return;
                            }
                            cancel_sent = true;
                        }
                        continue;
                    }
                    if message.get("id").and_then(Value::as_u64) == Some(prompt_id) {
                        if let Some(error) = message.get("error") {
                            self.finish_failed(
                                &request.run_id,
                                &entry,
                                "prompt_error",
                                bounded_json_summary(error),
                            );
                            self.cleanup_run_io(&mut child, &mut outbound);
                            return;
                        }
                        let Some(result) = message.get("result").cloned() else {
                            self.finish_failed(
                                &request.run_id,
                                &entry,
                                "invalid_prompt_response",
                                "missing prompt result".to_string(),
                            );
                            self.cleanup_run_io(&mut child, &mut outbound);
                            return;
                        };
                        let response: PromptResponse = match serde_json::from_value(result) {
                            Ok(response) => response,
                            Err(_) => {
                                self.finish_failed(
                                    &request.run_id,
                                    &entry,
                                    "unknown_stop_reason",
                                    "invalid or unknown ACP stopReason".to_string(),
                                );
                                self.cleanup_run_io(&mut child, &mut outbound);
                                return;
                            }
                        };
                        match response.stop_reason {
                            StopReason::EndTurn => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Completed,
                                CODING_AGENT_STOP_REASON_END_TURN,
                                None,
                            ),
                            StopReason::Cancelled => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Cancelled,
                                CODING_AGENT_STOP_REASON_CANCELLED,
                                None,
                            ),
                            StopReason::MaxTokens => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_MAX_TOKENS,
                                Some("ACP turn reached max tokens"),
                            ),
                            StopReason::MaxTurnRequests => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_MAX_TURN_REQUESTS,
                                Some("ACP turn reached max requests"),
                            ),
                            StopReason::Refusal => self.finish_terminal(
                                &request.run_id,
                                &entry,
                                CodingAgentRunState::Failed,
                                CODING_AGENT_STOP_REASON_REFUSAL,
                                Some("ACP agent refused the turn"),
                            ),
                            _ => self.finish_failed(
                                &request.run_id,
                                &entry,
                                "unknown_stop_reason",
                                "unknown ACP stop reason".to_string(),
                            ),
                        }
                        self.cleanup_run_io(&mut child, &mut outbound);
                        return;
                    }
                }
                Ok(
                    ReaderEvent::Eof
                    | ReaderEvent::Malformed
                    | ReaderEvent::TooLarge
                    | ReaderEvent::Io,
                ) => {
                    self.terminate_run_io(&mut child, &mut outbound);
                    self.mark_lost(&request.run_id, &entry, "coding_agent_transport_lost");
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {
                    if child.try_wait().ok().flatten().is_some() {
                        outbound.close();
                        let _ =
                            outbound.wait_finished_until(Instant::now() + ACP_IO_CLEANUP_TIMEOUT);
                        let _ = self.worker_threads.reap_finished();
                        self.mark_lost(&request.run_id, &entry, "coding_agent_process_exited");
                        return;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate_run_io(&mut child, &mut outbound);
                    self.mark_lost(&request.run_id, &entry, "coding_agent_transport_lost");
                    return;
                }
            }
        }
    }
}
