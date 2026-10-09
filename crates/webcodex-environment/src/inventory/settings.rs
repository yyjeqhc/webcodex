//! Portable nonsecret values, never mixed configuration or file payloads.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Setting<T> {
    Known {
        value: T,
    },
    #[default]
    Unknown,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettingsLanguage {
    #[serde(rename = "en-US")]
    EnUs,
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "zh-TW")]
    ZhTw,
    #[serde(rename = "de-DE")]
    DeDe,
    #[serde(rename = "fr-FR")]
    FrFr,
    #[serde(rename = "ja-JP")]
    JaJp,
    #[serde(rename = "ko-KR")]
    KoKr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesktopPreferences {
    pub language: SettingsLanguage,
    pub automatic_update_download: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SettingsObservation {
    pub device_display_name: Setting<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectReference {
    pub id: String,
    pub path: Setting<PathBuf>,
    pub status: PathStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSettings {
    pub environment_id: Setting<String>,
    pub local_server: Setting<bool>,
    pub local_runner: Setting<bool>,
    pub service_scope: Setting<crate::service::ServiceScope>,
    pub projects: Setting<Vec<ProjectReference>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SettingsExport {
    pub schema_version: u16,
    pub kind: String,
    pub cannot_restore: bool,
    pub privacy_notice: String,
    pub inventory_revision: String,
    pub environment: EnvironmentSettings,
    pub device_display_name: Setting<Option<String>>,
    pub desktop_preferences: Setting<DesktopPreferences>,
}

/// A small typed value projection from the existing authoritative observation.
/// No filesystem reads, secret switch, raw extension maps or import semantics.
pub fn build_settings_export(inventory: &PathInventory) -> SettingsExport {
    fn setting<T: Clone>(value: &Option<T>) -> Setting<T> {
        value
            .as_ref()
            .map_or(Setting::Unknown, |value| Setting::Known {
                value: value.clone(),
            })
    }
    let mut seen = std::collections::BTreeSet::new();
    let references_valid = inventory
        .entries
        .iter()
        .filter(|entry| entry.component == "project" && entry.purpose == "project_reference")
        .all(|entry| seen.insert(entry.id.as_str()));
    let projects = if inventory.environment_id.is_none()
        || !references_valid
        || inventory.issues.iter().any(|issue| {
            matches!(
                issue.code.as_str(),
                "project_reference_invalid"
                    | "project_references_truncated"
                    | "environment_changed"
                    | "inventory_truncated"
            )
        }) {
        Setting::Unknown
    } else {
        Setting::Known {
            value: inventory
                .entries
                .iter()
                .filter(|entry| {
                    entry.component == "project" && entry.purpose == "project_reference"
                })
                .take(16)
                .filter_map(|entry| {
                    Some(ProjectReference {
                        id: entry
                            .id
                            .strip_prefix("project.")?
                            .strip_suffix(".reference")?
                            .into(),
                        path: setting(
                            &entry
                                .configured_path
                                .as_ref()
                                .filter(|path| {
                                    path.to_str().is_some_and(|value| {
                                        !value.contains("://") && !value.contains('@')
                                    })
                                })
                                .cloned(),
                        ),
                        status: entry.status,
                    })
                })
                .collect(),
        }
    };
    SettingsExport {
        schema_version: 1,
        kind: "settings_export".into(),
        cannot_restore: true,
        privacy_notice: "Contains selected nonsecret settings and private project path references. Review before sharing. Credentials, URLs, raw configuration, databases, logs and project contents are excluded. This is not a restorable backup.".into(),
        inventory_revision: inventory.revision.clone(),
        environment: EnvironmentSettings {
            environment_id: setting(&inventory.environment_id),
            local_server: setting(&inventory.local_server),
            local_runner: setting(&inventory.local_runner),
            service_scope: setting(&inventory.service_scope),
            projects,
        },
        device_display_name: inventory.settings.device_display_name.clone(),
        // Desktop captures its current preferences only at explicit export.
        desktop_preferences: Setting::NotApplicable,
    }
}

pub(super) fn safe_display_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(char::is_control)
        && !value.contains("://")
        && !value.contains('@')
        && !webcodex_core::sensitive_text::secret_like_value(value)
}
