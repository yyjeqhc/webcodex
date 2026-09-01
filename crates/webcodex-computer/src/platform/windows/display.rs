use super::*;
#[cfg(windows)]
const MAX_NATIVE_DISPLAY_IDENTITY_BYTES: usize = 2048;
#[cfg(windows)]
const MAX_WINDOWS_DISPLAY_DEVICE_CHILDREN: u32 = 16;
#[cfg(windows)]
const MAX_WINDOWS_DISPLAY_SCAN: usize = 64;

#[cfg(windows)]
fn fixed_utf16_string(value: &[u16]) -> Result<String, String> {
    let end = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16(&value[..end])
        .map_err(|_| "display_failed: Windows display identity is invalid UTF-16".to_string())
}

#[cfg(windows)]
fn windows_display_identity(monitor: &Monitor) -> Result<Vec<u8>, String> {
    let adapter = monitor.name().map_err(|_| {
        "display_failed: Windows display adapter identity is unavailable".to_string()
    })?;
    if adapter.is_empty() || adapter.contains('\0') || adapter.len() > 512 {
        return Err("display_failed: Windows display adapter identity is invalid".to_string());
    }
    let mut adapter_wide = adapter.encode_utf16().collect::<Vec<_>>();
    adapter_wide.push(0);
    let adapter_wide = PCWSTR(adapter_wide.as_ptr());
    let mut interface_id: Option<String> = None;
    for index in 0..MAX_WINDOWS_DISPLAY_DEVICE_CHILDREN {
        let mut device = DISPLAY_DEVICEW::default();
        device.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
        let found = unsafe {
            EnumDisplayDevicesW(
                adapter_wide,
                index,
                &mut device,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        }
        .as_bool();
        if !found {
            break;
        }
        let candidate = fixed_utf16_string(&device.DeviceID)?;
        if candidate.is_empty() {
            continue;
        }
        if interface_id.is_some() {
            return Err(
                "display_failed: Windows display adapter has ambiguous monitor identity"
                    .to_string(),
            );
        }
        interface_id = Some(candidate);
    }
    let interface_id = interface_id.ok_or_else(|| {
        "display_failed: Windows display monitor interface identity is unavailable".to_string()
    })?;
    let mut identity = b"windows-display-v1\0".to_vec();
    identity.extend_from_slice(adapter.to_ascii_lowercase().as_bytes());
    identity.push(0);
    identity.extend_from_slice(interface_id.to_ascii_lowercase().as_bytes());
    if identity.len() > MAX_NATIVE_DISPLAY_IDENTITY_BYTES {
        return Err("display_failed: Windows display identity exceeds bound".to_string());
    }
    Ok(identity)
}

#[cfg(windows)]
fn platform_display_from_monitor(monitor: &Monitor) -> Result<PlatformDisplay, String> {
    let native_identity = windows_display_identity(monitor)?;
    let width = monitor
        .width()
        .map_err(|_| "display_failed: Windows display width is unavailable".to_string())?;
    let height = monitor
        .height()
        .map_err(|_| "display_failed: Windows display height is unavailable".to_string())?;
    let primary = monitor
        .is_primary()
        .map_err(|_| "display_failed: Windows display primary state is unavailable".to_string())?;
    if width == 0 || height == 0 {
        return Err("display_failed: Windows display geometry is invalid".to_string());
    }
    Ok(PlatformDisplay {
        native_identity,
        width,
        height,
        primary,
    })
}

#[cfg(windows)]
fn windows_monitors() -> Result<Vec<Monitor>, String> {
    let monitors = Monitor::all()
        .map_err(|_| "display_failed: Windows display enumeration failed".to_string())?;
    if monitors.len() > MAX_WINDOWS_DISPLAY_SCAN {
        return Err("display_failed: Windows display count exceeds native scan bound".to_string());
    }
    Ok(monitors)
}

#[cfg(windows)]
pub(crate) fn list_displays(limit: usize) -> Result<Vec<PlatformDisplay>, String> {
    if limit == 0 || limit > crate::MAX_DISPLAYS + 1 {
        return Err("invalid_request: display discovery native limit is invalid".to_string());
    }
    windows_monitors()?
        .into_iter()
        .take(limit)
        .map(|monitor| platform_display_from_monitor(&monitor))
        .collect()
}

#[cfg(windows)]
pub(super) fn find_exact_display(display: &DisplayRecord) -> Result<Monitor, String> {
    let mut exact = None;
    for monitor in windows_monitors()? {
        let candidate = platform_display_from_monitor(&monitor)?;
        if candidate.native_identity != display.native_identity {
            continue;
        }
        if candidate.width != display.width || candidate.height != display.height {
            return Err(
                "stale_display: native display geometry changed after discovery".to_string(),
            );
        }
        if exact.is_some() {
            return Err("stale_display: native display identity is no longer unique".to_string());
        }
        exact = Some(monitor);
    }
    exact.ok_or_else(|| "stale_display: native display identity changed or disappeared".to_string())
}

#[cfg(windows)]
pub(super) fn windows_virtual_desktop_metrics() -> Result<(i32, i32, u32, u32), String> {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = u32::try_from(unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }).map_err(|_| {
        "pointer_input_failed: Windows virtual desktop width is invalid".to_string()
    })?;
    let height = u32::try_from(unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) }).map_err(|_| {
        "pointer_input_failed: Windows virtual desktop height is invalid".to_string()
    })?;
    if width == 0 || height == 0 {
        return Err("pointer_input_failed: Windows virtual desktop geometry is empty".to_string());
    }
    Ok((left, top, width, height))
}

