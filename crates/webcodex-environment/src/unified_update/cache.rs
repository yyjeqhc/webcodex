//! An application-owned cache, not an EnvironmentStore. Reuses Core's private
//! file/ACL primitives without creating an Environment or changing its authority.
use super::{stable_version, UpdateError, UpdateResult};
use crate::storage::{
    atomic_private_write, ensure_private_directory, open_existing_private, open_private,
};
use fs2::FileExt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PrivateUpdateCache {
    root: PathBuf,
    readonly: bool,
    existing_only: bool,
}

/// The operation scope owns the advisory fence even if a concurrent process
/// spawn temporarily inherits its close-on-exec descriptor.
pub struct PrivateUpdateLock {
    file: File,
}

impl Drop for PrivateUpdateLock {
    fn drop(&mut self) {
        // Closing only our descriptor can leave flock held by an inherited
        // open-file description. Release the fence before closing the handle.
        let _ = FileExt::unlock(&self.file);
    }
}

fn failed<T>(_: T) -> UpdateError {
    UpdateError::CacheUnavailable
}
fn leaf(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}
fn link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_symlink()
    }
}

impl PrivateUpdateCache {
    pub fn open(root: PathBuf) -> UpdateResult<Self> {
        if !root.is_absolute() {
            return Err(UpdateError::CacheUnavailable);
        }
        ensure_private_directory(&root).map_err(failed)?;
        Ok(Self {
            root,
            readonly: false,
            existing_only: false,
        })
    }
    /// Read-only admission: missing cache stays missing and no lock is created.
    pub fn open_existing(root: PathBuf) -> UpdateResult<Option<Self>> {
        let store = crate::EnvironmentStore::open_existing(root.clone()).map_err(failed)?;
        Ok(store.map(|_| Self {
            root,
            readonly: true,
            existing_only: true,
        }))
    }
    /// Explicit reconciliation may update admitted existing state, but cannot
    /// provision a missing cache root or candidate directory.
    pub(super) fn open_existing_for_update(root: PathBuf) -> UpdateResult<Option<Self>> {
        Ok(Self::open_existing(root)?.map(|mut cache| {
            cache.readonly = false;
            cache
        }))
    }
    pub(super) fn lock_existing_exclusive(&self) -> UpdateResult<PrivateUpdateLock> {
        let file = open_existing_private(&self.file("update.lock")?, true).map_err(failed)?;
        file.try_lock_exclusive().map_err(failed)?;
        Ok(PrivateUpdateLock { file })
    }
    pub fn existing_child(&self, name: &str) -> UpdateResult<Option<Self>> {
        if !leaf(name) {
            return Err(UpdateError::CacheUnavailable);
        }
        Self::open_existing(self.root.join(name))
    }
    pub fn lock_existing(&self) -> UpdateResult<PrivateUpdateLock> {
        let file = open_existing_private(&self.root.join("update.lock"), false).map_err(failed)?;
        file.try_lock_shared().map_err(failed)?;
        Ok(PrivateUpdateLock { file })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn file(&self, name: &str) -> UpdateResult<PathBuf> {
        if !leaf(name) {
            return Err(UpdateError::CacheUnavailable);
        }
        if self.existing_only {
            if crate::EnvironmentStore::open_existing(self.root.clone())
                .map_err(failed)?
                .is_none()
            {
                return Err(UpdateError::CacheUnavailable);
            }
        } else {
            ensure_private_directory(&self.root).map_err(failed)?;
        }
        Ok(self.root.join(name))
    }
    pub fn child(&self, name: &str) -> UpdateResult<Self> {
        if self.existing_only {
            let mut child = self
                .existing_child(name)?
                .ok_or(UpdateError::CacheUnavailable)?;
            child.readonly = self.readonly;
            return Ok(child);
        }
        Self::open(self.file(name)?)
    }
    pub fn lock(&self) -> UpdateResult<PrivateUpdateLock> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        if self.existing_only {
            return self.lock_existing_exclusive();
        }
        let path = self.file("update.lock")?;
        let file = if path.exists() {
            open_existing_private(&path, true)
        } else {
            open_private(&path, true)
        }
        .map_err(failed)?;
        file.try_lock_exclusive().map_err(failed)?;
        Ok(PrivateUpdateLock { file })
    }
    pub fn read(&self, name: &str, limit: u64) -> UpdateResult<Option<Vec<u8>>> {
        let path = self.file(name)?;
        if !path.try_exists().map_err(failed)? {
            return Ok(None);
        }
        let mut file = open_existing_private(&path, false).map_err(failed)?;
        if file.metadata().map_err(failed)?.len() > limit {
            return Err(UpdateError::CacheUnavailable);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(failed)?;
        if bytes.len() as u64 > limit {
            return Err(UpdateError::CacheUnavailable);
        }
        Ok(Some(bytes))
    }
    pub fn write(&self, name: &str, bytes: &[u8]) -> UpdateResult<()> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        if bytes.len() > 1024 * 1024 {
            return Err(UpdateError::CacheUnavailable);
        }
        atomic_private_write(&self.file(name)?, bytes).map_err(failed)
    }
    pub fn open_file(&self, name: &str) -> UpdateResult<File> {
        open_existing_private(&self.file(name)?, false).map_err(failed)
    }
    pub fn create_file(&self, name: &str) -> UpdateResult<File> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        open_private(&self.file(name)?, true).map_err(failed)
    }
    pub fn remove_file(&self, name: &str) -> UpdateResult<()> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        let path = self.file(name)?;
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(UpdateError::CacheUnavailable),
            Ok(metadata) => {
                if link(&metadata) || !metadata.is_file() {
                    return Err(UpdateError::CacheUnavailable);
                }
                drop(open_existing_private(&path, false).map_err(failed)?);
                std::fs::remove_file(path).map_err(failed)
            }
        }
    }
    /// Caller streamed and hashed the private part. No final name is visible
    /// until the bytes and containing directory are durably committed.
    pub fn commit(&self, partial: &str, final_name: &str) -> UpdateResult<()> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        let part = self.file(partial)?;
        let target = self.file(final_name)?;
        if target.try_exists().map_err(failed)? {
            return Err(UpdateError::CacheUnavailable);
        }
        // Windows FlushFileBuffers requires a handle opened for writing.
        // Reopen the completed private part writable, flush it, then close the
        // handle before the atomic rename so the durability contract is the same
        // on every supported platform.
        let part_file = open_existing_private(&part, true).map_err(failed)?;
        part_file.sync_all().map_err(failed)?;
        drop(part_file);
        std::fs::rename(&part, &target).map_err(failed)?;
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|file| file.sync_all())
            .map_err(failed)?;
        Ok(())
    }
    /// Root entries outside our canonical version/temporary namespace are not
    /// touched. Recursion never follows links, and work is bounded.
    pub fn retain_version(&self, keep: Option<&str>) -> UpdateResult<()> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        if self.existing_only {
            self.file("update-state.json")?;
        } else {
            ensure_private_directory(&self.root).map_err(failed)?;
        }
        if keep.is_some_and(|value| !stable_version(value)) {
            return Err(UpdateError::CacheUnavailable);
        }
        let mut remaining = 50_000usize;
        for (index, entry) in std::fs::read_dir(&self.root).map_err(failed)?.enumerate() {
            if index >= 128 {
                return Err(UpdateError::CacheUnavailable);
            }
            let entry = entry.map_err(failed)?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if keep == Some(name.as_str()) {
                continue;
            }
            let temporary = name
                .strip_prefix(".setup-")
                .is_some_and(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()));
            if stable_version(&name) || temporary {
                remove_tree(&entry.path(), 0, &mut remaining)?;
            }
        }
        Ok(())
    }
    pub fn remove_child_tree(&self, name: &str) -> UpdateResult<()> {
        if self.readonly {
            return Err(UpdateError::CacheUnavailable);
        }
        let path = self.file(name)?;
        if std::fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(());
        }
        remove_tree(&path, 0, &mut 50_000)
    }
}

