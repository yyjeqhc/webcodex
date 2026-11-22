//! Stage-two Diagnose: one bounded ephemeral ACP session used only to project
//! the provider's live config options. The probe performs exactly
//! `initialize` then `session/new` over the same managed-child, bounded-frame
//! transport as a Coding Run, and terminates the whole process tree on every
//! exit path. It never sends `session/prompt`, `session/set_config_option`,
//! or any provider tool call, and can surface provider-side side effects such
//! as an external login prompt only as a closed failure classification.

use super::protocol::{
    bounded_setup_wait, request_frame, wait_outbound_write, wait_response, AcpFailure,
    AcpOutboundWriter, OutboundWriteOutcome, ReaderEvent,
};
use super::*;
use webcodex_core::coding_agent::{
    bounded_diagnostic_label, diagnostic_identity_key_is_bounded, CodingAgentDiagnoseOutcome,
    CodingAgentDiagnosePhase, CodingAgentDiagnoseRequest, CodingAgentLiveAcpDiagnostic,
    CodingAgentLiveConfigChoice, CodingAgentLiveConfigOption, CODING_AGENT_MAX_CONFIG_OPTIONS,
    CODING_AGENT_MAX_CONFIG_SELECT_CHOICES,
};

/// Read at most one bounded ACP frame. `read_line` would only check the
/// size after allocating an arbitrarily long provider-controlled line.
/// A limited BufRead view enforces the byte cap before parsing or allocation.
pub(super) fn read_bounded_diagnostic_frame<R: BufRead>(reader: &mut R) -> Option<ReaderEvent> {
    let mut frame = Vec::new();
    let event = reader
        .take((ACP_MESSAGE_MAX_BYTES + 1) as u64)
        .read_until(b'\n', &mut frame);
    match event {
        Ok(0) => None,
        Ok(size) if size > ACP_MESSAGE_MAX_BYTES => Some(ReaderEvent::TooLarge),
        Ok(_) => Some(
            serde_json::from_slice::<Value>(&frame)
                .map(ReaderEvent::Message)
                .unwrap_or(ReaderEvent::Malformed),
        ),
        Err(error) => Some(ReaderEvent::Io(error.kind())),
    }
}

/// Owns the ephemeral probe process and its sole outbound writer. Dropping it
/// terminates the whole process tree and drains the I/O owners, so success,
/// fault, timeout, and Runner shutdown all clean up identically.
struct EphemeralAcpProbe {
    child: ManagedChild,
    outbound: AcpOutboundWriter,
    responses: mpsc::Receiver<ReaderEvent>,
}

impl EphemeralAcpProbe {
    fn spawn(
        threads: &BackgroundThreads,
        config: &AcpAgentConfig,
        environment: &[(String, std::ffi::OsString)],
        project_root: &str,
    ) -> Result<Self, AcpFailure> {
        let mut command = Command::new(&config.executable);
        command.args(&config.args).env_clear();
        for (key, value) in environment {
            command.env(key, value);
        }
        command.current_dir(project_root);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = ManagedChild::spawn_background(&mut command)
            .map_err(|error| AcpFailure::SpawnIo(error.kind()))?;
        let stdin = child
            .child_mut()
            .stdin
            .take()
            .ok_or(AcpFailure::WriterUnavailable)?;
        let stdout = child
            .child_mut()
            .stdout
            .take()
            .ok_or(AcpFailure::WorkerIo(std::io::ErrorKind::BrokenPipe))?;
        // Provider stderr is drained to a sink and never captured; it can
        // never become a diagnostic result, error, or log projection.
        if let Some(stderr) = child.child_mut().stderr.take() {
            let _ = thread::Builder::new()
                .name("wc-acp-diag-stderr".to_string())
                .spawn(move || {
                    let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
                });
        }
        let outbound = AcpOutboundWriter::spawn(stdin, threads)
            .map_err(|error| AcpFailure::WorkerIo(error.kind()))?;
        let (tx, responses) = mpsc::sync_channel(32);
        let _reader = thread::Builder::new()
            .name("wc-acp-diag-stdout".to_string())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_bounded_diagnostic_frame(&mut reader) {
                        Some(ReaderEvent::Message(value)) => {
                            if tx.send(ReaderEvent::Message(value)).is_err() {
                                break;
                            }
                        }
                        Some(failure) => {
                            let _ = tx.send(failure);
                            break;
                        }
                        None => {
                            let _ = tx.send(ReaderEvent::Eof);
                            break;
                        }
                    }
                }
            })
            .map_err(|error| AcpFailure::WorkerIo(error.kind()))?;
        Ok(Self {
            child,
            outbound,
            responses,
        })
    }
}

