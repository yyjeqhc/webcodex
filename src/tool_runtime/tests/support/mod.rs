//! Shared test helpers for tool_runtime tests.

pub(super) mod artifacts;
mod assertions;
mod auth;
mod files;
mod runner;
mod runtime;

pub(super) use assertions::*;
pub(super) use auth::*;
pub(super) use files::*;
pub(super) use runner::*;
pub(super) use runtime::*;

// Exercise the canonical discovery producer with the compact fixture defaults.
// These argument conveniences have no production caller or alternate output shape.
impl crate::tool_runtime::ToolRuntime {
    pub(crate) fn compact_tool_manifest_payload(&self) -> serde_json::Value {
        self.compact_tool_manifest_payload_bounded(None, None, None)
            .expect("default read_tool_manifest payload must succeed")
    }

    pub(crate) fn compact_tool_manifest_payload_bounded(
        &self,
        categories: Option<Vec<String>>,
        intent: Option<String>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value, crate::tool_runtime::ToolResult> {
        self.tool_manifest_payload_for_categories(
            None,
            categories,
            intent,
            limit,
            true,
            true,
            Default::default(),
            None,
            None,
        )
    }
}
