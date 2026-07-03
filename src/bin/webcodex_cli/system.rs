use std::path::{Path, PathBuf};

pub(crate) fn write_text_file(
    path: &Path,
    content: &str,
    overwrite: bool,
    secret: bool,
) -> Result<(), String> {
    if path.exists() && !overwrite {
        return Err(format!(
            "{} already exists; pass --overwrite to replace it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        if overwrite {
            options.create(true).truncate(true);
        } else {
            options.create_new(true);
        }
        if secret {
            options.mode(0o600);
        }
        let mut file = options
            .open(path)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::io::Write;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        if secret {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("failed to set permissions on {}: {}", path.display(), e))?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = secret;
        if overwrite {
            std::fs::write(path, content)
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        } else {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
            use std::io::Write;
            file.write_all(content.as_bytes())
                .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        }
    }
    Ok(())
}

pub(crate) fn discover_webcodex_binary() -> Option<PathBuf> {
    discover_named_binary_absolute("webcodex")
}

pub(crate) fn discover_named_binary_absolute(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if !dir.is_absolute() {
            continue;
        }
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{}.exe", name));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn systemctl_available() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join("systemctl").is_file())
}

pub(crate) fn is_systemd_platform() -> bool {
    cfg!(target_os = "linux") && systemctl_available()
}

/// Write `content` to `path` with 0600 permissions on Unix, creating parent
/// directories as needed. Used for one-time plaintext token files.
pub(crate) fn write_secret_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::io::Write;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("failed to set permissions on {}: {}", path.display(), e))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .map_err(|e| format!("failed to write {}: {}", path.display(), e))?;
    }
    Ok(())
}

pub(crate) fn discover_binary(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{}.exe", name));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub(crate) fn read_optional_token(
    path: &Option<PathBuf>,
    label: &str,
) -> Result<Option<String>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let token = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {} {}: {}", label, path.display(), e))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!("{} {} is empty", label, path.display()));
    }
    Ok(Some(token))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemdStatus {
    pub(crate) active: String,
    pub(crate) enabled: String,
}

pub(crate) fn query_systemd_service_status(service_name: &str) -> SystemdStatus {
    fn run_status(args: &[&str]) -> String {
        let output = std::process::Command::new("systemctl").args(args).output();
        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    "unknown".to_string()
                } else {
                    stdout
                }
            }
            Err(_) => "unknown".to_string(),
        }
    }
    if !systemctl_available() {
        return SystemdStatus {
            active: "unknown".to_string(),
            enabled: "unknown".to_string(),
        };
    }
    SystemdStatus {
        active: run_status(&["is-active", service_name]),
        enabled: run_status(&["is-enabled", service_name]),
    }
}

pub(crate) fn query_systemd_status() -> SystemdStatus {
    query_systemd_service_status("webcodex.service")
}