fn remove_tree(path: &Path, depth: usize, remaining: &mut usize) -> UpdateResult<()> {
    if depth > 32 || *remaining == 0 {
        return Err(UpdateError::CacheUnavailable);
    }
    *remaining -= 1;
    let metadata = std::fs::symlink_metadata(path).map_err(failed)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(UpdateError::CacheUnavailable);
        }
    }
    if link(&metadata) || !metadata.is_dir() {
        #[cfg(windows)]
        if metadata.is_dir() {
            return std::fs::remove_dir(path).map_err(failed);
        }
        return std::fs::remove_file(path).map_err(failed);
    }
    for entry in std::fs::read_dir(path).map_err(failed)? {
        remove_tree(&entry.map_err(failed)?.path(), depth + 1, remaining)?;
    }
    std::fs::remove_dir(path).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn existing_update_cache_never_provisions_roots_children_or_fences() {
        let temp = crate::test_tempdir().unwrap();
        let root = temp.path().join("existing-update");
        assert!(PrivateUpdateCache::open_existing_for_update(root.clone())
            .unwrap()
            .is_none());
        assert!(!root.exists());
        PrivateUpdateCache::open(root.clone()).unwrap();
        let update = PrivateUpdateCache::open_existing_for_update(root.clone())
            .unwrap()
            .unwrap();
        assert!(update.lock().is_err());
        assert!(!root.join("update.lock").exists());
        assert!(update.child("1.2.3").is_err());
        assert!(!root.join("1.2.3").exists());
        std::fs::remove_dir(&root).unwrap();
        assert!(update.write("update-state.json", b"state").is_err());
        assert!(update.retain_version(None).is_err());
        assert!(!root.exists());
    }
    #[test]
    fn readonly_cache_never_creates_missing_paths_or_recreates_removed_root() {
        let temp = crate::test_tempdir().unwrap();
        let root = temp.path().join("readonly");
        assert!(PrivateUpdateCache::open_existing(root.clone())
            .unwrap()
            .is_none());
        assert!(!root.exists());
        let cache = PrivateUpdateCache::open(root.clone()).unwrap();
        cache.write("fixture", b"value").unwrap();
        let readonly = PrivateUpdateCache::open_existing(root.clone())
            .unwrap()
            .unwrap();
        assert!(readonly.existing_child("missing").unwrap().is_none());
        assert!(readonly.write("new", b"no").is_err());
        assert!(!root.join("new").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let before = std::fs::metadata(&root).unwrap();
            assert_eq!(readonly.read("fixture", 16).unwrap().unwrap(), b"value");
            let after = std::fs::metadata(&root).unwrap();
            assert_eq!(before.mode(), after.mode());
            assert_eq!(
                (before.ctime(), before.ctime_nsec()),
                (after.ctime(), after.ctime_nsec())
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
        assert!(readonly.read("fixture", 16).is_err());
        assert!(!root.exists());
    }

    #[test]
    fn parts_are_private_atomic_and_cleanup_is_scoped() {
        let temp = crate::test_tempdir().unwrap();
        let cache = PrivateUpdateCache::open(temp.path().join("updates")).unwrap();
        let target = cache.child("1.2.3").unwrap().child("darwin-arm64").unwrap();
        let mut part = target.create_file("installer.part").unwrap();
        part.write_all(b"verified").unwrap();
        part.sync_all().unwrap();
        drop(part);
        assert!(!target.file("installer.pkg").unwrap().exists());
        target.commit("installer.part", "installer.pkg").unwrap();
        assert_eq!(
            target.read("installer.pkg", 8).unwrap().unwrap(),
            b"verified"
        );
        assert!(target.read("installer.pkg", 7).is_err());
        assert!(target.file("../outside").is_err());
        cache.write("user-note.txt", b"not ours to remove").unwrap();
        cache.retain_version(None).unwrap();
        assert!(cache.file("user-note.txt").unwrap().exists());
        assert!(!cache.file("1.2.3").unwrap().exists());
    }
    #[test]
    fn cache_has_cross_process_attempt_exclusion() {
        let temp = crate::test_tempdir().unwrap();
        let cache = PrivateUpdateCache::open(temp.path().join("updates")).unwrap();
        let lock = cache.lock().unwrap();
        assert!(cache.lock().is_err());
        drop(lock);
        assert!(cache.lock().is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn cache_attempt_lock_release_does_not_wait_for_an_inherited_handle() {
        for mode in ["new_exclusive", "existing_exclusive", "existing_shared"] {
            let temp = crate::test_tempdir().unwrap();
            let cache = PrivateUpdateCache::open(temp.path().join("updates")).unwrap();
            drop(cache.lock().unwrap());
            let existing = PrivateUpdateCache::open_existing_for_update(cache.root.clone())
                .unwrap()
                .unwrap();
            let lock = match mode {
                "new_exclusive" => cache.lock().unwrap(),
                "existing_exclusive" => existing.lock_existing_exclusive().unwrap(),
                _ => existing.lock_existing().unwrap(),
            };
            assert!(existing.lock_existing_exclusive().is_err(), "{mode}");
            // dup and fork share the same open-file description. A concurrent
            // process spawn can retain this descriptor until its close-on-exec.
            let inherited = lock.file.try_clone().unwrap();
            drop(lock);
            assert!(existing.lock_existing_exclusive().is_ok(), "{mode}");
            drop(inherited);
        }
    }
    #[cfg(unix)]
    #[test]
    fn planted_links_never_redirect_writes_reads_or_cleanup() {
        use std::os::unix::fs::symlink;
        let temp = crate::test_tempdir().unwrap();
        let cache = PrivateUpdateCache::open(temp.path().join("updates")).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("keep"), b"keep").unwrap();
        symlink(&outside, cache.root.join("1.2.3")).unwrap();
        assert!(cache.child("1.2.3").is_err());
        cache.retain_version(None).unwrap();
        assert!(outside.join("keep").exists());
        symlink(outside.join("keep"), cache.root.join("state.json")).unwrap();
        assert!(cache.read("state.json", 100).is_err());
        assert!(cache.write("state.json", b"bad").is_err());
    }
}
