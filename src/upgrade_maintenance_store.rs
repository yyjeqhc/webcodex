//! Private recovery file for the Server admission fence. It lives in the data
//! snapshot so a restarted Server remains closed until the original owner ends
//! the lease. File contents are never included in diagnostics or ordinary logs.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use webcodex_runner_registry::{MaintenanceStore, SavedMaintenanceLease};

const FILE: &str = "upgrade-maintenance.secret";
const MAX_BYTES: usize = 4096;

#[cfg(windows)]
fn windows_owner_sid(path: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = null_mut();
    let mut descriptor = null_mut();
    if unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    } != 0
        || owner.is_null()
    {
        return Err("maintenance owner unavailable".into());
    }
    let mut wide = null_mut();
    let ok = unsafe { ConvertSidToStringSidW(owner, &mut wide) } != 0;
    let result = if ok && !wide.is_null() {
        let mut len = 0;
        unsafe {
            while *wide.add(len) != 0 {
                len += 1;
            }
        }
        Some(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(wide, len)
        }))
    } else {
        None
    };
    unsafe {
        if !wide.is_null() {
            LocalFree(wide.cast());
        }
        LocalFree(descriptor);
    }
    result.ok_or_else(|| "maintenance owner unavailable".into())
}

#[cfg(windows)]
fn restrict_windows_file(path: &Path, parent: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let owner = windows_owner_sid(parent)?;
    let service = webcodex_environment::service::current_account()
        .map_err(|_| "maintenance service identity unavailable")?
        .identity;
    let sddl: Vec<u16> =
        format!("D:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{owner})(A;;FA;;;{service})\0")
            .encode_utf16()
            .collect();
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err("maintenance ACL creation failed".into());
    }
    let mut dacl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    let valid =
        unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
            != 0
            && present != 0
            && !dacl.is_null();
    let mut name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let status = if valid {
        unsafe {
            SetNamedSecurityInfoW(
                name.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                dacl,
                null_mut(),
            )
        }
    } else {
        1
    };
    unsafe {
        LocalFree(descriptor);
    }
    if status == 0 {
        Ok(())
    } else {
        Err("maintenance ACL installation failed".into())
    }
}

pub(crate) struct FileMaintenanceStore {
    path: PathBuf,
}

impl std::fmt::Debug for FileMaintenanceStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileMaintenanceStore")
            .finish_non_exhaustive()
    }
}

impl FileMaintenanceStore {
    pub(crate) fn new(data_dir: &Path) -> Result<Self, String> {
        let metadata =
            fs::symlink_metadata(data_dir).map_err(|_| "maintenance data directory unavailable")?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err("unsafe maintenance data directory".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("unsafe maintenance data directory".into());
            }
        }
        Ok(Self {
            path: data_dir.join(FILE),
        })
    }

    fn check_file(path: &Path) -> Result<(), String> {
        let metadata = fs::symlink_metadata(path).map_err(|_| "maintenance file unavailable")?;
        if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MAX_BYTES as u64 {
            return Err("unsafe maintenance file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
                return Err("unsafe maintenance file permissions".into());
            }
            let parent = fs::metadata(path.parent().ok_or("maintenance parent unavailable")?)
                .map_err(|_| "maintenance parent unavailable")?;
            if metadata.uid() != parent.uid() {
                return Err("maintenance file owner mismatch".into());
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("unsafe maintenance file".into());
            }
            webcodex_environment::runtime_entry::validate_windows_env_acl(path)
                .map_err(|_| "unsafe maintenance file ACL")?;
        }
        Ok(())
    }

    fn sync_parent(&self) -> Result<(), String> {
        #[cfg(unix)]
        File::open(self.path.parent().ok_or("maintenance parent unavailable")?)
            .and_then(|file| file.sync_all())
            .map_err(|_| String::from("maintenance directory sync failed"))?;
        Ok(())
    }
}

