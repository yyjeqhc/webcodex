use super::config::InstructionsConfig;
use super::configured_skills::metadata_is_link_like;
use super::CommandResult;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::time::Instant;
use webcodex_core::project_instructions::{
    InstructionSourceScope, LoadedInstructionCandidate, ProjectInstructionsSnapshot,
};
use webcodex_core::runner_instruction::{
    RunnerInstructionAction, RunnerInstructionRequest, RunnerInstructionSnapshotResponse,
    RUNNER_INSTRUCTION_RESPONSE_FORMAT, RUNNER_INSTRUCTION_RESPONSE_MAX_BYTES,
};

const MAX_CONFIGURED_INSTRUCTION_FILE_BYTES: u64 = 1024 * 1024;
#[cfg(windows)]
const WINDOWS_FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
#[cfg(windows)]
const WINDOWS_FILE_NON_DIRECTORY_FILE: u32 = 0x0000_0040;
#[cfg(windows)]
const WINDOWS_OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
#[cfg(windows)]
const WINDOWS_OBJ_DONT_REPARSE: u32 = 0x0000_1000;

pub(crate) fn handle_runner_instruction_request(
    generation: u64,
    config: &InstructionsConfig,
    request: RunnerInstructionRequest,
) -> CommandResult {
    let started = Instant::now();
    if request.validate().is_err() {
        return error_result(started, "instruction_invalid_request");
    }
    match request.action {
        RunnerInstructionAction::Snapshot => snapshot(generation, config, started),
    }
}

fn snapshot(generation: u64, config: &InstructionsConfig, started: Instant) -> CommandResult {
    let mut candidates = Vec::with_capacity(config.files.len());
    let mut identities = HashSet::with_capacity(config.files.len());
    let mut scan_complete = true;

    for (index, configured) in config.files.iter().enumerate() {
        let file = match open_instruction_file(configured) {
            Ok(file) => file,
            // A confirmed missing file withdraws its guidance. Permission and
            // other read failures remain unavailable, not deletion evidence.
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                scan_complete = false;
                continue;
            }
        };
        let identity = super::config::configured_skill_root_identity(configured);
        if !identities.insert(identity) {
            scan_complete = false;
            continue;
        }
        let metadata = match file.metadata() {
            Ok(metadata) if metadata.len() <= MAX_CONFIGURED_INSTRUCTION_FILE_BYTES => metadata,
            _ => {
                scan_complete = false;
                continue;
            }
        };
        let bytes = match read_instruction_bytes(file) {
            Ok(bytes) if bytes.len() as u64 == metadata.len() => bytes,
            _ => {
                scan_complete = false;
                continue;
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(content) => content,
            Err(_) => {
                scan_complete = false;
                continue;
            }
        };
        if content.is_empty() {
            continue;
        }
        let basename = configured
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("instructions");
        let logical_source = format!("runner/{index}/{basename}");
        let total_lines = line_count(&content);
        let full_sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
        candidates.push(LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Runner,
            path: logical_source,
            content,
            total_lines,
            full_sha256: Some(full_sha256),
        });
    }

    let snapshot = ProjectInstructionsSnapshot::from_candidates(candidates, scan_complete);
    let response = RunnerInstructionSnapshotResponse {
        format: RUNNER_INSTRUCTION_RESPONSE_FORMAT.to_string(),
        generation,
        scan_complete: snapshot.scan_complete,
        files: snapshot.files,
    };
    if response.validate().is_err() {
        return error_result(started, "instruction_response_invalid");
    }
    let stdout = match serde_json::to_string(&response) {
        Ok(stdout) if stdout.len() <= RUNNER_INSTRUCTION_RESPONSE_MAX_BYTES => stdout,
        _ => return error_result(started, "instruction_response_too_large"),
    };
    CommandResult {
        exit_code: Some(0),
        stdout: Some(stdout),
        stderr: None,
        duration_ms: Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
        error: None,
    }
}

// Instruction authority names ordinary files, not redirectable filesystem
// trees. Resolve the path through pinned directory handles so an ancestor
// cannot be swapped to a symlink/reparse point between validation and open.
fn open_instruction_file(path: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        open_instruction_file_unix(path, || {})
    }
    #[cfg(windows)]
    {
        open_instruction_file_windows(path, || {})
    }
    #[cfg(not(any(unix, windows)))]
    {
        unsupported_instruction_file_open(path)
    }
}

