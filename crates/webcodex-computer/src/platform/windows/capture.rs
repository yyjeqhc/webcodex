use super::*;

#[cfg(windows)]
pub(crate) fn capture_display(display: &DisplayRecord) -> Result<image::RgbaImage, String> {
    ensure_capture_permission()?;
    let monitor = find_exact_display(display)?;
    ensure_raw_capture_bound(display.width, display.height)?;
    let image = monitor
        .capture_image()
        .map_err(|_| "capture_failed: Windows display capture failed".to_string())?;
    if image.width() != display.width || image.height() != display.height {
        return Err(
            "capture_failed: Windows display capture geometry does not match the exact display"
                .to_string(),
        );
    }
    // Revalidate again after capture so a hotplug/replacement racing the read
    // causes the captured bytes to be discarded instead of accepted under a stale handle.
    find_exact_display(display)?;
    Ok(image)
}

#[cfg(windows)]
pub(in crate::platform) fn ensure_capture_permission() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn win32_capture_error(operation: &str) -> String {
    format!(
        "capture_failed: {operation} failed: {}",
        std::io::Error::last_os_error()
    )
}

#[cfg(windows)]
struct WindowDc {
    hwnd: SysHwnd,
    hdc: HDC,
}

#[cfg(windows)]
impl WindowDc {
    fn acquire(hwnd: SysHwnd) -> Result<Self, String> {
        let hdc = unsafe { GetWindowDC(hwnd) };
        if hdc.is_null() {
            Err(win32_capture_error("GetWindowDC"))
        } else {
            Ok(Self { hwnd, hdc })
        }
    }
}

#[cfg(windows)]
impl Drop for WindowDc {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseDC(self.hwnd, self.hdc);
        }
    }
}

#[cfg(windows)]
struct MemoryDc(HDC);

#[cfg(windows)]
impl MemoryDc {
    fn create(source: HDC) -> Result<Self, String> {
        let hdc = unsafe { CreateCompatibleDC(source) };
        if hdc.is_null() {
            Err(win32_capture_error("CreateCompatibleDC"))
        } else {
            Ok(Self(hdc))
        }
    }
}

#[cfg(windows)]
impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.0);
        }
    }
}

#[cfg(windows)]
struct OwnedBitmap(HBITMAP);

#[cfg(windows)]
impl OwnedBitmap {
    fn create(source: HDC, plan: crate::RawCapturePlan) -> Result<Self, String> {
        let bitmap =
            unsafe { CreateCompatibleBitmap(source, plan.native_width, plan.native_height) };
        if bitmap.is_null() {
            Err(win32_capture_error("CreateCompatibleBitmap"))
        } else {
            Ok(Self(bitmap))
        }
    }
}

#[cfg(windows)]
impl Drop for OwnedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0 as HGDIOBJ);
        }
    }
}

#[cfg(windows)]
struct SelectedBitmap {
    hdc: HDC,
    previous: HGDIOBJ,
}

#[cfg(windows)]
impl SelectedBitmap {
    fn select(hdc: HDC, bitmap: HBITMAP) -> Result<Self, String> {
        let previous = unsafe { SelectObject(hdc, bitmap as HGDIOBJ) };
        if previous.is_null() {
            Err(win32_capture_error("SelectObject"))
        } else {
            Ok(Self { hdc, previous })
        }
    }
}

#[cfg(windows)]
impl Drop for SelectedBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.previous);
        }
    }
}

#[cfg(windows)]
fn create_bounded_bitmap(
    source: HDC,
    fallback: crate::RawCapturePlan,
) -> Result<(OwnedBitmap, crate::RawCapturePlan), String> {
    let mut native_width = fallback.native_width;
    let mut native_height = fallback.native_height;
    let current = unsafe { GetCurrentObject(source, OBJ_BITMAP as u32) };
    if !current.is_null() {
        let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
        let copied = unsafe {
            GetObjectW(
                current,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut bitmap as *mut BITMAP).cast(),
            )
        };
        if copied != 0 {
            native_width = bitmap.bmWidth;
            native_height = bitmap.bmHeight;
        }
    }

    let plan = crate::raw_capture_plan(native_width, native_height)?;
    let bitmap = OwnedBitmap::create(source, plan)?;
    Ok((bitmap, plan))
}

