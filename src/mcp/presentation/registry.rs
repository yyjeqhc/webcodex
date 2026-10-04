//! One bundled inventory owns support and dispatch. No runtime lookup, lock,
//! wildcard fallback, tool admission, or plugin-provided callback is introduced.
use serde_json::Value;

pub(super) struct PresentationRenderer {
    pub tools: &'static [&'static str],
    pub project: fn(&str, &Value) -> Option<Value>,
}

// Registrations and complete formatting policy live together in their families.
static RENDERERS: &[PresentationRenderer] = &[
    super::jobs::LIST,
    super::jobs::OBSERVE,
    super::validation::RUN,
    super::validation::SUMMARY,
    super::git::CHANGES,
    super::git::REVIEW,
];
pub(super) fn for_tool(tool: &str) -> Option<&'static PresentationRenderer> {
    RENDERERS
        .iter()
        .find(|renderer| renderer.tools.contains(&tool))
}

#[cfg(test)]
mod tests;
