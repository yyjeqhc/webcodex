#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "macos")]
pub(super) use macos::*;
#[cfg(windows)]
pub(super) use windows::*;

use super::{bounded_text, PlatformWindow, SurfaceRecord};
use sha2::{Digest, Sha256};
use xcap::Window;

fn map_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("permission") {
        format!("permission_denied: {message}")
    } else {
        format!("capture_failed: {message}")
    }
}

#[cfg(any(target_os = "macos", windows))]
fn resolve_surface_window(surface: &SurfaceRecord) -> Result<Window, String> {
    let window = Window::all()
        .map_err(map_error)?
        .into_iter()
        .find(|window| window.id().ok() == Some(surface.native_id))
        .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
    let width = window.width().map_err(map_error)?;
    let height = window.height().map_err(map_error)?;
    let application = window.app_name().map_err(map_error)?;
    let title = window.title().map_err(map_error)?;
    let pid = window.pid().map_err(map_error)?;
    let current_identity = identity_hash(&window, &application, &title, width, height)?;
    if pid != surface.pid || current_identity != surface.identity_hash {
        return Err("stale_surface: window identity changed since enumeration".to_string());
    }
    if bounded_text(&application) != surface.application || bounded_text(&title) != surface.title {
        return Err("stale_surface: window metadata changed since enumeration".to_string());
    }
    Ok(window)
}

fn identity_hash(
    window: &Window,
    application: &str,
    title: &str,
    width: u32,
    height: u32,
) -> Result<[u8; 32], String> {
    let pid = window.pid().map_err(map_error)?;
    let mut hash = Sha256::new();
    hash.update(pid.to_le_bytes());
    hash.update((application.len() as u64).to_le_bytes());
    hash.update(application.as_bytes());
    hash.update((title.len() as u64).to_le_bytes());
    hash.update(title.as_bytes());
    hash.update(width.to_le_bytes());
    hash.update(height.to_le_bytes());
    Ok(hash.finalize().into())
}

pub(super) fn list_windows(limit: usize) -> Result<Vec<PlatformWindow>, String> {
    ensure_capture_permission()?;
    let mut output = Vec::new();
    for window in Window::all().map_err(map_error)? {
        if output.len() >= limit {
            break;
        }
        if window.is_minimized().unwrap_or(false) {
            continue;
        }
        let width = window.width().map_err(map_error)?;
        let height = window.height().map_err(map_error)?;
        if width == 0 || height == 0 {
            continue;
        }
        let application = window.app_name().map_err(map_error)?;
        let title = window.title().map_err(map_error)?;
        let identity_hash = identity_hash(&window, &application, &title, width, height)?;
        let (focused, active) = focus_state(&window);
        output.push(PlatformWindow {
            native_id: window.id().map_err(map_error)?,
            pid: window.pid().map_err(map_error)?,
            identity_hash,
            application,
            title,
            width,
            height,
            focused,
            active,
        });
    }
    Ok(output)
}

pub(super) fn capture_window(surface: &SurfaceRecord) -> Result<image::RgbaImage, String> {
    ensure_capture_permission()?;
    let window = Window::all()
        .map_err(map_error)?
        .into_iter()
        .find(|window| window.id().ok() == Some(surface.native_id))
        .ok_or_else(|| "stale_surface: window no longer exists".to_string())?;
    let width = window.width().map_err(map_error)?;
    let height = window.height().map_err(map_error)?;
    let application = window.app_name().map_err(map_error)?;
    let title = window.title().map_err(map_error)?;
    let current_identity = identity_hash(&window, &application, &title, width, height)?;
    if current_identity != surface.identity_hash {
        return Err("stale_surface: window identity changed since enumeration".to_string());
    }
    if bounded_text(&application) != surface.application || bounded_text(&title) != surface.title {
        return Err("stale_surface: window metadata changed since enumeration".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        ensure_platform_capture_bound(&window, width, height)?;
        window.capture_image().map_err(map_error)
    }
    #[cfg(windows)]
    {
        ensure_platform_capture_bound(&window, width, height)?;
        capture_window_gdi(surface.native_id, width, height)
    }
}