impl Drop for EphemeralAcpProbe {
    fn drop(&mut self) {
        let _ = self.child.terminate_tree();
        self.outbound.close();
        let deadline = Instant::now() + ACP_IO_CLEANUP_TIMEOUT;
        let _ = self.outbound.wait_finished_until(deadline);
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            let _ = self.child.wait_tree_exit(remaining);
        }
        let _ = self.child.try_wait();
    }
}

/// Closed mapping from the normalized Runner-private transport fault to the
/// wire outcome vocabulary. The fault value itself never crosses the boundary.
fn classify_probe_failure(failure: AcpFailure) -> CodingAgentDiagnoseOutcome {
    match failure {
        AcpFailure::TimedOut => CodingAgentDiagnoseOutcome::Timeout,
        AcpFailure::Interrupted => CodingAgentDiagnoseOutcome::Interrupted,
        AcpFailure::RpcRejected(_) => CodingAgentDiagnoseOutcome::Rejected,
        AcpFailure::ProcessExited => CodingAgentDiagnoseOutcome::ProcessExited,
        _ => CodingAgentDiagnoseOutcome::TransportFailed,
    }
}

fn probe_stage(phase: CodingAgentDiagnosePhase) -> &'static str {
    match phase {
        CodingAgentDiagnosePhase::Environment => "acp_diagnose_environment",
        CodingAgentDiagnosePhase::Spawn => "acp_diagnose_spawn",
        CodingAgentDiagnosePhase::Initialize => "acp_diagnose_initialize",
        CodingAgentDiagnosePhase::SessionNew => "acp_diagnose_session_new",
        CodingAgentDiagnosePhase::Projection => "acp_diagnose_projection",
    }
}

/// Empty, classified stop result. Each fault site normalizes its own cause
/// with [`AcpFailure::diagnose`] (which upgrades observed process exits) into
/// the Runner trace log; the wire response carries only the closed
/// outcome/phase pair.
fn probe_stop(
    request: &CodingAgentDiagnoseRequest,
    outcome: CodingAgentDiagnoseOutcome,
    phase: CodingAgentDiagnosePhase,
) -> CodingAgentLiveAcpDiagnostic {
    tracing::warn!(
        run_id = %request.run_id,
        stage = probe_stage(phase),
        ?outcome,
        "ACP live probe stopped"
    );
    CodingAgentLiveAcpDiagnostic {
        outcome,
        phase,
        protocol_version: None,
        options: Vec::new(),
        options_truncated: false,
    }
}

