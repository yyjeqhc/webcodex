//! Read-only location projections. This module never uses setup admission,
//! credential APIs, runtime probes, diagnostic collection, or persistence.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use webcodex_environment::inventory::{
    append_runner_configuration_paths, append_server_configuration_paths, build_backup_manifest,
    empty_environment_inventory, inspect_environment_paths, local_path_entry, recompute_revision,
    safe_build, BuildObservation, BuildSource, InventoryIssue, LogSource, LogSourceKind, PathEntry,
    PathInventory, PathKind, PathStatus, SafetyCategory,
};

mod export;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenInventoryRequest {
    pub entry_id: String,
    pub expected_revision: String,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryDocument {
    Inventory,
    BackupManifest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportInventoryRequest {
    pub kind: InventoryDocument,
    pub path: PathBuf,
    pub expected_revision: String,
}

fn inventory_error(code: &str) -> DesktopError {
    DesktopError::new(
        code,
        "The location or inventory action could not be confirmed",
        "Refresh Configuration and data, then check the location and its access permissions.",
    )
}

impl AppState {
    pub async fn path_inventory(&self) -> DesktopResult<PathInventory> {
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| inventory_error("desktop_operation_busy"))?;
        let root =
            webcodex_environment::default_environment_dir().map_err(environment::desktop_error)?;
        let inventory = desktop_inventory(core, &root, self.desktop_data_dir.source.label());
        export::bounded_json(&inventory)?;
        Ok(inventory)
    }

    pub async fn open_inventory_location(
        &self,
        request: OpenInventoryRequest,
    ) -> DesktopResult<()> {
        // Keep the selected Desktop context stable through native navigation.
        // External changes are reflected by rebuilding the read-only projection.
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| inventory_error("desktop_operation_busy"))?;
        let root =
            webcodex_environment::default_environment_dir().map_err(environment::desktop_error)?;
        let inventory = desktop_inventory(core, &root, self.desktop_data_dir.source.label());
        let directory = confirmed_location(&inventory, &request)?;
        crate::platform::opener::directory(&directory)
    }

    pub async fn export_inventory_document(
        &self,
        request: ExportInventoryRequest,
    ) -> DesktopResult<()> {
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| inventory_error("desktop_operation_busy"))?;
        let root =
            webcodex_environment::default_environment_dir().map_err(environment::desktop_error)?;
        let inventory = desktop_inventory(core, &root, self.desktop_data_dir.source.label());
        check_revision(&inventory, &request.expected_revision)?;
        let bytes = match request.kind {
            InventoryDocument::Inventory => export::bounded_json(&inventory)?,
            InventoryDocument::BackupManifest => {
                export::bounded_json(&build_backup_manifest(&inventory))?
            }
        };
        export::write_document(&request.path, &bytes, &inventory)
    }
}

fn check_revision(inventory: &PathInventory, expected: &str) -> DesktopResult<()> {
    if expected.is_empty()
        || expected != inventory.revision
        || inventory
            .issues
            .iter()
            .any(|issue| issue.code == "environment_changed")
    {
        return Err(inventory_error("inventory_changed"));
    }
    Ok(())
}

fn confirmed_location(
    inventory: &PathInventory,
    request: &OpenInventoryRequest,
) -> DesktopResult<PathBuf> {
    check_revision(inventory, &request.expected_revision)?;
    inventory
        .roots
        .iter()
        .chain(&inventory.entries)
        .find(|entry| {
            entry.id == request.entry_id
                && entry.status == PathStatus::Present
                && matches!(entry.kind, PathKind::File | PathKind::Directory)
        })
        .and_then(|entry| entry.directory_to_open.clone())
        .ok_or_else(|| inventory_error("inventory_location_unavailable"))
}

fn entry(
    id: &str,
    component: &str,
    purpose: &str,
    source: &str,
    path: &Path,
    kind: PathKind,
    category: SafetyCategory,
) -> PathEntry {
    local_path_entry(id, component, purpose, source, path, kind, category)
}

