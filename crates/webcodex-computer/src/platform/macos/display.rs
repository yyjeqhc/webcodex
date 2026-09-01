use super::*;
#[cfg(target_os = "macos")]
const MAX_MACOS_DISPLAY_SCAN: usize = 64;
#[cfg(target_os = "macos")]
const MAX_MACOS_DISPLAY_IDENTITY_BYTES: usize = 256;

#[cfg(target_os = "macos")]
#[link(name = "ColorSync", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display_id: u32) -> *const CFUUID;
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MacDisplayDescriptor {
    display_id: CGDirectDisplayID,
    stable_identity: Vec<u8>,
    display: PlatformDisplay,
}

#[cfg(target_os = "macos")]
fn macos_display_uuid(display_id: CGDirectDisplayID) -> Result<[u8; 16], String> {
    let pointer = unsafe { CGDisplayCreateUUIDFromDisplayID(display_id) };
    let pointer = NonNull::new(pointer.cast_mut())
        .ok_or_else(|| "display_failed: macOS stable display UUID is unavailable".to_string())?;
    let uuid = unsafe { CFRetained::from_raw(pointer) };
    Ok(uuid.uuid_bytes().into())
}

#[cfg(target_os = "macos")]
fn macos_stable_display_identity(display_id: CGDirectDisplayID) -> Result<Vec<u8>, String> {
    let mut identity = b"macos-display-stable-v1\0".to_vec();
    identity.extend_from_slice(&CGDisplayVendorNumber(display_id).to_be_bytes());
    identity.extend_from_slice(&CGDisplayModelNumber(display_id).to_be_bytes());
    identity.extend_from_slice(&CGDisplaySerialNumber(display_id).to_be_bytes());
    identity.extend_from_slice(&CGDisplayUnitNumber(display_id).to_be_bytes());
    identity.push(u8::from(CGDisplayIsBuiltin(display_id)));
    if identity.len() > MAX_MACOS_DISPLAY_IDENTITY_BYTES {
        return Err("display_failed: macOS stable display identity exceeds bound".to_string());
    }
    Ok(identity)
}

#[cfg(target_os = "macos")]
fn macos_bound_display_identity(
    stable_identity: &[u8],
    display_uuid: [u8; 16],
    display_id: CGDirectDisplayID,
) -> Result<Vec<u8>, String> {
    let mut identity = b"macos-display-binding-v1\0".to_vec();
    let stable_len = u16::try_from(stable_identity.len())
        .map_err(|_| "display_failed: macOS stable display identity exceeds bound".to_string())?;
    identity.extend_from_slice(&stable_len.to_be_bytes());
    identity.extend_from_slice(stable_identity);
    identity.extend_from_slice(&display_uuid);
    identity.extend_from_slice(&display_id.to_be_bytes());
    if identity.len() > MAX_MACOS_DISPLAY_IDENTITY_BYTES {
        return Err("display_failed: macOS bound display identity exceeds bound".to_string());
    }
    Ok(identity)
}

#[cfg(target_os = "macos")]
fn checked_macos_source_pixel_geometry(
    pixel_width: usize,
    pixel_height: usize,
) -> Result<(u32, u32), String> {
    if pixel_width == 0 || pixel_height == 0 {
        return Err(
            "display_failed: macOS current display mode pixel geometry is invalid".to_string(),
        );
    }
    let width = u32::try_from(pixel_width).map_err(|_| {
        "display_failed: macOS current display mode pixel width exceeds u32".to_string()
    })?;
    let height = u32::try_from(pixel_height).map_err(|_| {
        "display_failed: macOS current display mode pixel height exceeds u32".to_string()
    })?;
    ensure_raw_capture_bound(width, height).map_err(|error| {
        format!("display_failed: macOS current display mode pixel geometry exceeds raw capture bound: {error}")
    })?;
    Ok((width, height))
}

