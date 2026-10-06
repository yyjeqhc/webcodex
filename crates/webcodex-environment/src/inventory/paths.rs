use super::*;
use crate::storage::{is_link, validate_directory_ancestors};
use std::path::{Component, Path};

pub(super) fn admissible_display_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    text.len() <= MAX_PATH_BYTES
        && !text.chars().any(char::is_control)
        && !webcodex_core::sensitive_text::secret_like_value(text)
}

pub(super) fn admissible_path(path: &Path) -> bool {
    admissible_display_path(path) && !path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Inspect metadata only. Never creates files, follows links for content, reads
/// configuration, invokes a process, or acquires setup/service authority.
pub fn local_path_entry(
    id: &str,
    component: &str,
    purpose: &str,
    source: &str,
    path: &Path,
    kind: PathKind,
    category: SafetyCategory,
) -> PathEntry {
    let mut entry = reference_entry(
        id,
        component,
        purpose,
        source,
        kind,
        category,
        PathStatus::Unconfirmed,
    );
    if !admissible_path(path) || !path.is_absolute() {
        entry.status = PathStatus::Invalid;
        return entry;
    }
    entry.configured_path = Some(path.to_path_buf());
    let parent = if kind == PathKind::Directory {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    if let Err(error) = validate_directory_ancestors(parent) {
        entry.status = if error.code == "unsafe_path" {
            PathStatus::UnsafePath
        } else {
            PathStatus::Unreadable
        };
        return entry;
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            entry.status = PathStatus::Missing;
            return entry;
        }
        Err(_) => {
            entry.status = PathStatus::Unreadable;
            return entry;
        }
    };
    if is_link(&metadata) {
        entry.status = PathStatus::UnsafePath;
        return entry;
    }
    if !matches!(kind, PathKind::File if metadata.is_file())
        && !matches!(kind, PathKind::Directory if metadata.is_dir())
    {
        entry.status = PathStatus::Invalid;
        return entry;
    }
    match path.canonicalize() {
        Ok(canonical)
            if admissible_path(&canonical)
                && validate_directory_ancestors(if kind == PathKind::Directory {
                    &canonical
                } else {
                    canonical.parent().unwrap_or(&canonical)
                })
                .is_ok() =>
        {
            entry.status = PathStatus::Present;
            entry.canonical_path = Some(canonical.clone());
            entry.directory_to_open = if kind == PathKind::Directory {
                Some(canonical)
            } else {
                canonical.parent().map(Path::to_path_buf)
            };
        }
        _ => entry.status = PathStatus::Unreadable,
    }
    entry
}

pub(super) fn reference_entry(
    id: &str,
    component: &str,
    purpose: &str,
    source: &str,
    kind: PathKind,
    category: SafetyCategory,
    status: PathStatus,
) -> PathEntry {
    PathEntry {
        id: id.into(),
        component: component.into(),
        purpose: purpose.into(),
        source: source.into(),
        configured_path: None,
        canonical_path: None,
        directory_to_open: None,
        status,
        kind,
        category,
        log_source: None,
    }
}
