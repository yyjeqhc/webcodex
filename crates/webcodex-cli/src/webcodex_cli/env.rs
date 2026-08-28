use std::path::{Path, PathBuf};

use webcodex_runner_config::paths;

use crate::ServerInitOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerPathDefaults {
    pub(crate) data_dir: PathBuf,
    pub(crate) env_file: PathBuf,
}

pub(crate) fn default_server_paths() -> Result<ServerPathDefaults, String> {
    if paths::is_effective_root() {
        return Ok(ServerPathDefaults {
            data_dir: PathBuf::from("/var/lib/webcodex"),
            env_file: PathBuf::from("/etc/webcodex/webcodex.env"),
        });
    }
    let home = paths::home_dir().ok_or_else(|| {
        "cannot determine user home: set HOME (Unix) or USERPROFILE (Windows) to derive the Server env file path"
            .to_string()
    })?;
    Ok(ServerPathDefaults {
        data_dir: home.join(".local/share/webcodex"),
        env_file: home.join(".config/webcodex/webcodex.env"),
    })
}

pub(crate) fn is_effective_root() -> bool {
    paths::is_effective_root()
}

pub(crate) fn render_server_env(opts: &ServerInitOptions, token: &str) -> String {
    let mut content = String::new();
    content.push_str(&format!("WEBCODEX_ADDR={}\n", opts.listen.trim()));
    content.push_str(&format!("WEBCODEX_DATA={}\n", opts.data_dir.display()));
    content.push_str(&format!("WEBCODEX_TOKEN={}\n", token));
    if let Some(public_url) = &opts.public_url {
        let public_url = public_url.trim().trim_end_matches('/');
        content.push_str(&format!("WEBCODEX_PUBLIC_URL={public_url}\n"));
        content.push_str("WEBCODEX_OAUTH2_ENABLED=true\n");
        content.push_str(&format!("WEBCODEX_OAUTH2_ISSUER={public_url}\n"));
        content.push_str("WEBCODEX_OAUTH2_SHARED_KEY_BRIDGE=true\n");
    }
    content.push_str("WEBCODEX_SHARED_KEY_ENABLED=true\n");
    if opts.open {
        content.push_str("WEBCODEX_ALLOW_ANONYMOUS=true\n");
    }
    content
}

pub(crate) fn read_env_file_value(path: &Path, key: &str) -> Result<Option<String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read env file {}: {}", path.display(), e))?;
    Ok(parse_env_content_value(&content, key))
}

pub(crate) fn read_pairing_server_env_file_value(
    path: &Path,
    key: &str,
) -> Result<Option<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "failed to read server env file {}: {}; pairing create is a server/admin-side command. Run it on the server or pass a server/admin token file.",
            path.display(),
            e
        )
    })?;
    Ok(parse_env_content_value(&content, key))
}

pub(crate) fn parse_env_content_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((k, value)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let value = value.trim();
        let value = if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            &value[1..value.len() - 1]
        } else {
            value
        };
        return Some(value.to_string());
    }
    None
}
