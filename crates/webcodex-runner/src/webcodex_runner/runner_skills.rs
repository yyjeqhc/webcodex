use super::config::SkillsConfig;
use super::configured_skills;
use super::output::{CommandResult, ShellCommandResult};
#[cfg(windows)]
use super::shell::prepare_detached_process_launch;
use super::shell::{
    run_process_with_profiles_and_execution_state_with_start_hook, PreparedShellProfileCache,
};
use super::skill_store::SkillStore;
use super::{RunnerPolicy, ShellConfig};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;
use webcodex_core::runner_protocol::{
    PROCESS_ARGV_MAX_BYTES, PROCESS_ARG_MAX_BYTES, PROCESS_ARG_MAX_COUNT,
    PROCESS_EXECUTABLE_MAX_BYTES,
};
use webcodex_core::runner_skill::{
    RunnerSkillDescriptor, RunnerSkillExecutionRequest, RunnerSkillListResponse,
    RunnerSkillReadResponse, RunnerSkillRequest, RunnerSkillResolveResponse, RunnerSkillSource,
    RUNNER_SKILL_RESPONSE_FORMAT, RUNNER_SKILL_RESPONSE_MAX_BYTES,
};

const PYTHON_SKILL_WRAPPER: &str = r#"import os, sys
p = sys.argv[1]
a = sys.argv[2:]
src = sys.stdin.read()
sys.argv = [p, *a]
sys.path[0] = os.path.dirname(p)
g = {"__name__": "__main__", "__file__": p, "__package__": None, "__spec__": None, "__builtins__": __builtins__}
exec(compile(src, p, "exec"), g, g)
"#;

const SHELL_SKILL_WRAPPER: &str = "script=$(cat) || exit $?; eval \"$script\"";

struct PreparedSkillResourceExecution {
    target_path: PathBuf,
    script: String,
    _managed_snapshot: Option<tempfile::TempDir>,
}

fn prepare_skill_resource_execution(
    config: &SkillsConfig,
    client_id: &str,
    server_url: &str,
    request: &RunnerSkillExecutionRequest,
) -> Result<PreparedSkillResourceExecution, String> {
    let store = SkillStore::for_runner(client_id, server_url)?;
    prepare_skill_resource_execution_with_store(config, &store, request)
}

fn prepare_skill_resource_execution_with_store(
    config: &SkillsConfig,
    store: &SkillStore,
    request: &RunnerSkillExecutionRequest,
) -> Result<PreparedSkillResourceExecution, String> {
    request
        .validate()
        .map_err(|_| "skill_invalid_execution_request".to_string())?;
    let resolved = resolve_runner_skill(config, store, &request.skill_id)?;
    require_resolved_source(resolved.as_ref(), request.expected_source)?;
    match request.expected_source {
        RunnerSkillSource::Configured => {
            let prepared = configured_skills::prepare_execution_target(config, request)?;
            Ok(PreparedSkillResourceExecution {
                target_path: prepared.target_path,
                script: prepared.script,
                _managed_snapshot: None,
            })
        }
        RunnerSkillSource::Managed => {
            let prepared = store.prepare_execution_snapshot(request)?;
            let target_path = prepared.snapshot.path().join(&request.path);
            Ok(PreparedSkillResourceExecution {
                target_path,
                script: prepared.script,
                _managed_snapshot: Some(prepared.snapshot),
            })
        }
    }
}

