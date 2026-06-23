//! SSH executor stub — removed in v2.
//!
//! SSH execution is no longer supported. These stubs exist only to satisfy
//! callers that haven't been migrated yet. All SSH operations return errors.

use crate::projects::{ProjectConfig, SshConfig};

pub(super) fn build_ssh_targets(_proj: &ProjectConfig) -> Result<Vec<String>, String> {
    Err("SSH executor removed in v2".to_string())
}

pub(super) fn ssh_option_args(_config: Option<&SshConfig>) -> Vec<String> {
    Vec::new()
}

pub(super) fn build_ssh_command(
    _target: &str,
    _cmd: &str,
    _config: Option<&SshConfig>,
) -> std::process::Command {
    // Return a dummy command that will fail
    let mut cmd = std::process::Command::new("false");
    cmd
}

pub(super) fn is_pre_start_ssh_connect_failure(_code: i32, _stdout: &str, _stderr: &str) -> bool {
    false
}

pub(super) fn run_ssh_targets(
    _targets: &[String],
    _cmd: &str,
    _timeout: u64,
    _config: Option<&SshConfig>,
) -> (i32, String, String, u64) {
    (-1, String::new(), "SSH executor removed in v2".to_string(), 0)
}

pub(super) fn run_ssh_patch_targets(
    _targets: &[String],
    _path: &str,
    _patch: &str,
    _cmd: &str,
    _config: Option<&SshConfig>,
) {
    // No-op — SSH removed
}

pub(super) fn parse_ssh_batch_blocks(_stdout: &str, _count: usize, _nonce: &str) -> Vec<String> {
    Vec::new()
}
