//! Best-effort, bounded metadata for Desktop exits that erase in-memory output.
//! No activity messages, command lines, child output, or credentials enter here.
use crate::process::ProcessSnapshot;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const DIRECTORY: &str = "desktop-lifecycle-v1";
pub(crate) const CURRENT: &str = "current.jsonl";
pub(crate) const PREVIOUS: &str = "previous.jsonl";
const FILE_BYTES: u64 = 256 * 1024;
const QUEUE_ENTRIES: usize = 64;

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub(crate) enum LifecycleEvent {
    DesktopStarted,
    ExitRequested { code: Option<i32>, prevented: bool },
    DesktopExiting,
    ShutdownStarted,
    ShutdownWaitFinished,
    ProcessObserved { snapshot: ProcessSnapshot },
    ProcessStopped { snapshot: ProcessSnapshot },
}

#[derive(Serialize)]
struct Record {
    version: u32,
    session_id: String,
    desktop_pid: u32,
    timestamp_ms: u64,
    dropped_before: u64,
    #[serde(flatten)]
    event: LifecycleEvent,
}

enum Message {
    Record(Record),
    #[cfg(test)]
    Barrier(mpsc::SyncSender<()>),
}

#[derive(Clone)]
pub(crate) struct LifecycleLog {
    sender: mpsc::SyncSender<Message>,
    session_id: uuid::Uuid,
    dropped: Arc<AtomicU64>,
}

impl LifecycleLog {
    #[cfg(test)]
    pub(crate) fn test_barrier(&self) {
        let (tx, rx) = mpsc::sync_channel(1);
        self.sender.send(Message::Barrier(tx)).unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
    }

    pub(crate) fn start(data_dir: &Path) -> io::Result<Self> {
        let writer = Writer::open(data_dir)?;
        let (sender, receiver) = mpsc::sync_channel(QUEUE_ENTRIES);
        std::thread::Builder::new()
            .name("webcodex-lifecycle-log".into())
            .spawn(move || {
                let mut writer = writer;
                for message in receiver {
                    match message {
                        Message::Record(record) => {
                            if writer.append(&record).is_err() {
                                // Static text only. Journaling failure never changes
                                // process ownership, cleanup, or restart admission.
                                eprintln!("WebCodex Desktop lifecycle metadata logging stopped");
                                break;
                            }
                        }
                        #[cfg(test)]
                        Message::Barrier(ack) => {
                            let _ = ack.send(());
                        }
                    }
                }
            })?;
        let log = Self {
            sender,
            session_id: uuid::Uuid::new_v4(),
            dropped: Arc::new(AtomicU64::new(0)),
        };
        log.record(LifecycleEvent::DesktopStarted);
        Ok(log)
    }

    pub(crate) fn record(&self, event: LifecycleEvent) {
        let dropped_before = self.dropped.swap(0, Ordering::Relaxed);
        let record = Record {
            version: 1,
            session_id: self.session_id.to_string(),
            desktop_pid: std::process::id(),
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            dropped_before,
            event,
        };
        if self.sender.try_send(Message::Record(record)).is_err() {
            self.dropped
                .fetch_add(dropped_before.saturating_add(1), Ordering::Relaxed);
        }
        // No filesystem I/O or queue wait on the native session-shutdown path.
    }
}

struct Writer {
    directory: PathBuf,
    file: Option<File>,
    length: u64,
}

impl Writer {
    fn open(data_dir: &Path) -> io::Result<Self> {
        let directory = data_dir.join(DIRECTORY);
        check_directory(data_dir)?;
        match fs::create_dir(&directory) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        check_directory(&directory)?;
        let current = directory.join(CURRENT);
        let file = open_file(&current)?;
        let length = file.metadata()?.len();
        if length > FILE_BYTES {
            return Err(unsafe_path());
        }
        check_existing_file(&directory.join(PREVIOUS))?;
        let mut writer = Self {
            directory,
            file: Some(file),
            length,
        };
        // Preserve an interrupted partial write; keep the next record separate.
        if length != 0 {
            let file = writer.file.as_mut().unwrap();
            file.seek(SeekFrom::End(-1))?;
            let mut tail = [0];
            file.read_exact(&mut tail)?;
            if tail[0] != b'\n' {
                if length == FILE_BYTES {
                    writer.rotate()?;
                } else {
                    writer.file.as_mut().unwrap().write_all(b"\n")?;
                    writer.length += 1;
                }
            }
        }
        Ok(writer)
    }

    fn append(&mut self, record: &Record) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > FILE_BYTES {
            return Err(unsafe_path());
        }
        if self.length + bytes.len() as u64 > FILE_BYTES {
            self.rotate()?;
        }
        let file = self.file.as_mut().unwrap();
        file.write_all(&bytes)?;
        file.sync_data()?;
        self.length += bytes.len() as u64;
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        check_directory(&self.directory)?;
        let current = self.directory.join(CURRENT);
        let previous = self.directory.join(PREVIOUS);
        check_existing_file(&current)?;
        check_existing_file(&previous)?;
        drop(self.file.take());
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&current, &previous)?;
        self.file = Some(open_file(&current)?);
        self.length = 0;
        Ok(())
    }
}

fn unsafe_path() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidInput)
}

fn redirected(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn check_directory(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(unsafe_path());
    }
    for parent in path.ancestors() {
        let metadata = fs::symlink_metadata(parent)?;
        if !metadata.is_dir() || redirected(&metadata) {
            return Err(unsafe_path());
        }
    }
    Ok(())
}

fn check_existing_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.is_file() && !redirected(&metadata) && metadata.len() <= FILE_BYTES =>
        {
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        _ => Err(unsafe_path()),
    }
}

fn open_file(path: &Path) -> io::Result<File> {
    check_existing_file(path)?;
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
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
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || redirected(&metadata) {
        return Err(unsafe_path());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
            return Err(unsafe_path());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: file owns this live handle; info is valid output storage.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.nNumberOfLinks != 1 {
            return Err(unsafe_path());
        }
    }
    Ok(file)
}

#[cfg(test)]
#[path = "lifecycle_log/tests.rs"]
mod tests;