/// Project advertised options into the closed bounded identity view. Options
/// that exceed the budgets or fall outside the bounded identity vocabulary are
/// dropped and reported through the truncated flags rather than widening the
/// response contract.
pub(super) fn project_live_options(
    advertised: Vec<agent_client_protocol_schema::v1::SessionConfigOption>,
) -> (Vec<CodingAgentLiveConfigOption>, bool) {
    let mut truncated = false;
    let mut projected = Vec::new();
    for option in advertised {
        if projected.len() >= CODING_AGENT_MAX_CONFIG_OPTIONS {
            truncated = true;
            break;
        }
        let id = option.id.to_string();
        if !diagnostic_identity_key_is_bounded(&id) {
            truncated = true;
            continue;
        }
        let label = bounded_diagnostic_label(&option.name);
        let label = (!label.is_empty()).then_some(label);
        match option.kind {
            SessionConfigKind::Select(select) => {
                let advertised_choices = match select.options {
                    SessionConfigSelectOptions::Ungrouped(options) => options,
                    SessionConfigSelectOptions::Grouped(groups) => {
                        groups.into_iter().flat_map(|group| group.options).collect()
                    }
                    // Without an understood advertised choice list, a
                    // provider-controlled current value is not verified as a
                    // safe choice identifier. Never project it independently.
                    _ => {
                        projected.push(CodingAgentLiveConfigOption {
                            id,
                            label,
                            kind: "select".into(),
                            current: None,
                            choices: Vec::new(),
                            choices_truncated: true,
                        });
                        continue;
                    }
                };
                let mut choices = Vec::new();
                let mut choices_truncated = false;
                for choice in advertised_choices {
                    if choices.len() >= CODING_AGENT_MAX_CONFIG_SELECT_CHOICES {
                        choices_truncated = true;
                        break;
                    }
                    let value = choice.value.to_string();
                    if !diagnostic_identity_key_is_bounded(&value) {
                        choices_truncated = true;
                        continue;
                    }
                    let choice_label = bounded_diagnostic_label(&choice.name);
                    choices.push(CodingAgentLiveConfigChoice {
                        value,
                        label: (!choice_label.is_empty()).then_some(choice_label),
                    });
                }
                let current = select.current_value.to_string();
                // An arbitrary currentValue need not be among the provider's
                // selection IDs; it could contain a private opaque value.
                // Only repeat IDs that this projection also safely exposes.
                let current = choices
                    .iter()
                    .any(|choice| choice.value == current)
                    .then_some(current);
                projected.push(CodingAgentLiveConfigOption {
                    id,
                    label,
                    kind: "select".into(),
                    current,
                    choices,
                    choices_truncated,
                });
            }
            SessionConfigKind::Boolean(boolean) => {
                projected.push(CodingAgentLiveConfigOption {
                    id,
                    label,
                    kind: "boolean".into(),
                    current: Some(boolean.current_value.to_string()),
                    choices: Vec::new(),
                    choices_truncated: false,
                });
            }
            // A future ACP option kind is reported as incomplete projection
            // rather than guessed into the select/boolean vocabulary.
            _ => {
                truncated = true;
            }
        }
    }
    (projected, truncated)
}

impl CodingAgentManager {
    /// One bounded window covers the whole ephemeral session, so a hung
    /// provider cannot hold a diagnostic (or its process) past the budget.
    fn live_probe_budget(&self) -> Duration {
        #[cfg(test)]
        if let Some(budget) = *self.probe_test_budget.lock().unwrap() {
            return budget;
        }
        ACP_SETUP_TIMEOUT
    }

