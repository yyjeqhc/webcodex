use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use toml::{Table, Value as TomlValue};
use webcodex_admin::ServerHttpOptions;

use super::super::connections::{canonical_server_url, ensure_real_directory_tree};
use super::super::profiles::{client_output_dir_for_profile, validate_client_profile};

pub(super) const KEY_DISCLOSED_FILE: &str = ".hosted-key-disclosed";
const CONNECT_LOCK_FILE: &str = "connect.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectAuth {
    SharedKey,
    SharedKeyOAuth,
    ManagedOAuth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectOptions {
    pub(crate) server_url: String,
    pub(crate) server_http: ServerHttpOptions,
    pub(crate) key: Option<String>,
    pub(crate) key_file: Option<PathBuf>,
    pub(crate) auth: ConnectAuth,
    pub(crate) oauth_redirect_uri: Option<String>,
    pub(crate) oauth_computer_permissions: bool,
    pub(crate) oauth_local_mcp: bool,
    pub(crate) oauth_coding_agent: bool,
    pub(crate) username: Option<String>,
    pub(crate) project: PathBuf,
    pub(crate) profile: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) project_id: Option<String>,
    // Test seams are intentionally not exposed as command-line flags.
    pub(crate) config_base: Option<PathBuf>,
    pub(crate) state_base: Option<PathBuf>,
    pub(crate) runner_bin: Option<PathBuf>,
    pub(crate) wait_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedKey {
    pub(super) value: String,
    pub(super) generated: bool,
    pub(super) recovered_profile: Option<String>,
    pub(super) warn_short: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ExistingAgentConfig {
    pub(super) server_url: String,
    pub(super) token: String,
    pub(super) client_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ProjectFile {
    pub(crate) id: String,
    pub(crate) path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shell_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default = "default_true")]
    allow_patch: bool,
    #[serde(default)]
    disabled: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    hooks: BTreeMap<String, Vec<String>>,
}

fn default_true() -> bool {
    true
}

pub(super) struct ProfileLock {
    file: File,
}

impl ProfileLock {
    pub(super) fn acquire(state_dir: &Path) -> Result<Self, String> {
        let path = state_dir.join(CONNECT_LOCK_FILE);
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|error| format!("failed to open profile lock {}: {error}", path.display()))?;
        file.try_lock_exclusive().map_err(|_| {
            format!(
                "another WebCodex command is updating this profile; retry after it finishes ({})",
                path.display()
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub(super) fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn generate_shared_key() -> String {
    // Three independently generated UUID v4 values provide about 366 random
    // bits after their fixed version/variant bits, comfortably above 256 bits.
    let random = (0..3)
        .map(|_| uuid::Uuid::new_v4().simple().to_string())
        .collect::<String>();
    format!("wck_{random}")
}

fn normalize_shared_key(value: &str) -> Result<String, String> {
    let key = value.trim();
    if key.is_empty() {
        return Err("shared key cannot be empty".to_string());
    }
    if key.starts_with("wc_") {
        return Err(
            "wc_* values are managed WebCodex credentials, not hosted shared keys; use a different random value for `webcodex connect`, or use `webcodex login` for the managed flow"
                .to_string(),
        );
    }
    Ok(key.to_string())
}

fn read_key_file(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect key file {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("key file {} is not a regular file", path.display()));
    }
    if metadata.len() > 16 * 1024 {
        return Err(format!("key file {} is unexpectedly large", path.display()));
    }
    let value = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read key file {}: {error}", path.display()))?;
    normalize_shared_key(&value)
}

fn server_host_label(server_url: &str) -> String {
    let parsed = url::Url::parse(server_url).expect("canonical server URL must parse");
    let raw = parsed.host_str().unwrap_or("server");
    let mut label = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while label.contains("--") {
        label = label.replace("--", "-");
    }
    label = label.trim_matches('-').to_string();
    if label.is_empty() {
        label = "server".to_string();
    }
    label.chars().take(55).collect()
}

pub(super) fn derived_profile(server_url: &str, key: &str) -> String {
    let key_hash = sha256_hex(key.as_bytes());
    let identity = sha256_hex(format!("{server_url}\0{key_hash}").as_bytes());
    format!("{}-{}", server_host_label(server_url), &identity[..12])
}

pub(super) fn derived_oauth_profile(
    server_url: &str,
    username: &str,
    redirect_uri: &str,
) -> String {
    let identity = sha256_hex(
        format!(
            "{server_url}\0oauth\0{}\0{}",
            username.trim().to_ascii_lowercase(),
            redirect_uri.trim()
        )
        .as_bytes(),
    );
    format!(
        "{}-oauth-{}",
        server_host_label(server_url),
        &identity[..12]
    )
}

pub(super) fn generated_client_id(server_url: &str) -> String {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let host = server_host_label(server_url);
    let budget = 80usize.saturating_sub(17);
    format!(
        "{}-{}",
        host.chars().take(budget).collect::<String>(),
        &suffix[..16]
    )
}

fn sanitize_project_id(value: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !output.is_empty() {
            output.push('-');
            previous_separator = true;
        }
        if output.len() == 64 {
            break;
        }
    }
    output.truncate(output.trim_end_matches('-').len());
    if output.is_empty() {
        "project".to_string()
    } else {
        output
    }
}

fn validate_project_id(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("project id cannot be empty".to_string());
    }
    if value.len() > 64 {
        return Err("project id must be at most 64 characters".to_string());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("project id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(value.to_string())
}

pub(super) fn validate_existing_regular_file(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{} is not a regular file; refusing to read or replace it",
            path.display()
        ));
    }
    Ok(())
}