fn skill_execution_candidates(
    request: &RunnerSkillExecutionRequest,
    target: &str,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let extension = Path::new(&request.path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let candidates = match extension.as_str() {
        "py" => {
            let mut common = vec![
                "-B".to_string(),
                "-c".to_string(),
                PYTHON_SKILL_WRAPPER.to_string(),
                target.to_string(),
            ];
            common.extend(request.args.iter().cloned());
            let mut py = Vec::with_capacity(common.len() + 1);
            py.push("-3".to_string());
            py.extend(common.iter().cloned());
            vec![
                ("python3".to_string(), common.clone()),
                ("python".to_string(), common),
                ("py".to_string(), py),
            ]
        }
        "sh" => {
            let mut args = vec![
                "-c".to_string(),
                SHELL_SKILL_WRAPPER.to_string(),
                target.to_string(),
            ];
            args.extend(request.args.iter().cloned());
            vec![("sh".to_string(), args)]
        }
        _ => return Err("skill_resource_interpreter_unsupported".to_string()),
    };
    for (executable, args) in &candidates {
        let total = args.iter().fold(executable.len(), |total, arg| {
            total.saturating_add(1).saturating_add(arg.len())
        });
        if executable.is_empty()
            || executable.len() > PROCESS_EXECUTABLE_MAX_BYTES
            || args.len() > PROCESS_ARG_MAX_COUNT
            || args
                .iter()
                .any(|arg| arg.len() > PROCESS_ARG_MAX_BYTES || arg.contains('\0'))
            || total > PROCESS_ARGV_MAX_BYTES
        {
            return Err("skill_resource_interpreter_arguments_invalid".to_string());
        }
    }
    Ok(candidates)
}

fn interpreter_unavailable(result: &ShellCommandResult) -> bool {
    result.execution_state == webcodex_core::runner_protocol::ShellCommandExecutionState::NotStarted
        && result.result.error.as_deref().is_some_and(|error| {
            error.starts_with("failed to spawn structured process")
                || error.contains("structured process executable is unavailable")
        })
}

#[cfg(windows)]
fn windows_skill_interpreter_path_available(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    !configured_skills::metadata_is_link_like(&metadata)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_skill_resource_with_profiles_and_execution_state(
    generation: u64,
    skills: &SkillsConfig,
    client_id: &str,
    server_url: &str,
    policy: &RunnerPolicy,
    shell: &ShellConfig,
    project_registry_dir: &Path,
    cache: &PreparedShellProfileCache,
    cwd: Option<&str>,
    request: &RunnerSkillExecutionRequest,
    timeout_secs: u64,
    stop_requested: Option<&AtomicBool>,
    on_started: Option<&dyn Fn()>,
) -> ShellCommandResult {
    let prepared = match prepare_skill_resource_execution(skills, client_id, server_url, request) {
        Ok(prepared) => prepared,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(error),
            })
        }
    };
    let Some(target) = prepared.target_path.to_str() else {
        return ShellCommandResult::not_started(CommandResult {
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(0),
            error: Some("skill_resource_native_path_not_utf8".to_string()),
        });
    };
    let candidates = match skill_execution_candidates(request, target) {
        Ok(candidates) => candidates,
        Err(error) => {
            return ShellCommandResult::not_started(CommandResult {
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: Some(0),
                error: Some(error),
            })
        }
    };
    let last = candidates.len().saturating_sub(1);
    for (index, (executable, args)) in candidates.into_iter().enumerate() {
        #[cfg(windows)]
        let executable = {
            let resolved = match prepare_detached_process_launch(
                generation,
                policy,
                shell,
                project_registry_dir,
                cache,
                cwd,
                &executable,
                &args,
                timeout_secs,
                stop_requested,
            ) {
                Ok(launch) => launch.process.executable,
                Err(error)
                    if index != last
                        && error.contains("structured process executable is unavailable") =>
                {
                    continue;
                }
                Err(error) => {
                    return ShellCommandResult::not_started(CommandResult {
                        exit_code: None,
                        stdout: None,
                        stderr: None,
                        duration_ms: Some(0),
                        error: Some(error),
                    });
                }
            };
            if !windows_skill_interpreter_path_available(Path::new(&resolved)) {
                if index != last {
                    continue;
                }
                return ShellCommandResult::not_started(CommandResult {
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: Some(0),
                    error: Some("skill_resource_interpreter_unavailable".to_string()),
                });
            }
            resolved
        };
        let result = run_process_with_profiles_and_execution_state_with_start_hook(
            generation,
            policy,
            shell,
            project_registry_dir,
            cache,
            cwd,
            &executable,
            &args,
            Some(&prepared.script),
            timeout_secs,
            stop_requested,
            on_started,
        );
        if index != last && interpreter_unavailable(&result) {
            continue;
        }
        return result;
    }
    unreachable!("trusted Skill interpreter candidates are never empty")
}

