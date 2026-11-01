//! Local-only, bounded management pipe. It never accepts Computer operations.
use super::*;
use sha2::{Digest, Sha256};
use std::ffi::{c_void, OsStr, OsString};
use std::io::{Read, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::FromRawHandle;
use std::time::{Duration, Instant};

type Handle = *mut c_void;
const INVALID: Handle = -1_isize as Handle;
const DEADLINE: Duration = Duration::from_millis(750);
const MAX_FRAME: usize = 1024;

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
        mode: u32,
        pipe_mode: u32,
        instances: u32,
        out_size: u32,
        in_size: u32,
        timeout: u32,
        security: *const SecurityAttributes,
    ) -> Handle;
    fn ConnectNamedPipe(pipe: Handle, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: Handle) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn SetNamedPipeHandleState(
        pipe: Handle,
        mode: *const u32,
        collection: *const u32,
        timeout: *const u32,
    ) -> i32;
    fn GetNamedPipeServerProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn GetNamedPipeClientProcessId(pipe: Handle, pid: *mut u32) -> i32;
    fn PeekNamedPipe(
        pipe: Handle,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
    fn GetCurrentProcessId() -> u32;
    fn GetLastError() -> u32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        name: *mut u16,
        length: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
    fn LocalFree(value: Handle) -> Handle;
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

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn session(pid: u32) -> Result<u32, String> {
    let mut value = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut value) } == 0 || value == 0 {
        return Err(INTERACTIVE_SESSION_REQUIRED_ERROR.into());
    }
    Ok(value)
}
fn pipe_name(dir: &Path) -> Result<Vec<u16>, String> {
    let path = std::fs::canonicalize(dir).map_err(|_| "computer_control_unavailable")?;
    let path_bytes: Vec<u8> = path
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect();
    let digest = Sha256::digest(path_bytes);
    Ok(wide(OsStr::new(&format!(
        r"\\.\pipe\webcodex-computer-control-{}-{:x}",
        session(unsafe { GetCurrentProcessId() })?,
        digest
    ))))
}
fn check_peer(pid: u32, expected: &Path) -> Result<(), String> {
    if session(pid)? != session(unsafe { GetCurrentProcessId() })? {
        return Err("computer_control_unavailable: peer session changed".into());
    }
    let process = unsafe { OpenProcess(0x1000, 0, pid) };
    if process.is_null() {
        return Err("computer_control_unavailable".into());
    }
    let mut name = [0_u16; 32768];
    let mut length = name.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(process, 0, name.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    let actual = OsString::from_wide(&name[..length as usize]);
    let expected = std::fs::canonicalize(expected).map_err(|_| "computer_control_unavailable")?;
    let actual = std::fs::canonicalize(actual).map_err(|_| "computer_control_unavailable")?;
    if !ok || actual != expected {
        return Err("computer_control_unavailable: peer executable changed".into());
    }
    Ok(())
}

fn retry_io<T>(
    deadline: Instant,
    mut action: impl FnMut() -> std::io::Result<T>,
) -> Result<T, String> {
    loop {
        match action() {
            Ok(value) => return Ok(value),
            Err(error)
                if matches!(error.raw_os_error(), Some(231 | 232 | 536))
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => {
                return Err(
                    "computer_control_unavailable: local pipe deadline or disconnect".into(),
                )
            }
        }
    }
}
fn read_frame(file: &mut std::fs::File, deadline: Instant) -> Result<Vec<u8>, String> {
    fn exact(file: &mut std::fs::File, buffer: &mut [u8], deadline: Instant) -> Result<(), String> {
        let mut read = 0;
        while read < buffer.len() {
            let count = retry_io(deadline, || match file.read(&mut buffer[read..])? {
                0 => {
                    use std::os::windows::io::AsRawHandle;
                    if unsafe {
                        PeekNamedPipe(
                            file.as_raw_handle(),
                            std::ptr::null_mut(),
                            0,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                        )
                    } == 0
                    {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Err(std::io::Error::from_raw_os_error(232))
                    }
                }
                count => Ok(count),
            })?;
            read += count;
        }
        Ok(())
    }
    let mut length = [0; 4];
    exact(file, &mut length, deadline)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err("computer_control_unavailable: frame bound".into());
    }
    let mut bytes = vec![0; length];
    exact(file, &mut bytes, deadline)?;
    Ok(bytes)
}
fn write_frame(file: &mut std::fs::File, bytes: &[u8], deadline: Instant) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("computer_control_unavailable: frame bound".into());
    }
    let buffer = [(bytes.len() as u32).to_be_bytes().as_slice(), bytes].concat();
    let mut written = 0;
    while written < buffer.len() {
        let count = retry_io(deadline, || match file.write(&buffer[written..])? {
            0 => Err(std::io::Error::from_raw_os_error(232)),
            count => Ok(count),
        })?;
        written += count;
    }
    Ok(())
}

