//! Native linux operations; durable ownership remains in the shared model/store.
use super::super::*;

pub(in crate::webcodex_runner::detached_job) fn native_process_start_identity(
    pid: u32,
) -> Result<String, String> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
        .map_err(|error| format!("failed to read detached process start identity: {error}"))?;
    let close = stat
        .rfind(')')
        .ok_or_else(|| "malformed /proc process stat for detached identity".to_string())?;
    let remaining = stat
        .get(close + 2..)
        .ok_or_else(|| "malformed /proc process stat suffix for detached identity".to_string())?;
    // `/proc/<pid>/stat` field 22 is starttime. `remaining` begins at field 3
    // (`state`), so starttime is zero-based index 19 after the closing comm.
    let start = remaining
        .split_whitespace()
        .nth(19)
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .ok_or_else(|| "missing Linux process starttime for detached identity".to_string())?;
    Ok(format!("linux_start_{start}"))
}

pub(in crate::webcodex_runner::detached_job) fn detached_process_identity_is_live(
    lock_path: &Path,
    identity: &DetachedProcessIdentity,
) -> Result<bool, String> {
    validate_process_identity("recovery", identity)?;
    let current_start = match native_process_start_identity(identity.pid) {
        Ok(value) => value,
        Err(error) => {
            let proc_stat = PathBuf::from(format!("/proc/{}/stat", identity.pid));
            if !proc_stat.exists() {
                return Ok(false);
            }
            return Err(error);
        }
    };
    if current_start != identity.native_start_id {
        return Ok(false);
    }
    lifetime_lock_is_held(lock_path, &identity.creation_id)
}