#[cfg(target_os = "macos")]
fn macos_display_source_pixel_geometry(
    display_id: CGDirectDisplayID,
) -> Result<(u32, u32), String> {
    let mode = CGDisplayCopyDisplayMode(display_id)
        .ok_or_else(|| "display_failed: macOS current display mode is unavailable".to_string())?;
    checked_macos_source_pixel_geometry(
        CGDisplayMode::pixel_width(Some(&mode)),
        CGDisplayMode::pixel_height(Some(&mode)),
    )
}

#[cfg(target_os = "macos")]
fn macos_display_descriptor(display_id: CGDirectDisplayID) -> Result<MacDisplayDescriptor, String> {
    if display_id == 0 {
        return Err("display_failed: macOS returned a null display id".to_string());
    }
    let (width, height) = macos_display_source_pixel_geometry(display_id)?;
    let stable_identity = macos_stable_display_identity(display_id)?;
    let native_identity = macos_bound_display_identity(
        &stable_identity,
        macos_display_uuid(display_id)?,
        display_id,
    )?;
    Ok(MacDisplayDescriptor {
        display_id,
        stable_identity,
        display: PlatformDisplay {
            native_identity,
            width,
            height,
            primary: CGDisplayIsMain(display_id),
        },
    })
}

#[cfg(target_os = "macos")]
fn macos_display_descriptors() -> Result<Vec<MacDisplayDescriptor>, String> {
    let native_limit = MAX_MACOS_DISPLAY_SCAN
        .checked_add(1)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "display_failed: macOS display scan bound is invalid".to_string())?;
    let mut display_ids = vec![0; native_limit as usize];
    let mut display_count = 0u32;
    let error = unsafe {
        CGGetActiveDisplayList(native_limit, display_ids.as_mut_ptr(), &mut display_count)
    };
    if error != CGError::Success {
        return Err(format!(
            "display_failed: macOS active display enumeration failed with CGError({})",
            error.0
        ));
    }
    let display_count = usize::try_from(display_count)
        .map_err(|_| "display_failed: macOS display count exceeds usize".to_string())?;
    if display_count > MAX_MACOS_DISPLAY_SCAN || display_count > display_ids.len() {
        return Err("display_failed: macOS display count exceeds native scan bound".to_string());
    }
    display_ids.truncate(display_count);
    display_ids
        .into_iter()
        .map(macos_display_descriptor)
        .collect()
}

