#[derive(Debug)]
enum StoredConfigFile {
    Missing,
    Valid {
        config: StoredDesktopConfig,
        bytes: Vec<u8>,
    },
    Corrupt,
    Unsupported,
}

fn load_config(path: &Path, activity: &ActivityLog) -> DesktopResult<StoredDesktopConfig> {
    let backup_path = desktop_state_backup_path(path);
    match read_stored_config(path)? {
        StoredConfigFile::Unsupported => Err(DesktopError::new(
            "configuration_migration_failed",
            "The saved configuration requires a newer Desktop schema",
            "Update Desktop or explicitly restore the previous configuration from Diagnostics.",
        )),
        StoredConfigFile::Valid { config, .. } => Ok(config),
        StoredConfigFile::Missing => match read_stored_config(&backup_path)? {
            StoredConfigFile::Missing => Ok(StoredDesktopConfig::default()),
            StoredConfigFile::Valid { config, bytes } => {
                recover_config_from_backup(path, &bytes, activity)?;
                Ok(config)
            }
            StoredConfigFile::Corrupt | StoredConfigFile::Unsupported => {
                Err(desktop_state_corrupt())
            }
        },
        StoredConfigFile::Corrupt => match read_stored_config(&backup_path)? {
            StoredConfigFile::Valid { config, bytes } => {
                recover_config_from_backup(path, &bytes, activity)?;
                Ok(config)
            }
            StoredConfigFile::Missing
            | StoredConfigFile::Corrupt
            | StoredConfigFile::Unsupported => Err(desktop_state_corrupt()),
        },
    }
}

fn recover_config_from_backup(
    primary_path: &Path,
    bytes: &[u8],
    activity: &ActivityLog,
) -> DesktopResult<()> {
    write_atomic_file(primary_path, bytes).map_err(|error| {
        desktop_state_unavailable("Desktop could not restore the previous known-good state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    activity.push(
        ActivityEventKind::StateRecovered,
        "desktop_state",
        ActivityLevel::Warning,
        "Recovered Desktop state from the previous known-good snapshot",
    );
    Ok(())
}

fn read_stored_config(path: &Path) -> DesktopResult<StoredConfigFile> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(StoredConfigFile::Missing)
        }
        Err(error) => {
            return Err(
                desktop_state_unavailable("Desktop could not inspect its saved state")
                    .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) })),
            )
        }
    };
    if !metadata.is_file() || metadata.len() > DESKTOP_STATE_MAX_BYTES {
        return Ok(StoredConfigFile::Corrupt);
    }
    let bytes = std::fs::read(path).map_err(|error| {
        desktop_state_unavailable("Desktop could not read its saved state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    match serde_json::from_slice::<StoredDesktopConfig>(&bytes) {
        Ok(config) if config.schema_version == 1 => Ok(StoredConfigFile::Valid { config, bytes }),
        Ok(_) => Ok(StoredConfigFile::Unsupported),
        Err(_) => Ok(StoredConfigFile::Corrupt),
    }
}

fn save_config_atomically(path: &Path, encoded: &[u8]) -> DesktopResult<()> {
    if encoded.len() as u64 > DESKTOP_STATE_MAX_BYTES {
        return Err(DesktopError::new(
            "desktop_state_invalid",
            "Desktop state exceeded its bounded persistence size",
            "Retry after reducing the saved Desktop configuration.",
        ));
    }

    if let StoredConfigFile::Valid { bytes, .. } = read_stored_config(path)? {
        let backup = desktop_state_backup_path(path);
        write_atomic_file(&backup, &bytes).map_err(|error| {
            desktop_state_unavailable("Desktop could not preserve the previous known-good state")
                .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
        })?;
    }

    write_atomic_file(path, encoded).map_err(|error| {
        desktop_state_unavailable("Desktop could not persist its non-secret runtime state")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })
}

fn desktop_state_backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("desktop-state.json");
    path.with_file_name(format!("{file_name}.bak"))
}

fn state_temp_path(path: &Path) -> PathBuf {
    let id = NEXT_STATE_TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("desktop-state.json");
    path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), id))
}

pub(crate) fn write_atomic_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_file_with_hook(path, bytes, |_| Ok(()))
}

