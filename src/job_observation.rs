//! Compatibility facade for the shared Job observation contract.

pub(crate) use webcodex_core::job_observation::*;

/// Per-process epoch generation is application state, not part of the pure
/// observation contract owned by `webcodex-core`.
pub(crate) fn new_epoch() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
