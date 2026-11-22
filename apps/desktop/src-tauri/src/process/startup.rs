use crate::error::{DesktopError, ErrorComponent, RecoveryKind, SafeRecoveryAction};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub(crate) struct StartupDiagnostics(pub(super) Arc<Mutex<VecDeque<String>>>);

pub(crate) fn server_start_error(
    exit_code: Option<i32>,
    diagnostics: Option<&StartupDiagnostics>,
) -> DesktopError {
    let mut message = "The Desktop-owned WebCodex Server exited during startup".to_string();
    if let Some(code) = exit_code {
        message.push_str(&format!(" (exit code {code})"));
    }
    let mut listener_bind_failed = false;
    if let Some(diagnostics) = diagnostics {
        let logs = diagnostics
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // Do not expose arbitrary child output: it can contain credentials even
        // after the activity log's prefix-based redaction. Only reconstruct the
        // known listener failure from a validated socket address and OS code.
        if let Some(summary) = logs.iter().rev().find_map(|line| listener_failure(line)) {
            listener_bind_failed = true;
            message.push_str(". ");
            message.push_str(&summary);
        }
    }
    let mut error = DesktopError::new(
        "server_start_failed",
        message,
        if listener_bind_failed {
            "Check whether another program is already using the local Server port, then retry."
        } else {
            "Review the error details and Activity, then retry."
        },
    )
    .with_component(ErrorComponent::Server)
    .with_stage("server_start");
    if let Some(code) = exit_code {
        error = error.with_exit_code(code);
    }
    if listener_bind_failed {
        error = error
            .with_reason_code("local_port_unavailable")
            .with_recovery(
                RecoveryKind::UserAction,
                SafeRecoveryAction::OpenDiagnostics,
                false,
            );
    }
    error
}

fn listener_failure(line: &str) -> Option<String> {
    let (_, failure) = line.split_once("failed to bind HTTP listener ")?;
    let (address, cause) = failure.split_once(": ")?;
    let address = address.parse::<SocketAddr>().ok()?;
    let mut summary = format!("failed to bind HTTP listener {address}");
    if let Some((_, code)) = cause.split_once("(os error ") {
        if let Some((code, _)) = code.split_once(')') {
            if let Ok(code) = code.parse::<i32>() {
                summary.push_str(&format!(" (os error {code})"));
            }
        }
    }
    Some(summary)
}
