//! Small, private lifecycle journal for persistent services without a console.
//! Never feed stdout, HTTP bodies, tracing fields, credentials, or arbitrary
//! error strings to this writer; callers can only select fixed event variants.

use super::Component;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SERVICE_LOG_NAME: &str = "service-events.log";
const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceLogEvent {
    Starting,
    Ready,
    Stopped,
    Failed,
    RestartRequested,
}

impl ServiceLogEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::RestartRequested => "restart_requested",
        }
    }
}

/// One process keeps one handle open. The fixed JSON schema carries only
/// numeric time/PID and compiled-in component/event names.
pub struct ServiceLog {
    file: Mutex<File>,
    path: PathBuf,
}

pub struct ServiceLogGuard {
    log: ServiceLog,
    component: Component,
    completed: bool,
}

impl ServiceLog {
    pub fn from_managed_env() -> Result<Option<Self>, String> {
        match std::env::var_os("WEBCODEX_SERVICE_LOG_DIR") {
            Some(directory) => Self::open(Path::new(&directory)).map(Some),
            None => Ok(None),
        }
    }
    pub fn open(directory: &Path) -> Result<Self, String> {
        validate_directory(directory)?;
        let path = directory.join(SERVICE_LOG_NAME);
        let existed = path.exists();
        if existed {
            validate_file_path(&path)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let file = options
            .open(&path)
            .map_err(|_| "service lifecycle log is unavailable")?;
        if let Err(error) = validate_file_path(&path) {
            return Err(error);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = file
                .metadata()
                .map_err(|_| "service lifecycle log cannot be inspected")?;
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1
            {
                return Err("service lifecycle log is not private".into());
            }
        }
        #[cfg(windows)]
        crate::runtime_entry::validate_windows_env_acl(&path)
            .map_err(|_| "service lifecycle log ACL is not private")?;
        Ok(Self {
            file: Mutex::new(file),
            path,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(&self, component: Component, event: ServiceLogEvent) -> Result<(), String> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "service lifecycle clock is unavailable")?
            .as_secs();
        let line = format!("{{\"schema_version\":1,\"time_unix\":{timestamp},\"pid\":{},\"component\":\"{}\",\"event\":\"{}\"}}\n",
            std::process::id(), component.as_str(), event.as_str());
        let mut file = self
            .file
            .lock()
            .map_err(|_| "service lifecycle log lock failed")?;
        let size = file
            .metadata()
            .map_err(|_| "service lifecycle log cannot be inspected")?
            .len();
        if size > MAX_BYTES || size.saturating_add(line.len() as u64) > MAX_BYTES {
            file.set_len(0)
                .map_err(|_| "service lifecycle log cannot be bounded")?;
            file.seek(SeekFrom::Start(0))
                .map_err(|_| "service lifecycle log seek failed")?;
        } else {
            file.seek(SeekFrom::End(0))
                .map_err(|_| "service lifecycle log seek failed")?;
        }
        file.write_all(line.as_bytes())
            .and_then(|_| file.sync_data())
            .map_err(|_| "service lifecycle log write failed".into())
    }
}

impl ServiceLogGuard {
    pub fn open(directory: &Path, component: Component) -> Result<Self, String> {
        let log = ServiceLog::open(directory)?;
        log.record(component, ServiceLogEvent::Starting)?;
        Ok(Self {
            log,
            component,
            completed: false,
        })
    }

    pub fn from_managed_env(component: Component) -> Result<Option<Self>, String> {
        let Some(log) = ServiceLog::from_managed_env()? else {
            return Ok(None);
        };
        log.record(component, ServiceLogEvent::Starting)?;
        Ok(Some(Self {
            log,
            component,
            completed: false,
        }))
    }

    pub fn ready(&self) -> Result<(), String> {
        self.log.record(self.component, ServiceLogEvent::Ready)
    }

    pub fn stopped(&mut self) -> Result<(), String> {
        self.log.record(self.component, ServiceLogEvent::Stopped)?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for ServiceLogGuard {
    fn drop(&mut self) {
        if !self.completed {
            let _ = self.log.record(self.component, ServiceLogEvent::Failed);
        }
    }
}

fn validate_directory(directory: &Path) -> Result<(), String> {
    if !directory.is_absolute() {
        return Err("service lifecycle log directory must be absolute".into());
    }
    for ancestor in directory.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|_| "service lifecycle log directory is unavailable")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("service lifecycle log directory contains a link".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("service lifecycle log directory contains a reparse point".into());
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(directory)
            .map_err(|_| "service lifecycle log directory is unavailable")?;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err("service lifecycle log directory is not private".into());
        }
    }
    Ok(())
}

fn validate_file_path(path: &Path) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "service lifecycle log cannot be inspected")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("service lifecycle log is not a regular file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("service lifecycle log is a reparse point".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_log_is_private_bounded_and_contains_no_caller_text() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("private");
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let log = ServiceLog::open(&directory).unwrap();
        for _ in 0..5000 {
            log.record(Component::Tunnel, ServiceLogEvent::Ready)
                .unwrap();
        }
        let content = std::fs::read_to_string(log.path()).unwrap();
        assert!(content.len() <= MAX_BYTES as usize);
        assert!(content.lines().all(|line| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            value["component"] == "tunnel"
                && value["event"] == "ready"
                && value.as_object().unwrap().len() == 5
        }));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(log.path()).unwrap().permissions().mode() & 0o077,
                0
            );
        }
    }

    #[test]
    fn guard_records_terminal_failure_without_error_text() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("private");
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        {
            let guard = ServiceLogGuard::open(&directory, Component::Server).unwrap();
            guard.ready().unwrap();
        }
        let content = std::fs::read_to_string(directory.join(SERVICE_LOG_NAME)).unwrap();
        let events: Vec<_> = content
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["event"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(events, vec!["starting", "ready", "failed"]);
    }

    #[cfg(unix)]
    #[test]
    fn lifecycle_log_rejects_symlink_and_public_directory() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("private");
        std::fs::create_dir(&directory).unwrap();
        let directory = directory.canonicalize().unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let target = temp.path().join("foreign");
        std::fs::write(&target, b"do not overwrite").unwrap();
        symlink(&target, directory.join(SERVICE_LOG_NAME)).unwrap();
        assert!(ServiceLog::open(&directory).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"do not overwrite");
        std::fs::remove_file(directory.join(SERVICE_LOG_NAME)).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(ServiceLog::open(&directory).is_err());
    }
}