pub(crate) fn handle_runner_skill_request(
    config: &SkillsConfig,
    client_id: &str,
    server_url: &str,
    policy: &RunnerPolicy,
    request: RunnerSkillRequest,
) -> CommandResult {
    let started = Instant::now();
    if request.validate().is_err() {
        return error_result(started, "skill_invalid_request");
    }
    let management = request.requires_management_capability();
    let store = match SkillStore::for_runner(client_id, server_url) {
        Ok(store) => store,
        Err(_) => {
            return error_result(
                started,
                if management {
                    "skill_store_unavailable"
                } else {
                    "skill_catalog_unavailable"
                },
            )
        }
    };

    let result = match request {
        RunnerSkillRequest::List => list_runner_skills(config, &store)
            .and_then(|response| serialize_bounded(response, "skill_response_invalid")),
        RunnerSkillRequest::Resolve { skill_id } => resolve_runner_skill(config, &store, &skill_id)
            .map(|skill| RunnerSkillResolveResponse {
                format: RUNNER_SKILL_RESPONSE_FORMAT.to_string(),
                skill,
            })
            .and_then(|response| {
                response
                    .validate_for_request(&skill_id)
                    .map_err(|_| "skill_response_invalid".to_string())?;
                serialize_bounded(response, "skill_response_invalid")
            }),
        RunnerSkillRequest::Read {
            skill_id,
            expected_source,
            path,
            start_line,
            limit,
            expected_package_revision,
            expected_definition_revision,
        } => read_runner_skill(
            config,
            &store,
            &skill_id,
            expected_source,
            &path,
            start_line,
            limit,
            expected_package_revision.as_deref(),
            expected_definition_revision.as_deref(),
        )
        .and_then(|response| serialize_bounded(response, "skill_response_invalid")),
        RunnerSkillRequest::Versions {
            skill_key,
            offset,
            limit,
        } => store
            .versions(&skill_key, offset, limit)
            .and_then(|response| serialize_bounded(response, "skill_store_response_invalid")),
        RunnerSkillRequest::Install {
            skill_key,
            source_project_id,
            source_project_root,
            artifact_path,
            expected_artifact_sha256,
            idempotency_key,
            activate,
            expected_state_revision,
        } => store
            .install(
                policy,
                &skill_key,
                &source_project_id,
                &source_project_root,
                &artifact_path,
                &expected_artifact_sha256,
                &idempotency_key,
                activate,
                expected_state_revision.as_deref(),
            )
            .and_then(|response| serialize_bounded(response, "skill_store_response_invalid")),
        RunnerSkillRequest::Activate {
            skill_key,
            package_revision,
            expected_state_revision,
            idempotency_key,
        } => store
            .activate(
                &skill_key,
                &package_revision,
                &expected_state_revision,
                &idempotency_key,
            )
            .and_then(|response| serialize_bounded(response, "skill_store_response_invalid")),
        RunnerSkillRequest::RemoveRevision {
            skill_key,
            package_revision,
            expected_state_revision,
            idempotency_key,
        } => store
            .remove_revision(
                &skill_key,
                &package_revision,
                &expected_state_revision,
                &idempotency_key,
            )
            .and_then(|response| serialize_bounded(response, "skill_store_response_invalid")),
    };

    match result {
        Ok(stdout) => CommandResult {
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
            duration_ms: Some(started.elapsed().as_millis() as u64),
            error: None,
        },
        Err(code) => error_result(started, &code),
    }
}

