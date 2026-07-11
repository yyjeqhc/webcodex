//! Agent-side validation bridge: registry, execution, and adapters.
//!
//! Server sends declarative `ValidationBridgeRequest` values. The agent
//! resolves `adapter_id`, discovers the executable, builds argv, runs the tool
//! with bounded capture, parses structured output, relativizes paths, and
//! returns a sanitized `ValidationBridgeResponse`. No arbitrary shell commands
//! cross the bridge.

mod execute;
mod path;
mod pyright;
mod registry;

#[cfg(test)]
pub(crate) use registry::{adapter_metadata, registered_adapter_ids};

use super::config::AgentPolicy;
use super::output::CommandResult;
use super::projects::load_agent_project_summaries_from_dir;
use super::shell::cwd_allowed;
use crate::shell_protocol::ShellAgentShellRequest;
use crate::validation_bridge::{
    failure_kinds, validate_bridge_request, ValidationBridgeRequest, ValidationBridgeResponse,
    ValidationBridgeResultEnvelope, AGENT_VALIDATION_REQUEST_KIND,
    VALIDATION_BRIDGE_PROTOCOL_VERSION,
};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(crate) fn is_validation_request_kind(kind: &str) -> bool {
    kind == AGENT_VALIDATION_REQUEST_KIND
}

pub(crate) fn handle_validation_request(
    policy: &AgentPolicy,
    projects_dir: &Path,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let start = Instant::now();
    let Some(payload) = request.validation.as_ref() else {
        return validation_error_cmd(
            start,
            failure_kinds::MISSING_VALIDATION_PAYLOAD,
            "validation request missing typed payload",
        );
    };
    match execute_validation(policy, projects_dir, payload) {
        Ok(response) => {
            let envelope = ValidationBridgeResultEnvelope::ok(response);
            CommandResult {
                exit_code: Some(0),
                stdout: Some(envelope.to_stdout_json()),
                stderr: Some(String::new()),
                duration_ms: Some(start.elapsed().as_millis() as u64),
                error: None,
            }
        }
        Err(envelope) => CommandResult {
            exit_code: Some(0),
            stdout: Some(envelope.to_stdout_json()),
            stderr: Some(String::new()),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
    }
}

/// Internal entry used by tests and dispatch. Runs a typed validation request
/// against a resolved agent project.
pub(crate) fn execute_validation(
    policy: &AgentPolicy,
    projects_dir: &Path,
    request: &ValidationBridgeRequest,
) -> Result<ValidationBridgeResponse, ValidationBridgeResultEnvelope> {
    if let Err(message) = validate_bridge_request(request) {
        return Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::INVALID_ARGUMENTS,
            message,
        ));
    }

    let meta = registry::lookup_adapter(&request.adapter_id).ok_or_else(|| {
        ValidationBridgeResultEnvelope::err(
            failure_kinds::ADAPTER_NOT_FOUND,
            format!("unknown validation adapter '{}'", request.adapter_id),
        )
    })?;

    if meta.language != request.language {
        return Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::LANGUAGE_ADAPTER_MISMATCH,
            format!(
                "adapter '{}' serves language '{}', not '{}'",
                meta.adapter_id, meta.language, request.language
            ),
        ));
    }
    if meta.validation_kind != request.validation_kind {
        return Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::LANGUAGE_ADAPTER_MISMATCH,
            format!(
                "adapter '{}' serves kind '{}', not '{}'",
                meta.adapter_id, meta.validation_kind, request.validation_kind
            ),
        ));
    }

    let project = resolve_agent_project(projects_dir, &request.project_id)?;
    let project_root = validate_project_root(policy, &project)?;

    match meta.adapter_id {
        "pyright" => Ok(pyright::run_pyright(
            &project_root,
            request,
            policy.max_timeout_secs,
        )),
        other => Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::ADAPTER_NOT_FOUND,
            format!("adapter '{other}' is registered but not executable in this build"),
        )),
    }
}

/// Direct internal entry for unit/e2e tests that already have a project root.
#[cfg(test)]
pub(crate) fn execute_validation_at_root(
    project_root: &Path,
    request: &ValidationBridgeRequest,
    max_timeout_secs: u64,
) -> Result<ValidationBridgeResponse, ValidationBridgeResultEnvelope> {
    if let Err(message) = validate_bridge_request(request) {
        return Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::INVALID_ARGUMENTS,
            message,
        ));
    }
    let meta = registry::lookup_adapter(&request.adapter_id).ok_or_else(|| {
        ValidationBridgeResultEnvelope::err(
            failure_kinds::ADAPTER_NOT_FOUND,
            format!("unknown validation adapter '{}'", request.adapter_id),
        )
    })?;
    if meta.language != request.language || meta.validation_kind != request.validation_kind {
        return Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::LANGUAGE_ADAPTER_MISMATCH,
            "language/kind does not match adapter",
        ));
    }
    match meta.adapter_id {
        "pyright" => Ok(pyright::run_pyright(
            project_root,
            request,
            max_timeout_secs,
        )),
        other => Err(ValidationBridgeResultEnvelope::err(
            failure_kinds::ADAPTER_NOT_FOUND,
            format!("adapter '{other}' is not executable"),
        )),
    }
}

fn resolve_agent_project(
    projects_dir: &Path,
    project_id: &str,
) -> Result<PathBuf, ValidationBridgeResultEnvelope> {
    let projects = load_agent_project_summaries_from_dir(projects_dir);
    let project = projects
        .into_iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| {
            ValidationBridgeResultEnvelope::err(
                failure_kinds::UNKNOWN_PROJECT,
                "unknown agent project",
            )
        })?;
    Ok(PathBuf::from(project.path))
}

fn validate_project_root(
    policy: &AgentPolicy,
    path: &Path,
) -> Result<PathBuf, ValidationBridgeResultEnvelope> {
    cwd_allowed(policy, path).map_err(|message| {
        ValidationBridgeResultEnvelope::err(failure_kinds::INVALID_PROJECT_PATH, message)
    })?;
    std::fs::canonicalize(path).map_err(|_| {
        ValidationBridgeResultEnvelope::err(
            failure_kinds::INVALID_PROJECT_PATH,
            "project root is not accessible",
        )
    })
}

fn validation_error_cmd(start: Instant, code: &str, message: &str) -> CommandResult {
    let envelope = ValidationBridgeResultEnvelope::err(code, message);
    CommandResult {
        exit_code: Some(0),
        stdout: Some(envelope.to_stdout_json()),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

/// Empty response skeleton used by adapters for early failures.
pub(crate) fn base_response(
    request: &ValidationBridgeRequest,
    tool_available: bool,
) -> ValidationBridgeResponse {
    ValidationBridgeResponse {
        protocol_version: VALIDATION_BRIDGE_PROTOCOL_VERSION,
        adapter_id: request.adapter_id.clone(),
        language: request.language.clone(),
        validation_kind: request.validation_kind.clone(),
        success: false,
        command_started: false,
        exit_code: None,
        duration_ms: 0,
        failure_kind: None,
        diagnostics: None,
        tool_available,
        stdout_bytes: 0,
        stdout_capped: false,
        stderr_capped: false,
        stderr_summary: None,
        message: None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
