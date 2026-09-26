//! Public Core entrypoint for an explicitly selected legacy CLI Runner.
//! The user service is the only old CLI owner that can be handed off without
//! a privileged system-unit replacement. All other shapes fail before stop.
use crate::legacy_systemd::UserRunnerOwner;
use crate::migration::{migrate_legacy_environment, migration_journal, LegacyImport};
use crate::storage::read_private;
use crate::{
    current_account, EnvironmentStore, NativeEnvironment, ProjectRecord, SetupDiagnostic,
    SetupProgress, SetupRequest, SetupResult, SetupResultValue, SetupSecrets,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn conflict(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Review the original CLI service, configuration and Server identity; no old service was changed")
}

pub(crate) fn real_ancestors(path: &Path) -> SetupResultValue<()> {
    for ancestor in path.ancestors() {
        if let Ok(meta) = fs::symlink_metadata(ancestor) {
            if meta.file_type().is_symlink() {
                return Err(conflict(
                    "legacy_path_link",
                    "An old CLI path contains a symbolic link",
                ));
            }
        }
    }
    Ok(())
}

/// The caller explicitly selects the old profile and saved user credential.
/// Neither credential nor profile ownership is inferred from a system unit.
#[derive(Clone, Debug)]
pub struct LegacyCliRunnerInput {
    pub profile: Option<String>,
    pub user_token_file: PathBuf,
}

fn profile_name(profile: Option<&str>) -> SetupResultValue<Option<&str>> {
    match profile {
        None => Ok(None),
        Some(value)
            if !value.is_empty()
                && value.len() <= 80
                && value != "."
                && value != ".."
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                }) =>
        {
            Ok(Some(value))
        }
        _ => Err(conflict(
            "legacy_profile",
            "The old CLI profile name is invalid",
        )),
    }
}

fn old_paths(
    request: &SetupRequest,
    profile: Option<&str>,
) -> SetupResultValue<(PathBuf, PathBuf)> {
    let profile = profile_name(profile)?;
    let base = webcodex_runner_config::paths::user_config_home().map_err(|_| {
        conflict(
            "legacy_profile",
            "The old user configuration home cannot be resolved",
        )
    })?;
    if !base.is_absolute() || base.is_symlink() {
        return Err(conflict(
            "legacy_profile",
            "The old user configuration home is not a real absolute directory",
        ));
    }
    let config_dir = match profile {
        Some(name) => base.join("webcodex/clients").join(name),
        None => base.join("webcodex"),
    };
    if request.account.home != current_account()?.home
        || request.account.identity != current_account()?.identity
    {
        return Err(conflict(
            "legacy_owner",
            "Migration must run as the old project user",
        ));
    }
    let unit_name = profile.map_or_else(
        || "webcodex-runner.service".to_owned(),
        |name| format!("webcodex-runner-{name}.service"),
    );
    Ok((
        base.join("systemd/user").join(unit_name),
        config_dir.join("runner.toml"),
    ))
}