fn list_runner_skills(
    config: &SkillsConfig,
    store: &SkillStore,
) -> Result<RunnerSkillListResponse, String> {
    let configured = configured_skills::discover(config)?;
    let managed = store.list_active()?;
    let _managed_namespace_revision = managed.namespace_revision;
    let mut skills = configured
        .skills
        .into_iter()
        .map(|skill| skill.descriptor)
        .collect::<Vec<_>>();
    skills.extend(managed.skills);

    ensure_unique_skill_ids(&skills)?;
    let mut response = RunnerSkillListResponse {
        format: RUNNER_SKILL_RESPONSE_FORMAT.to_string(),
        skills,
        invalid_count: configured.invalid_count,
        diagnostics: configured.diagnostics,
        discovery_truncated: configured.discovery_truncated,
    };
    loop {
        response
            .validate()
            .map_err(|_| "skill_response_invalid".to_string())?;
        let encoded =
            serde_json::to_string(&response).map_err(|_| "skill_response_invalid".to_string())?;
        if encoded.len() <= RUNNER_SKILL_RESPONSE_MAX_BYTES {
            return Ok(response);
        }
        if response.skills.pop().is_none() {
            return Err("skill_response_too_large".to_string());
        }
        response.discovery_truncated = true;
    }
}

fn ensure_unique_skill_ids(skills: &[RunnerSkillDescriptor]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for skill in skills {
        if !seen.insert(skill.skill_id()) {
            return Err("skill_catalog_unavailable".to_string());
        }
    }
    Ok(())
}

fn resolve_runner_skill(
    config: &SkillsConfig,
    store: &SkillStore,
    skill_id: &str,
) -> Result<Option<RunnerSkillDescriptor>, String> {
    let configured = configured_skills::resolve_live_skill(config, skill_id)?;
    let managed = store.resolve_active(skill_id)?;
    resolve_candidates(configured.map(|skill| skill.descriptor), managed)
}

fn resolve_candidates(
    configured: Option<RunnerSkillDescriptor>,
    managed: Option<RunnerSkillDescriptor>,
) -> Result<Option<RunnerSkillDescriptor>, String> {
    match (configured, managed) {
        (None, None) => Ok(None),
        (Some(skill), None) | (None, Some(skill)) => Ok(Some(skill)),
        (Some(_), Some(_)) => Err("skill_catalog_unavailable".to_string()),
    }
}

fn require_resolved_source(
    resolved: Option<&RunnerSkillDescriptor>,
    expected_source: RunnerSkillSource,
) -> Result<(), String> {
    match resolved {
        Some(skill) if skill.source() == expected_source => Ok(()),
        Some(_) | None => Err("skill_source_changed".to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn read_runner_skill(
    config: &SkillsConfig,
    store: &SkillStore,
    skill_id: &str,
    expected_source: RunnerSkillSource,
    path: &str,
    start_line: usize,
    limit: usize,
    expected_package_revision: Option<&str>,
    expected_definition_revision: Option<&str>,
) -> Result<RunnerSkillReadResponse, String> {
    let resolved = resolve_runner_skill(config, store, skill_id)?;
    require_resolved_source(resolved.as_ref(), expected_source)?;

    let response = match expected_source {
        RunnerSkillSource::Configured => configured_skills::read_resource(
            config,
            skill_id,
            path,
            start_line,
            limit,
            expected_definition_revision,
        )?,
        RunnerSkillSource::Managed => store.read_resource(
            skill_id,
            path,
            start_line,
            limit,
            expected_package_revision,
            expected_definition_revision,
        )?,
    };
    response
        .validate_for_request(skill_id, expected_source, path, start_line, limit)
        .map_err(|_| "skill_response_invalid".to_string())?;

    // The unified wire family must not allow a target to silently switch source
    // while the source-specific read is in flight. Re-resolve only identity/source;
    // managed revisions remain pinned exclusively by explicit caller expectations.
    let after = resolve_runner_skill(config, store, skill_id)?;
    require_resolved_source(after.as_ref(), expected_source)?;
    Ok(response)
}

fn serialize_bounded<T: serde::Serialize>(value: T, invalid_code: &str) -> Result<String, String> {
    let output = serde_json::to_string(&value).map_err(|_| invalid_code.to_string())?;
    if output.len() > RUNNER_SKILL_RESPONSE_MAX_BYTES {
        return Err("skill_response_too_large".to_string());
    }
    Ok(output)
}

fn error_result(started: Instant, code: &str) -> CommandResult {
    CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(started.elapsed().as_millis() as u64),
        error: Some(code.to_string()),
    }
}

#[cfg(test)]
#[path = "runner_skills_tests.rs"]
mod tests;
