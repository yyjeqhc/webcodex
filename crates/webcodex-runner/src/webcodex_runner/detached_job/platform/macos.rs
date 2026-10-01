//! Native macos operations; durable ownership remains in the shared model/store.
use super::super::*;

pub(in crate::webcodex_runner::detached_job) fn macos_process_start_identity(
    pid: u32,
) -> Result<Option<String>, String> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: `info` points to writable storage for exactly one proc_bsdinfo,
    // and PROC_PIDTBSDINFO is the Darwin API for this PID's BSD process facts.
    let bytes = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if bytes != size as libc::c_int {
        let error = io::Error::last_os_error();
        if bytes == 0 && error.raw_os_error() == Some(libc::ESRCH) {
            // Darwin reports an unreaped zombie as ESRCH here even while
            // kill(pid, 0) can still succeed. A zombie cannot execute and its
            // process-owned lifetime lock is already released, so this is
            // definitive dead evidence for recovery without a PID-only probe.
            return Ok(None);
        }
        return Err(format!(
            "failed to read detached macOS process start identity for pid {pid}: returned {bytes} bytes ({error})"
        ));
    }
    // SAFETY: proc_pidinfo reported that it initialized the complete structure.
    let info = unsafe { info.assume_init() };
    if info.pbi_start_tvsec == 0 && info.pbi_start_tvusec == 0 {
        return Err("missing macOS process start identity".to_string());
    }
    Ok(Some(format!(
        "macos_start_{}_{}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    )))
}

pub(in crate::webcodex_runner::detached_job) fn native_process_start_identity(
    pid: u32,
) -> Result<String, String> {
    macos_process_start_identity(pid)?.ok_or_else(|| {
        format!("detached macOS process {pid} exited before its start identity was captured")
    })
}

pub(in crate::webcodex_runner::detached_job) fn detached_process_identity_is_live(
    lock_path: &Path,
    identity: &DetachedProcessIdentity,
) -> Result<bool, String> {
    validate_process_identity("recovery", identity)?;
    let Some(current_start) = macos_process_start_identity(identity.pid)? else {
        return Ok(false);
    };
    if current_start != identity.native_start_id {
        return Ok(false);
    }
    lifetime_lock_is_held(lock_path, &identity.creation_id)
}
