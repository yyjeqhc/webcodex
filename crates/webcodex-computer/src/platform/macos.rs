use super::{map_error, resolve_surface_window};
use crate::validate_key_input;
use crate::{
    bounded_text, ensure_raw_capture_bound, prepare_clipboard_write_text, validate_input_text,
    AccessibilityTreeResult, ApplicationRecord, ClipboardWriteEffectState, ComputerAction,
    DisplayRecord, ElementRecord, PlatformApplication, PlatformDisplay, PointerAction, PointerPlan,
    SurfaceRecord,
};
use crate::{is_supported_text_input_fingerprint, ElementFingerprint};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::time::{Duration, Instant};
use uuid::Uuid;
use xcap::Window;

#[cfg(target_os = "macos")]
use crate::{
    clipboard_read_result, ensure_correlated_fingerprint, is_secure_text_fingerprint,
    run_macos_clipboard_write_effect_steps, select_exact_ax_window_index,
    validate_element_state_target, validate_key_modifiers, validate_text_input_preflight,
    validate_text_input_target, AxObservationDeadline,
};

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeString, NSRunningApplication, NSWorkspace,
    NSWorkspaceOpenConfiguration,
};
#[cfg(target_os = "macos")]
use objc2_application_services::{AXError, AXIsProcessTrusted, AXUIElement, AXValue, AXValueType};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{
    CFArray, CFBoolean, CFIndex, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize, CFUUID,
};
#[cfg(all(test, target_os = "macos"))]
use objc2_core_graphics::CGBitmapContextCreateImage;
#[cfg(target_os = "macos")]
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDirectDisplayID, CGDisplayBounds,
    CGDisplayCopyDisplayMode, CGDisplayIsBuiltin, CGDisplayIsMain, CGDisplayMode,
    CGDisplayModelNumber, CGDisplayRotation, CGDisplaySerialNumber, CGDisplayUnitNumber,
    CGDisplayVendorNumber, CGError, CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID,
    CGEventTapLocation, CGEventType, CGGetActiveDisplayList, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGKeyCode, CGMouseButton, CGPreflightPostEventAccess,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{
    NSArray, NSBundle, NSDictionary, NSError, NSFileManager, NSSearchPathDirectory,
    NSSearchPathDomainMask, NSString, NSURL,
};
#[cfg(target_os = "macos")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver};
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_IDENTITY_BYTES: usize = 32 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_PATH_BYTES: usize = 8 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_BUNDLE_IDENTIFIER_BYTES: usize = 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_BUNDLE_METADATA_BYTES: usize = 4 * 1024;
#[cfg(target_os = "macos")]
const MAX_MACOS_APPLICATION_SCAN_DEPTH: usize = 2;
#[cfg(target_os = "macos")]
const MACOS_APPLICATION_LAUNCH_WAIT: Duration = Duration::from_secs(5);
#[cfg(target_os = "macos")]
const MAX_MACOS_DISPLAY_SCAN: usize = 64;
#[cfg(target_os = "macos")]
const MAX_MACOS_DISPLAY_IDENTITY_BYTES: usize = 256;
#[cfg(target_os = "macos")]
const MACOS_POINTER_READBACK_SETTLE_TIMEOUT: Duration = Duration::from_millis(50);
#[cfg(target_os = "macos")]
const MACOS_POINTER_READBACK_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[cfg(target_os = "macos")]
#[link(name = "ColorSync", kind = "framework")]
unsafe extern "C" {
    fn CGDisplayCreateUUIDFromDisplayID(display_id: u32) -> *const CFUUID;
}
#[cfg(target_os = "macos")]
pub(crate) fn read_clipboard() -> Result<Value, String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };
    let Some(native_text) = pasteboard.stringForType(string_type) else {
        return clipboard_read_result("macos", None);
    };
    if native_text.len() > crate::MAX_CLIPBOARD_TEXT_BYTES {
        return Err(
            "clipboard_too_large: clipboard UTF-8 text exceeds the 16 KiB bound".to_string(),
        );
    }
    let text = native_text.to_string();
    clipboard_read_result("macos", Some(&text))
}

