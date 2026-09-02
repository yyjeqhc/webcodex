use std::path::{Path, PathBuf};

use webcodex_runner_config::paths;

use crate::ServiceScope;

pub(crate) const CLIENT_PROFILE_ERROR: &str =
    "--profile must be a safe path component using only ASCII letters, digits, '.', '_' or '-'";

pub(crate) fn default_client_base_dir() -> Result<PathBuf, String> {
    paths::default_client_config_base_dir()
}

pub(crate) fn current_user_home() -> Result<PathBuf, String> {
    paths::user_home()
}

pub(crate) fn user_config_home() -> Result<PathBuf, String> {
    paths::user_config_home()
}

pub(crate) fn client_base_dir_for_scope(scope: ServiceScope) -> Result<PathBuf, String> {
    match scope {
        ServiceScope::User => Ok(user_config_home()?.join("webcodex")),
        ServiceScope::System => Ok(PathBuf::from("/etc/webcodex")),
    }
}

pub(crate) fn runner_config_for_scope(
    scope: ServiceScope,
    profile: Option<&str>,
) -> Result<PathBuf, String> {
    let base = client_base_dir_for_scope(scope)?;
    let dir = match profile {
        Some(profile) => client_output_dir_for_profile(&base, profile),
        None => base,
    };
    paths::resolve_runner_config_path(&dir)
}

pub(crate) fn client_profile_user_token_file_for_scope(
    scope: ServiceScope,
    profile: &str,
) -> Result<PathBuf, String> {
    Ok(
        client_output_dir_for_profile(&client_base_dir_for_scope(scope)?, profile)
            .join("webcodex-user-token"),
    )
}

pub(crate) fn client_profile_agent_token_file_for_scope(
    scope: ServiceScope,
    profile: &str,
) -> Result<PathBuf, String> {
    Ok(
        client_output_dir_for_profile(&client_base_dir_for_scope(scope)?, profile)
            .join("webcodex-runner-token"),
    )
}

pub(crate) fn user_systemd_unit_dir() -> Result<PathBuf, String> {
    Ok(user_config_home()?.join("systemd/user"))
}

pub(crate) fn runner_service_file_for_scope(
    scope: ServiceScope,
    profile: Option<&str>,
) -> Result<PathBuf, String> {
    let name = match profile {
        Some(profile) => format!("webcodex-runner-{profile}.service"),
        None => "webcodex-runner.service".to_string(),
    };
    let directory = match scope {
        ServiceScope::User => user_systemd_unit_dir()?,
        ServiceScope::System => PathBuf::from("/etc/systemd/system"),
    };
    Ok(directory.join(name))
}

pub(crate) fn validate_service_file_scope(
    scope: ServiceScope,
    service_file: &Path,
) -> Result<(), String> {
    if !service_file.is_absolute() {
        return Err("--service-file must be an absolute path".to_string());
    }
    let components = service_file
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    if components.contains(&"..") {
        return Err("--service-file cannot contain '..' path components".to_string());
    }
    let is_user_unit_path = components
        .windows(2)
        .any(|pair| pair == ["systemd", "user"]);
    let is_system_unit_path = service_file.starts_with("/etc")
        || service_file.starts_with("/usr/lib/systemd/system")
        || service_file.starts_with("/usr/local/lib/systemd/system")
        || service_file.starts_with("/lib/systemd/system")
        || service_file.starts_with("/run/systemd/system")
        || components
            .windows(2)
            .any(|pair| pair == ["systemd", "system"]);
    match scope {
        ServiceScope::User if is_system_unit_path => Err(format!(
            "user scope cannot write a system unit path: {}",
            service_file.display()
        )),
        ServiceScope::System if is_user_unit_path => Err(format!(
            "system scope cannot write a user unit path: {}",
            service_file.display()
        )),
        _ => Ok(()),
    }
}

pub(crate) fn default_client_state_base_dir() -> Result<PathBuf, String> {
    paths::default_client_state_base_dir()
}

pub(crate) fn validate_client_profile(profile: &str) -> Result<String, String> {
    let trimmed = profile.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.len() > 80
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(CLIENT_PROFILE_ERROR.to_string());
    }
    Ok(trimmed.to_string())
}

pub(crate) fn client_output_dir_for_profile(base_dir: &Path, profile: &str) -> PathBuf {
    base_dir.join("clients").join(profile)
}

pub(crate) fn client_profile_dir(profile: &str) -> Result<PathBuf, String> {
    Ok(client_output_dir_for_profile(
        &default_client_base_dir()?,
        profile,
    ))
}

pub(crate) fn client_state_dir_for_profile(base_dir: &Path, profile: &str) -> PathBuf {
    base_dir.join("clients").join(profile)
}

pub(crate) fn client_profile_state_dir(profile: &str) -> Result<PathBuf, String> {
    Ok(client_state_dir_for_profile(
        &default_client_state_base_dir()?,
        profile,
    ))
}

pub(crate) fn client_profile_runner_config(profile: &str) -> Result<PathBuf, String> {
    paths::resolve_runner_config_path(&client_profile_dir(profile)?)
}

pub(crate) fn client_profile_projects_dir(profile: &str) -> Result<PathBuf, String> {
    Ok(client_profile_dir(profile)?.join("projects.d"))
}

pub(crate) fn client_profile_user_token_file(profile: &str) -> Result<PathBuf, String> {
    Ok(client_profile_dir(profile)?.join("webcodex-user-token"))
}

pub(crate) fn client_profile_agent_token_file(profile: &str) -> Result<PathBuf, String> {
    Ok(client_profile_dir(profile)?.join("webcodex-runner-token"))
}