pub fn request(
    dir: &Path,
    expected_runner: &Path,
    request: &LocalRequest,
) -> Result<ControlSnapshot, String> {
    let name = pipe_name(dir)?;
    let deadline = Instant::now() + DEADLINE;
    let pipe = retry_io(deadline, || {
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                0xc0000000,
                0,
                std::ptr::null_mut(),
                3,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(handle)
        }
    })?;
    let mut file = unsafe { std::fs::File::from_raw_handle(pipe) };
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err("computer_control_unavailable".into());
    }
    check_peer(pid, expected_runner)?;
    let mode = 1_u32; // PIPE_NOWAIT, bounded reads/writes on both ends.
    if unsafe { SetNamedPipeHandleState(pipe, &mode, std::ptr::null(), std::ptr::null()) } == 0 {
        return Err("computer_control_unavailable".into());
    }
    let bytes = serde_json::to_vec(request).map_err(|_| "computer_control_unavailable")?;
    write_frame(&mut file, &bytes, deadline)?;
    let result: Result<ControlSnapshot, String> =
        serde_json::from_slice(&read_frame(&mut file, deadline)?)
            .map_err(|_| "computer_control_unavailable: invalid response")?;
    let _ = write_frame(&mut file, b"ack", deadline);
    result
}

pub fn serve(dir: &Path, control: Arc<LocalControl>) -> Result<(), String> {
    let name = pipe_name(dir)?;
    // Owner Rights grants the creating account access, without granting every
    // interactive user. The control pipe cannot execute native operations.
    let sddl = wide(OsStr::new("D:P(A;;GA;;;SY)(A;;GA;;;OW)"));
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
        return Err("computer_control_unavailable: pipe ACL".into());
    }
    let attributes = SecurityAttributes {
        length: std::mem::size_of::<SecurityAttributes>() as u32,
        descriptor,
        inherit: 0,
    };
    let pipe = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            3 | 0x80000,
            1 | 8,
            1,
            2048,
            2048,
            1000,
            &attributes,
        )
    };
    unsafe { LocalFree(descriptor) };
    if pipe == INVALID {
        return Err("computer_control_unavailable: local endpoint already owned".into());
    }
    // Capture the first handle before spawning, so startup fails synchronously
    // rather than silently losing the local stop entry point.
    let file = unsafe { std::fs::File::from_raw_handle(pipe) };
    std::thread::Builder::new()
        .name("computer-local-control".into())
        .spawn(move || {
            use std::os::windows::io::AsRawHandle;
            let mut file = file;
            loop {
                let pipe = file.as_raw_handle();
                let result = unsafe { ConnectNamedPipe(pipe, std::ptr::null_mut()) };
                let error = if result == 0 {
                    unsafe { GetLastError() }
                } else {
                    0
                };
                let connected = result != 0 || error == 535;
                if matches!(error, 109 | 232 | 233) {
                    // A client may close before ConnectNamedPipe observes it (for
                    // example after failing its server identity check).
                    unsafe { DisconnectNamedPipe(pipe) };
                }
                if !connected {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                let deadline = Instant::now() + DEADLINE;
                let mut pid = 0;
                // Desktop is a different binary. The protected pipe ACL and same
                // interactive session own client admission; executable pinning is
                // required in the client -> Runner direction.
                let admitted = unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } != 0
                    && session(pid).ok() == session(unsafe { GetCurrentProcessId() }).ok();
                if admitted {
                    let response = read_frame(&mut file, deadline)
                        .and_then(|bytes| {
                            serde_json::from_slice::<LocalRequest>(&bytes)
                                .map_err(|_| "computer_control_unavailable: invalid request".into())
                        })
                        .and_then(|request| request.apply(&control));
                    if let Ok(bytes) = serde_json::to_vec(&response) {
                        if write_frame(&mut file, &bytes, deadline).is_ok() {
                            let _ = read_frame(&mut file, deadline);
                        }
                    }
                }
                // Disconnect leaves this same server handle and ACL bound; a second
                // process cannot claim the endpoint between management requests.
                unsafe { DisconnectNamedPipe(pipe) };
            }
        })
        .map_err(|_| "computer_control_unavailable: management thread")?;
    Ok(())
}
