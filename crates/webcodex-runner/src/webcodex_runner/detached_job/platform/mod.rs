//! Compile-time platform selection, not a second lifecycle or runtime trait registry.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(super) use unix::*;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub(super) use linux::*;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub(super) use macos::*;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::*;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod other_unix;
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
pub(super) use other_unix::*;
#[cfg(not(any(unix, windows)))]
mod unsupported;
#[cfg(not(any(unix, windows)))]
pub(super) use unsupported::*;
