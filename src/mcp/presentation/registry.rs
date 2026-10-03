//! One bundled inventory owns support and dispatch. No runtime lookup, lock,
//! wildcard fallback, tool admission, or plugin-provided callback is introduced.
use serde_json::Value;

pub(super) struct PresentationRenderer {
    pub tools: &'static [&'static str],
    pub project: fn(&str, &Value) -> Option<Value>,
}

// Validation owns its registrations and complete formatters. The other existing
// formatters remain in their current owner until their own coherent extraction.
static RENDERERS: &[PresentationRenderer] = &[
    PresentationRenderer {
        tools: &["list_jobs"],
        project: |_, output| super::list_jobs_presentation(output),
    },
    PresentationRenderer {
        tools: &["observe_jobs"],
        project: |_, output| super::observe_jobs_presentation(output),
    },
    super::validation::RUN,
    super::validation::SUMMARY,
    PresentationRenderer {
        tools: &["read_workspace_changes"],
        project: |_, output| super::show_changes_presentation(output),
    },
    PresentationRenderer {
        tools: &["read_git_review_summary"],
        project: |_, output| super::git_review_presentation(output),
    },
];

pub(super) fn for_tool(tool: &str) -> Option<&'static PresentationRenderer> {
    RENDERERS
        .iter()
        .find(|renderer| renderer.tools.contains(&tool))
}

#[cfg(test)]
mod tests;
