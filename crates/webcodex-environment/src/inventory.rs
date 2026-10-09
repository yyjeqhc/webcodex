//! Bounded metadata inventory derived from the unique authoritative Environment.
//! No setup, locks, processes, network, traversal scans, payload backup or restore.
mod configuration;
mod entries;
mod manifest;
mod model;
mod paths;
mod settings;
use crate::storage::{read_private, validate_existing_private_directory};
use crate::{EnvironmentRecord, SetupJournal};
pub use configuration::{
    append_runner_configuration_paths, append_server_configuration_paths, inspect_runner_locations,
    inspect_server_locations, RunnerLocations, ServerLocations,
};
pub use manifest::build_backup_manifest;
pub use model::*;
pub use paths::local_path_entry;
use paths::{admissible_display_path, admissible_path, reference_entry};
use serde::de::DeserializeOwned;
pub use settings::*;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MAX_INVENTORY_ENTRIES: usize = 64;
/// Reserve space within a 1 MiB manifest for fixed exclusions and projections.
pub const MAX_INVENTORY_BYTES: usize = 900 * 1024;
const MAX_PATH_BYTES: usize = 4096;

pub fn empty_environment_inventory() -> PathInventory {
    PathInventory {
        schema_version: 1,
        observed_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX),
        environment_id: None,
        local_server: None,
        local_runner: None,
        service_scope: None,
        roots: vec![],
        revision: String::new(),
        entries: vec![],
        issues: vec![],
        builds: vec![],
        identities: vec![],
        settings: SettingsObservation::default(),
    }
}

pub(super) fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(char::is_control)
        && !webcodex_core::sensitive_text::secret_like_value(value)
}

/// Stable revision of the serialized path projection, excluding observation time.
/// Internal settings observations do not revise location/manifest actions. Desktop
/// calls this again after adding its own authoritative metadata. Not a capability.
pub fn recompute_revision(inventory: &mut PathInventory) {
    // Public adapter additions share one bound. Fixed metadata callers can never
    // grow the manifest into a directory inventory or an unbounded build dump.
    let mut truncated = inventory.entries.len() > MAX_INVENTORY_ENTRIES
        || inventory.roots.len() > 8
        || inventory.builds.len() > 8
        || inventory.identities.len() > 8
        || inventory.issues.len() > MAX_INVENTORY_ENTRIES;
    inventory.entries.truncate(MAX_INVENTORY_ENTRIES);
    inventory.roots.truncate(8);
    inventory.builds.truncate(8);
    inventory.identities.truncate(8);
    inventory.issues.truncate(MAX_INVENTORY_ENTRIES);
    while serde_json::to_vec(inventory).is_ok_and(|bytes| bytes.len() > MAX_INVENTORY_BYTES) {
        truncated = true;
        if inventory.entries.pop().is_none() {
            break;
        }
    }
    if truncated
        && !inventory
            .issues
            .iter()
            .any(|i| i.code == "inventory_truncated")
    {
        if inventory.issues.len() == MAX_INVENTORY_ENTRIES {
            inventory.issues.pop();
        }
        issue(inventory, "inventory_truncated", "environment.root");
    }
    let mut projection = inventory.clone();
    projection.observed_at_ms = 0;
    projection.revision.clear();
    inventory.revision = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&projection).unwrap_or_default())
    );
}

fn issue(inventory: &mut PathInventory, code: &str, id: &str) {
    if inventory.issues.len() < MAX_INVENTORY_ENTRIES {
        inventory.issues.push(InventoryIssue {
            code: code.into(),
            entry_id: Some(id.into()),
        });
    }
}
fn add(inventory: &mut PathInventory, entry: PathEntry) {
    if inventory.entries.len() < MAX_INVENTORY_ENTRIES {
        inventory.entries.push(entry);
    } else if !inventory
        .issues
        .iter()
        .any(|i| i.code == "inventory_truncated")
    {
        issue(inventory, "inventory_truncated", "environment.root");
    }
}
fn file(root: &Path, name: &str, id: &str, component: &str, category: SafetyCategory) -> PathEntry {
    local_path_entry(
        id,
        component,
        &id.replace('.', "_"),
        "derived",
        &root.join(name),
        PathKind::File,
        category,
    )
}
fn load<T: DeserializeOwned>(root: &Path, name: &str) -> Result<Option<T>, PathStatus> {
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(PathStatus::Unreadable),
        Ok(metadata) if crate::storage::is_link(&metadata) => Err(PathStatus::UnsafePath),
        Ok(_) => read_private(&path)
            .map_err(|_| PathStatus::Unreadable)
            .and_then(|bytes| {
                serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|_| PathStatus::Invalid)
            }),
    }
}
fn finish(mut inventory: PathInventory) -> PathInventory {
    recompute_revision(&mut inventory);
    inventory
}

