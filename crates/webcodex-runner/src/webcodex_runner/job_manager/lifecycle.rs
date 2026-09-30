//! Pure Job lifecycle and activity projections across execution boundaries.

use super::*;
pub(super) fn process_running_activity() -> ShellJobActivity {
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase: ShellJobActivityPhase::ProcessRunning,
        source: ShellJobActivitySource::RunnerExecution,
    }
}

pub(super) fn validation_step_activity(step: &ShellJobValidationStep) -> ShellJobActivity {
    let phase = match step.name.as_str() {
        "format" => ShellJobActivityPhase::ValidationFormat,
        "check" => ShellJobActivityPhase::ValidationCheck,
        "test" => ShellJobActivityPhase::ValidationTest,
        _ => unreachable!("canonical validation step name"),
    };
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase,
        source: ShellJobActivitySource::ValidationPlan,
    }
}

/// Recognize only a tiny bounded subset of Cargo's own stderr progress while a
/// canonical structured Cargo validation step is running. Clear phase-boundary
/// lines return to the step's canonical validation-plan activity so transient
/// Cargo detail cannot remain sticky after that detail has ended. This is
/// advisory activity provenance, not validation/completion evidence.
pub(super) fn cargo_activity_from_stderr(
    step: &ShellJobValidationStep,
    stderr: &str,
) -> Option<ShellJobActivity> {
    if step.program != "cargo" || !step.is_canonical() {
        return None;
    }
    let validation_activity = validation_step_activity(step);
    let mut observed = None;
    for line in stderr.lines() {
        let line = line.trim_start();
        let activity = if line.contains("Blocking waiting for file lock on build directory") {
            ShellJobActivity {
                state: ShellJobActivityState::Waiting,
                phase: ShellJobActivityPhase::CargoWaitingForBuildLock,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Compiling ") {
            ShellJobActivity {
                state: ShellJobActivityState::Working,
                phase: ShellJobActivityPhase::CargoCompiling,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Checking ") {
            ShellJobActivity {
                state: ShellJobActivityState::Working,
                phase: ShellJobActivityPhase::CargoChecking,
                source: ShellJobActivitySource::CargoOutput,
            }
        } else if line.starts_with("Finished ")
            || (step.name == "test"
                && (line.starts_with("Running unittests ")
                    || line.starts_with("Running tests/")
                    || line.starts_with("Running benches/")
                    || line.starts_with("Doc-tests ")))
        {
            validation_activity
        } else {
            continue;
        };
        observed = Some(activity);
    }
    observed
}

pub(super) fn runner_job_is_terminal(status: &str) -> bool {
    RunnerJobLifecycle::from_wire(status).is_ok_and(RunnerJobLifecycle::is_terminal)
}

pub(super) fn runner_job_is_active(status: &str) -> bool {
    RunnerJobLifecycle::from_wire(status).is_ok_and(RunnerJobLifecycle::is_runner_active)
}

pub(super) fn job_prestart_lifecycle(
    operation: &RunnerJobOperation,
) -> Option<ShellCommandExecutionState> {
    match operation {
        RunnerJobOperation::StartShell(_)
        | RunnerJobOperation::StartBuild(_)
        | RunnerJobOperation::StartProcess(_)
        | RunnerJobOperation::StartInteractiveProcess(_)
        | RunnerJobOperation::StartDetachedProcess(_)
        | RunnerJobOperation::StartScript(_)
        | RunnerJobOperation::StartSkillResource(_) => Some(ShellCommandExecutionState::NotStarted),
        RunnerJobOperation::StartValidation(operation)
            if operation
                .context
                .validation
                .as_ref()
                .is_some_and(|metadata| metadata.project_validation.is_some()) =>
        {
            Some(ShellCommandExecutionState::NotStarted)
        }
        RunnerJobOperation::StartValidation(_) | RunnerJobOperation::Stop { .. } => None,
    }
}

/// Compatibility-only lifecycle projection for malformed V2 Job requests that
/// fail before a canonical operation can be constructed. Production Job
/// execution never uses this string registry.
pub(crate) fn decode_failure_prestart_lifecycle(
    request: &RunnerRequest,
) -> Option<ShellCommandExecutionState> {
    matches!(
        request.kind.as_str(),
        "start_job"
            | "start_build_job"
            | "start_process_job"
            | "start_interactive_process_job"
            | "start_detached_process_job"
            | "start_script_job"
            | "start_skill_resource_job"
    )
    .then_some(ShellCommandExecutionState::NotStarted)
}

pub(super) fn post_spawn_interruption_lifecycle(
    operation: &RunnerJobOperation,
) -> Option<ShellCommandExecutionState> {
    matches!(
        operation,
        RunnerJobOperation::StartShell(_) | RunnerJobOperation::StartInteractiveProcess(_)
    )
    .then_some(ShellCommandExecutionState::OutcomeUnknown)
}

pub(super) fn post_spawn_interruption_reason(
    shutting_down: bool,
    stop_requested: bool,
    job_record_present: bool,
) -> Option<&'static str> {
    if shutting_down {
        Some("runner began shutdown after command start")
    } else if stop_requested {
        Some("job stop requested after command start")
    } else if !job_record_present {
        Some("runner lost the Job record after command start")
    } else {
        None
    }
}

pub(super) fn post_spawn_interruption_delta(
    operation: &RunnerJobOperation,
    duration_ms: u64,
    error: &str,
) -> RunnerJobDelta {
    RunnerJobDelta {
        status: "failed".to_string(),
        exit_code: None,
        duration_ms: Some(duration_ms),
        error: Some(error.to_string()),
        command_execution_state: post_spawn_interruption_lifecycle(operation),
        finished: true,
        ..Default::default()
    }
}

pub(super) fn raw_shell_job_terminal_lifecycle(
    status: &str,
    exit_code: Option<i32>,
) -> ShellCommandExecutionState {
    match RunnerJobLifecycle::from_wire(status).ok() {
        Some(lifecycle) if lifecycle.is_timed_out() => ShellCommandExecutionState::TimedOut,
        Some(
            RunnerJobLifecycle::Completed
            | RunnerJobLifecycle::Stopped
            | RunnerJobLifecycle::Cancelled,
        ) => ShellCommandExecutionState::Completed,
        Some(RunnerJobLifecycle::Failed) if exit_code.is_some() => {
            ShellCommandExecutionState::Completed
        }
        _ => ShellCommandExecutionState::OutcomeUnknown,
    }
}
