//! Bounded MCP display metadata. This layer never changes canonical results or
//! grants App admission; each bundled family owns its registration and formatting.
mod git;
mod jobs;
mod registry;
mod validation;

use serde_json::{json, Map, Value};

pub(super) const MCP_PRESENTATION_META_KEY: &str = "webcodex/presentation";
pub(super) const MCP_PRESENTATION_VERSION: u64 = 1;
pub(super) const MAX_MCP_PRESENTATION_ITEMS: usize = 8;
pub(super) const MAX_MCP_PRESENTATION_TEXT_CHARS: usize = 256;
pub(super) const MAX_MCP_PRESENTATION_DIFF_HUNKS: usize = MAX_MCP_PRESENTATION_ITEMS;
pub(super) const MAX_MCP_PRESENTATION_DIFF_LINES: usize = 80;
pub(super) const MAX_MCP_PRESENTATION_DIFF_CHARS: usize = 12 * 1024;

/// Explicit presentation entries rely on their own MCP tool descriptor carrying
/// Host App resource metadata. Routing one through the generic Adaptive Runtime
/// gateway preserves ToolRuntime semantics, but the Host sees only the gateway
/// descriptor and therefore cannot create the requested App card.
pub(super) fn tool_requires_direct_app_presentation(tool_name: &str) -> bool {
    crate::model_surface::tool_requires_direct_app_presentation(tool_name)
}

fn bounded_text(value: &Value) -> Option<String> {
    let value = value.as_str()?;
    let mut chars = value.chars();
    let bounded = chars
        .by_ref()
        .take(MAX_MCP_PRESENTATION_TEXT_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        let mut truncated = bounded
            .chars()
            .take(MAX_MCP_PRESENTATION_TEXT_CHARS.saturating_sub(1))
            .collect::<String>();
        truncated.push('…');
        Some(truncated)
    } else {
        Some(bounded)
    }
}

fn copy_bounded_text(source: &Value, target: &mut Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key).and_then(bounded_text) {
        target.insert(key.to_string(), Value::String(value));
    }
}

fn copy_scalar(source: &Value, target: &mut Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key) {
        if value.is_boolean() || value.is_number() {
            target.insert(key.to_string(), value.clone());
        }
    }
}

fn presentation_from_call_result(tool_name: &str, call_result: &Value) -> Option<Value> {
    let renderer = registry::for_tool(tool_name)?;
    let output = call_result.get("structuredContent")?.get("output")?;
    (renderer.project)(tool_name, output)
}

pub(super) fn attach_result_app_presentation(tool_name: &str, call_result: &mut Value) {
    let Some(presentation) = presentation_from_call_result(tool_name, call_result) else {
        return;
    };
    let Some(result) = call_result.as_object_mut() else {
        return;
    };
    let meta = result
        .entry("_meta".to_string())
        .or_insert_with(|| json!({}));
    let Some(meta) = meta.as_object_mut() else {
        return;
    };
    meta.insert(MCP_PRESENTATION_META_KEY.to_string(), presentation);
}
