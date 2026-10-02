//! Runner adapter for the native read-only LSP implementation.
//!
//! Runner-owned Project registration and filesystem policy are resolved here.
//! `webcodex-lsp` receives only an authorized canonical Project root and the
//! existing typed LSP payload.

use super::super::config::RunnerPolicy;
use super::super::output::CommandResult;
use super::super::projects::load_runner_project_summaries_from_dir;
use super::super::shell::cwd_allowed;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[cfg(test)]
use webcodex_core::lsp_bridge::AGENT_LSP_REQUEST_KIND;
use webcodex_core::lsp_bridge::{
    bound_error_message, error_codes, RunnerLspPayload, RunnerLspResultEnvelope,
};
#[cfg(test)]
use webcodex_core::runner_protocol::RunnerRequest;
use webcodex_lsp::{execute_lsp_operation, LspSupervisor};

#[cfg(test)]
pub(crate) fn is_lsp_request_kind(kind: &str) -> bool {
    kind == AGENT_LSP_REQUEST_KIND
}

pub(crate) fn handle_lsp_operation(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    supervisor: &LspSupervisor,
    payload: &RunnerLspPayload,
    timeout_secs: u64,
) -> CommandResult {
    let start = Instant::now();
    let operation_deadline = start
        .checked_add(Duration::from_secs(timeout_secs.max(1)))
        .unwrap_or(start);
    let envelope = match resolve_runner_project(project_registry_dir, &payload.project_id)
        .and_then(|path| validate_project_root(policy, &path))
    {
        Ok(project_root) => {
            execute_lsp_operation(project_root, supervisor, payload, operation_deadline)
        }
        Err(envelope) => envelope,
    };
    command_result(start, envelope)
}

#[cfg(test)]
pub(crate) fn handle_lsp_request(
    policy: &RunnerPolicy,
    project_registry_dir: &Path,
    supervisor: &LspSupervisor,
    request: &RunnerRequest,
) -> CommandResult {
    let Some(payload) = request.lsp.as_ref() else {
        return command_result(
            Instant::now(),
            RunnerLspResultEnvelope::err(
                error_codes::MISSING_LSP_PAYLOAD,
                "LSP request missing typed payload",
            ),
        );
    };
    handle_lsp_operation(
        policy,
        project_registry_dir,
        supervisor,
        payload,
        request.timeout_secs,
    )
}

fn resolve_runner_project(
    project_registry_dir: &Path,
    project_id: &str,
) -> Result<PathBuf, RunnerLspResultEnvelope> {
    let id = project_id.trim();
    if id.is_empty() {
        return Err(RunnerLspResultEnvelope::err(
            error_codes::UNKNOWN_PROJECT,
            "project_id cannot be empty",
        ));
    }
    load_runner_project_summaries_from_dir(project_registry_dir)
        .into_iter()
        .find(|project| project.id == id)
        .map(|project| PathBuf::from(project.path))
        .ok_or_else(|| {
            RunnerLspResultEnvelope::err(error_codes::UNKNOWN_PROJECT, "unknown agent project")
        })
}

fn validate_project_root(
    policy: &RunnerPolicy,
    path: &Path,
) -> Result<PathBuf, RunnerLspResultEnvelope> {
    cwd_allowed(policy, path).map_err(|message| {
        RunnerLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            bound_error_message(message),
        )
    })?;
    fs::canonicalize(path).map_err(|_| {
        RunnerLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "project root is not accessible",
        )
    })
}

fn command_result(start: Instant, envelope: RunnerLspResultEnvelope) -> CommandResult {
    CommandResult {
        // Structured LSP success/failure lives in the versioned envelope, not
        // the native-process exit status.
        exit_code: Some(0),
        stdout: Some(envelope.to_stdout_json()),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}