#[cfg(any(test, not(any(unix, windows))))]
fn unsupported_instruction_file_open(_path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure configured instruction reads are unsupported on this platform",
    ))
}

#[cfg(unix)]
fn open_instruction_file_unix(path: &Path, before_leaf: impl FnOnce()) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};

    let parent_path = path
        .parent()
        .ok_or_else(|| io::Error::other("instruction path must name an absolute file"))?;
    let parent = open_unix_directory(parent_path).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::other("instruction parent is unavailable")
        } else {
            error
        }
    })?;
    let leaf = path
        .file_name()
        .ok_or_else(|| io::Error::other("instruction path must name an absolute file"))?;
    let leaf = CString::new(std::os::unix::ffi::OsStrExt::as_bytes(leaf))
        .map_err(|_| io::Error::other("instruction path contains NUL"))?;

    before_leaf();

    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::NotFound
            && !unix_parent_still_current(parent_path, parent.as_raw_fd())
        {
            return Err(io::Error::other(
                "instruction parent changed during observation",
            ));
        }
        return Err(error);
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata_is_link_like(&metadata) {
        return Err(io::Error::other(
            "instruction handle is not an ordinary file",
        ));
    }
    if !unix_parent_still_current(parent_path, parent.as_raw_fd()) {
        return Err(io::Error::other(
            "instruction parent changed during observation",
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn open_unix_directory(path: &Path) -> io::Result<std::os::fd::OwnedFd> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;

    if !path.is_absolute() {
        return Err(io::Error::other("instruction parent must be absolute"));
    }
    let root_fd = unsafe { libc::open(b"/\0".as_ptr().cast(), unix_directory_open_flags()) };
    if root_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut directory = unsafe { OwnedFd::from_raw_fd(root_fd) };
    for component in path.components() {
        let name = match component {
            Component::RootDir | Component::CurDir => continue,
            Component::Normal(name) => name,
            Component::ParentDir | Component::Prefix(_) => {
                return Err(io::Error::other(
                    "instruction path must be absolute without parent traversal",
                ));
            }
        };
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::other("instruction path contains NUL"))?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                unix_directory_open_flags(),
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        directory = unsafe { OwnedFd::from_raw_fd(fd) };
    }
    Ok(directory)
}

#[cfg(unix)]
fn unix_parent_still_current(path: &Path, expected_fd: std::os::fd::RawFd) -> bool {
    use std::os::fd::AsRawFd;

    let Ok(current) = open_unix_directory(path) else {
        return false;
    };
    unix_fd_identity(current.as_raw_fd())
        .zip(unix_fd_identity(expected_fd))
        .is_some_and(|(current, expected)| current == expected)
}

#[cfg(unix)]
fn unix_fd_identity(fd: std::os::fd::RawFd) -> Option<(u64, u64)> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return None;
    }
    let stat = unsafe { stat.assume_init() };
    Some((stat.st_dev as u64, stat.st_ino as u64))
}

#[cfg(unix)]
fn unix_directory_open_flags() -> libc::c_int {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let access = libc::O_PATH;
    #[cfg(any(
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "aix",
        target_os = "fuchsia"
    ))]
    let access = libc::O_SEARCH;
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "aix",
        target_os = "fuchsia"
    )))]
    let access = libc::O_RDONLY;

    access | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC
}

#[cfg(windows)]
fn open_instruction_file_windows(path: &Path, before_leaf: impl FnOnce()) -> io::Result<File> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_SHARE_READ};

    let parent_path = path
        .parent()
        .ok_or_else(|| io::Error::other("instruction path must name an absolute file"))?;
    let parent = match open_windows_directory(parent_path) {
        Ok(handle) => File::from(handle),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(io::Error::other("instruction parent is unavailable"));
        }
        Err(error) => return Err(error),
    };
    let parent_identity = windows_file_identity(&parent)?;
    let leaf = path
        .file_name()
        .ok_or_else(|| io::Error::other("instruction path must name an absolute file"))?;

    before_leaf();

    let file = match windows_nt_open_relative(
        parent.as_raw_handle() as HANDLE,
        leaf,
        FILE_GENERIC_READ,
        FILE_SHARE_READ,
        WINDOWS_FILE_NON_DIRECTORY_FILE,
    ) {
        Ok(handle) => File::from(handle),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if !windows_parent_still_current(parent_path, parent_identity) {
                return Err(io::Error::other(
                    "instruction parent changed during observation",
                ));
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata_is_link_like(&metadata) {
        return Err(io::Error::other(
            "instruction handle is not an ordinary file",
        ));
    }
    if !windows_parent_still_current(parent_path, parent_identity) {
        return Err(io::Error::other(
            "instruction parent changed during observation",
        ));
    }
    Ok(file)
}

#[cfg(windows)]
fn windows_parent_still_current(path: &Path, expected: (u64, [u8; 16])) -> bool {
    open_windows_directory(path)
        .map(File::from)
        .and_then(|file| windows_file_identity(&file))
        .is_ok_and(|current| current == expected)
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> io::Result<(u64, [u8; 16])> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
    };

    let mut info = std::mem::MaybeUninit::<FILE_ID_INFO>::zeroed();
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FileIdInfo,
            info.as_mut_ptr().cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    Ok((info.VolumeSerialNumber, info.FileId.Identifier))
}