fn project_inventory(
    config: &toml::Value,
    config_path: &Path,
) -> SetupResultValue<Vec<ProjectRecord>> {
    let registry = config
        .get("project_registry_dir")
        .and_then(toml::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            conflict(
                "legacy_project_registry",
                "The old Runner does not identify a project registry",
            )
        })?;
    if registry
        != config_path
            .parent()
            .ok_or_else(|| {
                conflict(
                    "legacy_project_registry",
                    "The old Runner config path is invalid",
                )
            })?
            .join("project-registry")
    {
        return Err(conflict(
            "legacy_project_registry",
            "The old Runner uses a custom project registry path",
        ));
    }
    real_ancestors(&registry)?;
    let mut projects = Vec::new();
    for entry in fs::read_dir(&registry).map_err(|_| {
        conflict(
            "legacy_project_registry",
            "The old project registry is unavailable",
        )
    })? {
        let entry = entry.map_err(|_| {
            conflict(
                "legacy_project_registry",
                "The old project registry cannot be listed",
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            return Err(conflict(
                "legacy_project_registry",
                "The old project registry contains an unknown entry",
            ));
        }
        let bytes = read_private(&path)?;
        let value: toml::Value = toml::from_str(std::str::from_utf8(&bytes).map_err(|_| {
            conflict(
                "legacy_project_registry",
                "The old project record is invalid",
            )
        })?)
        .map_err(|_| {
            conflict(
                "legacy_project_registry",
                "The old project record is invalid",
            )
        })?;
        let id = value
            .get("id")
            .and_then(toml::Value::as_str)
            .filter(|id| {
                !id.is_empty()
                    && id.len() <= 64
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
            .ok_or_else(|| conflict("legacy_project_registry", "The old project ID is invalid"))?;
        if path.file_stem().and_then(|value| value.to_str()) != Some(id) {
            return Err(conflict(
                "legacy_project_registry",
                "The project filename does not match its ID",
            ));
        }
        let project = value
            .get("path")
            .and_then(toml::Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| {
                conflict("legacy_project_registry", "The old project path is missing")
            })?;
        if !project.is_absolute() || project.canonicalize().ok().as_ref() != Some(&project) {
            return Err(conflict(
                "legacy_project_registry",
                "The old project path is no longer canonical",
            ));
        }
        if value.get("disabled").and_then(toml::Value::as_bool) == Some(true) {
            return Err(conflict(
                "legacy_project_registry",
                "A disabled project needs an explicit migration review",
            ));
        }
        projects.push(ProjectRecord {
            id: id.into(),
            path: project,
        });
        if projects.len() > 200 {
            return Err(conflict(
                "legacy_project_registry",
                "The old project registry exceeds the migration limit",
            ));
        }
    }
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    if projects.is_empty()
        || projects
            .windows(2)
            .any(|pair| pair[0].id == pair[1].id || pair[0].path == pair[1].path)
    {
        return Err(conflict(
            "legacy_project_registry",
            "The old project registry is empty or ambiguous",
        ));
    }
    Ok(projects)
}

async fn verify_server_inventory(
    request: &SetupRequest,
    username: &str,
    token: &str,
    client_id: &str,
    projects: &[ProjectRecord],
) -> SetupResultValue<()> {
    let native = NativeEnvironment::new()?;
    let overview = native
        .post(
            &request.server_url,
            "/api/runtime-console/overview",
            Some(token),
            json!({}),
        )
        .await?;
    if overview.get("service").and_then(Value::as_str) != Some("webcodex")
        || overview.get("authenticated_user").and_then(Value::as_str) != Some(username)
    {
        return Err(conflict(
            "legacy_user_identity",
            "The Server does not verify the saved user credential against the old Runner owner",
        ));
    }
    let inventory = native
        .post(
            &request.server_url,
            "/api/runtime-console/projects",
            Some(token),
            json!({"client_id": client_id, "limit": 200}),
        )
        .await?;
    let visible = inventory
        .get("projects")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            conflict(
                "legacy_project_inventory",
                "Server project inventory is unavailable",
            )
        })?;
    for project in projects {
        let runtime_id = format!("agent:{client_id}:{}", project.id);
        if !visible.iter().any(|item| {
            item.get("id").and_then(Value::as_str) == Some(runtime_id.as_str())
                && item.get("path").and_then(Value::as_str) == project.path.to_str()
        }) {
            return Err(conflict(
                "legacy_project_inventory",
                "The Server does not confirm every old project registration",
            ));
        }
    }
    Ok(())
}

/// Migrate only a known Linux CLI user-level Runner unit. The old unit is
/// disabled and stopped by the shared Core migration coordinator; all
/// credentials and project IDs are reused, never re-paired or re-registered.
pub async fn migrate_legacy_cli_user_runner(
    store: &EnvironmentStore,
    request: SetupRequest,
    input: LegacyCliRunnerInput,
    secrets: &SetupSecrets,
    progress: impl FnMut(SetupProgress),
) -> SetupResultValue<SetupResult> {
    if request.local_server() || !request.local_runner() {
        return Err(conflict(
            "legacy_component_conflict",
            "This entrypoint migrates only an old user-level Runner",
        ));
    }
    if !input.user_token_file.is_absolute() {
        return Err(conflict(
            "legacy_user_credential",
            "Select an absolute saved user credential path",
        ));
    }
    let (unit, config_path) = old_paths(&request, input.profile.as_deref())?;
    for path in [&unit, &config_path, &input.user_token_file] {
        real_ancestors(path)?;
    }
    let config_bytes = read_private(&config_path)?;
    let config: toml::Value = toml::from_str(std::str::from_utf8(&config_bytes).map_err(|_| {
        conflict(
            "legacy_runner_configuration",
            "The old Runner config is invalid",
        )
    })?)
    .map_err(|_| {
        conflict(
            "legacy_runner_configuration",
            "The old Runner config is invalid",
        )
    })?;
    let username = config
        .get("owner")
        .and_then(toml::Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            conflict(
                "legacy_user_identity",
                "Ownerless shared-key Runner profiles cannot be migrated automatically",
            )
        })?;
    let client_id = config
        .get("client_id")
        .and_then(toml::Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            conflict(
                "legacy_runner_identity",
                "The old Runner client ID is absent",
            )
        })?;
    if config.get("server_url").and_then(toml::Value::as_str) != Some(request.server_url.as_str()) {
        return Err(conflict(
            "legacy_server_binding",
            "The old Runner targets another Server",
        ));
    }
    let projects = project_inventory(&config, &config_path)?;
    if !projects
        .iter()
        .any(|project| Some(&project.path) == request.project.as_ref())
    {
        return Err(conflict(
            "legacy_project_identity",
            "The selected project is not registered under the old Runner",
        ));
    }
    let token = crate::read_secret(&input.user_token_file)?;
    if token.expose().starts_with("wc_agent_")
        || token.expose().starts_with("wc_boot_")
        || token.expose().starts_with("wc_acct_")
    {
        return Err(conflict(
            "legacy_user_credential",
            "The selected file is not a user API credential",
        ));
    }
    verify_server_inventory(&request, username, token.expose(), client_id, &projects).await?;
    // The HTTP proof must still refer to the credential being imported. A
    // concurrent token rotation is an identity change, not an invitation to
    // capture a fresh owner snapshot and continue the cutover.
    if crate::read_secret(&input.user_token_file)?.expose() != token.expose() {
        return Err(conflict(
            "legacy_user_credential_changed",
            "The saved user credential changed during Server verification",
        ));
    }
    let registry = config_path
        .parent()
        .ok_or_else(|| {
            conflict(
                "legacy_project_registry",
                "The old Runner config path is invalid",
            )
        })?
        .join("project-registry");
    let mut extra = vec![input.user_token_file.clone(), registry.clone()];
    extra.extend(
        projects
            .iter()
            .map(|project| registry.join(format!("{}.toml", project.id))),
    );
    let mut owner =
        UserRunnerOwner::new(unit, config_path.clone(), request.account.clone(), extra)?;
    if migration_journal(store)?.is_none() {
        owner.require_active_enabled()?;
    }
    let import = LegacyImport {
        server_env_file: None,
        runner_config_file: Some(config_path),
        user_token_file: input.user_token_file,
        username: username.into(),
        runner_client_id: Some(client_id.into()),
        projects,
        tunnel_profiles: vec![],
    };
    migrate_legacy_environment(store, request, import, &mut owner, secrets, progress).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn rejects_unsafe_profile_and_ownerless_config() {
        assert!(profile_name(Some("../other")).is_err());
        assert!(profile_name(Some("runner_one")).is_ok());
        let ownerless: toml::Value =
            toml::from_str("server_url='https://example.test'\nclient_id='runner'\ntoken='shared'")
                .unwrap();
        assert!(ownerless
            .get("owner")
            .and_then(toml::Value::as_str)
            .is_none());
    }
    #[test]
    fn inventory_preserves_original_project_identity_and_rejects_unknown_entries() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("config");
        let registry = base.join("project-registry");
        let project = temp.path().join("project");
        fs::create_dir_all(&registry).unwrap();
        fs::create_dir(&project).unwrap();
        fs::set_permissions(&registry, fs::Permissions::from_mode(0o700)).unwrap();
        let content = format!(
            "id = 'sample'\npath = '{}'\nname = 'Keep this name'\nallow_patch = false\n",
            project.display()
        );
        let file = registry.join("sample.toml");
        fs::write(&file, content.as_bytes()).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let config: toml::Value = toml::from_str(&format!(
            "project_registry_dir = '{}'\n",
            registry.display()
        ))
        .unwrap();
        let result = project_inventory(&config, &base.join("runner.toml")).unwrap();
        assert_eq!(
            result,
            vec![ProjectRecord {
                id: "sample".into(),
                path: project
            }]
        );
        assert!(String::from_utf8(read_private(&file).unwrap())
            .unwrap()
            .contains("allow_patch = false"));
        fs::write(registry.join("unrecognized"), b"x").unwrap();
        assert!(project_inventory(&config, &base.join("runner.toml")).is_err());
    }
}
