use super::*;

pub(super) fn role_entries(
    inventory: &mut PathInventory,
    root: &Path,
    record: &EnvironmentRecord,
    intent_source: &str,
) {
    if record.request.local_runner() {
        let config = root.join("runner.toml");
        add(
            inventory,
            file(
                root,
                "runner.toml",
                "runner.configuration",
                "runner",
                SafetyCategory::MixedConfiguration,
            ),
        );
        let default_registry = configuration::default_runner_registry().ok();
        let mut locations = inspect_runner_locations(
            &config,
            default_registry.as_deref().unwrap_or_else(|| Path::new("")),
        );
        if locations.client_id.as_deref() != record.runner_client_id.as_deref()
            || locations.owner.as_deref() != record.username.as_deref()
            || locations.server_url.as_deref() != Some(record.request.server_url.as_str())
        {
            issue(
                inventory,
                "runner_binding_unconfirmed",
                "runner.configuration",
            );
            locations.registry.status = PathStatus::Unconfirmed;
            locations.registry.canonical_path = None;
            locations.registry.directory_to_open = None;
        } else if let Some(relative) = locations
            .registry
            .configured_path
            .as_ref()
            .filter(|p| !p.is_absolute())
        {
            let mut entry = local_path_entry(
                "runner.registry",
                "runner",
                "project_registration",
                "configured",
                &root.join(relative),
                PathKind::Directory,
                SafetyCategory::Secret,
            );
            entry.configured_path = Some(relative.clone());
            locations.registry = entry;
        }
        add(inventory, locations.registry);
        add(
            inventory,
            local_path_entry(
                "runner.computer_session",
                "runner",
                "computer_session",
                "derived",
                &root.join("cu"),
                PathKind::Directory,
                SafetyCategory::Secret,
            ),
        );
        service_log(inventory, root, "runner", record);
    } else {
        add(
            inventory,
            reference_entry(
                "runner.configuration",
                "runner",
                "runner_configuration",
                "saved_record",
                PathKind::File,
                SafetyCategory::MixedConfiguration,
                PathStatus::NotConfigured,
            ),
        );
    }
    if record.projects.len() > 16 {
        issue(
            inventory,
            "project_references_truncated",
            "environment.record",
        );
    }
    for project in record.projects.iter().take(16) {
        if !safe_identifier(&project.id)
            || !project
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            issue(inventory, "project_reference_invalid", "environment.record");
            continue;
        }
        let id = format!("project.{}.reference", project.id);
        let mut entry = if record.request.local_runner() {
            local_path_entry(
                &id,
                "project",
                "project_reference",
                intent_source,
                &project.path,
                PathKind::Directory,
                SafetyCategory::Data,
            )
        } else {
            let mut entry = reference_entry(
                &id,
                "project",
                "project_reference",
                intent_source,
                PathKind::RemoteReference,
                SafetyCategory::Data,
                PathStatus::Remote,
            );
            if admissible_path(&project.path) {
                entry.configured_path = Some(project.path.clone());
            } else {
                entry.status = PathStatus::Invalid;
            }
            entry
        };
        entry.source = intent_source.into();
        add(inventory, entry);
    }
}

pub(super) fn server_entries(
    inventory: &mut PathInventory,
    root: &Path,
    record: &EnvironmentRecord,
) {
    let directory = root.join("server");
    let config = directory.join("webcodex.env");
    add(
        inventory,
        file(
            root,
            "server/webcodex.env",
            "server.configuration",
            "server",
            SafetyCategory::MixedConfiguration,
        ),
    );
    let locations = inspect_server_locations(&config, Some(&directory));
    let data_path = configuration::resolved_location_path(&locations.data, Some(&directory));
    if let Some(data) = data_path {
        add(
            inventory,
            local_path_entry(
                "server.database",
                "server",
                "server_database",
                "derived",
                &data.join("webcodex.db"),
                PathKind::File,
                SafetyCategory::Data,
            ),
        );
    }
    add(inventory, locations.data);
    add(inventory, locations.trace);
    service_log(inventory, &directory, "server", record);
    add(
        inventory,
        file(
            root,
            "tunnel.json",
            "tunnel.profiles",
            "tunnel",
            SafetyCategory::Metadata,
        ),
    );
    match load::<Vec<crate::TunnelRecord>>(root, "tunnel.json") {
        Ok(Some(profiles)) => {
            if profiles.len() > 16 {
                issue(inventory, "tunnel_profiles_truncated", "tunnel.profiles");
            }
            for profile in profiles.into_iter().take(16) {
                if profile.profile_id.is_empty()
                    || profile.profile_id.len() > 64
                    || !profile
                        .profile_id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    || !safe_identifier(&profile.profile_id)
                {
                    issue(inventory, "tunnel_profile_invalid", "tunnel.profiles");
                    continue;
                }
                let directory = directory.join("tunnels").join(&profile.profile_id);
                add(
                    inventory,
                    local_path_entry(
                        &format!("tunnel.{}.configuration", profile.profile_id),
                        "tunnel",
                        "tunnel_configuration",
                        "saved_record",
                        &directory.join("webcodex.env"),
                        PathKind::File,
                        SafetyCategory::MixedConfiguration,
                    ),
                );
                service_log(
                    inventory,
                    &directory,
                    &format!("tunnel.{}", profile.profile_id),
                    record,
                );
            }
        }
        Ok(None) => {}
        Err(_) => issue(inventory, "tunnel_profiles_unavailable", "tunnel.profiles"),
    }
}

fn service_log(
    inventory: &mut PathInventory,
    directory: &Path,
    component: &str,
    record: &EnvironmentRecord,
) {
    let id = format!("{component}.lifecycle_log");
    let component_name = if component.starts_with("tunnel.") {
        "tunnel"
    } else {
        component
    };
    let root = inventory
        .roots
        .first()
        .and_then(|r| r.configured_path.as_ref());
    let spec = root
        .and_then(|root| crate::EnvironmentStore::existing(root.clone()).ok())
        .and_then(|store| {
            if let Some(profile) = component.strip_prefix("tunnel.") {
                crate::tunnel_service_spec(&store, record, profile).ok()
            } else {
                crate::service_spec(
                    &store,
                    record,
                    if component == "runner" {
                        crate::service::Component::Runner
                    } else {
                        crate::service::Component::Server
                    },
                )
                .ok()
            }
        });
    let unit_name = spec
        .as_ref()
        .map(crate::service::diagnostic_service_name)
        .filter(|name| {
            name.len() <= 192
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        });
    let (mut entry, kind) = if cfg!(target_os = "linux") {
        (
            reference_entry(
                &id,
                component_name,
                "service_journal",
                "platform",
                PathKind::SystemLog,
                SafetyCategory::Log,
                PathStatus::Unconfirmed,
            ),
            LogSourceKind::SystemdJournal,
        )
    } else if cfg!(target_os = "macos") || cfg!(windows) && record.request.service_scope.is_system()
    {
        (
            local_path_entry(
                &id,
                component_name,
                "service_lifecycle",
                "platform",
                &directory.join(crate::service::SERVICE_LOG_NAME),
                PathKind::File,
                SafetyCategory::Log,
            ),
            LogSourceKind::LifecycleFile,
        )
    } else {
        (
            reference_entry(
                &id,
                component_name,
                "service_diagnostics",
                "platform",
                PathKind::SystemLog,
                SafetyCategory::Log,
                PathStatus::Unconfirmed,
            ),
            LogSourceKind::TaskScheduler,
        )
    };
    entry.log_source = Some(LogSource {
        kind,
        unit_name,
        service_scope: Some(record.request.service_scope),
    });
    add(inventory, entry);
}
