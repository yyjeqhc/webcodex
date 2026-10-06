use super::*;

/// A description of locations and omissions, never a backup payload or restore capability.
pub fn build_backup_manifest(inventory: &PathInventory) -> BackupManifest {
    use SafetyCategory::*;
    let exclusions = [
        (Metadata, "Allowlisted identity, role, location and build observations only", "Raw state files, arbitrary extension fields, unknown files and settings values are omitted"),
        (MixedConfiguration, "Configuration locations only", "Server environment, Runner configuration and provider settings can contain credentials or arbitrary arguments"),
        (Secret, "Credential locations only", "Tokens, keys, private environment values and enrollment recovery contents are omitted"),
        (Data, "Data locations only", "Databases, history, project files, registration contents and session data are omitted"),
        (Log, "Log destination references only", "Raw logs and request traces can contain private inputs, outputs and credentials"),
        (Cache, "Cache and recovery locations only", "Upgrade snapshots may contain complete private data; cache and snapshot contents are omitted"),
        (Binary, "Binary locations and previously verified build metadata only", "Executable files and installer artifacts are omitted"),
    ].into_iter().map(|(category, projection, reason)| ManifestExclusion { category, projection: projection.into(), reason: reason.into() }).collect();
    let projections = [
        ("environment_intent", "Allowlisted saved roles, service scope and exact identities; no configuration values"),
        ("project_references", "Bounded saved project path references; no project files or registry contents"),
        ("ui_preferences", "Desktop preference category is acknowledged; saved preference values and arbitrary extensions are excluded"),
        ("connection_modes", "Connection mode category is acknowledged; URLs, arguments, private values and profile contents are excluded"),
        ("build_compatibility", "Allowlisted entry-point or previously verified build identity and declared data format; no runtime probes"),
    ].into_iter().map(|(category, description)| ManifestProjection { category: category.into(), description: description.into() }).collect();
    let mut required_restore_materials = vec![RestoreMaterial {
        id: "environment_and_desktop_configuration".into(),
        reason:
            "Original authoritative configuration and private values must be backed up separately"
                .into(),
    }];
    required_restore_materials.push(RestoreMaterial { id: "compatible_programs_and_data_format".into(), reason: "Separately retain compatible runtime programs and verify their declared persistent data format before any recovery".into() });
    if inventory.environment_id.is_some() {
        required_restore_materials.push(RestoreMaterial { id: "service_ownership_and_filesystem_permissions".into(), reason: "Recovery must preserve saved operating-system account, service scope, ownership and private filesystem permissions; this manifest changes none".into() });
        required_restore_materials.push(RestoreMaterial { id: "user_credentials".into(), reason: "Existing user identity requires its protected credentials; this manifest issues none".into() });
    }
    if inventory.local_server == Some(true) {
        required_restore_materials.push(RestoreMaterial { id: "server_configuration_credentials_and_consistent_data".into(), reason: "Complete Server recovery requires secret configuration and a consistent data backup through a separately authorized recovery process".into() });
        required_restore_materials.push(RestoreMaterial {
            id: "tunnel_credentials_if_configured".into(),
            reason: "Retain original Tunnel identity and private credentials separately".into(),
        });
    }
    if inventory.local_runner == Some(true) {
        required_restore_materials.push(RestoreMaterial { id: "runner_identity_configuration_and_registration".into(), reason: "Retain exact Runner identity, private configuration, project registration and referenced project files separately".into() });
    }
    BackupManifest { schema_version: 1, kind: "manifest_only".into(), does_not_contain_files: true, cannot_restore: true, privacy_notice: "Contains local paths and Environment identity metadata. Review before sharing. No file contents, settings values, credentials, logs or restore capability are included.".into(), inventory: inventory.clone(), exclusions, projections, required_restore_materials }
}