pub(super) fn read_existing_agent_config(
    path: &Path,
) -> Result<Option<ExistingAgentConfig>, String> {
    if !path.exists() {
        return Ok(None);
    }
    validate_existing_regular_file(path)?;
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read agent config {}: {error}", path.display()))?;
    toml::from_str(&content)
        .map(Some)
        .map_err(|error| format!("failed to parse agent config {}: {error}", path.display()))
}

pub(crate) fn read_project_files(
    projects_dir: &Path,
) -> Result<Vec<(PathBuf, ProjectFile)>, String> {
    let entries = match std::fs::read_dir(projects_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!(
                "failed to read project directory {}: {error}",
                projects_dir.display()
            ))
        }
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("toml"))
        .collect::<Vec<_>>();
    paths.sort();
    let mut projects = Vec::new();
    for path in paths {
        validate_existing_regular_file(&path)?;
        let content = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read project file {}: {error}", path.display()))?;
        let project: ProjectFile = toml::from_str(&content)
            .map_err(|error| format!("failed to parse project file {}: {error}", path.display()))?;
        validate_project_id(&project.id).map_err(|error| {
            format!(
                "invalid project id in project file {}: {error}",
                path.display()
            )
        })?;
        projects.push((path, project));
    }
    Ok(projects)
}

pub(crate) fn read_enabled_project_count(projects_dir: &Path) -> Result<usize, String> {
    read_project_files(projects_dir).map(|projects| {
        projects
            .into_iter()
            .filter(|(_, project)| !project.disabled)
            .count()
    })
}

pub(super) fn stored_project_matches(project: &ProjectFile, canonical_project: &Path) -> bool {
    Path::new(&project.path).canonicalize().is_ok_and(|path| {
        // Windows `canonicalize` can return `\\?\`-prefixed extended paths and
        // the filesystem is case-insensitive, so identity uses the shared
        // normalization instead of raw `Path` equality.
        webcodex_runner_config::paths::paths_equal(&path, canonical_project)
    })
}

