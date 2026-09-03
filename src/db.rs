//! Compatibility facade for the canonical durable store crate.

pub(crate) use crate::workspace_activity_store::WorkspaceActivityStore;
pub use webcodex_store::*;

#[cfg(test)]
#[path = "db/agent_wake_tests.rs"]
mod agent_wake_host_tests;

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;
