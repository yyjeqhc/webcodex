//! Runner shell environments, command preparation, and bounded execution.

use super::config::{
    dialect_for_program, platform_default_dialect, validate_shell_config, RunnerPolicy,
    ShellConfig, ShellDialect, ShellEnvironmentMode, ShellProfileConfig,
};
use super::output::{CommandResult, ShellCommandResult};
use super::output_text::{
    append_bounded_text, normalize_captured_output_text_with_truncation, normalize_output_text,
    CapturedOutputEncoding, FullStreamUtf8Validity, LeadingBom, OutputTextSource,
};
use super::projects::find_project_shell_context;
use std::collections::HashMap;
#[cfg(windows)]
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
#[cfg(windows)]
use webcodex_core::runner_protocol::ShellCommandExecutionState;
use webcodex_core::runner_protocol::{ShellProcessArgv, ShellScriptLanguage, ShellScriptPayload};

use webcodex_core::workflow_session_contract::ExecutionShell;
use webcodex_process::{GracefulTermination, ManagedChild};

#[path = "process_command.rs"]
mod process_command;
pub(crate) use process_command::structured_process_command;

const SHELL_PROFILE_PREPARE_TIMEOUT_SECS: u64 = 30;
const PROCESS_GROUP_TERMINATION_GRACE: Duration = Duration::from_millis(50);
const PROCESS_TREE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);
const PROCESS_PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const PROFILE_PREPARE_PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const TYPESCRIPT_NODE_VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const RAW_TAIL_CAPTURE_ALLOWANCE: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreparedShellProfileKey {
    generation: u64,
    project_key: String,
    profile_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedShellProfile {
    pub(crate) profile_name: String,
    program: String,
    args: Vec<String>,
    dialect: ShellDialect,
    env_snapshot: HashMap<String, String>,
}

/// A native-process launch environment produced by the existing Runner shell
/// profile machinery. It contains only the prepared environment snapshot and
/// resolves the final executable through that snapshot's PATH; no shell layer
/// is inserted around the child process.
#[derive(Debug, Clone)]
pub(crate) struct PreparedExecutionEnvironment {
    env_snapshot: HashMap<String, String>,
}

/// Lazily prepared shell environment snapshots. Snapshots are keyed by
/// config generation, project/cwd, and profile name because inline init
/// scripts such as `. .venv/bin/activate` are intentionally resolved from the
/// project cwd. A successful hot reload retires older cached generations after
/// the new generation prepares its first snapshot.
#[derive(Debug, Clone, Default)]
pub(crate) struct PreparedShellProfileCache {
    profiles: Arc<Mutex<HashMap<PreparedShellProfileKey, Arc<PreparedShellProfile>>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedDetachedProcessLaunch {
    pub(crate) process: ShellProcessArgv,
    pub(crate) cwd: String,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) timeout_secs: u64,
}

#[cfg(test)]
mod desktop_mcp_env_tests;

pub(crate) fn canonicalize_existing(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|e| format!("failed to access {}: {}", path.display(), e))
}

pub(crate) fn cwd_allowed(policy: &RunnerPolicy, cwd: &Path) -> Result<(), String> {
    if policy.allow_cwd_anywhere {
        return Ok(());
    }
    let cwd = canonicalize_existing(cwd)?;
    for root in &policy.allowed_roots {
        if let Ok(root) = canonicalize_existing(root) {
            // Case-insensitive component-wise containment on Windows.
            if webcodex_runner_config::paths::path_is_within(&cwd, &root) {
                return Ok(());
            }
        }
    }
    Err(format!(
        "cwd {} is outside allowed_roots",
        cwd.to_string_lossy()
    ))
}
mod commands;
mod drains;
mod environment;
mod execution;
mod internal;
mod output;
mod preparation;
mod process;
mod process_tree;
mod profiles;
mod run;
mod scripts;
#[cfg(windows)]
mod search;

#[cfg(windows)]
pub(crate) use commands::shell_quote_powershell;
pub(crate) use commands::{
    configured_explicit_shell_command, configured_node_project_check_job_command,
    configured_prepared_shell_job_command, configured_pytest_job_command,
    configured_ruff_job_command, configured_shell_job_command, configured_validation_job_command,
    explicit_shell_available, shell_quote,
};
use commands::{
    configured_prepared_shell_command, configured_process_command, configured_process_path,
    configured_shell_command, resolve_process_program,
};
#[cfg(test)]
use commands::{powershell_command_text, powershell_init_command_text, POWERSHELL_UTF8_PREAMBLE};
#[cfg(test)]
use drains::terminate_and_read_pipes;
use drains::{terminate_and_collect_pipes, ContinuousPipeDrain};
use environment::{
    apply_env_snapshot, apply_shell_environment, env_lookup, remove_sensitive_env, resolve_dialect,
    resolved_shell_program, should_inherit_env_key,
};
pub(crate) use environment::{base_shell_env, env_keys_equal, is_sensitive_env_key};
use execution::execute_configured_command;
#[cfg(test)]
use execution::spawned_output_failure;
pub(crate) use internal::{
    run_internal_posix_script_with_profiles_and_execution_state,
    run_internal_search_script_with_profiles_and_execution_state,
};
#[cfg(test)]
use output::IncrementalUtf8Validator;
use output::{read_bounded_pipe_tail, BoundedPipeTail};
use preparation::{run_prepare_command, stderr_tail};
pub(crate) use process::{
    prepare_detached_process_launch, run_process_with_profiles_and_execution_state,
    run_process_with_profiles_and_execution_state_with_internal_env_and_start_hook,
    run_process_with_profiles_and_execution_state_with_start_hook,
};
use process_tree::{
    terminate_child_process_tree, terminate_child_process_tree_until,
    terminate_child_without_output, wait_child_until, with_cleanup_error,
};
#[cfg(test)]
use profiles::powershell_profile_prepare_script;
pub(crate) use profiles::resolve_prepared_shell_profile;
#[cfg(test)]
use run::run_shell_impl;
pub(crate) use run::run_shell_with_profiles_and_execution_state;
#[cfg(test)]
pub(crate) use run::{run_shell, run_shell_with_profiles};
use scripts::configured_script_interpreter;
#[cfg(test)]
use scripts::{
    build_script_command, configured_script_runtime_plan, create_temporary_script,
    fixed_script_prefix_args, parse_node_version, typescript_node_prefix_args,
    typescript_node_probe_error, NodeVersion, ScriptRuntimePlan,
};
pub(crate) use scripts::{
    run_script_with_profiles_and_execution_state,
    run_script_with_profiles_and_execution_state_with_start_hook,
};
#[cfg(windows)]
#[cfg(test)]
use search::native_single_file_search_spec;
#[cfg(windows)]
use search::resolve_windows_internal_posix_interpreter;
#[cfg(windows)]
pub(crate) use search::run_windows_native_single_file_search_with_profiles;

#[cfg(test)]
#[path = "shell_tests.rs"]
mod runner_lifecycle_tests;