/// Inspects only fixed metadata/configuration files. Missing roots remain missing;
/// no EnvironmentStore::open/lock, service inspection, directory scan or API call.
pub fn inspect_environment_paths(
    root: &Path,
    expected_environment_id: Option<&str>,
) -> PathInventory {
    let mut inventory = empty_environment_inventory();
    inventory.roots.push(local_path_entry(
        "environment.root",
        "environment",
        "environment_root",
        "selected",
        root,
        PathKind::Directory,
        SafetyCategory::Metadata,
    ));
    let root_status = inventory.roots[0].status;
    if root_status != PathStatus::Present {
        issue(
            &mut inventory,
            if root_status == PathStatus::Missing {
                "environment_not_configured"
            } else {
                "environment_root_unavailable"
            },
            "environment.root",
        );
        if expected_environment_id.is_some() {
            issue(&mut inventory, "environment_changed", "environment.root");
        }
        return finish(inventory);
    }
    if let Err(error) = validate_existing_private_directory(root) {
        inventory.roots[0].status = if error.code == "unsafe_path" {
            PathStatus::UnsafePath
        } else {
            PathStatus::Unreadable
        };
        inventory.roots[0].directory_to_open = None;
        issue(
            &mut inventory,
            "environment_root_unavailable",
            "environment.root",
        );
        return finish(inventory);
    }
    add(
        &mut inventory,
        file(
            root,
            "environment.json",
            "environment.record",
            "environment",
            SafetyCategory::Metadata,
        ),
    );
    add(
        &mut inventory,
        file(
            root,
            "setup.json",
            "environment.setup",
            "environment",
            SafetyCategory::Metadata,
        ),
    );
    let record = match load::<EnvironmentRecord>(root, "environment.json") {
        Ok(Some(record)) => Some(record),
        Ok(None) => match load::<SetupJournal>(root, "setup.json") {
            Ok(Some(journal)) if journal.schema_version == crate::ENVIRONMENT_SCHEMA => {
                Some(journal.environment)
            }
            Ok(Some(_)) => {
                inventory.entries[1].status = PathStatus::Invalid;
                issue(&mut inventory, "setup_invalid", "environment.setup");
                None
            }
            Ok(None) => None,
            Err(status) => {
                inventory.entries[1].status = status;
                issue(&mut inventory, "setup_unavailable", "environment.setup");
                None
            }
        },
        Err(status) => {
            inventory.entries[0].status = status;
            issue(
                &mut inventory,
                "environment_record_unavailable",
                "environment.record",
            );
            None
        }
    };
    let Some(record) = record else {
        if expected_environment_id.is_some() {
            issue(&mut inventory, "environment_changed", "environment.record");
        } else {
            issue(
                &mut inventory,
                "environment_not_configured",
                "environment.record",
            );
        }
        return finish(inventory);
    };
    if record.schema_version != crate::ENVIRONMENT_SCHEMA
        || !safe_identifier(&record.environment_id)
    {
        issue(
            &mut inventory,
            "environment_record_invalid",
            "environment.record",
        );
        if expected_environment_id.is_some() {
            issue(&mut inventory, "environment_changed", "environment.record");
        }
        return finish(inventory);
    }
    if expected_environment_id.is_some_and(|id| id != record.environment_id) {
        issue(&mut inventory, "environment_changed", "environment.record");
        return finish(inventory);
    }
    let intent_source = if inventory.entries[0].status == PathStatus::Missing {
        "saved_journal"
    } else {
        "saved_record"
    };
    inventory.environment_id = Some(record.environment_id.clone());
    inventory.local_server = Some(record.request.local_server());
    inventory.local_runner = Some(record.request.local_runner());
    inventory.service_scope = Some(record.request.service_scope);
    for (kind, value) in [
        ("runner_client_id", record.runner_client_id.as_deref()),
        ("username", record.username.as_deref()),
        (
            "account_identity",
            Some(record.request.account.identity.as_str()),
        ),
    ] {
        if let Some(value) = value.filter(|value| safe_identifier(value)) {
            inventory.identities.push(IdentityObservation {
                kind: kind.into(),
                value: value.into(),
                source: intent_source.into(),
            });
        }
    }
    for (name, id, category) in [
        (
            "webcodex-user-token",
            "environment.user_credential",
            SafetyCategory::Secret,
        ),
        (
            "enrollment-recovery.json",
            "environment.enrollment_recovery",
            SafetyCategory::Secret,
        ),
        (
            "add-project.json",
            "environment.project_addition",
            SafetyCategory::Metadata,
        ),
        (
            "remove-project.json",
            "environment.project_removal",
            SafetyCategory::Metadata,
        ),
        (
            "migration.json",
            "environment.migration",
            SafetyCategory::Metadata,
        ),
        (
            "upgrade.json",
            "environment.upgrade",
            SafetyCategory::Metadata,
        ),
    ] {
        add(
            &mut inventory,
            file(root, name, id, "environment", category),
        );
    }
    add(
        &mut inventory,
        local_path_entry(
            "environment.upgrade_backups",
            "environment",
            "upgrade_recovery",
            "derived",
            &root.join("upgrade-backups"),
            PathKind::Directory,
            SafetyCategory::Cache,
        ),
    );
    for (id, path) in [
        ("runtime.cli", &record.request.binaries.cli),
        ("runtime.server", &record.request.binaries.server),
        ("runtime.runner", &record.request.binaries.runner),
    ] {
        add(
            &mut inventory,
            local_path_entry(
                id,
                "runtime",
                "runtime_binary",
                "saved_record",
                path,
                PathKind::File,
                SafetyCategory::Binary,
            ),
        );
    }
    if record.request.local_server() {
        entries::server_entries(&mut inventory, root, &record);
    } else {
        add(
            &mut inventory,
            reference_entry(
                "server.configuration",
                "server",
                "server_configuration",
                "saved_record",
                PathKind::RemoteReference,
                SafetyCategory::MixedConfiguration,
                PathStatus::Remote,
            ),
        );
        add(
            &mut inventory,
            reference_entry(
                "server.data",
                "server",
                "server_data",
                "saved_record",
                PathKind::RemoteReference,
                SafetyCategory::Data,
                PathStatus::Remote,
            ),
        );
        add(
            &mut inventory,
            reference_entry(
                "server.trace",
                "server",
                "request_trace",
                "saved_record",
                PathKind::RemoteReference,
                SafetyCategory::Log,
                PathStatus::Remote,
            ),
        );
    }
    entries::role_entries(&mut inventory, root, &record, intent_source);
    // Re-read identity without locking or creating setup state. A racing setup is
    // reported as changed; native open/export must recompute and compare revision.
    let final_record = if inventory.entries[0].status == PathStatus::Missing {
        load::<SetupJournal>(root, "setup.json")
            .ok()
            .flatten()
            .map(|j| j.environment)
    } else {
        load::<EnvironmentRecord>(root, "environment.json")
            .ok()
            .flatten()
    };
    if final_record
        .as_ref()
        .and_then(|r| serde_json::to_vec(r).ok())
        != serde_json::to_vec(&record).ok()
    {
        issue(&mut inventory, "environment_changed", "environment.record");
    }
    if intent_source == "saved_journal" {
        for entry in &mut inventory.entries {
            if entry.source == "saved_record" {
                entry.source = "saved_journal".into();
            }
        }
    }
    finish(inventory)
}

#[cfg(test)]
mod tests;
