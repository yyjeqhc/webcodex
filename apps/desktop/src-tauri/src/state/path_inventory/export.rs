use super::*;
use serde::Serialize;
use std::io::Write;

const MAX_EXPORT_BYTES: usize = 1024 * 1024;

pub(super) fn bounded_json(value: &impl Serialize) -> DesktopResult<Vec<u8>> {
    // The source record and entry count are independently bounded by Core.
    // Reject oversized serialized projections instead of silently omitting rows.
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| inventory_error("inventory_serialization_failed"))?;
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(inventory_error("inventory_too_large"));
    }
    Ok(bytes)
}

fn destination(path: &Path, inventory: &PathInventory) -> DesktopResult<PathBuf> {
    if !path.is_absolute()
        || path.extension().and_then(|value| value.to_str()) != Some("json")
        || path
            .components()
            .any(|value| matches!(value, std::path::Component::ParentDir))
        || path.as_os_str().len() > 32768
    {
        return Err(inventory_error("inventory_export_path_invalid"));
    }
    reject_managed_destination(path, inventory)?;
    let name = path
        .file_name()
        .filter(|name| !name.to_string_lossy().chars().any(char::is_control))
        .ok_or_else(|| inventory_error("inventory_export_path_invalid"))?;
    let parent = path
        .parent()
        .ok_or_else(|| inventory_error("inventory_export_path_invalid"))?;
    let location = entry(
        "export.parent",
        "desktop",
        "export_parent",
        "configured",
        parent,
        PathKind::Directory,
        SafetyCategory::Metadata,
    );
    if location.status != PathStatus::Present {
        return Err(inventory_error("inventory_export_path_unconfirmed"));
    }
    let parent = location
        .canonical_path
        .ok_or_else(|| inventory_error("inventory_export_path_unconfirmed"))?;
    let target = parent.join(name);
    reject_managed_destination(&target, inventory)?;
    Ok(target)
}

fn reject_managed_destination(target: &Path, inventory: &PathInventory) -> DesktopResult<()> {
    // Exports are new user-selected documents, never replacements for authority,
    // secrets, data or rollback state, including absent files at known locations.
    for location in inventory.roots.iter().chain(&inventory.entries) {
        for managed in [
            location.configured_path.as_deref(),
            location.canonical_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let protected_directory = location.kind == PathKind::Directory
                && (inventory.roots.iter().any(|root| root.id == location.id)
                    || matches!(
                        location.category,
                        SafetyCategory::Secret
                            | SafetyCategory::MixedConfiguration
                            | SafetyCategory::Data
                    ));
            if webcodex_runner_config::paths::paths_equal(target, managed)
                || protected_directory
                    && target.ancestors().any(|ancestor| {
                        webcodex_runner_config::paths::paths_equal(ancestor, managed)
                    })
            {
                return Err(inventory_error("inventory_export_managed_path"));
            }
        }
    }
    Ok(())
}

pub(super) fn write_document(
    path: &Path,
    bytes: &[u8],
    inventory: &PathInventory,
) -> DesktopResult<()> {
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(inventory_error("inventory_too_large"));
    }
    // A truncated projection cannot prove that the destination is outside every
    // managed location. Navigation remains available, but export fails closed.
    if inventory
        .issues
        .iter()
        .any(|issue| issue.code.ends_with("_truncated"))
    {
        return Err(inventory_error("inventory_incomplete"));
    }
    let target = destination(path, inventory)?;
    // Match the existing explicit support export's create-new contract. A failed
    // write is reported as unconfirmed; do not delete or overwrite an uncertain
    // destination on a retry. No configuration or filesystem tree is collected.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options
        .open(&target)
        .map_err(|_| inventory_error("inventory_export_file_exists_or_unavailable"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| inventory_error("inventory_write_unconfirmed"))
}
