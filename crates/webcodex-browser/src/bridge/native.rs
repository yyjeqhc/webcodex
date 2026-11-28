//! Native Messaging host. Authentication stays in this process, never in the
//! extension, stdout diagnostics, browser pages or model-visible output.
#[cfg(all(test, windows))]
mod tests;
use super::{profiles, protocol, Hello, Rendezvous};
use protocol::{EXTENSION_ID, MAX_COMMAND, MAX_RESPONSE, VERSION};
use serde_json::json;
use std::{
    io::{self, Read},
    net::TcpStream,
    time::{Duration, Instant},
};

fn parent_pid() -> io::Result<u32> {
    #[cfg(unix)]
    {
        let pid = unsafe { libc::getppid() };
        if pid > 1 {
            Ok(pid as u32)
        } else {
            Err(protocol::invalid())
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::Diagnostics::ToolHelp::*,
        };
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        let mut valid = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while valid {
            if entry.th32ProcessID == std::process::id() {
                found = Some(entry.th32ParentProcessID);
                break;
            }
            valid = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe {
            CloseHandle(snapshot);
        }
        found.filter(|pid| *pid > 0).ok_or_else(protocol::invalid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(io::ErrorKind::Unsupported.into())
    }
}

struct ParentProcess {
    pid: u32,
    #[cfg(windows)]
    process: std::os::windows::io::OwnedHandle,
}

impl ParentProcess {
    fn capture() -> io::Result<Self> {
        let pid = parent_pid()?;
        let parent = Self {
            pid,
            #[cfg(windows)]
            process: open_parent_process(pid)?,
        };
        parent.ensure_alive()?;
        Ok(parent)
    }

    fn ensure_alive(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            // The handle keeps the original process identity even after exit.
            // A repeated Toolhelp snapshot is expensive and only returns the
            // immutable creation-time parent PID; it cannot detect parent exit.
            match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
                WAIT_TIMEOUT => Ok(()),
                WAIT_OBJECT_0 => Err(protocol::invalid()),
                WAIT_FAILED => Err(io::Error::last_os_error()),
                _ => Err(protocol::invalid()),
            }
        }
        #[cfg(not(windows))]
        {
            if parent_pid()? == self.pid {
                Ok(())
            } else {
                Err(protocol::invalid())
            }
        }
    }
}

#[cfg(windows)]
fn open_parent_process(pid: u32) -> io::Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use windows_sys::Win32::{
        Foundation::{FILETIME, HANDLE},
        System::Threading::{
            GetCurrentProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_SYNCHRONIZE,
        },
    };

    fn created_at(process: HANDLE) -> io::Result<u64> {
        let mut created = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exited = created;
        let mut kernel = created;
        let mut user = created;
        if unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    let handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    // Own immediately so errors during identity validation also close it.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    // If the original parent exited before OpenProcess, its PID may now name a
    // later process. That object cannot be our parent; fail closed.
    if created_at(handle.as_raw_handle())? > created_at(unsafe { GetCurrentProcess() })? {
        return Err(protocol::invalid());
    }
    Ok(handle)
}

/// Called only by the dedicated Native Messaging binary, not by a tool gateway.
/// The manifest supplies the allowed caller origin; no endpoint/token argument is
/// accepted. Stdio framing and caller identity are validated before connecting.
pub fn run(args: &[String]) -> Result<(), &'static str> {
    if args.first().map(String::as_str) != Some(&format!("chrome-extension://{EXTENSION_ID}/"))
        || args.iter().skip(1).any(|arg| {
            !arg.strip_prefix("--parent-window=")
                .is_some_and(|value| value.parse::<u64>().is_ok())
        })
    {
        return Err("Browser native host requires its installed extension caller");
    }
    run_inner().map_err(|_| "Browser bridge is unavailable or the native connection closed")
}
fn run_inner() -> io::Result<()> {
    let root = profiles::state_root()
        .map_err(|_| protocol::invalid())?
        .join("extension");
    let mut file = profiles::private_file(&root.join("bridge.json"), false)
        .map_err(|_| protocol::invalid())?;
    let mut bytes = Vec::new();
    file.by_ref().take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(protocol::invalid());
    }
    let config: Rendezvous = serde_json::from_slice(&bytes).map_err(|_| protocol::invalid())?;
    if config.version != VERSION
        || !config.address.ip().is_loopback()
        || config.token.len() != 64
        || config.instance.len() != 32
    {
        return Err(protocol::invalid());
    }
    let parent = ParentProcess::capture()?;
    let mut socket = TcpStream::connect_timeout(&config.address, Duration::from_secs(2))?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    socket.set_nodelay(true)?;
    let hello = Hello {
        version: VERSION,
        instance: config.instance,
        token: config.token,
        extension_id: EXTENSION_ID.into(),
        parent_pid: parent.pid,
    };
    protocol::write_frame(
        &mut socket,
        &json!({"version":hello.version,"instance":hello.instance,"token":hello.token,
        "extension_id":hello.extension_id,"parent_pid":hello.parent_pid}),
        4096,
        false,
    )?;
    let mut frames = protocol::FrameReader::new(MAX_COMMAND);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            return Err(protocol::invalid());
        }
        if let Some(value) = frames.poll(&mut socket)? {
            if value["kind"] != "ready" || value["version"] != VERSION {
                return Err(protocol::invalid());
            }
            break;
        }
    }
    let mut outgoing = socket.try_clone()?;
    // Chrome owns this short-lived host process. EOF/disconnect returns from main,
    // terminating the stdio reader too; no Browser process is adopted or killed.
    std::thread::Builder::new()
        .name("browser-native-stdin".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            while let Ok(value) = protocol::read_blocking(&mut stdin, MAX_RESPONSE, true) {
                if protocol::write_frame(&mut outgoing, &value, MAX_RESPONSE, false).is_err() {
                    break;
                }
            }
            let _ = outgoing.shutdown(std::net::Shutdown::Both);
        })?;
    let mut stdout = io::stdout().lock();
    protocol::write_frame(
        &mut stdout,
        &json!({"kind":"ready","version":VERSION}),
        MAX_COMMAND,
        true,
    )?;
    loop {
        // Exact parent lifetime, not an untrusted PID reported by JavaScript.
        parent.ensure_alive()?;
        if let Some(value) = frames.poll(&mut socket)? {
            protocol::write_frame(&mut stdout, &value, MAX_COMMAND, true)?;
        }
    }
}
