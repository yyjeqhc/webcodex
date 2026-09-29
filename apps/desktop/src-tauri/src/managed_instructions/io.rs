//! Narrow native I/O for exactly instructions/AGENTS.md. Unix operations are
//! relative to held no-follow directory descriptors. Windows holds no-reparse
//! directories without write/delete sharing through atomic replacement.
use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;

pub(super) struct Directory {
    root_path: PathBuf,
    path: PathBuf,
    root: File,
    directory: File,
}

impl Directory {
    pub(super) fn open(root: &Path, create: bool) -> DesktopResult<Option<Self>> {
        let root_file = match open_directory(root) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    return Ok(None);
                }
                // First-run Desktop can have an observed physical data root
                // whose final directory has not been created yet. Only explicit
                // save/enable creates it; opening below rechecks the owned leaf.
                let mut builder = std::fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(root).map_err(invalid_io)?;
                open_directory(root).map_err(invalid_io)?
            }
            Err(error) => return Err(invalid_io(error)),
        };
        if !safe_metadata(&root_file.metadata().map_err(invalid_io)?, true) {
            return Err(unavailable());
        }
        let path = root.join("instructions");
        let directory = match open_child_directory(&root_file, &path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !create {
                    return Ok(None);
                }
                create_child_directory(&root_file, &path).map_err(invalid_io)?;
                open_child_directory(&root_file, &path).map_err(invalid_io)?
            }
            Err(error) => return Err(invalid_io(error)),
        };
        if !safe_metadata(&directory.metadata().map_err(invalid_io)?, true) {
            return Err(unavailable());
        }
        let result = Self {
            root_path: root.into(),
            path,
            root: root_file,
            directory,
        };
        result.verify_location()?;
        Ok(Some(result))
    }

    fn verify_location(&self) -> DesktopResult<()> {
        for (path, handle) in [(&self.root_path, &self.root), (&self.path, &self.directory)] {
            let metadata = std::fs::symlink_metadata(path).map_err(invalid_io)?;
            if !safe_metadata(&metadata, true) {
                return Err(unavailable());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let held = handle.metadata().map_err(invalid_io)?;
                if metadata.dev() != held.dev() || metadata.ino() != held.ino() {
                    return Err(unavailable());
                }
            }
            #[cfg(not(unix))]
            let _ = handle;
        }
        Ok(())
    }

    pub(super) fn read(&self) -> DesktopResult<Option<String>> {
        self.verify_location()?;
        let opened = self.open_file("AGENTS.md", false);
        let content = match opened {
            Ok(file) => Some(decode(file)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(invalid_io(error)),
        };
        self.verify_location()?;
        Ok(content)
    }

    pub(super) fn replace(&self, content: &str, expected: &str) -> DesktopResult<()> {
        self.replace_with_hook(content, expected, || {})
    }

    fn replace_with_hook(
        &self,
        content: &str,
        expected: &str,
        before_commit: impl FnOnce(),
    ) -> DesktopResult<()> {
        self.verify_location()?;
        let name = format!(".AGENTS.{}.tmp", uuid::Uuid::new_v4());
        let stage_error = |stage: &'static str, error: std::io::Error| {
            unavailable().with_details(serde_json::json!({
                "stage": stage,
                "os_error": error.raw_os_error(),
                "error_kind": format!("{:?}", error.kind()),
            }))
        };
        let mut file = self
            .open_file(&name, true)
            .map_err(|error| stage_error("create_staging", error))?;
        let prepared = (|| {
            file.write_all(content.as_bytes())
                .map_err(|error| stage_error("write_staging", error))?;
            file.sync_all()
                .map_err(|error| stage_error("sync_staging", error))?;
            drop(file);
            before_commit();
            if revision(self.read()?.as_deref()) != expected {
                return Err(conflict());
            }
            self.verify_location()?;
            self.rename(&name)
                .map_err(|error| stage_error("atomic_replace", error))?;
            Ok(())
        })();
        if prepared.is_err() {
            self.remove(&name);
        }
        prepared
    }

    #[cfg(unix)]
    fn open_file(&self, name: &str, create: bool) -> std::io::Result<File> {
        use std::os::fd::AsRawFd;
        let name = std::ffi::CString::new(name)?;
        let flags = libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if create {
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL
            } else {
                libc::O_RDONLY
            };
        from_fd(unsafe { libc::openat(self.directory.as_raw_fd(), name.as_ptr(), flags, 0o600) })
    }
    #[cfg(windows)]
    fn open_file(&self, name: &str, create: bool) -> std::io::Result<File> {
        use std::os::windows::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options
            .read(!create)
            .write(create)
            .create_new(create)
            .custom_flags(0x00200000); // OPEN_REPARSE_POINT
        options.open(self.path.join(name))
    }
    #[cfg(unix)]
    fn rename(&self, name: &str) -> std::io::Result<()> {
        use std::os::fd::AsRawFd;
        let source = std::ffi::CString::new(name)?;
        let destination = c"AGENTS.md";
        if unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                source.as_ptr(),
                self.directory.as_raw_fd(),
                destination.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        // Same guarantee as other Desktop atomic files: synced file, atomic
        // rename, best-effort directory durability on macOS filesystems.
        match self.directory.sync_all() {
            Ok(()) => Ok(()),
            Err(error)
                if error.kind() == std::io::ErrorKind::Unsupported
                    || error.raw_os_error() == Some(libc::EINVAL) =>
            {
                Ok(())
            }
            Err(_) => Ok(()), // The replacement is already committed; never claim it was rolled back.
        }
    }
    #[cfg(windows)]
    fn rename(&self, name: &str) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let from: Vec<u16> = self
            .path
            .join(name)
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let to: Vec<u16> = self
            .path
            .join("AGENTS.md")
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(unix)]
    fn remove(&self, name: &str) {
        use std::os::fd::AsRawFd;
        if let Ok(name) = std::ffi::CString::new(name) {
            unsafe {
                libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0);
            }
        }
    }
    #[cfg(windows)]
    fn remove(&self, name: &str) {
        let _ = std::fs::remove_file(self.path.join(name));
    }
}

