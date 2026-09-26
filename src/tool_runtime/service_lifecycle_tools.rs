//! Tool adapters for controlled service lifecycle state.

use super::{ToolResult, ToolRuntime};
use serde_json::json;

impl ToolRuntime {
    pub(crate) fn service_drain_tool(
        &self,
        draining: bool,
        expected_generation: u64,
    ) -> ToolResult {
        match self
            .service_lifecycle
            .set_draining(draining, expected_generation)
        {
            Ok((snapshot, state_changed)) => {
                if state_changed {
                    webcodex_core::runtime_diagnostics::record(
                        webcodex_core::runtime_diagnostics::DiagnosticSeverity::Info,
                        "service_lifecycle",
                        if snapshot.draining {
                            "drain_entered"
                        } else {
                            "drain_exited"
                        },
                        None,
                    );
                }
                ToolResult::ok(json!({
                    "state_changed": state_changed,
                    "service_lifecycle": snapshot.as_json(),
                }))
            }
            Err(conflict) => ToolResult::err_with_output(
                "service lifecycle generation changed; re-read runtime_status before retrying",
                json!({
                    "error_kind": "service_lifecycle_generation_conflict",
                    "state_changed": false,
                    "expected_generation": conflict.expected_generation,
                    "service_lifecycle": conflict.actual.as_json(),
                }),
            ),
        }
    }
}