#[cfg(windows)]
pub(super) fn windows_monitor_rect(monitor: &Monitor) -> Result<(i32, i32, u32, u32), String> {
    let x = monitor
        .x()
        .map_err(|_| "pointer_input_failed: Windows monitor x origin is unavailable".to_string())?;
    let y = monitor
        .y()
        .map_err(|_| "pointer_input_failed: Windows monitor y origin is unavailable".to_string())?;
    let width = monitor
        .width()
        .map_err(|_| "pointer_input_failed: Windows monitor width is unavailable".to_string())?;
    let height = monitor
        .height()
        .map_err(|_| "pointer_input_failed: Windows monitor height is unavailable".to_string())?;
    if width == 0 || height == 0 {
        return Err("pointer_input_failed: Windows monitor geometry is empty".to_string());
    }
    Ok((x, y, width, height))
}

#[cfg(windows)]
pub(super) fn windows_xcap_virtual_bounds() -> Result<(i32, i32, u32, u32), String> {
    let monitors = windows_monitors()?;
    let mut left = i64::MAX;
    let mut top = i64::MAX;
    let mut right = i64::MIN;
    let mut bottom = i64::MIN;
    for monitor in monitors {
        let (x, y, width, height) = windows_monitor_rect(&monitor)?;
        let x = i64::from(x);
        let y = i64::from(y);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x.checked_add(i64::from(width)).ok_or_else(|| {
            "pointer_input_failed: Windows monitor right edge overflowed".to_string()
        })?);
        bottom = bottom.max(y.checked_add(i64::from(height)).ok_or_else(|| {
            "pointer_input_failed: Windows monitor bottom edge overflowed".to_string()
        })?);
    }
    if left == i64::MAX || top == i64::MAX || right <= left || bottom <= top {
        return Err("pointer_input_failed: Windows display topology is empty".to_string());
    }
    let width = u32::try_from(right - left).map_err(|_| {
        "pointer_input_failed: Windows display topology width is invalid".to_string()
    })?;
    let height = u32::try_from(bottom - top).map_err(|_| {
        "pointer_input_failed: Windows display topology height is invalid".to_string()
    })?;
    Ok((
        i32::try_from(left).map_err(|_| {
            "pointer_input_failed: Windows topology left edge is invalid".to_string()
        })?,
        i32::try_from(top).map_err(|_| {
            "pointer_input_failed: Windows topology top edge is invalid".to_string()
        })?,
        width,
        height,
    ))
}