#[cfg(target_os = "macos")]
pub(crate) fn write_clipboard(text: &str) -> Result<Value, String> {
    // Complete caller validation and native string/object construction before
    // clearContents crosses the pasteboard mutation boundary.
    let prepared = prepare_clipboard_write_text(text)?;
    let native_text = NSString::from_str(text);
    let pasteboard = NSPasteboard::generalPasteboard();
    let string_type = unsafe { NSPasteboardTypeString };

    let effect = run_macos_clipboard_write_effect_steps(
        || pasteboard.clearContents(),
        || pasteboard.setString_forType(&native_text, string_type),
        || pasteboard.changeCount(),
    );
    match effect {
        ClipboardWriteEffectState::Success => Ok(json!({
            "platform": "macos",
            "text_bytes": prepared.text_bytes,
            "success": true,
        })),
        ClipboardWriteEffectState::OutcomeUnknown => Err(
            "outcome_unknown: macOS pasteboard changed after clearContents but the complete NSPasteboardTypeString replacement could not be proven"
                .to_string(),
        ),
        #[cfg(test)]
        ClipboardWriteEffectState::NotStarted => {
            unreachable!("macOS clearContents has no definite native failure result")
        }
    }
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
fn find_exact_macos_display(display: &DisplayRecord) -> Result<CGDirectDisplayID, String> {
    let candidates = macos_display_descriptors()?;
    find_exact_macos_display_in(display, &candidates)
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerNativeGeometry {
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    rotation_degrees: f64,
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerPlan {
    target_x: f64,
    target_y: f64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct MacPointerPreflight {
    target: MacPointerPlan,
    display_id: CGDirectDisplayID,
    geometry: MacPointerNativeGeometry,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MacPointerInputState {
    buttons_down: u32,
    modifier_flags: CGEventFlags,
}

#[cfg(target_os = "macos")]
fn macos_pointer_native_geometry(display_id: CGDirectDisplayID) -> MacPointerNativeGeometry {
    let bounds = CGDisplayBounds(display_id);
    MacPointerNativeGeometry {
        origin_x: bounds.origin.x,
        origin_y: bounds.origin.y,
        width: bounds.size.width,
        height: bounds.size.height,
        rotation_degrees: CGDisplayRotation(display_id),
    }
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_native_geometry(
    geometry: MacPointerNativeGeometry,
) -> Result<(), String> {
    if !geometry.origin_x.is_finite()
        || !geometry.origin_y.is_finite()
        || !geometry.width.is_finite()
        || !geometry.height.is_finite()
        || !geometry.rotation_degrees.is_finite()
    {
        return Err(
            "pointer_input_failed: macOS display bounds or rotation is non-finite".to_string(),
        );
    }
    if geometry.width <= 0.0 || geometry.height <= 0.0 {
        return Err("pointer_input_failed: macOS display bounds are empty or invalid".to_string());
    }
    let right = geometry.origin_x + geometry.width;
    let bottom = geometry.origin_y + geometry.height;
    if !right.is_finite()
        || !bottom.is_finite()
        || right <= geometry.origin_x
        || bottom <= geometry.origin_y
    {
        return Err(
            "pointer_input_failed: macOS display bounds cannot form an exact half-open rectangle"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn map_macos_pointer_coordinate(
    source_width: u32,
    source_height: u32,
    geometry: MacPointerNativeGeometry,
    x: u32,
    y: u32,
) -> Result<MacPointerPlan, String> {
    if source_width == 0 || source_height == 0 || x >= source_width || y >= source_height {
        return Err(
            "invalid_request: pointer coordinate is outside snapshot source geometry".to_string(),
        );
    }
    validate_macos_pointer_native_geometry(geometry)?;
    if geometry.rotation_degrees != 0.0 {
        return Err(
            "pointer_input_failed: macOS pointer mapping supports only an exact 0-degree display rotation"
                .to_string(),
        );
    }

    let target_x = geometry.origin_x + (f64::from(x) / f64::from(source_width)) * geometry.width;
    let target_y = geometry.origin_y + (f64::from(y) / f64::from(source_height)) * geometry.height;
    let right = geometry.origin_x + geometry.width;
    let bottom = geometry.origin_y + geometry.height;
    if !target_x.is_finite()
        || !target_y.is_finite()
        || target_x < geometry.origin_x
        || target_x >= right
        || target_y < geometry.origin_y
        || target_y >= bottom
    {
        return Err(
            "pointer_input_failed: macOS pointer target is outside exact display bounds"
                .to_string(),
        );
    }
    Ok(MacPointerPlan { target_x, target_y })
}

#[cfg(target_os = "macos")]
fn macos_pointer_input_state() -> MacPointerInputState {
    let state_id = CGEventSourceStateID::CombinedSessionState;
    let mut buttons_down = 0u32;
    for button in 0..32u32 {
        if CGEventSource::button_state(state_id, CGMouseButton(button)) {
            buttons_down |= 1u32 << button;
        }
    }
    MacPointerInputState {
        buttons_down,
        modifier_flags: CGEventSource::flags_state(state_id),
    }
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_input_state(
    action: PointerAction,
    state: MacPointerInputState,
) -> Result<(), String> {
    if state.buttons_down != 0 {
        return Err(
            "pointer_input_failed: shared desktop mouse button is already down".to_string(),
        );
    }
    if action == PointerAction::Click
        && state.modifier_flags.intersects(
            CGEventFlags::MaskShift
                | CGEventFlags::MaskControl
                | CGEventFlags::MaskAlternate
                | CGEventFlags::MaskCommand,
        )
    {
        return Err(
            "pointer_input_failed: shared desktop modifier key is already active".to_string(),
        );
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_macos_pointer_permission() -> Result<(), String> {
    if CGPreflightPostEventAccess() {
        Ok(())
    } else {
        Err("permission_denied: macOS event-posting permission is not granted".to_string())
    }
}

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_plan_with(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
    mut revalidate: impl FnMut(&DisplayRecord) -> Result<CGDirectDisplayID, String>,
    mut native_geometry: impl FnMut(CGDirectDisplayID) -> MacPointerNativeGeometry,
    mut input_state: impl FnMut() -> MacPointerInputState,
    mut permission_preflight: impl FnMut() -> Result<(), String>,
) -> Result<MacPointerPreflight, String> {
    let before_display_id = revalidate(display)?;
    let before_geometry = native_geometry(before_display_id);
    let target =
        map_macos_pointer_coordinate(display.width, display.height, before_geometry, x, y)?;

    let after_display_id = revalidate(display)?;
    let after_geometry = native_geometry(after_display_id);
    if after_display_id != before_display_id || after_geometry != before_geometry {
        return Err(
            "stale_display: macOS display placement or rotation changed during pointer preflight"
                .to_string(),
        );
    }
    validate_macos_pointer_native_geometry(after_geometry)?;
    permission_preflight()?;
    validate_macos_pointer_input_state(action, input_state())?;
    Ok(MacPointerPreflight {
        target,
        display_id: after_display_id,
        geometry: after_geometry,
    })
}

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_plan(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<MacPointerPreflight, String> {
    prepare_macos_pointer_plan_with(
        display,
        x,
        y,
        action,
        find_exact_macos_display,
        macos_pointer_native_geometry,
        macos_pointer_input_state,
        validate_macos_pointer_permission,
    )
}

#[cfg(target_os = "macos")]
type MacPreparedPointerEvents = (
    CFRetained<CGEventSource>,
    CFRetained<CGEvent>,
    Option<CFRetained<CGEvent>>,
    Option<CFRetained<CGEvent>>,
);

#[cfg(target_os = "macos")]
fn prepare_macos_pointer_events(
    action: PointerAction,
    target: MacPointerPlan,
) -> Result<MacPreparedPointerEvents, String> {
    let source =
        CGEventSource::new(CGEventSourceStateID::CombinedSessionState).ok_or_else(|| {
            "pointer_input_failed: macOS CombinedSessionState event source could not be created"
                .to_string()
        })?;
    let point = CGPoint {
        x: target.target_x,
        y: target.target_y,
    };
    let move_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::MouseMoved,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS MouseMoved event could not be created".to_string()
    })?;
    let move_location = CGEvent::location(Some(&move_event));
    if move_location.x != target.target_x || move_location.y != target.target_y {
        return Err(
            "pointer_input_failed: macOS MouseMoved event did not preserve the exact target"
                .to_string(),
        );
    }

    if action == PointerAction::Move {
        return Ok((source, move_event, None, None));
    }

    let down_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS LeftMouseDown event could not be created".to_string()
    })?;
    let up_event = CGEvent::new_mouse_event(
        Some(&source),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    )
    .ok_or_else(|| {
        "pointer_input_failed: macOS LeftMouseUp event could not be created".to_string()
    })?;
    for event in [&down_event, &up_event] {
        let location = CGEvent::location(Some(event));
        if location.x != target.target_x || location.y != target.target_y {
            return Err(
                "pointer_input_failed: macOS click event did not preserve the exact target"
                    .to_string(),
            );
        }
    }
    Ok((source, move_event, Some(down_event), Some(up_event)))
}

#[cfg(target_os = "macos")]
fn pointer_plan_native_geometry(plan: &PointerPlan) -> MacPointerNativeGeometry {
    MacPointerNativeGeometry {
        origin_x: plan.bounds_origin_x,
        origin_y: plan.bounds_origin_y,
        width: plan.bounds_width,
        height: plan.bounds_height,
        rotation_degrees: plan.rotation_degrees,
    }
}

#[cfg(target_os = "macos")]
fn macos_pointer_final_preflight(plan: &PointerPlan, action: PointerAction) -> Result<(), String> {
    let result = (|| {
        let display_id = find_exact_macos_display(&plan.display)?;
        if display_id != plan.native_display_id {
            return Err("stale_display: macOS display id changed before native post".to_string());
        }
        let geometry = macos_pointer_native_geometry(display_id);
        if geometry != pointer_plan_native_geometry(plan) {
            return Err(
                "stale_display: macOS display placement or rotation changed before native post"
                    .to_string(),
            );
        }
        validate_macos_pointer_native_geometry(geometry)?;
        if geometry.rotation_degrees != 0.0 {
            return Err(
                "pointer_input_failed: macOS pointer mapping supports only an exact 0-degree display rotation"
                    .to_string(),
            );
        }
        validate_macos_pointer_permission()?;
        validate_macos_pointer_input_state(action, macos_pointer_input_state())?;
        Ok(())
    })();
    result.map_err(|error: String| {
        format!(
            "not_started: macOS pointer final preflight failed after generation spend but before native event post: {error}"
        )
    })
}

#[cfg(target_os = "macos")]
fn macos_current_pointer_location() -> Result<(f64, f64), String> {
    let event = CGEvent::new(None).ok_or_else(|| {
        "pointer_input_failed: macOS current cursor event could not be created".to_string()
    })?;
    let location = CGEvent::location(Some(&event));
    if !location.x.is_finite() || !location.y.is_finite() {
        return Err(
            "pointer_input_failed: macOS current cursor location is non-finite".to_string(),
        );
    }
    Ok((location.x, location.y))
}

#[cfg(target_os = "macos")]
fn settle_macos_pointer_exact_observation_with(
    target_x: f64,
    target_y: f64,
    timeout: Duration,
    poll_interval: Duration,
    mut cursor_readback: impl FnMut() -> Result<(f64, f64), String>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<(), String> {
    if timeout.is_zero() || poll_interval.is_zero() {
        return Err(
            "pointer_input_failed: macOS cursor readback settle bounds are invalid".to_string(),
        );
    }
    let deadline = now() + timeout;
    let mut first_observation = true;
    loop {
        if !first_observation && now() >= deadline {
            return Err(
                "pointer_input_failed: macOS exact cursor target was not observed before bounded readback settle deadline"
                    .to_string(),
            );
        }
        first_observation = false;
        if cursor_readback().is_ok_and(|cursor| cursor == (target_x, target_y)) {
            return Ok(());
        }
        let observed_at = now();
        if observed_at >= deadline {
            return Err(
                "pointer_input_failed: macOS exact cursor target was not observed before bounded readback settle deadline"
                    .to_string(),
            );
        }
        sleep(poll_interval.min(deadline.saturating_duration_since(observed_at)));
    }
}

#[cfg(target_os = "macos")]
fn settle_macos_pointer_exact_observation(target_x: f64, target_y: f64) -> Result<(), String> {
    settle_macos_pointer_exact_observation_with(
        target_x,
        target_y,
        MACOS_POINTER_READBACK_SETTLE_TIMEOUT,
        MACOS_POINTER_READBACK_POLL_INTERVAL,
        macos_current_pointer_location,
        Instant::now,
        std::thread::sleep,
    )
}

#[cfg(target_os = "macos")]
fn macos_pointer_outcome_unknown(message: &str) -> String {
    format!("outcome_unknown: {message}")
}

#[cfg(target_os = "macos")]
fn dispatch_macos_pointer_with(
    action: PointerAction,
    target_x: f64,
    target_y: f64,
    mut final_preflight: impl FnMut() -> Result<(), String>,
    mut post_move: impl FnMut() -> Result<(), String>,
    mut exact_cursor_observation: impl FnMut(f64, f64) -> Result<(), String>,
    mut second_click_state: impl FnMut() -> Result<(), String>,
    mut post_down: impl FnMut() -> Result<(), String>,
    mut post_up: impl FnMut() -> Result<(), String>,
    mut left_button_down: impl FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    final_preflight().map_err(|error| {
        if error.starts_with("not_started:") {
            error
        } else {
            format!(
                "not_started: macOS pointer final preflight failed after generation spend but before native event post: {error}"
            )
        }
    })?;

    post_move().map_err(|_| {
        macos_pointer_outcome_unknown("macOS MouseMoved post outcome could not be proven")
    })?;
    exact_cursor_observation(target_x, target_y).map_err(|_| {
        macos_pointer_outcome_unknown(
            "macOS bounded cursor observation did not prove the exact target after MouseMoved",
        )
    })?;
    if action == PointerAction::Move {
        return Ok(true);
    }

    second_click_state().map_err(|_| {
        macos_pointer_outcome_unknown(
            "shared desktop input state changed after the exact pointer move; click button events were not attempted",
        )
    })?;
    post_down().map_err(|_| {
        macos_pointer_outcome_unknown("macOS LeftMouseDown post outcome could not be proven")
    })?;
    post_up().map_err(|_| {
        macos_pointer_outcome_unknown("macOS LeftMouseUp post outcome could not be proven")
    })?;

    exact_cursor_observation(target_x, target_y).map_err(|_| {
        macos_pointer_outcome_unknown(
            "macOS bounded final cursor observation did not prove the exact click target",
        )
    })?;
    if left_button_down().map_err(|_| {
        macos_pointer_outcome_unknown("macOS final left-button readback is unavailable")
    })? {
        return Err(macos_pointer_outcome_unknown(
            "macOS left mouse button remained down after click sequence",
        ));
    }
    Ok(true)
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

#[cfg(target_os = "macos")]
fn capture_revalidated_macos_display<T>(
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
fn macos_cg_image_to_rgba(
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

#[cfg(all(test, target_os = "macos"))]
mod macos_pointer_tests {
    use super::*;
    use std::cell::Cell;

    fn geometry(origin_x: f64, origin_y: f64, width: f64, height: f64) -> MacPointerNativeGeometry {
        MacPointerNativeGeometry {
            origin_x,
            origin_y,
            width,
            height,
            rotation_degrees: 0.0,
        }
    }

    fn display(width: u32, height: u32) -> DisplayRecord {
        DisplayRecord {
            native_identity: vec![1],
            width,
            height,
            primary: true,
        }
    }

    fn clean_input_state() -> MacPointerInputState {
        MacPointerInputState {
            buttons_down: 0,
            modifier_flags: CGEventFlags::empty(),
        }
    }

    #[test]
    fn macos_pointer_mapping_handles_1x_hidpi_origins_and_exact_edges() {
        let one_x = geometry(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, one_x, 0, 0).unwrap(),
            MacPointerPlan {
                target_x: 0.0,
                target_y: 0.0,
            }
        );
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, one_x, 1919, 1079).unwrap(),
            MacPointerPlan {
                target_x: 1919.0,
                target_y: 1079.0,
            }
        );

        let hidpi = geometry(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(3840, 2160, hidpi, 3839, 2159).unwrap(),
            MacPointerPlan {
                target_x: 1919.5,
                target_y: 1079.5,
            }
        );

        let negative = geometry(-1920.0, -120.0, 1920.0, 1080.0);
        assert_eq!(
            map_macos_pointer_coordinate(1920, 1080, negative, 1919, 1079).unwrap(),
            MacPointerPlan {
                target_x: -1.0,
                target_y: 959.0,
            }
        );

        let positive = geometry(1920.0, 120.0, 2560.0, 1440.0);
        assert_eq!(
            map_macos_pointer_coordinate(5120, 2880, positive, 5119, 2879).unwrap(),
            MacPointerPlan {
                target_x: 4479.5,
                target_y: 1559.5,
            }
        );
    }

    #[test]
    fn macos_pointer_mapping_rejects_source_edges_invalid_bounds_and_rotation() {
        let valid = geometry(0.0, 0.0, 1920.0, 1080.0);
        for (x, y) in [(1920, 0), (0, 1080)] {
            let error = map_macos_pointer_coordinate(1920, 1080, valid, x, y).unwrap_err();
            assert!(error.starts_with("invalid_request:"), "{error}");
        }

        for invalid in [
            geometry(0.0, 0.0, 0.0, 1080.0),
            geometry(0.0, 0.0, -1.0, 1080.0),
            geometry(f64::NAN, 0.0, 1920.0, 1080.0),
            geometry(0.0, 0.0, f64::INFINITY, 1080.0),
        ] {
            let error = map_macos_pointer_coordinate(1920, 1080, invalid, 0, 0).unwrap_err();
            assert!(error.starts_with("pointer_input_failed:"), "{error}");
        }

        let mut rotated = valid;
        rotated.rotation_degrees = 90.0;
        let error = map_macos_pointer_coordinate(1920, 1080, rotated, 0, 0).unwrap_err();
        assert!(error.contains("0-degree"), "{error}");

        let mut invalid_rotation = valid;
        invalid_rotation.rotation_degrees = f64::NAN;
        let error = map_macos_pointer_coordinate(1920, 1080, invalid_rotation, 0, 0).unwrap_err();
        assert!(error.contains("non-finite"), "{error}");
    }

    #[test]
    fn macos_pointer_preflight_revalidates_bounds_rotation_and_existing_display_fence() {
        let display = display(3840, 2160);
        for changed in [
            geometry(0.0, 0.0, 1919.0, 1080.0),
            MacPointerNativeGeometry {
                rotation_degrees: 90.0,
                ..geometry(0.0, 0.0, 1920.0, 1080.0)
            },
        ] {
            let observations = Cell::new(0usize);
            let error = prepare_macos_pointer_plan_with(
                &display,
                100,
                100,
                PointerAction::Move,
                |_| Ok(7),
                |_| {
                    let call = observations.get();
                    observations.set(call + 1);
                    if call == 0 {
                        geometry(0.0, 0.0, 1920.0, 1080.0)
                    } else {
                        changed
                    }
                },
                clean_input_state,
                || Ok(()),
            )
            .expect_err("placement or rotation changes must stale the display");
            assert!(error.starts_with("stale_display:"), "{error}");
            assert_eq!(observations.get(), 2);
        }

        let validations = Cell::new(0usize);
        let error = prepare_macos_pointer_plan_with(
            &display,
            100,
            100,
            PointerAction::Move,
            |_| {
                let call = validations.get();
                validations.set(call + 1);
                if call == 0 {
                    Ok(7)
                } else {
                    Err("stale_display: simulated M3 identity/source geometry change".to_string())
                }
            },
            |_| geometry(0.0, 0.0, 1920.0, 1080.0),
            clean_input_state,
            || Ok(()),
        )
        .expect_err("existing exact display fence must remain authoritative");
        assert!(error.starts_with("stale_display:"), "{error}");
        assert_eq!(validations.get(), 2);
    }

    #[test]
    fn macos_pointer_shared_input_preflight_distinguishes_move_and_click() {
        let button_down = MacPointerInputState {
            buttons_down: 1 << 17,
            modifier_flags: CGEventFlags::empty(),
        };
        for action in [PointerAction::Move, PointerAction::Click] {
            let error = validate_macos_pointer_input_state(action, button_down).unwrap_err();
            assert!(error.contains("mouse button"), "{error}");
        }

        for modifier in [
            CGEventFlags::MaskShift,
            CGEventFlags::MaskControl,
            CGEventFlags::MaskAlternate,
            CGEventFlags::MaskCommand,
        ] {
            let state = MacPointerInputState {
                buttons_down: 0,
                modifier_flags: modifier,
            };
            validate_macos_pointer_input_state(PointerAction::Move, state)
                .expect("ordinary modifiers do not widen move policy into click policy");
            let error =
                validate_macos_pointer_input_state(PointerAction::Click, state).unwrap_err();
            assert!(error.contains("modifier"), "{error}");
        }
    }

    #[test]
    fn macos_pointer_permission_denial_is_definite_pre_effect() {
        let display = display(3840, 2160);
        let error = prepare_macos_pointer_plan_with(
            &display,
            0,
            0,
            PointerAction::Move,
            |_| Ok(7),
            |_| geometry(0.0, 0.0, 1920.0, 1080.0),
            clean_input_state,
            || Err("permission_denied: macOS event-posting permission is not granted".to_string()),
        )
        .expect_err("permission failure must stay before the generation spend boundary");
        assert!(error.starts_with("permission_denied:"), "{error}");
    }

    #[test]
    fn macos_pointer_event_construction_is_exact_and_non_effecting() {
        let target = MacPointerPlan {
            target_x: 1919.5,
            target_y: 1079.5,
        };
        let (source, move_event, down, up) =
            prepare_macos_pointer_events(PointerAction::Move, target).unwrap();
        assert_eq!(
            CGEventSource::source_state_id(Some(&source)),
            CGEventSourceStateID::CombinedSessionState
        );
        assert_eq!(CGEvent::r#type(Some(&move_event)), CGEventType::MouseMoved);
        assert!(down.is_none());
        assert!(up.is_none());
        let location = CGEvent::location(Some(&move_event));
        assert_eq!((location.x, location.y), (target.target_x, target.target_y));

        let (source, move_event, down, up) =
            prepare_macos_pointer_events(PointerAction::Click, target).unwrap();
        assert_eq!(
            CGEventSource::source_state_id(Some(&source)),
            CGEventSourceStateID::CombinedSessionState
        );
        let down = down.expect("click prepares exactly one left-down event");
        let up = up.expect("click prepares exactly one left-up event");
        assert_eq!(CGEvent::r#type(Some(&move_event)), CGEventType::MouseMoved);
        assert_eq!(CGEvent::r#type(Some(&down)), CGEventType::LeftMouseDown);
        assert_eq!(CGEvent::r#type(Some(&up)), CGEventType::LeftMouseUp);
        for event in [&move_event, &down, &up] {
            let location = CGEvent::location(Some(event));
            assert_eq!((location.x, location.y), (target.target_x, target.target_y));
        }
    }

    #[test]
    fn macos_pointer_exact_readback_settle_is_bounded_and_strict() {
        use std::cell::Cell;

        fn run_settle(
            cursor_sequence: &[(f64, f64)],
            timeout_ms: u64,
        ) -> (Result<(), String>, usize) {
            let reads = Cell::new(0usize);
            let elapsed = Cell::new(Duration::ZERO);
            let epoch = Instant::now();
            let last_cursor = *cursor_sequence.last().expect("non-empty cursor sequence");
            let result = settle_macos_pointer_exact_observation_with(
                10.5,
                20.5,
                Duration::from_millis(timeout_ms),
                Duration::from_millis(1),
                || {
                    let read = reads.get();
                    reads.set(read + 1);
                    Ok(cursor_sequence.get(read).copied().unwrap_or(last_cursor))
                },
                || epoch + elapsed.get(),
                |duration| elapsed.set(elapsed.get() + duration),
            );
            (result, reads.get())
        }

        let (result, reads) = run_settle(&[(10.0, 20.5), (10.5, 20.5)], 5);
        result.expect("first mismatch followed by exact readback must settle");
        assert_eq!(reads, 2);

        let (result, reads) =
            run_settle(&[(10.0, 20.5), (10.0, 20.5), (10.0, 20.5), (10.5, 20.5)], 5);
        result.expect("multiple mismatches followed by exact readback must settle");
        assert_eq!(reads, 4);

        let (result, reads) = run_settle(&[(10.0, 20.5)], 3);
        let error = result.expect_err("bounded settle must expire on only mismatches");
        assert!(error.starts_with("pointer_input_failed:"), "{error}");
        assert_eq!(reads, 3);

        let (result, reads) = run_settle(&[(10.500_000_000_1, 20.5)], 3);
        let error =
            result.expect_err("nearby fractional coordinates must not satisfy exact equality");
        assert!(error.starts_with("pointer_input_failed:"), "{error}");
        assert_eq!(reads, 3);
    }

    #[test]
    fn macos_pointer_dispatch_move_preserves_effect_boundary_and_uses_settle() {
        use std::cell::RefCell;

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Err("simulated stale final fence".to_string())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |_, _| {
                trace.borrow_mut().push("cursor_proof");
                Ok(())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("final preflight must fail before any post");
        assert!(error.starts_with("not_started:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight"]);

        let trace = RefCell::new(Vec::new());
        let success = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |x, y| {
                trace.borrow_mut().push("cursor_proof");
                assert_eq!((x, y), (10.5, 20.5));
                Ok(())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect("move should succeed after exact observation proof");
        assert!(success);
        assert_eq!(*trace.borrow(), vec!["preflight", "move", "cursor_proof"]);

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Err("post interrupted".to_string())
            },
            |_, _| unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("uncertain MouseMoved post stays outcome_unknown");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight", "move"]);

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Move,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Ok(())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |x, y| {
                trace.borrow_mut().push("cursor_proof");
                assert_eq!((x, y), (10.5, 20.5));
                Err("bounded exact observation exhausted".to_string())
            },
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
            || unreachable!(),
        )
        .expect_err("settle exhaustion after MouseMoved must stay outcome_unknown");
        assert!(error.starts_with("outcome_unknown:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight", "move", "cursor_proof"]);
    }

    #[test]
    fn macos_pointer_dispatch_click_preserves_two_phase_safety_and_unknown_boundaries() {
        use std::cell::{Cell, RefCell};

        fn run_click(
            proof_sequence: &[Result<(), &'static str>],
            second_state_ok: bool,
            down_ok: bool,
            up_ok: bool,
            final_left_down: bool,
        ) -> (Result<bool, String>, Vec<&'static str>) {
            let trace = RefCell::new(Vec::new());
            let proof_reads = Cell::new(0usize);
            let last_proof = *proof_sequence.last().expect("non-empty proof sequence");
            let result = dispatch_macos_pointer_with(
                PointerAction::Click,
                10.5,
                20.5,
                || {
                    trace.borrow_mut().push("preflight");
                    Ok(())
                },
                || {
                    trace.borrow_mut().push("move");
                    Ok(())
                },
                |x, y| {
                    trace.borrow_mut().push("cursor_proof");
                    assert_eq!((x, y), (10.5, 20.5));
                    let read = proof_reads.get();
                    proof_reads.set(read + 1);
                    proof_sequence
                        .get(read)
                        .copied()
                        .unwrap_or(last_proof)
                        .map_err(str::to_string)
                },
                || {
                    trace.borrow_mut().push("second_state");
                    second_state_ok
                        .then_some(())
                        .ok_or_else(|| "dirty".to_string())
                },
                || {
                    trace.borrow_mut().push("down");
                    down_ok
                        .then_some(())
                        .ok_or_else(|| "down interrupted".to_string())
                },
                || {
                    trace.borrow_mut().push("up");
                    up_ok
                        .then_some(())
                        .ok_or_else(|| "up interrupted".to_string())
                },
                || {
                    trace.borrow_mut().push("left_button");
                    Ok(final_left_down)
                },
            );
            (result, trace.into_inner())
        }

        let trace = RefCell::new(Vec::new());
        let error = dispatch_macos_pointer_with(
            PointerAction::Click,
            10.5,
            20.5,
            || {
                trace.borrow_mut().push("preflight");
                Err("simulated final fence".to_string())
            },
            || {
                trace.borrow_mut().push("move");
                Ok(())
            },
            |_, _| Ok(()),
            || Ok(()),
            || Ok(()),
            || Ok(()),
            || Ok(false),
        )
        .expect_err("click final preflight must fail before MouseMoved");
        assert!(error.starts_with("not_started:"), "{error}");
        assert_eq!(*trace.borrow(), vec!["preflight"]);

        let (error, trace) = run_click(&[Err("move proof exhausted")], true, true, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(trace, vec!["preflight", "move", "cursor_proof"]);
        assert!(!trace.contains(&"second_state"));
        assert!(!trace.contains(&"down"));
        assert!(!trace.contains(&"up"));

        let (success, trace) = run_click(&[Ok(()), Ok(())], true, true, true, false);
        assert!(success.unwrap());
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof",
                "left_button"
            ]
        );

        let (error, trace) = run_click(&[Ok(())], false, true, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec!["preflight", "move", "cursor_proof", "second_state"]
        );

        let (error, trace) = run_click(&[Ok(())], true, false, true, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec!["preflight", "move", "cursor_proof", "second_state", "down"]
        );

        let (error, trace) = run_click(&[Ok(())], true, true, false, false);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up"
            ]
        );

        let (error, trace) = run_click(
            &[Ok(()), Err("final proof exhausted")],
            true,
            true,
            true,
            false,
        );
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof"
            ]
        );
        assert!(!trace.contains(&"left_button"));

        let (error, trace) = run_click(&[Ok(()), Ok(())], true, true, true, true);
        assert!(error.unwrap_err().starts_with("outcome_unknown:"));
        assert_eq!(
            trace,
            vec![
                "preflight",
                "move",
                "cursor_proof",
                "second_state",
                "down",
                "up",
                "cursor_proof",
                "left_button"
            ]
        );
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) struct MacPointerReadOnlyProbe {
    pub(crate) source_width: u32,
    pub(crate) source_height: u32,
    pub(crate) bounds: (f64, f64, f64, f64),
    pub(crate) rotation_degrees: f64,
    pub(crate) mapped_edge: (f64, f64),
    pub(crate) buttons_down: u32,
    pub(crate) modifier_flags: u64,
    pub(crate) event_post_permission: bool,
    pub(crate) prohibited_modifiers_active: bool,
    pub(crate) constructed_event_count: usize,
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_pointer_read_only_probe_for_test(
    display: &PlatformDisplay,
) -> Result<MacPointerReadOnlyProbe, String> {
    let record = DisplayRecord {
        native_identity: display.native_identity.clone(),
        width: display.width,
        height: display.height,
        primary: display.primary,
    };
    let observed_input = std::cell::Cell::new(None);
    let observed_permission = std::cell::Cell::new(false);
    let plan = prepare_macos_pointer_plan_with(
        &record,
        display.width.saturating_sub(1),
        display.height.saturating_sub(1),
        PointerAction::Move,
        find_exact_macos_display,
        macos_pointer_native_geometry,
        || {
            let state = macos_pointer_input_state();
            observed_input.set(Some(state));
            state
        },
        || {
            let granted = CGPreflightPostEventAccess();
            observed_permission.set(granted);
            if granted {
                Ok(())
            } else {
                Err("permission_denied: macOS event-posting permission is not granted".to_string())
            }
        },
    )?;
    let display_id = find_exact_macos_display(&record)?;
    let native = macos_pointer_native_geometry(display_id);
    validate_macos_pointer_native_geometry(native)?;
    let input = observed_input.get().ok_or_else(|| {
        "pointer_input_failed: macOS pointer input state was not observed".to_string()
    })?;
    let prohibited_modifiers_active = input.modifier_flags.intersects(
        CGEventFlags::MaskShift
            | CGEventFlags::MaskControl
            | CGEventFlags::MaskAlternate
            | CGEventFlags::MaskCommand,
    );
    let (_source, move_event, down_event, up_event) =
        prepare_macos_pointer_events(PointerAction::Click, plan.target)?;
    let constructed_event_count =
        1 + usize::from(down_event.is_some()) + usize::from(up_event.is_some());
    for event in [
        Some(move_event.as_ref()),
        down_event.as_deref(),
        up_event.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let location = CGEvent::location(Some(event));
        if (location.x, location.y) != (plan.target.target_x, plan.target.target_y) {
            return Err(
                "pointer_input_failed: macOS prepared event target did not survive readback"
                    .to_string(),
            );
        }
    }
    Ok(MacPointerReadOnlyProbe {
        source_width: display.width,
        source_height: display.height,
        bounds: (
            native.origin_x,
            native.origin_y,
            native.width,
            native.height,
        ),
        rotation_degrees: native.rotation_degrees,
        mapped_edge: (plan.target.target_x, plan.target.target_y),
        buttons_down: input.buttons_down,
        modifier_flags: input.modifier_flags.bits(),
        event_post_permission: observed_permission.get(),
        prohibited_modifiers_active,
        constructed_event_count,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn prepare_pointer(
    display: &DisplayRecord,
    x: u32,
    y: u32,
    action: PointerAction,
) -> Result<PointerPlan, String> {
    let preflight = prepare_macos_pointer_plan(display, x, y, action)?;
    let (source, move_event, click_down_event, click_up_event) =
        prepare_macos_pointer_events(action, preflight.target)?;
    Ok(PointerPlan {
        display: display.clone(),
        native_display_id: preflight.display_id,
        bounds_origin_x: preflight.geometry.origin_x,
        bounds_origin_y: preflight.geometry.origin_y,
        bounds_width: preflight.geometry.width,
        bounds_height: preflight.geometry.height,
        rotation_degrees: preflight.geometry.rotation_degrees,
        target_x: preflight.target.target_x,
        target_y: preflight.target.target_y,
        _source: source,
        move_event,
        click_down_event,
        click_up_event,
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn dispatch_pointer(plan: PointerPlan, action: PointerAction) -> Result<bool, String> {
    let down_event = plan.click_down_event.as_deref();
    let up_event = plan.click_up_event.as_deref();
    if action == PointerAction::Click && (down_event.is_none() || up_event.is_none()) {
        return Err(
            "not_started: macOS click plan is incomplete after generation spend but before native event post"
                .to_string(),
        );
    }
    dispatch_macos_pointer_with(
        action,
        plan.target_x,
        plan.target_y,
        || macos_pointer_final_preflight(&plan, action),
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&plan.move_event));
            Ok(())
        },
        settle_macos_pointer_exact_observation,
        || validate_macos_pointer_input_state(PointerAction::Click, macos_pointer_input_state()),
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, down_event);
            Ok(())
        },
        || {
            CGEvent::post(CGEventTapLocation::HIDEventTap, up_event);
            Ok(())
        },
        || {
            Ok(CGEventSource::button_state(
                CGEventSourceStateID::CombinedSessionState,
                CGMouseButton::Left,
            ))
        },
    )
}
#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MacApplicationFileIdentity {
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MacApplicationIdentity {
    canonical_path: String,
    bundle_identifier: String,
    bundle_display_name: String,
    bundle_executable: String,
    bundle: MacApplicationFileIdentity,
    info_plist: MacApplicationFileIdentity,
    executable: MacApplicationFileIdentity,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MacApplicationLaunchCompletion {
    Success,
    Failed,
    Ambiguous,
}

#[cfg(target_os = "macos")]
fn mac_application_file_identity(metadata: &fs::Metadata) -> MacApplicationFileIdentity {
    MacApplicationFileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        mode: metadata.mode(),
        links: metadata.nlink(),
        size: metadata.size(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
    }
}

#[cfg(target_os = "macos")]
fn bounded_ns_string(value: &NSString, max_bytes: usize) -> Option<String> {
    if value.is_empty() || value.len() > max_bytes {
        return None;
    }
    let value = value.to_string();
    (!value.contains('\0')).then_some(value)
}

#[cfg(target_os = "macos")]
fn bounded_bundle_string(bundle: &NSBundle, key: &str, max_bytes: usize) -> Option<String> {
    let key = NSString::from_str(key);
    let value = bundle.objectForInfoDictionaryKey(&key)?;
    let value = value.downcast::<NSString>().ok()?;
    bounded_ns_string(&value, max_bytes)
}

#[cfg(target_os = "macos")]
fn path_from_file_url(url: &NSURL) -> Option<PathBuf> {
    let path = url.path()?;
    if path.is_empty() || path.len() > MAX_MACOS_APPLICATION_PATH_BYTES {
        return None;
    }
    let path = path.to_string();
    (!path.contains('\0')).then(|| PathBuf::from(path))
}

#[cfg(target_os = "macos")]
fn normalize_macos_application_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots = roots
        .into_iter()
        .filter_map(|root| {
            let canonical = fs::canonicalize(root).ok()?;
            let metadata = fs::symlink_metadata(&canonical).ok()?;
            metadata.is_dir().then_some(canonical)
        })
        .collect::<Vec<_>>();
    roots.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut non_overlapping = Vec::<PathBuf>::with_capacity(roots.len());
    for root in roots {
        if non_overlapping
            .iter()
            .any(|parent| root.starts_with(parent))
        {
            continue;
        }
        non_overlapping.push(root);
    }
    non_overlapping.sort();
    non_overlapping
}

#[cfg(target_os = "macos")]
fn macos_application_roots() -> Vec<PathBuf> {
    let manager = NSFileManager::defaultManager();
    let domains = NSSearchPathDomainMask::UserDomainMask
        | NSSearchPathDomainMask::LocalDomainMask
        | NSSearchPathDomainMask::SystemDomainMask;
    let mut roots = Vec::new();
    for directory in [
        NSSearchPathDirectory::ApplicationDirectory,
        NSSearchPathDirectory::AdminApplicationDirectory,
    ] {
        let urls = manager.URLsForDirectory_inDomains(directory, domains);
        roots.extend(urls.iter().filter_map(|url| path_from_file_url(&url)));
    }
    normalize_macos_application_roots(roots)
}

#[cfg(target_os = "macos")]
fn path_is_symlink_free_under_root(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = fs::symlink_metadata(&current) else {
            return false;
        };
        if metadata.file_type().is_symlink() {
            return false;
        }
    }
    true
}

#[cfg(target_os = "macos")]
fn mac_application_candidate_at_path(
    path: &Path,
    allowed_roots: &[PathBuf],
) -> Option<PlatformApplication> {
    if !path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    {
        return None;
    }
    let root = allowed_roots
        .iter()
        .find(|root| path.starts_with(root) && path_is_symlink_free_under_root(path, root))?;
    let canonical_path = fs::canonicalize(path).ok()?;
    if canonical_path != path || !canonical_path.starts_with(root) {
        return None;
    }
    let bundle_metadata = fs::symlink_metadata(&canonical_path).ok()?;
    if !bundle_metadata.is_dir() || bundle_metadata.file_type().is_symlink() {
        return None;
    }

    let canonical_path_text = canonical_path.to_str()?;
    if canonical_path_text.is_empty()
        || canonical_path_text.len() > MAX_MACOS_APPLICATION_PATH_BYTES
        || canonical_path_text.contains('\0')
    {
        return None;
    }
    let native_path = NSString::from_str(canonical_path_text);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_path, true);
    let bundle = NSBundle::bundleWithURL(&application_url)?;
    let bundle_url = path_from_file_url(&bundle.bundleURL())?;
    if fs::canonicalize(bundle_url).ok()? != canonical_path {
        return None;
    }

    let package_type = bounded_bundle_string(
        &bundle,
        "CFBundlePackageType",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )?;
    if package_type != "APPL" {
        return None;
    }
    let native_bundle_identifier = bundle.bundleIdentifier()?;
    let bundle_identifier =
        bounded_ns_string(&native_bundle_identifier, MAX_MACOS_BUNDLE_IDENTIFIER_BYTES)?;
    let display_name = bounded_bundle_string(
        &bundle,
        "CFBundleDisplayName",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )
    .or_else(|| bounded_bundle_string(&bundle, "CFBundleName", MAX_MACOS_BUNDLE_METADATA_BYTES))?;
    let bundle_executable = bounded_bundle_string(
        &bundle,
        "CFBundleExecutable",
        MAX_MACOS_BUNDLE_METADATA_BYTES,
    )?;

    let executable_url = bundle.executableURL()?;
    let executable_path = path_from_file_url(&executable_url)?;
    let expected_executable = canonical_path
        .join("Contents/MacOS")
        .join(&bundle_executable);
    if !path_is_symlink_free_under_root(&expected_executable, &canonical_path) {
        return None;
    }
    let canonical_executable = fs::canonicalize(&executable_path).ok()?;
    if canonical_executable != expected_executable
        || !canonical_executable.starts_with(&canonical_path)
        || canonical_executable.file_name()? != std::ffi::OsStr::new(&bundle_executable)
    {
        return None;
    }
    let executable_metadata = fs::symlink_metadata(&canonical_executable).ok()?;
    if !executable_metadata.is_file()
        || executable_metadata.file_type().is_symlink()
        || executable_metadata.mode() & 0o111 == 0
    {
        return None;
    }

    let info_plist_path = canonical_path.join("Contents/Info.plist");
    if !path_is_symlink_free_under_root(&info_plist_path, &canonical_path) {
        return None;
    }
    let canonical_info_plist = fs::canonicalize(&info_plist_path).ok()?;
    if canonical_info_plist != info_plist_path || !canonical_info_plist.starts_with(&canonical_path)
    {
        return None;
    }
    let info_plist_metadata = fs::symlink_metadata(&canonical_info_plist).ok()?;
    if !info_plist_metadata.is_file() || info_plist_metadata.file_type().is_symlink() {
        return None;
    }

    let identity = MacApplicationIdentity {
        canonical_path: canonical_path_text.to_string(),
        bundle_identifier,
        bundle_display_name: display_name.clone(),
        bundle_executable,
        bundle: mac_application_file_identity(&bundle_metadata),
        info_plist: mac_application_file_identity(&info_plist_metadata),
        executable: mac_application_file_identity(&executable_metadata),
    };
    let native_identity = serde_json::to_vec(&identity).ok()?;
    if native_identity.is_empty() || native_identity.len() > MAX_MACOS_APPLICATION_IDENTITY_BYTES {
        return None;
    }
    Some(PlatformApplication {
        display_name,
        native_identity,
    })
}

#[cfg(target_os = "macos")]
fn enumerate_macos_applications(roots: &[PathBuf], scan_limit: usize) -> Vec<PlatformApplication> {
    let roots = normalize_macos_application_roots(roots.iter().cloned());
    let mut queue = roots
        .iter()
        .cloned()
        .map(|root| (root, 0usize))
        .collect::<VecDeque<_>>();
    let mut scanned = 0usize;
    let mut applications = Vec::new();

    'scan: while let Some((directory, depth)) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            if scanned >= scan_limit {
                break 'scan;
            }
            scanned += 1;
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let is_app_bundle = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"));
            if is_app_bundle {
                // A .app bundle candidate is always a leaf, including when its
                // metadata cannot prove that it is safely launchable.
                if let Some(application) = mac_application_candidate_at_path(&path, &roots) {
                    applications.push(application);
                }
            } else if depth < MAX_MACOS_APPLICATION_SCAN_DEPTH {
                queue.push_back((path, depth + 1));
            }
        }
    }
    crate::sort_application_candidates(&mut applications);
    applications
}

#[cfg(target_os = "macos")]
pub(crate) fn list_applications(limit: usize) -> Result<Vec<PlatformApplication>, String> {
    if limit == 0 || limit > crate::MAX_APPLICATION_SCAN {
        return Err("invalid_request: macOS application scan bound is invalid".to_string());
    }
    Ok(enumerate_macos_applications(
        &macos_application_roots(),
        limit,
    ))
}

#[cfg(target_os = "macos")]
fn decode_macos_application_identity(
    native_identity: &[u8],
) -> Result<MacApplicationIdentity, String> {
    if native_identity.is_empty() || native_identity.len() > MAX_MACOS_APPLICATION_IDENTITY_BYTES {
        return Err("stale_application: macOS application identity is invalid".to_string());
    }
    let identity: MacApplicationIdentity = serde_json::from_slice(native_identity)
        .map_err(|_| "stale_application: macOS application identity is invalid".to_string())?;
    if identity.canonical_path.is_empty()
        || identity.canonical_path.len() > MAX_MACOS_APPLICATION_PATH_BYTES
        || identity.canonical_path.contains('\0')
    {
        return Err("stale_application: macOS application identity is invalid".to_string());
    }
    Ok(identity)
}

#[cfg(target_os = "macos")]
fn revalidate_macos_application_identity_with_roots(
    native_identity: &[u8],
    roots: &[PathBuf],
) -> Result<String, String> {
    let identity = decode_macos_application_identity(native_identity)?;
    let roots = normalize_macos_application_roots(roots.iter().cloned());
    let path = PathBuf::from(&identity.canonical_path);
    let fresh = mac_application_candidate_at_path(&path, &roots)
        .ok_or_else(|| "stale_application: macOS application changed or disappeared".to_string())?;
    if fresh.native_identity != native_identity {
        return Err("stale_application: macOS application identity changed".to_string());
    }
    Ok(identity.canonical_path)
}

#[cfg(target_os = "macos")]
fn configure_macos_application_launch(configuration: &NSWorkspaceOpenConfiguration) {
    configuration.setPromptsUserIfNeeded(false);
    configuration.setAddsToRecentItems(false);
    configuration.setActivates(false);
    configuration.setHides(false);
    configuration.setHidesOthers(false);
    configuration.setForPrinting(false);
    configuration.setCreatesNewApplicationInstance(false);
    configuration.setAllowsRunningApplicationSubstitution(false);
    configuration.setRequiresUniversalLinks(false);
    configuration.setAppleEvent(None);
    let arguments = NSArray::<NSString>::from_slice(&[]);
    let environment = NSDictionary::<NSString, NSString>::from_slices::<NSString>(&[], &[]);
    configuration.setArguments(&arguments);
    configuration.setEnvironment(&environment);
}

#[cfg(target_os = "macos")]
fn classify_macos_application_launch_completion(
    has_application: bool,
    has_error: bool,
) -> MacApplicationLaunchCompletion {
    match (has_application, has_error) {
        (true, false) => MacApplicationLaunchCompletion::Success,
        (false, true) => MacApplicationLaunchCompletion::Failed,
        _ => MacApplicationLaunchCompletion::Ambiguous,
    }
}

#[cfg(target_os = "macos")]
fn macos_application_launch_completion_channel() -> (
    Receiver<MacApplicationLaunchCompletion>,
    RcBlock<dyn Fn(*mut NSRunningApplication, *mut NSError)>,
) {
    let (sender, receiver) = mpsc::sync_channel(1);
    let completion: RcBlock<dyn Fn(*mut NSRunningApplication, *mut NSError)> = RcBlock::new(
        move |running_application: *mut NSRunningApplication, error: *mut NSError| {
            let completion = classify_macos_application_launch_completion(
                !running_application.is_null(),
                !error.is_null(),
            );
            let _ = sender.try_send(completion);
        },
    );
    (receiver, completion)
}

#[cfg(target_os = "macos")]
fn dispatch_revalidated_macos_application<T>(
    native_identity: &[u8],
    roots: &[PathBuf],
    prepared_path: &str,
    dispatch: impl FnOnce() -> T,
) -> Result<T, String> {
    let application_path =
        revalidate_macos_application_identity_with_roots(native_identity, roots)?;
    if application_path != prepared_path {
        return Err("stale_application: macOS application launch path changed".to_string());
    }
    Ok(dispatch())
}

#[cfg(target_os = "macos")]
fn wait_for_macos_application_launch(
    completion: Receiver<MacApplicationLaunchCompletion>,
    wait: Duration,
) -> Result<(), String> {
    match completion.recv_timeout(wait) {
        Ok(MacApplicationLaunchCompletion::Success) => Ok(()),
        Ok(MacApplicationLaunchCompletion::Failed) => Err(
            "outcome_unknown: macOS application launch reported failure after native dispatch"
                .to_string(),
        ),
        Ok(MacApplicationLaunchCompletion::Ambiguous) => Err(
            "outcome_unknown: macOS application launch returned ambiguous completion metadata"
                .to_string(),
        ),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(
            "outcome_unknown: macOS application launch completion deadline exceeded after native dispatch"
                .to_string(),
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(
            "outcome_unknown: macOS application launch completion was lost after native dispatch"
                .to_string(),
        ),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn launch_application(
    application_id: &str,
    application: &ApplicationRecord,
) -> Result<Value, String> {
    // Prepare every launch object that does not depend on the current target identity
    // before the final filesystem/bundle proof. This keeps same-path replacement
    // failures definite pre-effect instead of widening the gap before NSWorkspace dispatch.
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    let workspace = NSWorkspace::sharedWorkspace();
    let (receiver, completion) = macos_application_launch_completion_channel();

    // The stored path may be used to prepare the immutable NSURL, but it does not
    // authorize launch. Full bundle/filesystem identity is revalidated again after
    // this target-dependent object preparation and immediately before native dispatch.
    let prepared_identity = decode_macos_application_identity(&application.native_identity)?;
    let native_application_path = NSString::from_str(&prepared_identity.canonical_path);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_application_path, true);
    dispatch_revalidated_macos_application(
        &application.native_identity,
        &macos_application_roots(),
        &prepared_identity.canonical_path,
        || {
            // This asynchronous call is the native effect boundary. It is submitted
            // exactly once; every timeout, callback loss, or non-success completion is
            // unknown and never causes a second launch request.
            workspace.openApplicationAtURL_configuration_completionHandler(
                &application_url,
                &configuration,
                Some(&completion),
            );
        },
    )?;
    wait_for_macos_application_launch(receiver, MACOS_APPLICATION_LAUNCH_WAIT)?;
    Ok(json!({
        "platform": "macos",
        "application_id": application_id,
        "success": true,
    }))
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn application_identity_revalidates_for_test(
    native_identity: &[u8],
) -> Result<(), String> {
    revalidate_macos_application_identity_with_roots(native_identity, &macos_application_roots())
        .map(drop)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_applications_in_roots_for_test(
    roots: &[PathBuf],
    scan_limit: usize,
) -> Vec<PlatformApplication> {
    enumerate_macos_applications(roots, scan_limit)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_identity_revalidates_in_roots_for_test(
    native_identity: &[u8],
    roots: &[PathBuf],
) -> Result<(), String> {
    revalidate_macos_application_identity_with_roots(native_identity, roots).map(drop)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_preparation_race_for_test(
    native_identity: &[u8],
    roots: &[PathBuf],
    between_preparation_and_dispatch: impl FnOnce(),
) -> (Result<(), String>, usize) {
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    let workspace = NSWorkspace::sharedWorkspace();
    let (receiver, completion) = macos_application_launch_completion_channel();
    let prepared_identity = match decode_macos_application_identity(native_identity) {
        Ok(identity) => identity,
        Err(error) => return (Err(error), 0),
    };
    let native_application_path = NSString::from_str(&prepared_identity.canonical_path);
    let application_url = NSURL::fileURLWithPath_isDirectory(&native_application_path, true);

    between_preparation_and_dispatch();

    let mut dispatch_attempts = 0usize;
    let result = dispatch_revalidated_macos_application(
        native_identity,
        roots,
        &prepared_identity.canonical_path,
        || {
            dispatch_attempts += 1;
            let _prepared_objects = (
                &configuration,
                &workspace,
                &receiver,
                &completion,
                &application_url,
            );
        },
    );
    (result, dispatch_attempts)
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_configuration_for_test() -> bool {
    let configuration = NSWorkspaceOpenConfiguration::configuration();
    configure_macos_application_launch(&configuration);
    !configuration.promptsUserIfNeeded()
        && !configuration.addsToRecentItems()
        && !configuration.activates()
        && !configuration.hides()
        && !configuration.hidesOthers()
        && !configuration.isForPrinting()
        && !configuration.createsNewApplicationInstance()
        && !configuration.allowsRunningApplicationSubstitution()
        && !configuration.requiresUniversalLinks()
        && configuration.appleEvent().is_none()
        && configuration.arguments().len() == 0
        && configuration.environment().len() == 0
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_completion_for_test(
    has_application: bool,
    has_error: bool,
) -> &'static str {
    match classify_macos_application_launch_completion(has_application, has_error) {
        MacApplicationLaunchCompletion::Success => "success",
        MacApplicationLaunchCompletion::Failed => "outcome_unknown",
        MacApplicationLaunchCompletion::Ambiguous => "outcome_unknown",
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) fn macos_application_launch_lost_completion_for_test() -> (bool, bool) {
    let (disconnected_sender, disconnected_receiver) = mpsc::sync_channel(1);
    drop(disconnected_sender);
    let disconnected = wait_for_macos_application_launch(disconnected_receiver, Duration::ZERO)
        .is_err_and(|error| error.starts_with("outcome_unknown:"));

    let (_pending_sender, pending_receiver) = mpsc::sync_channel(1);
    let timed_out = wait_for_macos_application_launch(pending_receiver, Duration::ZERO)
        .is_err_and(|error| error.starts_with("outcome_unknown:"));
    (disconnected, timed_out)
}

#[cfg(target_os = "macos")]
pub(crate) fn ensure_capture_permission() -> Result<(), String> {
    let granted = objc2_core_graphics::CGPreflightScreenCaptureAccess();
    if granted {
        Ok(())
    } else {
        Err("permission_denied: macOS Screen Recording permission is not granted".to_string())
    }
}

#[cfg(target_os = "macos")]
const MAX_AX_WINDOWS: usize = 64;
#[cfg(target_os = "macos")]
const MAX_AX_CHILD_COUNT: usize = 1_000_000;
#[cfg(target_os = "macos")]
const MAX_AX_ACTION_NAMES: usize = 64;

#[cfg(target_os = "macos")]
fn accessibility_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else {
        format!(
            "accessibility_failed: {operation} failed with AXError({})",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn checked_surface_pid(surface: &SurfaceRecord) -> Result<libc::pid_t, String> {
    libc::pid_t::try_from(surface.pid)
        .map_err(|_| "stale_surface: surface PID exceeds native range".to_string())
}

#[cfg(target_os = "macos")]
fn key_code(key: &str) -> Result<CGKeyCode, String> {
    match key {
        "enter" => Ok(0x24),
        "tab" => Ok(0x30),
        "escape" => Ok(0x35),
        "home" => Ok(0x73),
        "page_up" => Ok(0x74),
        "end" => Ok(0x77),
        "page_down" => Ok(0x79),
        "arrow_left" => Ok(0x7b),
        "arrow_right" => Ok(0x7c),
        "arrow_down" => Ok(0x7d),
        "arrow_up" => Ok(0x7e),
        _ => Err("invalid_request: computer key is outside the closed vocabulary".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn key_modifier_flags(modifiers: &[String]) -> Result<CGEventFlags, String> {
    validate_key_modifiers(modifiers)?;
    let mut flags = CGEventFlags::empty();
    for modifier in modifiers {
        flags |= match modifier.as_str() {
            "shift" => CGEventFlags::MaskShift,
            "control" => CGEventFlags::MaskControl,
            "option" => CGEventFlags::MaskAlternate,
            "command" => CGEventFlags::MaskCommand,
            _ => unreachable!("validate_key_input closed modifier vocabulary"),
        };
    }
    Ok(flags)
}

#[cfg(all(test, target_os = "macos"))]
mod key_input_native_contract_tests {
    use super::*;

    #[test]
    fn closed_key_codes_and_modifier_flags_are_stable() {
        for (key, expected) in [
            ("enter", 0x24),
            ("tab", 0x30),
            ("escape", 0x35),
            ("home", 0x73),
            ("page_up", 0x74),
            ("end", 0x77),
            ("page_down", 0x79),
            ("arrow_left", 0x7b),
            ("arrow_right", 0x7c),
            ("arrow_down", 0x7d),
            ("arrow_up", 0x7e),
        ] {
            assert_eq!(key_code(key).unwrap(), expected, "{key}");
        }
        assert!(key_code("a").is_err());

        let flags = key_modifier_flags(&["shift".to_string(), "command".to_string()]).unwrap();
        assert!(flags.contains(CGEventFlags::MaskShift));
        assert!(flags.contains(CGEventFlags::MaskCommand));
        assert!(!flags.contains(CGEventFlags::MaskAlternate));

        let mut surface = SurfaceRecord {
            native_id: 1,
            pid: 1,
            identity_hash: [0; 32],
            application: "test".to_string(),
            title: "test".to_string(),
            width: 1,
            height: 1,
        };
        assert_eq!(checked_surface_pid(&surface).unwrap(), 1);
        surface.pid = u32::MAX;
        assert!(checked_surface_pid(&surface)
            .unwrap_err()
            .starts_with("stale_surface:"));
    }
}

#[cfg(target_os = "macos")]
fn control_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "control_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn scroll_attempt_error(operation: &str, error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::ActionUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "scroll_failed: {operation} was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after the native action was attempted",
            error.0
        )
    }
}

#[cfg(all(test, target_os = "macos"))]
mod scroll_attempt_tests {
    use super::*;

    #[test]
    fn rejected_scroll_action_is_definite_but_unclassified_native_error_is_unknown() {
        let rejected = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::ActionUnsupported,
        );
        assert!(rejected.starts_with("scroll_failed:"), "{rejected}");

        let uncertain = scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            AXError::NoValue,
        );
        assert!(uncertain.starts_with("outcome_unknown:"), "{uncertain}");
    }
}

#[cfg(target_os = "macos")]
fn window_activation_attempt_error(
    operation: &str,
    error: AXError,
    prior_effect_succeeded: bool,
) -> String {
    if prior_effect_succeeded {
        format!(
            "outcome_unknown: {operation} returned AXError({}) after application activation had already succeeded",
            error.0
        )
    } else {
        control_attempt_error(operation, error)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod window_activation_tests {
    use super::*;

    #[test]
    fn partial_window_activation_failure_is_always_outcome_unknown() {
        let partial = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            true,
        );
        assert!(partial.starts_with("outcome_unknown:"), "{partial}");

        let not_started = window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            AXError::ActionUnsupported,
            false,
        );
        assert!(not_started.starts_with("control_failed:"), "{not_started}");
    }
}

#[cfg(target_os = "macos")]
fn text_input_attempt_error(error: AXError) -> String {
    if error == AXError::APIDisabled {
        "permission_denied: macOS Accessibility permission is not granted".to_string()
    } else if matches!(
        error,
        AXError::IllegalArgument
            | AXError::InvalidUIElement
            | AXError::AttributeUnsupported
            | AXError::NotImplemented
    ) {
        format!(
            "input_failed: AXUIElementSetAttributeValue(AXValue) was rejected with AXError({})",
            error.0
        )
    } else {
        format!(
            "outcome_unknown: AXUIElementSetAttributeValue(AXValue) returned AXError({}) after the native text write was attempted",
            error.0
        )
    }
}

#[cfg(target_os = "macos")]
fn prepare_ax_call(deadline: &AxObservationDeadline, element: &AXUIElement) -> Result<(), String> {
    let timeout_secs = deadline.remaining_timeout_secs()?;
    let error = unsafe { element.set_messaging_timeout(timeout_secs) };
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementSetMessagingTimeout", error));
    }
    deadline.ensure_remaining()
}

#[cfg(target_os = "macos")]
fn optional_ax_value(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CFRetained<CFType>>, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut raw: *const CFType = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_attribute_value(&attribute, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
                "accessibility_failed: AX attribute succeeded with null value".to_string()
            })?;
            Ok(Some(unsafe { CFRetained::from_raw(raw) }))
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(None),
        error => Err(accessibility_error("AXUIElementCopyAttributeValue", error)),
    }
}

#[cfg(target_os = "macos")]
fn optional_ax_string(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<String>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFString>()
        .ok()
        .map(|value| bounded_text(&value.to_string())))
}

#[cfg(target_os = "macos")]
fn element_fingerprint(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    inherited_protected: bool,
) -> Result<ElementFingerprint, String> {
    let role = optional_ax_string(deadline, element, "AXRole")?
        .ok_or_else(|| "accessibility_failed: AX element is missing a string role".to_string())?;
    let protected = inherited_protected
        || optional_ax_bool(deadline, element, "AXProtectedContent")?.unwrap_or(false);
    Ok(ElementFingerprint {
        role,
        subrole: optional_ax_string(deadline, element, "AXSubrole")?,
        identifier: optional_ax_string(deadline, element, "AXIdentifier")?,
        title: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXTitle")?
        },
        description: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXDescription")?
        },
        placeholder: if protected {
            None
        } else {
            optional_ax_string(deadline, element, "AXPlaceholderValue")?
        },
        protected,
    })
}

#[cfg(target_os = "macos")]
fn optional_ax_bool(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<bool>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    Ok(value
        .downcast::<CFBoolean>()
        .ok()
        .map(|value| value.value()))
}

#[cfg(target_os = "macos")]
fn optional_ax_point(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGPoint>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGPoint {
        return Ok(None);
    }
    let mut point = CGPoint::ZERO;
    let copied = unsafe { value.value(AXValueType::CGPoint, NonNull::from(&mut point).cast()) };
    Ok(copied.then_some(point))
}

#[cfg(target_os = "macos")]
fn optional_ax_size(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CGSize>, String> {
    let Some(value) = optional_ax_value(deadline, element, attribute)? else {
        return Ok(None);
    };
    let Ok(value) = value.downcast::<AXValue>() else {
        return Ok(None);
    };
    if unsafe { value.r#type() } != AXValueType::CGSize {
        return Ok(None);
    }
    let mut size = CGSize::ZERO;
    let copied = unsafe { value.value(AXValueType::CGSize, NonNull::from(&mut size).cast()) };
    Ok(copied.then_some(size))
}

#[cfg(target_os = "macos")]
fn ax_window_geometry_matches(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<bool, String> {
    const TOLERANCE: f64 = 2.0;
    let Some(position) = optional_ax_point(deadline, element, "AXPosition")? else {
        return Ok(false);
    };
    let Some(size) = optional_ax_size(deadline, element, "AXSize")? else {
        return Ok(false);
    };
    Ok((position.x - f64::from(x)).abs() <= TOLERANCE
        && (position.y - f64::from(y)).abs() <= TOLERANCE
        && (size.width - f64::from(width)).abs() <= TOLERANCE
        && (size.height - f64::from(height)).abs() <= TOLERANCE)
}

#[cfg(target_os = "macos")]
fn ax_array_count(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<usize, String> {
    let attribute = CFString::from_static_str(attribute);
    let mut count: CFIndex = 0;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.attribute_value_count(&attribute, NonNull::from(&mut count)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => {
            let count = usize::try_from(count).map_err(|_| {
                "accessibility_failed: AX array count is negative or too large".to_string()
            })?;
            if count > MAX_AX_CHILD_COUNT {
                return Err(
                    "accessibility_failed: AX child count exceeds bounded inspection limit"
                        .to_string(),
                );
            }
            Ok(count)
        }
        AXError::AttributeUnsupported | AXError::NoValue => Ok(0),
        error => Err(accessibility_error(
            "AXUIElementGetAttributeValueCount",
            error,
        )),
    }
}

#[cfg(target_os = "macos")]
fn ax_elements(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    count: usize,
) -> Result<Vec<CFRetained<AXUIElement>>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let attribute = CFString::from_static_str(attribute_name);
    let max_values = CFIndex::try_from(count)
        .map_err(|_| "accessibility_failed: AX array request exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe {
        element.copy_attribute_values(&attribute, 0, max_values, NonNull::from(&mut raw))
    };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX array copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    let mut output = Vec::with_capacity(array.len());
    for value in array.iter() {
        let element = value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AX element array contained a non-element value".to_string()
        })?;
        output.push(element);
    }
    Ok(output)
}

#[cfg(target_os = "macos")]
fn ax_supports_action(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    expected_action: &'static str,
) -> Result<bool, String> {
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.copy_action_names(NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyActionNames", error));
    }
    let raw = NonNull::new(raw.cast_mut()).ok_or_else(|| {
        "accessibility_failed: AX action-name copy succeeded with null value".to_string()
    })?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() > MAX_AX_ACTION_NAMES {
        return Err(
            "accessibility_failed: AX action-name list exceeds bounded inspection limit"
                .to_string(),
        );
    }
    let expected_utf16_len = expected_action.encode_utf16().count();
    for value in array.iter() {
        let action = value.downcast::<CFString>().map_err(|_| {
            "accessibility_failed: AX action-name array contained a non-string value".to_string()
        })?;
        let action_len = usize::try_from(action.length()).map_err(|_| {
            "accessibility_failed: AX action name has invalid string length".to_string()
        })?;
        if action_len == expected_utf16_len && action.to_string() == expected_action {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(target_os = "macos")]
fn ax_attribute_settable(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
) -> Result<bool, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let mut settable = 0u8;
    prepare_ax_call(deadline, element)?;
    let error = unsafe { element.is_attribute_settable(&attribute, NonNull::from(&mut settable)) };
    deadline.ensure_remaining()?;
    match error {
        AXError::Success => Ok(settable != 0),
        AXError::AttributeUnsupported => Ok(false),
        error => Err(accessibility_error("AXUIElementIsAttributeSettable", error)),
    }
}

#[cfg(target_os = "macos")]
fn ax_element_at(
    deadline: &AxObservationDeadline,
    element: &AXUIElement,
    attribute_name: &'static str,
    index: usize,
) -> Result<CFRetained<AXUIElement>, String> {
    let attribute = CFString::from_static_str(attribute_name);
    let index = CFIndex::try_from(index)
        .map_err(|_| "stale_element: AX child index exceeds CFIndex".to_string())?;
    let mut raw: *const CFArray = std::ptr::null();
    prepare_ax_call(deadline, element)?;
    let error =
        unsafe { element.copy_attribute_values(&attribute, index, 1, NonNull::from(&mut raw)) };
    deadline.ensure_remaining()?;
    if error != AXError::Success {
        return Err(accessibility_error("AXUIElementCopyAttributeValues", error));
    }
    let raw = NonNull::new(raw.cast_mut())
        .ok_or_else(|| "stale_element: AX child lookup returned null".to_string())?;
    let array: CFRetained<CFArray> = unsafe { CFRetained::from_raw(raw) };
    let array: &CFArray<CFType> = unsafe { array.cast_unchecked() };
    if array.len() != 1 {
        return Err("stale_element: AX child path no longer resolves exactly".to_string());
    }
    array
        .iter()
        .next()
        .expect("single AX child")
        .downcast::<AXUIElement>()
        .map_err(|_| "stale_element: AX child path resolved to a non-element value".to_string())
}

#[cfg(target_os = "macos")]
fn exact_ax_window(
    surface: &SurfaceRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    let native_window = resolve_surface_window(surface)?;
    deadline.ensure_remaining()?;
    let x = native_window.x().map_err(map_error)?;
    let y = native_window.y().map_err(map_error)?;
    let width = native_window.width().map_err(map_error)?;
    let height = native_window.height().map_err(map_error)?;
    let pid = checked_surface_pid(surface)?;
    let application = unsafe { AXUIElement::new_application(pid) };
    let window_count = ax_array_count(deadline, &application, "AXWindows")?;
    if window_count == 0 || window_count > MAX_AX_WINDOWS {
        return Err(
            "accessibility_failed: exact AX window cannot be resolved within the bounded window set"
                .to_string(),
        );
    }
    let mut windows = ax_elements(deadline, &application, "AXWindows", window_count)?;
    let mut geometry_matches = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        if ax_window_geometry_matches(deadline, window, x, y, width, height)? {
            geometry_matches.push(index);
        }
    }
    if !geometry_matches.is_empty() {
        let index = select_exact_ax_window_index(&geometry_matches, &[], windows.len())?;
        return Ok(windows.swap_remove(index));
    }

    let mut title_matches = Vec::new();
    if !surface.title.is_empty() {
        for (index, window) in windows.iter().enumerate() {
            if optional_ax_string(deadline, window, "AXTitle")?
                .is_some_and(|title| bounded_text(&title) == surface.title)
            {
                title_matches.push(index);
            }
        }
    }
    let index = select_exact_ax_window_index(&geometry_matches, &title_matches, windows.len())?;
    Ok(windows.swap_remove(index))
}

#[cfg(target_os = "macos")]
pub(crate) fn accessibility_status() -> Result<Value, String> {
    Ok(json!({
        "platform": "macos",
        "trusted": unsafe { AXIsProcessTrusted() },
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn accessibility_tree(
    surface_id: &str,
    surface: &SurfaceRecord,
    max_depth: usize,
    max_nodes: usize,
) -> Result<AccessibilityTreeResult, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let deadline = AxObservationDeadline::new();
    let root = exact_ax_window(surface, &deadline)?;
    let mut queue = VecDeque::from([(
        root,
        None::<String>,
        0usize,
        Vec::<usize>::new(),
        Vec::<ElementFingerprint>::new(),
        false,
    )]);
    let mut nodes = Vec::with_capacity(max_nodes.min(64));
    let mut elements = Vec::with_capacity(max_nodes.min(64));
    let mut truncated = false;
    while let Some((element, parent_element_id, depth, path, mut lineage, inherited_protected)) =
        queue.pop_front()
    {
        deadline.ensure_remaining()?;
        if nodes.len() >= max_nodes {
            truncated = true;
            break;
        }
        let element_id = format!("element_{}", Uuid::new_v4().simple());
        let fingerprint = element_fingerprint(&deadline, &element, inherited_protected)?;
        let role = fingerprint.role.clone();
        let subrole = fingerprint.subrole.clone();
        let title = fingerprint.title.clone();
        let description = fingerprint.description.clone();
        let placeholder = fingerprint.placeholder.clone();
        let protected = fingerprint.protected;
        lineage.push(fingerprint);
        let sensitive = role == "AXSecureTextField"
            || subrole
                .as_deref()
                .is_some_and(|value| value.contains("Secure"));
        let value = if sensitive || protected {
            None
        } else {
            optional_ax_string(&deadline, &element, "AXValue")?
        };
        let enabled = optional_ax_bool(&deadline, &element, "AXEnabled")?;
        let focused = optional_ax_bool(&deadline, &element, "AXFocused")?;
        let child_count = ax_array_count(&deadline, &element, "AXChildren")?;
        if depth < max_depth && child_count > 0 {
            let reserved = nodes.len() + queue.len() + 1;
            let remaining = max_nodes.saturating_sub(reserved);
            let take = child_count.min(remaining);
            if take < child_count {
                truncated = true;
            }
            for (index, child) in ax_elements(&deadline, &element, "AXChildren", take)?
                .into_iter()
                .enumerate()
            {
                let mut child_path = path.clone();
                child_path.push(index);
                queue.push_back((
                    child,
                    Some(element_id.clone()),
                    depth + 1,
                    child_path,
                    lineage.clone(),
                    protected,
                ));
            }
        } else if child_count > 0 {
            truncated = true;
        }
        elements.push((
            element_id.clone(),
            ElementRecord {
                surface_id: surface_id.to_string(),
                path,
                lineage,
            },
        ));
        nodes.push(json!({
            "element_id": element_id,
            "parent_element_id": parent_element_id,
            "depth": depth,
            "role": role,
            "subrole": subrole,
            "title": title,
            "description": description,
            "value": value,
            "placeholder": placeholder,
            "enabled": enabled,
            "focused": focused,
            "child_count": child_count,
        }));
    }
    if !queue.is_empty() {
        truncated = true;
    }
    deadline.ensure_remaining()?;
    let node_count = nodes.len();
    Ok(AccessibilityTreeResult {
        output: json!({
            "platform": "macos",
            "surface_id": surface_id,
            "nodes": nodes,
            "node_count": node_count,
            "truncated": truncated,
            "max_depth": max_depth,
            "max_nodes": max_nodes,
        }),
        elements,
    })
}

#[cfg(target_os = "macos")]
fn resolve_correlated_element(
    surface: &SurfaceRecord,
    element: &ElementRecord,
    deadline: &AxObservationDeadline,
) -> Result<CFRetained<AXUIElement>, String> {
    if element.lineage.len() != element.path.len() + 1 {
        return Err("stale_element: AX element correlation lineage is incomplete".to_string());
    }
    let mut current = exact_ax_window(surface, deadline)?;
    let current_root_fingerprint = element_fingerprint(deadline, &current, false)?;
    ensure_correlated_fingerprint(&element.lineage[0], &current_root_fingerprint, true)?;
    for (depth, &index) in element.path.iter().enumerate() {
        let child_count = ax_array_count(deadline, &current, "AXChildren")?;
        if index >= child_count {
            return Err("stale_element: AX child path no longer exists".to_string());
        }
        current = ax_element_at(deadline, &current, "AXChildren", index)?;
        let current_fingerprint =
            element_fingerprint(deadline, &current, element.lineage[depth].protected)?;
        ensure_correlated_fingerprint(&element.lineage[depth + 1], &current_fingerprint, false)?;
    }
    Ok(current)
}

#[cfg(target_os = "macos")]
pub(crate) fn element_state(
    surface_id: &str,
    element_id: &str,
    observation_generation: u32,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target = validate_element_state_target(element)?;
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    let protected = element.contains_protected_content()
        || element.lineage.iter().any(is_secure_text_fingerprint);
    let enabled_for_effect = enabled != Some(false);
    let can_press =
        !protected && enabled_for_effect && ax_supports_action(&deadline, &current, "AXPress")?;
    let can_focus = !protected
        && enabled_for_effect
        && ax_attribute_settable(&deadline, &current, "AXFocused")?;

    let supported_text = !protected && is_supported_text_input_fingerprint(target);
    let (value_empty, can_input_text) = if supported_text {
        let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
        let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
        let value_empty = current_value.as_deref().map(str::is_empty);
        let can_input_text = enabled != Some(false)
            && focused == Some(true)
            && value_settable
            && value_empty == Some(true);
        (value_empty, can_input_text)
    } else {
        (None, false)
    };
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "observation_generation": observation_generation,
        "enabled": enabled,
        "focused": focused,
        "protected": protected,
        "value_empty": value_empty,
        "can_press": can_press,
        "can_focus": can_focus,
        "can_input_text": can_input_text,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn activate_window(surface_id: &str, surface: &SurfaceRecord) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }

    // Re-resolve the native surface and exact AX window immediately before
    // any effect so an opaque stale surface cannot drift to another window.
    let deadline = AxObservationDeadline::new();
    let window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(surface.pid as _) };
    let frontmost = optional_ax_bool(&deadline, &application, "AXFrontmost")?;
    if frontmost != Some(true) && !ax_attribute_settable(&deadline, &application, "AXFrontmost")? {
        return Err(
            "control_failed: AX application does not allow AXFrontmost to be set".to_string(),
        );
    }
    if !ax_supports_action(&deadline, &window, "AXRaise")? {
        return Err("control_failed: exact AX window does not support AXRaise".to_string());
    }

    // Prepare both native call sites before the first mutation. After the
    // application becomes frontmost, any later failure is a partial effect.
    prepare_ax_call(&deadline, &application)?;
    prepare_ax_call(&deadline, &window)?;
    let mut application_activated = false;
    if frontmost != Some(true) {
        let error = unsafe {
            application.set_attribute_value(
                &CFString::from_static_str("AXFrontmost"),
                CFBoolean::new(true),
            )
        };
        if error != AXError::Success {
            return Err(window_activation_attempt_error(
                "AXUIElementSetAttributeValue(AXFrontmost)",
                error,
                false,
            ));
        }
        application_activated = true;
    }

    let error = unsafe { window.perform_action(&CFString::from_static_str("AXRaise")) };
    if error != AXError::Success {
        return Err(window_activation_attempt_error(
            "AXUIElementPerformAction(AXRaise)",
            error,
            application_activated,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn control(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    action: ComputerAction,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be controlled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for control".to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;

    match action {
        ComputerAction::Press if !ax_supports_action(&deadline, &current, "AXPress")? => {
            return Err(
                "control_failed: AX element does not support the AXPress action".to_string(),
            );
        }
        ComputerAction::Focus if !ax_attribute_settable(&deadline, &current, "AXFocused")? => {
            return Err(
                "control_failed: AX element does not allow AXFocused to be set".to_string(),
            );
        }
        _ => {}
    }

    prepare_ax_call(&deadline, &current)?;
    let error = match action {
        ComputerAction::Press => unsafe {
            current.perform_action(&CFString::from_static_str("AXPress"))
        },
        ComputerAction::Focus => unsafe {
            current.set_attribute_value(
                &CFString::from_static_str("AXFocused"),
                CFBoolean::new(true),
            )
        },
    };
    if error != AXError::Success {
        return Err(control_attempt_error(
            match action {
                ComputerAction::Press => "AXUIElementPerformAction(AXPress)",
                ComputerAction::Focus => "AXUIElementSetAttributeValue(AXFocused)",
            },
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "action": action.as_str(),
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn scroll_to_element(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let target_fingerprint = element
        .target_fingerprint()
        .ok_or_else(|| "stale_element: AX element correlation lineage is incomplete".to_string())?;
    if element.contains_protected_content() {
        return Err(
            "permission_denied: macOS Accessibility protected content cannot be scrolled"
                .to_string(),
        );
    }
    if !target_fingerprint.has_positive_evidence() {
        return Err(
            "stale_element: AX element lacks positive correlation evidence for scrolling"
                .to_string(),
        );
    }
    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    if !ax_supports_action(&deadline, &current, "AXScrollToVisible")? {
        return Err(
            "scroll_failed: AX element does not support the AXScrollToVisible action".to_string(),
        );
    }
    prepare_ax_call(&deadline, &current)?;
    let error = unsafe { current.perform_action(&CFString::from_static_str("AXScrollToVisible")) };
    if error != AXError::Success {
        return Err(scroll_attempt_error(
            "AXUIElementPerformAction(AXScrollToVisible)",
            error,
        ));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
fn validate_key_input_target(
    deadline: &AxObservationDeadline,
    application: &AXUIElement,
    exact_window: &CFRetained<AXUIElement>,
) -> Result<(), String> {
    if optional_ax_bool(deadline, application, "AXFrontmost")? != Some(true) {
        return Err(
            "key_input_failed: exact surface application must already be frontmost".to_string(),
        );
    }
    let focused_window = optional_ax_value(deadline, application, "AXFocusedWindow")?
        .ok_or_else(|| {
            "key_input_failed: exact surface application has no focused window".to_string()
        })?
        .downcast::<AXUIElement>()
        .map_err(|_| "accessibility_failed: AXFocusedWindow is not an AXUIElement".to_string())?;
    if &focused_window != exact_window {
        return Err(
            "key_input_failed: exact surface must already be the focused window".to_string(),
        );
    }

    if let Some(focused_value) = optional_ax_value(deadline, application, "AXFocusedUIElement")? {
        let focused_element = focused_value.downcast::<AXUIElement>().map_err(|_| {
            "accessibility_failed: AXFocusedUIElement is not an AXUIElement".to_string()
        })?;
        let fingerprint = element_fingerprint(deadline, &focused_element, false)?;
        if fingerprint.protected || is_secure_text_fingerprint(&fingerprint) {
            return Err(
                "permission_denied: protected or secure Accessibility content cannot receive key input"
                    .to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn key_input(
    surface_id: &str,
    surface: &SurfaceRecord,
    key: &str,
    modifiers: &[String],
) -> Result<Value, String> {
    validate_key_input(key, modifiers)?;
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    if !CGPreflightPostEventAccess() {
        return Err("permission_denied: macOS event-posting permission is not granted".to_string());
    }

    let pid = checked_surface_pid(surface)?;
    let deadline = AxObservationDeadline::new();
    let exact_window = exact_ax_window(surface, &deadline)?;
    let application = unsafe { AXUIElement::new_application(pid) };

    let key_code = key_code(key)?;
    let flags = key_modifier_flags(modifiers)?;
    let key_down = CGEvent::new_keyboard_event(None, key_code, true)
        .ok_or_else(|| "key_input_failed: could not create native key-down event".to_string())?;
    let key_up = CGEvent::new_keyboard_event(None, key_code, false)
        .ok_or_else(|| "key_input_failed: could not create native key-up event".to_string())?;
    CGEvent::set_flags(Some(&key_down), flags);
    CGEvent::set_flags(Some(&key_up), flags);

    // This is the final authority/privacy check before the first effect. Quartz
    // posts keyboard events to a process rather than a specific window, so keep
    // the exact focused-window check as close to dispatch as possible.
    validate_key_input_target(&deadline, &application, &exact_window)?;
    deadline.ensure_remaining()?;

    CGEvent::post_to_pid(pid, Some(&key_down));
    CGEvent::post_to_pid(pid, Some(&key_up));
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "key": key,
        "modifiers": modifiers,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn input_text(
    surface_id: &str,
    element_id: &str,
    surface: &SurfaceRecord,
    element: &ElementRecord,
    text: &str,
) -> Result<Value, String> {
    if !unsafe { AXIsProcessTrusted() } {
        return Err("permission_denied: macOS Accessibility permission is not granted".to_string());
    }
    let text_bytes = validate_input_text(text)?;
    validate_text_input_target(element)?;

    let deadline = AxObservationDeadline::new();
    let current = resolve_correlated_element(surface, element, &deadline)?;
    let enabled = optional_ax_bool(&deadline, &current, "AXEnabled")?;
    let value_settable = ax_attribute_settable(&deadline, &current, "AXValue")?;
    let focused = optional_ax_bool(&deadline, &current, "AXFocused")?;
    // This is deliberately the final read before the effect. The helper may
    // truncate a non-empty string for bounded observation, but emptiness is
    // preserved exactly and caller text is never transformed or normalized.
    let current_value = optional_ax_string(&deadline, &current, "AXValue")?;
    validate_text_input_preflight(enabled, focused, value_settable, current_value.as_deref())?;

    let text_value = CFString::from_str(text);
    prepare_ax_call(&deadline, &current)?;
    let error =
        unsafe { current.set_attribute_value(&CFString::from_static_str("AXValue"), &text_value) };
    if error != AXError::Success {
        return Err(text_input_attempt_error(error));
    }
    Ok(json!({
        "platform": "macos",
        "surface_id": surface_id,
        "element_id": element_id,
        "text_bytes": text_bytes,
        "success": true,
    }))
}

#[cfg(target_os = "macos")]
pub(crate) fn ensure_platform_capture_bound(
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
pub(crate) fn focus_state(window: &Window) -> (Option<bool>, Option<bool>) {
    // xcap 0.9.8 reports frontmost-application state on macOS, not exact
    // window focus. Preserve that reliable signal as `active` only.
    (None, window.is_focused().ok())
}
