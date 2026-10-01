//! Native other unix operations; durable ownership remains in the shared model/store.
use super::super::*;

pub(in crate::webcodex_runner::detached_job) fn native_process_start_identity(
    _pid: u32,
) -> Result<String, String> {
    Err(
        "detached native process start identity is not implemented on this Unix platform"
            .to_string(),
    )
}
