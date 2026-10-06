//! Browser-owned storage, never a caller-supplied Chrome profile path.
use crate::{BrowserError, BrowserResult};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[cfg(windows)]
mod windows;

pub(crate) fn state_root() -> BrowserResult<PathBuf> {
    let root = webcodex_runner_config::paths::default_client_state_base_dir()
        .map_err(|_| state_error())?
        .join("browser");
    if !root.is_absolute() {
        return Err(state_error());
    }
    Ok(root)
}

pub(crate) fn state_error() -> BrowserError {
    BrowserError::not_started(
        "browser_state_unavailable",
        "Browser state must be a private, user-owned directory without symbolic links",
    )
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

pub(crate) fn private_dir(path: &Path) -> BrowserResult<()> {
    if !path.is_absolute() {
        return Err(state_error());
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(meta) if is_link(&meta) || !meta.is_dir() => return Err(state_error()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(state_error()),
        }
    }
    let created = !path.exists();
    if created {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|_| state_error())?;
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| state_error())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(state_error());
        }
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        windows::private_path(path, created)?;
    }
    Ok(())
}

pub(crate) fn private_file(path: &Path, create: bool) -> BrowserResult<File> {
    private_dir(path.parent().ok_or_else(state_error)?)?;
    match std::fs::symlink_metadata(path) {
        Ok(meta) if is_link(&meta) || !meta.is_file() => return Err(state_error()),
        Ok(_) => {}
        Err(error) if create && error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(state_error()),
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(create)
        .create(create)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(|_| state_error())?;
    let metadata = file.metadata().map_err(|_| state_error())?;
    if !metadata.is_file() || is_link(&metadata) {
        return Err(state_error());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(state_error());
        }
    }
    #[cfg(windows)]
    windows::private_path(path, false)?;
    Ok(file)
}

pub(crate) fn validate_profile_id(name: &str) -> BrowserResult<()> {
    if name.is_empty()
        || name.len() > 48
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(BrowserError::not_started(
            "invalid_profile",
            "Managed profile must be a 1..48 character lowercase name, not a filesystem path",
        ));
    }
    Ok(())
}

pub(crate) struct ManagedProfile {
    path: PathBuf,
    name: String,
    _lock: File,
}
impl ManagedProfile {
    pub(crate) fn acquire(root: &Path, name: &str) -> BrowserResult<Self> {
        validate_profile_id(name)?;
        private_dir(root)?;
        let directory = root.join("profiles").join(name);
        private_dir(&directory)?;
        let lock = private_file(&directory.join("owner.lock"), true)?;
        lock.try_lock_exclusive().map_err(|_| BrowserError::not_started("profile_busy", "This managed profile already has an owner; close that Browser before launching it again"))?;
        let path = directory.join("data");
        private_dir(&path)?;
        // Chromium's own singleton lock is independent of our Runner lease. Do
        // not attach to an orphan process after a Runner restart or clear its lock.
        #[cfg(unix)]
        if let Ok(target) = std::fs::read_link(path.join("SingletonLock")) {
            let pid = target
                .to_str()
                .and_then(|text| text.rsplit_once('-'))
                .and_then(|(_, pid)| pid.parse::<i32>().ok())
                .filter(|pid| *pid > 0);
            let alive = pid.is_none_or(|pid| unsafe { libc::kill(pid, 0) } == 0
                || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH));
            if alive {
                return Err(BrowserError::not_started(
                    "profile_busy",
                    "The managed Chrome profile still has a live or unverified native owner",
                ));
            }
        }
        let active = path.join("DevToolsActivePort");
        match std::fs::symlink_metadata(&active) {
            Ok(meta) if meta.is_file() && !is_link(&meta) => {
                std::fs::remove_file(active).map_err(|_| state_error())?
            }
            Ok(_) => return Err(state_error()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(state_error()),
        }
        Ok(Self {
            path,
            name: name.to_owned(),
            _lock: lock,
        })
    }
}

pub(crate) enum OwnedProfile {
    Ephemeral(TempDir),
    Managed(ManagedProfile),
}
impl OwnedProfile {
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::Ephemeral(temp) => temp.path(),
            Self::Managed(profile) => &profile.path,
        }
    }
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Self::Ephemeral(_) => None,
            Self::Managed(profile) => Some(&profile.name),
        }
    }
}

#[cfg(test)]
mod tests;
