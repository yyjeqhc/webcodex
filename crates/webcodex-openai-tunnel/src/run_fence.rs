//! Host-local run ownership and explicit recovery. No effect is ever replayed.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[path = "run_receipts.rs"]
mod receipts;
pub(crate) use receipts::Receipts;
#[path = "run_transitions.rs"]
mod transitions;
pub use transitions::CredentialTransition;

const PROTECTED_HISTORY_MAGIC: &[u8; 8] = b"WCTHST01";
const PROTECTED_HISTORY_BYTES: usize = 72;
const MAX_AUTO_ARCHIVES: usize = 128;
const MAX_AUTO_ARCHIVE_BYTES: u64 = 64 * 1024;
const MAX_RUN_MARKER_BYTES: u64 = 4096;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunIdentity {
    pub version: u32,
    pub run_id: String,
    pub owner_pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_started: Option<u64>,
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

/// The lock file survives clean stops. Its immutable protected-history header
/// distinguishes a fresh store from missing receipts even without an active run.
fn protected_binding(lock: &mut File) -> Result<Option<[u8; 32]>, &'static str> {
    Ok(transitions::read_history(lock)?.map(|history| history.original_binding))
}

fn initialize_protected_history(
    lock: &mut File,
    binding: [u8; 32],
    root: &Path,
) -> Result<(), &'static str> {
    let mut header = [0; PROTECTED_HISTORY_BYTES];
    header[..8].copy_from_slice(PROTECTED_HISTORY_MAGIC);
    header[8..40].copy_from_slice(&binding);
    let checksum = Sha256::digest(&header[..40]);
    header[40..].copy_from_slice(&checksum);
    lock.seek(SeekFrom::Start(0))
        .map_err(|_| "tunnel_fence_io")?;
    lock.write_all(&header)
        .and_then(|_| lock.sync_all())
        .map_err(|_| "tunnel_fence_io")?;
    sync_directory(root)
}

fn missing(path: &Path) -> Result<bool, &'static str> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) => Err("tunnel_fence_io"),
    }
}

fn archive_directory(path: &Path) -> Result<PathBuf, &'static str> {
    Ok(path.with_file_name(format!(
        "{}.recovered",
        path.file_name()
            .ok_or("tunnel_fence_path")?
            .to_string_lossy()
    )))
}

