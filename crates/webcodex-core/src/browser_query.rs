//! Shared, read-only semantic snapshot filter. Never grants element authority.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserSnapshotQuery {
    /// Case-insensitive literal substring of name, description or field label/placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub text: Option<String>,
    /// Exact case-insensitive semantic role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub role: Option<String>,
    /// Case-insensitive literal substring of semantic group label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub group: Option<String>,
    /// Case-insensitive literal substring of form section label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub section: Option<String>,
    /// Restrict to native or semantic form fields, including disabled/read-only fields.
    #[serde(default)]
    pub fields_only: bool,
}

impl BrowserSnapshotQuery {
    pub fn is_valid(&self) -> bool {
        let filters = [&self.text, &self.role, &self.group, &self.section];
        (self.fields_only || filters.iter().any(|value| value.is_some()))
            && filters
                .into_iter()
                .flatten()
                .all(|value| !value.trim().is_empty() && value.chars().count() <= 128)
    }
}