#[cfg(windows)]
pub(in crate::platform) fn capture_window_gdi(
    native_id: u32,
    output_width: u32,
    output_height: u32,
) -> Result<image::RgbaImage, String> {
    let output_plan = crate::raw_capture_plan(
        crate::native_capture_dimension(output_width, "window width")?,
        crate::native_capture_dimension(output_height, "window height")?,
    )?;
    let hwnd = native_id as i32 as isize as SysHwnd;
    if hwnd.is_null() {
        return Err("capture_failed: window handle is invalid".to_string());
    }

    let window_dc = WindowDc::acquire(hwnd)?;
    let (bitmap, capture_plan) = create_bounded_bitmap(window_dc.hdc, output_plan)?;
    let memory_dc = MemoryDc::create(window_dc.hdc)?;
    let selected = SelectedBitmap::select(memory_dc.0, bitmap.0)?;

    let mut captured = unsafe { PrintWindow(hwnd, memory_dc.0, 2) } != 0;
    if !captured {
        captured = unsafe { PrintWindow(hwnd, memory_dc.0, 0) } != 0;
    }
    if !captured {
        captured = unsafe { PrintWindow(hwnd, memory_dc.0, 4) } != 0;
    }
    if !captured {
        captured = unsafe {
            BitBlt(
                memory_dc.0,
                0,
                0,
                capture_plan.native_width,
                capture_plan.native_height,
                window_dc.hdc,
                0,
                0,
                SRCCOPY,
            )
        } != 0;
    }
    if !captured {
        return Err(win32_capture_error("PrintWindow/BitBlt"));
    }
    drop(selected);

    let raw_size = u32::try_from(capture_plan.byte_len)
        .map_err(|_| "image_too_large: raw RGBA capture size exceeds Win32".to_string())?;
    let mut bitmap_info: BITMAPINFO = unsafe { std::mem::zeroed() };
    bitmap_info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: capture_plan.native_width,
        biHeight: -capture_plan.native_height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: raw_size,
        ..unsafe { std::mem::zeroed() }
    };
    let mut pixels = vec![0u8; capture_plan.byte_len];
    let scan_lines = unsafe {
        GetDIBits(
            memory_dc.0,
            bitmap.0,
            0,
            capture_plan.height,
            pixels.as_mut_ptr().cast(),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };
    if scan_lines != capture_plan.native_height {
        return Err(win32_capture_error("GetDIBits"));
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let image = image::RgbaImage::from_raw(capture_plan.width, capture_plan.height, pixels)
        .ok_or_else(|| "capture_failed: RGBA image dimensions are inconsistent".to_string())?;
    if image.width() == output_plan.width && image.height() == output_plan.height {
        Ok(image)
    } else {
        Ok(image::imageops::resize(
            &image,
            output_plan.width,
            output_plan.height,
            image::imageops::FilterType::CatmullRom,
        ))
    }
}

#[cfg(windows)]
pub(in crate::platform) fn ensure_platform_capture_bound(
    window: &Window,
    width: u32,
    height: u32,
) -> Result<(), String> {
    // xcap 0.9.8 is built here without WGC, so its GDI path can allocate a
    // display-backed bitmap before cropping to the requested window. Bound
    // both the current window and current monitor; if monitor metrics cannot
    // be obtained we fail closed instead of pretending the window size alone
    // bounds the native allocation.
    ensure_raw_capture_bound(width, height)?;
    let monitor = window.current_monitor().map_err(map_error)?;
    let monitor_width = monitor.width().map_err(map_error)?;
    let monitor_height = monitor.height().map_err(map_error)?;
    ensure_raw_capture_bound(monitor_width, monitor_height)?;
    Ok(())
}

#[cfg(windows)]
pub(in crate::platform) fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
    // Windows xcap compares this HWND with GetForegroundWindow(), which is
    // an exact-window signal suitable for both CU-1 fields.
    let focused = window.is_focused().ok();
    (focused, focused)
}