impl MaintenanceStore for FileMaintenanceStore {
    fn load(&self) -> Result<Option<SavedMaintenanceLease>, String> {
        match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("maintenance file metadata unavailable".into()),
            Ok(_) => Self::check_file(&self.path)?,
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(&self.path)
            .map_err(|_| "maintenance file unreadable")?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| "maintenance file read failed")?;
        if bytes.len() > MAX_BYTES {
            return Err("maintenance file exceeds bound".into());
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "maintenance file invalid".into())
    }

    fn save(&self, lease: &SavedMaintenanceLease) -> Result<(), String> {
        if self.path.exists() {
            Self::check_file(&self.path)?;
        }
        let bytes = serde_json::to_vec(lease).map_err(|_| "maintenance serialization failed")?;
        if bytes.len() > MAX_BYTES {
            return Err("maintenance file exceeds bound".into());
        }
        let parent = self.path.parent().ok_or("maintenance parent unavailable")?;
        let temporary = parent.join(format!(
            ".upgrade-maintenance-{}.tmp",
            uuid::Uuid::new_v4().simple()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "maintenance temporary file unavailable")?;
        #[cfg(windows)]
        restrict_windows_file(&temporary, parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let owner = fs::metadata(parent).map_err(|_| "maintenance parent unavailable")?;
            let current = file
                .metadata()
                .map_err(|_| "maintenance temporary file unavailable")?;
            if current.uid() != owner.uid() || current.gid() != owner.gid() {
                use std::os::fd::AsRawFd;
                if unsafe { libc::fchown(file.as_raw_fd(), owner.uid(), owner.gid()) } != 0 {
                    return Err("maintenance file owner could not be set".into());
                }
            }
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "maintenance file permissions could not be set")?;
        }
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "maintenance file write failed")?;
        drop(file);
        Self::check_file(&temporary)?;
        #[cfg(not(windows))]
        fs::rename(&temporary, &self.path).map_err(|_| "maintenance file replace failed")?;
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let target: Vec<u16> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    source.as_ptr(),
                    target.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err("maintenance file replace failed".into());
            }
        }
        self.sync_parent()
    }

    fn clear(&self) -> Result<(), String> {
        Self::check_file(&self.path)?;
        fs::remove_file(&self.path).map_err(|_| "maintenance file remove failed")?;
        // A failed directory sync may leave the old fence on disk after a
        // crash, which is safe; reopening admission after unlink is safe too.
        let _ = self.sync_parent();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use webcodex_runner_registry::{MaintenanceScope, RunnerAccess, RunnerRegistry};

    #[tokio::test]
    async fn restart_restores_gate_until_original_token_ends_it() {
        let data = tempfile::tempdir().unwrap();
        let mut first = RunnerRegistry::default();
        first
            .attach_maintenance_store(Arc::new(FileMaintenanceStore::new(data.path()).unwrap()))
            .await
            .unwrap();
        let bootstrap = RunnerAccess {
            global_visibility: true,
            owner_bypass: true,
            username: None,
            group: None,
        };
        let grant = first
            .begin_maintenance(
                MaintenanceScope::AllRuntimes,
                "bootstrap",
                &"a".repeat(64),
                &bootstrap,
            )
            .await
            .unwrap();
        assert!(data.path().join(FILE).is_file());
        drop(first);

        let mut second = RunnerRegistry::default();
        second
            .attach_maintenance_store(Arc::new(FileMaintenanceStore::new(data.path()).unwrap()))
            .await
            .unwrap();
        assert_ne!(
            grant.server_epoch,
            second.observation_epoch_for_maintenance()
        );
        assert!(second.admit_runtime_call().await.is_err());
        assert!(second
            .begin_maintenance(
                MaintenanceScope::AllRuntimes,
                "other",
                &"b".repeat(64),
                &bootstrap
            )
            .await
            .is_err());
        second
            .end_maintenance("bootstrap", grant.token())
            .await
            .unwrap();
        assert!(!data.path().join(FILE).exists());
        assert!(second.admit_runtime_call().await.is_ok());

        let mut third = RunnerRegistry::default();
        third
            .attach_maintenance_store(Arc::new(FileMaintenanceStore::new(data.path()).unwrap()))
            .await
            .unwrap();
        assert!(third.admit_runtime_call().await.is_ok());
    }

    #[tokio::test]
    async fn invalid_recovery_file_aborts_startup() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join(FILE), b"not-json").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(data.path().join(FILE), fs::Permissions::from_mode(0o600)).unwrap();
        }
        let mut registry = RunnerRegistry::default();
        assert!(registry
            .attach_maintenance_store(Arc::new(FileMaintenanceStore::new(data.path()).unwrap()))
            .await
            .is_err());
    }
}