fn reference(
    id: &str,
    component: &str,
    purpose: &str,
    kind: PathKind,
    status: PathStatus,
    category: SafetyCategory,
) -> PathEntry {
    PathEntry {
        id: id.into(),
        component: component.into(),
        purpose: purpose.into(),
        source: "in_memory".into(),
        configured_path: None,
        canonical_path: None,
        status,
        kind,
        category,
        directory_to_open: None,
        log_source: (kind == PathKind::InMemory).then_some(LogSource {
            kind: LogSourceKind::InMemory,
            unit_name: None,
            service_scope: None,
        }),
    }
}

fn desktop_inventory(core: &DesktopCore, root: &Path, source: &str) -> PathInventory {
    let context = InventoryContext {
        config: &core.config,
        data_dir: &core.data_dir,
        config_path: &core.config_path,
        configuration_issue: core.configuration_issue.is_some(),
        cached_builds: core
            .adapter
            .binaries()
            .map(|binaries| binaries.builds.as_slice())
            .unwrap_or(&[]),
    };
    collect_inventory(&context, root, source)
}

struct InventoryContext<'a> {
    config: &'a StoredDesktopConfig,
    data_dir: &'a Path,
    config_path: &'a Path,
    configuration_issue: bool,
    cached_builds: &'a [webcodex_core::desktop_runtime_contract::MachineBuildInfo],
}

fn collect_inventory(core: &InventoryContext<'_>, root: &Path, source: &str) -> PathInventory {
    let persistent = core.config.persistent_environment.as_deref();
    let legacy = persistent.is_none() && core.config.topology.is_some();
    let mut inventory = if !legacy && !core.configuration_issue {
        inspect_environment_paths(root, persistent)
    } else {
        let mut result = empty_environment_inventory();
        let mut location = entry(
            "environment.root",
            "environment",
            "environment_root",
            "platform",
            root,
            PathKind::Directory,
            SafetyCategory::Metadata,
        );
        location.status = if legacy {
            PathStatus::NotApplicable
        } else {
            PathStatus::Unconfirmed
        };
        location.directory_to_open = None;
        result.roots.push(location);
        result
    };
    inventory.roots.push(entry(
        "desktop.root",
        "desktop",
        "desktop_root",
        source,
        &core.data_dir,
        PathKind::Directory,
        SafetyCategory::Metadata,
    ));
    for (id, purpose, path, category) in [
        (
            "desktop.settings",
            "desktop_settings",
            core.config_path.to_path_buf(),
            SafetyCategory::MixedConfiguration,
        ),
        (
            "desktop.settings_recovery",
            "desktop_settings_recovery",
            desktop_state_backup_path(&core.config_path),
            SafetyCategory::MixedConfiguration,
        ),
        (
            "desktop.locale",
            "desktop_locale",
            core.data_dir.join("desktop-locale.json"),
            SafetyCategory::Metadata,
        ),
        (
            "desktop.tunnel_config",
            "desktop_tunnel_config",
            core.data_dir.join("secrets/tunnel-config.json"),
            SafetyCategory::MixedConfiguration,
        ),
        (
            "desktop.download_cache",
            "desktop_download_cache",
            core.data_dir.join("stable-updates-v1"),
            SafetyCategory::Cache,
        ),
    ] {
        let kind = if category == SafetyCategory::Cache {
            PathKind::Directory
        } else {
            PathKind::File
        };
        inventory.entries.push(entry(
            id,
            "desktop",
            purpose,
            "desktop_state",
            &path,
            kind,
            category,
        ));
    }
    inventory.entries.push(reference(
        "desktop.activity",
        "desktop",
        "desktop_activity",
        PathKind::InMemory,
        PathStatus::Present,
        SafetyCategory::Log,
    ));
    inventory.entries.push(reference(
        "desktop.process_output",
        "desktop",
        "desktop_process_output",
        PathKind::InMemory,
        PathStatus::Present,
        SafetyCategory::Log,
    ));
    if core.configuration_issue {
        inventory.issues.push(InventoryIssue {
            code: "desktop_configuration_unconfirmed".into(),
            entry_id: Some("desktop.settings".into()),
        });
    } else if legacy {
        append_legacy_locations(&mut inventory, core);
    }
    let mut build = crate::commands::get_desktop_build_info();
    build.version = env!("CARGO_PKG_VERSION").into();
    inventory.builds.push(BuildObservation {
        source: BuildSource::EntryPoint,
        build: safe_build(&build),
    });
    for build in core.cached_builds {
        inventory.builds.push(BuildObservation {
            source: BuildSource::PreviouslyVerified,
            build: safe_build(build),
        });
    }
    recompute_revision(&mut inventory);
    // Selection context is non-secret. Changes to an otherwise identical legacy
    // location must still invalidate an old action after a Runtime switch.
    let context = core
        .config
        .runtime
        .as_ref()
        .and_then(|runtime| runtime.runner_client_id.as_deref())
        .unwrap_or("");
    inventory.revision = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}:{}:{}",
                inventory.revision, core.config.runtime_selection_revision, context
            )
            .as_bytes()
        )
    );
    inventory
}