#[cfg(windows)]
fn open_windows_directory(path: &Path) -> io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::path::Component;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let mut components = path.components();
    let prefix = match components.next() {
        Some(Component::Prefix(prefix)) => prefix,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "instruction path must be an absolute Windows path",
            ))
        }
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "instruction path must include a Windows root",
        ));
    }

    let mut root = std::path::PathBuf::from(prefix.as_os_str());
    root.push(r"\");
    let wide = std::os::windows::ffi::OsStrExt::encode_wide(root.as_os_str())
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // Resolve only the Windows drive/share root through Win32 namespace rules.
    // Every real filesystem descendant is then opened relative to the pinned
    // parent handle with OBJ_DONT_REPARSE, so junction/reparse ancestors remain
    // fail-closed without rejecting the ordinary DOS drive mapping itself.
    let root_handle: HANDLE = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if root_handle.is_null() || root_handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let mut directory = unsafe { OwnedHandle::from_raw_handle(root_handle as RawHandle) };

    for component in components {
        let name = match component {
            Component::Normal(name) => name,
            Component::CurDir => continue,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "instruction path must not contain parent traversal",
                ))
            }
        };
        directory = windows_nt_open_relative(
            directory.as_raw_handle() as HANDLE,
            name,
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            WINDOWS_FILE_DIRECTORY_FILE,
        )?;
    }

    Ok(directory)
}

#[cfg(windows)]
fn windows_nt_open_relative(
    root: windows_sys::Win32::Foundation::HANDLE,
    name: &std::ffi::OsStr,
    desired_access: u32,
    share_access: u32,
    create_options: u32,
) -> io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::ffi::OsStrExt;

    let mut name = name.encode_wide().collect::<Vec<_>>();
    windows_nt_open(
        root,
        &mut name,
        desired_access,
        share_access,
        create_options,
    )
}

#[cfg(windows)]
fn windows_nt_open(
    root: windows_sys::Win32::Foundation::HANDLE,
    name: &mut [u16],
    desired_access: u32,
    share_access: u32,
    create_options: u32,
) -> io::Result<std::os::windows::io::OwnedHandle> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::io::{FromRawHandle, OwnedHandle, RawHandle};
    use std::ptr;
    use windows_sys::Win32::Foundation::{
        RtlNtStatusToDosError, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, UNICODE_STRING,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    const FILE_OPEN: u32 = 1;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;

    #[repr(C)]
    struct ObjectAttributes {
        length: u32,
        root_directory: HANDLE,
        object_name: *const UNICODE_STRING,
        attributes: u32,
        security_descriptor: *const c_void,
        security_quality_of_service: *const c_void,
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtCreateFile(
            file_handle: *mut HANDLE,
            desired_access: u32,
            object_attributes: *const ObjectAttributes,
            io_status_block: *mut IO_STATUS_BLOCK,
            allocation_size: *const i64,
            file_attributes: u32,
            share_access: u32,
            create_disposition: u32,
            create_options: u32,
            ea_buffer: *const c_void,
            ea_length: u32,
        ) -> NTSTATUS;
    }

    let name_length = u16::try_from(name.len().saturating_mul(size_of::<u16>()))
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "instruction path is too long"))?;
    let object_name = UNICODE_STRING {
        Length: name_length,
        MaximumLength: name_length,
        Buffer: name.as_mut_ptr(),
    };
    let object_attributes = ObjectAttributes {
        length: size_of::<ObjectAttributes>() as u32,
        root_directory: root,
        object_name: &object_name,
        attributes: WINDOWS_OBJ_CASE_INSENSITIVE | WINDOWS_OBJ_DONT_REPARSE,
        security_descriptor: ptr::null(),
        security_quality_of_service: ptr::null(),
    };
    let mut io_status = std::mem::MaybeUninit::<IO_STATUS_BLOCK>::zeroed();
    let mut handle: HANDLE = std::ptr::null_mut();
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access | SYNCHRONIZE_ACCESS,
            &object_attributes,
            io_status.as_mut_ptr(),
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            share_access,
            FILE_OPEN,
            create_options | FILE_SYNCHRONOUS_IO_NONALERT,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        let code = unsafe { RtlNtStatusToDosError(status) };
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::other(
            "NtCreateFile returned an invalid instruction handle",
        ));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

