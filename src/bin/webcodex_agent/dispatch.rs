use super::lsp::{handle_lsp_request, is_lsp_request_kind, LspSupervisor};
use super::validation::{handle_validation_request, is_validation_request_kind};
use super::{handle_project_op, run_shell_with_profiles, AgentPolicy, AgentSink, ShellConfig};
use crate::shell_protocol::ShellAgentShellRequest;
use crate::{handle_file_request, is_file_request_kind, JobManager};
use std::path::Path;

/// Execute a single agent request (shell/file/job/lsp/validation) and send the
/// result over the active transport. This is the shared dispatch path used by
/// both the polling loop (`handle_one_poll`) and the WebSocket loop. It contains
/// no transport-specific code: all outgoing traffic goes through `sink`.
pub(crate) fn dispatch_request(
    sink: &AgentSink,
    policy: &AgentPolicy,
    shell: &ShellConfig,
    jobs: &JobManager,
    projects_dir: &Path,
    lsp: &LspSupervisor,
    request: ShellAgentShellRequest,
) -> Result<bool, String> {
    match request.kind.as_str() {
        "start_job" | "start_validation_job" => {
            jobs.enqueue(
                sink.clone(),
                policy.clone(),
                shell.clone(),
                projects_dir.to_path_buf(),
                request,
            );
            Ok(true)
        }
        "stop_job" => {
            if let Some(job_id) = request.job_id.as_deref() {
                if let Err(e) = jobs.stop(job_id) {
                    eprintln!("webcodex-agent stop_job error: {}", e);
                }
            }
            Ok(true)
        }
        kind if is_file_request_kind(kind) => {
            let request_id = request.request_id.clone();
            let result = handle_file_request(policy, &request);
            sink.submit_result(request_id, result)
        }
        "register_project" | "create_project" => {
            let request_id = request.request_id.clone();
            let result = handle_project_op(policy, projects_dir, &request);
            sink.submit_result(request_id, result)
        }
        kind if is_lsp_request_kind(kind) => {
            // Explicit LSP branch — must never fall through to shell execution.
            let request_id = request.request_id.clone();
            let result = handle_lsp_request(policy, projects_dir, lsp, &request);
            sink.submit_result(request_id, result)
        }
        kind if is_validation_request_kind(kind) => {
            // Explicit validation bridge branch — never fall through to shell.
            let request_id = request.request_id.clone();
            let result = handle_validation_request(policy, projects_dir, &request);
            sink.submit_result(request_id, result)
        }
        _ => {
            let request_id = request.request_id.clone();
            let result = run_shell_with_profiles(
                policy,
                shell,
                projects_dir,
                &jobs.prepared_profiles,
                request.cwd.as_deref(),
                &request.command,
                request.stdin.as_deref(),
                request.timeout_secs,
                None,
            );
            sink.submit_result(request_id, result)
        }
    }
}

pub(crate) fn is_project_op(kind: &str) -> bool {
    kind == "register_project" || kind == "create_project"
}
