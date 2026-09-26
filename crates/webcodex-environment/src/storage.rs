use crate::{EnvironmentRecord, SetupDiagnostic, SetupJournal, SetupResultValue};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAX_STATE_BYTES: u64 = 1024 * 1024;

pub fn default_environment_dir() -> SetupResultValue<PathBuf> {
    webcodex_runner_config::paths::default_client_config_base_dir()
        .map(|base| base.join("environment"))
        .map_err(|_| {
            SetupDiagnostic::new(
                "home_unavailable",
                "Could not determine this user's configuration directory",
                "Set an explicit environment directory",
            )
        })
}

pub struct EnvironmentStore {
    root: PathBuf,
}
pub struct EnvironmentLock {
    _file: File,
}

impl EnvironmentStore {
    pub fn open(root: PathBuf) -> SetupResultValue<Self> {
        if !root.is_absolute() {
            return Err(SetupDiagnostic::new(
                "state_path",
                "Environment directory must be absolute",
                "Select an absolute path owned by the project user",
            ));
        }
        ensure_private_directory(&root)?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn lock(&self) -> SetupResultValue<EnvironmentLock> {
        let path = self.root.join("setup.lock");
        let file = match open_private(&path, true) {
            Ok(file) => file,
            Err(_) if path.exists() => open_existing_private(&path, true)?,
            Err(error) => return Err(error),
        };
        file.try_lock_exclusive().map_err(|_| {
            SetupDiagnostic::new(
                "setup_busy",
                "Another process owns environment setup",
                "Wait for the existing setup operation to finish",
            )
        })?;
        Ok(EnvironmentLock { _file: file })
    }
    pub fn load_journal(&self) -> SetupResultValue<Option<SetupJournal>> {
        self.read_json("setup.json")
    }
    pub fn save_journal(&self, journal: &SetupJournal) -> SetupResultValue<()> {
        self.write_json("setup.json", journal)
    }
    pub fn load_environment(&self) -> SetupResultValue<Option<EnvironmentRecord>> {
        self.read_json("environment.json")
    }
    pub fn save_environment(&self, record: &EnvironmentRecord) -> SetupResultValue<()> {
        self.write_json("environment.json", record)
    }
    pub(crate) fn read_json<T: DeserializeOwned>(&self, name: &str) -> SetupResultValue<Option<T>> {
        let path = self.root.join(name);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(SetupDiagnostic::io()),
            Ok(_) => {
                let bytes = read_private(&path)?;
                serde_json::from_slice(&bytes).map(Some).map_err(|_| {
                    SetupDiagnostic::new(
                        "state_invalid",
                        "Saved environment state is invalid",
                        "Restore a valid state backup; do not delete credentials or rerun pairing",
                    )
                })
            }
        }
    }
    pub(crate) fn write_json<T: Serialize>(&self, name: &str, value: &T) -> SetupResultValue<()> {
        let bytes = serde_json::to_vec_pretty(value).map_err(|_| SetupDiagnostic::io())?;
        atomic_private_write(&self.root.join(name), &bytes)
    }
}

pub(crate) fn ensure_private_directory(path: &Path) -> SetupResultValue<()> {
    // Inspect every existing ancestor before creation; never follow a planted link.
    for parent in path.ancestors() {
        if let Ok(metadata) = std::fs::symlink_metadata(parent) {
            if is_link(&metadata) || !metadata.is_dir() {
                return Err(SetupDiagnostic::new(
                    "unsafe_path",
                    "Configuration path contains a link or a non-directory",
                    "Choose a real directory owned by the project user",
                ));
            }
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
        builder.create(path).map_err(|_| SetupDiagnostic::io())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(SetupDiagnostic::new(
                "state_permissions",
                "Environment directory must be private and owned by this user",
                "Correct the directory ownership and permissions before resuming",
            ));
        }
    }
    #[cfg(windows)]
    {
        if created {
            secure_windows_path(path)?;
        }
        validate_windows_private(path)?;
    }
    Ok(())
}

fn is_link(metadata: &std::fs::Metadata) -> bool {
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

fn validate_private_file(file: &File) -> SetupResultValue<()> {
    let metadata = file.metadata().map_err(|_| SetupDiagnostic::io())?;
    if !metadata.is_file() {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(SetupDiagnostic::io());
        }
    }
    Ok(())
}