    /// Stage-two ACP readiness probe. Launches the provider exactly as a
    /// Coding Run would (explicit `env_from_env` resolution, authorized
    /// Project cwd), negotiates `initialize` then `session/new`, and projects
    /// the live advertised options. Provider startup may have external side
    /// effects, which is why this rides only the explicitly authorized
    /// Diagnose request.
    pub(super) fn run_live_acp_probe(
        &self,
        request: &CodingAgentDiagnoseRequest,
        provider: &ProviderEntry,
    ) -> CodingAgentLiveAcpDiagnostic {
        // One live probe at a time per Runner keeps the ephemeral process
        // population bounded even under concurrent diagnostic requests.
        // Never queue a launch for an unbounded time outside its work budget.
        // A busy Runner returns a closed diagnostic rather than building a
        // queue of waiting requests and potentially starting processes later.
        let _probe_permit = match self.live_probe_permit.try_lock() {
            Ok(permit) => permit,
            Err(std::sync::TryLockError::WouldBlock) => {
                return probe_stop(
                    request,
                    CodingAgentDiagnoseOutcome::Busy,
                    CodingAgentDiagnosePhase::Spawn,
                );
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return probe_stop(
                    request,
                    CodingAgentDiagnoseOutcome::Interrupted,
                    CodingAgentDiagnosePhase::Spawn,
                );
            }
        };
        let deadline = Instant::now() + self.live_probe_budget();
        let environment = match resolve_environment(&provider.config) {
            Ok(environment) => environment,
            Err(_) => {
                return probe_stop(
                    request,
                    CodingAgentDiagnoseOutcome::EnvUnavailable,
                    CodingAgentDiagnosePhase::Environment,
                );
            }
        };
        let mut probe = match EphemeralAcpProbe::spawn(
            &self.worker_threads,
            &provider.config,
            &environment,
            &request.project_root,
        ) {
            Ok(probe) => probe,
            Err(failure) => {
                let failure = failure.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::Spawn),
                    None,
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::Spawn,
                );
            }
        };
        // wait_response interprets a true flag as cancellation and the probe
        // has no cancel flag, so Runner shutdown is honored by explicit checks
        // between the two bounded setup RPCs instead.
        if !self.accepting.load(Ordering::Acquire) {
            return probe_stop(
                request,
                CodingAgentDiagnoseOutcome::Interrupted,
                CodingAgentDiagnosePhase::Spawn,
            );
        }
        let initialize_frame = match request_frame(
            1,
            "initialize",
            json!({
                "protocolVersion": 1,
                "clientCapabilities": {},
                "clientInfo": {"name":"webcodex-runner","version":env!("CARGO_PKG_VERSION")}
            }),
        ) {
            Ok(frame) => frame,
            Err(_) => {
                let failure = AcpFailure::InvalidFrame.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::Initialize),
                    None,
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::Initialize,
                );
            }
        };
        let mut child_status = || probe.child.try_wait().ok().flatten();
        let pending = match probe.outbound.start_frame(initialize_frame) {
            Ok(pending) => pending,
            Err(failure) => {
                let failure = failure.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::Initialize),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::Initialize,
                );
            }
        };
        match wait_outbound_write(pending, deadline, None, Some(&self.accepting)) {
            OutboundWriteOutcome::Written => {}
            outcome => {
                let failure = outcome.failure().expect("non-written outcome").diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::Initialize),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::Initialize,
                );
            }
        }
        let Some(initialize_wait) = bounded_setup_wait(deadline) else {
            return probe_stop(
                request,
                CodingAgentDiagnoseOutcome::Timeout,
                CodingAgentDiagnosePhase::Initialize,
            );
        };
        let initialize = match wait_response(&probe.responses, 1, initialize_wait, None) {
            Ok(value) => value,
            Err(failure) => {
                let failure = failure.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::Initialize),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::Initialize,
                );
            }
        };
        if initialize.get("protocolVersion").and_then(Value::as_u64) != Some(1) {
            return probe_stop(
                request,
                CodingAgentDiagnoseOutcome::ProtocolUnsupported,
                CodingAgentDiagnosePhase::Initialize,
            );
        }
        if !self.accepting.load(Ordering::Acquire) {
            return probe_stop(
                request,
                CodingAgentDiagnoseOutcome::Interrupted,
                CodingAgentDiagnosePhase::Initialize,
            );
        }
        let new_session_frame = match request_frame(
            2,
            "session/new",
            json!({
                "cwd": request.project_root,
                "mcpServers": []
            }),
        ) {
            Ok(frame) => frame,
            Err(_) => {
                let failure = AcpFailure::InvalidFrame.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::SessionNew),
                    None,
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::SessionNew,
                );
            }
        };
        let pending = match probe.outbound.start_frame(new_session_frame) {
            Ok(pending) => pending,
            Err(failure) => {
                let failure = failure.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::SessionNew),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::SessionNew,
                );
            }
        };
        match wait_outbound_write(pending, deadline, None, Some(&self.accepting)) {
            OutboundWriteOutcome::Written => {}
            outcome => {
                let failure = outcome.failure().expect("non-written outcome").diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::SessionNew),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::SessionNew,
                );
            }
        }
        let Some(session_new_wait) = bounded_setup_wait(deadline) else {
            return probe_stop(
                request,
                CodingAgentDiagnoseOutcome::Timeout,
                CodingAgentDiagnosePhase::SessionNew,
            );
        };
        let new_value = match wait_response(&probe.responses, 2, session_new_wait, None) {
            Ok(value) => value,
            Err(failure) => {
                let failure = failure.diagnose(
                    &request.run_id,
                    probe_stage(CodingAgentDiagnosePhase::SessionNew),
                    child_status(),
                );
                return probe_stop(
                    request,
                    classify_probe_failure(failure),
                    CodingAgentDiagnosePhase::SessionNew,
                );
            }
        };
        let new_session: NewSessionResponse = match serde_json::from_value(new_value) {
            Ok(response) => response,
            Err(_) => {
                return probe_stop(
                    request,
                    CodingAgentDiagnoseOutcome::InvalidResponse,
                    CodingAgentDiagnosePhase::SessionNew,
                );
            }
        };
        let (options, options_truncated) =
            project_live_options(new_session.config_options.unwrap_or_default());
        CodingAgentLiveAcpDiagnostic {
            outcome: CodingAgentDiagnoseOutcome::Ok,
            phase: CodingAgentDiagnosePhase::Projection,
            protocol_version: Some(1),
            options,
            options_truncated,
        }
    }
}
