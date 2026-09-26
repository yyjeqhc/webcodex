use super::*;

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    fn socket_path(dir: &Path) -> PathBuf {
        dir.join("computer-session.sock")
    }

    fn peer_uid(stream: &UnixStream) -> Result<u32, String> {
        let mut uid: libc::uid_t = 0;
        let mut gid: libc::gid_t = 0;
        let result = unsafe {
            libc::getpeereid(std::os::fd::AsRawFd::as_raw_fd(stream), &mut uid, &mut gid)
        };
        if result == 0 {
            Ok(uid)
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }

    fn peer_pid(stream: &UnixStream) -> Result<i32, String> {
        let mut pid = 0_i32;
        let mut size = std::mem::size_of::<i32>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                std::os::fd::AsRawFd::as_raw_fd(stream),
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&mut pid as *mut i32).cast(),
                &mut size,
            )
        };
        if result == 0 && size as usize == std::mem::size_of::<i32>() && pid > 0 {
            Ok(pid)
        } else {
            Err("computer session peer PID unavailable".to_string())
        }
    }

    #[link(name = "proc")]
    unsafe extern "C" {
        fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, size: u32) -> i32;
    }

    fn peer_is_same_binary(stream: &UnixStream) -> bool {
        let Ok(pid) = peer_pid(stream) else {
            return false;
        };
        let mut bytes = [0_u8; 4096];
        let used = unsafe { proc_pidpath(pid, bytes.as_mut_ptr().cast(), bytes.len() as u32) };
        let Ok(expected) = std::env::current_exe() else {
            return false;
        };
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(used.max(0) as usize);
        used > 0
            && std::str::from_utf8(&bytes[..end])
                .ok()
                .is_some_and(|actual| {
                    std::fs::canonicalize(actual).ok() == std::fs::canonicalize(expected).ok()
                })
    }

    pub(super) fn exchange(
        config: &ClientConfig,
        request: WireRequest,
        timeout: Duration,
    ) -> Result<WireResponse, String> {
        validate_dir(&config.dir, false)?;
        let owner = std::fs::metadata(&config.dir)
            .map_err(|e| e.to_string())?
            .uid();
        let mut stream =
            UnixStream::connect(socket_path(&config.dir)).map_err(|e| e.to_string())?;
        if peer_uid(&stream)? != owner || !peer_is_same_binary(&stream) {
            return Err("computer session peer identity changed".to_string());
        }
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        write_frame(&mut stream, &bytes, REQUEST_MAX_BYTES)?;
        serde_json::from_slice(&read_frame(&mut stream, RESPONSE_MAX_BYTES)?)
            .map_err(|e| e.to_string())
    }

    pub(super) fn run_helper(dir: &Path) -> Result<(), String> {
        use fs2::FileExt;
        use std::os::unix::fs::PermissionsExt;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.join("computer-session.lock"))
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(
            dir.join("computer-session.lock"),
            std::fs::Permissions::from_mode(0o600),
        )
        .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|_| "computer session helper already running".to_string())?;
        let socket = socket_path(dir);
        if let Ok(meta) = std::fs::symlink_metadata(&socket) {
            if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
                return Err("computer session socket is not owned by helper".to_string());
            }
            // A second helper must fail at bind; only remove a stale socket after
            // a connection probe proves no listener still owns it.
            if UnixStream::connect(&socket).is_ok() {
                return Err("computer session helper already running".to_string());
            }
            std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
        }
        let listener = UnixListener::bind(&socket).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        let mut state = HelperState::new();
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else { continue };
            let uid = match peer_uid(&stream) {
                Ok(uid) => uid,
                Err(_) => continue,
            };
            let pid = match peer_pid(&stream) {
                Ok(pid) => pid,
                Err(_) => continue,
            };
            if uid != 0 || !peer_is_same_binary(&stream) {
                continue;
            }
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            let response = match read_frame(&mut stream, REQUEST_MAX_BYTES).and_then(|bytes| {
                serde_json::from_slice::<WireRequest>(&bytes).map_err(|e| e.to_string())
            }) {
                Ok(request) => state.handle(request, pid as u32, native_session_available()),
                Err(_) => WireResponse::Rejected {
                    error: "invalid_request: malformed computer session frame".to_string(),
                },
            };
            if let Ok(bytes) = serde_json::to_vec(&response) {
                let _ = write_frame(&mut stream, &bytes, RESPONSE_MAX_BYTES);
            }
        }
        Ok(())
    }

    // CGSession reports the actual console/login state, rather than the
    // process's environment. This is checked before *every* operation.
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const std::ffi::c_void;
    }
    #[link(name = "SystemConfiguration", kind = "framework")]
    unsafe extern "C" {
        fn SCDynamicStoreCopyConsoleUser(
            store: *const std::ffi::c_void,
            uid: *mut u32,
            gid: *mut u32,
        ) -> *const std::ffi::c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const std::ffi::c_void,
            text: *const i8,
            encoding: u32,
        ) -> *const std::ffi::c_void;
        fn CFDictionaryGetValue(
            dict: *const std::ffi::c_void,
            key: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;
        fn CFBooleanGetValue(value: *const std::ffi::c_void) -> u8;
        fn CFRelease(value: *const std::ffi::c_void);
    }

    fn session_flag(dict: *const std::ffi::c_void, key: &[u8]) -> bool {
        let name = unsafe {
            CFStringCreateWithCString(std::ptr::null(), key.as_ptr().cast(), 0x0800_0100)
        };
        if name.is_null() {
            return false;
        }
        let value = unsafe { CFDictionaryGetValue(dict, name) };
        let enabled = !value.is_null() && unsafe { CFBooleanGetValue(value) } != 0;
        unsafe { CFRelease(name) };
        enabled
    }

    pub(super) fn native_session_available() -> bool {
        let mut console_uid = u32::MAX;
        let mut console_gid = u32::MAX;
        let user = unsafe {
            SCDynamicStoreCopyConsoleUser(std::ptr::null(), &mut console_uid, &mut console_gid)
        };
        if user.is_null() {
            return false;
        }
        unsafe { CFRelease(user) };
        if console_uid != unsafe { libc::geteuid() } {
            return false;
        }
        let dict = unsafe { CGSessionCopyCurrentDictionary() };
        if dict.is_null() {
            return false;
        }
        let eligible = session_flag(dict, b"kCGSessionOnConsoleKey\0")
            && session_flag(dict, b"kCGSessionLoginDoneKey\0");
        unsafe { CFRelease(dict) };
        eligible
    }
}

#[cfg(target_os = "macos")]
pub(super) use macos::{exchange, native_session_available, run_helper};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::{exchange, native_session_available, run_helper};