fn append_legacy_locations(inventory: &mut PathInventory, core: &InventoryContext<'_>) {
    let Some(topology) = core.config.topology.as_ref() else {
        return;
    };
    let local_server = matches!(topology.server, ServerTopology::Local);
    let local_runner = matches!(topology.runner, RunnerTopology::Local);
    inventory.local_server = Some(local_server);
    inventory.local_runner = Some(local_runner);
    let runtime = core.config.runtime.as_ref();
    if local_server {
        if let Some(path) = runtime.and_then(|value| value.server_env_file.as_deref()) {
            // Legacy Desktop children inherit an unrecorded process cwd. An
            // env-file directory is not authority for relative runtime paths.
            append_server_configuration_paths(inventory, path, None);
        } else {
            inventory.entries.push(reference(
                "server.configuration",
                "server",
                "server_configuration",
                PathKind::File,
                PathStatus::Unconfirmed,
                SafetyCategory::MixedConfiguration,
            ));
        }
    } else {
        for (id, purpose, kind, category) in [
            (
                "server.configuration",
                "server_configuration",
                PathKind::RemoteReference,
                SafetyCategory::MixedConfiguration,
            ),
            (
                "server.data",
                "server_data",
                PathKind::RemoteReference,
                SafetyCategory::Data,
            ),
            (
                "server.trace",
                "server_trace",
                PathKind::RemoteReference,
                SafetyCategory::Log,
            ),
        ] {
            inventory.entries.push(reference(
                id,
                "server",
                purpose,
                kind,
                PathStatus::Remote,
                category,
            ));
        }
    }
    if local_runner {
        if let Some(runtime) = runtime {
            if let Some(path) = runtime.runner_config.as_deref() {
                append_runner_configuration_paths(
                    inventory,
                    path,
                    runtime.runner_client_id.as_deref(),
                    Some(&runtime.server_url),
                );
            } else {
                inventory.entries.push(reference(
                    "runner.configuration",
                    "runner",
                    "runner_configuration",
                    PathKind::File,
                    PathStatus::Unconfirmed,
                    SafetyCategory::MixedConfiguration,
                ));
            }
        }
    } else {
        inventory.entries.push(reference(
            "runner.configuration",
            "runner",
            "runner_configuration",
            PathKind::File,
            PathStatus::NotApplicable,
            SafetyCategory::MixedConfiguration,
        ));
    }
    if let Some(path) = runtime.and_then(|value| value.user_token_file.as_deref()) {
        inventory.entries.push(entry(
            "runtime.user_credential",
            "runtime",
            "environment_user_credential",
            "desktop_state",
            path,
            PathKind::File,
            SafetyCategory::Secret,
        ));
    }
    // Legacy Desktop children retain output in memory. Do not manufacture
    // persistent lifecycle logs for processes not managed as OS services.
}

#[cfg(test)]
mod tests;
