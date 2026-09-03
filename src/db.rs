//! Compatibility facade for the canonical durable store crate.

pub(crate) use crate::workspace_activity_store::WorkspaceActivityStore;
pub use webcodex_store::*;

#[cfg(test)]
pub(crate) mod memory {
    pub(crate) use webcodex_store::{
        memory_definition_hash, memory_state_revision, MemoryPrincipalAttribution, MemoryPriority,
        MemoryScopeAttribution, MemorySetInput, MEMORY_SCOPE_IDENTITY_ATTRIBUTED,
    };
}
