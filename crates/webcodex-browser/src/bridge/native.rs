//! Native Messaging host. Authentication stays in this process, never in the
//! extension, stdout diagnostics, browser pages or model-visible output.
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
    let parent = parent_pid()?;
    let mut socket = TcpStream::connect_timeout(&config.address, Duration::from_secs(2))?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    socket.set_nodelay(true)?;
    let hello = Hello {
        version: VERSION,
        instance: config.instance,
        token: config.token,
        extension_id: EXTENSION_ID.into(),
        parent_pid: parent,
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
        if parent_pid()? != parent {
            return Err(protocol::invalid());
        }
        if let Some(value) = frames.poll(&mut socket)? {
            protocol::write_frame(&mut stdout, &value, MAX_COMMAND, true)?;
        }
    }
}
