use super::*;
use sha2::{Digest, Sha256};
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};

type Handle = *mut c_void;
const INVALID_HANDLE: Handle = -1_isize as Handle;
const PIPE_ACCESS_DUPLEX: u32 = 0x3;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
const OPEN_EXISTING: u32 = 3;
const GENERIC_READ_WRITE: u32 = 0xC000_0000;
const ERROR_PIPE_CONNECTED: u32 = 535;

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    descriptor: *mut c_void,
    inherit: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open_mode: u32,
        pipe_mode: u32,
        max_instances: u32,
        out_size: u32,
        in_size: u32,
        default_timeout: u32,
        security: *const SecurityAttributes,
    ) -> Handle;
    fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: Handle) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn WaitNamedPipeW(name: *const u16, timeout: u32) -> i32;
    fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn WTSGetActiveConsoleSessionId() -> u32;
    fn GetCurrentProcessId() -> u32;
    fn GetLastError() -> u32;
    fn LocalFree(value: Handle) -> Handle;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        name: *mut u16,
        length: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> Handle;
    fn GetUserObjectInformationW(
        object: Handle,
        index: u32,
        info: *mut c_void,
        length: u32,
        needed: *mut u32,
    ) -> i32;
    fn CloseDesktop(desktop: Handle) -> i32;
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

fn pipe_name(dir: &Path) -> Vec<u16> {
    let digest = Sha256::digest(dir.as_os_str().to_string_lossy().as_bytes());
    let name = format!(r"\\.\pipe\webcodex-computer-{:x}", digest);
    wide(OsStr::new(&name))
}

fn session_id(pid: u32) -> Option<u32> {
    let mut id = u32::MAX;
    (unsafe { ProcessIdToSessionId(pid, &mut id) } != 0).then_some(id)
}

fn image_path(pid: u32) -> Option<String> {
    let handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut name = [0_u16; 32768];
    let mut length = name.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, name.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(handle) };
    ok.then(|| String::from_utf16_lossy(&name[..length as usize]))
}

fn same_image(actual: &str, expected: &Path) -> bool {
    match (
        std::fs::canonicalize(actual),
        std::fs::canonicalize(expected),
    ) {
        (Ok(actual), Ok(expected)) => actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy()),
        _ => false,
    }
}

fn check_server(pipe: Handle) -> Result<(), String> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 || pid == 0 {
        return Err("computer session server identity unavailable".to_string());
    }
    let active = unsafe { WTSGetActiveConsoleSessionId() };
    if active == u32::MAX || active == 0 || session_id(pid) != Some(active) {
        return Err("computer session server is outside active console".to_string());
    }
    let expected = std::env::current_exe().map_err(|e| e.to_string())?;
    let actual = image_path(pid).ok_or("computer session server image unavailable")?;
    if !same_image(&actual, &expected) {
        return Err("computer session server image does not match Runner".to_string());
    }
    Ok(())
}

fn exchange_inner(config: &ClientConfig, request: WireRequest) -> Result<WireResponse, String> {
    validate_dir(&config.dir, false)?;
    let name = pipe_name(&config.dir);
    if unsafe { WaitNamedPipeW(name.as_ptr(), 500) } == 0 {
        return Err("computer session pipe unavailable".to_string());
    }
    let pipe = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ_WRITE,
            0,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if pipe == INVALID_HANDLE {
        return Err("computer session pipe connect failed".to_string());
    }
    if let Err(error) = check_server(pipe) {
        unsafe { CloseHandle(pipe) };
        return Err(error);
    }
    let mut file = unsafe { std::fs::File::from_raw_handle(pipe) };
    let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    write_frame(&mut file, &bytes, REQUEST_MAX_BYTES)?;
    serde_json::from_slice(&read_frame(&mut file, RESPONSE_MAX_BYTES)?).map_err(|e| e.to_string())
}

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

pub(super) fn exchange(
    config: &ClientConfig,
    request: WireRequest,
    timeout: Duration,
) -> Result<WireResponse, String> {
    // Synchronous Win32 byte pipes have no per-read deadline. Own one worker
    // at a time; timed-out effects remain unknown and cannot be redispatched.
    if IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return Err("computer session operation already in flight".to_string());
    }
    let config = config.clone();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = exchange_inner(&config, request);
        IN_FLIGHT.store(false, Ordering::Release);
        let _ = tx.send(result);
    });
    rx.recv_timeout(timeout)
        .map_err(|_| "computer session operation deadline exceeded".to_string())?
}

pub(super) fn native_session_available() -> bool {
    let active = unsafe { WTSGetActiveConsoleSessionId() };
    if active == 0
        || active == u32::MAX
        || session_id(unsafe { GetCurrentProcessId() }) != Some(active)
    {
        return false;
    }
    let desktop = unsafe { OpenInputDesktop(0, 0, 0x0001) }; // DESKTOP_READOBJECTS
    if desktop.is_null() {
        return false;
    }
    let mut name = [0_u16; 64];
    let mut needed = 0;
    let ok = unsafe {
        GetUserObjectInformationW(
            desktop,
            2,
            name.as_mut_ptr().cast(),
            (name.len() * 2) as u32,
            &mut needed,
        )
    } != 0; // UOI_NAME
    unsafe { CloseDesktop(desktop) };
    let end = name.iter().position(|ch| *ch == 0).unwrap_or(name.len());
    ok && String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("Default")
}

pub(super) fn run_helper(dir: &Path) -> Result<(), String> {
    let name = pipe_name(dir);
    // Only LocalSystem may connect. The creating user receives the server
    // handle directly and needs no DACL access. Reject remote pipe clients.
    let sddl = wide(OsStr::new("D:P(A;;GA;;;SY)"));
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err("computer session pipe ACL creation failed".to_string());
    }
    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor,
        inherit: 0,
    };
    let mut state = HelperState::new();
    loop {
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64 * 1024,
                64 * 1024,
                1000,
                &attributes,
            )
        };
        if pipe == INVALID_HANDLE {
            unsafe { LocalFree(descriptor) };
            return Err(
                "computer session pipe creation failed (another helper may own it)".to_string(),
            );
        }
        let connected = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if connected {
            let mut pid = 0;
            let expected = std::env::current_exe().ok();
            let peer_ok = unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } != 0
                && pid != 0
                && session_id(pid) == Some(0)
                && image_path(pid)
                    .zip(expected)
                    .is_some_and(|(actual, expected)| same_image(&actual, &expected));
            if peer_ok {
                let mut file = unsafe { std::fs::File::from_raw_handle(pipe) };
                let response = match read_frame(&mut file, REQUEST_MAX_BYTES).and_then(|bytes| {
                    serde_json::from_slice::<WireRequest>(&bytes).map_err(|e| e.to_string())
                }) {
                    Ok(request) => state.handle(request, pid, native_session_available()),
                    Err(_) => WireResponse::Rejected {
                        error: "invalid_request: malformed computer session frame".to_string(),
                    },
                };
                if let Ok(bytes) = serde_json::to_vec(&response) {
                    let _ = write_frame(&mut file, &bytes, RESPONSE_MAX_BYTES);
                }
                std::mem::forget(file); // disconnect and close below
            }
        }
        unsafe {
            DisconnectNamedPipe(pipe);
            CloseHandle(pipe);
        }
    }
}