fn recover_key_for_project(
    config_base: &Path,
    canonical_server: &str,
    canonical_project: &Path,
    explicit_profile: Option<&str>,
) -> Result<Option<(String, String, bool)>, String> {
    let profiles = config_base.join("clients");
    let mut candidates = Vec::new();
    let profile_names = if let Some(profile) = explicit_profile {
        vec![profile.to_string()]
    } else {
        match std::fs::read_dir(&profiles) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
                .filter(|name| validate_client_profile(name).is_ok())
                .collect(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(format!(
                    "failed to inspect existing profiles {}: {error}",
                    profiles.display()
                ))
            }
        }
    };
    for profile in profile_names {
        let profile_dir = client_output_dir_for_profile(config_base, &profile);
        let config_path = profile_dir.join("agent.toml");
        let Some(config) = read_existing_agent_config(&config_path)? else {
            continue;
        };
        let Ok(stored_server) = canonical_server_url(&config.server_url) else {
            continue;
        };
        if stored_server.url != canonical_server {
            continue;
        }
        let Ok(key) = normalize_shared_key(&config.token) else {
            continue;
        };
        let project_match = read_project_files(&profile_dir.join("projects.d"))?
            .iter()
            .any(|(_, project)| stored_project_matches(project, canonical_project));
        if project_match || explicit_profile.is_some() {
            let key_needs_display =
                key.starts_with("wck_") && !profile_dir.join(KEY_DISCLOSED_FILE).is_file();
            candidates.push((key, profile, key_needs_display));
        }
    }
    if candidates.len() > 1 {
        return Err(
            "more than one hosted profile matches this Server and project; rerun with --profile or --key"
                .to_string(),
        );
    }
    Ok(candidates.pop())
}

pub(super) fn resolve_key(
    opts: &ConnectOptions,
    config_base: &Path,
    canonical_server: &str,
    canonical_project: &Path,
) -> Result<ResolvedKey, String> {
    if opts.key.is_some() && opts.key_file.is_some() {
        return Err("--key and --key-file are mutually exclusive".to_string());
    }
    if let Some(value) = &opts.key {
        let value = normalize_shared_key(value)?;
        return Ok(ResolvedKey {
            warn_short: value.len() < 16,
            value,
            generated: false,
            recovered_profile: None,
        });
    }
    if let Some(path) = &opts.key_file {
        let value = read_key_file(path)?;
        return Ok(ResolvedKey {
            warn_short: value.len() < 16,
            value,
            generated: false,
            recovered_profile: None,
        });
    }
    if let Some((value, profile, key_needs_display)) = recover_key_for_project(
        config_base,
        canonical_server,
        canonical_project,
        opts.profile.as_deref(),
    )? {
        return Ok(ResolvedKey {
            value,
            generated: key_needs_display,
            recovered_profile: Some(profile),
            warn_short: false,
        });
    }
    Ok(ResolvedKey {
        value: generate_shared_key(),
        generated: true,
        recovered_profile: None,
        warn_short: false,
    })
}

pub(super) fn ensure_private_directory(path: &Path) -> Result<PathBuf, String> {
    let path = ensure_real_directory_tree(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("failed to protect {}: {error}", path.display()))?;
    }
    Ok(path)
}

