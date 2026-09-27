//! TCC probes describe this Desktop process only, never the Runner's authority.
use crate::error::{DesktopError, DesktopResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

// Runner permission status is tri-state in the Desktop contract even though
// current macOS public probes can only report the Desktop process directly.
#[allow(dead_code)]
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    Unknown,
}

#[derive(Serialize)]
pub struct ComputerPermissions {
    pub supported: bool,
    pub foreground: bool,
    pub execution_process: Option<String>,
    pub execution_path: Option<String>,
    pub runner_accessibility: PermissionStatus,
    pub runner_screen_recording: PermissionStatus,
    pub desktop_accessibility: bool,
    pub desktop_screen_recording: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAction {
    Accessibility,
    ScreenRecording,
    OpenSettings,
    OpenAccessibilitySettings,
    OpenScreenRecordingSettings,
    ShowRunner,
}

pub fn probe() -> ComputerPermissions {
    probe_for_runner(None)
}

pub fn probe_for_runner(runner_path: Option<&Path>) -> ComputerPermissions {
    #[cfg(target_os = "macos")]
    unsafe {
        return ComputerPermissions {
            supported: true,
            foreground: false,
            execution_process: Some("WebCodex Runner".into()),
            execution_path: runner_path.map(|path| path.to_string_lossy().into_owned()),
            // macOS's public probes used here report only the current process.
            // Do not turn the Desktop result into a claim about another binary.
            runner_accessibility: PermissionStatus::Unknown,
            runner_screen_recording: PermissionStatus::Unknown,
            desktop_accessibility: macos::AXIsProcessTrusted(),
            desktop_screen_recording: macos::CGPreflightScreenCaptureAccess(),
        };
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = runner_path;
        ComputerPermissions {
            supported: false,
            foreground: false,
            execution_process: None,
            execution_path: None,
            runner_accessibility: PermissionStatus::Unknown,
            runner_screen_recording: PermissionStatus::Unknown,
            desktop_accessibility: false,
            desktop_screen_recording: false,
        }
    }
}

pub fn request(
    action: PermissionAction,
    runner_path: Option<&Path>,
) -> DesktopResult<ComputerPermissions> {
    #[cfg(target_os = "macos")]
    {
        match action {
            PermissionAction::Accessibility => unsafe {
                macos::request_accessibility();
            },
            PermissionAction::ScreenRecording => unsafe {
                macos::CGRequestScreenCaptureAccess();
            },
            PermissionAction::OpenSettings => open_settings("Privacy")?,
            PermissionAction::OpenAccessibilitySettings => open_settings("Privacy_Accessibility")?,
            PermissionAction::OpenScreenRecordingSettings => {
                open_settings("Privacy_ScreenCapture")?
            }
            PermissionAction::ShowRunner => {
                let path = runner_path
                    .filter(|path| path.exists())
                    .ok_or_else(unavailable)?;
                let status = std::process::Command::new("/usr/bin/open")
                    .arg("-R")
                    .arg(path)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|_| unavailable())?;
                if !status.success() {
                    return Err(unavailable());
                }
            }
        }
        Ok(probe_for_runner(runner_path))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (action, runner_path);
        Err(unavailable())
    }
}

#[cfg(target_os = "macos")]
fn open_settings(pane: &str) -> DesktopResult<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg(format!(
            "x-apple.systempreferences:com.apple.preference.security?{pane}"
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|_| unavailable())?;
    if status.success() {
        Ok(())
    } else {
        Err(unavailable())
    }
}

fn unavailable() -> DesktopError {
    DesktopError::new(
        "computer_permissions_unavailable",
        "System permission controls are unavailable",
        "Open system privacy settings and check the Runner permission owner.",
    )
}
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
        static kAXTrustedCheckOptionPrompt: *const c_void;
        pub fn CGPreflightScreenCaptureAccess() -> bool;
        pub fn CGRequestScreenCaptureAccess() -> bool;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: *const c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
        fn CFRelease(value: *const c_void);
    }
    pub unsafe fn request_accessibility() {
        // Static CF objects outlive this dictionary; no retain callbacks required.
        let key = kAXTrustedCheckOptionPrompt;
        let value = kCFBooleanTrue;
        let options = CFDictionaryCreate(
            std::ptr::null(),
            &key,
            &value,
            1,
            std::ptr::null(),
            std::ptr::null(),
        );
        if !options.is_null() {
            AXIsProcessTrustedWithOptions(options);
            CFRelease(options);
        }
    }
}
