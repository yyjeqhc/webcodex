//! Literal-argument, allowlisted navigation. No cmd/sh/PowerShell interpolation.
use crate::error::{DesktopError, DesktopResult};
use std::path::Path;

fn unavailable() -> DesktopError {
    DesktopError::new(
        "diagnostic_location_unavailable",
        "The selected location could not be opened",
        "Open the corresponding Runtime or app-data location with your system file manager.",
    )
}

pub fn directory(path: &Path) -> DesktopResult<()> {
    let path = path.canonicalize().map_err(|_| unavailable())?;
    if !path.is_dir() {
        return Err(unavailable());
    }
    open_literal(path.as_os_str())
}

pub fn url(url: &str) -> DesktopResult<()> {
    let parsed = url::Url::parse(url).map_err(|_| unavailable())?;
    let local = matches!(
        parsed.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    let github = parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.port().is_none()
        && (parsed.path() == "/yyjeqhc/webcodex"
            || parsed.path().starts_with("/yyjeqhc/webcodex/"));
    if (!local && !github)
        || !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(unavailable());
    }
    if local && parsed.path() != "/runtime" && parsed.path() != "/runtime/" {
        return Err(unavailable());
    }
    open_literal(std::ffi::OsStr::new(url))
}

fn open_literal(target: &std::ffi::OsStr) -> DesktopResult<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        let file: Vec<u16> = target.encode_wide().chain(Some(0)).collect();
        let verb: Vec<u16> = "open".encode_utf16().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        };
        if (result as isize) <= 32 {
            return Err(unavailable());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::process::{Command, Stdio};
        let mut command = Command::new(if cfg!(target_os = "macos") {
            "/usr/bin/open"
        } else {
            "xdg-open"
        });
        command
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Only start the OS launcher; do not manage/terminate the user's browser
        // or file manager as if it were a Desktop-owned Runtime process.
        let mut child = command.spawn().map_err(|_| unavailable())?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}