fn read_instruction_bytes(reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    // A pre-read metadata length is not a resource bound: the file can grow.
    reader
        .take(MAX_CONFIGURED_INSTRUCTION_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_CONFIGURED_INSTRUCTION_FILE_BYTES {
        return Err(io::Error::other("instruction file exceeds the byte limit"));
    }
    Ok(bytes)
}

fn line_count(content: &str) -> usize {
    if content.is_empty() {
        0
    } else {
        content.bytes().filter(|byte| *byte == b'\n').count()
            + usize::from(!content.ends_with('\n'))
    }
}

fn error_result(started: Instant, code: &str) -> CommandResult {
    CommandResult {
        exit_code: Some(1),
        stdout: None,
        stderr: None,
        duration_ms: Some(started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)),
        error: Some(code.to_string()),
    }
}

#[cfg(test)]
#[path = "runner_instruction_tests.rs"]
mod safety_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn omitted_config_produces_complete_empty_snapshot() {
        let response = handle_runner_instruction_request(
            1,
            &InstructionsConfig::default(),
            RunnerInstructionRequest::snapshot(),
        );
        let parsed: RunnerInstructionSnapshotResponse =
            serde_json::from_str(response.stdout.as_deref().unwrap()).unwrap();
        assert!(parsed.scan_complete);
        assert!(parsed.files.is_empty());
    }

    #[test]
    fn snapshot_uses_logical_sources_and_live_content() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap().join("AGENTS.md");
        std::fs::write(&path, "first\n").unwrap();
        let config = InstructionsConfig {
            files: vec![path.clone()],
        };

        let first =
            handle_runner_instruction_request(7, &config, RunnerInstructionRequest::snapshot());
        let first_stdout = first.stdout.as_deref().unwrap();
        assert!(!first_stdout.contains(path.to_string_lossy().as_ref()));
        let first: RunnerInstructionSnapshotResponse = serde_json::from_str(first_stdout).unwrap();
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.files[0].path, "runner/0/AGENTS.md");
        assert_eq!(first.files[0].content, "first");
        assert!(first.files[0].read_more.is_none());
        let first_fingerprint = first.files[0].fingerprint.clone();

        std::fs::write(&path, "second\n").unwrap();
        let second =
            handle_runner_instruction_request(7, &config, RunnerInstructionRequest::snapshot());
        let second: RunnerInstructionSnapshotResponse =
            serde_json::from_str(second.stdout.as_deref().unwrap()).unwrap();
        assert_eq!(second.generation, 7);
        assert_eq!(second.files[0].content, "second");
        assert_ne!(second.files[0].fingerprint, first_fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_rejects_configured_symlink_instead_of_following_it() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("secret.txt");
        let link = tmp.path().join("AGENTS.md");
        std::fs::write(&target, "must not be projected\n").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let config = InstructionsConfig { files: vec![link] };

        let response =
            handle_runner_instruction_request(1, &config, RunnerInstructionRequest::snapshot());
        assert_eq!(response.exit_code, Some(0));
        let stdout = response.stdout.as_deref().unwrap();
        assert!(!stdout.contains("must not be projected"));
        let parsed: RunnerInstructionSnapshotResponse = serde_json::from_str(stdout).unwrap();
        assert!(!parsed.scan_complete);
        assert!(parsed.files.is_empty());
    }

    #[test]
    fn snapshot_does_not_require_project_allowed_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().canonicalize().unwrap().join("global.md");
        std::fs::write(&path, "runner only").unwrap();
        let config = InstructionsConfig {
            files: vec![PathBuf::from(&path)],
        };
        let response =
            handle_runner_instruction_request(1, &config, RunnerInstructionRequest::snapshot());
        assert_eq!(response.exit_code, Some(0));
    }
}