#[cfg(target_os = "macos")]
fn ensure_unique_macos_display_identities(
    displays: &[MacDisplayDescriptor],
    error_kind: &str,
) -> Result<(), String> {
    for (index, display) in displays.iter().enumerate() {
        if displays[..index]
            .iter()
            .any(|prior| prior.stable_identity == display.stable_identity)
        {
            return Err(format!(
                "{error_kind}: macOS stable display identity is ambiguous"
            ));
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_exact_macos_display_in(
    display: &DisplayRecord,
    candidates: &[MacDisplayDescriptor],
) -> Result<CGDirectDisplayID, String> {
    ensure_unique_macos_display_identities(candidates, "stale_display")?;
    let mut exact = None;
    for candidate in candidates {
        if candidate.display.native_identity != display.native_identity {
            continue;
        }
        if candidate.display.width != display.width || candidate.display.height != display.height {
            return Err(
                "stale_display: macOS display source pixel geometry changed after discovery"
                    .to_string(),
            );
        }
        if exact.replace(candidate.display_id).is_some() {
            return Err("stale_display: macOS display identity is no longer unique".to_string());
        }
    }
    exact.ok_or_else(|| {
        "stale_display: macOS display identity changed, was replaced, or disappeared".to_string()
    })
}

#[cfg(target_os = "macos")]
pub(super) fn find_exact_macos_display(
    display: &DisplayRecord,
) -> Result<CGDirectDisplayID, String> {
    let candidates = macos_display_descriptors()?;
    find_exact_macos_display_in(display, &candidates)
}

#[cfg(target_os = "macos")]
pub(crate) fn list_displays(limit: usize) -> Result<Vec<PlatformDisplay>, String> {
    if limit == 0 || limit > crate::MAX_DISPLAYS + 1 {
        return Err("invalid_request: display discovery native limit is invalid".to_string());
    }
    let displays = macos_display_descriptors()?;
    ensure_unique_macos_display_identities(&displays, "display_failed")?;
    Ok(displays
        .into_iter()
        .take(limit)
        .map(|descriptor| descriptor.display)
        .collect())
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_display_identity_revalidates_for_test(
    display: &PlatformDisplay,
) -> Result<(), String> {
    let record = DisplayRecord {
        native_identity: display.native_identity.clone(),
        width: display.width,
        height: display.height,
        primary: display.primary,
    };
    find_exact_macos_display(&record).map(|_| ())
}

#[cfg(all(test, target_os = "macos"))]
mod macos_display_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn descriptor(
        display_id: CGDirectDisplayID,
        stable_marker: u8,
        width: u32,
        height: u32,
    ) -> MacDisplayDescriptor {
        let stable_identity = vec![stable_marker];
        MacDisplayDescriptor {
            display_id,
            stable_identity: stable_identity.clone(),
            display: PlatformDisplay {
                native_identity: macos_bound_display_identity(
                    &stable_identity,
                    [stable_marker; 16],
                    display_id,
                )
                .unwrap(),
                width,
                height,
                primary: display_id == 1,
            },
        }
    }

    fn record(descriptor: &MacDisplayDescriptor) -> DisplayRecord {
        DisplayRecord {
            native_identity: descriptor.display.native_identity.clone(),
            width: descriptor.display.width,
            height: descriptor.display.height,
            primary: descriptor.display.primary,
        }
    }

    #[test]
    fn macos_display_rgba_conversion_preserves_vertical_order_and_channels() {
        let width = 2u32;
        let height = 2u32;
        let mut source_pixels = vec![
            10u8, 20, 30, 255, 40, 50, 60, 255, // top row
            200, 150, 100, 255, 70, 80, 90, 255, // bottom row
        ];
        let color_space = CGColorSpace::new_device_rgb().expect("synthetic RGB color space");
        let bitmap_info =
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
        let context = unsafe {
            CGBitmapContextCreate(
                source_pixels.as_mut_ptr().cast(),
                width as usize,
                height as usize,
                8,
                width as usize * 4,
                Some(&color_space),
                bitmap_info,
            )
        }
        .expect("synthetic RGBA bitmap context");
        let source = CGBitmapContextCreateImage(Some(&context)).expect("synthetic CGImage");
        drop(context);

        let converted =
            macos_cg_image_to_rgba(&source, width, height).expect("production RGBA conversion");
        assert_eq!(converted.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(converted.get_pixel(1, 0).0, [40, 50, 60, 255]);
        assert_eq!(converted.get_pixel(0, 1).0, [200, 150, 100, 255]);
        assert_eq!(converted.get_pixel(1, 1).0, [70, 80, 90, 255]);
    }

    #[test]
    fn macos_hidpi_display_source_geometry_uses_mode_backing_pixels() {
        let logical_display_geometry = (1920u32, 1080u32);
        let source_geometry = checked_macos_source_pixel_geometry(3840, 2160).unwrap();
        assert_eq!(source_geometry, (3840, 2160));
        assert_ne!(source_geometry, logical_display_geometry);

        let discovered = descriptor(9, 3, source_geometry.0, source_geometry.1);
        let record = record(&discovered);
        let captured = capture_revalidated_macos_display(
            &record,
            |_| Ok(9),
            |_| Ok((3840usize, 2160usize)),
            |geometry| *geometry,
        )
        .expect("HiDPI capture must use current-mode source pixels");
        assert_eq!(captured, (3840, 2160));
    }

    #[test]
    fn macos_source_pixel_geometry_is_positive_u32_and_raw_capture_bounded() {
        for geometry in [(0, 2160), (3840, 0)] {
            let error = checked_macos_source_pixel_geometry(geometry.0, geometry.1).unwrap_err();
            assert!(error.starts_with("display_failed:"), "{error}");
        }
        let error = checked_macos_source_pixel_geometry(usize::MAX, 1).unwrap_err();
        assert!(error.contains("exceeds u32"), "{error}");
        let error = checked_macos_source_pixel_geometry(8192, 4097).unwrap_err();
        assert!(error.contains("raw capture bound"), "{error}");
    }

    #[test]
    fn macos_display_identity_revalidation_fails_closed_on_replacement_hotplug_and_geometry() {
        let discovered = descriptor(1, 7, 3840, 2160);
        let record = record(&discovered);
        assert_eq!(
            find_exact_macos_display_in(&record, std::slice::from_ref(&discovered)).unwrap(),
            1
        );

        let replacement = descriptor(1, 8, 3840, 2160);
        let error = find_exact_macos_display_in(&record, &[replacement])
            .expect_err("same native id with a different stable identity must be stale");
        assert!(error.starts_with("stale_display:"), "{error}");

        let replugged = descriptor(2, 7, 3840, 2160);
        let error = find_exact_macos_display_in(&record, &[replugged])
            .expect_err("a replugged display with a new native id must be stale");
        assert!(error.starts_with("stale_display:"), "{error}");

        let mut changed_geometry = discovered.clone();
        changed_geometry.display.width += 1;
        let error = find_exact_macos_display_in(&record, &[changed_geometry])
            .expect_err("source pixel geometry changes must be stale");
        assert!(error.starts_with("stale_display:"), "{error}");

        let ambiguous = [descriptor(1, 7, 3840, 2160), descriptor(2, 7, 3840, 2160)];
        let error = find_exact_macos_display_in(&record, &ambiguous)
            .expect_err("ambiguous stable native identity must fail closed");
        assert!(
            error.contains("stable display identity is ambiguous"),
            "{error}"
        );
    }

    #[derive(Debug)]
    struct SimulatedCapture {
        width: usize,
        height: usize,
        dropped: Rc<Cell<bool>>,
    }

    impl Drop for SimulatedCapture {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }

    #[test]
    fn macos_display_capture_revalidates_before_and_after_and_discards_races() {
        let discovered = descriptor(9, 3, 3840, 2160);
        let record = record(&discovered);
        let validations = Cell::new(0);
        let dropped = Rc::new(Cell::new(false));
        let captured = capture_revalidated_macos_display(
            &record,
            |_| {
                validations.set(validations.get() + 1);
                Ok(9)
            },
            |_| {
                Ok(SimulatedCapture {
                    width: 3840,
                    height: 2160,
                    dropped: Rc::clone(&dropped),
                })
            },
            |capture| (capture.width, capture.height),
        )
        .unwrap();
        assert_eq!(validations.get(), 2);
        assert!(!dropped.get());
        drop(captured);
        assert!(dropped.get());

        let validations = Cell::new(0);
        let dropped = Rc::new(Cell::new(false));
        let error = capture_revalidated_macos_display(
            &record,
            |_| {
                let call = validations.get();
                validations.set(call + 1);
                if call == 0 {
                    Ok(9)
                } else {
                    Err("stale_display: simulated hotplug during capture".to_string())
                }
            },
            |_| {
                Ok(SimulatedCapture {
                    width: 3840,
                    height: 2160,
                    dropped: Rc::clone(&dropped),
                })
            },
            |capture| (capture.width, capture.height),
        )
        .expect_err("post-capture identity change must discard captured bytes");
        assert!(error.starts_with("stale_display:"), "{error}");
        assert_eq!(validations.get(), 2);
        assert!(dropped.get());
    }

    #[test]
    fn macos_display_capture_rejects_wrong_backing_geometry_after_post_revalidation() {
        let discovered = descriptor(4, 2, 3840, 2160);
        let record = record(&discovered);
        for captured_geometry in [(1920usize, 1080usize), (3840usize, 2159usize)] {
            let validations = Cell::new(0);
            let error = capture_revalidated_macos_display(
                &record,
                |_| {
                    validations.set(validations.get() + 1);
                    Ok(4)
                },
                |_| Ok(captured_geometry),
                |geometry| *geometry,
            )
            .expect_err("captured backing pixel geometry must exactly match source pixels");
            assert!(error.starts_with("capture_failed:"), "{error}");
            assert_eq!(validations.get(), 2);
        }
    }
}
