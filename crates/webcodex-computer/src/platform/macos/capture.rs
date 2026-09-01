use super::*;

#[cfg(target_os = "macos")]
pub(super) fn capture_revalidated_macos_display<T>(
    display: &DisplayRecord,
    mut revalidate: impl FnMut(&DisplayRecord) -> Result<CGDirectDisplayID, String>,
    capture: impl FnOnce(CGDirectDisplayID) -> Result<T, String>,
    geometry: impl FnOnce(&T) -> (usize, usize),
) -> Result<T, String> {
    let before = revalidate(display)?;
    let captured = capture(before)?;
    let (captured_width, captured_height) = geometry(&captured);
    let after = revalidate(display)?;
    if after != before {
        return Err(
            "stale_display: macOS display identity changed while capture was in progress"
                .to_string(),
        );
    }
    if captured_width != display.width as usize || captured_height != display.height as usize {
        return Err(
            "capture_failed: macOS display capture pixel geometry does not match the exact display"
                .to_string(),
        );
    }
    Ok(captured)
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn create_macos_display_image(
    display_id: CGDirectDisplayID,
) -> Result<CFRetained<CGImage>, String> {
    objc2_core_graphics::CGDisplayCreateImage(display_id)
        .ok_or_else(|| "capture_failed: macOS exact display capture failed".to_string())
}

#[cfg(target_os = "macos")]
pub(super) fn macos_cg_image_to_rgba(
    image: &CGImage,
    width: u32,
    height: u32,
) -> Result<image::RgbaImage, String> {
    let byte_len = usize::try_from(ensure_raw_capture_bound(width, height)?).map_err(|_| {
        "image_too_large: raw macOS RGBA capture does not fit address space".to_string()
    })?;
    let bytes_per_row = usize::try_from(u64::from(width) * 4).map_err(|_| {
        "image_too_large: raw macOS RGBA row does not fit address space".to_string()
    })?;
    let mut pixels = vec![0u8; byte_len];
    let color_space = CGColorSpace::new_device_rgb()
        .ok_or_else(|| "capture_failed: macOS RGB color space is unavailable".to_string())?;
    let bitmap_info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
        )
    }
    .ok_or_else(|| "capture_failed: macOS RGBA bitmap context creation failed".to_string())?;
    CGContext::draw_image(
        Some(&context),
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: f64::from(width),
                height: f64::from(height),
            },
        },
        Some(image),
    );
    CGContext::flush(Some(&context));
    drop(context);
    image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| "capture_failed: macOS RGBA image dimensions are inconsistent".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn capture_display(display: &DisplayRecord) -> Result<image::RgbaImage, String> {
    ensure_capture_permission()?;
    ensure_raw_capture_bound(display.width, display.height)?;
    let image = capture_revalidated_macos_display(
        display,
        find_exact_macos_display,
        create_macos_display_image,
        |image| (CGImage::width(Some(image)), CGImage::height(Some(image))),
    )?;
    macos_cg_image_to_rgba(&image, display.width, display.height)
}

#[cfg(target_os = "macos")]
pub(in crate::platform) fn ensure_capture_permission() -> Result<(), String> {
    let granted = objc2_core_graphics::CGPreflightScreenCaptureAccess();
    if granted {
        Ok(())
    } else {
        Err("permission_denied: macOS Screen Recording permission is not granted".to_string())
    }
}

#[cfg(target_os = "macos")]
pub(in crate::platform) fn ensure_platform_capture_bound(
    _window: &Window,
    width: u32,
    height: u32,
) -> Result<(), String> {
    // xcap's macOS window bounds are display-space dimensions while
    // CGWindowListCreateImage can produce backing-pixel images. Use the
    // largest live display scale as a conservative bound, including windows
    // spanning displays; missing/invalid scale information fails closed.
    let monitors = xcap::Monitor::all().map_err(map_error)?;
    let mut max_scale: Option<f32> = None;
    for monitor in monitors {
        let scale = monitor.scale_factor().map_err(map_error)?;
        if !scale.is_finite() || scale <= 0.0 {
            return Err(
                "image_too_large: cannot establish a bounded macOS capture scale".to_string(),
            );
        }
        max_scale = Some(max_scale.map_or(scale, |current| current.max(scale)));
    }
    let scale = max_scale
        .ok_or_else(|| {
            "image_too_large: cannot establish a bounded macOS capture scale".to_string()
        })?
        .max(1.0);
    let scaled = |value: u32| -> Result<u32, String> {
        let pixels = (f64::from(value) * f64::from(scale)).ceil();
        if !pixels.is_finite() || pixels > f64::from(u32::MAX) {
            return Err("image_too_large: macOS capture dimensions overflow".to_string());
        }
        Ok(pixels as u32)
    };
    ensure_raw_capture_bound(scaled(width)?, scaled(height)?)?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(in crate::platform) fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
    // xcap 0.9.8 reports frontmost-application state on macOS, not exact
    // window focus. Preserve that reliable signal as `active` only.
    (None, window.is_focused().ok())
}
