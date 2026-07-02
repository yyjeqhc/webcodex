use std::path::{Path, PathBuf};

use crate::ServerInitOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ServerPathDefaults {
    pub(crate) data_dir: PathBuf,
    pub(crate) env_file: PathBuf,
}

pub(crate) fn default_server_paths() -> ServerPathDefaults {
    if is_effective_root() {
        return ServerPathDefaults {
            data_dir: PathBuf::from("/var/lib/webcodex"),
            env_file: PathBuf::from("/etc/webcodex/webcodex.env"),
        };
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    ServerPathDefaults {
        data_dir: home.join(".local/share/webcodex"),
        env_file: home.join(".config/webcodex/webcodex.env"),
    }
}

#[cfg(unix)]
pub(crate) fn is_effective_root() -> bool {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                let mut parts = rest.split_whitespace();
                let _real = parts.next();
                if let Some(effective) = parts.next() {
                    return effective == "0";
                }
            }
        }
    }
    std::env::var("USER").is_ok_and(|u| u == "root")
}

#[cfg(not(unix))]
pub(crate) fn is_effective_root() -> bool {
    false
}

pub(crate) fn render_server_env(opts: &ServerInitOptions, token: &str) -> String {
    let mut content = String::new();
    content.push_str(&format!("WEBCODEX_ADDR={}\n", opts.listen.trim()));
    content.push_str(&format!("WEBCODEX_DATA={}\n", opts.data_dir.display()));
    content.push_str(&format!("WEBCODEX_TOKEN={}\n", token));
    if let Some(public_url) = &opts.public_url {
        content.push_str(&format!(
            "WEBCODEX_PUBLIC_URL={}\n",
            public_url.trim().trim_end_matches('/')
        ));
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
