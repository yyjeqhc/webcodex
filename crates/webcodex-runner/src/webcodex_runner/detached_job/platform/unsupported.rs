//! Native unsupported operations; durable ownership remains in the shared model/store.
use super::super::*;

pub(in crate::webcodex_runner::detached_job) fn ensure_private_dir(
    _path: &Path,
) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

pub(in crate::webcodex_runner::detached_job) fn set_private_dir_permissions(
    _path: &Path,
) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

pub(in crate::webcodex_runner::detached_job) fn atomic_write_json<T: Serialize>(
    _path: &Path,
    _value: &T,
    _max_bytes: usize,
) -> Result<(), String> {
    Err("detached Job durable state is unsupported on this platform".to_string())
}

pub(in crate::webcodex_runner::detached_job) fn exclusive_lock(
    _path: &Path,
    _blocking: bool,
) -> Result<(), String> {
    Err("detached Job locking is unsupported on this platform".to_string())
}
