//! Host-local run ownership and explicit recovery. No effect is ever replayed.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunIdentity {
    pub version: u32,
    pub run_id: String,
    pub owner_pid: u32,
}

#[derive(Debug)]
pub struct RunFence {
    path: PathBuf,
    identity: RunIdentity,
    _lock: File,
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options
}
fn private_directory(root: &Path) -> Result<(), &'static str> {
    if !root.is_absolute() {
        return Err("tunnel_fence_path");
    }
    for ancestor in root.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err("tunnel_fence_path");
        }
    }
    fs::create_dir_all(root).map_err(|_| "tunnel_fence_io")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))
            .map_err(|_| "tunnel_fence_io")?;
    }
    Ok(())
}
fn sync_directory(root: &Path) -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        for ancestor in root.ancestors() {
            File::open(ancestor)
                .and_then(|f| f.sync_all())
                .map_err(|_| "tunnel_fence_io")?;
        }
    }
    let _ = root;
    Ok(())
}
fn paths(root: &Path, origin: &str, tunnel: &str) -> (PathBuf, PathBuf) {
    let key = format!(
        "{:x}",
        Sha256::digest(format!("{}/{tunnel}", origin.trim_end_matches('/')))
    );
    (
        root.join(format!("{key}.active")),
        root.join(format!("{key}.lock")),
    )
}
fn lock(root: &Path, origin: &str, tunnel: &str) -> Result<(PathBuf, File), &'static str> {
    private_directory(root)?;
    let (path, lock_path) = paths(root, origin, tunnel);
    let file = private_options()
        .create(true)
        .truncate(false)
        .open(lock_path)
        .map_err(|_| "tunnel_fence_io")?;
    let meta = file.metadata().map_err(|_| "tunnel_fence_io")?;
    if !meta.is_file() {
        return Err("tunnel_fence_path");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
            return Err("tunnel_fence_path");
        }
    }
    file.try_lock_exclusive()
        .map_err(|_| "tunnel_owner_active")?;
    Ok((path, file))
}
fn read(path: &Path) -> Result<Option<RunIdentity>, &'static str> {
    let meta = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("tunnel_fence_io"),
        Ok(meta) => meta,
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4096 {
        return Err("tunnel_fence_invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
            return Err("tunnel_fence_invalid");
        }
    }
    let mut bytes = Vec::new();
    private_options()
        .open(path)
        .map_err(|_| "tunnel_fence_io")?
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| "tunnel_fence_io")?;
    if bytes == b"native-tunnel-run-v1\n" {
        return Err("tunnel_legacy_owner_unverifiable");
    }
    let identity: RunIdentity =
        serde_json::from_slice(&bytes).map_err(|_| "tunnel_fence_invalid")?;
    if identity.version != 2
        || identity.owner_pid == 0
        || !uuid::Uuid::parse_str(&identity.run_id)
            .is_ok_and(|id| id.to_string() == identity.run_id)
    {
        return Err("tunnel_fence_invalid");
    }
    Ok(Some(identity))
}
impl RunFence {
    pub fn acquire(root: &Path, origin: &str, tunnel: &str) -> Result<Self, &'static str> {
        let (path, lock) = lock(root, origin, tunnel)?;
        let identity = RunIdentity {
            version: 2,
            run_id: uuid::Uuid::new_v4().to_string(),
            owner_pid: std::process::id(),
        };
        let mut file = private_options()
            .create_new(true)
            .open(&path)
            .map_err(|_| "tunnel_restart_uncertain")?;
        let bytes = serde_json::to_vec(&identity).map_err(|_| "tunnel_fence_io")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "tunnel_fence_io")?;
        sync_directory(root)?;
        Ok(Self {
            path,
            identity,
            _lock: lock,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Only call after observing the owned task complete without uncertain work.
    pub fn finish(&self) -> Result<(), &'static str> {
        if read(&self.path)?.as_ref() != Some(&self.identity) {
            return Err("tunnel_fence_changed");
        }
        fs::remove_file(&self.path).map_err(|_| "tunnel_fence_io")?;
        let result = sync_directory(self.path.parent().ok_or("tunnel_fence_path")?);
        if result.is_err() {
            // Keep a visible fence when unlink durability could not be proved.
            if let Ok(mut file) = private_options().create_new(true).open(&self.path) {
                let _ = file
                    .write_all(&serde_json::to_vec(&self.identity).map_err(|_| "tunnel_fence_io")?);
                let _ = file.sync_all();
            }
            return result;
        }
        FileExt::unlock(&self._lock).map_err(|_| "tunnel_fence_io")
    }
}

/// Pure observation: never creates a lock, directory, or owner.
pub fn observe(
    root: &Path,
    origin: &str,
    tunnel: &str,
) -> Result<Option<RunIdentity>, &'static str> {
    read(&paths(root, origin, tunnel).0)
}

/// Exclusive exact-run archival. Caller owns the profile/service fences and must
/// prove the recorded process is absent. Legacy fences cannot supply that proof.
pub fn archive(
    root: &Path,
    origin: &str,
    tunnel: &str,
    expected_run: &str,
    owner_absent: impl FnOnce(u32) -> bool,
) -> Result<PathBuf, &'static str> {
    if !uuid::Uuid::parse_str(expected_run).is_ok_and(|id| id.to_string() == expected_run) {
        return Err("tunnel_fence_changed");
    }
    let (path, _lock) = lock(root, origin, tunnel)?;
    let archived = root
        .join(format!(
            "{}.recovered",
            path.file_name()
                .ok_or("tunnel_fence_path")?
                .to_string_lossy()
        ))
        .join(expected_run);
    let identity = read(&path)?.ok_or("tunnel_fence_missing")?;
    if identity.run_id != expected_run {
        return Err("tunnel_fence_changed");
    }
    if !owner_absent(identity.owner_pid) {
        return Err("tunnel_owner_active");
    }
    let directory = archived.parent().ok_or("tunnel_fence_path")?;
    private_directory(directory)?;
    if fs::symlink_metadata(&archived).is_ok() {
        return Err("tunnel_archive_exists");
    }
    // All admitted owners/recoveries share the identity lock. The destination is
    // private and unique to this exact run; retained archives are never replaced.
    fs::rename(&path, &archived).map_err(|_| "tunnel_fence_io")?;
    sync_directory(directory)?;
    Ok(archived)
}

/// Same per-user authority directory for embedded, standalone and recovery.
pub fn default_root() -> Result<PathBuf, &'static str> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("WebCodex/state/tunnel-runs"))
            .ok_or("tunnel_fence_path")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))
            .map(|p| p.join("webcodex/tunnel-runs"))
            .ok_or("tunnel_fence_path")
    }
}

#[cfg(test)]
#[path = "run_fence_tests.rs"]
mod tests;