/// Read at most the count budget, rejecting unknown file kinds and oversized
/// markers. This preflight runs before journal repair or any history mutation.
/// With no protected header, only exact, explicitly archived v2 runs establish
/// reconciled legacy history. Empty, mixed or protected archives remain uncertain.
fn check_archives(
    directory: &Path,
    incoming: Option<u64>,
    require_legacy: bool,
    history: Option<&transitions::ProtectedHistory>,
) -> Result<(), &'static str> {
    if incoming.is_some_and(|bytes| bytes > MAX_RUN_MARKER_BYTES) {
        return Err("tunnel_archive_invalid");
    }
    let meta = match fs::symlink_metadata(directory) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("tunnel_fence_io"),
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("tunnel_archive_invalid");
    }
    let entries = fs::read_dir(directory).map_err(|_| "tunnel_fence_io")?;
    let mut count = usize::from(incoming.is_some());
    let mut bytes = incoming.unwrap_or(0);
    for entry in entries {
        let entry = entry.map_err(|_| "tunnel_fence_io")?;
        count += 1;
        if count > MAX_AUTO_ARCHIVES {
            return Err("tunnel_archive_capacity");
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| "tunnel_fence_io")?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RUN_MARKER_BYTES {
            return Err("tunnel_archive_invalid");
        }
        bytes = bytes
            .checked_add(meta.len())
            .ok_or("tunnel_archive_capacity")?;
        if bytes > MAX_AUTO_ARCHIVE_BYTES {
            return Err("tunnel_archive_capacity");
        }
        if require_legacy || history.is_some() {
            let identity = read(&entry.path())
                .map_err(|_| "tunnel_archive_invalid")?
                .ok_or("tunnel_archive_invalid")?;
            if require_legacy && identity.version != 2 {
                return Err("tunnel_protected_history_missing");
            }
            if identity.version == 3
                && !history.is_some_and(|history| {
                    identity
                        .receipt_binding
                        .as_deref()
                        .is_some_and(|text| history.contains_binding_text(text))
                })
            {
                return Err("tunnel_archive_binding_changed");
            }
            if entry.file_name().to_str() != Some(identity.run_id.as_str()) {
                return Err("tunnel_archive_invalid");
            }
        }
    }
    if require_legacy && count == 0 {
        return Err("tunnel_protected_history_missing");
    }
    Ok(())
}
fn read(path: &Path) -> Result<Option<RunIdentity>, &'static str> {
    let meta = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("tunnel_fence_io"),
        Ok(meta) => meta,
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_RUN_MARKER_BYTES {
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
    if !matches!(identity.version, 2 | 3)
        || identity.owner_pid == 0
        || !uuid::Uuid::parse_str(&identity.run_id)
            .is_ok_and(|id| id.to_string() == identity.run_id)
    {
        return Err("tunnel_fence_invalid");
    }
    if identity.owner_started == Some(0)
        || (identity.version == 2
            && (identity.receipt_binding.is_some() || identity.owner_started.is_some()))
        || (identity.version == 3
            && !identity.receipt_binding.as_ref().is_some_and(|binding| {
                binding.len() == 64
                    && binding
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }))
    {
        return Err("tunnel_fence_invalid");
    }
    Ok(Some(identity))
}
impl RunFence {
    pub fn acquire(root: &Path, origin: &str, tunnel: &str) -> Result<Self, &'static str> {
        let (path, mut lock) = lock(root, origin, tunnel)?;
        if protected_binding(&mut lock)?.is_some() {
            return Err("tunnel_receipts_required");
        }
        if !missing(&path.with_extension("transitions"))? {
            return Err("tunnel_protected_history_missing");
        }
        match fs::symlink_metadata(path.with_extension("receipts")) {
            // Legacy callers must not bypass history left by a protected run.
            Ok(_) => return Err("tunnel_receipts_required"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("tunnel_fence_io"),
        }
        check_archives(&archive_directory(&path)?, None, true, None)?;
        let identity = RunIdentity {
            version: 2,
            run_id: uuid::Uuid::new_v4().to_string(),
            owner_pid: std::process::id(),
            receipt_binding: None,
            owner_started: None,
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

    /// Recovery is admitted only for runs protected by the same durable receipt
    /// journal. Old v2 fences have no across-restart receipts and remain blocked.
    pub(crate) fn acquire_restartable(
        root: &Path,
        origin: &str,
        tunnel: &str,
        binding: [u8; 32],
        owner_started: Option<u64>,
        owner_absent: impl FnOnce(&RunIdentity) -> bool,
    ) -> Result<(Self, Receipts, Option<u64>), &'static str> {
        let (path, mut lock) = lock(root, origin, tunnel)?;
        let previous = read(&path)?;
        let binding_text = format!("{:x}", Sha256::digest(binding));
        let history = transitions::read_history(&mut lock)?;
        if let Some(history) = history.as_ref() {
            let _ = transitions::check_intents(&path, history, false)?;
        } else if !missing(&path.with_extension("transitions"))? {
            return Err("tunnel_protected_history_missing");
        }
        if let Some(identity) = &previous {
            if identity.version != 3
                || identity.receipt_binding.as_ref() != Some(&binding_text)
                || !history
                    .as_ref()
                    .is_some_and(|history| history.current_binding() == binding)
            {
                return Err("tunnel_restart_uncertain");
            }
            if !owner_absent(identity) {
                return Err("tunnel_owner_active");
            }
        }
        let journal_path = path.with_extension("receipts");
        let directory = archive_directory(&path)?;
        let initialize = match history.as_ref() {
            Some(saved) if saved.current_binding() == binding => false,
            Some(_) => return Err("tunnel_protected_history_binding_changed"),
            None => {
                // Surviving protected evidence cannot become a fresh namespace
                // just because one independent state file was lost/restored.
                if previous.is_some() || !missing(&journal_path)? {
                    return Err("tunnel_protected_history_missing");
                }
                check_archives(&directory, None, true, None)?;
                true
            }
        };
        let archived = if let Some(identity) = &previous {
            let archived = directory.join(&identity.run_id);
            if !missing(&archived)? {
                return Err("tunnel_archive_exists");
            }
            let incoming = fs::symlink_metadata(&path)
                .map_err(|_| "tunnel_fence_io")?
                .len();
            check_archives(&directory, Some(incoming), false, history.as_ref())?;
            Some(archived)
        } else {
            if !initialize {
                check_archives(&directory, None, false, history.as_ref())?;
            }
            None
        };
        if initialize {
            // Publish permanent protection before journal creation/admission.
            // Interrupted initialization stays blocked; it never reseeds history.
            initialize_protected_history(&mut lock, binding, root)?;
        }
        // Existing protection requires receipts after *both* clean and dirty stops.
        let original_binding = history
            .as_ref()
            .map_or(binding, |history| history.original_binding);
        let cutoff_seconds = history
            .as_ref()
            .and_then(|history| history.cutoff_seconds());
        let journal = Receipts::open(&journal_path, original_binding, !initialize)?;
        if let Some(archived) = archived {
            private_directory(&directory)?;
            fs::rename(&path, archived).map_err(|_| "tunnel_fence_io")?;
            sync_directory(&directory)?;
        }
        let identity = RunIdentity {
            version: 3,
            run_id: uuid::Uuid::new_v4().to_string(),
            owner_pid: std::process::id(),
            receipt_binding: Some(binding_text),
            owner_started,
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
        Ok((
            Self {
                path,
                identity,
                _lock: lock,
            },
            journal,
            cutoff_seconds,
        ))
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
    let archived = archive_directory(&path)?.join(expected_run);
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