pub(crate) fn atomic_write(path: &Path, content: &[u8], secret: bool) -> Result<bool, String> {
    if path.exists() {
        validate_existing_regular_file(path)?;
        let existing = std::fs::read(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        if existing == content {
            if secret {
                protect_secret_file(path)?;
            }
            return Ok(false);
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{} has no valid file name", path.display()))?;
    let temporary = parent.join(format!(".{filename}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| format!("failed to create {}: {error}", temporary.display()))?;
        file.write_all(content)
            .map_err(|error| format!("failed to write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync {}: {error}", temporary.display()))?;
        std::fs::rename(&temporary, path).map_err(|error| {
            format!(
                "failed to atomically replace {} with {}: {error}",
                path.display(),
                temporary.display()
            )
        })?;
        if secret {
            protect_secret_file(path)?;
        }
        Ok(true)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

// On non-Unix the body is a no-op, so the `path` parameter is unused there.
#[cfg_attr(not(unix), allow(unused_variables))]
pub(super) fn protect_secret_file(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("failed to protect {}: {error}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn render_project_file(project: &ProjectFile) -> Result<String, String> {
    toml::to_string(project).map_err(|error| format!("failed to render project config: {error}"))
}

pub(crate) fn resolve_project(
    projects_dir: &Path,
    canonical_project: &Path,
    explicit_id: Option<&str>,
) -> Result<(PathBuf, ProjectFile, bool), String> {
    let existing = read_project_files(projects_dir)?;
    if let Some((path, project)) = existing
        .iter()
        .find(|(_, project)| stored_project_matches(project, canonical_project))
    {
        if explicit_id.is_some_and(|id| id.trim() != project.id) {
            return Err(format!(
                "project {} is already registered as {}; refusing to create a duplicate",
                canonical_project.display(),
                project.id
            ));
        }
        return Ok((path.clone(), project.clone(), true));
    }

    let basename = canonical_project
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project");
    let explicit = explicit_id.map(validate_project_id).transpose()?;
    let mut id = explicit
        .clone()
        .unwrap_or_else(|| sanitize_project_id(basename));
    if let Some((_, collision)) = existing.iter().find(|(_, project)| project.id == id) {
        if explicit.is_some() {
            return Err(format!(
                "project id {} is already registered for a different path; choose another --project-id",
                collision.id
            ));
        }
        let path_hash = sha256_hex(canonical_project.to_string_lossy().as_bytes());
        let suffix = &path_hash[..8];
        let budget = 64usize.saturating_sub(suffix.len() + 1);
        id = format!(
            "{}-{suffix}",
            id.chars()
                .take(budget)
                .collect::<String>()
                .trim_end_matches('-')
        );
        if existing.iter().any(|(_, project)| project.id == id) {
            return Err(format!(
                "derived project id {id} is already registered for a different path; use --project-id"
            ));
        }
    }
    let project_path = projects_dir.join(format!("{id}.toml"));
    Ok((
        project_path,
        ProjectFile {
            id,
            path: canonical_project.to_string_lossy().to_string(),
            shell_profile: None,
            name: Some(basename.to_string()),
            kind: None,
            description: None,
            allow_patch: true,
            disabled: false,
            hooks: BTreeMap::new(),
        },
        false,
    ))
}

fn read_agent_document(path: &Path) -> Result<Table, String> {
    if !path.exists() {
        return Ok(Table::new());
    }
    validate_existing_regular_file(path)?;
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read agent config {}: {error}", path.display()))?;
    let document: TomlValue = toml::from_str(&content)
        .map_err(|error| format!("failed to parse agent config {}: {error}", path.display()))?;
    document
        .as_table()
        .cloned()
        .ok_or_else(|| format!("agent config {} is not a TOML table", path.display()))
}

pub(super) fn render_agent_document(
    path: &Path,
    server_url: &str,
    key: &str,
    client_id: &str,
    projects_dir: &Path,
    canonical_project: &Path,
) -> Result<String, String> {
    let mut root = read_agent_document(path)?;
    root.insert(
        "server_url".to_string(),
        TomlValue::String(server_url.to_string()),
    );
    root.insert("token".to_string(), TomlValue::String(key.to_string()));
    root.insert(
        "client_id".to_string(),
        TomlValue::String(client_id.to_string()),
    );
    root.insert(
        "display_name".to_string(),
        TomlValue::String(client_id.to_string()),
    );
    root.remove("owner");
    root.insert(
        "projects_dir".to_string(),
        TomlValue::String(projects_dir.to_string_lossy().to_string()),
    );
    root.insert(
        "transport".to_string(),
        TomlValue::String("websocket".to_string()),
    );
    root.entry("poll_interval_ms".to_string())
        .or_insert(TomlValue::Integer(1000));

    let policy = root
        .entry("policy".to_string())
        .or_insert_with(|| TomlValue::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| format!("agent config {} has a non-table policy", path.display()))?;
    policy.insert("allow_raw_shell".to_string(), TomlValue::Boolean(true));
    policy.insert("allow_cwd_anywhere".to_string(), TomlValue::Boolean(false));
    let mut roots = policy
        .get("allowed_roots")
        .and_then(TomlValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(TomlValue::as_str)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    roots.insert(canonical_project.to_string_lossy().to_string());
    policy.insert(
        "allowed_roots".to_string(),
        TomlValue::Array(roots.into_iter().map(TomlValue::String).collect()),
    );
    toml::to_string(&root).map_err(|error| format!("failed to render agent config: {error}"))
}

pub(super) fn validate_existing_profile(
    config: Option<&ExistingAgentConfig>,
    canonical_server: &str,
    key: &str,
) -> Result<(), String> {
    let Some(config) = config else {
        return Ok(());
    };
    let stored_server = canonical_server_url(&config.server_url)
        .map_err(|_| "existing profile has an invalid server URL".to_string())?;
    if stored_server.url != canonical_server {
        return Err(
            "selected profile belongs to a different Server; choose another --profile".to_string(),
        );
    }
    if config.token.trim() != key {
        return Err(
            "selected profile belongs to a different shared key; choose another --profile"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_is_strong_and_not_managed() {
        let key = generate_shared_key();
        assert!(key.starts_with("wck_"));
        assert!(!key.starts_with("wc_"));
        assert!(key.len() >= 4 + 96);
    }

    #[test]
    fn explicit_shared_key_validation_trims_and_rejects_managed_values() {
        assert_eq!(
            normalize_shared_key("  shared-key  ").unwrap(),
            "shared-key"
        );
        assert!(normalize_shared_key("  ").unwrap_err().contains("empty"));
        for managed in ["wc_pat_example", "wc_agent_example", "wc_acct_example"] {
            let error = normalize_shared_key(managed).unwrap_err();
            assert!(error.contains("managed WebCodex credentials"));
            assert!(!error.contains(managed));
        }
    }

    #[test]
    fn profile_is_stable_and_separates_keys_and_origins() {
        let first = derived_profile("https://example.test", "alpha");
        assert_eq!(first, derived_profile("https://example.test", "alpha"));
        assert_ne!(first, derived_profile("https://example.test", "beta"));
        assert_ne!(first, derived_profile("http://example.test", "alpha"));
        validate_client_profile(&first).unwrap();
    }

    #[test]
    fn oauth_profile_is_stable_and_separates_user_callback_and_origin() {
        let first = derived_oauth_profile(
            "https://example.test",
            "Alice",
            "https://client.example/callback",
        );
        assert_eq!(
            first,
            derived_oauth_profile(
                "https://example.test",
                "alice",
                "https://client.example/callback"
            )
        );
        assert_ne!(
            first,
            derived_oauth_profile(
                "https://example.test",
                "bob",
                "https://client.example/callback"
            )
        );
        assert_ne!(
            first,
            derived_oauth_profile(
                "https://example.test",
                "alice",
                "https://client.example/other"
            )
        );
        assert_ne!(
            first,
            derived_oauth_profile(
                "http://example.test",
                "alice",
                "https://client.example/callback"
            )
        );
        validate_client_profile(&first).unwrap();
    }

    #[test]
    fn omitted_key_is_generated_once_then_recovered_from_the_matching_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let project = project.canonicalize().unwrap();
        let config_base = tmp.path().join("config");
        let options = ConnectOptions {
            server_url: "https://example.test".to_string(),
            server_http: ServerHttpOptions::default(),
            key: None,
            key_file: None,
            auth: ConnectAuth::SharedKey,
            oauth_redirect_uri: None,
            oauth_computer_permissions: false,
            oauth_local_mcp: false,
            oauth_coding_agent: false,
            username: None,
            project: project.clone(),
            profile: None,
            client_id: None,
            project_id: None,
            config_base: Some(config_base.clone()),
            state_base: None,
            runner_bin: None,
            wait_timeout_ms: 100,
        };
        let first = resolve_key(&options, &config_base, "https://example.test", &project).unwrap();
        assert!(first.generated);
        let profile = derived_profile("https://example.test", &first.value);
        let profile_dir = config_base.join("clients").join(&profile);
        std::fs::create_dir_all(profile_dir.join("projects.d")).unwrap();
        std::fs::write(
            profile_dir.join("agent.toml"),
            format!(
                "server_url = \"https://example.test\"\ntoken = {:?}\nclient_id = \"client\"\n",
                first.value
            ),
        )
        .unwrap();
        std::fs::write(
            profile_dir.join("projects.d/project.toml"),
            format!("id = \"project\"\npath = {:?}\n", project.to_string_lossy()),
        )
        .unwrap();
        let recovered =
            resolve_key(&options, &config_base, "https://example.test", &project).unwrap();
        assert!(recovered.generated);
        assert_eq!(recovered.value, first.value);
        assert_eq!(
            recovered.recovered_profile.as_deref(),
            Some(profile.as_str())
        );
        std::fs::write(profile_dir.join(KEY_DISCLOSED_FILE), "disclosed = true\n").unwrap();
        let disclosed =
            resolve_key(&options, &config_base, "https://example.test", &project).unwrap();
        assert!(!disclosed.generated);
    }

    #[test]
    fn project_id_sanitization_is_runner_compatible() {
        assert_eq!(
            sanitize_project_id("Hello, 世界 / repo.git"),
            "hello-repo-git"
        );
        assert_eq!(sanitize_project_id("..."), "project");
        validate_project_id(&sanitize_project_id(&"a".repeat(100))).unwrap();
    }

    #[test]
    fn project_collision_gets_stable_suffix_and_explicit_collision_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let projects = tmp.path().join("projects.d");
        std::fs::create_dir(&projects).unwrap();
        let one = tmp.path().join("one/demo");
        let two = tmp.path().join("two/demo");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let (_, first, _) = resolve_project(&projects, &one.canonicalize().unwrap(), None).unwrap();
        atomic_write(
            &projects.join(format!("{}.toml", first.id)),
            render_project_file(&first).unwrap().as_bytes(),
            false,
        )
        .unwrap();
        let (_, second, _) =
            resolve_project(&projects, &two.canonicalize().unwrap(), None).unwrap();
        assert!(second.id.starts_with("demo-"));
        assert_eq!(
            second.id,
            resolve_project(&projects, &two.canonicalize().unwrap(), None)
                .unwrap()
                .1
                .id
        );
        let error =
            resolve_project(&projects, &two.canonicalize().unwrap(), Some("demo")).unwrap_err();
        assert!(error.contains("different path"));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_config_updates_preserve_secret_permissions_and_merge_roots() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("agent.toml");
        let project_one = tmp.path().join("one");
        let project_two = tmp.path().join("two");
        let projects = tmp.path().join("projects.d");
        std::fs::create_dir(&project_one).unwrap();
        std::fs::create_dir(&project_two).unwrap();
        std::fs::create_dir(&projects).unwrap();
        let first = render_agent_document(
            &config,
            "https://example.test",
            "shared",
            "client",
            &projects,
            &project_one,
        )
        .unwrap();
        assert!(atomic_write(&config, first.as_bytes(), true).unwrap());
        let second = render_agent_document(
            &config,
            "https://example.test",
            "shared",
            "client",
            &projects,
            &project_two,
        )
        .unwrap();
        assert!(atomic_write(&config, second.as_bytes(), true).unwrap());
        let parsed: TomlValue = toml::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        let roots = parsed["policy"]["allowed_roots"].as_array().unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(
            std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!tmp.path().join("one/agent.toml").exists());
        assert!(!atomic_write(&config, second.as_bytes(), true).unwrap());
    }
}