#[cfg(unix)]
fn from_fd(fd: libc::c_int) -> std::io::Result<File> {
    use std::os::fd::FromRawFd;
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
#[cfg(unix)]
fn open_directory(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}
#[cfg(unix)]
fn open_child_directory(root: &File, _: &Path) -> std::io::Result<File> {
    use std::os::fd::AsRawFd;
    from_fd(unsafe {
        libc::openat(
            root.as_raw_fd(),
            c"instructions".as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })
}
#[cfg(unix)]
fn create_child_directory(root: &File, _: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    if unsafe { libc::mkdirat(root.as_raw_fd(), c"instructions".as_ptr(), 0o700) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    Ok(())
}
#[cfg(windows)]
fn open_directory(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path) // BACKUP_SEMANTICS | OPEN_REPARSE_POINT, READ sharing only
}
#[cfg(windows)]
fn open_child_directory(_: &File, path: &Path) -> std::io::Result<File> {
    open_directory(path)
}
#[cfg(windows)]
fn create_child_directory(_: &File, path: &Path) -> std::io::Result<()> {
    match std::fs::create_dir(path) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn directory_replacement_cannot_redirect_atomic_commit() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let directory = Directory::open(root.path(), true).unwrap().unwrap();
        directory.replace("original", "missing").unwrap();
        let error = directory
            .replace_with_hook("bad", &revision(Some("original")), || {
                std::fs::rename(
                    root.path().join("instructions"),
                    root.path().join("old-instructions"),
                )
                .unwrap();
                std::os::unix::fs::symlink(outside.path(), root.path().join("instructions"))
                    .unwrap();
            })
            .unwrap_err();
        assert_eq!(error.code, "managed_instructions_unavailable");
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        assert_eq!(
            std::fs::read_to_string(root.path().join("old-instructions/AGENTS.md")).unwrap(),
            "original"
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("old-instructions"))
                .unwrap()
                .count(),
            1
        );
    }
    #[test]
    fn atomic_save_rechecks_content_after_staging() {
        let tmp = tempfile::tempdir().unwrap();
        let directory = Directory::open(tmp.path(), true).unwrap().unwrap();
        directory.replace("original", "missing").unwrap();
        let error = directory
            .replace_with_hook("stale", &revision(Some("original")), || {
                std::fs::write(tmp.path().join("instructions/AGENTS.md"), "external edit").unwrap();
            })
            .unwrap_err();
        assert_eq!(error.code, "managed_instructions_conflict");
        assert_eq!(directory.read().unwrap().as_deref(), Some("external edit"));
        assert_eq!(
            std::fs::read_dir(tmp.path().join("instructions"))
                .unwrap()
                .count(),
            1
        );
    }
}