pub(crate) fn write_atomic_file_with_hook<F>(
    path: &Path,
    bytes: &[u8],
    before_replace: F,
) -> io::Result<()>
where
    F: FnOnce(&Path) -> io::Result<()>,
{
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temp_path = state_temp_path(path);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp_path)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        before_replace(&temp_path)?;
        atomic_replace(&temp_path, path)?;
        sync_state_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(not(windows))]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_state_directory(path: &Path) -> io::Result<()> {
    let directory = File::open(path)?;
    match directory.sync_all() {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == io::ErrorKind::Unsupported
                || error.raw_os_error() == Some(libc::EINVAL) =>
        {
            // Some Unix filesystems (notably macOS variants) do not support
            // directory fsync. The file itself has already been synced and the
            // same-directory rename is atomic, so treat this specific platform
            // limitation as best-effort durability rather than a false save
            // failure after replacement has already succeeded.
            Ok(())
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(unix))]
fn sync_state_directory(_path: &Path) -> io::Result<()> {
    // Windows uses MOVEFILE_WRITE_THROUGH for the replacement. Opening a
    // directory for FlushFileBuffers would require broader sharing semantics
    // than the app-data policy needs here.
    Ok(())
}

fn desktop_state_corrupt() -> DesktopError {
    DesktopError::new(
        "desktop_state_corrupt",
        "Desktop saved state is corrupt and no valid recovery snapshot is available",
        "Restore or remove the Desktop state files explicitly, then restart WebCodex Desktop.",
    )
    .with_details(serde_json::json!({ "category": "state_corrupt" }))
}

fn desktop_state_unavailable(message: &'static str) -> DesktopError {
    DesktopError::new(
        "desktop_state_unavailable",
        message,
        "Check local app-data permissions and retry.",
    )
}

fn loopback_socket_from_server_url(server_url: &str) -> Option<SocketAddr> {
    let url = url::Url::parse(server_url).ok()?;
    if url.scheme() != "http"
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let port = url.port()?;
    match url.host()? {
        url::Host::Ipv4(address) if address.is_loopback() => {
            Some(SocketAddr::new(address.into(), port))
        }
        url::Host::Ipv6(address) if address.is_loopback() => {
            Some(SocketAddr::new(address.into(), port))
        }
        url::Host::Domain(domain) if domain.eq_ignore_ascii_case("localhost") => {
            Some(SocketAddr::from(([127, 0, 0, 1], port)))
        }
        _ => None,
    }
}

fn read_desktop_server_env(env_file: &Path) -> DesktopResult<String> {
    let metadata = std::fs::symlink_metadata(env_file).map_err(|error| {
        DesktopError::new(
            "desktop_state_unavailable",
            "Desktop could not inspect its local Server environment",
            "Check local app-data permissions and retry.",
        )
        .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    if !metadata.is_file() || metadata.len() > DESKTOP_SERVER_ENV_MAX_BYTES {
        return Err(DesktopError::new(
            "desktop_state_invalid",
            "Desktop local Server environment is not a bounded regular file",
            "Restore the Desktop-owned local Server configuration and retry.",
        ));
    }
    let bytes = std::fs::read(env_file).map_err(|error| {
        DesktopError::new(
            "desktop_state_unavailable",
            "Desktop could not read its local Server environment",
            "Check local app-data permissions and retry.",
        )
        .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })?;
    String::from_utf8(bytes).map_err(|_| {
        DesktopError::new(
            "desktop_state_invalid",
            "Desktop local Server environment is not valid UTF-8",
            "Restore the Desktop-owned local Server configuration and retry.",
        )
    })
}

fn desktop_server_listen_from_env(env_file: &Path) -> DesktopResult<String> {
    let content = read_desktop_server_env(env_file)?;
    desktop_server_listen_from_content(&content)
}

fn desktop_server_listen_from_content(content: &str) -> DesktopResult<String> {
    let mut listen = None;
    for line in content.lines() {
        let candidate = line.trim_start();
        let candidate = candidate
            .strip_prefix("export ")
            .unwrap_or(candidate)
            .trim_start();
        let Some((key, value)) = candidate.split_once('=') else {
            continue;
        };
        if key.trim() != "WEBCODEX_ADDR" {
            continue;
        }
        if listen.is_some() {
            return Err(DesktopError::new(
                "desktop_state_invalid",
                "Desktop local Server environment contains duplicate WEBCODEX_ADDR entries",
                "Restore the Desktop-owned local Server configuration and retry.",
            ));
        }
        let value = value.trim();
        if value.is_empty() {
            return Err(DesktopError::new(
                "desktop_state_invalid",
                "Desktop local Server address is empty",
                "Restore the Desktop-owned local Server configuration and retry.",
            ));
        }
        listen = Some(value.to_string());
    }
    listen.ok_or_else(|| {
        DesktopError::new(
            "desktop_state_invalid",
            "Desktop local Server environment does not contain WEBCODEX_ADDR",
            "Restore the Desktop-owned local Server configuration and retry.",
        )
    })
}

fn desktop_server_url_from_env(env_file: &Path) -> DesktopResult<String> {
    let listen = desktop_server_listen_from_env(env_file)?;
    let server_url = format!("http://{listen}");
    if loopback_socket_from_server_url(&server_url).is_none() {
        return Err(DesktopError::new(
            "desktop_state_invalid",
            "Desktop local Server address is not a valid loopback endpoint",
            "Restore the Desktop-owned local Server configuration and retry.",
        ));
    }
    Ok(server_url)
}

fn recover_stale_desktop_loopback_address(
    env_file: &Path,
    server_url: &str,
) -> DesktopResult<Option<String>> {
    let Some(address) = loopback_socket_from_server_url(server_url) else {
        return Ok(None);
    };
    match TcpListener::bind(address) {
        Ok(listener) => {
            drop(listener);
            Ok(None)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
            ) =>
        {
            let listen = reserve_loopback_address()?;
            rewrite_desktop_server_address(env_file, &listen)?;
            Ok(Some(format!("http://{listen}")))
        }
        Err(_) => Ok(None),
    }
}

fn rewrite_desktop_server_address(env_file: &Path, listen: &str) -> DesktopResult<()> {
    let content = read_desktop_server_env(env_file)?;

    let mut replaced = 0usize;
    let mut updated = String::with_capacity(content.len().saturating_add(listen.len()));
    for segment in content.split_inclusive('\n') {
        let (body, ending) = if let Some(body) = segment.strip_suffix("\r\n") {
            (body, "\r\n")
        } else if let Some(body) = segment.strip_suffix('\n') {
            (body, "\n")
        } else {
            (segment, "")
        };
        let candidate = body.trim_start();
        let candidate = candidate
            .strip_prefix("export ")
            .unwrap_or(candidate)
            .trim_start();
        let is_address = candidate
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == "WEBCODEX_ADDR");
        if is_address {
            replaced = replaced.saturating_add(1);
            if replaced > 1 {
                return Err(DesktopError::new(
                    "desktop_state_invalid",
                    "Desktop local Server environment contains duplicate WEBCODEX_ADDR entries",
                    "Restore the Desktop-owned local Server configuration and retry.",
                ));
            }
            let equals = body.find('=').ok_or_else(|| {
                DesktopError::new(
                    "desktop_state_invalid",
                    "Desktop local Server address entry is malformed",
                    "Restore the Desktop-owned local Server configuration and retry.",
                )
            })?;
            updated.push_str(&body[..=equals]);
            updated.push_str(listen);
            updated.push_str(ending);
        } else {
            updated.push_str(segment);
        }
    }
    if replaced != 1 {
        return Err(DesktopError::new(
            "desktop_state_invalid",
            "Desktop local Server environment does not contain exactly one WEBCODEX_ADDR entry",
            "Restore the Desktop-owned local Server configuration and retry.",
        ));
    }
    write_atomic_file(env_file, updated.as_bytes()).map_err(|error| {
        desktop_state_unavailable("Desktop could not persist its recovered local Server address")
            .with_details(serde_json::json!({ "io_kind": format!("{:?}", error.kind()) }))
    })
}

fn reserve_loopback_address() -> DesktopResult<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not reserve a loopback port for WebCodex",
            "Check local networking and retry.",
        )
    })?;
    let address = listener.local_addr().map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not inspect the reserved loopback port",
            "Retry setup.",
        )
    })?;
    Ok(address.to_string())
}