fn open_existing_private(path: &Path, writable: bool) -> SetupResultValue<File> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(windows)]
    validate_windows_private(path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(writable);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| SetupDiagnostic::io())?;
    validate_private_file(&file)?;
    Ok(file)
}

fn open_private(path: &Path, create_new: bool) -> SetupResultValue<File> {
    if !create_new {
        return open_existing_private(path, true);
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path).map_err(|_| SetupDiagnostic::io())?;
    // No credential bytes are written until the restrictive ACL is installed.
    #[cfg(windows)]
    {
        secure_windows_path(path)?;
        validate_windows_private(path)?;
    }
    validate_private_file(&file)?;
    Ok(file)
}

pub(crate) fn read_private(path: &Path) -> SetupResultValue<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MAX_STATE_BYTES {
        return Err(SetupDiagnostic::io());
    }
    let mut file = open_existing_private(path, false)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_STATE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    if bytes.len() as u64 > MAX_STATE_BYTES {
        return Err(SetupDiagnostic::io());
    }
    Ok(bytes)
}

pub(crate) fn atomic_private_write(path: &Path, bytes: &[u8]) -> SetupResultValue<()> {
    let parent = path.parent().ok_or_else(SetupDiagnostic::io)?;
    ensure_private_directory(parent)?;
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if is_link(&metadata) || !metadata.is_file() {
            return Err(SetupDiagnostic::io());
        }
    }
    let temporary = parent.join(format!(".setup-{}", uuid::Uuid::new_v4().simple()));
    let mut file = open_private(&temporary, true)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| SetupDiagnostic::io())?;
    drop(file);
    #[cfg(not(windows))]
    std::fs::rename(&temporary, path).map_err(|_| SetupDiagnostic::io())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                target.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(SetupDiagnostic::io());
        }
    }
    #[cfg(unix)]
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|_| SetupDiagnostic::io())?;
    Ok(())
}

#[cfg(windows)]
fn secure_windows_path(path: &Path) -> SetupResultValue<()> {
    // The native ACL implementation is shared with service identity validation.
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let sid = crate::service::current_account()
        .map_err(crate::native::service_error)?
        .identity;
    let sddl: Vec<u16> = format!("O:{sid}D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{sid})\0")
        .encode_utf16()
        .collect();
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(SetupDiagnostic::io());
    }
    let mut dacl = std::ptr::null_mut();
    let mut owner = std::ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    let ok = unsafe {
        GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) != 0
            && GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) != 0
    };
    let mut name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = if ok {
        unsafe {
            SetNamedSecurityInfoW(
                name.as_mut_ptr(),
                1,
                DACL_SECURITY_INFORMATION
                    | OWNER_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                owner,
                std::ptr::null_mut(),
                dacl,
                std::ptr::null_mut(),
            )
        }
    } else {
        1
    };
    unsafe {
        windows_sys::Win32::Foundation::LocalFree(descriptor);
    }
    if result == 0 {
        Ok(())
    } else {
        Err(SetupDiagnostic::io())
    }
}

#[cfg(windows)]
fn validate_windows_private(path: &Path) -> SetupResultValue<()> {
    crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| SetupDiagnostic::io())?;
    let expected = crate::service::current_account()
        .map_err(crate::native::service_error)?
        .identity;
    if windows_path_owner(path)? != expected {
        return Err(SetupDiagnostic::io());
    }
    Ok(())
}

#[cfg(windows)]
pub(crate) fn windows_path_owner(path: &Path) -> SetupResultValue<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::OWNER_SECURITY_INFORMATION;
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || owner.is_null() {
        return Err(SetupDiagnostic::io());
    }
    let mut text = std::ptr::null_mut();
    let valid = unsafe { ConvertSidToStringSidW(owner, &mut text) } != 0;
    let value = if valid {
        let mut length = 0;
        unsafe {
            while *text.add(length) != 0 {
                length += 1;
            }
        }
        Some(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(text, length)
        }))
    } else {
        None
    };
    unsafe {
        if !text.is_null() {
            LocalFree(text.cast());
        }
        LocalFree(descriptor);
    }
    value.ok_or_else(SetupDiagnostic::io)
}
